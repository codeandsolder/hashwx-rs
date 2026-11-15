//! AWS F1 SDK PCIe BAR0 wrapper; on target link against libfpga_mgmt.
use std::error::Error;
use std::ffi::c_int;

#[link(name = "fpga_mgmt")]
unsafe extern "C" {
    fn fpga_pci_init() -> c_int;
    fn fpga_pci_attach(slot: c_int, pf: c_int, bar: c_int, flags: u32, handle: *mut c_int)
    -> c_int;
    fn fpga_pci_detach(handle: c_int) -> c_int;
    fn fpga_pci_poke(handle: c_int, offset: u64, value: u32) -> c_int;
    fn fpga_pci_peek(handle: c_int, offset: u64, value: *mut u32) -> c_int;
}

pub struct Fpga(c_int);
impl Fpga {
    pub fn attach(slot: c_int) -> Result<Self, Box<dyn Error>> {
        // SAFETY: AWS C API, no raw data pointers except a valid stack handle.
        if unsafe { fpga_pci_init() } != 0 {
            return Err("fpga_pci_init failed".into());
        }
        let mut handle = -1;
        // App PF 0, BAR0 0, standard SDK flags 0.
        // SAFETY: handle is a valid mutable pointer to an initialized c_int.
        let rc = unsafe { fpga_pci_attach(slot, 0, 0, 0, &raw mut handle) };
        if rc != 0 {
            return Err(format!("fpga_pci_attach(slot={slot}) returned {rc}").into());
        }
        Ok(Self(handle))
    }
    pub fn poke(&self, offset: u64, value: u32) -> Result<(), Box<dyn Error>> {
        // SAFETY: valid attached handle, 32-bit MMIO register offset.
        let rc = unsafe { fpga_pci_poke(self.0, offset, value) };
        if rc != 0 {
            return Err(format!("poke at 0x{offset:x} failed: {rc}").into());
        }
        Ok(())
    }
    pub fn peek(&self, offset: u64) -> Result<u32, Box<dyn Error>> {
        let mut value = 0u32;
        // SAFETY: valid attached handle and stack 32-bit output pointer.
        let rc = unsafe { fpga_pci_peek(self.0, offset, &raw mut value) };
        if rc != 0 {
            return Err(format!("peek at 0x{offset:x} failed: {rc}").into());
        }
        Ok(value)
    }
    pub fn read64(&self, low: u64) -> Result<u64, Box<dyn Error>> {
        Ok(u64::from(self.peek(low)?) | (u64::from(self.peek(low + 4)?) << 32))
    }
    pub fn write64(&self, low: u64, x: u64) -> Result<(), Box<dyn Error>> {
        self.poke(low, x as u32)?;
        self.poke(low + 4, (x >> 32) as u32)
    }
}
impl Drop for Fpga {
    fn drop(&mut self) {
        // SAFETY: one successful attach, exactly one detach upon drop.
        let _ = unsafe { fpga_pci_detach(self.0) };
    }
}

