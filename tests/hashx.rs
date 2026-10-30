#![cfg(feature = "hashx")]

const SEED1: &[u8] = b"This is a test\0";
const SEED2: &[u8] = b"Lorem ipsum dolor sit amet\0";

#[test]
fn upstream_vectors_interpreted() -> Result<(), hashwx::HashXError> {
    let first = hashwx::HashX::interpret(SEED1)?;
    assert_eq!(first.hash(0), 0x98eacb7d56542f2b);
    assert_eq!(first.hash(123_456), 0xaf937ca60ad5bdae);
    assert_eq!(
        first.hash_bytes(0),
        [
            0x2b, 0x2f, 0x54, 0x56, 0x7d, 0xcb, 0xea, 0x98, 0xfd, 0xb5, 0xd5, 0xe5, 0xce, 0x9a,
            0x65, 0x98, 0x3c, 0x4a, 0x4e, 0x35, 0xab, 0x14, 0x64, 0xb1, 0xef, 0xb6, 0x1e, 0x83,
            0xb7, 0x07, 0x4b, 0xb2,
        ]
    );

    let second = hashwx::HashX::interpret(SEED2)?;
    assert_eq!(second.hash(123_456), 0xaab0bbf45b153dab);
    assert_eq!(second.hash(987_654_321_123_456_789), 0x7432327c49f0fe8d);
    Ok(())
}

#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn native_matches_interpreter() -> Result<(), hashwx::HashXError> {
    for seed in [SEED1, SEED2] {
        let native = hashwx::HashX::compile(seed)?;
        let interpreted = hashwx::HashX::interpret(seed)?;
        assert_eq!(native.runtime(), hashwx::HashXRuntime::Compiled);
        for nonce in [0, 1, 123_456, 987_654_321_123_456_789] {
            assert_eq!(native.hash_bytes(nonce), interpreted.hash_bytes(nonce));
        }
    }
    Ok(())
}

#[test]
fn reports_rejected_seeds() {
    assert!(hashwx::HashX::interpret(b"\xf8\x05\x00\x00").is_ok());
    assert!(matches!(
        hashwx::HashX::interpret(b"\xf9\x05\x00\x00"),
        Err(hashwx::HashXError::ProgramConstraints)
    ));
    assert!(matches!(
        hashwx::HashX::interpret(b"\x5d\x93\x02\x00"),
        Err(hashwx::HashXError::ProgramConstraints)
    ));
    assert!(hashwx::HashX::interpret(b"\x5e\x93\x02\x00").is_ok());
}
