use vibefreak_core::coproc::armv6m::ArmV6MCpu;
use std::path::Path;

#[test]
fn test_matrix_firmware_execution() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let matrix_bin_path = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");

    if !matrix_bin_path.exists() {
        eprintln!("Matrix binary not found, skipping");
        return;
    }

    let mut bin = std::fs::read(&matrix_bin_path).expect("Failed to read matrix bin");
    if bin.len() < 0x10000 {
        bin.resize(0x10000, 0xFF);
    }
    // Set Chip ID = 2 (Matrix)
    bin[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x02, 0x00, 0x00]);

    let mut cpu = ArmV6MCpu::new(bin, 16 * 1024);

    for _step in 0..10 {
        cpu.step_cycles(50_000);
    }

    eprintln!("SRAM 0x6C0..0x704: {:02X?}", &cpu.sram[0x6C0..0x704]);
    for i in 0..48 {
        let b = cpu.sram[0x6C0 + i];
        let port_char = match (b >> 5) & 3 {
            0 => 'A',
            1 => 'B',
            2 => 'C',
            _ => 'F',
        };
        let pin = b & 0x1F;
        let active_low = (b & 0x80) != 0;
        eprintln!("Contact {:2}: port {} pin {:2} active_low={}", i, port_char, pin, active_low);
    }

    // Configure contact descriptors in SRAM for the matrix scanner at 0x20000704
    // Contact 5: offset 0 = 0x02, offset 2 = 0x05
    let contact_5_off = 0x704 + 5 * 6;
    cpu.sram[contact_5_off..contact_5_off + 6].copy_from_slice(&[0x02, 0x00, 0x05, 0x00, 0x03, 0x00]);

    // Keep CTS (GPIOA Pin 11) LOW (CTS asserted = ready to receive)
    cpu.gpioa_idr &= !(1 << 11);

    // Step coprocessor cycles while pin is HIGH
    for _step in 0..5 {
        cpu.step_cycles(50_000);
    }

    // Press Contact 5: pull GPIOA Pin 1 LOW
    cpu.gpioa_idr &= !(1 << 1);

    let mut press_tx = Vec::new();
    let mut release_tx = Vec::new();

    for step in 0..10 {
        if step == 4 {
            // Release Contact 5
            cpu.gpioa_idr |= 1 << 1;
        }

        let fsm_state = cpu.sram[0x5CD];
        if fsm_state == 2 {
            cpu.gpioa_idr |= 1 << 11;
        } else {
            cpu.gpioa_idr &= !(1 << 11);
        }

        cpu.step_cycles(50_000);

        let tx1: Vec<u8> = cpu.drain_usart1_tx();
        if step < 4 {
            press_tx.extend(tx1);
        } else {
            release_tx.extend(tx1);
        }
    }

    assert_eq!(press_tx, [0xAA, 0x20, 0x01, 0x00, 0x02, 0x00, 0x05, 0xD2]);
    assert_eq!(release_tx, [0xAA, 0x20, 0x02, 0x00, 0x02, 0x00, 0x05, 0xD3]);
}

