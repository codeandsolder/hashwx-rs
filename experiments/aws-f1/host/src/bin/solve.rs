//! HashWX F1 proof-of-work search and candidate CPU verification.
use hashwx::{HashWx, generate};
use hashwx_f1_fixtures::aws::{Fpga, parse_number, parse_seed};
use hashwx_f1_fixtures::encoding::encode_hashwx;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 || args.len() > 6 {
        return Err("usage: hashwx-f1-solve <seed_hex_64> <start_nonce> <nonce_count> <exclusive_target> [slot=0]".into());
    }
    let seed = parse_seed(&args[1])?;
    let nonce = parse_number(&args[2])?;
    let count = u32::try_from(parse_number(&args[3])?)?;
    let target = parse_number(&args[4])?;
    let slot = if args.len() == 6 {
        args[5].parse::<i32>()?
    } else {
        0
    };
    let set = generate(&seed);
    let oracle = HashWx::interpret_program(&set);
    let fpga = Fpga::attach(slot)?;
    if fpga.peek(0x600)? != 0x48585731 {
        return Err("wrong F1 AFI: HashWX register ID is not HXW1".into());
    }
    for (index, word) in encode_hashwx(&set).into_iter().enumerate() {
        fpga.poke(0x800 + index as u64 * 4, word)?;
    }
    fpga.write64(0x608, nonce)?;
    fpga.write64(0x610, target)?;
    fpga.poke(0x618, count)?;
    fpga.poke(0x604, 1)?;
    // Unbounded polling on an actual device is intentional; watchdog handled
    // by the caller, not a guessed time limit tied to unknown fabric clocks.
    loop {
        let state = fpga.peek(0x61c)?;
        if state & 2 != 0 {
            let attempts = fpga.peek(0x620)?;
            if state & 4 != 0 {
                let winner_nonce = fpga.read64(0x624)?;
                let winner_hash = fpga.read64(0x62c)?;
                let verify = oracle.hash(winner_nonce);
                if verify != winner_hash
                    || verify >= target
                    || (winner_nonce.wrapping_sub(nonce) >= u64::from(count))
                {
                    return Err(format!("FPGA returned invalid candidate: nonce={winner_nonce:x} actual={winner_hash:x} CPU={verify:x}").into());
                }
                println!(
                    "FOUND nonce=0x{winner_nonce:016x} hash=0x{winner_hash:016x} attempts={attempts}"
                );
            } else {
                println!("NOT FOUND attempts={attempts}");
            }
            break;
        }
        std::thread::yield_now();
    }
    Ok(())
}
