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
use crate::hashx::program::Program;
use crate::hashx::register::RegisterFile as NativeRegisterFile;
use crate::hashx::{Instruction, RegisterId};
use dynasmrt::{AssemblyOffset, DynasmApi, DynasmLabelApi, ExecutableBuffer, dynasm};

#[cfg(windows)]
type HashFn = unsafe extern "win64" fn(*mut NativeRegisterFile);
#[cfg(not(windows))]
type HashFn = unsafe extern "sysv64" fn(*mut NativeRegisterFile);
#[cfg(windows)]
type HashWordFn = unsafe extern "win64" fn(u64) -> u64;
#[cfg(not(windows))]
type HashWordFn = unsafe extern "sysv64" fn(u64) -> u64;

pub struct Compiled {
    buffer: ExecutableBuffer,
    function: HashFn,
    hash_word_function: HashWordFn,
}

impl core::fmt::Debug for Compiled {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CompiledHashX")
            .field("code_len", &self.buffer.len())
            .finish_non_exhaustive()
    }
}

impl Compiled {
    pub(crate) fn compile(program: &Program, register_key: [u64; 4]) -> Result<Self, CompileError> {
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

        let hash_word_entry = assembler.offset();
        emit_hash_word_function(&mut assembler, instructions, register_key, arg)?;

        let buffer = assembler
            .finalize()
            .map_err(|_| CompileError("failed to finalize x86-64 HashX code".into()))?;
        // SAFETY: offset zero is the emitted function entry with the exact HashFn ABI,
        // and `buffer` is retained for the lifetime of `function`.
        let function: HashFn =
            unsafe { core::mem::transmute::<*const u8, HashFn>(buffer.ptr(AssemblyOffset(0))) };
        // SAFETY: `hash_word_entry` starts a second emitted function with the
        // exact HashWordFn ABI, and `buffer` owns both entry points.
        let hash_word_function: HashWordFn =
            unsafe { core::mem::transmute::<*const u8, HashWordFn>(buffer.ptr(hash_word_entry)) };
        Ok(Self {
            buffer,
            function,
            hash_word_function,
        })
    }

    #[inline]
    pub(crate) fn invoke(&self, registers: &mut NativeRegisterFile) {
        // SAFETY: `compile` creates this pointer with the HashFn ABI and the executable
        // allocation is owned by `self` for the duration of the call.
        unsafe { (self.function)(registers) };
    }

    #[inline]
    pub(crate) fn hash_word(&self, input: u64) -> u64 {
        // SAFETY: `compile` creates this pointer with the HashWordFn ABI and the
        // executable allocation is owned by `self` for the duration of the call.
        unsafe { (self.hash_word_function)(input) }
    }
}

fn emit_hash_word_function(
    assembler: &mut dynasmrt::x64::Assembler,
    instructions: &[Instruction; 512],
    register_key: [u64; 4],
    arg: u8,
) -> Result<(), CompileError> {
    let [k0, k1, k2, k3] = register_key.map(|word| i64::from_ne_bytes(word.to_ne_bytes()));

    // This entry point implements the common 64-bit HashX result entirely in
    // machine registers. The SipHash2,4 initializer produces R0..R7 directly
    // in r8..r15, avoiding a temporary RegisterFile and eight loads/stores.
    dynasm!(assembler
        ; .arch x64
        ; push r12
        ; push r13
        ; push r14
        ; push r15
    );
    #[cfg(windows)]
    dynasm!(assembler; .arch x64; push rsi);
    dynasm!(assembler
        ; .arch x64
        ; mov r8, QWORD k0
        ; mov r9, QWORD k1
        ; mov r10, QWORD k2
        ; mov r11, QWORD k3
        ; xor r9, 0xee
        ; xor r11, Rq(arg)
    );
    emit_sip_round(assembler, 8, 9, 10, 11);
    emit_sip_round(assembler, 8, 9, 10, 11);
    dynasm!(assembler
        ; .arch x64
        ; xor r8, Rq(arg)
        ; xor r10, 0xee
    );
    for _ in 0..4 {
        emit_sip_round(assembler, 8, 9, 10, 11);
    }
    dynasm!(assembler
        ; .arch x64
        ; mov r12, r8
        ; mov r13, r9
        ; mov r14, r10
        ; mov r15, r11
        ; xor r13, 0xdd
    );
    for _ in 0..4 {
        emit_sip_round(assembler, 12, 13, 14, 15);
    }

    dynasm!(assembler
        ; .arch x64
        ; xor edx, edx
        ; xor esi, esi
    );
    let mut target = None;
    for instruction in instructions {
        emit_instruction(assembler, instruction, &mut target)?;
    }

    // Digest in-place. Only word zero is needed, so the eight program registers
    // become the two SipState halves directly and no RegisterFile is written.
    dynasm!(assembler
        ; .arch x64
        ; mov rax, QWORD k0
        ; add r8, rax
        ; mov rax, QWORD k1
        ; add r9, rax
        ; mov rax, QWORD k2
        ; add r14, rax
        ; mov rax, QWORD k3
        ; add r15, rax
    );
    emit_sip_round_v0(assembler, 8, 9, 10, 11);
    emit_sip_round_v0(assembler, 12, 13, 14, 15);
    dynasm!(assembler
        ; .arch x64
        ; mov rax, r8
        ; xor rax, r12
    );
    #[cfg(windows)]
    dynasm!(assembler; .arch x64; pop rsi);
    dynasm!(assembler
        ; .arch x64
        ; pop r15
        ; pop r14
        ; pop r13
        ; pop r12
        ; ret
    );
    Ok(())
}

fn emit_sip_round_v0(assembler: &mut dynasmrt::x64::Assembler, v0: u8, v1: u8, v2: u8, v3: u8) {
    // The final HashX word only needs SipRound's resulting v0. The updates to
    // v1 and the second half of v2 are not on that dependency path.
    dynasm!(assembler
        ; .arch x64
        ; add Rq(v0), Rq(v1)
        ; add Rq(v2), Rq(v3)
        ; rol Rq(v3), 16
        ; xor Rq(v3), Rq(v2)
        ; rol Rq(v0), 32
        ; add Rq(v0), Rq(v3)
    );
}

fn emit_sip_round(assembler: &mut dynasmrt::x64::Assembler, v0: u8, v1: u8, v2: u8, v3: u8) {
    dynasm!(assembler
        ; .arch x64
        ; add Rq(v0), Rq(v1)
        ; add Rq(v2), Rq(v3)
        ; rol Rq(v1), 13
        ; rol Rq(v3), 16
        ; xor Rq(v1), Rq(v0)
        ; xor Rq(v3), Rq(v2)
        ; rol Rq(v0), 32
        ; add Rq(v2), Rq(v1)
        ; add Rq(v0), Rq(v3)
        ; rol Rq(v1), 17
        ; rol Rq(v3), 21
        ; xor Rq(v1), Rq(v2)
        ; xor Rq(v3), Rq(v0)
        ; rol Rq(v2), 32
    );
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
            let skip = assembler.new_dynamic_label();
            let mask = i32::from_ne_bytes(mask.to_ne_bytes());
            dynasm!(assembler
                ; .arch x64
                ; test esi, esi
                ; jnz =>skip
                ; test edx, mask
                ; jnz =>skip
                ; mov esi, -1
                ; jmp =>label
                ; =>skip
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
