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
fn vectors() {
    let a = hashwx::HashWx::compile(&seed(b"This is a test seed for hashwx")).unwrap();
    assert_eq!(a.hash(0), 0x973684176f8ee362);
    assert_eq!(a.hash(123456), 0x401983bb07d69b07);
    let b = hashwx::HashWx::compile(&seed(b"Lorem ipsum dolor sit amet")).unwrap();
    assert_eq!(b.hash(123456), 0x4af38d834a9a8d3d);
    assert_eq!(b.hash(987654321123456789), 0x6a8a5514432e17a3);
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
