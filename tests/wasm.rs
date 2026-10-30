#[cfg(feature = "wasm")]
#[test]
fn module_valid() -> Result<(), wasmparser::BinaryReaderError> {
    let mut seed = [0u8; 32];
    let s = b"This is a test seed for hashwx";
    seed[..s.len()].copy_from_slice(s);
    let bytes = hashwx::wasm::compile(&seed);
    wasmparser::validate(&bytes)?;
    assert!(bytes.len() < 20_000);
    Ok(())
}
