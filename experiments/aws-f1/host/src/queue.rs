//! Generic F1 v2 BAR0 queue protocol; testable independently of AWS SDK.
use std::error::Error;

pub trait QueueIo {
    fn poke(&self, offset: u64, value: u32) -> Result<(), Box<dyn Error>>;
    fn peek(&self, offset: u64) -> Result<u32, Box<dyn Error>>;
    fn read64(&self, low: u64) -> Result<u64, Box<dyn Error>> {
        Ok(u64::from(self.peek(low)?) | (u64::from(self.peek(low + 4)?) << 32))
    }
    fn write64(&self, low: u64, value: u64) -> Result<(), Box<dyn Error>> {
        self.poke(low, value as u32)?;
        self.poke(low + 4, (value >> 32) as u32)
    }
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
pub fn scan_batches<B: QueueIo, F>(
    fpga: &B,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};

    struct FakeIo {
        registers: QueueRegisters,
        inner: RefCell<FakeState>,
    }

    #[derive(Default)]
    struct FakeState {
        registers: BTreeMap<u64, u32>,
        pending: VecDeque<(u32, u64, u32)>,
        completed: VecDeque<JobResult>,
        submitted: Vec<(u32, u64, u32)>,
        max_outstanding: usize,
        winning_nonce: Option<u64>,
        error: u32,
        corrupt_tag: bool,
    }

    impl FakeIo {
        fn new(registers: QueueRegisters, winning_nonce: Option<u64>) -> Self {
            let inner = FakeState {
                winning_nonce,
                ..FakeState::default()
            };
            Self {
                registers,
                inner: RefCell::new(inner),
            }
        }
    }

    impl QueueIo for FakeIo {
        fn poke(&self, offset: u64, value: u32) -> Result<(), Box<dyn Error>> {
            let mut s = self.inner.borrow_mut();
            let r = self.registers;
            if offset == r.control && value & 1 != 0 {
                if s.pending.len() >= 4 {
                    s.error |= 1;
                } else {
                    let nonce = u64::from(s.registers[&r.first_nonce])
                        | (u64::from(s.registers[&(r.first_nonce + 4)]) << 32);
                    let count = s.registers[&r.count];
                    let tag = s.registers[&r.tag_in];
                    s.pending.push_back((tag, nonce, count));
                    s.submitted.push((tag, nonce, count));
                    s.max_outstanding = s.max_outstanding.max(s.pending.len() + s.completed.len());
                }
            } else if offset == r.pop && value & 1 != 0 {
                assert!(s.completed.pop_front().is_some(), "pop of empty FIFO");
            } else {
                s.registers.insert(offset, value);
            }
            Ok(())
        }

        fn peek(&self, offset: u64) -> Result<u32, Box<dyn Error>> {
            let mut s = self.inner.borrow_mut();
            let r = self.registers;
            if offset == r.depth {
                return Ok(4);
            }
            if offset == r.error {
                return Ok(s.error);
            }
            if offset == r.status {
                if s.completed.len() < 4
                    && let Some((tag, nonce, count)) = s.pending.pop_front()
                {
                    let hit = s
                        .winning_nonce
                        .filter(|winner| winner.wrapping_sub(nonce) < u64::from(count));
                    let mut digest = [0; 8];
                    if let Some(winner) = hit {
                        for (i, word) in digest.iter_mut().enumerate() {
                            *word = (winner as u32).wrapping_add(i as u32);
                        }
                    }
                    let corrupt_tag = s.corrupt_tag;
                    s.completed.push_back(JobResult {
                        tag: tag.wrapping_add(u32::from(corrupt_tag)),
                        range_start: nonce,
                        range_count: count,
                        attempts: count,
                        found: hit.is_some(),
                        winner: hit.unwrap_or(0),
                        digest,
                    });
                }
                let front = s.completed.front();
                return Ok((u32::from(s.pending.len() < 4) << 3)
                    | (u32::from(front.is_some()) << 1)
                    | (u32::from(front.is_some_and(|j| j.found)) << 2)
                    | (s.error << 4));
            }
            let Some(front) = s.completed.front() else {
                return Ok(*s.registers.get(&offset).unwrap_or(&0));
            };
            if offset == r.tag_out {
                return Ok(front.tag);
            }
            if offset == r.attempts {
                return Ok(front.attempts);
            }
            if offset == r.winner_nonce {
                return Ok(front.winner as u32);
            }
            if offset == r.winner_nonce + 4 {
                return Ok((front.winner >> 32) as u32);
            }
            if offset >= r.digest && offset < r.digest + r.digest_words as u64 * 4 {
                return Ok(front.digest[((offset - r.digest) / 4) as usize]);
            }
            Ok(*s.registers.get(&offset).unwrap_or(&0))
        }
    }

