use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_wave_user_oscillator_synthesis() {
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

    if !main_bin.exists() {
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");

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

    // Step type encoder 16 times to reach WaveUser (engine slot 16 from BasicWaves)
    for _step in 1..=16 {
        emu.step_type_encoder(1);
        for _ in 0..50 {
            emu.step_audio_block_quantum();
        }
    }

    // Settle encoder debounce (1023 audio blocks)
    for _ in 0..1100 {
        emu.step_audio_block_quantum();
    }

    let fb = emu.coproc.get_framebuffer();
    let nonzero = fb.iter().filter(|&&b| b != 0).count();
    println!("WaveUser screen active: fb nonzero pixels = {}", nonzero);

    // Trigger key 12
    println!("Triggering note on WaveUser engine...");
    emu.set_key(12, 200);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }

    let mut max_amp = 0i32;
    let mut nz_blocks = 0;
    for blk_i in 0..100 {
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
        if blk_i == 10 {
            let samples: Vec<i16> = block.as_chunks::<4>().0.iter()
                .map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            println!("WaveUser block 10 samples (first 16): {:?}", &samples[..16]);
        }
    }

    emu.set_key(12, 0);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }

    println!("WaveUser synthesis results: max_amp = {}, nz_blocks = {}", max_amp, nz_blocks);
    assert!(max_amp > 1000, "WaveUser should synthesize audio, got max_amp = {}", max_amp);
    assert!(nz_blocks > 50, "WaveUser should have sustained audio, got nz_blocks = {}", nz_blocks);
}
