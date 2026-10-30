mod backend;
mod program;
mod tables;
#[cfg(feature = "wasm")]
pub mod wasm;
pub use program::{
    Instruction, Op, PROGRAM_COUNT, PROGRAM_SIZE, Program, ProgramSet, SEED_SIZE, generate,
};

#[derive(Debug)]
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
    #[inline]
    #[must_use]
    pub fn hash(&self, n: u64) -> u64 {
        self.inner.hash(n)
    }
    #[must_use]
    pub const fn code_len(&self) -> usize {
        self.inner.code_len()
    }
}

#[cfg(feature = "hashx")]
pub use hashx::{Error as HashXError, Runtime as HashXRuntime};

/// Upstream-compatible `HashX` v1 execution mode.
///
/// `HashX` deliberately delegates program generation and execution to the
/// maintained upstream implementation instead of duplicating its scheduler and
/// constraint model. With the `native` feature on x86-64 or `AArch64`,
/// [`HashX::compile`] requires the upstream native JIT; [`HashX::interpret`] is
/// always available as a portable reference path.
#[cfg(feature = "hashx")]
#[derive(Debug)]
pub struct HashX {
    inner: hashx::HashX,
}

#[cfg(feature = "hashx")]
impl HashX {
    /// Build an interpreter-only `HashX` instance.
    ///
    /// # Errors
    ///
    /// Returns [`HashXError`] when the seed cannot satisfy `HashX` program
    /// constraints.
    pub fn interpret(seed: &[u8]) -> Result<Self, HashXError> {
        Self::with_runtime(seed, hashx::RuntimeOption::InterpretOnly)
    }

    /// Build a `HashX` instance and require native code generation.
    ///
    /// # Errors
    ///
    /// Returns [`HashXError`] when the seed cannot satisfy `HashX` program
    /// constraints or native code generation fails.
    #[cfg(all(
        feature = "native",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    pub fn compile(seed: &[u8]) -> Result<Self, HashXError> {
        Self::with_runtime(seed, hashx::RuntimeOption::CompileOnly)
    }

    fn with_runtime(seed: &[u8], runtime: hashx::RuntimeOption) -> Result<Self, HashXError> {
        let mut builder = hashx::HashXBuilder::new();
        builder.runtime(runtime);
        Ok(Self {
            inner: builder.build(seed)?,
        })
    }

    /// Return the runtime selected for this `HashX` instance.
    #[must_use]
    pub fn runtime(&self) -> HashXRuntime {
        self.inner.runtime()
    }

    /// Calculate the first 64 bits of the `HashX` digest.
    #[inline]
    #[must_use]
    pub fn hash(&self, nonce: u64) -> u64 {
        self.inner.hash_to_u64(nonce)
    }

    /// Calculate the full 256-bit `HashX` digest.
    #[inline]
    #[must_use]
    pub fn hash_bytes(&self, nonce: u64) -> [u8; 32] {
        self.inner.hash_to_bytes(nonce)
    }
}
