use crate::tables::{OPCODE_TEMPLATES, SRC_DEEP, SRC_SHALLOW};
pub const SEED_SIZE: usize = 32;
pub const PROGRAM_COUNT: usize = 32;
pub const PROGRAM_SIZE: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    MulOr = 0,
    MulXor = 1,
    MulAdd = 2,
    Rmcg = 3,
    XorRor = 4,
    AddRor = 5,
    SubRor = 6,
    XorAsr = 7,
    AddAsr = 8,
    SubAsr = 9,
    XorLsr = 10,
    AddLsr = 11,
    SubLsr = 12,
}
impl Op {
    const fn mul(value: u32) -> Self {
        match value % 3 {
            0 => Self::MulOr,
            1 => Self::MulXor,
            _ => Self::MulAdd,
        }
    }
    const fn xas(value: u32) -> Self {
        match value % 9 {
            0 => Self::XorRor,
            1 => Self::AddRor,
            2 => Self::SubRor,
            3 => Self::XorAsr,
            4 => Self::AddAsr,
            5 => Self::SubAsr,
            6 => Self::XorLsr,
            7 => Self::AddLsr,
            _ => Self::SubLsr,
        }
    }
    const fn immediate(self, value: u64) -> u8 {
        match self {
            Self::MulOr | Self::MulXor | Self::MulAdd => {
                let index = match value % 3 {
                    0 => 0,
                    1 => 1,
                    _ => 2,
                };
                [1, 9, 33][index]
            }
            Self::Rmcg | Self::XorRor | Self::AddRor | Self::SubRor => {
                1 + (value % 63).to_le_bytes()[0]
            }
            _ => 1 + (value % 3).to_le_bytes()[0],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub op: Op,
    pub dst: u8,
    pub src: u8,
    pub imm: u8,
}
const EMPTY_I: Instruction = Instruction {
    op: Op::Rmcg,
    dst: 0,
    src: 8,
    imm: 1,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Program {
    pub body: [Instruction; 8],
    pub unconditional: bool,
}
const EMPTY_P: Program = Program {
    body: [EMPTY_I; 8],
    unconditional: false,
};
#[derive(Clone, Debug)]
pub struct ProgramSet {
    pub programs: [Program; 32],
    pub hash_key: [u64; 2],
}
#[derive(Clone, Copy)]
struct SipRng {
    key: [u64; 2],
    state: [u64; 4],
    count: usize,
}
fn small_index(value: u64, modulus: u8) -> usize {
    usize::from((value % u64::from(modulus)).to_le_bytes()[0])
}
const fn low_u32(value: u64) -> u32 {
    let bytes = value.to_le_bytes();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
const fn high_u32(value: u64) -> u32 {
    let bytes = value.to_le_bytes();
    u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
}
const fn sip(state: &mut [u64; 4]) {
    state[0] = state[0].wrapping_add(state[1]);
    state[2] = state[2].wrapping_add(state[3]);
    state[1] = state[1].rotate_left(13);
    state[3] = state[3].rotate_left(16);
    state[1] ^= state[0];
    state[3] ^= state[2];
    state[0] = state[0].rotate_left(32);
    state[2] = state[2].wrapping_add(state[1]);
    state[0] = state[0].wrapping_add(state[3]);
    state[1] = state[1].rotate_left(17);
    state[3] = state[3].rotate_left(21);
    state[1] ^= state[2];
    state[3] ^= state[0];
    state[2] = state[2].rotate_left(32);
}
impl SipRng {
    fn new(key: [u64; 2], salt: u64) -> Self {
        let mut state = [
            0x736f_6d65_7073_6575 ^ key[0],
            0x646f_7261_6e64_6f6d ^ key[1],
            0x6c79_6765_6e65_7261 ^ key[0],
            0x7465_6462_7974_6573 ^ key[1],
        ];
        state[3] ^= salt;
        sip(&mut state);
        state[0] ^= salt;
        state[2] ^= 0xbb;
        for _ in 0..3 {
            sip(&mut state);
        }
        Self {
            key,
            state,
            count: 4,
        }
    }
    fn mix(&mut self) {
        self.state[0] ^= self.key[0];
        self.state[1] ^= self.key[1];
        self.state[2] ^= self.key[0];
        self.state[3] ^= self.key[1];
        for _ in 0..4 {
            sip(&mut self.state);
        }
    }
    fn next(&mut self) -> u64 {
        if self.count == 0 {
            self.mix();
            self.count = 4;
        }
        self.count -= 1;
        self.state[self.count]
    }
}
const fn read_u64(seed: &[u8; 32], offset: usize) -> u64 {
    u64::from_le_bytes([
        seed[offset],
        seed[offset + 1],
        seed[offset + 2],
        seed[offset + 3],
        seed[offset + 4],
        seed[offset + 5],
        seed[offset + 6],
        seed[offset + 7],
    ])
}
const fn key(seed: &[u8; 32], offset: usize) -> [u64; 2] {
    [read_u64(seed, offset), read_u64(seed, offset + 8)]
}
fn one(rng: &mut SipRng, last: bool, deep: bool) -> Program {
    let mut random = [0u64; 8];
    for value in &mut random {
        *value = rng.next();
    }
    let template = OPCODE_TEMPLATES[small_index(random[7], 35)];
    let mut ops = [Op::Rmcg; 8];
    let mut selectors = [0u32; 7];
    for index in 0..7 {
        let low = low_u32(random[index]);
        selectors[index] = high_u32(random[index]);
        ops[index + 1] = if (template >> index) & 1 != 0 {
            Op::mul(low)
        } else {
            Op::xas(low)
        };
    }
    let mut destinations = [0u8; 8];
    for index_u8 in 0u8..8 {
        let index = usize::from(index_u8);
        destinations[index] = index_u8;
        if index != 0 {
            let width = u32::from(index_u8) + 1;
            let selected = (selectors[index - 1] % width).to_le_bytes()[0];
            let selected = usize::from(selected);
            destinations[index] = destinations[selected];
            destinations[selected] = index_u8;
        }
    }
    let permutation = if deep {
        &SRC_DEEP[small_index(random[7], 24)]
    } else {
        &SRC_SHALLOW[usize::from(random[7].to_le_bytes()[0])]
    };
    let mut sources = [0u8; 8];
    sources[0] = 8;
    for index in 1..8 {
        sources[index] = destinations[usize::from(permutation[index])];
    }
    let mut body = [EMPTY_I; 8];
    for index in 0..8 {
        body[index] = Instruction {
            op: ops[index],
            dst: destinations[index],
            src: sources[index],
            imm: ops[index].immediate(rng.next()),
        };
    }
    Program {
        body,
        unconditional: last,
    }
}
#[must_use]
pub fn generate(seed: &[u8; 32]) -> ProgramSet {
    let program_key = key(seed, 0);
    let hash_key = key(seed, 16);
    let mut rng = SipRng::new(program_key, u64::MAX);
    let mut programs = [EMPTY_P; 32];
    for (index, program) in programs.iter_mut().enumerate() {
        *program = one(&mut rng, index == 31, index < 31 && index % 6 == 0);
    }
    ProgramSet { programs, hash_key }
}