#[test]
fn test_matrix_firmware_encoder_execution() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let matrix_bin_path = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");

    if !matrix_bin_path.exists() {
        eprintln!("Matrix binary not found, skipping");
        return;
    }

    let mut bin = std::fs::read(&matrix_bin_path).expect("Failed to read matrix bin");
    if bin.len() < 0x10000 {
        bin.resize(0x10000, 0xFF);
    }
    // Set Chip ID = 2 (Matrix)
    bin[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x02, 0x00, 0x00]);

    let mut cpu = ArmV6MCpu::new(bin, 16 * 1024);

    for _step in 0..15 {
        cpu.step_cycles(50_000);
    }

    // Keep CTS (GPIOA Pin 11) LOW (CTS asserted = ready to receive)
    cpu.gpioa_idr &= !(1 << 11);

    for _step in 0..5 {
        cpu.step_cycles(50_000);
    }

    // Drain initial startup frames
    let _ = cpu.drain_usart1_tx();

    // Enqueue first encoder event: Preset encoder delta=+1
    // Category: 0x00, Cmd: 0x0F, Group: 0x08, ID: 0x00, Delta: 0x0001
    let event1: [u8; 6] = [0x00, 0x0F, 0x08, 0x00, 0x00, 0x01];
    cpu.sram[0x5D8..0x5D8 + 6].copy_from_slice(&event1);
    cpu.sram[0x5D0..0x5D4].copy_from_slice(&1u32.to_le_bytes());

    // Enqueue second encoder event: Matrix encoder delta=+2
    // Category: 0x00, Cmd: 0x0F, Group: 0x09, ID: 0x00, Delta: 0x0002
    let event2: [u8; 6] = [0x00, 0x0F, 0x09, 0x00, 0x00, 0x02];
    cpu.sram[0x5DE..0x5DE + 6].copy_from_slice(&event2);
    cpu.sram[0x5D0..0x5D4].copy_from_slice(&2u32.to_le_bytes());

    let mut enc_tx = Vec::new();
    for _step in 0..60 {
        let fsm_state = cpu.sram[0x5CD];
        if fsm_state == 2 {
            cpu.gpioa_idr |= 1 << 11;
        } else {
            cpu.gpioa_idr &= !(1 << 11);
        }

        cpu.step_cycles(5_000);
        enc_tx.extend(cpu.drain_usart1_tx());
    }

    assert!(
        enc_tx.windows(8).any(|w| w == [0xAA, 0x00, 0x0F, 0x08, 0x00, 0x00, 0x01, 0xC2]),
        "Expected autonomous Preset encoder frame in enc_tx: {:02X?}",
        enc_tx
    );
    assert!(
        enc_tx.windows(8).any(|w| w == [0xAA, 0x00, 0x0F, 0x09, 0x00, 0x00, 0x02, 0xC4]),
        "Expected autonomous Matrix encoder frame in enc_tx: {:02X?}",
        enc_tx
    );
    let read_idx = u32::from_le_bytes(cpu.sram[0x5D4..0x5D8].try_into().unwrap());
    assert_eq!(read_idx, 2, "read_idx must have caught up to 2");
}

#[test]
fn test_keyboard_firmware_execution() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let kbd_bin_path = repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

    if !kbd_bin_path.exists() {
        eprintln!("Keyboard binary not found, skipping");
        return;
    }

    let mut kbd = vibefreak_core::coproc::keyboard::KeyboardCoprocessor::new(&kbd_bin_path)
        .expect("Failed to initialize keyboard coprocessor");

    // 1. Physical Touch: Key 5, pressure 80
    kbd.set_key(5, 80);
    kbd.step(9600 * 50);
    let press_tx = kbd.drain_usart_tx();

    // 2. Physical Release: Key 5, pressure 0
    kbd.set_key(5, 0);
    kbd.step(9600 * 110);
    let release_tx = kbd.drain_usart_tx();

    // Verify autonomous Note On / Touch frame for Key 5:
    assert!(
        press_tx.windows(8).any(|w| w == [0xAA, 0x30, 0x07, 0x01, 0x05, 0x01, 0xFF, 0xE7]),
        "Expected Key 5 autonomous touch frame in press_tx: {:02X?}",
        press_tx
    );

    // Verify autonomous Note Off / Release frame for Key 5 (pressure 0):
    assert!(
        release_tx.windows(8).any(|w| w == [0xAA, 0x30, 0x07, 0x01, 0x05, 0x00, 0x00, 0xE7]),
        "Expected Key 5 autonomous release frame in release_tx: {:02X?}",
        release_tx
    );
}

#[test]
fn test_matrix_contacts_protocol() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let matrix_bin_path = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");

    if !matrix_bin_path.exists() {
        eprintln!("Matrix binary not found, skipping");
        return;
    }

    let mut matrix = vibefreak_core::coproc::matrix::MatrixCoprocessor::new(&matrix_bin_path)
        .expect("Failed to initialize matrix coprocessor");

    for contact in vibefreak_core::coproc::matrix::FRONT_PANEL_CONTACTS {
        matrix.set_microswitch(contact.name, true);
        for _ in 0..6 {
            matrix.step(50_000);
        }
        let press_tx = matrix.drain_usart_tx();
        let press_frame = press_tx
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .unwrap_or_else(|| panic!("No press frame emitted for contact {}", contact.name));

        eprintln!("Contact {}: press_frame={:02X?}", contact.name, press_frame);
        assert_eq!(press_frame[2], 0x01, "cmd must be 0x01 (press) for {}", contact.name);
        assert_eq!(press_frame[3], contact.group, "group mismatch for {}", contact.name);
        assert_eq!(press_frame[4], contact.id, "id mismatch for {}", contact.name);
        let expected_chk = (press_frame[0..7].iter().map(|&b| b as u32).sum::<u32>() & 0xFF) as u8;
        assert_eq!(press_frame[7], expected_chk, "Checksum mismatch on press for {}", contact.name);

        matrix.set_microswitch(contact.name, false);
        for _ in 0..6 {
            matrix.step(50_000);
        }
        let release_tx = matrix.drain_usart_tx();
        let release_frame = release_tx
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .unwrap_or_else(|| panic!("No release frame emitted for contact {}", contact.name));

        assert_eq!(release_frame[2], 0x02, "cmd must be 0x02 (release) for {}", contact.name);
        assert_eq!(release_frame[3], contact.group, "group mismatch for {}", contact.name);
        assert_eq!(release_frame[4], contact.id, "id mismatch for {}", contact.name);
        let expected_chk = (release_frame[0..7].iter().map(|&b| b as u32).sum::<u32>() & 0xFF) as u8;
        assert_eq!(release_frame[7], expected_chk, "Checksum mismatch on release for {}", contact.name);
    }
}

