use crate::{
    ProgramSet, SEED_SIZE, generate,
    program::{Instruction as HIns, Op, Program},
};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
    MemArg, MemorySection, MemoryType, Module, TypeSection, ValType,
};

const R0: u32 = 1;
const R1: u32 = 2;
const R2: u32 = 3;
const R3: u32 = 4;
const R4: u32 = 5;
const R5: u32 = 6;
const R6: u32 = 7;
const R7: u32 = 8;
const R8: u32 = 9;
const T0: u32 = 10;
const T1: u32 = 11;
const T2: u32 = 12;
const T3: u32 = 13;
const REP: u32 = 14;
const BC: u32 = 15;
const MP: u32 = 16;
const REGS: [u32; 8] = [R0, R1, R2, R3, R4, R5, R6, R7];
#[expect(
    clippy::needless_pass_by_value,
    reason = "callers construct temporary Wasm instructions and this helper immediately encodes them"
)]
fn e(f: &mut Function, x: Instruction<'_>) {
    f.instruction(&x);
}
fn copy_local(f: &mut Function, d: u32, s: u32) {
    e(f, Instruction::LocalGet(s));
    e(f, Instruction::LocalSet(d));
}
fn xor_const(f: &mut Function, r: u32, v: u64) {
    e(f, Instruction::LocalGet(r));
    e(f, Instruction::I64Const(v.cast_signed()));
    e(f, Instruction::I64Xor);
    e(f, Instruction::LocalSet(r));
}
fn sip(function: &mut Function, reg0: u32, reg1: u32, reg2: u32, reg3: u32) {
    for &(destination, left, right) in &[(reg0, reg0, reg1), (reg2, reg2, reg3)] {
        e(function, Instruction::LocalGet(left));
        e(function, Instruction::LocalGet(right));
        e(function, Instruction::I64Add);
        e(function, Instruction::LocalSet(destination));
    }
    for &(register, rotation) in &[(reg1, 13), (reg3, 16)] {
        e(function, Instruction::LocalGet(register));
        e(function, Instruction::I64Const(rotation));
        e(function, Instruction::I64Rotl);
        e(function, Instruction::LocalSet(register));
    }
    for &(destination, left, right) in &[(reg1, reg1, reg0), (reg3, reg3, reg2)] {
        e(function, Instruction::LocalGet(left));
        e(function, Instruction::LocalGet(right));
        e(function, Instruction::I64Xor);
        e(function, Instruction::LocalSet(destination));
    }
    e(function, Instruction::LocalGet(reg0));
    e(function, Instruction::I64Const(32));
    e(function, Instruction::I64Rotl);
    e(function, Instruction::LocalSet(reg0));
    for &(destination, left, right) in &[(reg2, reg2, reg1), (reg0, reg0, reg3)] {
        e(function, Instruction::LocalGet(left));
        e(function, Instruction::LocalGet(right));
        e(function, Instruction::I64Add);
        e(function, Instruction::LocalSet(destination));
    }
    for &(register, rotation) in &[(reg1, 17), (reg3, 21)] {
        e(function, Instruction::LocalGet(register));
        e(function, Instruction::I64Const(rotation));
        e(function, Instruction::I64Rotl);
        e(function, Instruction::LocalSet(register));
    }
    for &(destination, left, right) in &[(reg1, reg1, reg2), (reg3, reg3, reg0)] {
        e(function, Instruction::LocalGet(left));
        e(function, Instruction::LocalGet(right));
        e(function, Instruction::I64Xor);
        e(function, Instruction::LocalSet(destination));
    }
    e(function, Instruction::LocalGet(reg2));
    e(function, Instruction::I64Const(32));
    e(function, Instruction::I64Rotl);
    e(function, Instruction::LocalSet(reg2));
}
const fn reg(r: u8) -> u32 {
    if r < 8 { REGS[r as usize] } else { R8 }
}
fn load_src(f: &mut Function, r: u8) {
    e(f, Instruction::LocalGet(reg(r)));
    e(f, Instruction::I32WrapI64);
    e(f, Instruction::I32Const(16383));
    e(f, Instruction::I32And);
    e(
        f,
        Instruction::I64Load(MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }),
    );
}
fn emit_op(f: &mut Function, x: HIns, mem: bool) {
    let d = reg(x.dst);
    let s = reg(x.src);
    match x.op {
        Op::MulOr | Op::MulXor | Op::MulAdd => {
            e(f, Instruction::LocalGet(d));
            e(f, Instruction::I64Const(i64::from(x.imm)));
            e(
                f,
                match x.op {
                    Op::MulOr => Instruction::I64Or,
                    Op::MulXor => Instruction::I64Xor,
                    _ => Instruction::I64Add,
                },
            );
            e(f, Instruction::LocalSet(d));
            e(f, Instruction::LocalGet(d));
            if mem {
                load_src(f, x.src);
            } else {
                e(f, Instruction::LocalGet(s));
            }
            e(f, Instruction::I64Mul);
            e(f, Instruction::LocalSet(d));
        }
        Op::Rmcg => {
            e(f, Instruction::LocalGet(d));
            e(f, Instruction::LocalGet(R8));
            e(f, Instruction::I64Mul);
            e(f, Instruction::I64Const(i64::from(x.imm)));
            e(f, Instruction::I64Rotr);
            e(f, Instruction::LocalSet(d));
        }
        _ => {
            e(f, Instruction::LocalGet(d));
            e(f, Instruction::I64Const(i64::from(x.imm)));
            e(
                f,
                match x.op {
                    Op::XorRor | Op::AddRor | Op::SubRor => Instruction::I64Rotr,
                    Op::XorAsr | Op::AddAsr | Op::SubAsr => Instruction::I64ShrS,
                    _ => Instruction::I64ShrU,
                },
            );
            e(f, Instruction::LocalSet(d));
            e(f, Instruction::LocalGet(d));
            if mem {
                load_src(f, x.src);
            } else {
                e(f, Instruction::LocalGet(s));
            }
            e(
                f,
                match x.op {
                    Op::XorRor | Op::XorAsr | Op::XorLsr => Instruction::I64Xor,
                    Op::AddRor | Op::AddAsr | Op::AddLsr => Instruction::I64Add,
                    _ => Instruction::I64Sub,
                },
            );
            e(f, Instruction::LocalSet(d));
        }
    }
}
fn snapshot(f: &mut Function) {
    e(f, Instruction::LocalGet(MP));
    e(f, Instruction::I32Const(64));
    e(f, Instruction::I32Sub);
    e(f, Instruction::LocalSet(MP));
    for (r, off) in REGS.into_iter().zip([56, 48, 40, 32, 24, 16, 8, 0]) {
        e(f, Instruction::LocalGet(MP));
        e(f, Instruction::LocalGet(r));
        e(
            f,
            Instruction::I64Store(MemArg {
                offset: off,
                align: 3,
                memory_index: 0,
            }),
        );
    }
}
fn emit_program(f: &mut Function, p: &Program, mem: bool) {
    e(f, Instruction::Block(BlockType::Empty));
    e(f, Instruction::Loop(BlockType::Empty));
    if !mem {
        snapshot(f);
    }
    for &x in &p.body {
        emit_op(f, x, mem);
    }
    if !p.unconditional {
        let bf = reg(p.body[0].dst);
        e(f, Instruction::LocalGet(bf));
        e(f, Instruction::I64Const(32));
        e(f, Instruction::I64And);
        e(f, Instruction::I64Eqz);
        e(f, Instruction::I32Eqz);
        e(f, Instruction::BrIf(1));
    }
    e(f, Instruction::LocalGet(BC));
    e(f, Instruction::I32Eqz);
    e(f, Instruction::BrIf(1));
    e(f, Instruction::LocalGet(BC));
    e(f, Instruction::I32Const(1));
    e(f, Instruction::I32Sub);
    e(f, Instruction::LocalSet(BC));
    e(f, Instruction::Br(0));
    e(f, Instruction::End);
    e(f, Instruction::End);
}
fn phase(f: &mut Function, set: &ProgramSet, mem: bool) {
    e(f, Instruction::I32Const(4));
    e(f, Instruction::LocalSet(REP));
    e(f, Instruction::Loop(BlockType::Empty));
    e(f, Instruction::I32Const(32));
    e(f, Instruction::LocalSet(BC));
    for p in &set.programs {
        emit_program(f, p, mem);
    }
    e(f, Instruction::LocalGet(REP));
    e(f, Instruction::I32Const(1));
    e(f, Instruction::I32Sub);
    e(f, Instruction::LocalTee(REP));
    e(f, Instruction::BrIf(0));
    e(f, Instruction::End);
}
fn body(set: &ProgramSet) -> Function {
    let mut f = Function::new([(13, ValType::I64), (3, ValType::I32)]);
    let [k0, k1] = set.hash_key;
    for (r, v) in [
        (R0, 0x736f_6d65_7073_6575 ^ k0),
        (R1, 0x646f_7261_6e64_6f6d ^ k1),
        (R2, 0x6c79_6765_6e65_7261 ^ k0),
        (R3, 0x7465_6462_7974_6573 ^ k1),
    ] {
        e(&mut f, Instruction::I64Const(v.cast_signed()));
        e(&mut f, Instruction::LocalSet(r));
    }
    e(&mut f, Instruction::LocalGet(R3));
    e(&mut f, Instruction::LocalGet(0));
    e(&mut f, Instruction::I64Xor);
    e(&mut f, Instruction::LocalSet(R3));
    sip(&mut f, R0, R1, R2, R3);
    e(&mut f, Instruction::LocalGet(R0));
    e(&mut f, Instruction::LocalGet(0));
    e(&mut f, Instruction::I64Xor);
    e(&mut f, Instruction::LocalSet(R0));
    xor_const(&mut f, R2, 0xbb);
    for _ in 0..3 {
        sip(&mut f, R0, R1, R2, R3);
    }
    copy_local(&mut f, T0, R3);
    copy_local(&mut f, T1, R2);
    copy_local(&mut f, T2, R1);
    copy_local(&mut f, T3, R0);
    xor_const(&mut f, R0, k0);
    xor_const(&mut f, R1, k1);
    xor_const(&mut f, R2, k0);
    xor_const(&mut f, R3, k1);
    for _ in 0..4 {
        sip(&mut f, R0, R1, R2, R3);
    }
    copy_local(&mut f, R4, R3);
    copy_local(&mut f, R5, R2);
    copy_local(&mut f, R6, R1);
    copy_local(&mut f, R7, R0);
    copy_local(&mut f, R0, T0);
    copy_local(&mut f, R1, T1);
    copy_local(&mut f, R2, T2);
    copy_local(&mut f, R3, T3);
    e(&mut f, Instruction::LocalGet(R0));
    e(&mut f, Instruction::LocalGet(R4));
    e(&mut f, Instruction::I64Xor);
    e(&mut f, Instruction::I64Const(-8));
    e(&mut f, Instruction::I64And);
    e(&mut f, Instruction::I64Const(3));
    e(&mut f, Instruction::I64Or);
    e(&mut f, Instruction::LocalSet(R8));
    e(&mut f, Instruction::I32Const(16384));
    e(&mut f, Instruction::LocalGet(R8));
    e(
        &mut f,
        Instruction::I64Store(MemArg {
            offset: 0,
            align: 3,
            memory_index: 0,
        }),
    );
    e(&mut f, Instruction::I32Const(16384));
    e(&mut f, Instruction::LocalSet(MP));
    phase(&mut f, set, false);
    phase(&mut f, set, true);
    sip(&mut f, R0, R1, R2, R3);
    sip(&mut f, R4, R5, R6, R7);
    e(&mut f, Instruction::LocalGet(R3));
    e(&mut f, Instruction::LocalGet(R7));
    e(&mut f, Instruction::I64Xor);
    e(&mut f, Instruction::LocalGet(R8));
    e(&mut f, Instruction::I64Xor);
    e(&mut f, Instruction::End);
    f
}
#[must_use]
pub fn compile(seed: &[u8; SEED_SIZE]) -> Vec<u8> {
    compile_program(&generate(seed))
}
#[must_use]
pub fn compile_program(set: &ProgramSet) -> Vec<u8> {
    let mut m = Module::new();
    let mut ts = TypeSection::new();
    ts.ty().function([ValType::I64], [ValType::I64]);
    m.section(&ts);
    let mut fs = FunctionSection::new();
    fs.function(0);
    m.section(&fs);
    let mut ms = MemorySection::new();
    ms.memory(MemoryType {
        minimum: 1,
        maximum: Some(1),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    m.section(&ms);
    let mut es = ExportSection::new();
    es.export("hash", ExportKind::Func, 0);
    m.section(&es);
    let mut cs = CodeSection::new();
    cs.function(&body(set));
    m.section(&cs);
    m.finish()
}
