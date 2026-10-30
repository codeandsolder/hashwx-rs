//! Repo-owned `HashX` v1 program generation, reference execution, and state setup.
//!
//! The compatibility model is derived from the Tor Project's `hashx` 0.10.0
//! implementation (LGPL-3.0-only). Native execution is compiled by hashwx-rs's
//! own dynasm backends in `crate::backend`.

mod constraints;
mod generator;
mod program;
mod rand;
mod register;
mod scheduler;
mod siphash;

use crate::CompileError;
use rand::SipRand;
use register::RegisterFile;
use siphash::SipState;

#[cfg(all(feature = "native", target_arch = "aarch64"))]
use crate::backend::hashx_aarch64 as native_backend;
#[cfg(all(feature = "native", target_arch = "x86_64"))]
use crate::backend::hashx_x86_64 as native_backend;

/// `HashX` program construction or JIT compilation error.
#[derive(Debug)]
pub enum HashXError {
    /// This seed produces a program that fails `HashX`'s whole-program constraints.
    ProgramConstraints,
    /// Native code generation failed.
    Compile(CompileError),
}

impl core::fmt::Display for HashXError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ProgramConstraints => f.write_str("HashX program constraints rejected this seed"),
            Self::Compile(error) => write!(f, "HashX native compilation failed: {error}"),
        }
    }
}

impl std::error::Error for HashXError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ProgramConstraints => None,
            Self::Compile(error) => Some(error),
        }
    }
}

impl From<CompileError> for HashXError {
    fn from(error: CompileError) -> Self {
        Self::Compile(error)
    }
}

/// Effective `HashX` execution engine.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HashXRuntime {
    /// Portable Rust reference interpreter.
    Interpret,
    /// hashwx-rs native JIT.
    Compiled,
}

#[derive(Debug)]
enum RuntimeProgram {
    Interpret(program::Program),
    #[cfg(all(
        feature = "native",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    Compiled(native_backend::Compiled),
}

/// `HashX` v1 implemented by hashwx-rs's own generator and execution backends.
#[derive(Debug)]
pub struct HashX {
    register_key: SipState,
    program: RuntimeProgram,
}

impl HashX {
    /// Full `HashX` digest width in bytes.
    pub const FULL_SIZE: usize = 32;

    fn generate(seed: &[u8]) -> Result<(program::Program, SipState), HashXError> {
        let (program_key, register_key) = SipState::pair_from_seed(seed);
        let mut rng = SipRand::new(program_key);
        Ok((program::Program::generate(&mut rng)?, register_key))
    }

    /// Build an interpreter-only `HashX` instance.
    ///
    /// # Errors
    ///
    /// Returns [`HashXError::ProgramConstraints`] for a rejected seed.
    pub fn interpret(seed: &[u8]) -> Result<Self, HashXError> {
        let (program, register_key) = Self::generate(seed)?;
        Ok(Self {
            register_key,
            program: RuntimeProgram::Interpret(program),
        })
    }

    /// Build a `HashX` instance using hashwx-rs's native JIT.
    ///
    /// # Errors
    ///
    /// Returns an error for a rejected seed or if native code generation fails.
    #[cfg(all(
        feature = "native",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    pub fn compile(seed: &[u8]) -> Result<Self, HashXError> {
        let (program, register_key) = Self::generate(seed)?;
        #[cfg(target_arch = "x86_64")]
        let compiled = native_backend::Compiled::compile(&program, register_key.into())?;
        #[cfg(target_arch = "aarch64")]
        let compiled = native_backend::Compiled::compile(&program)?;
        Ok(Self {
            register_key,
            program: RuntimeProgram::Compiled(compiled),
        })
    }

    /// Return the active `HashX` execution engine.
    #[must_use]
    pub const fn runtime(&self) -> HashXRuntime {
        match self.program {
            RuntimeProgram::Interpret(_) => HashXRuntime::Interpret,
            #[cfg(all(
                feature = "native",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ))]
            RuntimeProgram::Compiled(_) => HashXRuntime::Compiled,
        }
    }

    fn hash_to_regs(&self, input: u64) -> RegisterFile {
        let mut regs = RegisterFile::new(self.register_key, input);
        match &self.program {
            RuntimeProgram::Interpret(program) => program.interpret(&mut regs),
            #[cfg(all(
                feature = "native",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ))]
            RuntimeProgram::Compiled(program) => program.invoke(&mut regs),
        }
        regs
    }

    /// Calculate the first 64 bits of the `HashX` digest.
    #[inline]
    #[must_use]
    pub fn hash(&self, input: u64) -> u64 {
        #[cfg(all(feature = "native", target_arch = "x86_64"))]
        if let RuntimeProgram::Compiled(program) = &self.program {
            return program.hash_word(input);
        }
        self.hash_to_regs(input).digest(self.register_key)[0]
    }

    /// Calculate the full 256-bit `HashX` digest.
    #[must_use]
    pub fn hash_bytes(&self, input: u64) -> [u8; Self::FULL_SIZE] {
        let words = self.hash_to_regs(input).digest(self.register_key);
        let mut bytes = [0_u8; Self::FULL_SIZE];
        for (chunk, word) in bytes.as_chunks_mut::<8>().0.iter_mut().zip(words) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }
}

#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub use program::{Instruction, Program};
#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub use register::{RegisterFile as NativeRegisterFile, RegisterId};
