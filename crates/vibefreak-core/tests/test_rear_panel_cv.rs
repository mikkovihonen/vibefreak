use vibefreak_core::{VibeFreakEmulator, VECTOR_EXTI15_10};
use std::path::Path;

#[test]
fn test_rear_panel_cv_gate_clock() {
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

    eprintln!("[1] Booting emulator...");
    emu.boot(120_000_000);

    // Initial rear panel read
    let rear = emu.get_rear_panel();
    eprintln!(
        "Initial Rear Panel: PitchCV={:.3}V (raw={}), Gate={:.1}V (raw={}, 12v={}), Pressure={:.3}V (raw={}), ClkOut={:.1}V, ClkIn={}",
        rear.pitch_cv_volts, rear.pitch_cv_raw, rear.gate_volts, rear.gate_raw, rear.gate_mode_12v,
        rear.pressure_volts, rear.pressure_raw, rear.clock_out_volts, rear.clock_in_raw
    );

    assert!(rear.pitch_cv_volts >= 0.0 && rear.pitch_cv_volts <= 10.0);
    assert!(rear.gate_volts >= 0.0 && rear.gate_volts <= 12.0);
    assert!(rear.pressure_volts >= 0.0 && rear.pressure_volts <= 10.0);
    assert!(!rear.clock_in_raw);

    // Test Clock In pulse high
    eprintln!("[2] Pulsing Clock In high...");
    emu.set_clock_in(true);
    let rear_high = emu.get_rear_panel();
    assert!(rear_high.clock_in_raw);
    assert!(emu.uc.get_data().nvic.is_pending(VECTOR_EXTI15_10));

    // Test Clock In low
    eprintln!("[3] Setting Clock In low...");
    emu.set_clock_in(false);
    let rear_low = emu.get_rear_panel();
    assert!(!rear_low.clock_in_raw);
    eprintln!("Rear panel CV/Gate/Clock test passed successfully!");
}

#[test]
fn test_internal_clock_output_pulse() {
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

    eprintln!("[1] Booting emulator...");
    emu.boot(120_000_000);

    let tim5 = &emu.uc.get_data().mmio.timers.timers[4];
    let tim8 = &emu.uc.get_data().mmio.timers.timers[7];
    eprintln!(
        "Post-boot TIM5: cr1=0x{:x}, cr2=0x{:x}, smcr=0x{:x}, dier=0x{:x}, sr=0x{:x}, cnt={}, arr={}, psc={}",
        tim5.cr1, tim5.cr2, tim5.smcr, tim5.dier, tim5.sr, tim5.cnt, tim5.arr, tim5.psc
    );
    eprintln!(
        "Post-boot TIM8: cr1=0x{:x}, cr2=0x{:x}, smcr=0x{:x}, dier=0x{:x}, sr=0x{:x}, cnt={}, arr={}, psc={}",
        tim8.cr1, tim8.cr2, tim8.smcr, tim8.dier, tim8.sr, tim8.cnt, tim8.arr, tim8.psc
    );

    let vtor = emu.uc.get_data().nvic.vtor;
    let iser = emu.uc.get_data().nvic.iser;
    eprintln!("VTOR = 0x{:08x}, ISER = [0x{:x}, 0x{:x}, 0x{:x}]", vtor, iser[0], iser[1], iser[2]);
    for v in [60, 61, 62, 66] {
        let mut b = [0u8; 4];
        let _ = emu.uc.mem_read((vtor + 4 * v) as u64, &mut b);
        let addr = u32::from_le_bytes(b);
        eprintln!(
            "Vector {}: is_enabled={}, is_pending={}, addr=0x{:08x}",
            v, emu.uc.get_data().nvic.is_enabled(v), emu.uc.get_data().nvic.is_pending(v), addr
        );
    }

    // Turn Arpeggiator ON and engage Hold + Key
    eprintln!("[2] Turning Arpeggiator ON...");
    emu.set_microswitch("arp_seq", true);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }

    eprintln!("[3] Pressing Key 12 and Hold...");
    emu.coproc.set_key(12, 200);
    emu.set_microswitch("hold", true);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("hold", false);

    // Step audio blocks and record clock transitions and audio output
    eprintln!("[4] Stepping audio blocks to observe clock pulse and arpeggiator audio...");
    let mut high_count = 0;
    let mut low_count = 0;
    let mut max_amp = 0i32;
    let mut nz_blocks = 0;
    let mut prev_clk = emu.get_rear_panel().clock_out_raw;

    for block in 0..300 {
        let blk = emu.step_audio_block_quantum();
        let mut blk_peak = 0i32;
        for chunk in blk.as_chunks::<4>().0 {
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

        let rear = emu.get_rear_panel();
        if rear.clock_out_raw {
            high_count += 1;
            assert_eq!(rear.clock_out_volts, 5.0);
        } else {
            low_count += 1;
            assert_eq!(rear.clock_out_volts, 0.0);
        }
        if rear.clock_out_raw != prev_clk {
            eprintln!(
                "Clock transition at block {}: raw={}, volts={:.1}V",
                block, rear.clock_out_raw, rear.clock_out_volts
            );
            prev_clk = rear.clock_out_raw;
        }
    }

    eprintln!(
        "Arp Audio Output: max_amp={}, nz_blocks={}/300; Clock Output: high_blocks={}, low_blocks={}",
        max_amp, nz_blocks, high_count, low_count
    );
    assert!(
        max_amp > 1000 && nz_blocks > 50,
        "Arpeggiator produced silence or broken audio! max_amp={}, nz_blocks={}",
        max_amp, nz_blocks
    );
    assert!(high_count > 0 && low_count > 0, "Clock output did not pulse! high={}, low={}", high_count, low_count);

    // [5] Try to turn Arp OFF
    eprintln!("[5] Trying to turn Arpeggiator OFF...");
    let arp_led_before = emu.coproc.get_panel_leds()[33];
    eprintln!("Arp LED before: {}", arp_led_before);
    assert_eq!(arp_led_before, 255, "Arp LED should be ON when Arpeggiator is engaged");

    emu.set_microswitch("arp_seq", true);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    let arp_led_after = emu.coproc.get_panel_leds()[33];
    eprintln!("Arp LED after: {}", arp_led_after);
    assert_eq!(arp_led_after, 0, "Arp LED should turn OFF when arp_seq is pressed again");
}

