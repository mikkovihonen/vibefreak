use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_factory_preset_volume_initialization() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let main_bin = repo_root.join("original_firmware/unpacked/nanowave_main_payload_stm32f446.bin");
    let oled_bin =
        repo_root.join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");
    let matrix_bin =
        repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
    let kbd_bin =
        repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !main_bin.exists() {
        eprintln!("Skipping test: stock firmware binary not found");
        return;
    }

    let qspi_bin =
        repo_root.join("original_firmware/unpacked/userwavetable_factory_payload_qspi.bin");

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");

    // Boot with built-in stock patches and factory wavetables
    emu.boot(120_000_000);

    let mut preset_ready = false;
    for i in 0..3000 {
        emu.step_audio_block_quantum();
        if i % 100 == 0 {
            let fb = emu.coproc.get_framebuffer();
            let nonzero = fb.iter().filter(|&&b| b != 0).count();
            if nonzero > 400 {
                eprintln!("Preset 1 screen reached at block {}", i);
                preset_ready = true;
                break;
            }
        }
    }
    assert!(preset_ready, "Preset 1 screen must be reached");

    // Verify Amp Mod LED is ON (Panel LED index 32)
    let panel_leds = emu.get_panel_leds();
    eprintln!("Panel LED 32 (Amp Mod): {}", panel_leds[32]);
    assert_eq!(panel_leds[32], 255, "Amp Mod LED must be ON initially");

    // Warm up audio pipeline
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }

    // Play Key 12 (Middle C) with pressure 200
    emu.set_key(12, 200);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }

    let mut on_peaks = Vec::new();
    for _ in 0..20 {
        let blk = emu.step_audio_block_quantum();
        let mut peak = 0i16;
        for chunk in blk.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > peak.abs() {
                peak = left;
            }
        }
        on_peaks.push(peak);
    }
    let max_on = on_peaks.iter().map(|p| p.abs()).max().unwrap_or(0);
    eprintln!("Note On peaks (20 blocks): {:?}", on_peaks);
    eprintln!("Max Note On peak amplitude: {}", max_on);
    assert!(max_on > 1000, "Factory preset must generate audible synth tone on Note On");

    // Release Key 12
    emu.set_key(12, 0);
    let mut off_peaks = Vec::new();
    let mut last_ccr1 = 8191u16;
    let mut spike_detected = false;

    for i in 0..3500 {
        let blk = emu.step_audio_block_quantum();
        let mut peak = 0i16;
        for chunk in blk.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > peak.abs() {
                peak = left;
            }
        }
        let tim14_ccr1 = emu.uc.get_data().mmio.timers.timers[13].ccr1 as u16;
        let vca_gain = emu.audio_proc.vca.get_gain();
        if i % 250 == 0 || i == 3499 {
            eprintln!("Release step {}: peak = {}, tim14_ccr1 = {}, vca_gain = {}", i, peak, tim14_ccr1, vca_gain);
        }
        // Check for double-trigger spike (gain jumping back up by more than 100 during release)
        if tim14_ccr1 > last_ccr1 + 100 {
            eprintln!("ERROR: Double trigger gain spike detected at step {}: ccr1 jumped from {} to {}", i, last_ccr1, tim14_ccr1);
            spike_detected = true;
        }
        last_ccr1 = tim14_ccr1;
        off_peaks.push(peak);
    }
    assert!(!spike_detected, "Double trigger gain spike must not occur during decay");
    let final_peak = off_peaks.last().unwrap_or(&-1);
    eprintln!("Note Off final peak amplitude: {}", final_peak);
    assert!(final_peak.abs() <= 1, "Envelope must decay to silence after note release, got {}", final_peak);
}
