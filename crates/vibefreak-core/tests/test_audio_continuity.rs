use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_audio_waveform_continuity() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let main_bin = repo_root.join("original_firmware/unpacked/nanowave_main_payload_stm32f446.bin");
    let qspi_bin = repo_root.join("original_firmware/unpacked/userwavetable_factory_payload_qspi.bin");
    let oled_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");
    let matrix_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
    let kbd_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !main_bin.exists() {
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");
    let presets = repo_root.join("user_presets.bin");
    if presets.exists() {
        let _ = emu.load_qspi_from_disk(&presets);
    }

    emu.boot(100_000_000);

    // Warm up
    for _ in 0..500 {
        emu.step_audio_block_quantum();
    }

    // Play Note 60
    emu.enqueue_midi_bytes(&[0x90, 60, 100]);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }

    // Collect 20 consecutive blocks of audio
    let mut all_samples: Vec<i16> = Vec::new();
    let mut block_starts: Vec<usize> = Vec::new();

    for _ in 0..20 {
        block_starts.push(all_samples.len());
        let blk = emu.step_audio_block_quantum();
        for chunk in blk.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            all_samples.push(left);
        }
    }

    println!("\n=== AUDIO CONTINUITY ANALYSIS (20 blocks = {} samples) ===", all_samples.len());
    // Check discontinuities at block boundaries
    let mut max_boundary_jump = 0i32;
    let mut max_internal_jump = 0i32;

    for (b_idx, &start) in block_starts.iter().enumerate().skip(1) {
        let prev_last = all_samples[start - 1];
        let curr_first = all_samples[start];
        let jump = (curr_first as i32 - prev_last as i32).abs();
        if jump > max_boundary_jump {
            max_boundary_jump = jump;
        }
        println!("Block {} boundary (end of blk {} to start of blk {}): sample {} -> {}, jump = {}",
            b_idx, b_idx - 1, b_idx, prev_last, curr_first, jump);
    }

    for window in all_samples.windows(2) {
        let diff = (window[1] as i32 - window[0] as i32).abs();
        if diff > max_internal_jump {
            max_internal_jump = diff;
        }
    }

    println!("Max internal consecutive sample difference: {}", max_internal_jump);
    println!("Max block boundary sample jump: {}", max_boundary_jump);

    // Print first 64 samples
    println!("First 64 samples: {:?}", &all_samples[..64]);
}

#[test]
fn test_key_touch_and_midi_latency() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let main_bin = repo_root.join("original_firmware/unpacked/nanowave_main_payload_stm32f446.bin");
    let qspi_bin = repo_root.join("original_firmware/unpacked/userwavetable_factory_payload_qspi.bin");
    let oled_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");
    let matrix_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
    let kbd_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !main_bin.exists() {
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");
    let presets = repo_root.join("user_presets.bin");
    if presets.exists() {
        let _ = emu.load_qspi_from_disk(&presets);
    }

    emu.boot(100_000_000);

    // Warm up until silence
    for _ in 0..500 {
        emu.step_audio_block_quantum();
    }

    // 1. Measure MIDI Note On Latency
    println!("\n=== MEASURING MIDI NOTE ON LATENCY ===");
    emu.enqueue_midi_bytes(&[0x90, 60, 100]);
    let mut midi_blocks = 0;
    for b in 0..200 {
        let blk = emu.step_audio_block_quantum();
        if blk.iter().any(|&s| s != 0) {
            midi_blocks = b;
            break;
        }
    }
    println!("MIDI Note On response: {} blocks ({} ms)",
        midi_blocks, (midi_blocks as f64 * 32.0 / 48.0));

    // Release MIDI Note and wait for silence
    emu.enqueue_midi_bytes(&[0x80, 60, 0]);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }

    // 2. Measure Capacitive Keybed Touch Latency
    println!("\n=== MEASURING CAPACITIVE KEYBED TOUCH LATENCY ===");
    emu.set_key(12, 100); // Key 12 (Middle C)
    let mut kbd_blocks = 0;
    for b in 0..200 {
        let blk = emu.step_audio_block_quantum();
        if blk.iter().any(|&s| s != 0) {
            kbd_blocks = b;
            break;
        }
    }
    println!("Keybed Touch response: {} blocks ({} ms)",
        kbd_blocks, (kbd_blocks as f64 * 32.0 / 48.0));
}

