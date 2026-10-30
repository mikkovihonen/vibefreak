use std::path::Path;
use vibefreak_core::VibeFreakEmulator;

#[test]
fn test_analog_filter_resonance_sweep_and_pressure() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mff_path = std::env::var("VIBEFREAK_MFF_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("original_firmware/microfreak/MicroFreak_Firmware_Update_5_0_0_2084.mff"));
    if !mff_path.exists() {
        eprintln!("Skipping test: MFF firmware package not found (set VIBEFREAK_MFF_PATH to run)");
        return;
    }

    let pkg = vibefreak_core::MffFirmwarePackage::from_file(&mff_path).unwrap();
    let mut emu = VibeFreakEmulator::from_package(&pkg).unwrap();

    eprintln!("[1] Booting emulator...");
    emu.boot(100_000_000);

    for _ in 0..1500 {
        emu.step_audio_block_quantum();
    }

    // 1. Verify idle rear panel pressure voltage is 0.0V (not 9.939V!)
    let rear_idle = emu.get_rear_panel();
    eprintln!("Idle Rear Panel: Pressure CV = {:.3} V (raw = {})", rear_idle.pressure_volts, rear_idle.pressure_raw);
    assert_eq!(rear_idle.pressure_raw, 0, "Idle pressure raw should be 0 when no keys are pressed");
    assert_eq!(rear_idle.pressure_volts, 0.0, "Idle pressure voltage should be 0.0V");

    // 2. Verify DAC Channel 1 (dor1) has been updated by the firmware for active preset resonance
    let dor1_initial = emu.uc.get_data().mmio.dac.dor1 & 0x0FFF;
    eprintln!("Preset 1 initial DAC Channel 1 value = 0x{:03X} ({})", dor1_initial, dor1_initial);
    assert!(dor1_initial > 3500, "Preset 1 must initialize with low resonance (high DAC1 voltage)");

    // 3. Sweep resonance knob progressively to catch and reach high resonance
    for v in [0, 200, 500, 1000, 2000, 3500] {
        emu.set_potentiometer(0x0102, v);
        for _ in 0..60 {
            emu.step_audio_block_quantum();
        }
    }
    let dor1_high = emu.uc.get_data().mmio.dac.dor1 & 0x0FFF;
    let resonance_high = 4095u16.saturating_sub(dor1_high as u16);
    eprintln!("Resonance pot = 3500: dor1 = 0x{:03X} ({}), effective resonance = {}", dor1_high, dor1_high, resonance_high);
    assert!(dor1_high < 1000, "DAC1 must decrease to under 1000 at high resonance");
    assert!(resonance_high > 3000, "Effective resonance must be > 3000 when knob is set to 3500");

    // 4. Now sweep down to 0 (minimum resonance)
    for v in [2000, 1000, 500, 100, 0] {
        emu.set_potentiometer(0x0102, v);
        for _ in 0..60 {
            emu.step_audio_block_quantum();
        }
    }
    let dor1_min = emu.uc.get_data().mmio.dac.dor1 & 0x0FFF;
    let resonance_min = 4095u16.saturating_sub(dor1_min as u16);
    eprintln!("Resonance pot returned to 0: dor1 = 0x{:03X} ({}), effective resonance = {}", dor1_min, dor1_min, resonance_min);
    assert_eq!(dor1_min, 4095, "DAC1 must output 4095 (0x0FFF) at minimum resonance knob");
    assert_eq!(resonance_min, 0, "Effective resonance must be 0 at minimum knob position");

    // 5. Test Key Pressure (Aftertouch): pressing key 12 writes to DAC Channel 2 (dor2)
    emu.set_key(12, 100);
    for _ in 0..100 {
        emu.step_audio_block_quantum();
    }
    let rear_pressed = emu.get_rear_panel();
    eprintln!("Key 12 pressed: Pressure CV = {:.3} V (raw = {})", rear_pressed.pressure_volts, rear_pressed.pressure_raw);
    assert!(rear_pressed.pressure_raw > 0, "Pressure raw must be non-zero when key is pressed with aftertouch");
    assert!(rear_pressed.pressure_volts > 0.5, "Pressure voltage must increase above 0.5V on key press");
}
