//! Small native throughput/JIT construction benchmark.

#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use std::time::Instant;

#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
const WARMUP_HASHES: u32 = 30_000;
#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
const BENCH_HASHES: u32 = 300_000;

#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "hashwx".to_owned());
    match mode.as_str() {
        "hashwx" => bench_hashwx(),
        "hashx" => bench_hashx(),
        _ => Err(format!("unknown mode {mode:?}; expected hashwx or hashx").into()),
    }
}

#[cfg(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn bench_hashwx() -> Result<(), Box<dyn std::error::Error>> {
    let mut seed = [0u8; 32];
    for (value, byte) in (0u8..32).zip(&mut seed) {
        *byte = value;
    }
    for _ in 0..4 {
        let t = Instant::now();
        let h = hashwx::HashWx::compile(&seed)?;
        let build = t.elapsed();
        let check = h.hash(123_456);
        for i in 0..u64::from(WARMUP_HASHES) {
            std::hint::black_box(h.hash(i));
        }
        let mut sink = 0u64;
        let t = Instant::now();
        for i in 0..u64::from(BENCH_HASHES) {
            sink ^= h.hash(i);
        }
        let sec = t.elapsed().as_secs_f64();
        let build_us = build.as_secs_f64() * 1e6;
        let code_len = h.code_len().unwrap_or(0);
        let rate = f64::from(BENCH_HASHES) / sec;
        println!(
            "mode=hashwx build_us={build_us:.3} code={code_len} check={check:016x} rate={rate:.3} sink={sink:016x}"
        );
    }
    Ok(())
}

#[cfg(all(
    feature = "native",
    feature = "hashx",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn bench_hashx() -> Result<(), Box<dyn std::error::Error>> {
    let seed = b"This is a test\0";
    for _ in 0..4 {
        let t = Instant::now();
        let h = hashwx::HashX::compile(seed)?;
        let build = t.elapsed();
        let check = h.hash(123_456);
        for i in 0..u64::from(WARMUP_HASHES) {
            std::hint::black_box(h.hash(i));
        }
        let mut sink = 0u64;
        let t = Instant::now();
        for i in 0..u64::from(BENCH_HASHES) {
            sink ^= h.hash(i);
        }
        let sec = t.elapsed().as_secs_f64();
        let build_us = build.as_secs_f64() * 1e6;
        let rate = f64::from(BENCH_HASHES) / sec;
        println!(
            "mode=hashx build_us={build_us:.3} check={check:016x} rate={rate:.3} sink={sink:016x}"
        );
    }
    Ok(())
}

#[cfg(all(
    feature = "native",
    not(feature = "hashx"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn bench_hashx() -> Result<(), Box<dyn std::error::Error>> {
    Err("hashx benchmark mode requires the hashx feature".into())
}

#[cfg(not(all(
    feature = "native",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn main() {}
