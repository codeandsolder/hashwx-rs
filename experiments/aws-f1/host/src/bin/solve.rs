//! HashWX F1 proof-of-work search and candidate CPU verification.
use hashwx::{HashWx, generate};
use hashwx_f1_fixtures::aws::{Fpga, HASHWX_QUEUE, parse_number, parse_seed, scan_batches};
use hashwx_f1_fixtures::encoding::encode_hashwx;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 || args.len() > 6 {
        return Err("usage: hashwx-f1-solve <seed_hex_64> <start_nonce> <nonce_count> <exclusive_target> [slot=0]".into());
    }
    let seed = parse_seed(&args[1])?;
    let nonce = parse_number(&args[2])?;
    let count = parse_number(&args[3])?;
    let target = parse_number(&args[4])?;
    let slot = if args.len() == 6 {
        args[5].parse::<i32>()?
    } else {
        0
    };
    let set = generate(&seed);
    let oracle = HashWx::interpret_program(&set);
    let fpga = Fpga::attach(slot)?;
    if fpga.peek(0x600)? != 0x48585732 {
        return Err("wrong F1 AFI: HashWX v2 register ID is not HXW2".into());
    }
    for (index, word) in encode_hashwx(&set).into_iter().enumerate() {
        fpga.poke(0x800 + index as u64 * 4, word)?;
    }
    let (attempts, result) = scan_batches(&fpga, HASHWX_QUEUE, nonce, count, target, |r| {
        let hash = u64::from(r.digest[0]) | (u64::from(r.digest[1]) << 32);
        let verify = oracle.hash(r.winner);
        if verify != hash || verify >= target {
            return Err(format!(
                "invalid HashWX result nonce=0x{:016x}: FPGA={hash:016x} CPU={verify:016x}",
                r.winner
            )
            .into());
        }
        Ok(())
    })?;
    if let Some(hit) = result {
        let hash = u64::from(hit.digest[0]) | (u64::from(hit.digest[1]) << 32);
        println!(
            "FOUND nonce=0x{:016x} hash=0x{hash:016x} attempts={attempts}",
            hit.winner
        );
    } else {
        println!("NOT FOUND attempts={attempts}");
    }
    Ok(())
}
