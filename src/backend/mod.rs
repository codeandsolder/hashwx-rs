#[cfg(all(feature = "native", target_arch = "aarch64"))]
pub(crate) mod aarch64;
#[cfg(all(feature = "native", target_arch = "x86_64"))]
pub mod x86_64;
