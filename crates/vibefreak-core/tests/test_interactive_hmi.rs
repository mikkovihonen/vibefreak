use vibefreak_core::VibeFreakEmulator;
use std::path::Path;

#[test]
fn test_hmi_responsiveness_on_patch_screen() {
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

    eprintln!("[2] Stepping to initial patch screen...");
    // Run until non-zero OLED framebuffer bytes indicate patch screen has loaded
    for step in 0..3000 {
        emu.step_audio_block_quantum();
        if step % 250 == 0 {
            let nz = emu.coproc.get_framebuffer().iter().filter(|&&b| b != 0).count();
            let mut tick_bytes = [0u8; 4];
            let _ = emu.uc.mem_read(0x2001_E1D8, &mut tick_bytes);
            let uwtick = u32::from_le_bytes(tick_bytes);
            let matrix_leds = emu.coproc.get_matrix_leds();
            let panel_leds = emu.coproc.get_panel_leds();
            let matrix_leds_lit = matrix_leds.iter().filter(|&&b| b != 0).count();
            let panel_leds_lit = panel_leds.iter().filter(|&&b| b != 0).count();
            eprintln!(
                "Step {}: PC=0x{:08X}, uwTick={}, OLED nz={}, MatrixLit={}, PanelLit={}",
                step, emu.pc, uwtick, nz, matrix_leds_lit, panel_leds_lit
            );
            if step >= 1250 && nz > 50 && panel_leds_lit > 0 {
                break;
            }
        }
    }

    let matrix_leds = emu.coproc.get_matrix_leds();
    let panel_leds = emu.get_panel_leds();
    let matrix_leds_lit = matrix_leds.iter().filter(|&&b| b != 0).count();
    let panel_leds_lit = panel_leds.iter().filter(|&&b| b != 0).count();
    eprintln!("[3] On Patch Screen: MatrixLit={}, PanelLit={}", matrix_leds_lit, panel_leds_lit);

    eprintln!("[3b] Testing Key 12 on Initial Patch Screen...");
    emu.set_key(12, 200);
    let mut nz_patch1 = 0;
    for _ in 0..150 {
        let b = emu.step_audio_block_quantum();
        if b.iter().any(|&x| x != 0) {
            nz_patch1 += 1;
        }
    }
    eprintln!("Key 12 on Patch 1: non_zero_blocks = {}", nz_patch1);
    assert!(nz_patch1 > 0, "Keybed touch must synthesize audio blocks");
    emu.set_key(12, 0);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    eprintln!("[4] Testing Filter Type switch (LPF -> BPF)...");
    let panel_leds_before = emu.get_panel_leds();
    eprintln!("panel_leds_before[7..10]: {:?}", &panel_leds_before[7..10]);
    assert_eq!(panel_leds_before[7], 255, "LPF LED must be ON initially");
    assert_eq!(panel_leds_before[8], 0, "BPF LED must be OFF initially");
    assert_eq!(panel_leds_before[9], 0, "HPF LED must be OFF initially");

    let fb_before_filter = emu.coproc.get_framebuffer();

    // Advance physical switch from LPF (Position 0) to BPF (Position 1)
    emu.set_microswitch("filter_type", true);
    for _ in 0..150 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("filter_type", false);
    for _ in 0..150 {
        emu.step_audio_block_quantum();
    }
    let panel_leds_bpf = emu.get_panel_leds();
    eprintln!("After Filter Type (LPF -> BPF): Panel LEDs: {:?}", &panel_leds_bpf[..]);
    assert_eq!(panel_leds_bpf[7], 0, "LPF LED must turn off");
    assert_eq!(panel_leds_bpf[8], 255, "BPF LED must turn on");

    let fb_after_filter = emu.coproc.get_framebuffer();
    assert_ne!(
        fb_before_filter, fb_after_filter,
        "Filter Type button press must trigger filter graphic popup on OLED display"
    );

    eprintln!("[4b] Testing Filter Type switch (BPF -> HPF)...");
    // Advance physical switch from BPF (Position 1) to HPF (Position 2)
    emu.set_microswitch("filter_type", true);
    for _ in 0..150 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("filter_type", false);
    for _ in 0..150 {
        emu.step_audio_block_quantum();
    }
    let panel_leds_hpf = emu.get_panel_leds();
    eprintln!("After Filter Type (BPF -> HPF): Panel LEDs: {:?}", &panel_leds_hpf[..]);
    assert_eq!(panel_leds_hpf[8], 0, "BPF LED must turn off");
    assert_eq!(panel_leds_hpf[9], 255, "HPF LED must turn on");

    eprintln!("[5a] Testing Panel microswitch toggle...");
    let fb_before_panel = emu.coproc.get_framebuffer();
    emu.set_microswitch("panel", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("panel", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let fb_after_panel = emu.coproc.get_framebuffer();
    eprintln!("Panel: OLED changed={}", fb_before_panel != fb_after_panel);

    eprintln!("[5b] Testing Cycling Env microswitch press...");
    let p_before_cyc = emu.get_panel_leds();
    eprintln!("Before cyc_mode: Panel LEDs: {:?}", &p_before_cyc[..]);
    emu.set_microswitch("cyc_mode", true);
    for _s in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("cyc_mode", false);
    for _s in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_leds_cyc = emu.get_panel_leds();
    eprintln!("After Cycling Env mode: Panel LEDs: {:?}", &p_leds_cyc[..]);

    let p_leds_before_para = emu.coproc.get_panel_leds();
    eprintln!("Before Paraphonic toggle: LED[28] = {}", p_leds_before_para[28]);
    assert_eq!(p_leds_before_para[28], 0, "Paraphonic LED must be OFF initially in preset 1");
    let fb_before_para = emu.coproc.get_framebuffer();

    // Actuate physical Paraphonic microswitch:
    // On physical Arturia MicroFreak hardware, toggling Paraphonic mode does NOT trigger an OLED popup;
    // status feedback is strictly physical optical luminescence via the dedicated white LED under the button (panel_leds[28]).
    emu.set_microswitch("paraphonic", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("paraphonic", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_leds_para_on = emu.coproc.get_panel_leds();
    eprintln!("After Paraphonic toggle 1 (OFF -> ON): LED[28] = {}", p_leds_para_on[28]);
    assert_eq!(p_leds_para_on[28], 255, "Paraphonic LED must turn ON");
    assert_eq!(
        emu.coproc.get_framebuffer(), fb_before_para,
        "Authentic hardware parity: Paraphonic button toggle must not alter OLED (dedicated button LED indicates status)"
    );

    emu.set_microswitch("paraphonic", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("paraphonic", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_leds_para_off = emu.coproc.get_panel_leds();
    eprintln!("After Paraphonic toggle 2 (ON -> OFF): LED[28] = {}", p_leds_para_off[28]);
    assert_eq!(p_leds_para_off[28], 0, "Paraphonic LED must turn back OFF");

    eprintln!("[5d] Testing Amp Mod microswitch toggle...");
    let p_leds_before_amp = emu.coproc.get_panel_leds();
    eprintln!("Amp Mod LED before: {}", p_leds_before_amp[32]);
    assert_eq!(p_leds_before_amp[32], 255, "Amp Mod LED must be ON initially in preset 1");
    emu.set_microswitch("amp_mod", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("amp_mod", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_leds_amp_off = emu.coproc.get_panel_leds();
    eprintln!("After Amp Mod toggle 1 (ON -> OFF): LED[32] = {}", p_leds_amp_off[32]);
    assert_eq!(p_leds_amp_off[32], 0, "Amp Mod LED must turn OFF");

    emu.set_microswitch("amp_mod", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("amp_mod", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_leds_amp_on = emu.coproc.get_panel_leds();
    eprintln!("After Amp Mod toggle 2 (OFF -> ON): LED[32] = {}", p_leds_amp_on[32]);
    assert_eq!(p_leds_amp_on[32], 255, "Amp Mod LED must turn back ON");

    eprintln!("[5e] Testing Utility microswitch toggle...");

    assert_eq!(emu.coproc.get_panel_leds()[30], 0, "Utility LED must be OFF initially");
    let fb_before_util = emu.coproc.get_framebuffer();
    emu.set_microswitch("utility", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("utility", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_leds_util_on = emu.coproc.get_panel_leds();
    let fb_after_util_on = emu.coproc.get_framebuffer();
    eprintln!("After Utility ON: Panel LEDs = {:?}", &p_leds_util_on[28..35]);
    assert_eq!(p_leds_util_on[30], 255, "Utility LED must turn ON when opening utility menu");
    assert_ne!(fb_before_util, fb_after_util_on, "OLED framebuffer must display Utility menu");

    // Toggle utility menu off
    emu.set_microswitch("utility", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("utility", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_leds_util_off = emu.coproc.get_panel_leds();
    eprintln!("After Utility OFF: Panel LEDs = {:?}", &p_leds_util_off[28..35]);
    assert_eq!(p_leds_util_off[30], 0, "Utility LED must turn OFF when closing utility menu");

    eprintln!("[5f] Testing LFO Shape microswitch (Sine -> Triangle)...");
    let p_leds_lfo_init = emu.coproc.get_panel_leds();
    eprintln!("LFO LEDs before: {:?}", &p_leds_lfo_init[0..6]);
    assert_eq!(p_leds_lfo_init[0], 255, "LFO Sine LED must be ON initially");
    assert_eq!(p_leds_lfo_init[1], 0, "LFO Triangle LED must be OFF initially");
    emu.set_microswitch("lfo_shape", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("lfo_shape", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_leds_lfo_after = emu.coproc.get_panel_leds();
    eprintln!("LFO LEDs after: {:?}", &p_leds_lfo_after[0..6]);
    assert_eq!(p_leds_lfo_after[0], 0, "LFO Sine LED must turn OFF");
    assert_eq!(p_leds_lfo_after[1], 255, "LFO Triangle LED must turn ON");

    eprintln!("[5g] Testing LFO Sync microswitch toggle (lfo_rate encoder click)...");
    let p_lfo_sync_before = emu.coproc.get_panel_leds()[6];
    eprintln!("LFO Sync LED before: {}", p_lfo_sync_before);
    emu.set_microswitch("lfo_rate", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("lfo_rate", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_lfo_sync_after = emu.coproc.get_panel_leds()[6];
    eprintln!("LFO Sync LED after: {}", p_lfo_sync_after);
    assert_ne!(p_lfo_sync_before, p_lfo_sync_after, "LFO Sync LED must toggle on lfo_rate encoder click");

    eprintln!("[5h] Testing Octave Down microswitch...");
    let oct_down_before = emu.coproc.get_panel_leds()[24];
    eprintln!("Oct Down LED before: {}", oct_down_before);
    emu.set_microswitch("oct_down", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("oct_down", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let oct_down_after = emu.coproc.get_panel_leds()[24];
    eprintln!("Oct Down LED after: {}", oct_down_after);
    assert_eq!(oct_down_after, 255, "Octave Down LED must be lit after Octave Down press");

    eprintln!("[5i] Testing Octave Up microswitch...");
    let oct_up_before = emu.coproc.get_panel_leds()[25];
    eprintln!("Oct Up LED before: {}", oct_up_before);
    emu.set_microswitch("oct_up", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("oct_up", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let oct_down_after2 = emu.coproc.get_panel_leds()[24];
    let oct_up_after = emu.coproc.get_panel_leds()[25];
    eprintln!("After Oct Up 1: Oct Down LED={}, Oct Up LED={}", oct_down_after2, oct_up_after);
    assert_eq!(oct_down_after2, 0, "Octave Down LED must turn off at center octave (0)");
    assert_eq!(oct_up_after, 0, "Octave Up LED must be off at center octave (0)");

    // Press Octave Up once more to reach +1:
    emu.set_microswitch("oct_up", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("oct_up", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let oct_down_after3 = emu.coproc.get_panel_leds()[24];
    let oct_up_after2 = emu.coproc.get_panel_leds()[25];
    eprintln!("After Oct Up 2: Oct Down LED={}, Oct Up LED={}", oct_down_after3, oct_up_after2);
    assert_eq!(oct_down_after3, 0, "Octave Down LED must be off at +1 octave");
    assert_eq!(oct_up_after2, 255, "Octave Up LED must be lit at +1 octave");

    eprintln!("[5j] Testing Arp/Seq toggle microswitch (Arp alone and Shift + Seq)...");
    let p_arp_white_before = emu.coproc.get_panel_leds()[33];
    let p_arp_blue_before = emu.coproc.get_panel_leds()[34];
    eprintln!("Arp/Seq LEDs before: White={}, Blue={}", p_arp_white_before, p_arp_blue_before);
    assert_eq!(p_arp_white_before, 0);
    assert_eq!(p_arp_blue_before, 0);

    // 1. Press Arp/Seq without shift -> Arpeggiator ON (White LED lit)
    emu.set_microswitch("arp_seq", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_arp_white_after = emu.coproc.get_panel_leds()[33];
    let p_arp_blue_after = emu.coproc.get_panel_leds()[34];
    eprintln!("Arp/Seq LEDs after Arp toggle ON: White={}, Blue={}", p_arp_white_after, p_arp_blue_after);
    assert_eq!(p_arp_white_after, 255, "Arpeggiator White LED must be ON after Arp/Seq press without shift");
    assert_eq!(p_arp_blue_after, 0, "Sequencer Blue LED must be OFF");

    // 2. Press Arp/Seq without shift again -> Arpeggiator OFF (White LED off)
    emu.set_microswitch("arp_seq", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    assert_eq!(emu.coproc.get_panel_leds()[33], 0, "Arpeggiator White LED must turn OFF after second press");
    assert_eq!(emu.coproc.get_panel_leds()[34], 0, "Sequencer Blue LED must remain OFF");

    // 3. Shift + Arp/Seq -> Sequencer ON (Blue LED lit)
    emu.set_microswitch("shift", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("shift", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_arp_white_seq = emu.coproc.get_panel_leds()[33];
    let p_arp_blue_seq = emu.coproc.get_panel_leds()[34];
    eprintln!("Arp/Seq LEDs after Shift + Arp/Seq (Seq ON): White={}, Blue={}", p_arp_white_seq, p_arp_blue_seq);
    assert_eq!(p_arp_white_seq, 0, "Arpeggiator White LED must be OFF in Sequencer mode");
    assert_eq!(p_arp_blue_seq, 255, "Sequencer Blue LED must be ON after Shift + Arp/Seq");

    // 4. Press Arp/Seq without shift -> Sequencer OFF (Blue LED off)
    emu.set_microswitch("arp_seq", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    assert_eq!(emu.coproc.get_panel_leds()[33], 0, "Arpeggiator White LED must remain OFF");
    assert_eq!(emu.coproc.get_panel_leds()[34], 0, "Sequencer Blue LED must turn OFF after Arp/Seq press");

    // 5. Shift + Arp/Seq to turn Sequencer ON, then Shift + Arp/Seq to switch directly to Arpeggiator
    emu.set_microswitch("shift", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("shift", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    assert_eq!(emu.coproc.get_panel_leds()[33], 0, "Arp White LED must be OFF in Sequencer mode");
    assert_eq!(emu.coproc.get_panel_leds()[34], 255, "Sequencer Blue LED must be ON");

    // Mode switch: Shift + Arp/Seq while Sequencer is ON switches back to Arpeggiator
    emu.set_microswitch("shift", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("shift", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    assert_eq!(emu.coproc.get_panel_leds()[33], 255, "Arpeggiator White LED must be ON after mode switch");
    assert_eq!(emu.coproc.get_panel_leds()[34], 0, "Sequencer Blue LED must be OFF after mode switch");

    // Turn Arp OFF
    emu.set_microswitch("arp_seq", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    assert_eq!(emu.coproc.get_panel_leds()[33], 0, "Arpeggiator White LED must turn OFF");
    assert_eq!(emu.coproc.get_panel_leds()[34], 0, "Sequencer Blue LED must be OFF");

    eprintln!("[5k] Testing Up | A microswitch...");
    let p_up_before = emu.coproc.get_icon_leds()[1];
    eprintln!("Up | A LED before: {}", p_up_before);
    emu.set_microswitch("up", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("up", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_up_after = emu.coproc.get_icon_leds()[1];
    eprintln!("Up | A LED after: {}", p_up_after);

    eprintln!("[5l] Testing Order | B microswitch...");
    let p_order_before = emu.coproc.get_icon_leds()[2];
    eprintln!("Order | B LED before: {}", p_order_before);
    emu.set_microswitch("order", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("order", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_order_after = emu.coproc.get_icon_leds()[2];
    eprintln!("Order | B LED after: {}", p_order_after);

    eprintln!("[5m] Testing Assign 1, Assign 2, Assign 3 microswitches...");
    for (i, assign_name) in ["assign_1", "assign_2", "assign_3"].iter().enumerate() {
        eprintln!("Testing microswitch: {}", assign_name);
        let mut buf_before = [0u8; 4];
        let _ = emu.uc.mem_read(0x20003714 + 0x14, &mut buf_before);
        let f14_before = u32::from_le_bytes(buf_before);
        let fb_before = emu.coproc.get_framebuffer();
        emu.set_microswitch(assign_name, true);
        for _ in 0..100 {
            emu.step_audio_block_quantum();
        }
        let mut buf_pressed = [0u8; 4];
        let _ = emu.uc.mem_read(0x20003714 + 0x14, &mut buf_pressed);
        let f14_pressed = u32::from_le_bytes(buf_pressed);
        let fb_pressed = emu.coproc.get_framebuffer();
        let pleds_pressed = emu.coproc.get_panel_leds();
        emu.set_microswitch(assign_name, false);
        for _ in 0..200 {
            emu.step_audio_block_quantum();
        }
        let mut buf_after = [0u8; 4];
        let _ = emu.uc.mem_read(0x20003714 + 0x14, &mut buf_after);
        let f14_after = u32::from_le_bytes(buf_after);
        let fb_after = emu.coproc.get_framebuffer();
        let pleds_after = emu.coproc.get_panel_leds();
        eprintln!("{}: f14: before=0x{:08X}, pressed=0x{:08X}, after=0x{:08X}", assign_name, f14_before, f14_pressed, f14_after);
        eprintln!("{}: OLED nz: before={}, pressed={}, after={}, pressed!=before={}, after!=before={}",
            assign_name,
            fb_before.iter().filter(|&&b| b != 0).count(),
            fb_pressed.iter().filter(|&&b| b != 0).count(),
            fb_after.iter().filter(|&&b| b != 0).count(),
            fb_pressed != fb_before,
            fb_after != fb_before
        );
        eprintln!("{}: pressed LED[21..24]={:?}, after LED[21..24]={:?}",
            assign_name, &pleds_pressed[21..24], &pleds_after[21..24]);
        assert_eq!(f14_pressed, 1 << (2 + i), "{} press must set bit {} in g_PanelPresetCtrl->field_0x14_u32", assign_name, 2 + i);
        assert_eq!(f14_after, 0, "{} release must clear bit in g_PanelPresetCtrl->field_0x14_u32", assign_name);
        assert_eq!(pleds_pressed[21 + i], 255, "{} LED (panel_leds[{}]) must be lit while pressed", assign_name, 21 + i);
        assert_eq!(pleds_after[21 + i], 0, "{} LED (panel_leds[{}]) must be off after release", assign_name, 21 + i);
    }

    eprintln!("[5n] Testing Hold microswitch...");
    let fb_before_hold = emu.coproc.get_framebuffer();
    emu.set_microswitch("hold", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("hold", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let fb_after_hold = emu.coproc.get_framebuffer();
    eprintln!("Hold: OLED changed={}", fb_before_hold != fb_after_hold);

    eprintln!("[5o] Testing Save microswitch...");
    let p_save_before = emu.coproc.get_panel_leds()[31];
    let fb_before_save = emu.coproc.get_framebuffer();
    emu.set_microswitch("save", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("save", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_save_after = emu.coproc.get_panel_leds()[31];
    let fb_after_save = emu.coproc.get_framebuffer();
    eprintln!("Save: LED[31] before={}, after={}, OLED changed={}", p_save_before, p_save_after, fb_before_save != fb_after_save);

    // Toggle Save OFF to return to patch play mode
    emu.set_microswitch("save", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("save", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let p_save_off = emu.coproc.get_panel_leds()[31];
    eprintln!("Save: LED[31] after toggle off={}", p_save_off);

    eprintln!("[5p] Testing Spice, Dice, Bend touch buttons...");
    for touch_name in ["spice", "dice", "bend"] {
        eprintln!("Testing microswitch: {}", touch_name);
        let fb_before = emu.coproc.get_framebuffer();
        emu.set_microswitch(touch_name, true);
        for _ in 0..100 {
            emu.step_audio_block_quantum();
        }
        emu.set_microswitch(touch_name, false);
        for _ in 0..200 {
            emu.step_audio_block_quantum();
        }
        let fb_after = emu.coproc.get_framebuffer();
        eprintln!("{}: OLED changed={}", touch_name, fb_before != fb_after);
    }

    eprintln!("[5q] Testing Shift microswitch...");
    emu.set_microswitch("shift", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    let mut panel_preset_flags = [0u8; 4];
    emu.uc.mem_read(0x20003714 + 0x14, &mut panel_preset_flags).unwrap();
    let flags = u32::from_le_bytes(panel_preset_flags);
    eprintln!("Shift active in firmware: 0x20003714+0x14=0x{:08X}", flags);
    assert_ne!(flags & 2, 0, "Shift button press must set bit 1 in g_PanelPresetCtrl->field_0x14_u32");

    emu.set_microswitch("shift", false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let mut panel_preset_rel = [0u8; 4];
    emu.uc.mem_read(0x20003714 + 0x14, &mut panel_preset_rel).unwrap();
    let rel_flags = u32::from_le_bytes(panel_preset_rel);
    eprintln!("Shift released in firmware: 0x20003714+0x14=0x{:08X}", rel_flags);
    assert_eq!(rel_flags & 2, 0, "Shift button release must clear bit 1 in g_PanelPresetCtrl->field_0x14_u32");

    eprintln!("[5r] Testing Preset and Matrix encoder clicks...");
    for enc_click in ["preset", "matrix"] {
        emu.set_microswitch(enc_click, true);
        for _ in 0..100 {
            emu.step_audio_block_quantum();
        }
        emu.set_microswitch(enc_click, false);
        for _ in 0..200 {
            emu.step_audio_block_quantum();
        }
    }

    // Rate encoder push switch toggles Arp Sync:
    eprintln!("[5r2] Testing Rate encoder push switch (Arp Sync)...");
    let fb_before_rate = emu.coproc.get_framebuffer();
    emu.set_microswitch("rate", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("rate", false);
    let mut rate_fb_changed = false;
    for _ in 0..300 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_framebuffer() != fb_before_rate {
            rate_fb_changed = true;
            break;
        }
    }
    eprintln!("Rate encoder click (Arp Sync toggle): OLED changed={}", rate_fb_changed);
    assert!(rate_fb_changed, "Rate encoder click must trigger OLED update (Arp Sync toggle)");

    eprintln!("[5s] Testing Oct | Mod microswitch...");
    let fb_before_mod = emu.coproc.get_framebuffer();
    emu.set_microswitch("oct_mod", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("oct_mod", false);
    let mut mod_fb_changed = false;
    for _ in 0..300 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_framebuffer() != fb_before_mod {
            mod_fb_changed = true;
            break;
        }
    }
    eprintln!("Oct | Mod: OLED changed={}", mod_fb_changed);
    assert!(mod_fb_changed, "Oct | Mod microswitch must trigger OLED update (cycles Arp Octave)");

    eprintln!("[6] Testing Matrix encoder delta=+1...");
    emu.step_matrix_encoder(1);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let matrix_leds = emu.coproc.get_matrix_leds();
    let matrix_leds_after = matrix_leds.iter().filter(|&&b| b != 0).count();
    eprintln!("[6b] After Matrix encoder: MatrixLit={}", matrix_leds_after);
    eprintln!("Matrix LEDs: {:?}", &matrix_leds[..]);

    let mut preset_bytes_before = [0u8; 2];
    emu.uc.mem_read(0x2000_0BDC, &mut preset_bytes_before).unwrap();
    let preset_before = u16::from_le_bytes(preset_bytes_before);
    eprintln!("[7] Testing Preset encoder delta=+1 (Preset before = {})...", preset_before);
    assert_eq!(preset_before, 0, "Initial preset index must be 0");

    emu.step_preset_encoder(1);
    for _ in 0..300 {
        emu.step_audio_block_quantum();
    }
    let mut preset_bytes_after = [0u8; 2];
    emu.uc.mem_read(0x2000_0BDC, &mut preset_bytes_after).unwrap();
    let preset_after = u16::from_le_bytes(preset_bytes_after);
    eprintln!("[7b] Preset after delta=+1: {}", preset_after);
    assert_eq!(preset_after, 1, "Preset encoder step +1 must increment preset from 0 to 1");

    // Test reverse stepping: delta=-1 decrements back to preset 0
    emu.step_preset_encoder(-1);
    for _ in 0..800 {
        emu.step_audio_block_quantum();
    }
    let mut preset_bytes_back = [0u8; 2];
    emu.uc.mem_read(0x2000_0BDC, &mut preset_bytes_back).unwrap();
    let preset_back = u16::from_le_bytes(preset_bytes_back);
    eprintln!("[7c] Preset after delta=-1: {}", preset_back);
    assert_eq!(preset_back, 0, "Preset encoder step -1 must decrement preset from 1 to 0");

    let nz_after_enc = emu.coproc.get_framebuffer().iter().filter(|&&b| b != 0).count();
    eprintln!(
        "[8] After Preset Encoder: PC=0x{:08X}, OLED nz={}",
        emu.pc, nz_after_enc
    );

    eprintln!("[9] Testing MIDI Note On (Note 60, Vel 100)...");
    emu.enqueue_midi_bytes(&[0x90, 60, 100]);
    let mut midi_non_zero_blocks = 0;
    for _ in 0..150 {
        let block = emu.step_audio_block_quantum();
        if block.iter().any(|&b| b != 0) {
            midi_non_zero_blocks += 1;
        }
    }
    eprintln!("After 150 blocks with MIDI Note On: non_zero_audio_blocks={}", midi_non_zero_blocks);
    assert!(midi_non_zero_blocks > 0, "MIDI Note On must synthesize audio blocks");
    emu.enqueue_midi_bytes(&[0x80, 60, 0]); // Note Off
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }

    eprintln!("[10a] Testing Multi-Button Modifier Combination: Shift + Paraphonic (Chord Mode)...");
    let fb_before_shift_para = emu.coproc.get_framebuffer();
    emu.set_microswitch("shift", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    let mut shift_pre_para = [0u8; 4];
    emu.uc.mem_read(0x20003714 + 0x14, &mut shift_pre_para).unwrap();
    assert_ne!(u32::from_le_bytes(shift_pre_para) & 2, 0, "Shift modifier must be active in firmware before pressing Paraphonic");

    let mut chord_oled_changed = false;
    emu.set_microswitch("paraphonic", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_framebuffer() != fb_before_shift_para {
            chord_oled_changed = true;
        }
    }
    emu.set_microswitch("paraphonic", false);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_framebuffer() != fb_before_shift_para {
            chord_oled_changed = true;
        }
    }
    emu.set_microswitch("shift", false);
    for _ in 0..150 {
        emu.step_audio_block_quantum();
    }
    eprintln!("Shift + Paraphonic: OLED changed={}", chord_oled_changed);
    assert!(chord_oled_changed, "Shift + Paraphonic must trigger Chord Mode OLED popup");

    eprintln!("[10b] Testing Concurrent Contact Closures: Octave Down + Octave Up overlap...");
    emu.set_microswitch("oct_down", true);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("oct_up", true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("oct_down", false);
    for _ in 0..50 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("oct_up", false);
    for _ in 0..150 {
        emu.step_audio_block_quantum();
    }
    eprintln!("Octave Down + Up concurrent overlap processed cleanly");

    eprintln!("[10c] Testing Rapid Successive Bursts: Cycling Env 3x toggles...");
    for cycle in 0..3 {
        emu.set_microswitch("cyc_mode", true);
        for _ in 0..60 {
            emu.step_audio_block_quantum();
        }
        emu.set_microswitch("cyc_mode", false);
        for _ in 0..60 {
            emu.step_audio_block_quantum();
        }
        let pleds = emu.get_panel_leds();
        eprintln!("Rapid burst cycle {}: Env={}, Run={}, Loop={}", cycle + 1, pleds[11], pleds[12], pleds[13]);
    }
}

#[test]
fn test_raw_pin_and_encoder_dispatch() {
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
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
    }

    // 1. Actuate Amp Mod switch via physical pin (Port C, Pin 11, Group 0x06)
    let amp_initial = emu.coproc.get_panel_leds()[32];
    assert_eq!(amp_initial, 255, "Amp Mod LED must be ON initially");
    emu.set_raw_pin('C', 11, 0x06, true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_raw_pin('C', 11, 0x06, false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let amp_after = emu.coproc.get_panel_leds()[32];
    assert_eq!(amp_after, 0, "Amp Mod LED must turn OFF after raw pin toggle on PC11");

    // 1b. Actuate Paraphonic switch via physical pin (Port C, Pin 9, Group 0x07)
    let para_initial = emu.coproc.get_panel_leds()[28];
    assert_eq!(para_initial, 0, "Paraphonic LED must be OFF initially");
    emu.set_raw_pin('C', 9, 0x07, true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_raw_pin('C', 9, 0x07, false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    let para_after = emu.coproc.get_panel_leds()[28];
    assert_eq!(para_after, 255, "Paraphonic LED must turn ON after raw pin toggle on PC9");

    // 2. Actuate Utility switch via physical pin (Port C, Pin 7, Group 0x08)
    assert_eq!(emu.coproc.get_panel_leds()[30], 0, "Utility LED must be OFF initially");
    emu.set_raw_pin('C', 7, 0x08, true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_raw_pin('C', 7, 0x08, false);
    for _ in 0..200 {
        emu.step_audio_block_quantum();
    }
    assert_eq!(emu.coproc.get_panel_leds()[30], 255, "Utility LED must be ON after raw pin toggle on PC7");

    // 3. Actuate Oct | Mod switch via physical pin (Port C, Pin 4, Group 0x04)
    let fb_before_mod = emu.coproc.get_framebuffer();
    emu.set_raw_pin('C', 4, 0x04, true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_raw_pin('C', 4, 0x04, false);
    let mut fb_changed = false;
    for s in 0..600 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_framebuffer() != fb_before_mod {
            eprintln!("Oct|Mod changed OLED at step {}", s);
            fb_changed = true;
            break;
        }
    }
    assert!(fb_changed, "OLED framebuffer must update after raw Oct|Mod toggle on PC4");

    // 3b. Actuate Rate push switch via physical pin (Port C, Pin 5, Group 0x04)
    let fb_before_rate = emu.coproc.get_framebuffer();
    emu.set_raw_pin('C', 5, 0x04, true);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_raw_pin('C', 5, 0x04, false);
    let mut fb_rate_changed = false;
    for s in 0..600 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_framebuffer() != fb_before_rate {
            eprintln!("Rate push changed OLED at step {}", s);
            fb_rate_changed = true;
            break;
        }
    }
    assert!(fb_rate_changed, "OLED framebuffer must update after raw Rate push toggle on PC5");

    // 4. Actuate Cutoff potentiometer via Group 0x03, Pin 0 (0x0300)
    // ActivePresetAndSequencerState (0x2001C898): Group 3 parameter array pointer is at 0x2001C898 + (3 + 11) * 4
    let mut val_ptr_bytes = [0u8; 4];
    let _ = emu.uc.mem_read(0x2001C898 + (3 + 0x0B) * 4, &mut val_ptr_bytes);
    let val_ptr = u32::from_le_bytes(val_ptr_bytes);
    assert_ne!(val_ptr, 0, "Group 3 parameter array pointer must be valid");

    // Baseline pot was primed on boot to 2048. Now actuate user knob movement to 100 (near-minimum 12-bit ADC value)
    emu.set_potentiometer(0x0300, 100);
    for _ in 0..150 {
        emu.step_audio_block_quantum();
    }

    let mut final_group3 = [0u8; 2];
    let _ = emu.uc.mem_read(val_ptr as u64, &mut final_group3);
    let cutoff_val = i16::from_le_bytes(final_group3);
    assert_eq!(cutoff_val, 0, "Cutoff parameter in ActivePreset must update to 0");
}

#[test]
fn test_potentiometer_glide_and_cycling_envelope_actuations() {
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
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
    }

    let updated_cids = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let updated_cids_clone = updated_cids.clone();

    // Hook FUN_08023c9c (0x08023C9C) which is called when a parameter is committed after hook/catch
    emu.uc.add_code_hook(0x08023C9C, 0x08023C9C, move |uc, _pc, _size| {
        let r1 = uc.reg_read(unicorn_engine::RegisterARM::R1).unwrap_or(0);
        let r2 = uc.reg_read(unicorn_engine::RegisterARM::R2).unwrap_or(0);
        updated_cids_clone.lock().unwrap().push((r1 as u16, r2 as u16));
    }).unwrap();

    // 1. Verify Cutoff Filter (Group 1, ID 1: 0x0101)
    emu.set_potentiometer(0x0101, 0);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_potentiometer(0x0101, 3500);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        updated_cids.lock().unwrap().iter().any(|&(cid, _)| cid == 0x0101),
        "Cutoff filter potentiometer (0x0101) must trigger parameter update: updated_cids={:?}",
        *updated_cids.lock().unwrap()
    );

    // 1b. Verify Resonance Filter (Group 1, ID 2: 0x0102)
    emu.set_potentiometer(0x0102, 0);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_potentiometer(0x0102, 3500);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        updated_cids.lock().unwrap().iter().any(|&(cid, _)| cid == 0x0102),
        "Resonance filter potentiometer (0x0102) must trigger parameter update"
    );

    // 2. Verify Glide (Group 3, ID 0: 0x0300)
    // Glide potentiometer catches and triggers parameter commit 0x0300
    emu.set_potentiometer(0x0300, 3500);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        updated_cids.lock().unwrap().iter().any(|&(cid, _)| cid == 0x0300),
        "Glide potentiometer (0x0300) must trigger parameter update"
    );

    // 3. Verify Cycling Rise Time (Group 2, ID 1: 0x0201)
    // Hook requires crossing the preset value (~333 in 12-bit scale) to catch
    emu.set_potentiometer(0x0201, 0);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_potentiometer(0x0201, 3500);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        updated_cids.lock().unwrap().iter().any(|&(cid, _)| cid == 0x0201),
        "Cycling Rise (0x0201) must trigger parameter update"
    );

    // 3. Verify Cycling Fall Time (Group 2, ID 3: 0x0203)
    // Hook requires crossing the preset value (~333 in 12-bit scale) to catch
    emu.set_potentiometer(0x0203, 0);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_potentiometer(0x0203, 3500);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        updated_cids.lock().unwrap().iter().any(|&(cid, _)| cid == 0x0203),
        "Cycling Fall (0x0203) must trigger parameter update"
    );

    // 4. Verify Cycling Hold/Sust (Group 2, ID 4: 0x0204)
    // Hook requires crossing the preset value (0 in 12-bit scale) to catch
    emu.set_potentiometer(0x0204, 0);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_potentiometer(0x0204, 3500);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        updated_cids.lock().unwrap().iter().any(|&(cid, _)| cid == 0x0204),
        "Cycling Hold (0x0204) must trigger parameter update"
    );

    // 5. Verify Cycling Amount (Group 2, ID 6: 0x0206)
    // Hook requires crossing the initial preset value (4095) before latching
    emu.set_potentiometer(0x0206, 4095);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    emu.set_potentiometer(0x0206, 2000);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        updated_cids.lock().unwrap().iter().any(|&(cid, _)| cid == 0x0206),
        "Cycling Amount (0x0206) must trigger parameter update"
    );
}

#[test]
fn test_digital_oscillator_encoders() {
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
    for _ in 0..1500 {
        emu.step_audio_block_quantum();
    }

    let oscillator_encoder_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let osc_events_clone = oscillator_encoder_events.clone();

    // Hook fn_Local_Off_08042BC8 (0x08042BC8) which handles Digital Oscillator encoders:
    // r0 = engine object, r1 = param id (1=wave, 3=timbre, 5=shape), r2 = delta
    emu.uc.add_code_hook(0x08042BC8, 0x08042BC8, move |uc, _pc, _size| {
        let r1 = uc.reg_read(unicorn_engine::RegisterARM::R1).unwrap_or(0);
        let r2 = uc.reg_read(unicorn_engine::RegisterARM::R2).unwrap_or(0);
        osc_events_clone.lock().unwrap().push((r1 as u16, r2 as i16));
    }).unwrap();

    // 1. Step Wave encoder (+1)
    emu.step_wave_encoder(1);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        oscillator_encoder_events.lock().unwrap().iter().any(|&(id, delta)| id == 1 && delta == 1),
        "Wave encoder step must reach engine with param ID 1 and delta 1"
    );

    // 2. Step Timbre encoder (+2)
    emu.step_timbre_encoder(2);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        oscillator_encoder_events.lock().unwrap().iter().any(|&(id, delta)| id == 3 && delta == 2),
        "Timbre encoder step must reach engine with param ID 3 and delta 2"
    );

    // 3. Step Shape encoder (-1)
    emu.step_shape_encoder(-1);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    assert!(
        oscillator_encoder_events.lock().unwrap().iter().any(|&(id, delta)| id == 5 && delta == -1),
        "Shape encoder step must reach engine with param ID 5 and delta -1"
    );

    // 4. Step Type encoder (+1)
    let initial_fb = emu.coproc.get_framebuffer();
    emu.step_type_encoder(1);
    let mut fb_changed = false;
    for _ in 0..500 {
        emu.step_audio_block_quantum();
        if emu.coproc.get_framebuffer() != initial_fb {
            fb_changed = true;
            break;
        }
    }
    assert!(
        fb_changed,
        "Stepping Type encoder must switch engine and update OLED display"
    );
}


