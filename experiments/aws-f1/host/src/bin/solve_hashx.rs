//! HashX v1 F1 proof-of-work search and full 256-bit CPU verification.
use hashwx::{HashX, HashXProgram};
use hashwx_f1_fixtures::aws::{Fpga, HASHX_QUEUE, parse_hex_bytes, parse_number, scan_batches};
use hashwx_f1_fixtures::encoding::encode_hashx;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 5 || args.len() > 6 {
        return Err("usage: hashx-f1-solve <seed_hex_any_even_length> <start_nonce> <nonce_count> <exclusive_target> [slot=0]".into());
    }
    let seed = parse_hex_bytes(&args[1])?;
    let first = parse_number(&args[2])?;
    let count = parse_number(&args[3])?;
    let target = parse_number(&args[4])?;
    let slot = if args.len() == 6 { args[5].parse()? } else { 0 };
    let set = HashXProgram::generate(&seed)?;
    let cpu = HashX::interpret_program(&set);
    let fpga = Fpga::attach(slot)?;
    if fpga.peek(0x1000)? != 0x48585832 {
        return Err("wrong F1 AFI: HashX v2 HXX2 marker unavailable".into());
    }
    for (i, word) in encode_hashx(&set).into_iter().enumerate() {
        fpga.poke(0x4000 + (i as u64) * 4, word)?;
    }
    let (attempts, result) = scan_batches(&fpga, HASHX_QUEUE, first, count, target, |r| {
        let mut digest = [0u8; 32];
        for (i, word) in r.digest.iter().enumerate() {
            digest[4 * i..4 * i + 4].copy_from_slice(&word.to_le_bytes());
        }
        let expected = cpu.hash_bytes(r.winner);
        let low = u64::from_le_bytes(digest[..8].try_into()?);
        if expected != digest || low >= target {
            return Err(format!(
                "invalid HashX result nonce={:#018x}: FPGA digest differs from CPU or target",
                r.winner
            )
            .into());
        }
        Ok(())
    })?;
    if let Some(hit) = result {
        let low = u64::from(hit.digest[0]) | (u64::from(hit.digest[1]) << 32);
        println!(
            "FOUND nonce={:#018x} hash0={low:#018x} attempts={attempts}",
            hit.winner
        );
    } else {
        println!("NOT FOUND attempts={attempts}");
    }
    Ok(())
}
