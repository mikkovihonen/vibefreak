use vibefreak_core::{
    AudioInputSource, HeadsetJackState, VibeFreakEmulator,
};
use std::path::Path;

#[test]
fn test_tlv320aic3104_unit_and_emulator_integration() {
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

    let mut emu = VibeFreakEmulator::new(
        &main_bin,
        Some(&qspi_bin),
        &oled_bin,
        &matrix_bin,
        &kbd_bin,
    )
    .expect("Failed to initialize emulator");

    eprintln!("[1] Verifying initial codec state...");
    let initial_rear = emu.get_rear_panel();
    assert_eq!(initial_rear.headset_jack, HeadsetJackState::Disconnected);
    assert!(!initial_rear.headset_button);
    assert_eq!(initial_rear.audio_input_source, AudioInputSource::MicIn);

    eprintln!("[2] Testing headset jack state transitions...");
    emu.set_headset_jack(HeadsetJackState::HeadsetWithMic);
    assert_eq!(emu.get_headset_jack(), HeadsetJackState::HeadsetWithMic);
    let rear_trrs = emu.get_rear_panel();
    assert_eq!(rear_trrs.headset_jack, HeadsetJackState::HeadsetWithMic);

    emu.set_headset_jack(HeadsetJackState::Headphones);
    assert_eq!(emu.get_headset_jack(), HeadsetJackState::Headphones);
    let rear_trs = emu.get_rear_panel();
    assert_eq!(rear_trs.headset_jack, HeadsetJackState::Headphones);

    emu.set_headset_jack(HeadsetJackState::Disconnected);
    assert_eq!(emu.get_headset_jack(), HeadsetJackState::Disconnected);
    let rear_unplugged = emu.get_rear_panel();
    assert_eq!(rear_unplugged.headset_jack, HeadsetJackState::Disconnected);

    eprintln!("[3] Testing headset inline hook-switch / mic button...");
    emu.set_headset_button(true);
    assert!(emu.get_headset_button());
    assert!(emu.get_rear_panel().headset_button);

    emu.set_headset_button(false);
    assert!(!emu.get_headset_button());
    assert!(!emu.get_rear_panel().headset_button);

    eprintln!("[4] Testing audio input source selection...");
    emu.set_audio_input_source(AudioInputSource::LineIn);
    assert_eq!(emu.get_audio_input_source(), AudioInputSource::LineIn);
    assert_eq!(emu.get_rear_panel().audio_input_source, AudioInputSource::LineIn);

    emu.set_audio_input_source(AudioInputSource::MicIn);
    assert_eq!(emu.get_audio_input_source(), AudioInputSource::MicIn);
    assert_eq!(emu.get_rear_panel().audio_input_source, AudioInputSource::MicIn);

    eprintln!("[5] Testing audio input sample feeding and ADC processing...");
    // Feed 4 stereo test samples
    let test_samples = vec![0.5f32, -0.5f32, 0.25f32, -0.25f32];
    emu.feed_audio_input(&test_samples);

    {
        let codec = emu.get_codec_mut();
        // Since headset is disconnected and muted by default until jack inserted or unmuted,
        // let's connect TRRS headset and unmute PGA
        codec.jack_state = HeadsetJackState::HeadsetWithMic;
        codec.page0[15] = 0x00; // Left PGA 0.0 dB, unmuted
        codec.page0[16] = 0x00; // Right PGA 0.0 dB, unmuted

        let (left0, right0) = codec.read_adc_stereo_sample();
        assert!((left0 - 16383).abs() <= 2);
        assert!((right0 - 16383).abs() <= 2);

        let (left1, right1) = codec.read_adc_stereo_sample();
        assert!((left1 - (-16383)).abs() <= 2);
        assert!((right1 - (-16383)).abs() <= 2);
    }

    emu.clear_audio_input();
    {
        let codec = emu.get_codec_mut();
        let (left_empty, right_empty) = codec.read_adc_stereo_sample();
        assert_eq!(left_empty, 0);
        assert_eq!(right_empty, 0);
    }

    eprintln!("[6] Booting emulator with stock firmware to verify I2C codec communication...");
    // Connect TRRS headset before booting
    emu.set_headset_jack(HeadsetJackState::HeadsetWithMic);
    emu.boot(120_000_000);

    // Verify codec register access occurred during firmware boot
    let codec = emu.get_codec();
    eprintln!(
        "Post-boot Codec Reg 103 (scratchpad) = 0x{:02X}, Active Page = {}",
        codec.page0[103], codec.active_page
    );
    for (i, &val) in codec.page0.iter().enumerate() {
        if val != 0 {
            eprintln!("  page0[{}] = 0x{:02X}", i, val);
        }
    }
    // Firmware writes 0xC0 to Reg 8, 0x09 to Reg 9, 0xC0 to Reg 109
    assert_eq!(codec.page0[8], 0xC0);
    assert_eq!(codec.page0[9], 0x09);
    assert_eq!(codec.page0[109], 0xC0);

    // Verify Reg 13 headset detection and button readback
    emu.get_codec_mut().page0[13] |= 0x80; // Enable headset detection bit 7
    emu.set_headset_jack(HeadsetJackState::HeadsetWithMic);
    assert_eq!((emu.get_codec().read_reg(13) >> 5) & 0x03, 0b11);

    emu.set_headset_jack(HeadsetJackState::Headphones);
    assert_eq!((emu.get_codec().read_reg(13) >> 5) & 0x03, 0b01);

    emu.set_headset_jack(HeadsetJackState::Disconnected);
    assert_eq!((emu.get_codec().read_reg(13) >> 5) & 0x03, 0b00);

    // Button readback
    emu.set_headset_button(true);
    assert_eq!(emu.get_codec().read_reg(13) & 0x10, 0x10);
    emu.set_headset_button(false);
    assert_eq!(emu.get_codec().read_reg(13) & 0x10, 0x00);

    // Verify DMA2 Stream 4 (SAI2_A TX) and Stream 6 (SAI2_B RX) configuration
    let s4 = &emu.get_dma2().streams[4];
    assert_ne!(s4.cr & 1, 0, "DMA2 Stream 4 (SAI2_A TX) must be enabled");
    assert_eq!(s4.par, 0x40015C20, "DMA2 Stream 4 PAR must be SAI2_A DR");

    let s6 = &emu.get_dma2().streams[6];
    assert_ne!(s6.cr & 1, 0, "DMA2 Stream 6 (SAI2_B RX) must be enabled");
    assert_eq!(s6.par, 0x40015C40, "DMA2 Stream 6 PAR must be SAI2_B DR");


    // Verify audio rendering block execution with DMA2 Stream 6 ingestion
    eprintln!("[7] Rendering audio block with audio input stream...");
    let audio_input_block = vec![0.3f32; 256];
    emu.feed_audio_input(&audio_input_block);

    let raw_audio_bytes = emu.render_raw_audio_block();
    assert_eq!(raw_audio_bytes.len(), 128);

    eprintln!("TLV320AIC3104 integration test passed successfully!");
}
