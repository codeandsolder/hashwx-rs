//! Single-thread benchmark matching tevador's canonical C benchmark workloads.

use std::{hint::black_box, time::Instant};

const HASHWX_DEFAULT_SEEDS: u32 = 3_000;
const HASHWX_DEFAULT_NONCES: u32 = 463;
const HASHX_DEFAULT_SEEDS: u32 = 500;
const HASHX_DEFAULT_NONCES: u32 = 65_536;

const HASHWX_WORKER_KEY: [u64; 2] = [0xb443_266e_0c61_253a, 0x85cf_eef0_bcbd_b1e9];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().ok_or("expected mode: hashwx or hashx")?;
    let defaults = match mode.as_str() {
        "hashwx" => (HASHWX_DEFAULT_SEEDS, HASHWX_DEFAULT_NONCES),
        "hashx" => (HASHX_DEFAULT_SEEDS, HASHX_DEFAULT_NONCES),
        _ => return Err(format!("unknown mode {mode:?}; expected hashwx or hashx").into()),
    };
    let mut seeds = defaults.0;
    let mut nonces = defaults.1;
    let mut start = 0_u32;

    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--seeds" => seeds = value.parse()?,
            "--nonces" => nonces = value.parse()?,
            "--start" => start = value.parse()?,
            _ => return Err(format!("unknown argument {flag:?}").into()),
        }
    }

    if mode == "hashwx" {
        bench_hashwx(start, seeds, nonces)?;
    } else {
        bench_hashx(start, seeds, nonces)?;
    }
    Ok(())
}

fn bench_hashwx(start: u32, seeds: u32, nonces: u32) -> Result<(), Box<dyn std::error::Error>> {
    let mut total_hashes = 0_u32;
    let mut sink = 0_u64;
    let started = Instant::now();
    for salt in start..start.saturating_add(seeds) {
        let seed = canonical_hashwx_seed(u64::from(salt));
        let hash = hashwx::HashWx::compile(&seed)?;
        for nonce in 0..u64::from(nonces) {
            sink ^= black_box(hash.hash(nonce));
        }
        total_hashes = total_hashes
            .checked_add(nonces)
            .ok_or("hash count overflow")?;
    }
    report(
        "hashwx",
        total_hashes,
        started.elapsed().as_secs_f64(),
        sink,
    );
    Ok(())
}

fn bench_hashx(start: u32, seeds: u32, nonces: u32) -> Result<(), Box<dyn std::error::Error>> {
    let mut total_hashes = 0_u32;
    let mut sink = 0_u64;
    let started = Instant::now();
    for seed_index in start..start.saturating_add(seeds) {
        let seed = seed_index.to_ne_bytes();
        let hash = match hashwx::HashX::compile(&seed) {
            Ok(hash) => hash,
            Err(hashwx::HashXError::ProgramConstraints) => continue,
            Err(error) => return Err(Box::new(error)),
        };
        for nonce in 0..u64::from(nonces) {
            let digest = black_box(hash.hash_bytes(nonce));
            sink ^= u64::from_le_bytes(digest[..8].try_into()?);
        }
        total_hashes = total_hashes
            .checked_add(nonces)
            .ok_or("hash count overflow")?;
    }
    report("hashx", total_hashes, started.elapsed().as_secs_f64(), sink);
    Ok(())
}

fn report(mode: &str, hashes: u32, seconds: f64, sink: u64) {
    let rate = f64::from(hashes) / seconds;
    println!("mode={mode} total_hashes={hashes} hashes_per_sec={rate:.6} sink={sink:016x}");
}

fn canonical_hashwx_seed(salt: u64) -> [u8; 32] {
    let [k0, k1] = HASHWX_WORKER_KEY;
    let mut state = [
        0x736f_6d65_7073_6575 ^ k0,
        0x646f_7261_6e64_6f6d ^ k1,
        0x6c79_6765_6e65_7261 ^ k0,
        0x7465_6462_7974_6573 ^ k1,
    ];
    state[3] ^= salt;
    sip_round(&mut state);
    state[0] ^= salt;
    state[2] ^= 0xbb;
    for _ in 0..3 {
        sip_round(&mut state);
    }

    let mut seed = [0_u8; 32];
    for (word, bytes) in state
        .into_iter()
        .zip(seed.as_chunks_mut::<8>().0.iter_mut())
    {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    seed
}

fn sip_round(v: &mut [u64; 4]) {
    v[0] = v[0].wrapping_add(v[1]);
    v[2] = v[2].wrapping_add(v[3]);
    v[1] = v[1].rotate_left(13);
    v[3] = v[3].rotate_left(16);
    v[1] ^= v[0];
    v[3] ^= v[2];
    v[0] = v[0].rotate_left(32);
    v[2] = v[2].wrapping_add(v[1]);
    v[0] = v[0].wrapping_add(v[3]);
    v[1] = v[1].rotate_left(17);
    v[3] = v[3].rotate_left(21);
    v[1] ^= v[2];
    v[3] ^= v[0];
    v[2] = v[2].rotate_left(32);
}
