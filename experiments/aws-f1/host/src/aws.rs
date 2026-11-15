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

impl crate::queue::QueueIo for Fpga {
    fn poke(&self, offset: u64, value: u32) -> Result<(), Box<dyn Error>> {
        Self::poke(self, offset, value)
    }

    fn peek(&self, offset: u64) -> Result<u32, Box<dyn Error>> {
        Self::peek(self, offset)
    }
}
