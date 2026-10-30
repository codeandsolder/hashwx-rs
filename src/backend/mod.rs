#[cfg(all(feature = "native", target_arch = "aarch64"))]
pub(crate) mod aarch64;
#[cfg(all(feature = "hashx", feature = "native", target_arch = "aarch64"))]
pub(crate) mod hashx_aarch64;
#[cfg(all(feature = "hashx", feature = "native", target_arch = "x86_64"))]
pub mod hashx_x86_64;
#[cfg(all(feature = "native", target_arch = "x86_64"))]
pub mod x86_64;
