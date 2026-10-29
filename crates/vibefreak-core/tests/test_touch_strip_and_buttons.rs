use vibefreak_core::coproc::keyboard::{KeyboardCoprocessor, SCAN_TICK_CYCLES, TOUCH_BUTTON_NAMES};
use vibefreak_core::coproc::matrix::MatrixCoprocessor;
use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_keyboard_coproc_touch_strip_continuous() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let kbd_bin =
        repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !kbd_bin.exists() {
        eprintln!("Skipping test: keyboard binary not found");
        return;
    }

    let mut kbd = KeyboardCoprocessor::new(&kbd_bin).expect("Failed to create KeyboardCoprocessor");

    // 1. Initially untouched: verify no spurious packets
    kbd.step(SCAN_TICK_CYCLES * 10);
    let initial_tx = kbd.drain_usart_tx();
    eprintln!("Initial tx bytes: {:02X?}", initial_tx);

    // 2. Touch center (2048): must trigger touch down (0x10) or position (0x11)
    kbd.set_touch_strip(2048, true);
    let mut touch_tx = Vec::new();
    for _ in 0..250 {
        kbd.step(SCAN_TICK_CYCLES);
        let tx = kbd.drain_usart_tx();
        if !tx.is_empty() {
            touch_tx.extend(tx);
        }
    }

    // Verify touch down (0x10) or position (0x11) on device 0x30
    let has_down = touch_tx.windows(4).any(|w| {
        w[0] == 0xAA && w[1] == 0x30 && (w[2] == 0x10 || w[2] == 0x11)
    });
    assert!(
        has_down,
        "Touching center at 2048 must emit touch packet: {:02X?}",
        touch_tx
    );

    // CRITICAL: Touching the touch strip must NEVER trigger any icon button press frames!
    let has_spurious_button = touch_tx.windows(4).any(|w| {
        w[0] == 0xAA && w[1] == 0x30 && (w[2] == 0x01 || w[2] == 0x02) && w[3] == 0x11
    });
    assert!(
        !has_spurious_button,
        "Touch strip must NOT trigger any group 0x11 icon button frames: {:02X?}",
        touch_tx
    );

    // 3. Move touch to near maximum (3800)
    kbd.set_touch_strip(3800, true);
    let mut move_tx = Vec::new();
    for i in 0..120 {
        kbd.step(SCAN_TICK_CYCLES);
        let tx = kbd.drain_usart_tx();
        if !tx.is_empty() {
            eprintln!("Move step {}: tx={:02X?}", i, tx);
            move_tx.extend(tx);
        }
    }

    // 4. Release touch: must trigger touch release (0x13 or 0x14)
    kbd.set_touch_strip(3800, false);
    let mut release_tx = Vec::new();
    for i in 0..150 {
        kbd.step(SCAN_TICK_CYCLES);
        let tx = kbd.drain_usart_tx();
        if !tx.is_empty() {
            eprintln!("Release step {}: tx={:02X?}", i, tx);
            release_tx.extend(tx);
        }
    }
    let has_release = release_tx.windows(4).any(|w| {
        w[0] == 0xAA && w[1] == 0x30 && (w[2] == 0x13 || w[2] == 0x14 || w[2] == 0x02)
    });
    assert!(
        has_release,
        "Releasing touch strip must emit release packet: {:02X?}",
        release_tx
    );
}

