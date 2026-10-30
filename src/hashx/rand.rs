// HashX v1 compatibility model derived from Tor Project hashx 0.10.0 (LGPL-3.0-only).
// Adapted for hashwx-rs; execution backends are implemented separately in this repository.
//! Pseudorandom number utilities for `HashX`'s program generator
//!
//! `HashX` uses pseudorandom numbers to make individual decisions in the program
//! generation process. The program generator consumes u8 and u32 values that
//! use a shared u64 generator, implemented using SipHash1,3.
//!
//! We use the [`Rng`] trait for this underlying u64 generator,
//! allowing substitute random number generators for testing or for special
//! purposes that don't require compatibility with `HashX` proper.
//!
//! The stateful u8 and u32 layer comes from this module's [`RngBuffer`].
//! It's important for the u8 and u32 queues to share a common generator.
//! The order of dequeueing u8 items vs u32 items intentionally modifies the
//! assignment of particular u64 [`Rng`] values to the two queues.

use std::convert::Infallible;

use crate::hashx::siphash::{SipState, siphash13_ctr};
use arrayvec::ArrayVec;
use rand_core::{Rng, TryRng};

/// Wrap a [`Rng`] implementation for fast `u8` and `u32` output.
///
/// This maintains small queues for each data type: up to one `u32` and up to
/// 7 bytes. The queueing behavior matches conventions required by `HashX`:
/// The underlying `u64` values are always generated lazily, and component
/// values are extracted in big endian order.
#[derive(Debug)]
pub struct RngBuffer<'a, T: Rng> {
    /// Inner [`Rng`] implementation
    inner: &'a mut T,
    /// Buffer of remaining u8 values from breaking up a u64
    u8_vec: ArrayVec<u8, 7>,
    /// Up to one buffered u32 value
    u32_opt: Option<u32>,
}

impl<'a, T: Rng> RngBuffer<'a, T> {
    /// Construct a new empty buffer around a [`Rng`] implementation.
    ///
    /// No actual random numbers will be generated until the first call to
    /// [`Self::next_u8`] or [`Self::next_u32`].
    #[inline]
    pub(crate) fn new(rng: &'a mut T) -> Self {
        Self {
            inner: rng,
            u8_vec: ArrayVec::default(),
            u32_opt: None,
        }
    }

    /// Request 32 bits from the buffered random number generator.
    ///
    /// If we have buffered data stored, returns that. If not,
    /// requests 64 bits from the [`Rng`] and saves half for later.
    #[inline]
    pub(crate) fn next_u32(&mut self) -> u32 {
        let previous = self.u32_opt;
        if let Some(value) = previous {
            self.u32_opt = None;
            value
        } else {
            let bytes = self.inner.next_u64().to_le_bytes();
            let low = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let high = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            self.u32_opt = Some(low);
            high
        }
    }

    /// Request 8 bits from the buffered random number generator.
    ///
    /// If we have buffered data stored, returns that. If not,
    /// requests 64 bits from the [`Rng`] and saves 7 bytes for later.
    #[inline]
    pub(crate) fn next_u8(&mut self) -> u8 {
        let value = self.u8_vec.pop();
        if let Some(value) = value {
            value
        } else {
            // Little endian (reversed) order here,
            // because we dequeue items from the end of the Vec.
            let bytes = self.inner.next_u64().to_le_bytes();
            for &byte in &bytes[..7] {
                self.u8_vec.push(byte);
            }
            bytes[7]
        }
    }
}

/// HashX-style random number generator built on SipHash1,3
///
/// This is an implementation of [`Rng`] using SipHash1,3 as
/// the 64-bit PRNG layer needed by `HashX`'s program generator.
#[derive(Debug, Clone)]
pub struct SipRand {
    /// `SipHash` state vector used as input to SipHash1,3 in counter mode
    key: SipState,
    /// Next unused counter value
    counter: u64,
}

impl SipRand {
    /// Build a new `SipHash` random number generator.
    ///
    /// The internal SipHash1,3 generator is initialized to a supplied
    /// internal state, and the counter is reset to zero.
    #[inline]
    pub const fn new(key: SipState) -> Self {
        Self::new_with_counter(key, 0)
    }

    /// Build a new [`SipRand`] with a specific initial counter value.
    #[inline]
    pub const fn new_with_counter(key: SipState, counter: u64) -> Self {
        Self { key, counter }
    }
}

impl TryRng for SipRand {
    type Error = Infallible;

    /// Generate a full 64-bit random result using SipHash1,3.
    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        let value = siphash13_ctr(self.key, self.counter);
        self.counter += 1;
        Ok(value)
    }

    /// Return a 32-bit value by discarding the upper half of a 64-bit result.
    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        let bytes = self.next_u64().to_le_bytes();
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Fill `dest` with random data.
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Infallible> {
        rand_core::utils::fill_bytes_via_next_word(dest, || Ok::<u64, Infallible>(self.next_u64()))
    }
}
