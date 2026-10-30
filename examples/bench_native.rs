#[cfg(feature = "native")]
use std::time::Instant;

#[cfg(feature = "native")]
fn main() {
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() {
        *b = i as u8;
    }
    for _ in 0..4 {
        let t = Instant::now();
        let h = hashwx::HashWx::compile(&seed).unwrap();
        let build = t.elapsed();
        let check = h.hash(123456);
        for i in 0..30000 {
            std::hint::black_box(h.hash(i));
        }
        const N: u64 = 300000;
        let mut sink = 0u64;
        let t = Instant::now();
        for i in 0..N {
            sink ^= h.hash(i)
        }
        let sec = t.elapsed().as_secs_f64();
        println!(
            "build_us={:.3} code={} check={:016x} rate={:.3} sink={:016x}",
            build.as_secs_f64() * 1e6,
            h.code_len(),
            check,
            N as f64 / sec,
            sink
        );
    }
}

#[cfg(not(feature = "native"))]
fn main() {}
