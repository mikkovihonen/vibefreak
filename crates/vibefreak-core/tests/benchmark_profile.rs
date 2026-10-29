use vibefreak_core::VibeFreakEmulator;
use std::path::Path;
use std::time::Instant;

#[test]
fn test_benchmark_performance_breakdown() {
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
    emu.profiling = true;
    let presets = repo_root.join("user_presets.bin");
    if presets.exists() {
        let _ = emu.load_qspi_from_disk(&presets);
    }

    println!("\n=== PERFORMANCE BENCHMARK ===");
    let t_boot = Instant::now();
    emu.boot(100_000_000);
    let boot_dur = t_boot.elapsed();
    println!("Boot 100M cycles: {:?} ({:.2} MHz)", boot_dur, 100.0 / boot_dur.as_secs_f64());
    println!("Firmware systick_load: {}, systick_ctrl: 0x{:X}", emu.uc.get_data().nvic.systick_load, emu.uc.get_data().nvic.systick_ctrl);
    println!("CPU model: {:?}", emu.uc.ctl_get_cpu_model());
    println!("Matrix systick_load: {}, ctrl: 0x{:X}", emu.coproc.matrix.cpu.systick_load, emu.coproc.matrix.cpu.systick_ctrl);
    println!("Keyboard systick_load: {}, ctrl: 0x{:X}", emu.coproc.keyboard.cpu.systick_load, emu.coproc.keyboard.cpu.systick_ctrl);
    // Measure audio blocks
    emu.coproc.time_oled = std::time::Duration::ZERO;
    emu.coproc.time_matrix = std::time::Duration::ZERO;
    emu.coproc.time_kbd = std::time::Duration::ZERO;
    emu.coproc.oled.cpu.ff_hits = 0;
    emu.coproc.oled.cpu.ff_cycles = 0;
    emu.coproc.matrix.cpu.ff_hits = 0;
    emu.coproc.matrix.cpu.ff_cycles = 0;
    let matrix_cycles_start = emu.coproc.matrix.cpu.cycles;
    emu.coproc.keyboard.cpu.ff_hits = 0;
    emu.coproc.keyboard.cpu.ff_cycles = 0;
    let kbd_cycles_start = emu.coproc.keyboard.cpu.cycles;
    let num_blocks = 2000;
    let t_audio = Instant::now();
    let mut t_prev = Instant::now();
    let mut non_zero_count = 0;
    for i in 0..num_blocks {
        let blk = emu.step_audio_block_quantum();
        if blk.iter().any(|&b| b != 0) {
            non_zero_count += 1;
        }


        if (i + 1) % 50 == 0 {
            let el = t_prev.elapsed();
            println!("Blocks {:03}..{:03}: {:?} (PC=0x{:08X})", i - 49, i, el, emu.pc);
            t_prev = Instant::now();
        }
    }
    println!("Non-zero audio blocks: {} / {}", non_zero_count, num_blocks);
    let audio_dur = t_audio.elapsed();
    let virtual_time = (num_blocks as f64 * 32.0) / 48000.0;
    let real_time = audio_dur.as_secs_f64();
    let ratio = virtual_time / real_time;
    println!(
        "{} audio blocks: {:?} (virtual audio time: {:.3}s, real time: {:.3}s, speed: {:.3}x real-time)",
        num_blocks, audio_dur, virtual_time, real_time, ratio
    );
    println!("Per audio block: {:?}", audio_dur / num_blocks);
    println!("Breakdown of {} blocks:", num_blocks);
    println!("  - STM32F446 step_mcu:     {:?} ({:.1}%)", emu.perf_step_mcu, (emu.perf_step_mcu.as_secs_f64() / real_time) * 100.0);
    println!("  - STM32F446 fast_forward: {:?} ({:.1}%)", emu.perf_fast_forward, (emu.perf_fast_forward.as_secs_f64() / real_time) * 100.0);
    println!("  - STM32F446 systick:      {:?} ({:.1}%)", emu.perf_systick, (emu.perf_systick.as_secs_f64() / real_time) * 100.0);
    println!("  - STM32F0 coproc:         {:?} ({:.1}%)", emu.perf_coproc, (emu.perf_coproc.as_secs_f64() / real_time) * 100.0);
    println!("      * OLED:   {:?} (PC=0x{:08X}, R0=0x{:08X}, sleeping={})", emu.coproc.time_oled, emu.coproc.oled.cpu.regs[15], emu.coproc.oled.cpu.regs[0], emu.coproc.oled.cpu.sleeping);
    println!("      * Matrix: {:?} (ff_hits={}, ff_cycles={}/{})",
        emu.coproc.time_matrix, emu.coproc.matrix.cpu.ff_hits, emu.coproc.matrix.cpu.ff_cycles, emu.coproc.matrix.cpu.cycles - matrix_cycles_start);
    println!("      * Kbd:    {:?} (ff_hits={}, ff_cycles={}/{})",
        emu.coproc.time_kbd, emu.coproc.keyboard.cpu.ff_hits, emu.coproc.keyboard.cpu.ff_cycles, emu.coproc.keyboard.cpu.cycles - kbd_cycles_start);
    println!("  - DMA2 render_audio:      {:?} ({:.1}%)", emu.perf_render_audio, (emu.perf_render_audio.as_secs_f64() / real_time) * 100.0);
    println!("      * Enter exception:      {:?}", emu.perf_render_enter_exc);
    println!("      * Unicorn emu_start:    {:?}", emu.perf_render_emu_start);
    println!("      * Exit exception:       {:?}", emu.perf_render_exit_exc);
    println!("      * Rust SEM/VCA DSP:     {:?}", emu.perf_render_dsp);
    println!("  - USART/Co-sched service: {:?} ({:.1}%)", emu.perf_usart, (emu.perf_usart.as_secs_f64() / real_time) * 100.0);
}

