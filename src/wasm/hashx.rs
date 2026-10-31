//! WebAssembly emitter for `HashX` v1.

use crate::{HashXError, HashXInstruction, HashXProgram, HashXRegisterId};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
    Module, TypeSection, ValType,
};

use super::{e, sip};

const R0: u32 = 1;
const R1: u32 = 2;
const R2: u32 = 3;
const R3: u32 = 4;
const R4: u32 = 5;
const R5: u32 = 6;
const R6: u32 = 7;
const R7: u32 = 8;
const REGS: [u32; 8] = [R0, R1, R2, R3, R4, R5, R6, R7];

const S0: u32 = 9;
const S1: u32 = 10;
const S2: u32 = 11;
const S3: u32 = 12;

const A: u32 = 13;
const B: u32 = 14;
const U0: u32 = 15;
const V0: u32 = 16;
const U1: u32 = 17;
const V1: u32 = 18;
const T: u32 = 19;
const W1: u32 = 20;
const W2: u32 = 21;
const K: u32 = 22;
const BRANCH_TAKEN: u32 = 23;
const MULH_RESULT: u32 = 24;
const LIMB_MASK: i64 = 0xffff_ffff;

fn reg(id: HashXRegisterId) -> u32 {
    REGS[usize::from(id.index())]
}

fn copy_local(f: &mut Function, dst: u32, src: u32) {
    e(f, Instruction::LocalGet(src));
    e(f, Instruction::LocalSet(dst));
}

fn set_i64(f: &mut Function, dst: u32, value: u64) {
    e(f, Instruction::I64Const(value.cast_signed()));
    e(f, Instruction::LocalSet(dst));
}

fn xor_const(f: &mut Function, dst: u32, value: u64) {
    e(f, Instruction::LocalGet(dst));
    e(f, Instruction::I64Const(value.cast_signed()));
    e(f, Instruction::I64Xor);
    e(f, Instruction::LocalSet(dst));
}

fn add_const(f: &mut Function, dst: u32, value: u64) {
    e(f, Instruction::LocalGet(dst));
    e(f, Instruction::I64Const(value.cast_signed()));
    e(f, Instruction::I64Add);
    e(f, Instruction::LocalSet(dst));
}

// WebAssembly wide arithmetic has `i64.mul_wide_u` / `i64.mul_wide_s`, which
// removes most of this HashX Wasm performance gap (about 0.54x -> 0.91x native
// in the 2026-10-31 audit). Keep this baseline lowering for engine portability
// until Node/V8 accepts the proposal by default. The CI tripwire in
// `.github/scripts/node-wide-arithmetic-tripwire.mjs` intentionally fails when
// that happens so this lowering is revisited rather than silently becoming stale.
fn unsigned_mul_high(f: &mut Function, dst: u32, src: u32) {
    copy_local(f, A, dst);
    copy_local(f, B, src);

    for (out, input) in [(U0, A), (V0, B)] {
        e(f, Instruction::LocalGet(input));
        e(f, Instruction::I64Const(LIMB_MASK));
        e(f, Instruction::I64And);
        e(f, Instruction::LocalSet(out));
    }
    for (out, input) in [(U1, A), (V1, B)] {
        e(f, Instruction::LocalGet(input));
        e(f, Instruction::I64Const(32));
        e(f, Instruction::I64ShrU);
        e(f, Instruction::LocalSet(out));
    }

    e(f, Instruction::LocalGet(U0));
    e(f, Instruction::LocalGet(V0));
    e(f, Instruction::I64Mul);
    e(f, Instruction::LocalSet(T));
    e(f, Instruction::LocalGet(T));
    e(f, Instruction::I64Const(32));
    e(f, Instruction::I64ShrU);
    e(f, Instruction::LocalSet(K));

    e(f, Instruction::LocalGet(U1));
    e(f, Instruction::LocalGet(V0));
    e(f, Instruction::I64Mul);
    e(f, Instruction::LocalGet(K));
    e(f, Instruction::I64Add);
    e(f, Instruction::LocalSet(T));
    e(f, Instruction::LocalGet(T));
    e(f, Instruction::I64Const(LIMB_MASK));
    e(f, Instruction::I64And);
    e(f, Instruction::LocalSet(W1));
    e(f, Instruction::LocalGet(T));
    e(f, Instruction::I64Const(32));
    e(f, Instruction::I64ShrU);
    e(f, Instruction::LocalSet(W2));

    e(f, Instruction::LocalGet(U0));
    e(f, Instruction::LocalGet(V1));
    e(f, Instruction::I64Mul);
    e(f, Instruction::LocalGet(W1));
    e(f, Instruction::I64Add);
    e(f, Instruction::LocalSet(T));
    e(f, Instruction::LocalGet(T));
    e(f, Instruction::I64Const(32));
    e(f, Instruction::I64ShrU);
    e(f, Instruction::LocalSet(K));

    e(f, Instruction::LocalGet(U1));
    e(f, Instruction::LocalGet(V1));
    e(f, Instruction::I64Mul);
    e(f, Instruction::LocalGet(W2));
    e(f, Instruction::I64Add);
    e(f, Instruction::LocalGet(K));
    e(f, Instruction::I64Add);
    e(f, Instruction::LocalSet(dst));
}

