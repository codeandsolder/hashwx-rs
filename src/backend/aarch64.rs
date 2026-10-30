#![expect(
    clippy::useless_conversion,
    reason = "dynasm dynamic-register expansion uses .into() internally"
)]

use crate::{
    CompileError,
    program::{Instruction, Op, Program, ProgramSet},
};
use dynasmrt::{DynasmApi, DynasmLabelApi, ExecutableBuffer, dynasm};

type HashFn = unsafe extern "C" fn(*mut u64);

pub(crate) struct Compiled {
    _buffer: ExecutableBuffer,
    function: HashFn,
    hash_key: [u64; 2],
    code_len: usize,
}

impl Compiled {
    pub(crate) fn compile(set: &ProgramSet) -> Result<Self, CompileError> {
        let mut o = dynasmrt::aarch64::Assembler::new().map_err(|e| CompileError(e.to_string()))?;
        let entry = o.offset();
        dynasm!(o
            ; .arch aarch64
            ; mov x8, x0
            ; ldr x12, [x0, 64]
            ; ldp x6, x7, [x0, 48]
            ; ldp x4, x5, [x0, 32]
            ; ldp x2, x3, [x0, 16]
            ; ldp x0, x1, [x0]
            ; mov x10, 1
            ; mov x11, 9
            ; mov x14, 33
            ; sub sp, sp, 16
            ; str x12, [sp]
        );
        phase(&mut o, &set.programs, false);
        phase(&mut o, &set.programs, true);
        dynasm!(o
            ; .arch aarch64
            ; stp x6, x7, [x8, 48]
            ; stp x4, x5, [x8, 32]
            ; stp x2, x3, [x8, 16]
            ; stp x0, x1, [x8]
            ; add sp, sp, 4, LSL 12
            ; add sp, sp, 16
            ; ret
        );
        let b = o
            .finalize()
            .map_err(|_| CompileError("failed to finalize AArch64 code".into()))?;
        let code_len = b.len();
        let function: HashFn = unsafe { core::mem::transmute(b.ptr(entry)) };
        Ok(Self {
            _buffer: b,
            function,
            hash_key: set.hash_key,
            code_len,
        })
    }

    #[inline]
    pub(crate) fn hash(&self, nonce: u64) -> u64 {
        let mut state = initialize(self.hash_key, nonce);
        unsafe { (self.function)(state.as_mut_ptr()) };
        finalize(&state)
    }

    pub(crate) const fn code_len(&self) -> usize {
        self.code_len
    }
}

fn sip(v: &mut [u64; 4]) {
    v[0] = v[0].wrapping_add(v[1]);
    v[2] = v[2].wrapping_add(v[3]);
    v[1] = v[1].rotate_left(13);
    v[3] = v[3].rotate_left(16);
    v[1] ^= v[0];
    v[3] ^= v[2];
    v[0] = v[0].rotate_left(32);
    v[2] = v[2].wrapping_add(v[1]);
    v[0] = v[0].wrapping_add(v[3]);
    v[1] = v[1].rotate_left(17);
    v[3] = v[3].rotate_left(21);
    v[1] ^= v[2];
    v[3] ^= v[0];
    v[2] = v[2].rotate_left(32);
}

fn initialize([k0, k1]: [u64; 2], nonce: u64) -> [u64; 9] {
    let mut v = [
        0x736f_6d65_7073_6575 ^ k0,
        0x646f_7261_6e64_6f6d ^ k1,
        0x6c79_6765_6e65_7261 ^ k0,
        0x7465_6462_7974_6573 ^ k1,
    ];
    v[3] ^= nonce;
    sip(&mut v);
    v[0] ^= nonce;
    v[2] ^= 0xbb;
    for _ in 0..3 {
        sip(&mut v);
    }
    let mut r = [0u64; 9];
    [r[0], r[1], r[2], r[3]] = [v[3], v[2], v[1], v[0]];
    v[0] ^= k0;
    v[1] ^= k1;
    v[2] ^= k0;
    v[3] ^= k1;
    for _ in 0..4 {
        sip(&mut v);
    }
    [r[4], r[5], r[6], r[7]] = [v[3], v[2], v[1], v[0]];
    r[8] = ((r[0] ^ r[4]) & !7) | 3;
    r
}

fn finalize(r: &[u64; 9]) -> u64 {
    let mut lo = [r[0], r[1], r[2], r[3]];
    let mut hi = [r[4], r[5], r[6], r[7]];
    sip(&mut lo);
    sip(&mut hi);
    lo[3] ^ hi[3] ^ r[8]
}

