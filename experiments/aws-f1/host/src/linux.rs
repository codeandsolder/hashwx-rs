//! Linux F1 PCI AppPF character-device transport.
//! A kernel PCI driver owns DMA mapping and returns the *device DMA bus*
//! address, not a userspace pointer / pagemap PFN. The 4 KiB coherent region
//! holds a command ring (0..1024), completion ring (1024..2048) and padding.
//! SPDX-License-Identifier: LGPL-3.0-only
use crate::pcim::{RING_BYTES, RING_SLOTS, SLOT_BYTES};
use crate::queue::QueueIo;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::mem::size_of;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::NonNull;

#[repr(C)]
#[derive(Default, Debug)]
struct DeviceInfo {
    dma_address: u64,
    dma_size: u32,
    abi_version: u32,
}

#[repr(C)]
#[derive(Default)]
struct Register {
    offset: u32,
    value: u32,
}

const fn ioctl_code(direction: u32, number: u32, size: usize) -> libc::c_ulong {
    ((direction << 30) | ((size as u32) << 16) | ((b'H' as u32) << 8) | number) as libc::c_ulong
}
const GET_INFO: libc::c_ulong = ioctl_code(2, 0x70, size_of::<DeviceInfo>());
const PEEK: libc::c_ulong = ioctl_code(3, 0x71, size_of::<Register>());
const POKE: libc::c_ulong = ioctl_code(1, 0x72, size_of::<Register>());
const COHERENT_BYTES: usize = 4096;
const MMIO_BYTES: u64 = 0x6000;

pub struct F1Device {
    file: File,
    mapped: NonNull<u8>,
    pub dma_bus_address: u64,
}

impl F1Device {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut info = DeviceInfo::default();
        // SAFETY: file is open and the kernel reads/writes the exact C ABI
        // struct, initialized above. ioctl request is the shared UAPI code.
        if unsafe { libc::ioctl(file.as_raw_fd(), GET_INFO, &raw mut info) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if info.abi_version != 1
            || info.dma_size as usize != COHERENT_BYTES
            || info.dma_address == 0
            || !info.dma_address.is_multiple_of(4096)
        {
            return Err(format!("invalid F1 device ABI / DMA geometry: {info:?}").into());
        }
        // SAFETY: coherent 4 KiB buffer owned by PCI driver; its mmap
        // implementation only accepts the entire allocation and offset 0.
        let mapped = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                COHERENT_BYTES,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if mapped == libc::MAP_FAILED {
            return Err(std::io::Error::last_os_error().into());
        }
        let mapped = NonNull::new(mapped.cast::<u8>()).ok_or("null coherent mmap")?;
        Ok(Self {
            file,
            mapped,
            dma_bus_address: info.dma_address,
        })
    }

    pub fn ring_addresses(&self) -> (u64, u64) {
        (self.dma_bus_address, self.dma_bus_address + RING_BYTES)
    }

    fn slot_address(&self, completion: bool, slot: u32) -> Result<*mut u64, Box<dyn Error>> {
        if slot >= RING_SLOTS {
            return Err(format!("out-of-range F1 DMA slot {slot}").into());
        }
        let offset = usize::from(completion) * RING_BYTES as usize + slot as usize * SLOT_BYTES;
        // SAFETY: offset covers at most 2048 bytes of the mapped 4096
        // bytes, is 64-byte aligned, and F1Driver mmap is page-aligned.
        Ok(unsafe { self.mapped.as_ptr().add(offset).cast::<u64>() })
    }

    pub fn write_descriptor(
        &self,
        slot: u32,
        value: &[u8; SLOT_BYTES],
    ) -> Result<(), Box<dyn Error>> {
        let dst = self.slot_address(false, slot)?;
        for (index, bytes) in value.chunks_exact(8).enumerate() {
            let word = u64::from_le_bytes(bytes.try_into()?);
            // SAFETY: mapped coherent PCI DMA memory, aligned u64 slot;
            // volatile prevents compiler from eliding device-visible stores.
            unsafe { std::ptr::write_volatile(dst.add(index), word.to_le()) };
        }
        Ok(())
    }

    pub fn read_completion(&self, slot: u32) -> Result<[u8; SLOT_BYTES], Box<dyn Error>> {
        let src = self.slot_address(true, slot)?;
        let mut value = [0u8; SLOT_BYTES];
        for (index, bytes) in value.chunks_exact_mut(8).enumerate() {
            // SAFETY: completed slots are read only after the device's
            // produced counter advances and an acquire fence in pcim::progress.
            let word = unsafe { std::ptr::read_volatile(src.add(index)) };
            bytes.copy_from_slice(&u64::from_le(word).to_le_bytes());
        }
        Ok(value)
    }
}

impl QueueIo for F1Device {
    fn poke(&self, offset: u64, value: u32) -> Result<(), Box<dyn Error>> {
        if offset >= MMIO_BYTES || !offset.is_multiple_of(4) {
            return Err("invalid F1 MMIO write offset".into());
        }
        let mut reg = Register {
            offset: u32::try_from(offset)?,
            value,
        };
        // SAFETY: kernel accepts exact C UAPI struct and validates address.
        if unsafe { libc::ioctl(self.file.as_raw_fd(), POKE, &raw mut reg) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    fn peek(&self, offset: u64) -> Result<u32, Box<dyn Error>> {
        if offset >= MMIO_BYTES || !offset.is_multiple_of(4) {
            return Err("invalid F1 MMIO read offset".into());
        }
        let mut reg = Register {
            offset: u32::try_from(offset)?,
            value: 0,
        };
        // SAFETY: kernel accepts exact C UAPI struct and fills value.
        if unsafe { libc::ioctl(self.file.as_raw_fd(), PEEK, &raw mut reg) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(reg.value)
    }
}

impl Drop for F1Device {
    fn drop(&mut self) {
        // SAFETY: exactly one successful coherent mmap.
        let rc = unsafe { libc::munmap(self.mapped.as_ptr().cast(), COHERENT_BYTES) };
        debug_assert_eq!(rc, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ioctl_codes_match_linux_generic_x86_64_uapi() {
        assert_eq!(size_of::<DeviceInfo>(), 16);
        assert_eq!(size_of::<Register>(), 8);
        assert_eq!(GET_INFO as u64, 0x8010_4870);
        assert_eq!(PEEK as u64, 0xc008_4871);
        assert_eq!(POKE as u64, 0x4008_4872);
        assert_eq!(RING_BYTES, 1024);
    }
}
