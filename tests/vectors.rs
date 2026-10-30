#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn seed(s: &[u8]) -> [u8; 32] {
    let mut x = [0u8; 32];
    x[..s.len()].copy_from_slice(s);
    x
}
#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn vectors() -> Result<(), hashwx::CompileError> {
    let a = hashwx::HashWx::compile(&seed(b"This is a test seed for hashwx"))?;
    assert_eq!(a.hash(0), 0x9736_8417_6f8e_e362);
    assert_eq!(a.hash(123_456), 0x4019_83bb_07d6_9b07);
    let b = hashwx::HashWx::compile(&seed(b"Lorem ipsum dolor sit amet"))?;
    assert_eq!(b.hash(123_456), 0x4af3_8d83_4a9a_8d3d);
    assert_eq!(b.hash(987_654_321_123_456_789), 0x6a8a_5514_432e_17a3);
    Ok(())
}
#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn send_sync() {
    fn yes<T: Send + Sync>() {}
    yes::<hashwx::HashWx>();
}
