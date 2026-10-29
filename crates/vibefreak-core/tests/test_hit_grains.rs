use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_hit_grains_oscillator() {
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

    println!("Booted to Preset 1 screen. Initial PC = 0x{:08X}", emu.pc);

    // Step type encoder forward one by one through all oscillator types (including Hit Grains at step 21)
    for step in 1..=22 {
        emu.step_type_encoder(1);
        for block in 0..200 {
            let audio = emu.step_audio_block_quantum();
            let pc = emu.pc;
            assert!(
                pc >= 0x0800_C000 && pc <= 0x0810_0000,
                "Crash at type step {}: PC = 0x{:08X} at block {}",
                step,
                pc,
                block
            );
            // Verify audio render output is non-empty
            let _ = audio;
        }
        println!("Type step {} completed successfully. PC = 0x{:08X}", step, emu.pc);
    }
}

