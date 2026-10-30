use std::path::Path;
use vibefreak_core::VibeFreakEmulator;

#[test]
fn test_arpeggiator_time_division_changes() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mff_path = repo_root.join("tests/fixtures/MicroFreak_Firmware_Update_5_0_0_2084.mff");
    if !mff_path.exists() {
        eprintln!("MFF firmware package not found at {:?}", mff_path);
        return;
    }

    let pkg = vibefreak_core::MffFirmwarePackage::from_file(&mff_path).unwrap();
    let mut emu = VibeFreakEmulator::from_package(&pkg).unwrap();

    eprintln!("[1] Booting emulator...");
    emu.boot(100_000_000);

    for _ in 0..1500 {
        emu.step_audio_block_quantum();
    }

    eprintln!("[2] Turning Arpeggiator ON...");
    emu.set_microswitch("arp_seq", true);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("arp_seq", false);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }

    eprintln!("[3] Pressing Key 12 and latching Hold...");
    emu.coproc.set_key(12, 200);
    emu.set_microswitch("hold", true);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("hold", false);

    eprintln!("[4] Stepping audio blocks to verify baseline Arpeggiator output...");
    let mut max_amp_baseline = 0i32;
    for _ in 0..100 {
        let blk = emu.step_audio_block_quantum();
        for chunk in blk.as_chunks::<4>().0 {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
            if left.abs() > max_amp_baseline {
                max_amp_baseline = left.abs();
            }
        }
    }
    eprintln!("Baseline Arp max amplitude = {}", max_amp_baseline);
    assert!(max_amp_baseline > 1000, "Arpeggiator should produce audio when active and note held");

    // Click Rate encoder to toggle Sync ON
    eprintln!("[5] Clicking Rate encoder (toggle Sync ON)...");
    emu.set_microswitch("rate", true);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }
    emu.set_microswitch("rate", false);
    for _ in 0..20 {
        emu.step_audio_block_quantum();
    }

    // Change time division up (+1 step)
    eprintln!("[6] Stepping Rate encoder by +1 to change Time Division...");
    emu.step_rate_encoder(1);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }

    // Verify notes continue playing after first division change
    let mut notes_phase1 = 0;
    let mut max_amp_phase1 = 0i32;
    let mut was_loud = false;
    for _ in 0..1200 {
        let blk = emu.step_audio_block_quantum();
        let mut blk_peak = 0i32;
        for chunk in blk.as_chunks::<4>().0 {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
            if left.abs() > blk_peak.abs() {
                blk_peak = left;
            }
        }
        if blk_peak.abs() > max_amp_phase1 {
            max_amp_phase1 = blk_peak.abs();
        }
        let is_loud = blk_peak.abs() > 2000;
        if is_loud && !was_loud {
            notes_phase1 += 1;
        }
        was_loud = is_loud;
    }
    eprintln!("Phase 1 (+1 division): notes_triggered={}, max_amp={}", notes_phase1, max_amp_phase1);
    assert!(notes_phase1 >= 2, "Expected at least 2 notes after first division change, got {}", notes_phase1);
    assert!(max_amp_phase1 > 1000, "Audio amplitude should remain high after first division change");

    // Change time division up again (+1 step)
    eprintln!("[7] Stepping Rate encoder by +1 again...");
    emu.step_rate_encoder(1);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }

    let mut notes_phase2 = 0;
    let mut max_amp_phase2 = 0i32;
    let mut was_loud = false;
    for _ in 0..1200 {
        let blk = emu.step_audio_block_quantum();
        let mut blk_peak = 0i32;
        for chunk in blk.as_chunks::<4>().0 {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
            if left.abs() > blk_peak.abs() {
                blk_peak = left;
            }
        }
        if blk_peak.abs() > max_amp_phase2 {
            max_amp_phase2 = blk_peak.abs();
        }
        let is_loud = blk_peak.abs() > 2000;
        if is_loud && !was_loud {
            notes_phase2 += 1;
        }
        was_loud = is_loud;
    }
    eprintln!("Phase 2 (+2 division): notes_triggered={}, max_amp={}", notes_phase2, max_amp_phase2);
    assert!(notes_phase2 >= 2, "Expected at least 2 notes after second division change, got {}", notes_phase2);
    assert!(max_amp_phase2 > 1000, "Audio amplitude should remain high after second division change");

    // Change time division back down (-1 step)
    eprintln!("[8] Stepping Rate encoder by -1 to step division back down...");
    emu.step_rate_encoder(-1);
    for _ in 0..30 {
        emu.step_audio_block_quantum();
    }

    let mut notes_phase3 = 0;
    let mut max_amp_phase3 = 0i32;
    let mut was_loud = false;
    for _ in 0..1200 {
        let blk = emu.step_audio_block_quantum();
        let mut blk_peak = 0i32;
        for chunk in blk.as_chunks::<4>().0 {
            let left = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
            if left.abs() > blk_peak.abs() {
                blk_peak = left;
            }
        }
        if blk_peak.abs() > max_amp_phase3 {
            max_amp_phase3 = blk_peak.abs();
        }
        let is_loud = blk_peak.abs() > 2000;
        if is_loud && !was_loud {
            notes_phase3 += 1;
        }
        was_loud = is_loud;
    }
    eprintln!("Phase 3 (-1 division): notes_triggered={}, max_amp={}", notes_phase3, max_amp_phase3);
    assert!(notes_phase3 >= 2, "Expected at least 2 notes after division step down, got {}", notes_phase3);
    assert!(max_amp_phase3 > 1000, "Audio amplitude should remain high after division step down");

    eprintln!("Arpeggiator time division transitions tested successfully without freezing!");
}

