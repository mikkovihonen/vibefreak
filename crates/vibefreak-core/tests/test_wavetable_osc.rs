use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_wavetable_oscillator_synthesis() {
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
        emu.load_qspi_from_disk(&presets_path).unwrap();
    }

    let _ = emu.uc.add_code_hook(0x0803_0720, 0x0803_0720, move |uc, _addr, _size| {
        let r0 = uc.reg_read(unicorn_engine::RegisterARM::R0 as i32).unwrap_or(0);
        let r1 = uc.reg_read(unicorn_engine::RegisterARM::R1 as i32).unwrap_or(0);
        let r2 = uc.reg_read(unicorn_engine::RegisterARM::R2 as i32).unwrap_or(0);
        let r3 = uc.reg_read(unicorn_engine::RegisterARM::R3 as i32).unwrap_or(0);
        let lr = uc.reg_read(unicorn_engine::RegisterARM::LR as i32).unwrap_or(0);
        println!("[test_wt_osc WT INIT] r0=0x{:08X}, r1={}, r2=0x{:08X}, r3=0x{:08X}, lr=0x{:08X}", r0, r1, r2, r3, lr);
    });

    let _ = emu.uc.add_code_hook(0x0801_7F9A, 0x0801_7F9A, move |uc, _addr, _size| {
        static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let count = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count < 5 {
            let ip = uc.reg_read(unicorn_engine::RegisterARM::IP as i32).unwrap_or(0);
            let r3 = uc.reg_read(unicorn_engine::RegisterARM::R3 as i32).unwrap_or(0);
            let mut pcm = [0u8; 8];
            let _ = uc.mem_read(ip as u64, &mut pcm);
            println!("[test_wt_osc WT READ] count={}: ip=0x{:08X}, r3={}, bytes={:02X?}", count, ip, r3, pcm);
        }
    });

    emu.boot(100_000_000);

    // Boot to active screen
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
        let fb = emu.coproc.get_framebuffer();
        let nonzero = fb.iter().filter(|&&b| b != 0).count();
        if nonzero > 400 {
            break;
        }
    }

    // Step preset encoder to preset 4 ("WaveTable Seq")
    for p in 2..=4 {
        emu.step_preset_encoder(1);
        for _ in 0..300 {
            emu.step_audio_block_quantum();
        }
        println!("At preset {}", p);
    }

    let fb = emu.coproc.get_framebuffer();
    println!("--- PRESET 4 OLED DISPLAY ---");
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

    // Now press a key and record audio
    println!("Pressing key 12 on Preset 4 (WaveTable)...");

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let qspi_reads = Arc::new(AtomicUsize::new(0));
    let flash_wt_reads = Arc::new(AtomicUsize::new(0));

    let qspi_clone = qspi_reads.clone();
    let _ = emu.uc.add_mem_hook(
        unicorn_engine::HookType::MEM_READ,
        0x90F5_0000,
        0x90FA_FFFF,
        move |_uc, _access, addr, size, _val| {
            let count = qspi_clone.fetch_add(1, Ordering::Relaxed);
            if count < 10 {
                println!("QSPI WT READ: addr=0x{:08X}, size={}", addr, size);
            }
            true
        },
    );

    let flash_clone = flash_wt_reads.clone();
    let _ = emu.uc.add_mem_hook(
        unicorn_engine::HookType::MEM_READ,
        0x0806_3000,
        0x0806_CFFF,
        move |uc, _access, addr, size, _val| {
            let count = flash_clone.fetch_add(1, Ordering::Relaxed);
            if count < 10 {
                let pc = uc.reg_read(unicorn_engine::RegisterARM::PC as i32).unwrap_or(0);
                println!("FLASH WT READ: pc=0x{:08X}, addr=0x{:08X}, size={}", pc, addr, size);
            }
            true
        },
    );

    emu.set_key(12, 200);

    let mut non_zero_blocks = 0;
    let mut max_peak = 0i16;
    for b in 0..150 {
        let block = emu.step_audio_block_quantum();
        let mut block_samples = [0i16; 32];
        for i in 0..32 {
            let s = i16::from_le_bytes([block[i * 2], block[i * 2 + 1]]);
            block_samples[i] = s;
            let abs_s = s.abs();
            if abs_s > max_peak {
                max_peak = abs_s;
            }
        }
        let nz = block_samples.iter().any(|&s| s != 0);
        if nz {
            non_zero_blocks += 1;
        }
        if b % 25 == 0 || b < 5 {
            println!("Block {}: nz={}, max_sample={}, first_4={:?}", b, nz, block_samples.iter().map(|s| s.abs()).max().unwrap_or(0), &block_samples[..4]);
        }
    }
    emu.set_key(12, 0);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }

    println!("Preset 4 WaveTable synthesis result: non_zero_blocks={}, max_peak={}", non_zero_blocks, max_peak);
    assert!(max_peak > 1000 && non_zero_blocks > 50, "Preset 4 WaveTable must synthesize sound!");

    println!("Testing Wave encoder (Table selection)...");
    for table_step in 1..=16 {
        emu.step_wave_encoder(1);
        for _ in 0..50 {
            emu.step_audio_block_quantum();
        }
        let fb = emu.coproc.get_framebuffer();
        println!("After step_wave_encoder(1) step {}: fb non-zero = {}", table_step, fb.iter().filter(|&&b| b != 0).count());
    }
}