#[test]
fn test_multikey_polyphony_and_keybed_response() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let main_bin = repo_root.join("original_firmware/unpacked/nanowave_main_payload_stm32f446.bin");
    let qspi_bin = repo_root.join("original_firmware/unpacked/userwavetable_factory_payload_qspi.bin");
    let oled_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");
    let matrix_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
    let kbd_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !main_bin.exists() {
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");
    let presets = repo_root.join("user_presets.bin");
    if presets.exists() {
        let _ = emu.load_qspi_from_disk(&presets);
    }

    emu.boot(100_000_000);

    for _ in 0..500 {
        emu.step_audio_block_quantum();
    }

    // Test holding 3 keys for 100 blocks, then pressing 4th key
    for k in 0..25 { emu.set_key(k, 0); }
    for _ in 0..100 { emu.step_audio_block_quantum(); }

    emu.set_key(12, 100);
    emu.set_key(16, 100);
    emu.set_key(19, 100);
    for _ in 0..100 { emu.step_audio_block_quantum(); }

    let mut buf_state = [0u8; 8];
    let _ = emu.uc.mem_read(0x2001E004 + 0x10, &mut buf_state);
    let h1 = u32::from_le_bytes(buf_state[0..4].try_into().unwrap());
    let t1 = u32::from_le_bytes(buf_state[4..8].try_into().unwrap());
    let count1 = (h1.wrapping_sub(t1)) & 0x3F;
    assert!(count1 < 32, "USART2 ring buffer saturated during 3-key hold: count={}", count1);

    // Press 4th key (key 23)
    emu.set_key(23, 100);
    for _ in 0..10 { emu.step_audio_block_quantum(); }

    let _ = emu.uc.mem_read(0x2001E004 + 0x10, &mut buf_state);
    let h2 = u32::from_le_bytes(buf_state[0..4].try_into().unwrap());
    let t2 = u32::from_le_bytes(buf_state[4..8].try_into().unwrap());
    let count2 = (h2.wrapping_sub(t2)) & 0x3F;
    assert!(count2 < 32, "USART2 ring buffer saturated on 4th key press: count={}", count2);

    // Release key 23
    emu.set_key(23, 0);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }

    let _ = emu.uc.mem_read(0x2001E004 + 0x10, &mut buf_state);
    let h3 = u32::from_le_bytes(buf_state[0..4].try_into().unwrap());
    let t3 = u32::from_le_bytes(buf_state[4..8].try_into().unwrap());
    let count3 = (h3.wrapping_sub(t3)) & 0x3F;
    assert!(count3 < 32, "USART2 ring buffer saturated on key release while holding keys: count={}", count3);

    // Release all keys and verify buffer completely drains
    for k in 0..25 { emu.set_key(k, 0); }
    for _ in 0..50 { emu.step_audio_block_quantum(); }

    let _ = emu.uc.mem_read(0x2001E004 + 0x10, &mut buf_state);
    let h4 = u32::from_le_bytes(buf_state[0..4].try_into().unwrap());
    let t4 = u32::from_le_bytes(buf_state[4..8].try_into().unwrap());
    let count4 = (h4.wrapping_sub(t4)) & 0x3F;
    assert_eq!(count4, 0, "USART2 ring buffer did not drain cleanly when all keys released: count={}", count4);
}

#[test]
fn test_user_scenario_hold_0_12_and_press_9_11_21_23_24() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let main_bin = repo_root.join("original_firmware/unpacked/nanowave_main_payload_stm32f446.bin");
    let qspi_bin = repo_root.join("original_firmware/unpacked/userwavetable_factory_payload_qspi.bin");
    let oled_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");
    let matrix_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
    let kbd_bin = repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !main_bin.exists() {
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");
    let presets = repo_root.join("user_presets.bin");
    if presets.exists() {
        let _ = emu.load_qspi_from_disk(&presets);
    }

    emu.boot(100_000_000);

    for _ in 0..500 {
        emu.step_audio_block_quantum();
    }

    // Release all first
    for k in 0..25 { emu.set_key(k, 0); }
    for _ in 0..100 { emu.step_audio_block_quantum(); }

    println!("\n=== USER SCENARIO: Hold Key 0 (lowest C) and Key 12 (middle C) ===");
    emu.set_key(0, 100);
    for _ in 0..50 { emu.step_audio_block_quantum(); }
    emu.set_key(12, 100);
    for _ in 0..50 { emu.step_audio_block_quantum(); }

    let test_keys = [9, 11, 21, 23, 24];
    for &k in &test_keys {
        println!("--- Testing pressing key {} while holding 0 and 12 ---", k);
        emu.set_key(k, 100);
        let mut audio_samples = Vec::new();
        for _ in 0..30 {
            let blk = emu.step_audio_block_quantum();
            audio_samples.extend_from_slice(&blk);
        }
        let mut buf_state = [0u8; 8];
        let _ = emu.uc.mem_read(0x2001E004 + 0x10, &mut buf_state);
        let h = u32::from_le_bytes(buf_state[0..4].try_into().unwrap());
        let t = u32::from_le_bytes(buf_state[4..8].try_into().unwrap());
        let count = (h.wrapping_sub(t)) & 0x3F;
        let non_zero = audio_samples.iter().filter(|&&b| b != 0).count();
        assert!(non_zero > 0, "Key {} must produce audio while holding 0 and 12", k);
        assert!(count < 32, "Ring buffer must not overflow for key {}", k);

        emu.set_key(k, 0);
        for _ in 0..30 { emu.step_audio_block_quantum(); }
    }
}


