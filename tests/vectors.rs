//! Official `HashWX` vector and trait-contract tests.

fn seed(s: &[u8]) -> [u8; 32] {
    let mut x = [0_u8; 32];
    x[..s.len()].copy_from_slice(s);
    x
}

#[test]
fn portable_vectors() {
    let a = hashwx::HashWx::interpret(&seed(b"This is a test seed for hashwx"));
    assert_eq!(a.runtime(), hashwx::HashWxRuntime::Interpret);
    assert_eq!(a.code_len(), None);
    assert_eq!(a.hash(0), 0x9736_8417_6f8e_e362);
    assert_eq!(a.hash(123_456), 0x4019_83bb_07d6_9b07);

    let b = hashwx::HashWx::interpret(&seed(b"Lorem ipsum dolor sit amet"));
    assert_eq!(b.hash(123_456), 0x4af3_8d83_4a9a_8d3d);
    assert_eq!(b.hash(987_654_321_123_456_789), 0x6a8a_5514_432e_17a3);
}

#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn native_matches_portable_vectors() -> Result<(), hashwx::CompileError> {
    for (seed, nonces) in [
        (
            seed(b"This is a test seed for hashwx"),
            [0_u64, 123_456, 1, u64::MAX],
        ),
        (
            seed(b"Lorem ipsum dolor sit amet"),
            [123_456, 987_654_321_123_456_789, 0, u64::MAX],
        ),
    ] {
        let native = hashwx::HashWx::compile(&seed)?;
        let portable = hashwx::HashWx::interpret(&seed);
        assert_eq!(native.runtime(), hashwx::HashWxRuntime::Compiled);
        assert!(native.code_len().is_some());
        for nonce in nonces {
            assert_eq!(native.hash(nonce), portable.hash(nonce));
        }
    }
    Ok(())
}

#[test]
fn best_available_runtime_matches_explicit_engine() -> Result<(), hashwx::CompileError> {
    let seed = seed(b"This is a test seed for hashwx");
    let automatic = hashwx::HashWx::new(&seed)?;
    #[cfg(all(
        feature = "native",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    assert_eq!(automatic.runtime(), hashwx::HashWxRuntime::Compiled);
    #[cfg(not(all(
        feature = "native",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    assert_eq!(automatic.runtime(), hashwx::HashWxRuntime::Interpret);
    assert_eq!(automatic.hash(123_456), 0x4019_83bb_07d6_9b07);
    Ok(())
}

#[test]
fn send_sync() {
    fn yes<T: Send + Sync>() {}
    yes::<hashwx::HashWx>();
}
