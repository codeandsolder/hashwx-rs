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
    fn mul(v: u32) -> Self {
        match v % 3 {
            0 => Self::MulOr,
            1 => Self::MulXor,
            _ => Self::MulAdd,
        }
    }
    fn xas(v: u32) -> Self {
        match v % 9 {
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
    fn immediate(self, v: u64) -> u8 {
        match self {
            Self::MulOr | Self::MulXor | Self::MulAdd => [1, 9, 33][v as usize % 3],
            Self::Rmcg | Self::XorRor | Self::AddRor | Self::SubRor => 1 + (v % 63) as u8,
            _ => 1 + (v % 3) as u8,
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
fn sip(v: &mut [u64; 4]) {
    v[0] = v[0].wrapping_add(v[1]);
    v[2] = v[2].wrapping_add(v[3]);
    v[1] = v[1].rotate_left(13);
    v[3] = v[3].rotate_left(16);
    v[1] ^= v[0];
    v[3] ^= v[2];
    v[0] = v[0].rotate_left(32);
    v[2] = v[2].wrapping_add(v[1]);
    v[0] = v[0].wrapping_add(v[3]);
    v[1] = v[1].rotate_left(17);
    v[3] = v[3].rotate_left(21);
    v[1] ^= v[2];
    v[3] ^= v[0];
    v[2] = v[2].rotate_left(32)
}
impl SipRng {
    fn new(key: [u64; 2], salt: u64) -> Self {
        let mut s = [
            0x736f6d6570736575 ^ key[0],
            0x646f72616e646f6d ^ key[1],
            0x6c7967656e657261 ^ key[0],
            0x7465646279746573 ^ key[1],
        ];
        s[3] ^= salt;
        sip(&mut s);
        s[0] ^= salt;
        s[2] ^= 0xbb;
        for _ in 0..3 {
            sip(&mut s)
        }
        Self {
            key,
            state: s,
            count: 4,
        }
    }
    fn mix(&mut self) {
        self.state[0] ^= self.key[0];
        self.state[1] ^= self.key[1];
        self.state[2] ^= self.key[0];
        self.state[3] ^= self.key[1];
        for _ in 0..4 {
            sip(&mut self.state)
        }
    }
    fn next(&mut self) -> u64 {
        if self.count == 0 {
            self.mix();
            self.count = 4
        }
        self.count -= 1;
        self.state[self.count]
    }
}
fn key(seed: &[u8; 32], o: usize) -> [u64; 2] {
    [
        u64::from_le_bytes(seed[o..o + 8].try_into().unwrap()),
        u64::from_le_bytes(seed[o + 8..o + 16].try_into().unwrap()),
    ]
}
fn one(r: &mut SipRng, last: bool, deep: bool) -> Program {
    let mut g = [0u64; 8];
    for v in &mut g {
        *v = r.next()
    }
    let t = OPCODE_TEMPLATES[g[7] as usize % OPCODE_TEMPLATES.len()];
    let mut ops = [Op::Rmcg; 8];
    let mut sels = [0u32; 7];
    for i in 0..7 {
        let lo = g[i] as u32;
        sels[i] = (g[i] >> 32) as u32;
        ops[i + 1] = if (t >> i) & 1 != 0 {
            Op::mul(lo)
        } else {
            Op::xas(lo)
        }
    }
    let mut dst = [0u8; 8];
    for i in 0..8 {
        dst[i] = i as u8;
        if i != 0 {
            let j = sels[i - 1] as usize % (i + 1);
            dst[i] = dst[j];
            dst[j] = i as u8
        }
    }
    let perm = if deep {
        &SRC_DEEP[g[7] as usize % SRC_DEEP.len()]
    } else {
        &SRC_SHALLOW[g[7] as usize % SRC_SHALLOW.len()]
    };
    let mut src = [0u8; 8];
    src[0] = 8;
    for i in 1..8 {
        src[i] = dst[usize::from(perm[i])]
    }
    let mut body = [EMPTY_I; 8];
    for i in 0..8 {
        body[i] = Instruction {
            op: ops[i],
            dst: dst[i],
            src: src[i],
            imm: ops[i].immediate(r.next()),
        }
    }
    Program {
        body,
        unconditional: last,
    }
}
pub fn generate(seed: &[u8; 32]) -> ProgramSet {
    let pk = key(seed, 0);
    let hk = key(seed, 16);
    let mut r = SipRng::new(pk, u64::MAX);
    let mut p = [EMPTY_P; 32];
    for (i, x) in p.iter_mut().enumerate() {
        *x = one(&mut r, i == 31, i < 31 && i % 6 == 0)
    }
    ProgramSet {
        programs: p,
        hash_key: hk,
    }
}
