use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_matrix_and_preset_microswitch_actions() {
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
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");

    eprintln!("[1] Booting emulator...");
    emu.boot(120_000_000);



    // Step to initial patch screen
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
    }

    let fb_initial = emu.coproc.get_framebuffer();
    let leds_initial = emu.coproc.get_matrix_leds();
    eprintln!("[2] Initial patch screen: nonzero fb={}, lit matrix leds={}",
        fb_initial.iter().filter(|&&b| b != 0).count(),
        leds_initial.iter().filter(|&&b| b != 0).count(),
    );
    eprintln!("  Matrix CPU PC=0x{:08X}, FSM=0x{:02X}, GPIOA_IDR=0x{:08X}",
        emu.coproc.matrix.cpu.pc(),
        emu.coproc.matrix.cpu.sram[0x5CD],
        emu.coproc.matrix.cpu.gpioa_idr,
    );
    eprintln!("  Matrix CPU USART1 TX len={}", emu.coproc.matrix.cpu.usart1.tx.len());

    let mut sram_mat = [0u8; 16];
    let _ = emu.uc.mem_read(0x2000_35A4, &mut sram_mat);
    let mut sram_pre = [0u8; 16];
    let _ = emu.uc.mem_read(0x2000_3714, &mut sram_pre);
    eprintln!("[2b] SRAM matrix obj={:02X?}, preset obj={:02X?}", sram_mat, sram_pre);

    // 1. Test Preset Encoder step and click on Main Patch Screen:
    let mut preset_bytes_before = [0u8; 2];
    let _ = emu.uc.mem_read(0x2000_0BDC, &mut preset_bytes_before);
    let preset_before = u16::from_le_bytes(preset_bytes_before);
    eprintln!("[3] Stepping Preset Encoder delta=+1 (Preset before = {})...", preset_before);
    let fb_before_preset_step = emu.coproc.get_framebuffer();
    emu.step_preset_encoder(1);
    for _ in 0..300 {
        emu.step_audio_block_quantum();
    }
    let mut preset_bytes_after = [0u8; 2];
    let _ = emu.uc.mem_read(0x2000_0BDC, &mut preset_bytes_after);
    let preset_after = u16::from_le_bytes(preset_bytes_after);
    let fb_after_preset_step = emu.coproc.get_framebuffer();
    eprintln!("[3b] Preset after delta=+1: {}, fb_changed={}", preset_after, fb_before_preset_step != fb_after_preset_step);
    assert_eq!(preset_after, preset_before + 1, "Preset encoder step must increment preset index");
    assert!(fb_before_preset_step != fb_after_preset_step, "Preset encoder step must update OLED display");

    eprintln!("[4] Actuating Preset Encoder push microswitch (press)...");
    emu.set_microswitch("preset", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }

    eprintln!("[5] Actuating Preset Encoder push microswitch (release)...");
    emu.set_microswitch("preset", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }

    let fb_after_preset = emu.coproc.get_framebuffer();
    eprintln!("[5b] After Preset click: nonzero fb={}", fb_after_preset.iter().filter(|&&b| b != 0).count());

    // 2. Test Matrix Encoder click:
    eprintln!("[6] Actuating Matrix Encoder push microswitch (press)...");
    emu.set_microswitch("matrix", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    let _ = emu.uc.mem_read(0x2000_35A4, &mut sram_mat);
    eprintln!("[6b] After press: SRAM matrix obj={:02X?}", sram_mat);

    eprintln!("[7] Actuating Matrix Encoder push microswitch (release)...");
    emu.set_microswitch("matrix", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let _ = emu.uc.mem_read(0x2000_35A4, &mut sram_mat);
    eprintln!("[7b] After release: SRAM matrix obj={:02X?}", sram_mat);
    assert_eq!(sram_mat[4], 0, "Matrix mode must transition to 0 (Amount Edit Mode) on click release");

    let fb_after_matrix = emu.coproc.get_framebuffer();
    let leds_after_matrix = emu.coproc.get_matrix_leds();
    eprintln!("[8] After Matrix click: nonzero fb={}, lit matrix leds={}, fb_changed={}",
        fb_after_matrix.iter().filter(|&&b| b != 0).count(),
        leds_after_matrix.iter().filter(|&&b| b != 0).count(),
        fb_after_matrix != fb_after_preset,
    );
    assert_eq!(leds_initial.iter().filter(|&&b| b != 0).count(), 0, "Initial matrix LEDs must be off");
    assert_eq!(leds_after_matrix.iter().filter(|&&b| b != 0).count(), 1, "Matrix encoder click must light up selected matrix point LED");
    assert!(fb_after_matrix != fb_after_preset, "Matrix encoder click must open modulation amount screen");

    // 2b. Test Matrix Encoder stepping in Amount Edit Mode:
    eprintln!("[9] Stepping Matrix Encoder delta=+1 in Amount Edit Mode...");
    let fb_before_amount_step = emu.coproc.get_framebuffer();
    emu.step_matrix_encoder(1);
    for _ in 0..250 {
        emu.step_audio_block_quantum();
    }
    let fb_after_amount_step = emu.coproc.get_framebuffer();
    eprintln!("[9b] After Matrix amount step: fb_changed={}", fb_before_amount_step != fb_after_amount_step);
    assert!(fb_before_amount_step != fb_after_amount_step, "Matrix encoder step in Amount Edit Mode must update OLED modulation amount display");

    // 2c. Test Matrix Encoder second click to toggle back to navigation mode:
    eprintln!("[10] Clicking Matrix Encoder again to exit amount edit mode...");
    emu.set_microswitch("matrix", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("matrix", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let _ = emu.uc.mem_read(0x2000_35A4, &mut sram_mat);
    eprintln!("[10b] After second click: SRAM matrix obj={:02X?}", sram_mat);
    assert_eq!(sram_mat[4], 1, "Matrix mode must transition back to 1 (Navigation Mode) on second click");

    // 3. Test Raw Pin and Encoder Dispatch for Matrix and Preset:
    eprintln!("[11] Testing raw pin dispatch for Matrix push switch (PA5, Group 0x09)...");
    let mode_before_raw = sram_mat[4];
    emu.set_raw_pin('A', 5, 0x09, true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_raw_pin('A', 5, 0x09, false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let _ = emu.uc.mem_read(0x2000_35A4, &mut sram_mat);
    let mode_after_raw = sram_mat[4];
    eprintln!("[11b] After raw Matrix push switch: mode before={}, after={}",
        mode_before_raw, mode_after_raw
    );
    assert_ne!(
        mode_before_raw,
        mode_after_raw,
        "Raw pin toggle on PA5 (Group 0x09) must toggle matrix modulation edit state"
    );

    eprintln!("[12] Testing Octave Down and Up microswitch press/release hooks...");
    let fb_before_oct = emu.coproc.get_framebuffer();
    let pleds_before_oct = emu.coproc.get_panel_leds();
    eprintln!("  Oct LEDs before: Down={}, Up={}", pleds_before_oct[24], pleds_before_oct[25]);
    eprintln!("  Actuating Octave Down (press)...");
    emu.set_microswitch("oct_down", true);
    for _ in 0..100 { emu.step_audio_block_quantum(); }
    eprintln!("  Actuating Octave Down (release)...");
    emu.set_microswitch("oct_down", false);
    let mut saw_down_led_on = false;
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_panel_leds()[24] == 255 {
            saw_down_led_on = true;
            break;
        }
    }
    let pleds_after_down = emu.coproc.get_panel_leds();
    let fb_after_down = emu.coproc.get_framebuffer();
    assert!(saw_down_led_on, "Octave Down LED must turn on when oct_down is pressed");
    assert_ne!(fb_before_oct, fb_after_down, "OLED framebuffer must update with Octave display");
    assert_eq!(pleds_after_down[25], 0, "Octave Up LED must remain off when transposed down");

    // Read parameter 0x0301 from g_ActivePresetAndSequencer (0x2001C898)
    let mut param_ptr_bytes = [0u8; 4];
    let _ = emu.uc.mem_read(0x2001C898 + 12 + 0x2C, &mut param_ptr_bytes);
    let param_ptr = u32::from_le_bytes(param_ptr_bytes);
    let mut param_val = [0u8; 2];
    let _ = emu.uc.mem_read((param_ptr + 2) as u64, &mut param_val);
    let octave_down_val = i16::from_le_bytes(param_val);
    assert_eq!(octave_down_val, 10922, "Octave parameter 0x0301 must decrement to -1 octave (10922)");

    eprintln!("  Actuating Octave Up (press to return to center)...");
    emu.set_microswitch("oct_up", true);
    for _ in 0..100 { emu.step_audio_block_quantum(); }
    eprintln!("  Actuating Octave Up (release)...");
    emu.set_microswitch("oct_up", false);
    for _ in 0..200 { emu.step_audio_block_quantum(); }
    let pleds_after_up = emu.coproc.get_panel_leds();
    let fb_after_up = emu.coproc.get_framebuffer();
    assert_eq!(pleds_after_up[24], 0, "Octave Down LED must be off at center octave (0)");
    assert_eq!(pleds_after_up[25], 0, "Octave Up LED must be off at center octave (0)");
    assert_ne!(fb_after_down, fb_after_up, "OLED framebuffer must update when returning to center");

    let _ = emu.uc.mem_read((param_ptr + 2) as u64, &mut param_val);
    let octave_center_val = i16::from_le_bytes(param_val);
    assert_eq!(octave_center_val, 16384, "Octave parameter 0x0301 must return to center 0 (16384)");

    eprintln!("  Actuating Octave Up (press to reach +1 octave)...");
    emu.set_microswitch("oct_up", true);
    for _ in 0..100 { emu.step_audio_block_quantum(); }
    eprintln!("  Actuating Octave Up (release)...");
    emu.set_microswitch("oct_up", false);
    let mut saw_up_led_on = false;
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_panel_leds()[25] == 255 {
            saw_up_led_on = true;
            break;
        }
    }
    let pleds_after_up2 = emu.coproc.get_panel_leds();
    let fb_after_up2 = emu.coproc.get_framebuffer();
    assert!(saw_up_led_on, "Octave Up LED must turn on when transposed up (+1)");
    assert_eq!(pleds_after_up2[24], 0, "Octave Down LED must remain off when transposed up");
    assert_ne!(fb_after_up, fb_after_up2, "OLED framebuffer must update when reaching +1 octave");

    let _ = emu.uc.mem_read((param_ptr + 2) as u64, &mut param_val);
    let octave_up_val = i16::from_le_bytes(param_val);
    assert_eq!(octave_up_val, 21845, "Octave parameter 0x0301 must increment to +1 octave (21845)");
}

#[test]
fn test_preset_encoder_utility_navigation() {
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

    emu.boot(120_000_000);

    // Step to initial patch screen
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
    }

    let fb_patch = emu.coproc.get_framebuffer();

    // 1. Open Utility menu
    emu.set_microswitch("utility", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("utility", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }

    let fb_util = emu.coproc.get_framebuffer();
    let leds_util = emu.coproc.get_panel_leds();
    assert_eq!(leds_util[30], 255, "Utility LED must be lit");
    assert!(fb_patch != fb_util, "Utility menu must change OLED framebuffer");

    // 2. Turn Preset encoder to scroll utility items
    let fb_before_scroll = emu.coproc.get_framebuffer();
    emu.step_preset_encoder(1);
    for _ in 0..300 {
        emu.step_audio_block_quantum();
    }
    let fb_after_scroll = emu.coproc.get_framebuffer();
    assert!(fb_before_scroll != fb_after_scroll, "Turning Preset encoder in Utility menu must scroll and update OLED");

    // 3. Click Preset encoder to enter submenu / item
    let fb_before_click = emu.coproc.get_framebuffer();
    emu.set_microswitch("preset", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("preset", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let fb_after_click = emu.coproc.get_framebuffer();
    assert!(fb_before_click != fb_after_click, "Clicking Preset encoder in Utility menu must enter submenu and update OLED");

    // 4. Turn Preset encoder inside submenu
    emu.step_preset_encoder(1);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }

    // 5. Click Preset encoder inside submenu to toggle parameter
    let fb_before_sub_click = emu.coproc.get_framebuffer();
    emu.set_microswitch("preset", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("preset", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let fb_after_sub_click = emu.coproc.get_framebuffer();
    assert!(fb_before_sub_click != fb_after_sub_click, "Clicking Preset encoder in submenu must toggle/edit parameter and update OLED");
}
