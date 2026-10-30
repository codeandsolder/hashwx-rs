#[cfg(feature = "wasm")]
fn main() {
    for (name, s) in [
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
        seed[..s.len()].copy_from_slice(s);
        let t = std::time::Instant::now();
        let bytes = hashwx::wasm::compile(&seed);
        eprintln!(
            "{name} bytes={} build_us={:.3}",
            bytes.len(),
            t.elapsed().as_secs_f64() * 1e6
        );
        std::fs::write(name, bytes).unwrap();
    }
}
#[cfg(not(feature = "wasm"))]
fn main() {}
