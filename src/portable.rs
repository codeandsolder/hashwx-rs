//! Architecture-neutral `HashWX` execution.

use crate::program::{Instruction, Op, Program, ProgramSet};

const SCRATCH_MASK: u16 = 16_383;
const SCRATCH_BYTES: usize = 16_392;
const SNAPSHOT_BYTES: usize = 64;

#[inline]
const fn sip(v: &mut [u64; 4]) {
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
    v[2] = v[2].rotate_left(32);
}

fn initialize([k0, k1]: [u64; 2], nonce: u64) -> [u64; 9] {
    let mut v = [
        0x736f_6d65_7073_6575 ^ k0,
        0x646f_7261_6e64_6f6d ^ k1,
        0x6c79_6765_6e65_7261 ^ k0,
        0x7465_6462_7974_6573 ^ k1,
    ];
    v[3] ^= nonce;
    sip(&mut v);
    v[0] ^= nonce;
    v[2] ^= 0xbb;
    for _ in 0..3 {
        sip(&mut v);
    }

    let mut r = [0_u64; 9];
    [r[0], r[1], r[2], r[3]] = [v[3], v[2], v[1], v[0]];
    v[0] ^= k0;
    v[1] ^= k1;
    v[2] ^= k0;
    v[3] ^= k1;
    for _ in 0..4 {
        sip(&mut v);
    }
    [r[4], r[5], r[6], r[7]] = [v[3], v[2], v[1], v[0]];
    r[8] = ((r[0] ^ r[4]) & !7) | 3;
    r
}

const fn finalize(r: &[u64; 9]) -> u64 {
    let mut lo = [r[0], r[1], r[2], r[3]];
    let mut hi = [r[4], r[5], r[6], r[7]];
    sip(&mut lo);
    sip(&mut hi);
    lo[3] ^ hi[3] ^ r[8]
}

fn store_word(memory: &mut [u8; SCRATCH_BYTES], offset: usize, value: u64) {
    memory[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

const fn load_word(memory: &[u8; SCRATCH_BYTES], offset: usize) -> u64 {
    u64::from_le_bytes([
        memory[offset],
        memory[offset + 1],
        memory[offset + 2],
        memory[offset + 3],
        memory[offset + 4],
        memory[offset + 5],
        memory[offset + 6],
        memory[offset + 7],
    ])
}

fn snapshot(memory: &mut [u8; SCRATCH_BYTES], cursor: &mut usize, registers: &[u64; 9]) {
    *cursor -= SNAPSHOT_BYTES;
    for (register, offset) in registers[..8]
        .iter()
        .zip([56_usize, 48, 40, 32, 24, 16, 8, 0])
    {
        store_word(memory, *cursor + offset, *register);
    }
}

fn source(registers: &[u64; 9], memory: &[u8; SCRATCH_BYTES], src: u8, memory_phase: bool) -> u64 {
    if memory_phase {
        let address = registers[usize::from(src)].to_le_bytes();
        let offset = usize::from(u16::from_le_bytes([address[0], address[1]]) & SCRATCH_MASK);
        load_word(memory, offset)
    } else {
        registers[usize::from(src)]
    }
}

const fn arithmetic_shift_right(value: u64, shift: u32) -> u64 {
    let signed = i64::from_ne_bytes(value.to_ne_bytes()) >> shift;
    u64::from_ne_bytes(signed.to_ne_bytes())
}

fn execute_instruction(
    registers: &mut [u64; 9],
    memory: &[u8; SCRATCH_BYTES],
    instruction: Instruction,
    memory_phase: bool,
) {
    let dst = usize::from(instruction.dst);
    let shift = u32::from(instruction.imm);
    match instruction.op {
        Op::MulOr => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = (registers[dst] | u64::from(instruction.imm)).wrapping_mul(src);
        }
        Op::MulXor => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = (registers[dst] ^ u64::from(instruction.imm)).wrapping_mul(src);
        }
        Op::MulAdd => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = registers[dst]
                .wrapping_add(u64::from(instruction.imm))
                .wrapping_mul(src);
        }
        Op::Rmcg => {
            registers[dst] = registers[dst]
                .wrapping_mul(registers[8])
                .rotate_right(shift);
        }
        Op::XorRor => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = registers[dst].rotate_right(shift) ^ src;
        }
        Op::AddRor => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = registers[dst].rotate_right(shift).wrapping_add(src);
        }
        Op::SubRor => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = registers[dst].rotate_right(shift).wrapping_sub(src);
        }
        Op::XorAsr => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = arithmetic_shift_right(registers[dst], shift) ^ src;
        }
        Op::AddAsr => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = arithmetic_shift_right(registers[dst], shift).wrapping_add(src);
        }
        Op::SubAsr => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = arithmetic_shift_right(registers[dst], shift).wrapping_sub(src);
        }
        Op::XorLsr => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = (registers[dst] >> shift) ^ src;
        }
        Op::AddLsr => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = (registers[dst] >> shift).wrapping_add(src);
        }
        Op::SubLsr => {
            let src = source(registers, memory, instruction.src, memory_phase);
            registers[dst] = (registers[dst] >> shift).wrapping_sub(src);
        }
    }
}

fn phase(
    programs: &[Program; 32],
    registers: &mut [u64; 9],
    memory: &mut [u8; SCRATCH_BYTES],
    cursor: &mut usize,
    memory_phase: bool,
) {
    for _ in 0..4 {
        let mut branch_budget = 32_u32;
        for program in programs {
            loop {
                if !memory_phase {
                    snapshot(memory, cursor, registers);
                }
                for &instruction in &program.body {
                    execute_instruction(registers, memory, instruction, memory_phase);
                }
                let branch_flag = registers[usize::from(program.body[0].dst)];
                if branch_budget == 0 || (!program.unconditional && branch_flag & 32 != 0) {
                    break;
                }
                branch_budget -= 1;
            }
        }
    }
}

#[expect(
    clippy::large_stack_arrays,
    reason = "per-call HashWX scratch is intentionally stack-local so immutable HashWx values remain Sync and concurrent hashes need neither allocation nor locking"
)]
pub fn hash(set: &ProgramSet, nonce: u64) -> u64 {
    let mut registers = initialize(set.hash_key, nonce);
    let mut memory = [0_u8; SCRATCH_BYTES];
    store_word(&mut memory, 16_384, registers[8]);
    let mut cursor = 16_384;
    phase(
        &set.programs,
        &mut registers,
        &mut memory,
        &mut cursor,
        false,
    );
    debug_assert_eq!(
        cursor, 0,
        "HashWX register phase must produce 16 KiB of snapshots"
    );
    phase(
        &set.programs,
        &mut registers,
        &mut memory,
        &mut cursor,
        true,
    );
    finalize(&registers)
}