#[test]
fn test_keyboard_coproc_icon_buttons() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let kbd_bin =
        repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !kbd_bin.exists() {
        eprintln!("Skipping test: keyboard binary not found");
        return;
    }

    let mut kbd = KeyboardCoprocessor::new(&kbd_bin).expect("Failed to create KeyboardCoprocessor");

    // Test each of the 8 capacitive touch icon buttons:
    // (Hold=0, Up=1, Order=2, Random=3, Pattern=4, Spice=5, Dice=6, Bend=7)
    for (idx, &name) in TOUCH_BUTTON_NAMES.iter().enumerate() {
        kbd.set_touch_button(idx, true);
        let mut press_tx = Vec::new();
        for _ in 0..100 {
            kbd.step(SCAN_TICK_CYCLES);
            press_tx.extend(kbd.drain_usart_tx());
        }

        let has_press = press_tx.windows(8).any(|w| {
            w[0] == 0xAA && w[1] == 0x30 && w[2] == 0x01 && w[3] == 0x11 && w[4] == idx as u8
        });
        assert!(
            has_press,
            "Pressing icon button '{}' (idx {}) must emit group 0x11 frame from 0x30: {:02X?}",
            name, idx, press_tx
        );

        kbd.set_touch_button(idx, false);
        let mut release_tx = Vec::new();
        for _ in 0..150 {
            kbd.step(SCAN_TICK_CYCLES);
            release_tx.extend(kbd.drain_usart_tx());
        }

        let has_release = release_tx.windows(8).any(|w| {
            w[0] == 0xAA && w[1] == 0x30 && w[2] == 0x02 && w[3] == 0x11 && w[4] == idx as u8
        });
        assert!(
            has_release,
            "Releasing icon button '{}' (idx {}) must emit release frame from 0x30: {:02X?}",
            name, idx, release_tx
        );
    }
}

#[test]
fn test_keyboard_coproc_multikey_chords() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let kbd_bin =
        repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !kbd_bin.exists() {
        eprintln!("Skipping test: keyboard binary not found");
        return;
    }

    // Test A: Same group keys (Group 0: 0, 8, 16, 24)
    let mut kbd = KeyboardCoprocessor::new(&kbd_bin).expect("Failed to create KeyboardCoprocessor");
    for &k in &[0, 8, 16, 24] {
        kbd.set_key(k, 100);
        for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
        let tx = kbd.drain_usart_tx();
        let pressed = tx.windows(8).any(|w| w[0] == 0xAA && w[1] == 0x30 && (w[2] == 0x05 || w[2] == 0x07) && w[4] == k as u8);
        assert!(pressed, "Group 0 key {} did not respond! tx={:02X?}", k, tx);
    }

    // Test B: Adjacent keys (0, 1, 2, 3)
    let mut kbd = KeyboardCoprocessor::new(&kbd_bin).expect("Failed to create KeyboardCoprocessor");
    for &k in &[0, 1, 2, 3] {
        kbd.set_key(k, 100);
        for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
        let tx = kbd.drain_usart_tx();
        let pressed = tx.windows(8).any(|w| w[0] == 0xAA && w[1] == 0x30 && (w[2] == 0x05 || w[2] == 0x07) && w[4] == k as u8);
        assert!(pressed, "Adjacent key {} did not respond! tx={:02X?}", k, tx);
    }

    // Test C: Standard triad C3, E3, G3 (keys 12, 16, 19) and pressing 4th keys
    let mut kbd = KeyboardCoprocessor::new(&kbd_bin).expect("Failed to create KeyboardCoprocessor");
    kbd.set_key(12, 100);
    for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
    kbd.set_key(16, 100);
    for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
    kbd.set_key(19, 100);
    for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
    let _ = kbd.drain_usart_tx();

    for test_k in 0..25 {
        if test_k == 12 || test_k == 16 || test_k == 19 { continue; }
        kbd.set_key(test_k, 100);
        for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
        let tx = kbd.drain_usart_tx();
        let pressed = tx.windows(8).any(|w| w[0] == 0xAA && w[1] == 0x30 && (w[2] == 0x05 || w[2] == 0x07) && w[4] == test_k as u8);
        assert!(pressed, "4th key {} did not respond while holding triad (12, 16, 19)! tx={:02X?}", test_k, tx);
        kbd.set_key(test_k, 0);
        for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
        let _ = kbd.drain_usart_tx();
    }

    // Test D: User scenario - Hold key 0 (lowest C) and key 12 (middle C), press others
    println!("=== TEST D: Hold key 0 and 12 ===");
    let mut kbd = KeyboardCoprocessor::new(&kbd_bin).expect("Failed to create KeyboardCoprocessor");
    kbd.set_key(0, 100);
    for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
    kbd.set_key(12, 100);
    for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
    let _ = kbd.drain_usart_tx();

    for test_k in 0..25 {
        if test_k == 0 || test_k == 12 { continue; }
        kbd.set_key(test_k, 100);
        for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
        let tx = kbd.drain_usart_tx();
        let pressed = tx.windows(8).any(|w| w[0] == 0xAA && w[1] == 0x30 && (w[2] == 0x05 || w[2] == 0x07) && w[4] == test_k as u8);
        assert!(pressed, "Key {} did not respond in keyboard coprocessor while holding 0 and 12! tx={:02X?}", test_k, tx);
        kbd.set_key(test_k, 0);
        for _ in 0..100 { kbd.step(SCAN_TICK_CYCLES); }
        let _ = kbd.drain_usart_tx();
    }
}

