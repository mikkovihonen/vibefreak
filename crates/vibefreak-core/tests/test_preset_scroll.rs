use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_scroll_past_preset_6() {
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

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");

    if presets_path.exists() {
        println!("Loading presets from {:?}", presets_path);
        emu.load_qspi_from_disk(&presets_path).unwrap();
    }

    emu.boot(100_000_000);

    // Boot until active preset screen
    for i in 0..1500 {
        emu.step_audio_block_quantum();
        let fb = emu.coproc.get_framebuffer();
        let nonzero = fb.iter().filter(|&&b| b != 0).count();
        if nonzero > 400 {
            println!("Reached active screen at block {}", i);
            break;
        }
    }

    // Step preset encoder forward through presets 2 to 16
    for target_preset in 2..=16 {
        emu.step_preset_encoder(1);
        for _block in 0..100 {
            emu.step_audio_block_quantum();
            let pc = emu.pc;
            if pc == 0 || pc > 0x0810_0000 || pc < 0x0800_C000 {
                panic!("Crash at target preset {}: PC = 0x{:08X}", target_preset, pc);
            }
        }
        let fb = emu.coproc.get_framebuffer();
        let nonzero = fb.iter().filter(|&&b| b != 0).count();
        println!("Preset {}: non-zero framebuffer bytes = {}, PC = 0x{:08X}", target_preset, nonzero, emu.pc);
        assert!(nonzero > 300, "Preset {} must render active OLED framebuffer", target_preset);
    }
}
