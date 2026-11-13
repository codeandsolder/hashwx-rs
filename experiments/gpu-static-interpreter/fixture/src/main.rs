use hashwx::{HashWx, generate};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = std::env::args()
        .nth(1)
        .unwrap_or("1024".to_owned())
        .parse::<usize>()?;
    let dst = PathBuf::from(std::env::args().nth(2).unwrap_or(".".to_owned()));
    std::fs::create_dir_all(&dst)?;
    let mut programs = BufWriter::new(File::create(dst.join("programs.bin"))?);
    let mut gold = BufWriter::new(File::create(dst.join("gold.bin"))?);
    for index in 0..count {
        let mut seed = [0u8; 32];
        for (part, chunk) in seed.chunks_exact_mut(8).enumerate() {
            let mut x = (index as u64).wrapping_mul(0x9e3779b97f4a7c15)
                ^ (part as u64).wrapping_mul(0x94d049bb133111eb);
            x ^= x >> 30;
            x = x.wrapping_mul(0xbf58476d1ce4e5b9);
            x ^= x >> 27;
            x = x.wrapping_mul(0x94d049bb133111eb);
            x ^= x >> 31;
            chunk.copy_from_slice(&x.to_le_bytes());
        }
        let p = generate(&seed);
        for sub in p.programs() {
            for op in sub.body() {
                let packed = (op.op() as u32)
                    | (u32::from(op.dst()) << 8)
                    | (u32::from(op.src()) << 16)
                    | (u32::from(op.imm()) << 24);
                programs.write_all(&packed.to_le_bytes())?;
            }
        }
        for sub in p.programs() {
            let value = u32::from(sub.unconditional());
            programs.write_all(&value.to_le_bytes())?;
        }
        for key in p.hash_key() {
            programs.write_all(&key.to_le_bytes())?;
        }
        let oracle = HashWx::interpret_program(&p);
        for nonce in 0..4_u64 {
            gold.write_all(&oracle.hash(nonce).to_le_bytes())?;
        }
        if index == 0 {
            println!(
                "seed0 golden: {:016x} {:016x}",
                oracle.hash(0),
                oracle.hash(1)
            );
        }
    }
    programs.flush()?;
    gold.flush()?;
    println!(
        "generated {count} programs ({} bytes each) and 4 independent reference hashes per program",
        1168
    );
    Ok(())
}
