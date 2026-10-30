//! Deterministic corpus checks for generated `HashWX` programs.

fn corpus_seed(index: u64) -> [u8; hashwx::SEED_SIZE] {
    fn splitmix64(mut value: u64) -> u64 {
        value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    let mut seed = [0_u8; hashwx::SEED_SIZE];
    for (word_index, chunk) in [0_u64, 1, 2, 3]
        .into_iter()
        .zip(seed.as_chunks_mut::<8>().0.iter_mut())
    {
        let word = splitmix64(index.wrapping_add(word_index * 0x0000_0100_0000_01b3));
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    seed
}

#[test]
fn generated_program_invariants_hold_across_corpus() {
    for seed_index in 0_u64..1_024 {
        let set = hashwx::generate(&corpus_seed(seed_index));
        for (program_index, program) in set.programs().iter().enumerate() {
            let mut destinations = 0_u16;
            let mut multiply_ops = 0_u8;

            for instruction in program.body() {
                assert!(instruction.dst() < 8);
                destinations |= 1_u16 << instruction.dst();

                match instruction.op() {
                    hashwx::Op::MulOr | hashwx::Op::MulXor | hashwx::Op::MulAdd => {
                        multiply_ops += 1;
                        assert!([1, 9, 33].contains(&instruction.imm()));
                    }
                    hashwx::Op::Rmcg => {
                        multiply_ops += 1;
                        assert_eq!(instruction.src(), 8);
                        assert!((1..=63).contains(&instruction.imm()));
                    }
                    hashwx::Op::XorRor | hashwx::Op::AddRor | hashwx::Op::SubRor => {
                        assert!((1..=63).contains(&instruction.imm()));
                    }
                    hashwx::Op::XorAsr
                    | hashwx::Op::AddAsr
                    | hashwx::Op::SubAsr
                    | hashwx::Op::XorLsr
                    | hashwx::Op::AddLsr
                    | hashwx::Op::SubLsr => {
                        assert!((1..=3).contains(&instruction.imm()));
                    }
                }
            }

            assert_eq!(destinations, 0xff);
            assert_eq!(multiply_ops, 4);
            assert_eq!(
                program.unconditional(),
                program_index == hashwx::PROGRAM_COUNT - 1
            );
        }
    }
}