    #[test]
    fn host_streams_64bit_ranges_and_preserves_wrapping_nonce() {
        for r in [HASHWX_QUEUE, HASHX_QUEUE] {
            let start = u64::MAX - 31;
            let winning_nonce = start.wrapping_add(2 * 16_384 + 5);
            let bus = FakeIo::new(r, Some(winning_nonce));
            let total = 16_384 * 9 + 7;
            let mut verified = 0;
            let (attempts, hit) = scan_batches(&bus, r, start, total, 0x1234, |job| {
                assert_eq!(job.winner, winning_nonce);
                for (i, word) in job.digest.iter().take(r.digest_words).enumerate() {
                    assert_eq!(*word, (winning_nonce as u32).wrapping_add(i as u32));
                }
                verified += 1;
                Ok(())
            })
            .unwrap();
            let hit = hit.unwrap();
            assert_eq!(hit.tag, 2);
            assert_eq!(hit.winner, winning_nonce);
            assert_eq!(verified, 1);
            let s = bus.inner.borrow();
            assert!(s.max_outstanding >= 2);
            assert!(s.submitted.len() >= 3 && s.submitted.len() < 9);
            assert_eq!(
                attempts,
                s.submitted
                    .iter()
                    .map(|(_, _, count)| u64::from(*count))
                    .sum::<u64>()
            );
            for (index, (tag, nonce, count)) in s.submitted.iter().enumerate() {
                assert_eq!(*tag, index as u32);
                assert_eq!(*nonce, start.wrapping_add(index as u64 * 16_384));
                assert_eq!(*count, 16_384);
            }
            assert!(s.pending.is_empty() && s.completed.is_empty());
        }
    }

    #[test]
    fn host_drains_all_chunks_when_no_nonce_qualifies() {
        for r in [HASHWX_QUEUE, HASHX_QUEUE] {
            let bus = FakeIo::new(r, None);
            let total = 16_384 * 10 + 17;
            let (attempts, hit) = scan_batches(&bus, r, 10, total, 0, |_| {
                panic!("no winning nonce is expected")
            })
            .unwrap();
            assert_eq!(attempts, total);
            assert!(hit.is_none());
            let s = bus.inner.borrow();
            assert_eq!(s.submitted.len(), 11);
            assert_eq!(s.submitted.last().unwrap().2, 17);
            assert!(s.completed.is_empty() && s.pending.is_empty());
        }
    }

    #[test]
    fn host_rejects_corrupt_tags_and_preexisting_protocol_errors() {
        let bus = FakeIo::new(HASHX_QUEUE, None);
        bus.inner.borrow_mut().corrupt_tag = true;
        assert!(scan_batches(&bus, HASHX_QUEUE, 0, 20, 0, |_| Ok(())).is_err());

        let bus = FakeIo::new(HASHWX_QUEUE, None);
        bus.inner.borrow_mut().error = 1;
        assert!(scan_batches(&bus, HASHWX_QUEUE, 0, 20, 0, |_| Ok(())).is_err());
    }

    #[test]
    fn zero_nonce_count_does_not_enqueue_any_work() {
        let bus = FakeIo::new(HASHX_QUEUE, None);
        let (attempts, result) =
            scan_batches(&bus, HASHX_QUEUE, u64::MAX, 0, 0, |_| Ok(())).unwrap();
        assert_eq!(attempts, 0);
        assert!(result.is_none());
        assert!(bus.inner.borrow().submitted.is_empty());
    }
}
