use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_wavetable_paraphonic_synthesis() {
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
    for _p in 2..=4 {
        emu.step_preset_encoder(1);
        for _ in 0..1000 {
            emu.step_audio_block_quantum();
        }
    }



    // Toggle Paraphonic mode ON
    emu.set_microswitch("paraphonic", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("paraphonic", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }



    // Allow background DMA transfers to finish QSPI wavetable caching
    for _ in 0..400 {
        emu.step_audio_block_quantum();
    }

    // Verify wavetable buffer in RAM is populated from QSPI flash
    let mut wt_buf = [0u8; 16384];
    let _ = emu.uc.mem_read(0x20004880, &mut wt_buf);
    let wt_nonzero = wt_buf.iter().filter(|&&b| b != 0).count();
    assert!(
        wt_nonzero > 4000,
        "Wavetable RAM buffer must be loaded from QSPI (found {} non-zero bytes)",
        wt_nonzero
    );

    // Press a 4-note chord on the keybed (Keys 12, 16, 19, 23 = C, E, G, B)
    emu.set_key(12, 200);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }
    emu.set_key(16, 200);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }
    emu.set_key(19, 200);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }
    emu.set_key(23, 200);

    let mut non_zero_blocks = 0;
    let mut max_peak = 0i16;

    for _ in 0..150 {
        let blk = emu.step_audio_block_quantum();
        for chunk in blk.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > 100 {
                non_zero_blocks += 1;
            }
            if left.abs() > max_peak {
                max_peak = left.abs();
            }
        }
    }

    // Print diagnostic wavetable slots and voice engine state
    let fs_base = 0x200018B8;
    for s in 0..4 {
        let mut buf_ptr = [0u8; 4];
        let mut offset = [0u8; 4];
        let _ = emu.uc.mem_read(fs_base + 0x1C30 + s * 8, &mut buf_ptr);
        let _ = emu.uc.mem_read(fs_base + 0x1C34 + s * 8, &mut offset);
        let b = u32::from_le_bytes(buf_ptr);
        let o = u32::from_le_bytes(offset);
        let mut data = [0u8; 1024];
        let _ = emu.uc.mem_read(b as u64, &mut data);
        let nz = data.iter().filter(|&&x| x != 0).count();
        println!("FS SLOT {}: buf=0x{:08X}, cached_offset=0x{:08X}, non-zero bytes={}", s, b, o, nz);
        assert!(
            nz > 500,
            "FS slot {} must be loaded from QSPI flash (found {} non-zero bytes)",
            s,
            nz
        );
    }

    let voices = [0x2000484C, 0x2000A230, 0x2000FC14, 0x200155F8];
    for (v_idx, &v_addr) in voices.iter().enumerate() {
        let mut engine_ptr_bytes = [0u8; 4];
        let _ = emu.uc.mem_read(v_addr + 0x50AC, &mut engine_ptr_bytes);
        let engine_ptr = u32::from_le_bytes(engine_ptr_bytes);

        let mut core_ptr_bytes = [0u8; 4];
        let mut vtable_bytes = [0u8; 4];
        if engine_ptr != 0 {
            let _ = emu.uc.mem_read(engine_ptr as u64, &mut vtable_bytes);
            let _ = emu.uc.mem_read((engine_ptr + 0x5C) as u64, &mut core_ptr_bytes);
        }
        let vtable = u32::from_le_bytes(vtable_bytes);
        let core_ptr = u32::from_le_bytes(core_ptr_bytes);

        let mut core_info = String::new();
        if core_ptr != 0 {
            let mut buf_bytes = [0u8; 4];
            let mut slot_byte = [0u8; 1];
            let mut table_bytes = [0u8; 2];
            let mut wave_bytes = [0u8; 4];
            let _ = emu.uc.mem_read((core_ptr + 0x68) as u64, &mut buf_bytes);
            let _ = emu.uc.mem_read((core_ptr + 0x65) as u64, &mut slot_byte);
            let _ = emu.uc.mem_read((core_ptr + 0x50) as u64, &mut table_bytes);
            let _ = emu.uc.mem_read((core_ptr + 0x54) as u64, &mut wave_bytes);
            core_info = format!(
                "core=0x{:08X}, slot={}, buf=0x{:08X}, table={}, wave=0x{:08X}",
                core_ptr,
                slot_byte[0],
                u32::from_le_bytes(buf_bytes),
                i16::from_le_bytes(table_bytes),
                u32::from_le_bytes(wave_bytes)
            );
        }

        let mut pitch_bytes = [0u8; 2];
        let _ = emu.uc.mem_read(v_addr + 8, &mut pitch_bytes);
        let pitch = i16::from_le_bytes(pitch_bytes);

        println!("VOICE {}: pitch={} engine=0x{:08X} (vtable=0x{:08X}) {}", v_idx, pitch, engine_ptr, vtable, core_info);
    }

    // Read SynthEngine at 0x20004848
    let se_base = 0x20004848;
    let mut se_bytes = [0u8; 16];
    let _ = emu.uc.mem_read(se_base + 0x167D6, &mut se_bytes[..2]);
    let para_switch = i16::from_le_bytes([se_bytes[0], se_bytes[1]]);
    let mut para_alloc_byte = [0u8; 1];
    let _ = emu.uc.mem_read(se_base + 0x16E00, &mut para_alloc_byte);
    let mut para_active_byte = [0u8; 1];
    let _ = emu.uc.mem_read(se_base + 0x16E01, &mut para_active_byte);
    println!("SYNTH ENGINE: para_switch={}, alloc_mode={}, para_active={}", para_switch, para_alloc_byte[0], para_active_byte[0]);

    // Release all keys
    emu.set_key(12, 0);
    emu.set_key(16, 0);
    emu.set_key(19, 0);
    emu.set_key(23, 0);

    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }

    assert!(
        max_peak > 1000 && non_zero_blocks > 50,
        "4-note chord must synthesize sound across voices! (peak={}, active_blocks={})",
        max_peak,
        non_zero_blocks
    );
}
