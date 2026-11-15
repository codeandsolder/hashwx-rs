//! F1 BAR0 host runner against AWS FPGA SDK libfpga_mgmt.
//! This intentionally requires an F1 host with the AWS SDK installed.
use hashwx::{HashWx, ProgramSet, generate};
use std::error::Error;
use std::ffi::c_int;

#[link(name = "fpga_mgmt")]
unsafe extern "C" {
    fn fpga_pci_init() -> c_int;
    fn fpga_pci_attach(slot: c_int, pf: c_int, bar: c_int, flags: u32, handle: *mut c_int) -> c_int;
    fn fpga_pci_detach(handle: c_int) -> c_int;
    fn fpga_pci_poke(handle: c_int, offset: u64, value: u32) -> c_int;
    fn fpga_pci_peek(handle: c_int, offset: u64, value: *mut u32) -> c_int;
}

struct Fpga(c_int);
impl Fpga {
    fn attach(slot: c_int) -> Result<Self, Box<dyn Error>> {
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
    fn poke(&self, offset: u64, value: u32) -> Result<(), Box<dyn Error>> {
        // SAFETY: valid attached handle, 32-bit MMIO register offset.
        let rc = unsafe { fpga_pci_poke(self.0, offset, value) };
        if rc != 0 {
            return Err(format!("poke at 0x{offset:x} failed: {rc}").into());
        }
        Ok(())
    }
    fn peek(&self, offset: u64) -> Result<u32, Box<dyn Error>> {
        let mut value = 0u32;
        // SAFETY: valid attached handle and stack 32-bit output pointer.
        let rc = unsafe { fpga_pci_peek(self.0, offset, &raw mut value) };
        if rc != 0 {
            return Err(format!("peek at 0x{offset:x} failed: {rc}").into());
        }
        Ok(value)
    }
    fn read64(&self, low: u64) -> Result<u64, Box<dyn Error>> {
        Ok(u64::from(self.peek(low)?) | (u64::from(self.peek(low+4)?) << 32))
    }
    fn write64(&self, low: u64, x: u64) -> Result<(), Box<dyn Error>> {
        self.poke(low, x as u32)?;
        self.poke(low+4, (x>>32) as u32)
    }
}
impl Drop for Fpga {
    fn drop(&mut self) {
        // SAFETY: one successful attach, exactly one detach upon drop.
        let _ = unsafe { fpga_pci_detach(self.0) };
    }
}

fn parse_number(s: &str) -> Result<u64, Box<dyn Error>> {
    if let Some(hex) = s.strip_prefix("0x") {
        Ok(u64::from_str_radix(hex,16)?)
    } else {
        Ok(s.parse()?)
    }
}
fn parse_seed(hex: &str) -> Result<[u8;32], Box<dyn Error>> {
    if hex.len() != 64 { return Err("seed must be 64 hex characters".into()); }
    let mut seed = [0u8;32];
    for (i,b) in seed.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i*2..i*2+2],16)?;
    }
    Ok(seed)
}
fn upload(fpga: &Fpga, set: &ProgramSet) -> Result<(),Box<dyn Error>> {
    for (i, pair) in set.programs().iter().flat_map(|p|p.body().iter())
        .collect::<Vec<_>>().chunks(2).enumerate() {
        let instruction = |inst: &hashwx::Instruction| -> u16 {
            let src = if inst.op() == hashwx::Op::Rmcg {0} else {inst.src()};
            ((inst.op() as u16)<<12) | ((inst.dst() as u16)<<9)
                | ((src as u16)<<6) | u16::from(inst.imm())
        };
        let word = u32::from(instruction(pair[0])) | (u32::from(instruction(pair[1]))<<16);
        fpga.poke(0x800+(i as u64)*4,word)?;
    }
    let [k0,k1] = set.hash_key();
    fpga.write64(0xA00,k0)?;
    fpga.write64(0xA08,k1)?;
    Ok(())
}

fn main() -> Result<(),Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 || args.len()>6 {
        return Err("usage: hashwx-f1-solve <seed_hex_64> <start_nonce> <nonce_count> <exclusive_target> [slot=0]".into());
    }
    let seed=parse_seed(&args[1])?;
    let nonce=parse_number(&args[2])?;
    let count=u32::try_from(parse_number(&args[3])?)?;
    let target=parse_number(&args[4])?;
    let slot=if args.len()==6 {args[5].parse::<c_int>()?} else {0};
    let set=generate(&seed);
    let oracle=HashWx::interpret_program(&set);
    let fpga=Fpga::attach(slot)?;
    if fpga.peek(0x600)? != 0x48585731 {
        return Err("wrong F1 AFI: HashWX register ID is not HXW1".into());
    }
    upload(&fpga,&set)?;
    fpga.write64(0x608,nonce)?;
    fpga.write64(0x610,target)?;
    fpga.poke(0x618,count)?;
    fpga.poke(0x604,1)?;
    // Unbounded polling on an actual device is intentional; watchdog handled
    // by the caller, not a guessed time limit tied to unknown fabric clocks.
    loop {
        let state=fpga.peek(0x61c)?;
        if state & 2 != 0 {
            let attempts=fpga.peek(0x620)?;
            if state & 4 != 0 {
                let winner_nonce=fpga.read64(0x624)?;
                let winner_hash=fpga.read64(0x62c)?;
                let verify=oracle.hash(winner_nonce);
                if verify!=winner_hash || verify>=target ||
                   (winner_nonce.wrapping_sub(nonce) >= u64::from(count)) {
                    return Err(format!("FPGA returned invalid candidate: nonce={winner_nonce:x} actual={winner_hash:x} CPU={verify:x}").into());
                }
                println!("FOUND nonce=0x{winner_nonce:016x} hash=0x{winner_hash:016x} attempts={attempts}");
            } else {
                println!("NOT FOUND attempts={attempts}");
            }
            break;
        }
        std::thread::yield_now();
    }
    Ok(())
}
