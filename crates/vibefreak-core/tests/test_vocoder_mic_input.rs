use vibefreak_core::peripherals::tlv320aic3104::HeadsetJackState;
use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_vocoder_microphone_audio_input_modulation() {
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
        eprintln!("Skipping test: main firmware binary not found");
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");
    emu.set_headset_jack(HeadsetJackState::HeadsetWithMic);

    // Build and load a dedicated Vocoder preset into Slot 1
    let vocoder_patch = vibefreak_core::factory_data::FactoryPatchDef {
        name: "Vocoder Test",
        engine_type: 14,
        cutoff: 0x7FFF,
        resonance: 0,
        filter_mode: 0,
        attack: 10,
        decay: 1500,
        sustain: 0x7FFF,
    };
    let p_data = vibefreak_core::factory_data::build_factory_preset(1, "Vocoder Test", Some(&vocoder_patch));
    let mut flash = vibefreak_core::factory_data::generate_factory_qspi_image();
    flash[0x00081000..0x00081000 + 4096].copy_from_slice(&p_data);
    let temp_qspi = std::env::temp_dir().join("test_vocoder_qspi.bin");
    std::fs::write(&temp_qspi, &flash).unwrap();
    emu.load_qspi_from_disk(&temp_qspi).unwrap();
    let _ = std::fs::remove_file(&temp_qspi);


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



    // 1. Verify DMA2 Stream 6 (SAI2_B Audio RX) and Stream 4 (SAI2_A Audio TX) are active
    let s4 = &emu.get_dma2().streams[4];
    assert_ne!(s4.cr & 1, 0, "DMA2 Stream 4 (SAI2_A TX) must be enabled");
    let s6 = &emu.get_dma2().streams[6];
    assert_ne!(s6.cr & 1, 0, "DMA2 Stream 6 (SAI2_B RX) must be enabled");

    // 2. Test Vocoder output WITHOUT mic input (silence / envelope gated)
    emu.clear_audio_input();
    emu.set_key(12, 200);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }
    let mut silent_max_amp = 0i16;
    for _ in 0..100 {
        let block = emu.step_audio_block_quantum();
        for chunk in block.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > silent_max_amp {
                silent_max_amp = left.abs();
            }
        }
    }
    emu.set_key(12, 0);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }

    // 3. Test Vocoder output WITH audio/mic input (generate a 440 Hz rich audio signal)
    emu.set_key(12, 200);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }

    let mut modulated_max_amp = 0i16;
    let mut modulated_nz_blocks = 0;
    let sample_rate = 48000.0f32;
    let freq = 440.0f32;
    let mut phase = 0.0f32;

    for _ in 0..150 {
        // Feed 32 frames of 440 Hz sine wave into mic input FIFO (AUDIO_BLOCK_FRAMES = 32)
        let mut mic_frames = [0.0f32; 32];
        for frame in &mut mic_frames {
            *frame = (phase * 2.0 * std::f32::consts::PI).sin() * 0.8;
            phase += freq / sample_rate;
            if phase >= 1.0 {
                phase -= 1.0;
            }
        }
        emu.feed_audio_input(&mic_frames);

        let block = emu.step_audio_block_quantum();
        let mut blk_peak = 0i16;
        for chunk in block.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > blk_peak.abs() {
                blk_peak = left.abs();
            }
        }
        if blk_peak.abs() > 0 {
            modulated_nz_blocks += 1;
        }
        if blk_peak.abs() > modulated_max_amp {
            modulated_max_amp = blk_peak.abs();
        }
    }

    emu.set_key(12, 0);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }

    assert!(
        modulated_max_amp > 1000,
        "Vocoder modulated output must produce audio (got max_amp = {})",
        modulated_max_amp
    );
    assert!(
        modulated_nz_blocks > 50,
        "Vocoder output must produce non-zero blocks continuously during mic input (got {} non-zero blocks)",
        modulated_nz_blocks
    );
    assert!(
        modulated_max_amp > silent_max_amp,
        "Vocoder output with mic input ({}) must be significantly louder than with silent mic ({})",
        modulated_max_amp, silent_max_amp
    );
}

