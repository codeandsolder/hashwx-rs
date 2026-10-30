// HashX v1 compatibility model derived from Tor Project hashx 0.10.0 (LGPL-3.0-only).
// Adapted for hashwx-rs; execution backends are implemented separately in this repository.
//! HashX-flavored `SipHash` implementation
//!
//! We need `SipHash` to generate parts of `HashX`'s internal state: the initial
//! register values for the hash program, and the stream of pseudorandom numbers
//! used to generate the program itself. The fundamentals are as described in
//! the `SipHash` paper, but much of the algorithm around the basic add-rotate-xor
//! core has been modified:
//!
//!   - Seeding: vanilla `SipHash` uses a nothing-up-my-sleeve constant to safely
//!     init 256 bits of internal state from 128 bits of user-supplied key data.
//!     The `HashX` implementation instead uses Blake2b to pre-process an
//!     arbitrary sized seed into a 512-bit pseudorandom value which is directly
//!     used to init the state of two `SipHash` instances.
//!
//!   - The `SipHash` paper describes a compression function that includes a
//!     length indicator and padding, and supports variable length inputs. This
//!     is not needed, and `HashX` uses its own way of constructing a SipHash2,4
//!     instance that takes a counter as input.
//!
//!   - `HashX` also needs SipHash1,3 which it uses for a lightweight pseudorandom
//!     number stream internally. This variant isn't typically used on its own
//!     or implemented in libraries. `HashX` also uses its own counter input
//!     construction method.
//!
//!   - In addition to the SipHash1,3 and SipHash2,4 counter modes, `HashX`
//!     makes use of raw `SipRounds` while digesting a `RegisterFile` after the
//!     generated hash function completes.
//!
//! `SipHash` is defined by Jean-Philippe Aumasson and Daniel J.Bernstein in
//! their paper "`SipHash`: a fast short-input PRF" (2012).

use blake2::Blake2bVarCore;
use blake2::digest::block_buffer::LazyBuffer;
use blake2::digest::core_api::{BlockSizeUser, UpdateCore, VariableOutputCore};
use std::fmt::{self, Debug};

/// Internal state of one `SipHash` instance
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SipState {
    /// State variable V0 as defined in the `SipHash` paper
    pub(crate) v0: u64,
    /// State variable V1 as defined in the `SipHash` paper
    pub(crate) v1: u64,
    /// State variable V2 as defined in the `SipHash` paper
    pub(crate) v2: u64,
    /// State variable V3 as defined in the `SipHash` paper
    pub(crate) v3: u64,
}

impl Debug for SipState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SipState[ {:#018x}, {:#018x}, {:#018x}, {:#018x} ]",
            self.v0, self.v1, self.v2, self.v3
        )
    }
}

impl From<SipState> for [u64; 4] {
    #[inline]
    fn from(s: SipState) -> Self {
        [s.v0, s.v1, s.v2, s.v3]
    }
}

impl From<[u64; 4]> for SipState {
    #[inline]
    fn from(a: [u64; 4]) -> Self {
        Self::new(a[0], a[1], a[2], a[3])
    }
}

impl SipState {
    /// Size of the internal `SipHash` state
    const SIZE: usize = 32;

    /// Construct a new `SipHash` state.
    ///
    /// This takes the parameters `v0..v3` as defined in the `SipHash` paper.
    #[inline]
    pub const fn new(v0: u64, v1: u64, v2: u64, v3: u64) -> Self {
        Self { v0, v1, v2, v3 }
    }

    /// Construct a new `SipHash` state directly from bytes.
    ///
    /// This is not suitable for use with arbitrary user input, such
    /// as all zeroes. `HashX` always generates these initialization vectors
    /// using another pseudorandom function (Blake2b).
    #[inline]
    pub fn new_from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        let word = |offset: usize| {
            u64::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ])
        };
        Self::new(word(0), word(8), word(16), word(24))
    }

    /// Construct a pair of `SipHash` instances from a seed.
    ///
    /// The seed may be an arbitrary length. Takes the Blake2b hash of the seed
    /// using the correct settings for `HashX`, splitting the digest into two
    /// [`Self::new_from_bytes()`] calls.
    pub fn pair_from_seed(seed: &[u8]) -> (Self, Self) {
        /// Choice of Blake2b engine; we need to use its lower level
        /// interface, to access `new_with_params()`.
        type Core = Blake2bVarCore;

        /// Blake2b block size
        type BlockSize = <Core as BlockSizeUser>::BlockSize;

        let mut buffer = LazyBuffer::<BlockSize>::new(&[]);
        let mut core = Core::new_with_params(b"HashX v1", &[], 0, 64);
        let mut digest = blake2::digest::Output::<Core>::default();

        buffer.digest_blocks(seed, |blocks| core.update_blocks(blocks));
        core.finalize_variable_core(&mut buffer, &mut digest);

        let mut first = [0_u8; Self::SIZE];
        let mut second = [0_u8; Self::SIZE];
        first.copy_from_slice(&digest[..Self::SIZE]);
        second.copy_from_slice(&digest[Self::SIZE..]);
        (Self::new_from_bytes(&first), Self::new_from_bytes(&second))
    }

    /// One `SipRound` as defined in the `SipHash` paper
    ///
    /// Modifies the `SipState` in-place.
    #[inline]
    pub(crate) const fn sip_round(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v1 = self.v1.rotate_left(13);
        self.v3 = self.v3.rotate_left(16);
        self.v1 ^= self.v0;
        self.v3 ^= self.v2;
        self.v0 = self.v0.rotate_left(32);

        self.v2 = self.v2.wrapping_add(self.v1);
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v1 = self.v1.rotate_left(17);
        self.v3 = self.v3.rotate_left(21);
        self.v1 ^= self.v2;
        self.v3 ^= self.v0;
        self.v2 = self.v2.rotate_left(32);
    }
}

/// `HashX`'s flavor of SipHash1,3 counter mode with 64-bit output
pub const fn siphash13_ctr(key: SipState, input: u64) -> u64 {
    let mut s = key;
    s.v3 ^= input;

    s.sip_round();

    s.v0 ^= input;
    s.v2 ^= 0xff;

    s.sip_round();
    s.sip_round();
    s.sip_round();

    s.v0 ^ s.v1 ^ s.v2 ^ s.v3
}

/// `HashX`'s flavor of SipHash2,4 counter mode with 512-bit output
pub const fn siphash24_ctr(key: SipState, input: u64) -> [u64; 8] {
    let mut s = key;
    s.v1 ^= 0xee;
    s.v3 ^= input;

    s.sip_round();
    s.sip_round();

    s.v0 ^= input;
    s.v2 ^= 0xee;

    s.sip_round();
    s.sip_round();
    s.sip_round();
    s.sip_round();

    let mut t = s;
    t.v1 ^= 0xdd;

    t.sip_round();
    t.sip_round();
    t.sip_round();
    t.sip_round();

    [s.v0, s.v1, s.v2, s.v3, t.v0, t.v1, t.v2, t.v3]
}
