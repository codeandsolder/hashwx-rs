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
#[cfg(all(feature = "native", target_arch = "x86_64"))]
pub struct HashWx {
    inner: backend::x86_64::Compiled,
}
#[cfg(all(feature = "native", target_arch = "x86_64"))]
impl HashWx {
    pub fn compile(seed: &[u8; 32]) -> Result<Self, CompileError> {
        Self::compile_program(&generate(seed))
    }
    pub fn compile_program(program: &ProgramSet) -> Result<Self, CompileError> {
        Ok(Self {
            inner: backend::x86_64::Compiled::compile(program)?,
        })
    }
    #[inline]
    pub fn hash(&self, n: u64) -> u64 {
        self.inner.hash(n)
    }
    pub fn code_len(&self) -> usize {
        self.inner.code_len()
    }
}
