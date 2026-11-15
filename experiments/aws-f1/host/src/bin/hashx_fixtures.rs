#[cfg(test)]
use hashwx::HashXInstruction as Instruction;
use hashwx::{HashX, HashXProgram};
use hashwx_f1_fixtures::encoding::encode_hashx;
use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tests/fixtures"));
    fs::create_dir_all(&dir)?;
    let mut successful_seeds = 0;
    let mut rejected_seeds = 0;
    for n in 0u64..1000 {
        let seed: Vec<u8> = if n < 32 {
            (0..32)
                .map(|i| ((n.wrapping_mul(149) + i * 71 + (n >> (i % 7)) * 37) & 255) as u8)
                .collect()
        } else {
            match n {
                32 => vec![],
                33 => vec![0],
                34 => (0..17).map(|i| (i * 31) as u8).collect(),
                35 => (0..96).map(|i| (i * 13) as u8).collect(),
                _ => break,
            }
        };
        let Ok(program) = HashXProgram::generate(&seed) else {
            rejected_seeds += 1;
            continue;
        };
        let encoded = encode_hashx(&program);
        let name = format!("{successful_seeds:02}");
        fs::write(
            dir.join(format!("hashx_config_{name}.hex")),
            encoded.map(|word| format!("{word:08x}\n")).concat(),
        )?;
        let reference = HashX::interpret_program(&program);
        if successful_seeds == 0 {
            let encode_digest = |nonce: u64| -> String {
                let digest = reference.hash_bytes(nonce);
                let mut text = String::with_capacity(65);
                for byte in digest.iter().rev() {
                    text.push_str(&format!("{byte:02x}"));
                }
                text.push('\n');
                text
            };
            let sequential: String = (0..64_u64).map(&encode_digest).collect();
            fs::write(dir.join("hashx_scan_golden.hex"), sequential)?;
            let wrap: String = [u64::MAX - 1, u64::MAX, 0, 1]
                .into_iter()
                .map(encode_digest)
                .collect();
            fs::write(dir.join("hashx_wrap_golden.hex"), wrap)?;
        }
        let mut golden = String::new();
        for nonce in [0u64, 1, 0x0123_4567_89ab_cdef] {
            let digest = reference.hash_bytes(nonce);
            for byte in digest.iter().rev() {
                golden.push_str(&format!("{byte:02x}"));
            }
            golden.push('\n');
        }
        fs::write(dir.join(format!("hashx_golden_{name}.hex")), golden)?;
        successful_seeds += 1;
        if successful_seeds == 36 {
            break;
        }
    }
    assert_eq!(successful_seeds, 36);
    println!(
        "HashX: generated {successful_seeds} programs / {} digests (rejected seeds: {rejected_seeds})",
        successful_seeds * 3
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_ir_roundtrip() {
        let mut successful = 0;
        for n in 0u64..128 {
            let seed: [u8; 32] =
                std::array::from_fn(|i| (n.wrapping_mul(73) + i as u64 * 31) as u8);
            let Ok(program) = HashXProgram::generate(&seed) else {
                continue;
            };
            let packed = encode_hashx(&program);
            for (i, inst) in program.instructions().iter().enumerate() {
                let raw = u64::from(packed[i * 2]) | (u64::from(packed[i * 2 + 1]) << 32);
                let op = (raw >> 60) & 15;
                let dst = (raw >> 57) & 7;
                let src = (raw >> 54) & 7;
                let shift = (raw >> 48) & 63;
                let imm = raw as u32;
                match inst {
                    Instruction::Mul { dst: d, src: s }
                    | Instruction::UMulH { dst: d, src: s }
                    | Instruction::SMulH { dst: d, src: s }
                    | Instruction::Sub { dst: d, src: s }
                    | Instruction::Xor { dst: d, src: s } => {
                        assert_eq!(dst, d.index().into());
                        assert_eq!(src, s.index().into());
                    }
                    Instruction::AddShift {
                        dst: d,
                        src: s,
                        left_shift: k,
                    } => {
                        assert_eq!(
                            (op, dst, src, shift),
                            (3, d.index().into(), s.index().into(), (*k).into())
                        );
                    }
                    Instruction::AddConst { dst: d, src: c } => {
                        assert_eq!((op, dst, imm), (4, d.index().into(), *c as u32));
                    }
                    Instruction::XorConst { dst: d, src: c } => {
                        assert_eq!((op, dst, imm), (7, d.index().into(), *c as u32));
                    }
                    Instruction::Rotate {
                        dst: d,
                        right_rotate: k,
                    } => {
                        assert_eq!((op, dst, shift), (8, d.index().into(), (*k).into()));
                    }
                    Instruction::Target => assert_eq!(op, 9),
                    Instruction::Branch { mask } => assert_eq!((op, imm), (10, *mask)),
                }
            }
            for (i, k) in program.register_key().iter().enumerate() {
                assert_eq!(
                    u64::from(packed[1024 + i * 2]) | (u64::from(packed[1025 + i * 2]) << 32),
                    *k
                );
            }
            successful += 1;
        }
        assert!(successful >= 100, "too many rejected seeds");
    }
}
