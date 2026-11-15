#[cfg(test)]
use hashwx::Op;
use hashwx::{HashWx, generate};
use hashwx_f1_fixtures::encoding::encode_hashwx;
use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tests/fixtures"));
    fs::create_dir_all(&dir)?;
    for seed_index in 0_u64..16 {
        let seed = std::array::from_fn(|i| {
            ((i as u64 * 61 + 17 + seed_index * 137 + (seed_index * i as u64 * 41)) & 255) as u8
        });
        let set = generate(&seed);
        let words = encode_hashwx(&set);
        let suffix = format!("{seed_index:02}");
        fs::write(
            dir.join(format!("config_{suffix}.hex")),
            words.map(|w| format!("{w:08x}\n")).concat(),
        )?;
        let reference = HashWx::interpret_program(&set);
        if seed_index == 0 {
            let sequential: String = (0..64_u64)
                .map(|nonce| format!("{:016x}\n", reference.hash(nonce)))
                .collect();
            fs::write(dir.join("hashwx_scan_golden.hex"), sequential)?;
            let wrap: String = [u64::MAX - 1, u64::MAX, 0, 1]
                .into_iter()
                .map(|nonce| format!("{:016x}\n", reference.hash(nonce)))
                .collect();
            fs::write(dir.join("hashwx_wrap_golden.hex"), wrap)?;
        }
        let mut golden = String::new();
        for nonce in [0_u64, 1, 0x0123_4567_89ab_cdef] {
            golden.push_str(&format!("{:016x}\n", reference.hash(nonce)));
        }
        fs::write(dir.join(format!("golden_{suffix}.hex")), &golden)?;
        if seed_index == 0 {
            fs::write(
                dir.join("config.hex"),
                words.map(|w| format!("{w:08x}\n")).concat(),
            )?;
            fs::write(dir.join("golden.hex"), golden)?;
        }
    }
    println!(
        "generated 16 program sets and 48 golden hashes in {}",
        dir.display()
    );
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
            let packed = encode_hashwx(&set);
            for (i, inst) in set
                .programs()
                .iter()
                .flat_map(|p| p.body().iter())
                .enumerate()
            {
                let raw = (packed[i / 2] >> ((i % 2) * 16)) as u16;
                assert_eq!(raw >> 12, inst.op() as u16);
                assert_eq!(((raw >> 9) & 7) as u8, inst.dst());
                if inst.op() != Op::Rmcg {
                    assert_eq!(((raw >> 6) & 7) as u8, inst.src());
                }
                assert_eq!((raw & 63) as u8, inst.imm());
            }
            let [k0, k1] = set.hash_key();
            assert_eq!(u64::from(packed[128]) | (u64::from(packed[129]) << 32), k0);
            assert_eq!(u64::from(packed[130]) | (u64::from(packed[131]) << 32), k1);
        }
    }
}
