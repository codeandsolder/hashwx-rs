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
mod hashx;
#[cfg(feature = "hashx")]
pub use hashx::{HashX, HashXError, HashXRuntime};
