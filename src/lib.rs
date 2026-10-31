//! `HashWX` 1.0 and `HashX` v1 program generation and execution.
//!
//! [`HashWx`] provides seed-specialized native JIT execution on x86-64 and
//! `AArch64`. [`ProgramSet`] exposes the generated `HashWX` IR read-only for
//! alternate backends, and the optional [`wasm`] module emits portable `HashWX`
//! WebAssembly. With the `hashx` feature, [`HashX`] provides the repo-owned
//! `HashX` v1 interpreter and native JIT.

mod backend;
mod program;
mod tables;
#[cfg(feature = "wasm")]
pub mod wasm;
pub use program::{
    Instruction, Op, PROGRAM_COUNT, PROGRAM_SIZE, Program, ProgramSet, SEED_SIZE, generate,
};

#[derive(Debug)]
/// Failure while creating or finalizing a native JIT function.
pub struct CompileError(String);
impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CompileError {}

#[cfg(all(feature = "native", target_arch = "aarch64"))]
use backend::aarch64 as native_backend;
#[cfg(all(feature = "native", target_arch = "x86_64"))]
use backend::x86_64 as native_backend;

#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
/// Seed-specialized native `HashWX` function.
///
/// The generated executable buffer is immutable after construction, and the
/// type is safe to share between threads.
pub struct HashWx {
    inner: native_backend::Compiled,
}
#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
impl HashWx {
    /// Compile a `HashWX` seed into native machine code.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] if the native assembler cannot be created or
    /// the emitted code cannot be finalized.
    pub fn compile(seed: &[u8; 32]) -> Result<Self, CompileError> {
        Self::compile_program(&generate(seed))
    }

    /// Compile an already-generated `HashWX` program into native machine code.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] if the native assembler cannot be created or
    /// the emitted code cannot be finalized.
    pub fn compile_program(program: &ProgramSet) -> Result<Self, CompileError> {
        Ok(Self {
            inner: native_backend::Compiled::compile(program)?,
        })
    }
    /// Hash one nonce with this seed-specialized `HashWX` function.
    #[inline]
    #[must_use]
    pub fn hash(&self, n: u64) -> u64 {
        self.inner.hash(n)
    }
    /// Size of the generated native function in bytes.
    #[must_use]
    pub const fn code_len(&self) -> usize {
        self.inner.code_len()
    }
}

#[cfg(feature = "hashx")]
pub(crate) mod hashx;
#[cfg(feature = "hashx")]
pub use hashx::{
    HashX, HashXError, HashXProgram, HashXRuntime, Instruction as HashXInstruction,
    NUM_INSTRUCTIONS as HASHX_PROGRAM_SIZE, RegisterId as HashXRegisterId,
};
