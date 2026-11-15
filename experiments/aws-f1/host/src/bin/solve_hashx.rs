//! HashX v1 F1 proof-of-work search and full 256-bit CPU verification.
use hashwx::{HashX, HashXProgram};
use hashwx_f1_fixtures::aws::{Fpga, parse_hex_bytes, parse_number};
use hashwx_f1_fixtures::encoding::encode_hashx;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 5 || args.len() > 6 {
        return Err("usage: hashx-f1-solve <seed_hex_any_even_length> <start_nonce> <nonce_count> <exclusive_target> [slot=0]".into());
    }
    let seed = parse_hex_bytes(&args[1])?;
    let first = parse_number(&args[2])?;
    let count = u32::try_from(parse_number(&args[3])?)?;
    let target = parse_number(&args[4])?;
    let slot = if args.len() == 6 { args[5].parse()? } else { 0 };
    let set = HashXProgram::generate(&seed)?;
    let cpu = HashX::interpret_program(&set);
    let fpga = Fpga::attach(slot)?;
    if fpga.peek(0x1000)? != 0x48585831 {
        return Err("wrong F1 AFI: HashX HXX1 marker unavailable".into());
    }
    for (i, word) in encode_hashx(&set).into_iter().enumerate() {
        fpga.poke(0x4000 + (i as u64) * 4, word)?;
    }
    fpga.write64(0x1008, first)?;
    fpga.write64(0x1010, target)?;
    fpga.poke(0x1018, count)?;
    fpga.poke(0x1004, 1)?;
    loop {
        let status = fpga.peek(0x101c)?;
        if status & 2 != 0 {
            let attempts = fpga.peek(0x1020)?;
            if status & 4 != 0 {
                let winner = fpga.read64(0x1024)?;
                let mut digest = [0u8; 32];
                for i in 0..8 {
                    digest[4 * i..4 * i + 4]
                        .copy_from_slice(&fpga.peek(0x102c + i as u64 * 4)?.to_le_bytes());
                }
                let expected = cpu.hash_bytes(winner);
                let result = u64::from_le_bytes(digest[0..8].try_into()?);
                if winner.wrapping_sub(first) >= u64::from(count)
                    || expected != digest
                    || result >= target
                {
                    return Err(format!("invalid HashX result nonce={winner:#018x}: FPGA digest differs from CPU or target").into());
                }
                println!("FOUND nonce={winner:#018x} hash0={result:#018x} attempts={attempts}");
            } else {
                println!("NOT FOUND attempts={attempts}");
            }
            break;
        }
        std::thread::yield_now();
    }
    Ok(())
}
