//! x86-64 native emitter for the repo-owned `HashX` IR.

#![expect(
    clippy::useless_conversion,
    reason = "dynasm dynamic-register expansion uses .into() internally"
)]
#![expect(
    clippy::used_underscore_binding,
    reason = "dynasm dynamic-register expansion synthesizes underscore-prefixed temporaries"
)]

use crate::CompileError;
use crate::hashx::{Instruction, NativeRegisterFile, Program, RegisterId};
use dynasmrt::{AssemblyOffset, DynasmApi, DynasmLabelApi, ExecutableBuffer, dynasm};

#[cfg(windows)]
type HashFn = unsafe extern "win64" fn(*mut NativeRegisterFile);
#[cfg(not(windows))]
type HashFn = unsafe extern "sysv64" fn(*mut NativeRegisterFile);

pub struct Compiled {
    buffer: ExecutableBuffer,
    function: HashFn,
}

impl core::fmt::Debug for Compiled {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CompiledHashX")
            .field("code_len", &self.buffer.len())
            .finish_non_exhaustive()
    }
}

impl Compiled {
    pub(crate) fn compile(program: &Program) -> Result<Self, CompileError> {
        let mut assembler =
            dynasmrt::x64::Assembler::new().map_err(|error| CompileError(error.to_string()))?;
        let arg = if cfg!(windows) { 1_u8 } else { 7_u8 };

        // rbx owns the RegisterFile pointer. HashX virtual R0..R7 map to r8..r15.
        dynasm!(assembler
            ; .arch x64
            ; push rbx
            ; push r12
            ; push r13
            ; push r14
            ; push r15
        );
        #[cfg(windows)]
        dynasm!(assembler; .arch x64; push rsi);
        dynasm!(assembler; .arch x64; mov rbx, Rq(arg));
        for register in RegisterId::all() {
            let hw = hw_register(register);
            let offset = register_offset(register);
            dynasm!(assembler; .arch x64; mov Rq(hw), [rbx + offset]);
        }

        // edx holds the low 32 bits of the last wide-multiply high result.
        // esi is zero until one branch is taken, then all ones forever.
        dynasm!(assembler
            ; .arch x64
            ; xor edx, edx
            ; xor esi, esi
            ; mov ecx, -1
        );

        let instructions: &[Instruction; 512] = program.into();
        let mut target = None;
        for instruction in instructions {
            emit_instruction(&mut assembler, instruction, &mut target)?;
        }

        for register in RegisterId::all() {
            let hw = hw_register(register);
            let offset = register_offset(register);
            dynasm!(assembler; .arch x64; mov [rbx + offset], Rq(hw));
        }
        #[cfg(windows)]
        dynasm!(assembler; .arch x64; pop rsi);
        dynasm!(assembler
            ; .arch x64
            ; pop r15
            ; pop r14
            ; pop r13
            ; pop r12
            ; pop rbx
            ; ret
        );

        let buffer = assembler
            .finalize()
            .map_err(|_| CompileError("failed to finalize x86-64 HashX code".into()))?;
        // SAFETY: offset zero is the emitted function entry with the exact HashFn ABI,
        // and `buffer` is retained for the lifetime of `function`.
        let function: HashFn =
            unsafe { core::mem::transmute::<*const u8, HashFn>(buffer.ptr(AssemblyOffset(0))) };
        Ok(Self { buffer, function })
    }

    #[inline]
    pub(crate) fn invoke(&self, registers: &mut NativeRegisterFile) {
        // SAFETY: `compile` creates this pointer with the HashFn ABI and the executable
        // allocation is owned by `self` for the duration of the call.
        unsafe { (self.function)(registers) };
    }
}

#[inline]
const fn hw_register(register: RegisterId) -> u8 {
    8 + register.as_u8()
}

#[inline]
fn register_offset(register: RegisterId) -> i32 {
    i32::from(register.as_u8()) * 8
}

fn emit_instruction(
    assembler: &mut dynasmrt::x64::Assembler,
    instruction: &Instruction,
    target: &mut Option<dynasmrt::DynamicLabel>,
) -> Result<(), CompileError> {
    match instruction {
        Instruction::Target => {
            let label = assembler.new_dynamic_label();
            *target = Some(label);
            dynasm!(assembler; .arch x64; =>label);
        }
        Instruction::Branch { mask } => {
            let label = (*target)
                .ok_or_else(|| CompileError("HashX branch emitted without a target".into()))?;
            let mask = i32::from_ne_bytes(mask.to_ne_bytes());
            dynasm!(assembler
                ; .arch x64
                ; or edx, esi
                ; test edx, mask
                ; cmovz esi, ecx
                ; jz =>label
            );
        }
        Instruction::Mul { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler; .arch x64; imul Rq(dst), Rq(src));
        }
        Instruction::UMulH { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler
                ; .arch x64
                ; mov rax, Rq(dst)
                ; mul Rq(src)
                ; mov Rq(dst), rdx
            );
        }
        Instruction::SMulH { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler
                ; .arch x64
                ; mov rax, Rq(dst)
                ; imul Rq(src)
                ; mov Rq(dst), rdx
            );
        }
        Instruction::AddShift {
            dst,
            src,
            left_shift,
        } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            match left_shift {
                0 => dynasm!(assembler; .arch x64; lea Rq(dst), [Rq(dst) + Rq(src)]),
                1 => dynasm!(assembler; .arch x64; lea Rq(dst), [Rq(dst) + 2 * Rq(src)]),
                2 => dynasm!(assembler; .arch x64; lea Rq(dst), [Rq(dst) + 4 * Rq(src)]),
                3 => dynasm!(assembler; .arch x64; lea Rq(dst), [Rq(dst) + 8 * Rq(src)]),
                _ => {
                    return Err(CompileError(
                        "HashX AddShift immediate outside 0..=3".into(),
                    ));
                }
            }
        }
        Instruction::AddConst { dst, src } => {
            let dst = hw_register(*dst);
            dynasm!(assembler; .arch x64; add Rq(dst), *src);
        }
        Instruction::Sub { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler; .arch x64; sub Rq(dst), Rq(src));
        }
        Instruction::Xor { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler; .arch x64; xor Rq(dst), Rq(src));
        }
        Instruction::XorConst { dst, src } => {
            let dst = hw_register(*dst);
            dynasm!(assembler; .arch x64; xor Rq(dst), *src);
        }
        Instruction::Rotate { dst, right_rotate } => {
            let dst = hw_register(*dst);
            let rotate = i8::from_ne_bytes([*right_rotate]);
            dynasm!(assembler; .arch x64; ror Rq(dst), rotate);
        }
    }
    Ok(())
}
