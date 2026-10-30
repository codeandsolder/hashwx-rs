// HashX v1 compatibility model derived from Tor Project hashx 0.10.0 (LGPL-3.0-only).
// Adapted for hashwx-rs; execution backends are implemented separately in this repository.
//! Define the internal hash program representation used by `HashX`.

use crate::hashx::HashXError;
use crate::hashx::generator::Generator;
use crate::hashx::register::{RegisterFile, RegisterId};
use rand_core::Rng;
use std::fmt;
use std::ops::BitXor;

/// Maximum number of instructions in the program
///
/// Programs with fewer instructions may be generated (for example, after
/// a timing stall when register allocation fails) but they will not pass
/// whole-program constraint tests.
pub const NUM_INSTRUCTIONS: usize = 512;

/// Type alias for a full-size array of [`Instruction`]s
pub type InstructionArray = [Instruction; NUM_INSTRUCTIONS];

/// Growable instruction buffer used while constructing a fixed-size program.
pub type InstructionVec = Vec<Instruction>;

/// Define the `HashX` virtual instruction set
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Instruction {
    /// 64-bit multiply of two registers, discarding overflow.
    Mul {
        /// Destination register
        dst: RegisterId,
        /// Source register
        src: RegisterId,
    },

    /// Unsigned 64x64 to 128-bit multiply, saving only the upper half.
    ///
    /// Result is written to dst, and the low 32 bits are saved for the
    /// next Branch test.
    UMulH {
        /// Destination register
        dst: RegisterId,
        /// Source register
        src: RegisterId,
    },

    /// Signed 64x64 to 128-bit multiply, saving only the upper half.
    ///
    /// Result is written to dst, and the low 32 bits are saved for the
    /// next Branch test.
    SMulH {
        /// Destination register
        dst: RegisterId,
        /// Source register
        src: RegisterId,
    },

    /// Shift source register left by a constant amount, add, discard overflow.
    AddShift {
        /// Destination register
        dst: RegisterId,
        /// Source register
        src: RegisterId,
        /// Number of bits to left shift by (0..=3 only)
        left_shift: u8,
    },

    /// 64-bit addition by a sign-extended 32-bit constant.
    AddConst {
        /// Destination register
        dst: RegisterId,
        /// Source immediate, sign-extended from 32-bit to 64-bit
        src: i32,
    },

    /// 64-bit subtraction (dst - src), discarding overflow.
    Sub {
        /// Destination register
        dst: RegisterId,
        /// Source register
        src: RegisterId,
    },

    /// 64-bit XOR of two registers.
    Xor {
        /// Destination register
        dst: RegisterId,
        /// Source register
        src: RegisterId,
    },

    /// XOR a 64-bit register with a sign-extended 32-bit constant.
    XorConst {
        /// Destination register
        dst: RegisterId,
        /// Source immediate, sign-extended from 32-bit to 64-bit
        src: i32,
    },

    /// Rotate a 64-bit register right by a constant amount.
    Rotate {
        /// Destination register
        dst: RegisterId,
        /// Number of bits to rotate right by (0..=63 only)
        right_rotate: u8,
    },

    /// Become the target for the next taken branch, if any.
    Target,

    /// One-shot conditional branch to the last Target.
    Branch {
        /// 32-bit branch condition mask
        ///
        /// This is tested against the last `UMulH`/`SMulH` result. (The low 32
        /// bits of the instruction result, which itself is the upper 64 bits
        /// of the multiplication result.)
        ///
        /// If `(result & mask)` is zero and no branches have been previously
        /// taken, we jump back to the Target and remember not to take any
        /// future branches. A well formed program will always have a `Target`
        /// prior to any `Branch`.
        mask: u32,
    },
}

/// An instruction operation, without any of its arguments
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Opcode {
    /// Opcode for [`Instruction::Mul`]
    Mul,
    /// Opcode for [`Instruction::UMulH`]
    UMulH,
    /// Opcode for [`Instruction::SMulH`]
    SMulH,
    /// Opcode for [`Instruction::AddShift`]
    AddShift,
    /// Opcode for [`Instruction::AddConst`]
    AddConst,
    /// Opcode for [`Instruction::Sub`]
    Sub,
    /// Opcode for [`Instruction::Xor`]
    Xor,
    /// Opcode for [`Instruction::XorConst`]
    XorConst,
    /// Opcode for [`Instruction::Rotate`]
    Rotate,
    /// Opcode for [`Instruction::Target`]
    Target,
    /// Opcode for [`Instruction::Branch`]
    Branch,
}

impl Instruction {
    /// Get this instruction's [`Opcode`].
    #[inline]
    pub(crate) const fn opcode(&self) -> Opcode {
        match self {
            Self::AddConst { .. } => Opcode::AddConst,
            Self::AddShift { .. } => Opcode::AddShift,
            Self::Branch { .. } => Opcode::Branch,
            Self::Mul { .. } => Opcode::Mul,
            Self::Rotate { .. } => Opcode::Rotate,
            Self::SMulH { .. } => Opcode::SMulH,
            Self::Sub { .. } => Opcode::Sub,
            Self::Target => Opcode::Target,
            Self::UMulH { .. } => Opcode::UMulH,
            Self::Xor { .. } => Opcode::Xor,
            Self::XorConst { .. } => Opcode::XorConst,
        }
    }

