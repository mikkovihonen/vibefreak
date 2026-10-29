use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_save_preset_workflow() {
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

    let mut protect_byte = [0u8; 1];
    let _ = emu.uc.mem_read(0x2000_0803, &mut protect_byte);
    println!("Booted. PC = 0x{:08X}, Memory Protect mode at 0x20000803 = 0x{:02X}", emu.pc, protect_byte[0]);

    // Disable Memory Protect (set to 0 = all slots writeable)
    let _ = emu.uc.mem_write(0x2000_0803, &[0u8]);

    // Press Save microswitch
    println!("Pressing Save button...");
    emu.set_microswitch("save", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("save", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }

    println!("After Save button release. PC = 0x{:08X}", emu.pc);

    // Print OLED display
    let fb = emu.coproc.get_framebuffer();
    println!("--- OLED DISPLAY AFTER SAVE ---");
    for y in 0..64 {
        let page = y / 8;
        let bit = 1 << (y % 8);
        let mut line = String::new();
        for x in 0..128 {
            if (fb[page * 128 + x] & bit) != 0 {
                line.push('#');
            } else {
                line.push(' ');
            }
        }
        if !line.trim().is_empty() {
            println!("{}", line);
        }
    }

    // Select slot 150
    println!("Selecting slot 150 (stepping Preset encoder +149)...");
    emu.step_preset_encoder(149);
    for _ in 0..500 {
        emu.step_audio_block_quantum();
    }

    let fb = emu.coproc.get_framebuffer();
    println!("--- OLED DISPLAY AT SLOT 150 ---");
    for y in 0..64 {
        let page = y / 8;
        let bit = 1 << (y % 8);
        let mut line = String::new();
        for x in 0..128 {
            if (fb[page * 128 + x] & bit) != 0 {
                line.push('#');
            } else {
                line.push(' ');
            }
        }
        if !line.trim().is_empty() {
            println!("{}", line);
        }
    }

    // Directly long press Save for > 1000 ms to execute Quick-Save to slot 150!
    println!("Long pressing Save for 4500 blocks (>1.5s) to execute save to slot 150...");
    emu.set_microswitch("save", true);
    for b in 0..4500 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected during long press Save at block {}: PC = 0x{:08X}", b, pc);
        }
    }
    emu.set_microswitch("save", false);
    for b in 0..1000 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected after long press Save release at block {}: PC = 0x{:08X}", b, pc);
        }
    }
    for b in 0..1000 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected after long press Save release at block {}: PC = 0x{:08X}", b, pc);
        }
    }

    println!("After long press Save, is_qspi_dirty = {}", emu.is_qspi_dirty());

    // Now click Save button to confirm save!
    println!("CLICKING SAVE BUTTON TO CONFIRM SAVE...");
    emu.set_microswitch("save", true);
    for _ in 0..100 { emu.step_audio_block_quantum(); }
    emu.set_microswitch("save", false);
    for b in 0..3000 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected after clicking Save button to save at block {}: PC = 0x{:08X}", b, pc);
        }
    }
    println!("After clicking Save button, is_qspi_dirty = {}", emu.is_qspi_dirty());

    let fb = emu.coproc.get_framebuffer();
    println!("--- OLED DISPLAY AFTER CLICKING SAVE TO SAVE ---");
    for y in 0..64 {
        let page = y / 8;
        let bit = 1 << (y % 8);
        let mut line = String::new();
        for x in 0..128 {
            if (fb[page * 128 + x] & bit) != 0 {
                line.push('#');
            } else {
                line.push(' ');
            }
        }
        if !line.trim().is_empty() {
            println!("{}", line);
        }
    }

    // Also test long press Preset encoder (which is another way to save)
    println!("Now testing long press Preset encoder save...");
    emu.set_microswitch("save", true);
    for _ in 0..100 { emu.step_audio_block_quantum(); }
    emu.set_microswitch("save", false);
    for _ in 0..200 { emu.step_audio_block_quantum(); }

    emu.step_preset_encoder(50);
    for _ in 0..500 { emu.step_audio_block_quantum(); }

    println!("Long pressing Preset encoder to save...");
    emu.set_microswitch("preset", true);
    for b in 0..4500 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected during long press Preset encoder at block {}: PC = 0x{:08X}", b, pc);
        }
    }
    emu.set_microswitch("preset", false);
    for b in 0..2500 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected after long press Preset encoder at block {}: PC = 0x{:08X}", b, pc);
        }
    }
    println!("Is QSPI dirty? {}", emu.is_qspi_dirty());
    assert!(emu.is_qspi_dirty(), "QSPI must be dirty after saving preset!");
    println!("Save test completed successfully!");
}

#[test]
fn test_save_preset_1_direct_and_reload() {
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

    for _ in 0..1500 {
        emu.step_audio_block_quantum();
        let fb = emu.coproc.get_framebuffer();
        let nonzero = fb.iter().filter(|&&b| b != 0).count();
        if nonzero > 400 {
            break;
        }
    }

    // Unprotect memory
    let _ = emu.uc.mem_write(0x2000_0803, &[0u8]);

    // Press Save button once to enter save screen
    println!("Pressing Save button once...");
    emu.set_microswitch("save", true);
    for _ in 0..100 { emu.step_audio_block_quantum(); }
    emu.set_microswitch("save", false);
    for _ in 0..200 { emu.step_audio_block_quantum(); }

    // Quick-save to current preset (Preset 1) by long-pressing Save
    println!("Long-pressing Save button on Preset 1...");
    emu.set_microswitch("save", true);
    for b in 0..4500 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected during long press Save on Preset 1 at block {}: PC = 0x{:08X}", b, pc);
        }
    }
    emu.set_microswitch("save", false);
    for b in 0..2000 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected after long press Save on Preset 1 at block {}: PC = 0x{:08X}", b, pc);
        }
    }

    println!("Is QSPI dirty after Preset 1 save? {}", emu.is_qspi_dirty());
    assert!(emu.is_qspi_dirty(), "QSPI must be dirty after saving Preset 1!");

    // Now navigate away (Preset 2) and navigate back (Preset 1) to force loading the newly saved preset!
    println!("Navigating to Preset 2...");
    emu.step_preset_encoder(1);
    for _ in 0..500 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected while navigating to Preset 2: PC = 0x{:08X}", pc);
        }
    }

    println!("Navigating back to saved Preset 1...");
    emu.step_preset_encoder(-1);
    for _ in 0..500 {
        emu.step_audio_block_quantum();
        let pc = emu.pc;
        if pc < 0x0800_C000 || pc > 0x0810_0000 {
            panic!("CRASH detected while loading saved Preset 1: PC = 0x{:08X}", pc);
        }
    }

    println!("Preset 1 save and reload succeeded without crash!");
}