fn imm_reg(imm: u8) -> u8 {
    match imm {
        1 => 10,
        9 => 11,
        33 => 14,
        _ => unreachable!("HashWX multiply immediate is one of 1, 9, or 33"),
    }
}

fn source(o: &mut dynasmrt::aarch64::Assembler, src: u8) {
    dynasm!(o
        ; .arch aarch64
        ; and x15, X(src), 16383
        ; ldr x15, [sp, x15]
    );
}

fn op(o: &mut dynasmrt::aarch64::Assembler, x: Instruction, mem: bool) {
    let d = x.dst;
    let s = x.src;
    let imm = u32::from(x.imm);
    match x.op {
        Op::MulOr | Op::MulXor | Op::MulAdd => {
            if mem {
                source(o, s);
            }
            let k = imm_reg(x.imm);
            match x.op {
                Op::MulOr => dynasm!(o; .arch aarch64; orr X(d), X(d), X(k)),
                Op::MulXor => dynasm!(o; .arch aarch64; eor X(d), X(d), X(k)),
                Op::MulAdd => dynasm!(o; .arch aarch64; add X(d), X(d), X(k)),
                _ => unreachable!(),
            }
            if mem {
                dynasm!(o; .arch aarch64; mul X(d), X(d), x15);
            } else {
                dynasm!(o; .arch aarch64; mul X(d), X(d), X(s));
            }
        }
        Op::Rmcg => dynasm!(o
            ; .arch aarch64
            ; mul X(d), X(d), x12
            ; ror X(d), X(d), imm
        ),
        _ => {
            if mem {
                source(o, s);
            }
            match x.op {
                Op::XorRor | Op::AddRor | Op::SubRor => {
                    dynasm!(o; .arch aarch64; ror X(d), X(d), imm)
                }
                Op::XorAsr | Op::AddAsr | Op::SubAsr => {
                    dynasm!(o; .arch aarch64; asr X(d), X(d), imm)
                }
                Op::XorLsr | Op::AddLsr | Op::SubLsr => {
                    dynasm!(o; .arch aarch64; lsr X(d), X(d), imm)
                }
                _ => unreachable!(),
            }
            if mem {
                match x.op {
                    Op::XorRor | Op::XorAsr | Op::XorLsr => {
                        dynasm!(o; .arch aarch64; eor X(d), X(d), x15)
                    }
                    Op::AddRor | Op::AddAsr | Op::AddLsr => {
                        dynasm!(o; .arch aarch64; add X(d), X(d), x15)
                    }
                    Op::SubRor | Op::SubAsr | Op::SubLsr => {
                        dynasm!(o; .arch aarch64; sub X(d), X(d), x15)
                    }
                    _ => unreachable!(),
                }
            } else {
                match x.op {
                    Op::XorRor | Op::XorAsr | Op::XorLsr => {
                        dynasm!(o; .arch aarch64; eor X(d), X(d), X(s))
                    }
                    Op::AddRor | Op::AddAsr | Op::AddLsr => {
                        dynasm!(o; .arch aarch64; add X(d), X(d), X(s))
                    }
                    Op::SubRor | Op::SubAsr | Op::SubLsr => {
                        dynasm!(o; .arch aarch64; sub X(d), X(d), X(s))
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

fn phase(o: &mut dynasmrt::aarch64::Assembler, programs: &[Program; 32], mem: bool) {
    let repeat = o.new_dynamic_label();
    dynasm!(o
        ; .arch aarch64
        ; mov x13, 4
        ; =>repeat
        ; mov x9, 0
    );
    for p in programs {
        let start = o.new_dynamic_label();
        let done = o.new_dynamic_label();
        dynasm!(o; .arch aarch64; =>start);
        if !mem {
            dynasm!(o
                ; .arch aarch64
                ; sub sp, sp, 64
                ; stp x1, x0, [sp, 48]
                ; stp x3, x2, [sp, 32]
                ; stp x5, x4, [sp, 16]
                ; stp x7, x6, [sp]
            );
        }
        for &x in &p.body {
            op(o, x, mem);
        }
        let bf = p.body[0].dst;
        dynasm!(o; .arch aarch64; tbnz x9, 5, =>done);
        if !p.unconditional {
            dynasm!(o; .arch aarch64; tbnz X(bf), 5, =>done);
        }
        dynasm!(o
            ; .arch aarch64
            ; add x9, x9, 1
            ; b =>start
            ; =>done
        );
    }
    dynasm!(o
        ; .arch aarch64
        ; subs x13, x13, 1
        ; b.ne =>repeat
    );
}