fn save_mulh_result(f: &mut Function, dst: u32) {
    e(f, Instruction::LocalGet(dst));
    e(f, Instruction::I32WrapI64);
    e(f, Instruction::LocalSet(MULH_RESULT));
}

fn emit_control(f: &mut Function, instruction: &HashXInstruction, target_open: &mut bool) -> bool {
    match instruction {
        HashXInstruction::Target => {
            if *target_open {
                e(f, Instruction::End);
            }
            e(f, Instruction::Loop(BlockType::Empty));
            *target_open = true;
            true
        }
        HashXInstruction::Branch { mask } => {
            if *target_open {
                e(f, Instruction::LocalGet(BRANCH_TAKEN));
                e(f, Instruction::I32Eqz);
                e(f, Instruction::LocalGet(MULH_RESULT));
                e(
                    f,
                    Instruction::I32Const(i32::from_ne_bytes(mask.to_ne_bytes())),
                );
                e(f, Instruction::I32And);
                e(f, Instruction::I32Eqz);
                e(f, Instruction::I32And);
                e(f, Instruction::If(BlockType::Empty));
                e(f, Instruction::I32Const(1));
                e(f, Instruction::LocalSet(BRANCH_TAKEN));
                e(f, Instruction::Br(1));
                e(f, Instruction::End);
                e(f, Instruction::End);
                *target_open = false;
            }
            true
        }
        _ => false,
    }
}

fn emit_data_instruction(f: &mut Function, instruction: &HashXInstruction) {
    match instruction {
        HashXInstruction::Mul { dst, src } => {
            let dst = reg(*dst);
            e(f, Instruction::LocalGet(dst));
            e(f, Instruction::LocalGet(reg(*src)));
            e(f, Instruction::I64Mul);
            e(f, Instruction::LocalSet(dst));
        }
        HashXInstruction::UMulH { dst, src } => {
            let dst = reg(*dst);
            unsigned_mul_high(f, dst, reg(*src));
            save_mulh_result(f, dst);
        }
        HashXInstruction::SMulH { dst, src } => {
            let dst = reg(*dst);
            unsigned_mul_high(f, dst, reg(*src));

            e(f, Instruction::LocalGet(A));
            e(f, Instruction::I64Const(63));
            e(f, Instruction::I64ShrS);
            e(f, Instruction::LocalGet(B));
            e(f, Instruction::I64And);
            e(f, Instruction::LocalSet(T));
            e(f, Instruction::LocalGet(dst));
            e(f, Instruction::LocalGet(T));
            e(f, Instruction::I64Sub);
            e(f, Instruction::LocalSet(dst));

            e(f, Instruction::LocalGet(B));
            e(f, Instruction::I64Const(63));
            e(f, Instruction::I64ShrS);
            e(f, Instruction::LocalGet(A));
            e(f, Instruction::I64And);
            e(f, Instruction::LocalSet(T));
            e(f, Instruction::LocalGet(dst));
            e(f, Instruction::LocalGet(T));
            e(f, Instruction::I64Sub);
            e(f, Instruction::LocalSet(dst));
            save_mulh_result(f, dst);
        }
        HashXInstruction::AddShift {
            dst,
            src,
            left_shift,
        } => {
            let dst = reg(*dst);
            e(f, Instruction::LocalGet(dst));
            e(f, Instruction::LocalGet(reg(*src)));
            e(f, Instruction::I64Const(i64::from(*left_shift)));
            e(f, Instruction::I64Shl);
            e(f, Instruction::I64Add);
            e(f, Instruction::LocalSet(dst));
        }
        HashXInstruction::AddConst { dst, src } => {
            add_const(
                f,
                reg(*dst),
                u64::from_ne_bytes(i64::from(*src).to_ne_bytes()),
            );
        }
        HashXInstruction::Sub { dst, src } => {
            let dst = reg(*dst);
            e(f, Instruction::LocalGet(dst));
            e(f, Instruction::LocalGet(reg(*src)));
            e(f, Instruction::I64Sub);
            e(f, Instruction::LocalSet(dst));
        }
        HashXInstruction::Xor { dst, src } => {
            let dst = reg(*dst);
            e(f, Instruction::LocalGet(dst));
            e(f, Instruction::LocalGet(reg(*src)));
            e(f, Instruction::I64Xor);
            e(f, Instruction::LocalSet(dst));
        }
        HashXInstruction::XorConst { dst, src } => {
            xor_const(
                f,
                reg(*dst),
                u64::from_ne_bytes(i64::from(*src).to_ne_bytes()),
            );
        }
        HashXInstruction::Rotate { dst, right_rotate } => {
            let dst = reg(*dst);
            e(f, Instruction::LocalGet(dst));
            e(f, Instruction::I64Const(i64::from(*right_rotate)));
            e(f, Instruction::I64Rotr);
            e(f, Instruction::LocalSet(dst));
        }
        HashXInstruction::Target | HashXInstruction::Branch { .. } => {}
    }
}