#[test]
fn test_benchmark_paraphonic_4voices() {
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
    let presets = repo_root.join("user_presets.bin");
    if presets.exists() {
        let _ = emu.load_qspi_from_disk(&presets);
    }

    emu.boot(100_000_000);

    // Toggle Paraphonic microswitch
    emu.set_microswitch("paraphonic", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("paraphonic", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    assert_eq!(emu.coproc.get_panel_leds()[28], 255, "Paraphonic LED must turn ON");

    // Press 4 keys: C (12), E (16), G (19), B (23)
    emu.set_key(12, 200);
    emu.set_key(16, 200);
    emu.set_key(19, 200);
    emu.set_key(23, 200);

    // Settle voices for 50 blocks
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    println!("FPSCR before audio: 0x{:08X}", emu.uc.reg_read(unicorn_engine::RegisterARM::FPSCR as i32).unwrap());

    // Profile basic blocks in 1 audio block
    {
        use std::sync::{Arc, Mutex};
        use std::collections::HashMap;
        let bb_map = Arc::new(Mutex::new(HashMap::<u32, (usize, usize)>::new()));
        let bb_clone = bb_map.clone();
        emu.uc.ctl_flush_tb().unwrap();
        let hook = emu.uc.add_block_hook(0x0800_0000, 0x0808_0000, move |_uc, addr, size| {
            let mut map = bb_clone.lock().unwrap();
            let entry = map.entry(addr as u32).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += size as usize;
        }).unwrap();

        let r_before = emu.uc.get_data().mmio_read_count;
        let w_before = emu.uc.get_data().mmio_write_count;
        let _ = emu.render_audio_block();
        let r_after = emu.uc.get_data().mmio_read_count;
        let w_after = emu.uc.get_data().mmio_write_count;
        let _ = emu.uc.remove_hook(hook);
        emu.uc.ctl_flush_tb().unwrap();
        println!("MMIO accesses during 1 audio block: reads={}, writes={}", r_after - r_before, w_after - w_before);

        let map = bb_map.lock().unwrap();
        let mut sorted: Vec<_> = map.iter().collect();
        sorted.sort_by_key(|&(_, &(_, bytes))| std::cmp::Reverse(bytes));
        let total_bytes: usize = sorted.iter().map(|(_, &(_, b))| b).sum();
        let total_blocks: usize = sorted.iter().map(|(_, &(c, _))| c).sum();
        println!("\n--- SINGLE BLOCK BASIC BLOCK PROFILE ---");
        println!("Total basic blocks executed: {}, total code bytes: ~{} (est. {} instructions)",
            total_blocks, total_bytes, total_bytes / 2);
        println!("Top 15 hottest basic blocks:");
        for (addr, (count, bytes)) in sorted.iter().take(15) {
            println!("  0x{:08X}: {} execs, {} bytes", addr, count, bytes);
        }
    }

    emu.profiling = true;
    emu.perf_step_mcu = std::time::Duration::ZERO;
    emu.perf_fast_forward = std::time::Duration::ZERO;
    emu.perf_systick = std::time::Duration::ZERO;
    emu.perf_coproc = std::time::Duration::ZERO;
    emu.perf_render_audio = std::time::Duration::ZERO;
    emu.perf_render_enter_exc = std::time::Duration::ZERO;
    emu.perf_render_emu_start = std::time::Duration::ZERO;
    emu.perf_render_exit_exc = std::time::Duration::ZERO;
    emu.perf_render_dsp = std::time::Duration::ZERO;
    emu.perf_usart = std::time::Duration::ZERO;

    println!("\n=== PARAPHONIC SCALING: 1, 2, 3, 4 VOICES ===");
    let keys = [12, 16, 19, 23];
    for n in 1..=4 {
        // Clear all keys first
        for &k in &keys {
            emu.set_key(k, 0);
        }
        for _ in 0..30 {
            emu.step_audio_block_quantum();
        }
        // Press n keys
        for &k in &keys[..n] {
            emu.set_key(k, 200);
        }
        for _ in 0..50 {
            emu.step_audio_block_quantum();
        }

        emu.profiling = true;
        emu.perf_step_mcu = std::time::Duration::ZERO;
        emu.perf_coproc = std::time::Duration::ZERO;
        emu.perf_render_audio = std::time::Duration::ZERO;
        emu.perf_systick = std::time::Duration::ZERO;

        let num_blocks = 200;
        let t_start = Instant::now();
        let mut non_zero = 0;
        for _ in 0..num_blocks {
            let blk = emu.step_audio_block_quantum();
            if blk.iter().any(|&b| b != 0) {
                non_zero += 1;
            }
        }
        let dur = t_start.elapsed();
        let virtual_time = (num_blocks as f64 * 32.0) / 48000.0;
        let real_time = dur.as_secs_f64();
        let speed = virtual_time / real_time;
        let per_block_us = dur.as_micros() as f64 / num_blocks as f64;
        let dma2_us = emu.perf_render_audio.as_micros() as f64 / num_blocks as f64;
        let mcu_us = emu.perf_step_mcu.as_micros() as f64 / num_blocks as f64;
        let coproc_us = emu.perf_coproc.as_micros() as f64 / num_blocks as f64;
        let oled_us = emu.coproc.time_oled.as_micros() as f64 / num_blocks as f64;
        let mat_us = emu.coproc.time_matrix.as_micros() as f64 / num_blocks as f64;
        let kbd_us = emu.coproc.time_kbd.as_micros() as f64 / num_blocks as f64;

        println!(
            "Paraphonic {} voices: {:5.1} µs/blk (speed: {:.2}x RT, nz: {:3}/{}) | DMA2: {:5.1} µs, MCU: {:4.1} µs, Coproc: {:4.1} µs (oled: {:4.1}, mat: {:4.1}, kbd: {:4.1})",
            n, per_block_us, speed, non_zero, num_blocks, dma2_us, mcu_us, coproc_us, oled_us, mat_us, kbd_us
        );
        emu.coproc.time_oled = std::time::Duration::ZERO;
        emu.coproc.time_matrix = std::time::Duration::ZERO;
        emu.coproc.time_kbd = std::time::Duration::ZERO;

    }

}

#[test]
fn test_benchmark_presets_mono() {
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
    let presets = repo_root.join("user_presets.bin");
    if presets.exists() {
        let _ = emu.load_qspi_from_disk(&presets);
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

    println!("\n=== PRESET MONO PERFORMANCE PROFILE (12 PRESETS) ===");
    for p in 1..=12 {
        if p > 1 {
            emu.step_preset_encoder(1);
            for _ in 0..300 {
                emu.step_audio_block_quantum();
            }
        }
        // Press key 12 (mono note)
        emu.set_key(12, 200);
        for _ in 0..20 {
            emu.step_audio_block_quantum();
        }

        emu.profiling = true;
        emu.perf_step_mcu = std::time::Duration::ZERO;
        emu.perf_coproc = std::time::Duration::ZERO;
        emu.perf_render_audio = std::time::Duration::ZERO;
        emu.perf_systick = std::time::Duration::ZERO;

        let num_blocks = 200;
        let t_start = Instant::now();
        let mut non_zero = 0;
        for _ in 0..num_blocks {
            let blk = emu.step_audio_block_quantum();
            if blk.iter().any(|&b| b != 0) {
                non_zero += 1;
            }
        }
        let dur = t_start.elapsed();
        let virtual_time = (num_blocks as f64 * 32.0) / 48000.0;
        let real_time = dur.as_secs_f64();
        let speed = virtual_time / real_time;
        let per_block_us = dur.as_micros() as f64 / num_blocks as f64;
        let dma2_us = emu.perf_render_audio.as_micros() as f64 / num_blocks as f64;
        let mcu_us = emu.perf_step_mcu.as_micros() as f64 / num_blocks as f64;
        let coproc_us = emu.perf_coproc.as_micros() as f64 / num_blocks as f64;

        let mut ep = [0u8; 4];
        let mut vt = [0u8; 4];
        let _ = emu.uc.mem_read(0x2000484C + 0x50AC, &mut ep);
        let engine_ptr = u32::from_le_bytes(ep);
        if engine_ptr != 0 {
            let _ = emu.uc.mem_read(engine_ptr as u64, &mut vt);
        }
        let vtable = u32::from_le_bytes(vt);

        println!(
            "Preset {:02}: {:5.1} µs/blk (speed: {:.2}x RT, nz: {:3}/{}) | DMA2: {:5.1} µs, MCU: {:4.1} µs, Coproc: {:4.1} µs | engine=0x{:08X} vtable=0x{:08X}",
            p, per_block_us, speed, non_zero, num_blocks, dma2_us, mcu_us, coproc_us, engine_ptr, vtable
        );

        // Release key 12
        emu.set_key(12, 0);
        for _ in 0..30 {
            emu.step_audio_block_quantum();
        }
    }
}

#[test]
fn test_benchmark_vanalog_paraphonic() {
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
    let presets = repo_root.join("user_presets.bin");
    if presets.exists() {
        let _ = emu.load_qspi_from_disk(&presets);
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

    // Step type encoder 5 times to reach V.Analog
    println!("\nStepping Type encoder to V.Analog...");
    for s in 1..=5 {
        emu.step_type_encoder(1);
        for _ in 0..50 {
            emu.step_audio_block_quantum();
        }
        for _ in 0..1100 {
            emu.step_audio_block_quantum();
        }
        println!("Type encoder step {} complete", s);
    }

    // Print OLED display text to verify
    let fb = emu.coproc.get_framebuffer();
    println!("--- OLED DISPLAY AFTER STEPPING TO V.ANALOG ---");
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

    // Toggle Paraphonic microswitch
    emu.set_microswitch("paraphonic", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("paraphonic", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    assert_eq!(emu.coproc.get_panel_leds()[28], 255, "Paraphonic LED must turn ON");

    // Inspect vtable for all 4 voices
    let voice_offsets = [0x2000484C, 0x2000A230, 0x2000FC14, 0x200155F8];
    for (i, &v_addr) in voice_offsets.iter().enumerate() {
        let mut ep = [0u8; 4];
        let mut vt = [0u8; 4];
        let _ = emu.uc.mem_read(v_addr + 0x50AC, &mut ep);
        let engine_ptr = u32::from_le_bytes(ep);
        if engine_ptr != 0 {
            let _ = emu.uc.mem_read(engine_ptr as u64, &mut vt);
        }
        let vtable = u32::from_le_bytes(vt);
        println!("Voice {}: engine_ptr=0x{:08X}, vtable=0x{:08X}", i, engine_ptr, vtable);
    }

    println!("Matrix nvic_ispr: 0x{:08X}, iser: 0x{:08X}, pending_wakeup: {}, R0=0x{:08X}, PC=0x{:08X}",
        emu.coproc.matrix.cpu.nvic_ispr, emu.coproc.matrix.cpu.nvic_iser, emu.coproc.matrix.cpu.has_pending_wakeup(),
        emu.coproc.matrix.cpu.regs[0], emu.coproc.matrix.cpu.regs[15]);
    println!("Matrix wakeup details: systick_pending={}, systick_ctrl=0x{:X}, systick_load={}, primask={}, pendsv={}, nvic_ispr=0x{:X}, usart1={}, adc={}, exti=0x{:X}, in_handler={}, active_exc={:?}",
        emu.coproc.matrix.cpu.systick_pending,
        emu.coproc.matrix.cpu.systick_ctrl,
        emu.coproc.matrix.cpu.systick_load,
        emu.coproc.matrix.cpu.primask,
        emu.coproc.matrix.cpu.pendsv_pending,
        emu.coproc.matrix.cpu.nvic_ispr,
        emu.coproc.matrix.cpu.usart1.is_interrupt_pending(),
        ((emu.coproc.matrix.cpu.adc_isr & emu.coproc.matrix.cpu.adc_ier) != 0),
        (emu.coproc.matrix.cpu.exti_pr & emu.coproc.matrix.cpu.exti_imr),
        emu.coproc.matrix.cpu.in_handler_mode,
        emu.coproc.matrix.cpu.active_exceptions);
    println!("Kbd nvic_ispr: 0x{:08X}, iser: 0x{:08X}, pending_wakeup: {}, R0=0x{:08X}, PC=0x{:08X}",
        emu.coproc.keyboard.cpu.nvic_ispr, emu.coproc.keyboard.cpu.nvic_iser, emu.coproc.keyboard.cpu.has_pending_wakeup(),
        emu.coproc.keyboard.cpu.regs[0], emu.coproc.keyboard.cpu.regs[15]);
    println!("Kbd wakeup details: systick_pending={}, systick_ctrl=0x{:X}, systick_load={}, primask={}, pendsv={}, nvic_ispr=0x{:X}, usart1={}, adc={}, exti=0x{:X}, in_handler={}, active_exc={:?}",
        emu.coproc.keyboard.cpu.systick_pending,
        emu.coproc.keyboard.cpu.systick_ctrl,
        emu.coproc.keyboard.cpu.systick_load,
        emu.coproc.keyboard.cpu.primask,
        emu.coproc.keyboard.cpu.pendsv_pending,
        emu.coproc.keyboard.cpu.nvic_ispr,
        emu.coproc.keyboard.cpu.usart1.is_interrupt_pending(),
        ((emu.coproc.keyboard.cpu.adc_isr & emu.coproc.keyboard.cpu.adc_ier) != 0),
        (emu.coproc.keyboard.cpu.exti_pr & emu.coproc.keyboard.cpu.exti_imr),
        emu.coproc.keyboard.cpu.in_handler_mode,
        emu.coproc.keyboard.cpu.active_exceptions);
    println!("OLED nvic_ispr: 0x{:08X}, iser: 0x{:08X}, pending_wakeup: {}, R0=0x{:08X}, PC=0x{:08X}",
        emu.coproc.oled.cpu.nvic_ispr, emu.coproc.oled.cpu.nvic_iser, emu.coproc.oled.cpu.has_pending_wakeup(),
        emu.coproc.oled.cpu.regs[0], emu.coproc.oled.cpu.regs[15]);
    println!("OLED wakeup details: systick_pending={}, systick_ctrl=0x{:X}, systick_load={}, primask={}, pendsv={}, in_handler={}",
        emu.coproc.oled.cpu.systick_pending,
        emu.coproc.oled.cpu.systick_ctrl,
        emu.coproc.oled.cpu.systick_load,
        emu.coproc.oled.cpu.primask,
        emu.coproc.oled.cpu.pendsv_pending,
        emu.coproc.oled.cpu.in_handler_mode);

    println!("\n=== V.ANALOG PARAPHONIC SCALING: 1, 2, 3, 4 VOICES ===");
    let keys = [12, 16, 19, 23];
    for n in 1..=4 {
        // Clear all keys first
        for &k in &keys {
            emu.set_key(k, 0);
        }
        for _ in 0..30 {
            emu.step_audio_block_quantum();
        }
        // Press n keys
        for &k in &keys[..n] {
            emu.set_key(k, 200);
        }
        for _ in 0..70 {
            emu.step_audio_block_quantum();
        }

        emu.profiling = true;
        emu.perf_step_mcu = std::time::Duration::ZERO;
        emu.perf_coproc = std::time::Duration::ZERO;
        emu.perf_render_audio = std::time::Duration::ZERO;
        emu.perf_render_emu_start = std::time::Duration::ZERO;
        emu.perf_render_dsp = std::time::Duration::ZERO;
        emu.perf_systick = std::time::Duration::ZERO;
        emu.perf_usart = std::time::Duration::ZERO;
        emu.coproc.time_oled = std::time::Duration::ZERO;
        emu.coproc.time_matrix = std::time::Duration::ZERO;
        emu.coproc.time_kbd = std::time::Duration::ZERO;

        let num_blocks = 200;
        let t_start = Instant::now();
        let mut non_zero = 0;
        for _ in 0..num_blocks {
            let blk = emu.step_audio_block_quantum();
            if blk.iter().any(|&b| b != 0) {
                non_zero += 1;
            }
        }
        let dur = t_start.elapsed();
        let virtual_time = (num_blocks as f64 * 32.0) / 48000.0;
        let real_time = dur.as_secs_f64();
        let speed = virtual_time / real_time;
        let per_block_us = dur.as_micros() as f64 / num_blocks as f64;
        let dma2_us = emu.perf_render_audio.as_micros() as f64 / num_blocks as f64;
        let emu_us = emu.perf_render_emu_start.as_micros() as f64 / num_blocks as f64;
        let dsp_us = emu.perf_render_dsp.as_micros() as f64 / num_blocks as f64;
        let mcu_us = emu.perf_step_mcu.as_micros() as f64 / num_blocks as f64;
        let systick_us = emu.perf_systick.as_micros() as f64 / num_blocks as f64;
        let usart_us = emu.perf_usart.as_micros() as f64 / num_blocks as f64;
        let coproc_us = emu.perf_coproc.as_micros() as f64 / num_blocks as f64;

        println!(
            "V.Analog Paraphonic {} voices: {:5.1} µs/blk (speed: {:.2}x RT, nz: {:3}/{}) | DMA2: {:5.1} µs (Unicorn: {:5.1} µs, DSP: {:4.1} µs), MCU: {:4.1} µs, SysTick: {:4.1} µs, USART: {:4.1} µs, Coproc: {:4.1} µs",
            n, per_block_us, speed, non_zero, num_blocks, dma2_us, emu_us, dsp_us, mcu_us, systick_us, usart_us, coproc_us
        );
        println!(
            "   Coproc breakdown: OLED={:?}, Matrix={:?}, Kbd={:?} | ff_hits: Matrix={}, Kbd={}, OLED={}",
            emu.coproc.time_oled, emu.coproc.time_matrix, emu.coproc.time_kbd,
            emu.coproc.matrix.cpu.ff_hits, emu.coproc.keyboard.cpu.ff_hits, emu.coproc.oled.cpu.ff_hits
        );
    }

    // Now profile basic blocks with 4 voices active
    println!("\n--- V.ANALOG 4-VOICE BASIC BLOCK PROFILE ---");
    {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        let bb_map = Arc::new(Mutex::new(HashMap::<u32, (usize, usize)>::new()));
        let bb_clone = bb_map.clone();
        emu.uc.ctl_flush_tb().unwrap();
        let hook = emu
            .uc
            .add_block_hook(0x0800_0000, 0x0808_0000, move |_uc, addr, size| {
                let mut map = bb_clone.lock().unwrap();
                let entry = map.entry(addr as u32).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += size as usize;
            })
            .unwrap();

        let r_before = emu.uc.get_data().mmio_read_count;
        let w_before = emu.uc.get_data().mmio_write_count;
        let _ = emu.render_audio_block();
        let r_after = emu.uc.get_data().mmio_read_count;
        let w_after = emu.uc.get_data().mmio_write_count;
        let _ = emu.uc.remove_hook(hook);
        emu.uc.ctl_flush_tb().unwrap();
        println!(
            "MMIO accesses during 1 audio block: reads={}, writes={}",
            r_after - r_before,
            w_after - w_before
        );

        let map = bb_map.lock().unwrap();
        let mut sorted: Vec<_> = map.iter().collect();
        sorted.sort_by_key(|&(_, &(_, bytes))| std::cmp::Reverse(bytes));
        let total_bytes: usize = sorted.iter().map(|(_, &(_, b))| b).sum();
        let total_blocks: usize = sorted.iter().map(|(_, &(c, _))| c).sum();
        println!(
            "Total basic blocks executed: {}, total code bytes: ~{} (est. {} instructions)",
            total_blocks,
            total_bytes,
            total_bytes / 2
        );
        println!("Top 20 hottest basic blocks:");
        for (addr, (count, bytes)) in sorted.iter().take(20) {
            println!("  0x{:08X}: {} execs, {} bytes", addr, count, bytes);
        }
    }
}

#[test]
fn test_benchmark_vocoder_performance() {
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
    emu.set_headset_jack(vibefreak_core::peripherals::tlv320aic3104::HeadsetJackState::HeadsetWithMic);

    // Build and load Vocoder preset into Slot 1
    let vocoder_patch = vibefreak_core::factory_data::FactoryPatchDef {
        name: "Vocoder Classic",
        engine_type: 14,
        cutoff: 0x7FFF,
        resonance: 0,
        filter_mode: 0,
        attack: 10,
        decay: 1500,
        sustain: 0x7FFF,
    };
    let p_data = vibefreak_core::factory_data::build_factory_preset(1, "Vocoder Classic", Some(&vocoder_patch));
    let mut flash = vibefreak_core::factory_data::generate_factory_qspi_image();
    flash[0x00081000..0x00081000 + 4096].copy_from_slice(&p_data);
    let temp_qspi = std::env::temp_dir().join("bench_vocoder_qspi.bin");
    std::fs::write(&temp_qspi, &flash).unwrap();
    emu.load_qspi_from_disk(&temp_qspi).unwrap();
    let _ = std::fs::remove_file(&temp_qspi);

    emu.boot(100_000_000);

    for _ in 0..1500 {
        emu.step_audio_block_quantum();
        let fb = emu.coproc.get_framebuffer();
        if fb.iter().filter(|&&b| b != 0).count() > 400 {
            break;
        }
    }

    // Press a key
    emu.set_key(12, 200);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }

    let sample_rate = 48000.0f32;
    let freq = 440.0f32;
    let mut phase = 0.0f32;

    println!("\n=== VOCODER MODULATION PERFORMANCE PROFILE ===");
    emu.profiling = true;
    emu.perf_step_mcu = std::time::Duration::ZERO;
    emu.perf_fast_forward = std::time::Duration::ZERO;
    emu.perf_systick = std::time::Duration::ZERO;
    emu.perf_coproc = std::time::Duration::ZERO;
    emu.perf_render_audio = std::time::Duration::ZERO;
    emu.perf_render_emu_start = std::time::Duration::ZERO;
    emu.perf_render_exit_exc = std::time::Duration::ZERO;
    emu.perf_render_dsp = std::time::Duration::ZERO;

    let num_blocks = 200;
    let t_start = Instant::now();
    let mut non_zero = 0;
    for _ in 0..num_blocks {
        let mut mic_frames = [0.0f32; 32];
        for f in &mut mic_frames {
            *f = (phase * 2.0 * std::f32::consts::PI).sin() * 0.8;
            phase += freq / sample_rate;
            if phase >= 1.0 {
                phase -= 1.0;
            }
        }
        emu.feed_audio_input(&mic_frames);
        let blk = emu.step_audio_block_quantum();
        if blk.iter().any(|&b| b != 0) {
            non_zero += 1;
        }
    }

    let dur = t_start.elapsed();
    let virtual_time = (num_blocks as f64 * 32.0) / 48000.0;
    let real_time = dur.as_secs_f64();
    let speed = virtual_time / real_time;
    let per_block_us = dur.as_micros() as f64 / num_blocks as f64;
    let dma2_us = emu.perf_render_audio.as_micros() as f64 / num_blocks as f64;
    let mcu_us = emu.perf_step_mcu.as_micros() as f64 / num_blocks as f64;
    let coproc_us = emu.perf_coproc.as_micros() as f64 / num_blocks as f64;

    println!(
        "Vocoder 1 voice with mic input: {:5.1} µs/blk (speed: {:.2}x RT, nz: {:3}/{}) | DMA2: {:5.1} µs (Unicorn: {:5.1} µs), MCU: {:5.1} µs, Coproc: {:4.1} µs",
        per_block_us, speed, non_zero, num_blocks,
        dma2_us, emu.perf_render_emu_start.as_micros() as f64 / num_blocks as f64,
        mcu_us, coproc_us
    );

    // Profile basic blocks during Vocoder execution
    println!("\n--- VOCODER BASIC BLOCK PROFILE ---");
    {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        let bb_map = Arc::new(Mutex::new(HashMap::<u32, (usize, usize)>::new()));
        let bb_clone = bb_map.clone();
        emu.uc.ctl_flush_tb().unwrap();
        let hook = emu
            .uc
            .add_block_hook(0x0800_0000, 0x0808_0000, move |_uc, addr, size| {
                let mut map = bb_clone.lock().unwrap();
                let entry = map.entry(addr as u32).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += size as usize;
            })
            .unwrap();

        let mut mic_frames = [0.0f32; 32];
        for f in &mut mic_frames {
            *f = (phase * 2.0 * std::f32::consts::PI).sin() * 0.8;
            phase += freq / sample_rate;
            if phase >= 1.0 {
                phase -= 1.0;
            }
        }
        emu.feed_audio_input(&mic_frames);
        let _ = emu.step_audio_block_quantum();
        let _ = emu.uc.remove_hook(hook);
        emu.uc.ctl_flush_tb().unwrap();

        let map = bb_map.lock().unwrap();
        let mut sorted: Vec<_> = map.iter().collect();
        sorted.sort_by_key(|&(_, &(_, bytes))| std::cmp::Reverse(bytes));
        let total_bytes: usize = sorted.iter().map(|(_, &(_, b))| b).sum();
        let total_blocks: usize = sorted.iter().map(|(_, &(c, _))| c).sum();
        println!(
            "Total basic blocks executed in 1 vocoder block: {}, total code bytes: ~{} (est. {} instructions)",
            total_blocks,
            total_bytes,
            total_bytes / 2
        );
        println!("Top 20 hottest basic blocks:");
        for (addr, (count, bytes)) in sorted.iter().take(20) {
            println!("  0x{:08X}: {} execs, {} bytes", addr, count, bytes);
        }
    }
}
