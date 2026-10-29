use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_all_engines_audio_output() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let main_bin = repo_root.join("original_firmware/unpacked/nanowave_main_payload_stm32f446.bin");
    let oled_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");
    let matrix_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
    let kbd_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");
    let qspi_bin = repo_root.join("original_firmware/unpacked/userwavetable_factory_payload_qspi.bin");
    let presets_path = repo_root.join("user_presets.bin");

    if !main_bin.exists() {
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");

    if presets_path.exists() {
        emu.load_qspi_from_disk(&presets_path).unwrap();
    }

    emu.boot(100_000_000);

    // Boot until active screen
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
        let fb = emu.coproc.get_framebuffer();
        let nonzero = fb.iter().filter(|&&b| b != 0).count();
        if nonzero > 400 {
            break;
        }
    }

    // Step type encoder through engine slots
    for engine_step in 0..=2 {
        if engine_step > 0 {
            emu.step_type_encoder(1);
            for _ in 0..50 {
                emu.step_audio_block_quantum();
            }
            // Settle debounce (1023 blocks)
            for _ in 0..1100 {
                emu.step_audio_block_quantum();
            }
        }

        // Read OLED framebuffer to see engine name
        let fb = emu.coproc.get_framebuffer();
        let fb_nonzero = fb.iter().filter(|&&b| b != 0).count();

        if engine_step == 2 {
            println!("--- OLED FOR ENGINE STEP 2 ---");
            for y in 0..64 {
                let page = y / 8;
                let bit = 1 << (y % 8);
                let mut line = String::new();
                for x in 0..128 {
                    if (fb[page * 128 + x] & bit) != 0 {
                        line.push('#');
                    } else {
                        line.push(' ');
                    }
                }
                if !line.trim().is_empty() {
                    println!("{}", line);
                }
            }
        }

        // Trigger key 12
        emu.set_key(12, 200);
        for _ in 0..20 {
            emu.step_audio_block_quantum();
        }

        let mut max_amp = 0i32;
        let mut nz_blocks = 0;
        let test_blocks = if engine_step == 2 { 300 } else { 100 };
        for _ in 0..test_blocks {
            let block = emu.step_audio_block_quantum();
            let mut blk_peak = 0i32;
            for chunk in block.as_chunks::<4>().0 {
                let left = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
                if left.abs() > blk_peak.abs() {
                    blk_peak = left;
                }
            }
            if blk_peak.abs() > 0 {
                nz_blocks += 1;
            }
            if blk_peak.abs() > max_amp {
                max_amp = blk_peak.abs();
            }
        }
        emu.set_key(12, 0);
        for _ in 0..20 {
            emu.step_audio_block_quantum();
        }

        println!("Engine step {}: fb_nonzero={}, max_amp={}, nz_blocks={}",
            engine_step, fb_nonzero, max_amp, nz_blocks);

        assert!(
            max_amp > 1000 && nz_blocks > 50,
            "Engine step {} produced silence! max_amp={}, nz_blocks={}",
            engine_step, max_amp, nz_blocks
        );
    }
}

