use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_oled_display_with_user_presets() {
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

    for i in 0..1500 {
        emu.step_audio_block_quantum();
        if i % 100 == 0 {
            let fb = emu.coproc.get_framebuffer();
            let nonzero = fb.iter().filter(|&&b| b != 0).count();
            if nonzero > 400 {
                println!("Reached active screen at block {}", i);
                println!("--- PRESET 1 OLED DISPLAY ---");
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
                    println!("{}", line);
                }
                println!("--------------------");

                println!("Clicking Preset encoder to open preset list...");
                emu.set_microswitch("preset", true);
                for _ in 0..10 { emu.step_audio_block_quantum(); }
                emu.set_microswitch("preset", false);
                for _ in 0..20 { emu.step_audio_block_quantum(); }

                let fb_list = emu.coproc.get_framebuffer();
                println!("--- PRESET LIST OLED DISPLAY ---");
                for y in 0..64 {
                    let page = y / 8;
                    let bit = 1 << (y % 8);
                    let mut line = String::new();
                    for x in 0..128 {
                        if (fb_list[page * 128 + x] & bit) != 0 {
                            line.push('#');
                        } else {
                            line.push(' ');
                        }
                    }
                    println!("{}", line);
                }
                println!("--------------------");

                println!("Rotating Preset encoder delta=+1 (Preset 2)...");
                emu.step_preset_encoder(1);
                for _ in 0..300 { emu.step_audio_block_quantum(); }

                let fb_preset2 = emu.coproc.get_framebuffer();
                println!("--- PRESET 2 OLED DISPLAY ---");
                for y in 0..64 {
                    let page = y / 8;
                    let bit = 1 << (y % 8);
                    let mut line = String::new();
                    for x in 0..128 {
                        if (fb_preset2[page * 128 + x] & bit) != 0 {
                            line.push('#');
                        } else {
                            line.push(' ');
                        }
                    }
                    println!("{}", line);
                }
                println!("--------------------");
                break;
            }
        }
    }
}
