use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_sample_and_grains_engines_produce_sound() {
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

    if !main_bin.exists() {
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");

    let last_out_buf = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let last_dsp_type = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)); // 1=Sample, 2=Granular
    let sample_osc_peak = std::sync::Arc::new(std::sync::atomic::AtomicI16::new(0));
    let granular_osc_peak = std::sync::Arc::new(std::sync::atomic::AtomicI16::new(0));

    let b_sample = last_out_buf.clone();
    let t_sample = last_dsp_type.clone();
    let _ = emu.uc.add_code_hook(0x0801_6E30, 0x0801_6E30, move |uc, _addr, _size| {
        let r1 = uc.reg_read(unicorn_engine::RegisterARM::R1 as i32).unwrap_or(0) as u32;
        b_sample.store(r1, std::sync::atomic::Ordering::Relaxed);
        t_sample.store(1, std::sync::atomic::Ordering::Relaxed);
    });

    let b_gran = last_out_buf.clone();
    let t_gran = last_dsp_type.clone();
    let _ = emu.uc.add_code_hook(0x0801_5C88, 0x0801_5C88, move |uc, _addr, _size| {
        let r1 = uc.reg_read(unicorn_engine::RegisterARM::R1 as i32).unwrap_or(0) as u32;
        b_gran.store(r1, std::sync::atomic::Ordering::Relaxed);
        t_gran.store(2, std::sync::atomic::Ordering::Relaxed);
    });

    let b_exit = last_out_buf.clone();
    let t_exit = last_dsp_type.clone();
    let s_exit_peak = sample_osc_peak.clone();
    let g_exit_peak = granular_osc_peak.clone();
    let _ = emu.uc.add_code_hook(0x0804_50C0, 0x0804_50C0, move |uc, _addr, _size| {
        let buf_addr = b_exit.load(std::sync::atomic::Ordering::Relaxed);
        let dsp_type = t_exit.load(std::sync::atomic::Ordering::Relaxed);
        if buf_addr != 0 && dsp_type != 0 {
            let mut buf = [0u8; 64];
            if uc.mem_read(buf_addr as u64, &mut buf).is_ok() {
                let mut peak = 0i16;
                for chunk in buf.chunks_exact(2) {
                    let s = i16::from_le_bytes([chunk[0], chunk[1]]);
                    if s.abs() > peak.abs() {
                        peak = s;
                    }
                }
                if dsp_type == 1 {
                    let curr = s_exit_peak.load(std::sync::atomic::Ordering::Relaxed);
                    if peak.abs() > curr.abs() {
                        s_exit_peak.store(peak, std::sync::atomic::Ordering::Relaxed);
                    }
                } else if dsp_type == 2 {
                    let curr = g_exit_peak.load(std::sync::atomic::Ordering::Relaxed);
                    if peak.abs() > curr.abs() {
                        g_exit_peak.store(peak, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }
    });

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

    // 1. Test Preset 1 (BasicWaves) sound output baseline
    emu.set_key(12, 200);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    let mut max_p1 = 0i16;
    for _ in 0..50 {
        let block = emu.step_audio_block_quantum();
        for chunk in block.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > max_p1 {
                max_p1 = left.abs();
            }
        }
    }
    emu.set_key(12, 0);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    assert!(max_p1 > 1000, "Preset 1 (BasicWaves) should produce audio, got max_amp = {}", max_p1);

    // 2. Step 17 times from initial to reach Sample engine (type index 18, table slot 20)
    for _ in 1..=17 {
        emu.step_type_encoder(1);
        for _ in 0..50 {
            emu.step_audio_block_quantum();
        }
    }

    // Allow the firmware encoder debounce countdown (1023 audio blocks) to settle
    for _ in 0..1100 {
        emu.step_audio_block_quantum();
    }

    // Trigger key on Sample engine
    println!("\n=== Testing Sample engine ===");
    emu.set_key(12, 200);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    let mut max_sample_amp = 0i16;
    let mut nz_sample_blocks = 0;
    for _ in 0..150 {
        let block = emu.step_audio_block_quantum();
        let mut blk_peak = 0i16;
        for chunk in block.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > blk_peak.abs() {
                blk_peak = left;
            }
        }
        if blk_peak.abs() > 0 {
            nz_sample_blocks += 1;
        }
        if blk_peak.abs() > max_sample_amp {
            max_sample_amp = blk_peak.abs();
        }
    }
    emu.set_key(12, 0);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    println!("Sample engine: max_amp = {}, nz_blocks = {}, RAW OSC DSP PEAK = {}",
        max_sample_amp, nz_sample_blocks, sample_osc_peak.load(std::sync::atomic::Ordering::Relaxed));

    // 3. Step 1 more to reach Scan Grains engine (table slot 21)
    println!("\n=== Stepping to Scan Grains engine ===");
    emu.step_type_encoder(1);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    // Settle debounce
    for _ in 0..1100 {
        emu.step_audio_block_quantum();
    }

    emu.set_key(12, 200);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    let mut max_scan_grains_amp = 0i16;
    let mut nz_scan_blocks = 0;
    for _ in 0..150 {
        let block = emu.step_audio_block_quantum();
        let mut blk_peak = 0i16;
        for chunk in block.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > blk_peak.abs() {
                blk_peak = left;
            }
        }
        if blk_peak.abs() > 0 {
            nz_scan_blocks += 1;
        }
        if blk_peak.abs() > max_scan_grains_amp {
            max_scan_grains_amp = blk_peak.abs();
        }
    }
    emu.set_key(12, 0);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    println!("Scan Grains engine: max_amp = {}, nz_blocks = {}, RAW OSC DSP PEAK = {}",
        max_scan_grains_amp, nz_scan_blocks, granular_osc_peak.load(std::sync::atomic::Ordering::Relaxed));

    // 4. Step 1 more to reach Cloud Grains engine (table slot 22)
    println!("\n=== Stepping to Cloud Grains engine ===");
    granular_osc_peak.store(0, std::sync::atomic::Ordering::Relaxed);
    emu.step_type_encoder(1);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    // Settle debounce
    for _ in 0..1100 {
        emu.step_audio_block_quantum();
    }

    emu.set_key(12, 200);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    let mut max_cloud_grains_amp = 0i16;
    let mut nz_cloud_blocks = 0;
    for _ in 0..150 {
        let block = emu.step_audio_block_quantum();
        let mut blk_peak = 0i16;
        for chunk in block.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > blk_peak.abs() {
                blk_peak = left;
            }
        }
        if blk_peak.abs() > 0 {
            nz_cloud_blocks += 1;
        }
        if blk_peak.abs() > max_cloud_grains_amp {
            max_cloud_grains_amp = blk_peak.abs();
        }
    }
    emu.set_key(12, 0);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    println!("Cloud Grains engine: max_amp = {}, nz_blocks = {}, RAW OSC DSP PEAK = {}",
        max_cloud_grains_amp, nz_cloud_blocks, granular_osc_peak.load(std::sync::atomic::Ordering::Relaxed));

    // 5. Step 1 more to reach Hit Grains engine (table slot 23)
    println!("\n=== Stepping to Hit Grains engine ===");
    granular_osc_peak.store(0, std::sync::atomic::Ordering::Relaxed);
    emu.step_type_encoder(1);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    // Settle debounce
    for _ in 0..1100 {
        emu.step_audio_block_quantum();
    }

    emu.set_key(12, 200);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    let mut max_hit_grains_amp = 0i16;
    let mut nz_hit_blocks = 0;
    for _ in 0..150 {
        let block = emu.step_audio_block_quantum();
        let mut blk_peak = 0i16;
        for chunk in block.chunks_exact(4) {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]);
            if left.abs() > blk_peak.abs() {
                blk_peak = left;
            }
        }
        if blk_peak.abs() > 0 {
            nz_hit_blocks += 1;
        }
        if blk_peak.abs() > max_hit_grains_amp {
            max_hit_grains_amp = blk_peak.abs();
        }
    }
    emu.set_key(12, 0);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    println!("Hit Grains engine: max_amp = {}, nz_blocks = {}, RAW OSC DSP PEAK = {}",
        max_hit_grains_amp, nz_hit_blocks, granular_osc_peak.load(std::sync::atomic::Ordering::Relaxed));

    assert!(max_sample_amp > 1000, "Sample engine audio output peak must be > 1000, got {}", max_sample_amp);
    assert!(sample_osc_peak.load(std::sync::atomic::Ordering::Relaxed).abs() > 8000, "Sample raw osc peak must be > 8000");
    assert!(max_scan_grains_amp > 1000, "Scan Grains audio output peak must be > 1000, got {}", max_scan_grains_amp);
    assert!(max_cloud_grains_amp > 1000, "Cloud Grains audio output peak must be > 1000, got {}", max_cloud_grains_amp);
    assert!(max_hit_grains_amp > 1000, "Hit Grains audio output peak must be > 1000, got {}", max_hit_grains_amp);
}
