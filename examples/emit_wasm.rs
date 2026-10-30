//! Emit generated `HashWX` WebAssembly modules for inspection.

#[cfg(feature = "wasm")]
fn main() -> std::io::Result<()> {
    for (name, seed_text) in [
        (
            "/tmp/hashwx-test1.wasm",
            b"This is a test seed for hashwx".as_slice(),
        ),
        (
            "/tmp/hashwx-test2.wasm",
            b"Lorem ipsum dolor sit amet".as_slice(),
        ),
    ] {
        let mut seed = [0u8; 32];
        seed[..seed_text.len()].copy_from_slice(seed_text);
        let t = std::time::Instant::now();
        let bytes = hashwx::wasm::compile(&seed);
        let byte_count = bytes.len();
        let build_us = t.elapsed().as_secs_f64() * 1e6;
        eprintln!("{name} bytes={byte_count} build_us={build_us:.3}");
        std::fs::write(name, bytes)?;
    }
    Ok(())
}

#[cfg(not(feature = "wasm"))]
fn main() {}
