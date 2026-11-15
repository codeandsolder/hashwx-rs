//! Authoritative wire encoders for the two distinct FPGA instruction sets.
use hashwx::{HashXInstruction as Instruction, HashXProgram, Op, ProgramSet};

/// 128 little-endian instruction pairs, then k0 and k1 as 4 LE u32 words.
pub fn encode_hashwx(set: &ProgramSet) -> [u32; 132] {
    let mut words = [0u32; 132];
    for (i, inst) in set
        .programs()
        .iter()
        .flat_map(|p| p.body().iter())
        .enumerate()
    {
        let op = inst.op() as u16;
        assert!(op <= Op::SubLsr as u16);
        assert!(inst.dst() < 8 && inst.src() <= 8 && inst.imm() < 64);
        let src = if inst.op() == Op::Rmcg { 0 } else { inst.src() };
        let instruction = (op << 12)
            | (u16::from(inst.dst()) << 9)
            | (u16::from(src) << 6)
            | u16::from(inst.imm());
        words[i / 2] |= u32::from(instruction) << ((i % 2) * 16);
    }
    let [k0, k1] = set.hash_key();
    words[128] = k0 as u32;
    words[129] = (k0 >> 32) as u32;
    words[130] = k1 as u32;
    words[131] = (k1 >> 32) as u32;
    words
}

pub fn encode_hashx(program: &HashXProgram) -> [u32; 1032] {
    let mut words = [0u32; 1032];
    for (index, instruction) in program.instructions().iter().enumerate() {
        let (op, dst, src, shift, imm): (u64, u64, u64, u64, u32) = match instruction {
            Instruction::Mul { dst, src } => (0, dst.index().into(), src.index().into(), 0, 0),
            Instruction::UMulH { dst, src } => (1, dst.index().into(), src.index().into(), 0, 0),
            Instruction::SMulH { dst, src } => (2, dst.index().into(), src.index().into(), 0, 0),
            Instruction::AddShift {
                dst,
                src,
                left_shift,
            } => (
                3,
                dst.index().into(),
                src.index().into(),
                (*left_shift).into(),
                0,
            ),
            Instruction::AddConst { dst, src } => (4, dst.index().into(), 0, 0, *src as u32),
            Instruction::Sub { dst, src } => (5, dst.index().into(), src.index().into(), 0, 0),
            Instruction::Xor { dst, src } => (6, dst.index().into(), src.index().into(), 0, 0),
            Instruction::XorConst { dst, src } => (7, dst.index().into(), 0, 0, *src as u32),
            Instruction::Rotate { dst, right_rotate } => {
                (8, dst.index().into(), 0, (*right_rotate).into(), 0)
            }
            Instruction::Target => (9, 0, 0, 0, 0),
            Instruction::Branch { mask } => (10, 0, 0, 0, *mask),
        };
        let packed = (op << 60) | (dst << 57) | (src << 54) | (shift << 48) | u64::from(imm);
        words[index * 2] = packed as u32;
        words[index * 2 + 1] = (packed >> 32) as u32;
    }
    for (i, key) in program.register_key().iter().enumerate() {
        words[1024 + i * 2] = *key as u32;
        words[1025 + i * 2] = (*key >> 32) as u32;
    }
    words
}