fn emit_instruction(f: &mut Function, instruction: &HashXInstruction, target_open: &mut bool) {
    if !emit_control(f, instruction, target_open) {
        emit_data_instruction(f, instruction);
    }
}

fn initialize(f: &mut Function, key: [u64; 4]) {
    for (local, value) in [S0, S1, S2, S3].into_iter().zip(key) {
        set_i64(f, local, value);
    }
    xor_const(f, S1, 0xee);
    e(f, Instruction::LocalGet(S3));
    e(f, Instruction::LocalGet(0));
    e(f, Instruction::I64Xor);
    e(f, Instruction::LocalSet(S3));
    sip(f, S0, S1, S2, S3);
    sip(f, S0, S1, S2, S3);
    e(f, Instruction::LocalGet(S0));
    e(f, Instruction::LocalGet(0));
    e(f, Instruction::I64Xor);
    e(f, Instruction::LocalSet(S0));
    xor_const(f, S2, 0xee);
    for _ in 0..4 {
        sip(f, S0, S1, S2, S3);
    }
    for (dst, src) in REGS[..4].iter().copied().zip([S0, S1, S2, S3]) {
        copy_local(f, dst, src);
    }
    xor_const(f, S1, 0xdd);
    for _ in 0..4 {
        sip(f, S0, S1, S2, S3);
    }
    for (dst, src) in REGS[4..].iter().copied().zip([S0, S1, S2, S3]) {
        copy_local(f, dst, src);
    }
    e(f, Instruction::I32Const(0));
    e(f, Instruction::LocalSet(BRANCH_TAKEN));
    e(f, Instruction::I32Const(0));
    e(f, Instruction::LocalSet(MULH_RESULT));
}

fn finalize(f: &mut Function, key: [u64; 4]) {
    copy_local(f, S0, R0);
    copy_local(f, S1, R1);
    copy_local(f, S2, R2);
    copy_local(f, S3, R3);
    add_const(f, S0, key[0]);
    add_const(f, S1, key[1]);

    copy_local(f, A, R4);
    copy_local(f, B, R5);
    copy_local(f, U0, R6);
    copy_local(f, V0, R7);
    add_const(f, U0, key[2]);
    add_const(f, V0, key[3]);

    sip(f, S0, S1, S2, S3);
    sip(f, A, B, U0, V0);
    e(f, Instruction::LocalGet(S0));
    e(f, Instruction::LocalGet(A));
    e(f, Instruction::I64Xor);
}

fn body(program: &HashXProgram) -> Function {
    let register_key = program.register_key();
    let mut f = Function::new([(22, ValType::I64), (2, ValType::I32)]);
    initialize(&mut f, register_key);
    let mut target_open = false;
    for instruction in program.instructions() {
        emit_instruction(&mut f, instruction, &mut target_open);
    }
    if target_open {
        e(&mut f, Instruction::End);
    }
    finalize(&mut f, register_key);
    e(&mut f, Instruction::End);
    f
}

/// Compile an already-generated `HashX` program into a self-contained WebAssembly module.
///
/// The generated module imports nothing and exports `hash(i64) -> i64`,
/// returning the first 64 bits of the `HashX` digest.
#[must_use]
pub fn compile_program(program: &HashXProgram) -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([ValType::I64], [ValType::I64]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("hash", ExportKind::Func, 0);
    module.section(&exports);
    let mut code = CodeSection::new();
    code.function(&body(program));
    module.section(&code);
    module.finish()
}

/// Compile a seed into a self-contained `HashX` WebAssembly module.
///
/// # Errors
///
/// Returns [`HashXError::ProgramConstraints`] for seeds rejected by `HashX`.
pub fn compile(seed: &[u8]) -> Result<Vec<u8>, HashXError> {
    let program = HashXProgram::generate(seed)?;
    Ok(compile_program(&program))
}
