#![expect(
    clippy::useless_conversion,
    reason = "dynasm dynamic-register expansion uses .into() internally"
)]
#![expect(
    clippy::used_underscore_binding,
    reason = "dynasm dynamic-register expansion synthesizes underscore-prefixed temporaries"
)]

use crate::{
    CompileError,
    program::{Instruction, Op, Program, ProgramSet},
};
use dynasmrt::{DynasmApi, DynasmLabelApi, ExecutableBuffer, dynasm};
#[cfg(windows)]
type HashFn = unsafe extern "win64" fn(u64) -> u64;
#[cfg(not(windows))]
type HashFn = unsafe extern "sysv64" fn(u64) -> u64;
pub struct Compiled {
    _buffer: ExecutableBuffer,
    function: HashFn,
    code_len: usize,
}
impl Compiled {
    pub(crate) fn compile(set: &ProgramSet) -> Result<Self, CompileError> {
        let mut assembler =
            dynasmrt::x64::Assembler::new().map_err(|error| CompileError(error.to_string()))?;
        let arg_reg = if cfg!(windows) { 1u8 } else { 7u8 };
        let [k0, k1] = set.hash_key;
        let init0 = (0x736f_6d65_7073_6575_u64 ^ k0).cast_signed();
        let init1 = (0x646f_7261_6e64_6f6d_u64 ^ k1).cast_signed();
        let init2 = (0x6c79_6765_6e65_7261_u64 ^ k0).cast_signed();
        let init3 = (0x7465_6462_7974_6573_u64 ^ k1).cast_signed();
        let signed_k0 = k0.cast_signed();
        let signed_k1 = k1.cast_signed();
        dynasm!(assembler;.arch x64;push rbx;push r12;push r13;push r14;push r15);
        #[cfg(windows)]
        dynasm!(assembler;push rsi);
        dynasm!(assembler;mov r8,QWORD init0;mov r9,QWORD init1;mov r10,QWORD init2;mov r11,QWORD init3;xor r11,Rq(arg_reg));
        emit_sip(&mut assembler, 8, 9, 10, 11);
        dynasm!(assembler;xor r8,Rq(arg_reg);xor r10,0xbb);
        for _ in 0..3 {
            emit_sip(&mut assembler, 8, 9, 10, 11);
        }
        dynasm!(assembler;mov r12,r11;mov r13,r10;mov r14,r9;mov r15,r8;mov rax,QWORD signed_k0;xor r8,rax;xor r10,rax;mov rax,QWORD signed_k1;xor r9,rax;xor r11,rax);
        for _ in 0..4 {
            emit_sip(&mut assembler, 8, 9, 10, 11);
        }
        dynasm!(assembler;mov rax,r11;mov rcx,r10;mov rdx,r9;mov rsi,r8;mov r8,r12;mov r9,r13;mov r10,r14;mov r11,r15;mov r12,rax;mov r13,rcx;mov r14,rdx;mov r15,rsi;mov rbx,r8;xor rbx,r12;and rbx,-8;or rbx,3;push rbx);
        phase(&mut assembler, &set.programs, false);
        phase(&mut assembler, &set.programs, true);
        dynasm!(assembler;add rsp,16392);
        emit_sip(&mut assembler, 8, 9, 10, 11);
        emit_sip(&mut assembler, 12, 13, 14, 15);
        dynasm!(assembler;mov rax,r11;xor rax,r15;xor rax,rbx);
        #[cfg(windows)]
        dynasm!(assembler;pop rsi);
        dynasm!(assembler;pop r15;pop r14;pop r13;pop r12;pop rbx;ret);
        let buffer = assembler
            .finalize()
            .map_err(|_| CompileError("failed to finalize x86-64 code".into()))?;
        let code_len = buffer.len();
        // SAFETY: offset zero is the entry point emitted above with the platform HashFn ABI,
        // and the executable buffer is retained in Self for at least as long as the pointer.
        let function: HashFn =
            unsafe { core::mem::transmute(buffer.ptr(dynasmrt::AssemblyOffset(0))) };
        Ok(Self {
            _buffer: buffer,
            function,
            code_len,
        })
    }
    #[inline]
    pub(crate) fn hash(&self, nonce: u64) -> u64 {
        // SAFETY: compile creates this pointer from code emitted with the exact HashFn ABI;
        // self owns the executable buffer that keeps the generated function alive.
        unsafe { (self.function)(nonce) }
    }
    pub(crate) const fn code_len(&self) -> usize {
        self.code_len
    }
}
fn emit_sip(assembler: &mut dynasmrt::x64::Assembler, reg0: u8, reg1: u8, reg2: u8, reg3: u8) {
    dynasm!(assembler;add Rq(reg0),Rq(reg1);add Rq(reg2),Rq(reg3);rol Rq(reg1),13;rol Rq(reg3),16;xor Rq(reg1),Rq(reg0);xor Rq(reg3),Rq(reg2);rol Rq(reg0),32;add Rq(reg2),Rq(reg1);add Rq(reg0),Rq(reg3);rol Rq(reg1),17;rol Rq(reg3),21;xor Rq(reg1),Rq(reg2);xor Rq(reg3),Rq(reg0);rol Rq(reg2),32);
}
#[inline]
const fn xr(register: u8) -> u8 {
    if register < 8 { 8 + register } else { 3 }
}
fn source(assembler: &mut dynasmrt::x64::Assembler, source: u8) {
    let source = xr(source);
    dynasm!(assembler;mov eax,Rd(source);and eax,16383);
}
fn op(assembler: &mut dynasmrt::x64::Assembler, instruction: Instruction, memory: bool) {
    let dst = xr(instruction.dst);
    let src = xr(instruction.src);
    let immediate = instruction.imm.cast_signed();
    match instruction.op {
        Op::MulOr | Op::MulXor | Op::MulAdd => {
            match instruction.op {
                Op::MulOr => dynasm!(assembler;or Rq(dst),i32::from(instruction.imm)),
                Op::MulXor => dynasm!(assembler;xor Rq(dst),i32::from(instruction.imm)),
                _ => dynasm!(assembler;add Rq(dst),i32::from(instruction.imm)),
            }
            if memory {
                source(assembler, instruction.src);
                dynasm!(assembler;imul Rq(dst),QWORD[rsp+rax]);
            } else {
                dynasm!(assembler;imul Rq(dst),Rq(src));
            }
        }
        Op::Rmcg => dynasm!(assembler;imul Rq(dst),rbx;ror Rq(dst),immediate),
        _ => {
            match instruction.op {
                Op::XorRor | Op::AddRor | Op::SubRor => {
                    dynasm!(assembler;ror Rq(dst),immediate);
                }
                Op::XorAsr | Op::AddAsr | Op::SubAsr => {
                    dynasm!(assembler;sar Rq(dst),immediate);
                }
                _ => dynasm!(assembler;shr Rq(dst),immediate),
            }
            if memory {
                source(assembler, instruction.src);
                match instruction.op {
                    Op::XorRor | Op::XorAsr | Op::XorLsr => {
                        dynasm!(assembler;xor Rq(dst),QWORD[rsp+rax]);
                    }
                    Op::AddRor | Op::AddAsr | Op::AddLsr => {
                        dynasm!(assembler;add Rq(dst),QWORD[rsp+rax]);
                    }
                    _ => dynasm!(assembler;sub Rq(dst),QWORD[rsp+rax]),
                }
            } else {
                match instruction.op {
                    Op::XorRor | Op::XorAsr | Op::XorLsr => {
                        dynasm!(assembler;xor Rq(dst),Rq(src));
                    }
                    Op::AddRor | Op::AddAsr | Op::AddLsr => {
                        dynasm!(assembler;add Rq(dst),Rq(src));
                    }
                    _ => dynasm!(assembler;sub Rq(dst),Rq(src)),
                }
            }
        }
    }
}
fn phase(assembler: &mut dynasmrt::x64::Assembler, programs: &[Program; 32], memory: bool) {
    let repeat = assembler.new_dynamic_label();
    dynasm!(assembler;mov esi,4;=>repeat;mov edx,32);
    for program in programs {
        let start = assembler.new_dynamic_label();
        let done = assembler.new_dynamic_label();
        dynasm!(assembler;=>start);
        if !memory {
            dynasm!(assembler;push r8;push r9;push r10;push r11;push r12;push r13;push r14;push r15);
        }
        for &instruction in &program.body {
            op(assembler, instruction, memory);
        }
        let branch_flag = xr(program.body[0].dst);
        dynasm!(assembler;test edx,edx;jz=>done);
        if !program.unconditional {
            dynasm!(assembler;test Rd(branch_flag),32;jnz=>done);
        }
        dynasm!(assembler;dec edx;jmp=>start;=>done);
    }
    dynasm!(assembler;dec esi;jnz=>repeat);
}
