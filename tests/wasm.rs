//! Runtime validation for generated `HashWX` WebAssembly.

#![cfg(feature = "wasm")]

use wasmi::{Engine, Linker, Module, Store};

fn seed(value: &[u8]) -> [u8; hashwx::SEED_SIZE] {
    let mut seed = [0_u8; hashwx::SEED_SIZE];
    seed[..value.len()].copy_from_slice(value);
    seed
}

fn run_hash(module_bytes: &[u8], input: u64) -> Result<u64, wasmi::Error> {
    let engine = Engine::default();
    let module = Module::new(&engine, module_bytes)?;
    let mut store = Store::new(&engine, ());
    let linker = Linker::<()>::new(&engine);
    let instance = linker.instantiate_and_start(&mut store, &module)?;
    let hash = instance.get_typed_func::<i64, i64>(&store, "hash")?;
    let result = hash.call(&mut store, i64::from_ne_bytes(input.to_ne_bytes()))?;
    Ok(u64::from_ne_bytes(result.to_ne_bytes()))
}

#[test]
fn generated_module_matches_official_vectors() -> Result<(), wasmi::Error> {
    let first = hashwx::wasm::compile(&seed(b"This is a test seed for hashwx"));
    assert!(first.len() < 20_000);
    assert_eq!(run_hash(&first, 0)?, 0x9736_8417_6f8e_e362);
    assert_eq!(run_hash(&first, 123_456)?, 0x4019_83bb_07d6_9b07);

    let second = hashwx::wasm::compile(&seed(b"Lorem ipsum dolor sit amet"));
    assert_eq!(run_hash(&second, 123_456)?, 0x4af3_8d83_4a9a_8d3d);
    assert_eq!(
        run_hash(&second, 987_654_321_123_456_789)?,
        0x6a8a_5514_432e_17a3
    );
    Ok(())
}
