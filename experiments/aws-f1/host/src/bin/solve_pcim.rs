//! Physical F1 DMA solver. Uses kernel-provided coherent PCI bus mappings,
//! 64-byte host rings and the same CPU-authoritative validation as BAR0 mode.
//! SPDX-License-Identifier: LGPL-3.0-only
use hashwx::{HashWx, HashX, HashXProgram, generate};
use hashwx_f1_fixtures::encoding::{encode_hashwx, encode_hashx};
use hashwx_f1_fixtures::linux::F1Device;
use hashwx_f1_fixtures::pcim::{self, Algorithm, Command, Completion, RING_SLOTS};
use hashwx_f1_fixtures::queue::QueueIo;
use std::collections::VecDeque;
use std::error::Error;
use std::time::{Duration, Instant};

enum Oracle {
    Wx(HashWx),
    X(HashX),
}
impl Oracle {
    fn digest(&self, nonce: u64) -> [u8; 32] {
        match self {
            Self::Wx(wx) => {
                let mut digest = [0u8; 32];
                digest[..8].copy_from_slice(&wx.hash(nonce).to_le_bytes());
                digest
            }
            Self::X(x) => x.hash_bytes(nonce),
        }
    }
}
fn number(arg: &str) -> Result<u64, Box<dyn Error>> {
    Ok(if let Some(hex) = arg.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)?
    } else {
        arg.parse()?
    })
}
fn seed_bytes(arg: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if !arg.len().is_multiple_of(2) {
        return Err("seed hex needs an even number of digits".into());
    }
    arg.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| {
            u8::from_str_radix(std::str::from_utf8(b)?, 16)
                .map_err(|error| -> Box<dyn Error> { error.into() })
        })
        .collect()
}
fn load(dev: &F1Device, algorithm: Algorithm, seed: &[u8]) -> Result<Oracle, Box<dyn Error>> {
    match algorithm {
        Algorithm::HashWx => {
            let seed: &[u8; 32] = seed
                .try_into()
                .map_err(|_| "HashWX seed must be exactly 32 bytes")?;
            let set = generate(seed);
            if dev.peek(0x600)? != 0x48585732 {
                return Err("wrong HashWX FPGA image marker".into());
            }
            for (i, w) in encode_hashwx(&set).into_iter().enumerate() {
                dev.poke(0x800 + i as u64 * 4, w)?;
            }
            Ok(Oracle::Wx(HashWx::interpret_program(&set)))
        }
        Algorithm::HashX => {
            let set = HashXProgram::generate(seed)?;
            if dev.peek(0x1000)? != 0x48585832 {
                return Err("wrong HashX FPGA image marker".into());
            }
            for (i, w) in encode_hashx(&set).into_iter().enumerate() {
                dev.poke(0x4000 + i as u64 * 4, w)?;
            }
            Ok(Oracle::X(HashX::interpret_program(&set)))
        }
    }
}
fn verify(
    completion: &Completion,
    command: &Command,
    oracle: &Oracle,
) -> Result<(), Box<dyn Error>> {
    if completion.tag != command.tag
        || completion.algorithm != command.algorithm
        || completion.attempts > command.count
        || (command.count != 0 && completion.attempts == 0)
    {
        return Err(format!("FPGA completion metadata mismatch: {completion:?}").into());
    }
    if !completion.found {
        if completion.attempts != command.count {
            return Err("no-hit completion must exhaust its range".into());
        }
        return Ok(());
    }
    if completion.nonce.wrapping_sub(command.first_nonce) >= u64::from(command.count) {
        return Err("FPGA winning nonce outside the submitted range".into());
    }
    let expected = oracle.digest(completion.nonce);
    let actual_low = u64::from_le_bytes(completion.digest[..8].try_into()?);
    if completion.digest != expected || actual_low >= command.exclusive_target {
        return Err(format!("invalid FPGA hash for nonce={:#018x}", completion.nonce).into());
    }
    Ok(())
}
fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 7 {
        return Err("usage: hashwx-f1-pcim </dev/hashwx-f1-BDF> <wx|x> <seed_hex> <start_nonce> <nonce_count> <exclusive_target>".into());
    }
    let algorithm = match args[2].as_str() {
        "wx" => Algorithm::HashWx,
        "x" => Algorithm::HashX,
        _ => return Err("algorithm must be wx or x".into()),
    };
    let seed = seed_bytes(&args[3])?;
    let mut next_nonce = number(&args[4])?;
    let mut remaining = number(&args[5])?;
    let target = number(&args[6])?;
    let dev = F1Device::open(&args[1])?;

    if dev.peek(0x2004)? != 0x5043494d || dev.peek(0x2000)? != 0 {
        return Err("wrong PCIM image or DMA engine already armed".into());
    }
    if dev.peek(0x61c)? & 1 != 0 || dev.peek(0x101c)? & 1 != 0 {
        return Err("scanner/queue must be drained before loading program".into());
    }
    let oracle = load(&dev, algorithm, &seed)?;
    if remaining == 0 {
        println!("NOT FOUND attempts=0");
        return Ok(());
    }

    let (desc_bus, result_bus) = dev.ring_addresses();
    pcim::arm(&dev, desc_bus, result_bus)?;
    let mut producer = 0u32;
    let mut acknowledged = 0u32;
    let mut next_tag = 0u32;
    let mut attempts = 0u64;
    let mut winner: Option<Completion> = None;
    let mut pending: VecDeque<Command> = VecDeque::new();
    let mut last_progress = Instant::now();

    // After arm, a failure requires device reset or successful drain.
    // The kernel keeps the coherent buffer allocated even after close.
    while remaining != 0 || !pending.is_empty() {
        let state = pcim::progress(&dev)?;
        if state.fault != 0 {
            return Err(format!("FPGA PCIM DMA fault code {}", state.fault).into());
        }
        if producer.wrapping_sub(state.consumed_commands) > RING_SLOTS
            || state.written_results.wrapping_sub(acknowledged) > RING_SLOTS
        {
            return Err("FPGA DMA counter invariant failed".into());
        }

        // Drain completions BEFORE posting more. The FPGA is free to
        // interleave algorithms, but this command stream uses only one.
        while acknowledged != state.written_results {
            let raw = dev.read_completion(acknowledged % RING_SLOTS)?;
            let completion = Completion::decode(&raw)?;
            let command = pending
                .pop_front()
                .ok_or("FPGA produced an unsubmitted completion")?;
            verify(&completion, &command, &oracle)?;
            attempts += u64::from(completion.attempts);
            if completion.found && winner.is_none() {
                winner = Some(completion);
                remaining = 0; // drain already submitted jobs
            }
            acknowledged = acknowledged.wrapping_add(1);
            pcim::acknowledge(&dev, acknowledged)?;
            last_progress = Instant::now();
        }

        let consumed = dev.peek(0x2020)?;
        while remaining != 0 && winner.is_none() && producer.wrapping_sub(consumed) < RING_SLOTS {
            let count = u32::try_from(remaining.min(16_384))?;
            let command = Command {
                tag: next_tag,
                algorithm,
                count,
                first_nonce: next_nonce,
                exclusive_target: target,
            };
            dev.write_descriptor(producer % RING_SLOTS, &command.encode())?;
            producer = producer.wrapping_add(1);
            pcim::publish(&dev, producer)?;
            pending.push_back(command);
            next_tag = next_tag.wrapping_add(1);
            next_nonce = next_nonce.wrapping_add(u64::from(count));
            remaining -= u64::from(count);
            last_progress = Instant::now();
        }
        if last_progress.elapsed() > Duration::from_secs(60) {
            return Err("FPGA DMA stalled for 60 seconds; inspect F1 PCIM errors".into());
        }
        std::thread::yield_now();
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = pcim::progress(&dev)?;
        if state.fault != 0 {
            return Err(format!("FPGA PCIM DMA fault while stopping: {}", state.fault).into());
        }
        if state.consumed_commands == producer
            && state.written_results == acknowledged
            && ((dev.peek(0x2028)? >> 5) & 0xf) == 1
        {
            dev.poke(0x2000, 0)?;
            if dev.peek(0x2000)? == 0 {
                break;
            }
            return Err("FPGA rejected PCIM shutdown".into());
        }
        if Instant::now() > deadline {
            return Err("FPGA did not return to idle after draining".into());
        }
        std::thread::yield_now();
    }

    if let Some(winner) = winner {
        println!(
            "FOUND nonce={:#018x} attempts={attempts} digest={:02x?}",
            winner.nonce, winner.digest
        );
    } else {
        println!("NOT FOUND attempts={attempts}");
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("F1 PCIM error: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_digest_target_and_wrapped_nonce_range() {
        let seed = [4u8; 32];
        let set = generate(&seed);
        let oracle = Oracle::Wx(HashWx::interpret_program(&set));
        let digest = oracle.digest(0);
        let value = u64::from_le_bytes(digest[..8].try_into().unwrap());
        let command = Command {
            tag: 99,
            algorithm: Algorithm::HashWx,
            count: 3,
            first_nonce: u64::MAX - 1,
            exclusive_target: value.saturating_add(1),
        };
        let completion = Completion {
            tag: 99,
            algorithm: Algorithm::HashWx,
            attempts: 3,
            found: true,
            nonce: 0,
            digest,
        };
        if value != u64::MAX {
            verify(&completion, &command, &oracle).unwrap();
        }
        let mut bad = completion;
        bad.tag += 1;
        assert!(verify(&bad, &command, &oracle).is_err());
    }
}
