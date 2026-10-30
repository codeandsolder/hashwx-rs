//! AArch64 native emitter for the repo-owned HashX IR.

use crate::CompileError;
use crate::hashx::{Instruction, NativeRegisterFile, Program, RegisterId};
use dynasmrt::{AssemblyOffset, DynasmApi, DynasmLabelApi, ExecutableBuffer, dynasm};

type HashFn = unsafe extern "C" fn(*mut NativeRegisterFile);

pub(crate) struct Compiled {
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
            dynasmrt::aarch64::Assembler::new().map_err(|error| CompileError(error.to_string()))?;

        // x0 retains the RegisterFile pointer. HashX R0..R7 map to x1..x8.
        for register in RegisterId::all() {
            let hw = hw_register(register);
            let offset = register_offset(register);
            dynasm!(assembler; .arch aarch64; ldr X(hw), [x0, #offset]);
        }
        dynasm!(assembler
            ; .arch aarch64
            ; mov w9, wzr
            ; mov w10, wzr
        );

        let instructions: &[Instruction; 512] = program.into();
        let mut target = None;
        for instruction in instructions {
            emit_instruction(&mut assembler, instruction, &mut target)?;
        }

        for register in RegisterId::all() {
            let hw = hw_register(register);
            let offset = register_offset(register);
            dynasm!(assembler; .arch aarch64; str X(hw), [x0, #offset]);
        }
        dynasm!(assembler; .arch aarch64; ret);

        let buffer = assembler
            .finalize()
            .map_err(|_| CompileError("failed to finalize AArch64 HashX code".into()))?;
        // SAFETY: offset zero is the emitted function entry with the exact HashFn ABI,
        // and `buffer` is retained for the lifetime of `function`.
        let function: HashFn =
            unsafe { core::mem::transmute::<*const u8, HashFn>(buffer.ptr(AssemblyOffset(0))) };
        Ok(Self { buffer, function })
    }

    #[inline]
    pub(crate) fn invoke(&self, registers: &mut NativeRegisterFile) {
        // SAFETY: `compile` creates this pointer with the HashFn ABI and owns the
        // executable allocation for the duration of the call.
        unsafe { (self.function)(registers) };
    }
}

#[inline]
const fn hw_register(register: RegisterId) -> u8 {
    1 + register.as_u8()
}

#[inline]
fn register_offset(register: RegisterId) -> u32 {
    u32::from(register.as_u8()) * 8
}

fn emit_i32(assembler: &mut dynasmrt::aarch64::Assembler, value: i32) {
    let bits = u32::from_ne_bytes(value.to_ne_bytes());
    let low = bits & 0xffff;
    let high = bits >> 16;
    if value < 0 {
        let inverted_high = (!high) & 0xffff;
        dynasm!(assembler
            ; .arch aarch64
            ; movn x11, #inverted_high, lsl #16
            ; movk x11, #low
        );
    } else {
        dynasm!(assembler
            ; .arch aarch64
            ; movz x11, #high, lsl #16
            ; movk x11, #low
        );
    }
}

fn emit_u32(assembler: &mut dynasmrt::aarch64::Assembler, value: u32) {
    let low = value & 0xffff;
    let high = value >> 16;
    dynasm!(assembler
        ; .arch aarch64
        ; movz w11, #high, lsl #16
        ; movk w11, #low
    );
}

fn emit_instruction(
    assembler: &mut dynasmrt::aarch64::Assembler,
    instruction: &Instruction,
    target: &mut Option<dynasmrt::DynamicLabel>,
) -> Result<(), CompileError> {
    match instruction {
        Instruction::Target => {
            let label = assembler.new_dynamic_label();
            *target = Some(label);
            dynasm!(assembler; .arch aarch64; =>label);
        }
        Instruction::Branch { mask } => {
            let label = (*target)
                .ok_or_else(|| CompileError("HashX branch emitted without a target".into()))?;
            emit_u32(assembler, *mask);
            dynasm!(assembler
                ; .arch aarch64
                ; orr w9, w9, w10
                ; tst w9, w11
                ; csinv w10, w10, wzr, ne
                ; b.eq =>label
            );
        }
        Instruction::Mul { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler; .arch aarch64; mul X(dst), X(dst), X(src));
        }
        Instruction::UMulH { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler
                ; .arch aarch64
                ; umulh X(dst), X(dst), X(src)
                ; mov w9, W(dst)
            );
        }
        Instruction::SMulH { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler
                ; .arch aarch64
                ; smulh X(dst), X(dst), X(src)
                ; mov w9, W(dst)
            );
        }
        Instruction::AddShift {
            dst,
            src,
            left_shift,
        } => {
            if *left_shift > 3 {
                return Err(CompileError(
                    "HashX AddShift immediate outside 0..=3".into(),
                ));
            }
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            let shift = u32::from(*left_shift);
            dynasm!(assembler; .arch aarch64; add X(dst), X(dst), X(src), lsl #shift);
        }
        Instruction::AddConst { dst, src } => {
            let dst = hw_register(*dst);
            emit_i32(assembler, *src);
            dynasm!(assembler; .arch aarch64; add X(dst), X(dst), x11);
        }
        Instruction::Sub { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler; .arch aarch64; sub X(dst), X(dst), X(src));
        }
        Instruction::Xor { dst, src } => {
            let dst = hw_register(*dst);
            let src = hw_register(*src);
            dynasm!(assembler; .arch aarch64; eor X(dst), X(dst), X(src));
        }
        Instruction::XorConst { dst, src } => {
            let dst = hw_register(*dst);
            emit_i32(assembler, *src);
            dynasm!(assembler; .arch aarch64; eor X(dst), X(dst), x11);
        }
        Instruction::Rotate { dst, right_rotate } => {
            let dst = hw_register(*dst);
            let rotate = u32::from(*right_rotate);
            dynasm!(assembler; .arch aarch64; ror X(dst), X(dst), #rotate);
        }
    }
    Ok(())
}
