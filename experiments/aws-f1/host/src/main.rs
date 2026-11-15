use hashwx::{HashWx, Op, ProgramSet, generate};
use std::{fs, path::PathBuf};

/// 128 little-endian instruction pairs, then k0 and k1 as 4 LE u32 words.
fn encode_program(set: &ProgramSet) -> [u32; 132] {
    let mut words = [0u32; 132];
    for (i, inst) in set.programs().iter().flat_map(|p| p.body().iter()).enumerate() {
        let op = inst.op() as u16;
        assert!(op <= Op::SubLsr as u16);
        assert!(inst.dst() < 8 && inst.src() <= 8 && inst.imm() < 64);
        let src = if inst.op() == Op::Rmcg { 0 } else { inst.src() };
        let instruction = (op << 12)
            | (u16::from(inst.dst()) << 9)
            | (u16::from(src) << 6)
            | u16::from(inst.imm());
        words[i / 2] |= u32::from(instruction) << ((i % 2) * 16);
    }
    let [k0, k1] = set.hash_key();
    words[128] = k0 as u32;
    words[129] = (k0 >> 32) as u32;
    words[130] = k1 as u32;
    words[131] = (k1 >> 32) as u32;
    words
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args_os().nth(1).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tests/fixtures"));
    fs::create_dir_all(&dir)?;
    for seed_index in 0_u64..16 {
        let seed = std::array::from_fn(|i| {
            ((i as u64 * 61 + 17 + seed_index * 137 + (seed_index * i as u64 * 41)) & 255) as u8
        });
        let set = generate(&seed);
        let words = encode_program(&set);
        let suffix = format!("{seed_index:02}");
        fs::write(
            dir.join(format!("config_{suffix}.hex")),
            words.map(|w| format!("{w:08x}\n")).concat(),
        )?;
        let reference = HashWx::interpret_program(&set);
        let mut golden = String::new();
        for nonce in [0_u64, 1, 0x0123_4567_89ab_cdef] {
            golden.push_str(&format!("{:016x}\n", reference.hash(nonce)));
        }
        fs::write(dir.join(format!("golden_{suffix}.hex")), &golden)?;
        if seed_index == 0 {
            fs::write(dir.join("config.hex"), words.map(|w| format!("{w:08x}\n")).concat())?;
            fs::write(dir.join("golden.hex"), golden)?;
        }
    }
    println!("generated 16 program sets and 48 golden hashes in {}", dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_encoding_preserves_ir() {
        for n in 0u64..128 {
            let mut seed = [0u8; 32];
            for (i, byte) in seed.iter_mut().enumerate() {
                *byte = (n.wrapping_mul(193).wrapping_add(i as u64 * 47) >> (i % 5)) as u8;
            }
            let set = generate(&seed);
            let packed = encode_program(&set);
            for (i, inst) in set.programs().iter().flat_map(|p| p.body().iter()).enumerate() {
                let raw = (packed[i/2] >> ((i%2)*16)) as u16;
                assert_eq!(raw >> 12, inst.op() as u16);
                assert_eq!(((raw >> 9) & 7) as u8, inst.dst());
                if inst.op() != Op::Rmcg {
                    assert_eq!(((raw >> 6) & 7) as u8, inst.src());
                }
                assert_eq!((raw & 63) as u8, inst.imm());
            }
            let [k0,k1] = set.hash_key();
            assert_eq!(u64::from(packed[128]) | (u64::from(packed[129]) << 32), k0);
            assert_eq!(u64::from(packed[130]) | (u64::from(packed[131]) << 32), k1);
        }
    }
}