#[test]
fn test_panel_step_timing() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let matrix_bin_path = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
    if !matrix_bin_path.exists() {
        eprintln!("Matrix binary not found, skipping");
        return;
    }
    let mut matrix = vibefreak_core::coproc::matrix::MatrixCoprocessor::new(&matrix_bin_path).unwrap();

    eprintln!("TIM6: cr1={}, psc={}, arr={}, sr={}, cnt={}",
        matrix.cpu.tim6_cr1, matrix.cpu.tim6_psc, matrix.cpu.tim6_arr, matrix.cpu.tim6_sr, matrix.cpu.tim6_cnt);
    let step_period = (matrix.cpu.tim6_psc + 1) * (if matrix.cpu.tim6_arr > 0 { matrix.cpu.tim6_arr } else { 499 });
    eprintln!("TIM6 step_period = {} cycles", step_period);

    eprintln!("[TIMING] Simulating prior buttons...");
    matrix.set_microswitch("cyc_mode", true);
    for _ in 0..50 { matrix.step(4_000); }
    let _ = matrix.drain_usart_tx();
    matrix.set_microswitch("cyc_mode", false);
    for _ in 0..50 { matrix.step(4_000); }
    let _ = matrix.drain_usart_tx();

    matrix.set_microswitch("paraphonic", true);
    for _ in 0..50 { matrix.step(4_000); }
    let _ = matrix.drain_usart_tx();
    matrix.set_microswitch("paraphonic", false);
    for _ in 0..50 { matrix.step(4_000); }
    let _ = matrix.drain_usart_tx();

    matrix.set_microswitch("paraphonic", true);
    for _ in 0..50 { matrix.step(4_000); }
    let _ = matrix.drain_usart_tx();
    matrix.set_microswitch("paraphonic", false);
    for _ in 0..50 { matrix.step(4_000); }
    let _ = matrix.drain_usart_tx();

    eprintln!("[TIMING] Feeding 0xF5 frame to matrix...");
    let f5_frame = [0xAA, 0xF5, 0x00, 0x06, 0x00, 0x00, 0x01, 0xA0];
    for b in f5_frame {
        matrix.feed_usart_rx(b);
    }
    for _ in 0..50 { matrix.step(4_000); }
    let _ = matrix.drain_usart_tx();

    eprintln!("[TIMING] Now pressing panel...");
    matrix.set_microswitch("panel", true);
    for step in 0..200 {
        matrix.step(4_000);
        let tx = matrix.drain_usart_tx();
        if !tx.is_empty() {
            eprintln!("[TIMING] Step {} ({} cycles): tx={:02X?}", step, step * 4_000, tx);
        }
    }
}

