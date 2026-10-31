//! `HashWX` 1.0 and `HashX` v1 program generation and execution.
//!
//! [`HashWx`] provides architecture-neutral Rust execution plus seed-specialized
//! native JIT execution on x86-64 and `AArch64`. [`ProgramSet`] exposes the generated
//! `HashWX` IR read-only for alternate backends, and the optional [`wasm`] module
//! emits portable WebAssembly for `HashWX` and, with `hashx`, `HashX`. [`HashX`] likewise
//! selects between its portable interpreter and native JIT.

mod backend;
mod portable;
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

/// Effective `HashWX` execution engine.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HashWxRuntime {
    /// Architecture-neutral Rust execution.
    Interpret,
    /// Seed-specialized native JIT.
    Compiled,
}

enum HashWxProgram {
    Interpret(Box<ProgramSet>),
    #[cfg(all(
        feature = "native",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    Compiled(native_backend::Compiled),
}

/// Seed-specialized `HashWX` function with portable and native execution paths.
pub struct HashWx {
    program: HashWxProgram,
}

impl HashWx {
    /// Build an architecture-neutral `HashWX` instance.
    #[must_use]
    pub fn interpret(seed: &[u8; SEED_SIZE]) -> Self {
        Self::interpret_program(&generate(seed))
    }

    /// Build an architecture-neutral instance from an already-generated program.
    #[must_use]
    pub fn interpret_program(program: &ProgramSet) -> Self {
        Self {
            program: HashWxProgram::Interpret(Box::new(program.clone())),
        }
    }

    /// Build the best execution engine available for this target.
    ///
    /// On x86-64 and `AArch64` with the `native` feature this uses the native JIT.
    /// Other targets use the portable Rust engine. Native compilation errors are
    /// returned rather than silently converted into interpreter execution.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] if a supported native JIT fails to compile.
    pub fn new(seed: &[u8; SEED_SIZE]) -> Result<Self, CompileError> {
        #[cfg(all(
            feature = "native",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ))]
        {
            Self::compile(seed)
        }
        #[cfg(not(all(
            feature = "native",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )))]
        {
            Ok(Self::interpret(seed))
        }
    }

    /// Compile a `HashWX` seed into native machine code.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] if the native assembler cannot be created or
    /// the emitted code cannot be finalized.
    #[cfg(all(
        feature = "native",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    pub fn compile(seed: &[u8; SEED_SIZE]) -> Result<Self, CompileError> {
        Self::compile_program(&generate(seed))
    }

    /// Compile an already-generated `HashWX` program into native machine code.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] if the native assembler cannot be created or
    /// the emitted code cannot be finalized.
    #[cfg(all(
        feature = "native",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    pub fn compile_program(program: &ProgramSet) -> Result<Self, CompileError> {
        Ok(Self {
            program: HashWxProgram::Compiled(native_backend::Compiled::compile(program)?),
        })
    }

    /// Return the active execution engine.
    #[must_use]
    pub const fn runtime(&self) -> HashWxRuntime {
        match &self.program {
            HashWxProgram::Interpret(_) => HashWxRuntime::Interpret,
            #[cfg(all(
                feature = "native",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ))]
            HashWxProgram::Compiled(_) => HashWxRuntime::Compiled,
        }
    }

    /// Hash one nonce with this `HashWX` instance.
    #[inline]
    #[must_use]
    pub fn hash(&self, nonce: u64) -> u64 {
        match &self.program {
            HashWxProgram::Interpret(program) => portable::hash(program, nonce),
            #[cfg(all(
                feature = "native",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ))]
            HashWxProgram::Compiled(program) => program.hash(nonce),
        }
    }

    /// Size of generated native code, or `None` for portable execution.
    #[must_use]
    pub const fn code_len(&self) -> Option<usize> {
        match &self.program {
            HashWxProgram::Interpret(_) => None,
            #[cfg(all(
                feature = "native",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ))]
            HashWxProgram::Compiled(program) => Some(program.code_len()),
        }
    }
}

#[cfg(feature = "hashx")]
pub(crate) mod hashx;
#[cfg(feature = "hashx")]
pub use hashx::{
    HashX, HashXError, HashXProgram, HashXRuntime, Instruction as HashXInstruction,
    NUM_INSTRUCTIONS as HASHX_PROGRAM_SIZE, RegisterId as HashXRegisterId,
};
