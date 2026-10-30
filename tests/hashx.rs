//! HashX compatibility, vector, and native differential tests.

#![cfg(feature = "hashx")]

const SEED1: &[u8] = b"This is a test\0";
const SEED2: &[u8] = b"Lorem ipsum dolor sit amet\0";

fn differential_seed(index: u64) -> [u8; 32] {
    fn splitmix64(mut value: u64) -> u64 {
        value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    let mut seed = [0_u8; 32];
    for (word_index, chunk) in [0_u64, 1, 2, 3]
        .into_iter()
        .zip(seed.as_chunks_mut::<8>().0.iter_mut())
    {
        let word = splitmix64(index.wrapping_add(word_index * 0x0000_0100_0000_01b3));
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    seed
}

#[test]
fn hashx_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<hashwx::HashX>();
    assert_send_sync::<hashwx::HashXProgram>();
}

#[test]
fn official_vectors_interpreted() -> Result<(), hashwx::HashXError> {
    let first = hashwx::HashX::interpret(SEED1)?;
    assert_eq!(first.hash(0), 0x98ea_cb7d_5654_2f2b);
    assert_eq!(first.hash(123_456), 0xaf93_7ca6_0ad5_bdae);
    assert_eq!(
        first.hash_bytes(0),
        [
            0x2b, 0x2f, 0x54, 0x56, 0x7d, 0xcb, 0xea, 0x98, 0xfd, 0xb5, 0xd5, 0xe5, 0xce, 0x9a,
            0x65, 0x98, 0x3c, 0x4a, 0x4e, 0x35, 0xab, 0x14, 0x64, 0xb1, 0xef, 0xb6, 0x1e, 0x83,
            0xb7, 0x07, 0x4b, 0xb2,
        ]
    );

    let second = hashwx::HashX::interpret(SEED2)?;
    assert_eq!(second.hash(123_456), 0xaab0_bbf4_5b15_3dab);
    assert_eq!(second.hash(987_654_321_123_456_789), 0x7432_327c_49f0_fe8d);
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
            assert_eq!(native.hash(nonce), interpreted.hash(nonce));
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

#[test]
fn local_generator_and_interpreter_match_upstream() -> Result<(), String> {
    use hashx::{HashXBuilder as UpstreamBuilder, RuntimeOption};

    for index in 0_u64..256 {
        let seed = differential_seed(index);
        let generated = hashwx::HashXProgram::generate(&seed);
        let local = generated.as_ref().map(hashwx::HashX::interpret_program);
        let mut upstream_builder = UpstreamBuilder::new();
        upstream_builder.runtime(RuntimeOption::InterpretOnly);
        let upstream = upstream_builder.build(&seed);

        match (local, upstream) {
            (Ok(local), Ok(upstream)) => {
                #[cfg(all(
                    feature = "native",
                    any(target_arch = "x86_64", target_arch = "aarch64")
                ))]
                let native = {
                    let generated = generated.map_err(|error| error.to_string())?;
                    assert_eq!(generated.instructions().len(), hashwx::HASHX_PROGRAM_SIZE);
                    hashwx::HashX::compile_program(&generated).map_err(|error| error.to_string())?
                };

                for nonce in [0, 1, index, 123_456, 0x0123_4567_89ab_cdef, u64::MAX] {
                    let expected = upstream.hash_to_bytes(nonce);
                    assert_eq!(local.hash_bytes(nonce), expected);
                    #[cfg(all(
                        feature = "native",
                        any(target_arch = "x86_64", target_arch = "aarch64")
                    ))]
                    {
                        assert_eq!(
                            native.hash(nonce),
                            u64::from_le_bytes(
                                expected[..8]
                                    .try_into()
                                    .map_err(|_| "upstream HashX digest word has wrong size")?
                            )
                        );
                        assert_eq!(native.hash_bytes(nonce), expected);
                    }
                }
            }
            (
                Err(hashwx::HashXError::ProgramConstraints),
                Err(hashx::Error::ProgramConstraints),
            ) => {}
            (local, upstream) => {
                return Err(format!(
                    "local/upstream construction diverged for seed index {index}: local={local:?}, upstream={upstream:?}"
                ));
            }
        }
    }
    Ok(())
}