#[test]
fn test_matrix_concurrent_contacts() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let matrix_bin_path = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");

    if !matrix_bin_path.exists() {
        eprintln!("Matrix binary not found, skipping");
        return;
    }

    let mut matrix = vibefreak_core::coproc::matrix::MatrixCoprocessor::new(&matrix_bin_path)
        .expect("Failed to initialize matrix coprocessor");

    // 1. Concurrently press Shift (Contact 37) and Paraphonic (Contact 31)
    matrix.set_microswitch("shift", true);
    for _ in 0..3 {
        matrix.step(50_000);
    }
    matrix.set_microswitch("paraphonic", true);
    for _ in 0..6 {
        matrix.step(50_000);
    }
    let press_tx = matrix.drain_usart_tx();

    // Verify both authentic press frames were emitted
    let has_shift_press = press_tx.windows(8).any(|w| {
        w[0] == 0xAA && w[1] == 0x20 && w[2] == 0x01 && w[3] == 0x08 && w[4] == 0x04
    });
    let has_para_press = press_tx.windows(8).any(|w| {
        w[0] == 0xAA && w[1] == 0x20 && w[2] == 0x01 && w[3] == 0x07 && w[4] == 0x00
    });
    assert!(has_shift_press, "Shift press frame must be emitted in concurrent press: {:02X?}", press_tx);
    assert!(has_para_press, "Paraphonic press frame must be emitted in concurrent press: {:02X?}", press_tx);

    // 2. Concurrently release Paraphonic first, then Shift
    matrix.set_microswitch("paraphonic", false);
    for _ in 0..3 {
        matrix.step(50_000);
    }
    matrix.set_microswitch("shift", false);
    for _ in 0..6 {
        matrix.step(50_000);
    }
    let release_tx = matrix.drain_usart_tx();

    let has_para_release = release_tx.windows(8).any(|w| {
        w[0] == 0xAA && w[1] == 0x20 && w[2] == 0x02 && w[3] == 0x07 && w[4] == 0x00
    });
    let has_shift_release = release_tx.windows(8).any(|w| {
        w[0] == 0xAA && w[1] == 0x20 && w[2] == 0x02 && w[3] == 0x08 && w[4] == 0x04
    });
    assert!(has_para_release, "Paraphonic release frame must be emitted in concurrent release: {:02X?}", release_tx);
    assert!(has_shift_release, "Shift release frame must be emitted in concurrent release: {:02X?}", release_tx);
}

#[test]
fn test_matrix_rapid_successive_bursts() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let matrix_bin_path = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");

    if !matrix_bin_path.exists() {
        eprintln!("Matrix binary not found, skipping");
        return;
    }

    let mut matrix = vibefreak_core::coproc::matrix::MatrixCoprocessor::new(&matrix_bin_path)
        .expect("Failed to initialize matrix coprocessor");

    let mut total_tx = Vec::new();
    // 5 rapid successive cycles on Cyc Mode (Contact 39)
    for _ in 0..5 {
        matrix.set_microswitch("cyc_mode", true);
        for _ in 0..6 {
            matrix.step(50_000);
        }
        total_tx.extend(matrix.drain_usart_tx());

        matrix.set_microswitch("cyc_mode", false);
        for _ in 0..6 {
            matrix.step(50_000);
        }
        total_tx.extend(matrix.drain_usart_tx());
    }

    let press_count = total_tx.windows(8).filter(|w| {
        w[0] == 0xAA && w[1] == 0x20 && w[2] == 0x01 && w[3] == 0x02 && w[4] == 0x00
    }).count();
    let release_count = total_tx.windows(8).filter(|w| {
        w[0] == 0xAA && w[1] == 0x20 && w[2] == 0x02 && w[3] == 0x02 && w[4] == 0x00
    }).count();

    assert_eq!(press_count, 5, "Expected exactly 5 press frames for cyc_mode during rapid burst");
    assert_eq!(release_count, 5, "Expected exactly 5 release frames for cyc_mode during rapid burst");
}

#[test]
fn test_discover_octave_pins() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let matrix_bin_path = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");

    if !matrix_bin_path.exists() {
        return;
    }

    let mut matrix = vibefreak_core::coproc::matrix::MatrixCoprocessor::new(&matrix_bin_path).unwrap();

    for &port in &['A', 'B', 'C', 'F'] {
        for pin in 0..16 {
            matrix.cpu.set_pin_level(port, pin, false); // Press (LOW)
            for _ in 0..6 {
                matrix.step(50_000);
            }
            let press_tx = matrix.drain_usart_tx();

            matrix.cpu.set_pin_level(port, pin, true); // Release (HIGH)
            for _ in 0..6 {
                matrix.step(50_000);
            }
            let _ = matrix.drain_usart_tx();

            let frames: Vec<_> = press_tx.windows(8).filter(|w| w[0] == 0xAA && w[1] == 0x20).collect();
            for f in frames {
                eprintln!("Pin Port {} Pin {:2}: cmd=0x{:02X}, group=0x{:02X}, id=0x{:02X}", port, pin, f[2], f[3], f[4]);
            }
        }
    }
}