pub fn parse_number(s: &str) -> Result<u64, Box<dyn Error>> {
    if let Some(hex) = s.strip_prefix("0x") {
        Ok(u64::from_str_radix(hex, 16)?)
    } else {
        Ok(s.parse()?)
    }
}
/// Decode any even-length hexadecimal seed, including the empty seed.
pub fn parse_hex_bytes(hex: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if !hex.len().is_multiple_of(2) {
        return Err("seed must contain an even number of hexadecimal digits".into());
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().as_chunks::<2>().0 {
        bytes.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
    }
    Ok(bytes)
}
pub fn parse_seed(hex: &str) -> Result<[u8; 32], Box<dyn Error>> {
    if hex.len() != 64 {
        return Err("seed must be 64 hex characters".into());
    }
    let mut seed = [0u8; 32];
    for (i, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    Ok(seed)
}

/// The F1 v2 queue ABI. One scanner runs at a time, but commands and
/// completed results are buffered independently so PCIe latency is amortized.
#[derive(Clone, Copy)]
pub struct QueueRegisters {
    pub control: u64,
    pub first_nonce: u64,
    pub target: u64,
    pub count: u64,
    pub status: u64,
    pub attempts: u64,
    pub winner_nonce: u64,
    pub digest: u64,
    pub digest_words: usize,
    pub tag_out: u64,
    pub pop: u64,
    pub tag_in: u64,
    pub depth: u64,
    pub error: u64,
}

pub const HASHWX_QUEUE: QueueRegisters = QueueRegisters {
    control: 0x604,
    first_nonce: 0x608,
    target: 0x610,
    count: 0x618,
    status: 0x61c,
    attempts: 0x620,
    winner_nonce: 0x624,
    digest: 0x62c,
    digest_words: 2,
    tag_out: 0x638,
    pop: 0x63c,
    tag_in: 0x644,
    depth: 0x648,
    error: 0x64c,
};

pub const HASHX_QUEUE: QueueRegisters = QueueRegisters {
    control: 0x1004,
    first_nonce: 0x1008,
    target: 0x1010,
    count: 0x1018,
    status: 0x101c,
    attempts: 0x1020,
    winner_nonce: 0x1024,
    digest: 0x102c,
    digest_words: 8,
    tag_out: 0x1050,
    pop: 0x1054,
    tag_in: 0x105c,
    depth: 0x1060,
    error: 0x1064,
};

pub struct JobResult {
    pub tag: u32,
    pub range_start: u64,
    pub range_count: u32,
    pub attempts: u32,
    pub found: bool,
    pub winner: u64,
    pub digest: [u32; 8],
}

/// Submit ranges in batches, ensuring command FIFO backpressure and ordered
/// completion FIFO draining. Each returned job is independently checked by
/// the caller against the authoritative CPU hash before acceptance.
///
/// Once a winner is found we stop enqueuing but drain already submitted jobs;
/// this means the FPGA is always idle and safe for subsequent program loads
/// when the function returns.
pub fn scan_batches<F>(
    fpga: &Fpga,
    regs: QueueRegisters,
    first_nonce: u64,
    total_count: u64,
    target: u64,
    mut verify: F,
) -> Result<(u64, Option<JobResult>), Box<dyn Error>>
where
    F: FnMut(&JobResult) -> Result<(), Box<dyn Error>>,
{
    use std::collections::VecDeque;

    // Individual queue commands are reasonably long compared with PCIe
    // traffic, but bounded so the device can honor early cancellation in
    // a future protocol revision without burning billions of hashes.
    const BATCH_SIZE: u64 = 16_384;
    let depth = u64::from(fpga.peek(regs.depth)?);
    if !(1..=65_535).contains(&depth) {
        return Err(format!("invalid FPGA queue depth {depth}").into());
    }
    let mut pending: VecDeque<(u32, u64, u32)> = VecDeque::new();
    let mut next_nonce = first_nonce;
    let mut remaining = total_count;
    let mut next_tag = 0u32;
    let mut total_attempts = 0u64;
    let mut first_hit = None;

    fpga.write64(regs.target, target)?;
    if fpga.peek(regs.error)? != 0 {
        return Err(
            "FPGA queue reports a previous protocol error; clear it before scanning".into(),
        );
    }
    while remaining != 0 || !pending.is_empty() {
        let status = fpga.peek(regs.status)?;
        if status & 0x30 != 0 {
            return Err(format!("FPGA queue protocol error status=0x{status:08x}").into());
        }

        // Queue multiple commands without pausing for each completed scan.
        if remaining != 0
            && first_hit.is_none()
            && pending.len() < usize::try_from(depth + 1)?
            && status & 8 != 0
        {
            let count = u32::try_from(remaining.min(BATCH_SIZE))?;
            fpga.write64(regs.first_nonce, next_nonce)?;
            fpga.poke(regs.count, count)?;
            fpga.poke(regs.tag_in, next_tag)?;
            fpga.poke(regs.control, 1)?;
            pending.push_back((next_tag, next_nonce, count));
            next_nonce = next_nonce.wrapping_add(u64::from(count));
            remaining -= u64::from(count);
            next_tag = next_tag.wrapping_add(1);
            continue;
        }

        if status & 2 != 0 {
            let (expected_tag, range_start, range_count) = pending
                .pop_front()
                .ok_or("FPGA completion without a submitted descriptor")?;
            let tag = fpga.peek(regs.tag_out)?;
            if tag != expected_tag {
                return Err(format!(
                    "out-of-order FPGA completion tag {tag}, expected {expected_tag}"
                )
                .into());
            }
            let attempts = fpga.peek(regs.attempts)?;
            if attempts > range_count || (range_count != 0 && attempts == 0) {
                return Err(format!("invalid FPGA attempts {attempts}/{range_count}").into());
            }
            let found = status & 4 != 0;
            let winner = if found {
                fpga.read64(regs.winner_nonce)?
            } else {
                0
            };
            let mut digest = [0u32; 8];
            if found {
                for (i, word) in digest.iter_mut().take(regs.digest_words).enumerate() {
                    *word = fpga.peek(regs.digest + u64::try_from(i)? * 4)?;
                }
            }
            let result = JobResult {
                tag,
                range_start,
                range_count,
                attempts,
                found,
                winner,
                digest,
            };
            if found {
                if winner.wrapping_sub(range_start) >= u64::from(range_count) {
                    return Err("FPGA winner outside submitted nonce range".into());
                }
                verify(&result)?;
                if first_hit.is_none() {
                    first_hit = Some(result);
                    remaining = 0; // no further queued work; drain submissions
                }
            }
            total_attempts += u64::from(attempts);
            fpga.poke(regs.pop, 1)?;
        } else if !pending.is_empty() {
            std::thread::yield_now();
        } else if remaining != 0 {
            return Err("queue refused new work while no commands are outstanding".into());
        }
    }
    Ok((total_attempts, first_hit))
}
