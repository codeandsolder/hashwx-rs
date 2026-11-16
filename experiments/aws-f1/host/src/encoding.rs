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

fn hashx_instruction_word(instruction: &Instruction) -> u64 {
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
    (op << 60) | (dst << 57) | (src << 54) | (shift << 48) | u64::from(imm)
}

/// A 512-slot microcode store followed by the four register-key u64s
/// and the packed instruction count. Consecutive independent MUL+ALU
/// pairs become a single 64-bit instruction (opcodes 11-13), so only
/// one instruction-memory read is required for two original operations.
///
/// Encoded fused instruction layout:
/// [63:60]=11+original_mul_opcode, [59:57]=mul_dst,
/// [56:54]=mul_src, [53:50]=alu_opcode, [49:47]=alu_dst,
/// [46:44]=alu_src, [43:38]=alu_shift, [37:6]=alu_imm,
/// [5:0]=0 (reserved).
/// Branch target markers are runtime-relative and require no relocation:
/// the hardware records the *new* packed index when executing TARGET.
pub fn encode_hashx(program: &HashXProgram) -> [u32; 1033] {
    let mut words = [0u32; 1033];
    let instructions: Vec<u64> = program
        .instructions()
        .iter()
        .map(hashx_instruction_word)
        .collect();
    let packed = pack_hashx_instructions(&instructions);
    for (index, word) in packed.iter().enumerate() {
        words[index * 2] = *word as u32;
        words[index * 2 + 1] = (*word >> 32) as u32;
    }
    for (i, key) in program.register_key().iter().enumerate() {
        words[1024 + i * 2] = *key as u32;
        words[1025 + i * 2] = (*key >> 32) as u32;
    }
    words[1032] = packed.len() as u32;
    words
}

/// Packs any nonempty sequence of up to 512 valid HashX instruction words.
/// Worst case (no mergeable pairs) is exactly 512 slots; the explicit
/// uploaded length means a terminator instruction is not required.
fn pack_hashx_instructions(instructions: &[u64]) -> Vec<u64> {
    assert!(!instructions.is_empty() && instructions.len() <= 512);
    let mut packed = Vec::with_capacity(instructions.len());
    let mut index = 0;
    while index < instructions.len() {
        let first = instructions[index];
        let mul_op = first >> 60;
        let mut word = first;
        let mut consumed = 1;
        if mul_op <= 2
            && let Some(next) = instructions.get(index + 1)
        {
            let second = *next;
            let alu_op = second >> 60;
            let mul_dst = (first >> 57) & 7;
            let alu_dst = (second >> 57) & 7;
            let alu_src = (second >> 54) & 7;
            let uses_src = matches!(alu_op, 3 | 5 | 6);
            if (3..=8).contains(&alu_op) && mul_dst != alu_dst && !(uses_src && alu_src == mul_dst)
            {
                word = ((11 + mul_op) << 60)
                    | (mul_dst << 57)
                    | (((first >> 54) & 7) << 54)
                    | (alu_op << 50)
                    | (alu_dst << 47)
                    | (alu_src << 44)
                    | (((second >> 48) & 63) << 38)
                    | ((second as u32 as u64) << 6);
                consumed = 2;
            }
        }
        packed.push(word);
        index += consumed;
    }
    packed
}

#[cfg(test)]
mod packing_tests {
    use super::pack_hashx_instructions;

    const fn inst(op: u64, dst: u64, src: u64, shift: u64, imm: u32) -> u64 {
        (op << 60) | (dst << 57) | (src << 54) | (shift << 48) | imm as u64
    }

    #[test]
    fn no_fusable_pairs_still_fit_512_slot_memory() {
        let source = vec![inst(0, 0, 1, 0, 0); 512];
        let packed = pack_hashx_instructions(&source);
        assert_eq!(packed.len(), 512);
        assert_eq!(packed, source);
    }

    #[test]
    fn pack_every_supported_alu_and_reject_dependencies() {
        // All six ALU opcodes, including full 32-bit signed immediates.
        for op in 3..=8 {
            let first = inst(2, 1, 4, 0, 0);
            let second = inst(op, 2, 1, 37, 0x8000_00dd);
            let packed = pack_hashx_instructions(&[first, second]);
            if matches!(op, 3 | 5 | 6) {
                assert_eq!(packed, vec![first, second]); // true RAW hazard
            } else {
                assert_eq!(packed.len(), 1);
                let fused = packed[0];
                assert_eq!(fused >> 60, 13);
                assert_eq!((fused >> 50) & 15, op);
                assert_eq!((fused >> 47) & 7, 2);
                assert_eq!((fused >> 38) & 63, 37);
                assert_eq!((fused >> 6) as u32, 0x8000_00dd);
                assert_eq!(fused & 63, 0);
            }
        }

        let first = inst(0, 1, 2, 0, 0);
        let write_conflict = inst(4, 1, 0, 0, 17);
        assert_eq!(
            pack_hashx_instructions(&[first, write_conflict]),
            vec![first, write_conflict]
        );
        let marker = inst(9, 0, 0, 0, 0);
        let branch = inst(10, 0, 0, 0, 0xabcd);
        let independent = inst(4, 3, 0, 0, 23);
        assert_eq!(
            pack_hashx_instructions(&[marker, first, independent, branch]),
            vec![
                marker,
                pack_hashx_instructions(&[first, independent])[0],
                branch
            ]
        );
    }
}