    /// Get this instruction's destination register, if any.
    #[inline]
    pub(crate) const fn destination(&self) -> Option<RegisterId> {
        match self {
            Self::AddConst { dst, .. }
            | Self::AddShift { dst, .. }
            | Self::Mul { dst, .. }
            | Self::Rotate { dst, .. }
            | Self::SMulH { dst, .. }
            | Self::Sub { dst, .. }
            | Self::UMulH { dst, .. }
            | Self::Xor { dst, .. }
            | Self::XorConst { dst, .. } => Some(*dst),
            Self::Branch { .. } | Self::Target => None,
        }
    }
}

/// Generated `HashX` program, as a boxed array of instructions
#[derive(Clone)]
pub struct Program(Box<InstructionArray>);

impl fmt::Debug for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Program {{")?;
        for (addr, inst) in self.0.iter().enumerate() {
            writeln!(f, " [{addr:3}]: {inst:?}")?;
        }
        write!(f, "}}")
    }
}

impl Program {
    /// Generate a new `Program` from an arbitrary [`Rng`] implementer
    ///
    /// This can return [`HashXError::ProgramConstraints`] if the `HashX`
    /// post-generation program verification fails. During normal use this
    /// will happen once per several thousand random seeds, and the caller
    /// should skip to another seed.
    pub(crate) fn generate<T: Rng>(rng: &mut T) -> Result<Self, HashXError> {
        let mut instructions = Vec::with_capacity(NUM_INSTRUCTIONS);
        Generator::new(rng).generate_program(&mut instructions)?;
        let boxed: Box<[Instruction]> = instructions.into_boxed_slice();
        let program: Box<InstructionArray> = boxed
            .try_into()
            .map_err(|_| HashXError::ProgramConstraints)?;
        Ok(Self(program))
    }

    /// Reference implementation for `Program` behavior
    ///
    /// Run the program from start to finish, with up to one branch,
    /// in the provided register file.
    pub(crate) fn interpret(&self, regs: &mut RegisterFile) {
        let mut program_counter = 0;
        let mut allow_branch = true;
        let mut branch_target = 0_usize;
        let mut mulh_result: u32 = 0;

        /// Common implementation for binary operations on registers
        macro_rules! binary_reg_op {
            ($dst:ident, $src:ident, $fn:ident, $pc:ident) => {{
                let a = regs.load(*$dst);
                let b = regs.load(*$src);
                regs.store(*$dst, a.$fn(b));
                $pc
            }};
        }

        /// Common implementation for binary operations with a const operand
        macro_rules! binary_const_op {
            ($dst:ident, $src:ident, $fn:ident, $pc:ident) => {{
                let a = regs.load(*$dst);
                let b_sign_extended = u64::from_ne_bytes(i64::from(*$src).to_ne_bytes());
                regs.store(*$dst, a.$fn(b_sign_extended));
                $pc
            }};
        }

        while program_counter < self.0.len() {
            let next_pc = program_counter + 1;
            program_counter = match &self.0[program_counter] {
                Instruction::Target => {
                    branch_target = program_counter;
                    next_pc
                }

                Instruction::Branch { mask } => {
                    if allow_branch && (mask & mulh_result) == 0 {
                        allow_branch = false;
                        branch_target
                    } else {
                        next_pc
                    }
                }

                Instruction::AddShift {
                    dst,
                    src,
                    left_shift,
                } => {
                    let a = regs.load(*dst);
                    let b = regs.load(*src);
                    let r = a.wrapping_add(b.wrapping_shl((*left_shift).into()));
                    regs.store(*dst, r);
                    next_pc
                }

                Instruction::Rotate { dst, right_rotate } => {
                    let a = regs.load(*dst);
                    let r = a.rotate_right((*right_rotate).into());
                    regs.store(*dst, r);
                    next_pc
                }

                Instruction::Mul { dst, src } => binary_reg_op!(dst, src, wrapping_mul, next_pc),
                Instruction::Sub { dst, src } => binary_reg_op!(dst, src, wrapping_sub, next_pc),
                Instruction::Xor { dst, src } => binary_reg_op!(dst, src, bitxor, next_pc),
                Instruction::UMulH { dst, src } => {
                    let product =
                        u128::from(regs.load(*dst)).wrapping_mul(u128::from(regs.load(*src)));
                    let bytes = product.to_le_bytes();
                    let result = u64::from_le_bytes([
                        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                        bytes[15],
                    ]);
                    let low = result.to_le_bytes();
                    mulh_result = u32::from_le_bytes([low[0], low[1], low[2], low[3]]);
                    regs.store(*dst, result);
                    next_pc
                }
                Instruction::SMulH { dst, src } => {
                    let left = i64::from_ne_bytes(regs.load(*dst).to_ne_bytes());
                    let right = i64::from_ne_bytes(regs.load(*src).to_ne_bytes());
                    let bytes = i128::from(left)
                        .wrapping_mul(i128::from(right))
                        .to_le_bytes();
                    let result = u64::from_le_bytes([
                        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                        bytes[15],
                    ]);
                    let low = result.to_le_bytes();
                    mulh_result = u32::from_le_bytes([low[0], low[1], low[2], low[3]]);
                    regs.store(*dst, result);
                    next_pc
                }
                Instruction::XorConst { dst, src } => binary_const_op!(dst, src, bitxor, next_pc),
                Instruction::AddConst { dst, src } => {
                    binary_const_op!(dst, src, wrapping_add, next_pc)
                }
            }
        }
    }
}

impl<'a> From<&'a Program> for &'a InstructionArray {
    #[inline]
    fn from(prog: &'a Program) -> Self {
        &prog.0
    }
}