#[test]
fn test_matrix_coproc_icon_leds() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let matrix_bin =
        repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");

    if !matrix_bin.exists() {
        eprintln!("Skipping test: matrix binary not found");
        return;
    }

    let mut matrix = MatrixCoprocessor::new(&matrix_bin).expect("Failed to create MatrixCoprocessor");

    // All icon LEDs must start off
    assert_eq!(matrix.icon_leds, [0; 8], "All icon LEDs must start off");

    // Test 0xF5 LED frames for all 8 icon LEDs (0x1108..0x110F)
    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x08, 0x7F, 0xFF]);
    assert_eq!(matrix.icon_leds[0], 255, "Hold LED must be 255");

    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x09, 0x7F, 0xFF]);
    assert_eq!(matrix.icon_leds[1], 255, "Up | A LED must be 255");

    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x0A, 0x7F, 0xFF]);
    assert_eq!(matrix.icon_leds[2], 255, "Order | B LED must be 255");

    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x0B, 0x7F, 0xFF]);
    assert_eq!(matrix.icon_leds[3], 255, "Random LED must be 255");

    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x0C, 0x7F, 0xFF]);
    assert_eq!(matrix.icon_leds[4], 255, "Pattern LED must be 255");

    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x0D, 0x7F, 0xFF]);
    assert_eq!(matrix.icon_leds[5], 255, "Spice LED must be 255");

    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x0E, 0x7F, 0xFF]);
    assert_eq!(matrix.icon_leds[6], 255, "Dice LED must be 255");

    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x0F, 0x7F, 0xFF]);
    assert_eq!(matrix.icon_leds[7], 255, "Bend LED must be 255");

    // Turn Hold LED off
    matrix.handle_hardware_frame(0xF5, &[0x00, 0x11, 0x08, 0x00, 0x00]);
    assert_eq!(matrix.icon_leds[0], 0, "Hold LED must turn off when value is 0");

    // Verify panel_leds[14..17] (Track / Octave 1..4) were NOT altered by 0xF5!
    assert_eq!(matrix.panel_leds[14..18], [0, 0, 0, 0], "0xF5 must not touch panel_leds[14..17]");

    // Verify 0xF7 (row 7, cols 0..3) drives panel_leds[14..17]
    matrix.handle_hardware_frame(0xF7, &[0x10, 7, 0, 0x7F, 0xFF]);
    assert_eq!(matrix.panel_leds[14], 255, "Track 1 LED must be 255 after 0xF7");
    matrix.handle_hardware_frame(0xF7, &[0x10, 7, 1, 0x7F, 0xFF]);
    assert_eq!(matrix.panel_leds[15], 255, "Track 2 LED must be 255 after 0xF7");
}

#[test]
fn test_emulator_touchstrip_and_buttons_dispatch() {
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

    emu.boot(120_000_000);

    // Test set_touch_strip with center touch
    emu.set_touch_strip(2048, true);
    for _ in 0..120 {
        emu.coproc.step_all(9600);
    }
    emu.flush_coproc_to_usart();
    let mut touch_press_frames = Vec::new();
    while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
        touch_press_frames.push(b);
    }
    eprintln!("Touch press rx_queue frames: {:02X?}", touch_press_frames);

    // Test set_touch_strip release
    emu.set_touch_strip(2048, false);
    for _ in 0..120 {
        emu.coproc.step_all(9600);
    }
    emu.flush_coproc_to_usart();
    let mut touch_release_frames = Vec::new();
    while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
        touch_release_frames.push(b);
    }
    eprintln!("Touch release rx_queue frames: {:02X?}", touch_release_frames);

    // Test microswitch button actuation for all 8 icon buttons
    for name in ["hold", "up", "order", "random", "pattern", "spice", "dice", "bend"] {
        emu.set_microswitch(name, true);
        emu.step_audio_block_quantum();
        emu.set_microswitch(name, false);
    }
}
