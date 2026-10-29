use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_stock_firmware_boot_and_framebuffer() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let main_bin = repo_root.join("original_firmware/unpacked/nanowave_main_payload_stm32f446.bin");
    let qspi_bin =
        repo_root.join("original_firmware/unpacked/userwavetable_factory_payload_qspi.bin");
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

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");

    let t0 = std::time::Instant::now();
    emu.boot(120_000_000);
    eprintln!("Boot completed in {:?}", t0.elapsed());


    let mut preset_reached = false;
    for i in 0..10000 {
        let blk = emu.step_audio_block_quantum();
        assert_eq!(blk.len(), 128);

        if i % 500 == 0 {
            let fb = emu.coproc.get_framebuffer();
            let nonzero = fb.iter().filter(|&&b| b != 0).count();
            if nonzero > 400 {
                preset_reached = true;
                break;
            }
        }
    }

    let fb = emu.coproc.get_framebuffer();
    let nonzero = fb.iter().filter(|&&b| b != 0).count();
    assert!(
        preset_reached && nonzero > 400,
        "Firmware must transition from splash screen to Preset 1 screen (nonzero bytes > 400, got {})",
        nonzero
    );
}

#[test]
fn test_open_firmware_boot() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let main_bin = repo_root.join("build/nanowave_main_payload_stm32f446.bin");
    let qspi_bin = repo_root.join("build/userwavetable_factory_payload_qspi.bin");
    let oled_bin = repo_root.join("build/nanowave_iochip_oled_payload_stm32f0.bin");
    let matrix_bin = repo_root.join("build/nanowave_iochip_matrix_payload_stm32f0.bin");
    let kbd_bin = repo_root.join("build/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !main_bin.exists() {
        eprintln!("Skipping test: open firmware binary not found in build/");
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator with open-source firmware");

    let t0 = std::time::Instant::now();
    emu.boot(10_000_000);
    eprintln!("Open firmware booted to PC=0x{:08X} in {:?}", emu.pc, t0.elapsed());
    assert!(emu.pc >= 0x0800_0000, "PC must be in flash memory space");
}

