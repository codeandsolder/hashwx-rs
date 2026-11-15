//! Wire-format and control protocol for the FPGA-initiated F1 PCIM DMA rings.
//!
//! The caller owns TWO distinct, physically contiguous, DMA-accessible,
//! coherent and pinned 1024-byte host-memory regions. Virtual pointers or
//! ordinary heap allocations MUST NOT be supplied as physical addresses.
//! Provisioning these buffers on a physical F1 host is not yet implemented.
//! This file implements the protocol and its testable mechanics, not a
//! pretend userspace-to-physical-address conversion.
use crate::queue::QueueIo;
use std::error::Error;
use std::sync::atomic::{Ordering, fence};

pub const RING_SLOTS: u32 = 16;
pub const SLOT_BYTES: usize = 64;
pub const RING_BYTES: u64 = RING_SLOTS as u64 * SLOT_BYTES as u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Algorithm {
    HashWx = 0,
    HashX = 1,
}

impl TryFrom<u32> for Algorithm {
    type Error = &'static str;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::HashWx),
            1 => Ok(Self::HashX),
            _ => Err("invalid DMA algorithm"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub tag: u32,
    pub algorithm: Algorithm,
    pub count: u32,
    pub first_nonce: u64,
    pub exclusive_target: u64,
}

impl Command {
    pub fn encode(&self) -> [u8; SLOT_BYTES] {
        let mut word = [0_u8; SLOT_BYTES];
        word[0..4].copy_from_slice(&self.tag.to_le_bytes());
        word[4..8].copy_from_slice(&(self.algorithm as u32).to_le_bytes());
        word[8..12].copy_from_slice(&self.count.to_le_bytes());
        word[16..24].copy_from_slice(&self.first_nonce.to_le_bytes());
        word[24..32].copy_from_slice(&self.exclusive_target.to_le_bytes());
        word
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub tag: u32,
    pub algorithm: Algorithm,
    pub attempts: u32,
    pub found: bool,
    pub nonce: u64,
    pub digest: [u8; 32],
}

impl Completion {
    pub fn decode(raw: &[u8; SLOT_BYTES]) -> Result<Self, Box<dyn Error>> {
        fn le32(buf: &[u8]) -> u32 {
            u32::from_le_bytes(buf.try_into().expect("slice of exactly four bytes"))
        }
        fn le64(buf: &[u8]) -> u64 {
            u64::from_le_bytes(buf.try_into().expect("slice of exactly eight bytes"))
        }
        if le32(&raw[12..16]) > 1 {
            return Err("noncanonical DMA found flag".into());
        }
        if raw[24..32] != [0; 8] {
            return Err("nonzero reserved DMA completion bytes".into());
        }
        let algorithm = Algorithm::try_from(le32(&raw[4..8]))?;
        let found = le32(&raw[12..16]) == 1;
        let nonce = le64(&raw[16..24]);
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&raw[32..64]);
        if algorithm == Algorithm::HashWx && digest[8..] != [0; 24] {
            return Err("HashWX DMA digest padding must be zero".into());
        }
        if !found && (nonce != 0 || digest != [0; 32]) {
            return Err("no-hit DMA completion contains a winner".into());
        }
        Ok(Self {
            tag: le32(&raw[0..4]),
            algorithm,
            attempts: le32(&raw[8..12]),
            found,
            nonce,
            digest,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub consumed_commands: u32,
    pub written_results: u32,
    pub fault: u32,
}

fn check_rings(desc_phys: u64, result_phys: u64) -> Result<(), Box<dyn Error>> {
    let upper = |addr: u64| addr.checked_add(RING_BYTES);
    if desc_phys == 0
        || result_phys == 0
        || !desc_phys.is_multiple_of(SLOT_BYTES as u64)
        || !result_phys.is_multiple_of(SLOT_BYTES as u64)
        || upper(desc_phys).is_none()
        || upper(result_phys).is_none()
    {
        return Err("DMA ring bases must be nonzero, 64B aligned, without address wrap".into());
    }
    if desc_phys < upper(result_phys).expect("already checked")
        && result_phys < upper(desc_phys).expect("already checked")
    {
        return Err("DMA descriptor and completion rings overlap".into());
    }
    Ok(())
}

/// Initialize and arm the DMA engine. Does not allocate or pin memory.
pub fn arm<B: QueueIo>(io: &B, desc_phys: u64, result_phys: u64) -> Result<(), Box<dyn Error>> {
    check_rings(desc_phys, result_phys)?;
    if io.peek(0x2030)? != RING_SLOTS || io.peek(0x2000)? != 0 {
        return Err("wrong DMA engine configuration or engine already active".into());
    }
    io.write64(0x2008, desc_phys)?;
    io.write64(0x2010, result_phys)?;
    io.poke(0x2000, 1)?;
    if io.peek(0x2000)? != 1 || io.peek(0x202c)? != 0 {
        return Err("FPGA rejected DMA engine arm".into());
    }
    Ok(())
}

/// Publish a producer counter AFTER populating and flushing descriptor slots.
/// Uses a CPU release fence. DMA memory itself must be coherent and pinned.
pub fn publish<B: QueueIo>(io: &B, producer: u32) -> Result<(), Box<dyn Error>> {
    let consumed = io.peek(0x2020)?;
    if producer.wrapping_sub(consumed) > RING_SLOTS {
        return Err("DMA command ring overrun".into());
    }
    fence(Ordering::Release);
    io.poke(0x2018, producer)?;
    Ok(())
}

/// Acquire the counters AFTER the device reports completed writes.
pub fn progress<B: QueueIo>(io: &B) -> Result<Progress, Box<dyn Error>> {
    let readback = Progress {
        consumed_commands: io.peek(0x2020)?,
        written_results: io.peek(0x2024)?,
        fault: io.peek(0x202c)?,
    };
    fence(Ordering::Acquire);
    Ok(readback)
}

/// Acknowledge completions only AFTER fully reading and verifying their
/// memory slots. The host controls how much space is released to the device.
pub fn acknowledge<B: QueueIo>(io: &B, ack: u32) -> Result<(), Box<dyn Error>> {
    let old = io.peek(0x201c)?;
    let written = io.peek(0x2024)?;
    if ack.wrapping_sub(old) > written.wrapping_sub(old) {
        return Err("DMA completion acknowledgment exceeds produced results".into());
    }
    fence(Ordering::Release);
    io.poke(0x201c, ack)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Mock {
        regs: RefCell<BTreeMap<u64, u32>>,
    }
    impl QueueIo for Mock {
        fn poke(&self, offset: u64, value: u32) -> Result<(), Box<dyn Error>> {
            self.regs.borrow_mut().insert(offset, value);
            Ok(())
        }
        fn peek(&self, offset: u64) -> Result<u32, Box<dyn Error>> {
            Ok(*self.regs.borrow().get(&offset).unwrap_or(&0))
        }
    }

    #[test]
    fn descriptor_wire_format_matches_fpga_512bit_beat() {
        let c = Command {
            tag: 0x1020_3040,
            algorithm: Algorithm::HashX,
            count: 0xfedc_ba98,
            first_nonce: 0x1122_3344_5566_7788,
            exclusive_target: 0x8877_6655_4433_2211,
        };
        let word = c.encode();
        assert_eq!(&word[..8], &[0x40, 0x30, 0x20, 0x10, 1, 0, 0, 0]);
        assert_eq!(&word[8..12], &0xfedc_ba98_u32.to_le_bytes());
        assert_eq!(&word[12..16], &[0; 4]);
        assert_eq!(&word[16..24], &c.first_nonce.to_le_bytes());
        assert_eq!(&word[24..32], &c.exclusive_target.to_le_bytes());
        assert_eq!(&word[32..], &[0; 32]);
    }

    #[test]
    fn complete_digest_roundtrip_and_reject_malformed_completion() {
        let mut data = [0u8; SLOT_BYTES];
        data[..4].copy_from_slice(&7u32.to_le_bytes());
        data[4..8].copy_from_slice(&1u32.to_le_bytes());
        data[8..12].copy_from_slice(&16u32.to_le_bytes());
        data[12..16].copy_from_slice(&1u32.to_le_bytes());
        data[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
        for (i, b) in data[32..].iter_mut().enumerate() {
            *b = i as u8;
        }
        let c = Completion::decode(&data).unwrap();
        assert_eq!(c.algorithm, Algorithm::HashX);
        assert_eq!(c.tag, 7);
        assert_eq!(c.nonce, u64::MAX);
        assert_eq!(c.digest[31], 31);
        data[4] = 0; // HashWX must reject nonzero digest high words
        assert!(Completion::decode(&data).is_err());
        data[4] = 17; // invalid algorithm
        assert!(Completion::decode(&data).is_err());
        data[4] = 1;
        data[12] = 2; // non-boolean found
        assert!(Completion::decode(&data).is_err());
    }

    #[test]
    fn configure_rejects_unaligned_overlapping_or_wrapping_rings() {
        let mock = Mock::default();
        mock.poke(0x2030, RING_SLOTS).unwrap();
        assert!(arm(&mock, 0, 0x2000).is_err());
        assert!(arm(&mock, 0x1001, 0x2000).is_err());
        assert!(arm(&mock, 0x1000, 0x1300).is_err());
        assert!(arm(&mock, u64::MAX - 63, 0x2000).is_err());
        arm(&mock, 0x1000, 0x2000).unwrap();
        assert_eq!(mock.peek(0x2008).unwrap(), 0x1000);
        assert_eq!(mock.peek(0x2010).unwrap(), 0x2000);
    }

    #[test]
    fn reject_ring_capacity_violation_and_over_acknowledgment() {
        let mock = Mock::default();
        mock.poke(0x2020, 1234).unwrap();
        publish(&mock, 1250).unwrap();
        assert!(publish(&mock, 1251).is_err());
        mock.poke(0x2024, 9).unwrap();
        acknowledge(&mock, 9).unwrap();
        assert!(acknowledge(&mock, 10).is_err());
        let state = progress(&mock).unwrap();
        assert_eq!(state.consumed_commands, 1234);
        assert_eq!(state.written_results, 9);
    }
}
