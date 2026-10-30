#![expect(
    clippy::useless_conversion,
    reason = "dynasm dynamic-register expansion uses .into() internally"
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
pub(crate) struct Compiled {
    _buffer: ExecutableBuffer,
    function: HashFn,
    code_len: usize,
}
impl Compiled {
    pub(crate) fn compile(set: &ProgramSet) -> Result<Self, CompileError> {
        let mut o = dynasmrt::x64::Assembler::new().map_err(|e| CompileError(e.to_string()))?;
        let a = if cfg!(windows) { 1u8 } else { 7u8 };
        let [k0, k1] = set.hash_key;
        dynasm!(o;.arch x64;push rbx;push r12;push r13;push r14;push r15);
        #[cfg(windows)]
        dynasm!(o;push rsi);
        dynasm!(o;mov r8,QWORD(0x736f6d6570736575u64^k0)as i64;mov r9,QWORD(0x646f72616e646f6du64^k1)as i64;mov r10,QWORD(0x6c7967656e657261u64^k0)as i64;mov r11,QWORD(0x7465646279746573u64^k1)as i64;xor r11,Rq(a));
        emit_sip(&mut o, 8, 9, 10, 11);
        dynasm!(o;xor r8,Rq(a);xor r10,0xbb);
        for _ in 0..3 {
            emit_sip(&mut o, 8, 9, 10, 11)
        }
        dynasm!(o;mov r12,r11;mov r13,r10;mov r14,r9;mov r15,r8;mov rax,QWORD k0 as i64;xor r8,rax;xor r10,rax;mov rax,QWORD k1 as i64;xor r9,rax;xor r11,rax);
        for _ in 0..4 {
            emit_sip(&mut o, 8, 9, 10, 11)
        }
        dynasm!(o;mov rax,r11;mov rcx,r10;mov rdx,r9;mov rsi,r8;mov r8,r12;mov r9,r13;mov r10,r14;mov r11,r15;mov r12,rax;mov r13,rcx;mov r14,rdx;mov r15,rsi;mov rbx,r8;xor rbx,r12;and rbx,-8;or rbx,3;push rbx);
        phase(&mut o, &set.programs, false);
        phase(&mut o, &set.programs, true);
        dynasm!(o;add rsp,16392);
        emit_sip(&mut o, 8, 9, 10, 11);
        emit_sip(&mut o, 12, 13, 14, 15);
        dynasm!(o;mov rax,r11;xor rax,r15;xor rax,rbx);
        #[cfg(windows)]
        dynasm!(o;pop rsi);
        dynasm!(o;pop r15;pop r14;pop r13;pop r12;pop rbx;ret);
        let b = o
            .finalize()
            .map_err(|_| CompileError("failed to finalize x86-64 code".into()))?;
        let n = b.len();
        let f: HashFn = unsafe { core::mem::transmute(b.ptr(dynasmrt::AssemblyOffset(0))) };
        Ok(Self {
            _buffer: b,
            function: f,
            code_len: n,
        })
    }
    #[inline]
    pub(crate) fn hash(&self, n: u64) -> u64 {
        unsafe { (self.function)(n) }
    }
    pub(crate) fn code_len(&self) -> usize {
        self.code_len
    }
}
fn emit_sip(o: &mut dynasmrt::x64::Assembler, a: u8, b: u8, c: u8, d: u8) {
    dynasm!(o;add Rq(a),Rq(b);add Rq(c),Rq(d);rol Rq(b),13;rol Rq(d),16;xor Rq(b),Rq(a);xor Rq(d),Rq(c);rol Rq(a),32;add Rq(c),Rq(b);add Rq(a),Rq(d);rol Rq(b),17;rol Rq(d),21;xor Rq(b),Rq(c);xor Rq(d),Rq(a);rol Rq(c),32)
}
#[inline]
fn xr(r: u8) -> u8 {
    if r < 8 { 8 + r } else { 3 }
}
fn source(o: &mut dynasmrt::x64::Assembler, s: u8) {
    let s = xr(s);
    dynasm!(o;mov eax,Rd(s);and eax,16383)
}
fn op(o: &mut dynasmrt::x64::Assembler, x: Instruction, mem: bool) {
    let d = xr(x.dst);
    let s = xr(x.src);
    let im = x.imm as i8;
    match x.op {
        Op::MulOr | Op::MulXor | Op::MulAdd => {
            match x.op {
                Op::MulOr => dynasm!(o;or Rq(d),x.imm as i32),
                Op::MulXor => dynasm!(o;xor Rq(d),x.imm as i32),
                _ => dynasm!(o;add Rq(d),x.imm as i32),
            }
            if mem {
                source(o, x.src);
                dynasm!(o;imul Rq(d),QWORD[rsp+rax])
            } else {
                dynasm!(o;imul Rq(d),Rq(s))
            }
        }
        Op::Rmcg => dynasm!(o;imul Rq(d),rbx;ror Rq(d),im),
        _ => {
            match x.op {
                Op::XorRor | Op::AddRor | Op::SubRor => dynasm!(o;ror Rq(d),im),
                Op::XorAsr | Op::AddAsr | Op::SubAsr => dynasm!(o;sar Rq(d),im),
                _ => dynasm!(o;shr Rq(d),im),
            }
            if mem {
                source(o, x.src);
                match x.op {
                    Op::XorRor | Op::XorAsr | Op::XorLsr => dynasm!(o;xor Rq(d),QWORD[rsp+rax]),
                    Op::AddRor | Op::AddAsr | Op::AddLsr => dynasm!(o;add Rq(d),QWORD[rsp+rax]),
                    _ => dynasm!(o;sub Rq(d),QWORD[rsp+rax]),
                }
            } else {
                match x.op {
                    Op::XorRor | Op::XorAsr | Op::XorLsr => dynasm!(o;xor Rq(d),Rq(s)),
                    Op::AddRor | Op::AddAsr | Op::AddLsr => dynasm!(o;add Rq(d),Rq(s)),
                    _ => dynasm!(o;sub Rq(d),Rq(s)),
                }
            }
        }
    }
}
fn phase(o: &mut dynasmrt::x64::Assembler, ps: &[Program; 32], mem: bool) {
    let rep = o.new_dynamic_label();
    dynasm!(o;mov esi,4;=>rep;mov edx,32);
    for p in ps {
        let s = o.new_dynamic_label();
        let done = o.new_dynamic_label();
        dynasm!(o;=>s);
        if !mem {
            dynasm!(o;push r8;push r9;push r10;push r11;push r12;push r13;push r14;push r15)
        }
        for &x in &p.body {
            op(o, x, mem)
        }
        let bf = xr(p.body[0].dst);
        dynasm!(o;test edx,edx;jz=>done);
        if !p.unconditional {
            dynasm!(o;test Rd(bf),32;jnz=>done)
        }
        dynasm!(o;dec edx;jmp=>s;=>done)
    }
    dynasm!(o;dec esi;jnz=>rep)
}
