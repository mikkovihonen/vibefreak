//! Texas Instruments TLV320AIC3104 Low-Power Stereo Audio Codec emulation.
//!
//! Controls real-time audio input/output, ADC/DAC routing, PGA gains, and
//! TRRS headset / microphone jack insertion and hook-switch button detection per
//! the official Texas Instruments TLV320AIC3104 hardware specification.

use std::collections::VecDeque;
use serde::{Deserialize, Serialize};

/// 7-bit I2C Slave Address (AD0 connected to GND).
pub const TLV320AIC3104_I2C_ADDR_7BIT: u8 = 0x18;
/// 8-bit I2C Slave Write Address (0x18 << 1 | 0).
pub const TLV320AIC3104_I2C_ADDR_WRITE: u8 = 0x30;
/// 8-bit I2C Slave Read Address (0x18 << 1 | 1).
pub const TLV320AIC3104_I2C_ADDR_READ: u8 = 0x31;

/// Physical rear-panel headset jack insertion state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadsetJackState {
    /// No 3.5mm plug inserted into the headset jack.
    Disconnected,
    /// Standard 3.5mm TRS stereo headphones (no microphone).
    Headphones,
    /// 3.5mm TRRS cellular/gooseneck headset with electret microphone.
    HeadsetWithMic,
}

impl Default for HeadsetJackState {
    fn default() -> Self {
        Self::Disconnected
    }
}

/// Physical audio input source selectable for digitization through the ADC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioInputSource {
    /// TRRS Headset microphone input (MIC1LP / MIC1LM).
    MicIn,
    /// Rear/Aux Line input (LINE1L / LINE1R).
    LineIn,
}

impl Default for AudioInputSource {
    fn default() -> Self {
        Self::MicIn
    }
}

/// Texas Instruments TLV320AIC3104 Stereo Audio Codec hardware model.
#[derive(Debug, Clone)]
pub struct Tlv320aic3104 {
    /// Page 0 register bank (128 registers).
    pub page0: [u8; 128],
    /// Page 1 register bank (128 registers).
    pub page1: [u8; 128],
    /// Current active page index (0 or 1, switched via Register 0).
    pub active_page: u8,
    /// Physical 3.5mm headset jack connection status.
    pub jack_state: HeadsetJackState,
    /// Physical headset hook-switch / mic inline button state.
    pub button_pressed: bool,
    /// Selected physical audio input source.
    pub input_source: AudioInputSource,
    /// Sample FIFO holding incoming analog audio PCM samples (-1.0 to 1.0) for the ADC.
    pub input_fifo: VecDeque<f32>,
    /// Precomputed Left ADC linear gain multiplier.
    left_linear_gain: f32,
    /// Precomputed Right ADC linear gain multiplier.
    right_linear_gain: f32,
}

impl Default for Tlv320aic3104 {
    fn default() -> Self {
        let mut codec = Self {
            page0: [0u8; 128],
            page1: [0u8; 128],
            active_page: 0,
            jack_state: HeadsetJackState::Disconnected,
            button_pressed: false,
            input_source: AudioInputSource::MicIn,
            input_fifo: VecDeque::with_capacity(1024),
            left_linear_gain: 1.0,
            right_linear_gain: 1.0,
        };
        codec.reset();
        codec
    }
}

impl Tlv320aic3104 {
    /// Creates a new TLV320AIC3104 codec instance with authentic silicon power-on defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Performs a full hardware/power-on reset, restoring all registers to silicon defaults.
    pub fn reset(&mut self) {
        self.active_page = 0;
        self.page0 = [0u8; 128];
        self.page1 = [0u8; 128];
        self.left_linear_gain = 1.0;
        self.right_linear_gain = 1.0;

        // Silicon defaults per TLV320AIC3104 specification:
        // Page 0, Reg 15 & 16: Left & Right ADC PGA Gain default to 0x80 (Muted, 0 dB)
        self.page0[15] = 0x80;
        self.page0[16] = 0x80;

        // Page 0, Reg 43 & 44: Left & Right DAC Digital Volume default to 0x00 (0 dB unmuted)
        self.page0[43] = 0x00;
        self.page0[44] = 0x00;

        // Page 0, Reg 51: HPOUT output level control (0x00 default)
        self.page0[51] = 0x00;

        self.input_fifo.clear();
    }

    /// Reads a register from the active page.
    pub fn read_reg(&self, reg: u8) -> u8 {
        let reg_idx = (reg & 0x7F) as usize;

        // Register 0 on any page returns the active page number
        if reg_idx == 0 {
            return self.active_page;
        }

        if self.active_page == 0 {
            match reg_idx {
                // Page 0, Register 13: Headset / Button Press Detection Register A
                13 => {
                    let mut val = self.page0[13] & 0x8F; // Preserve enable (bit 7) and debounce (bits 3:0)
                    let enabled = (self.page0[13] & 0x80) != 0;
                    if enabled {
                        let status_bits = match self.jack_state {
                            HeadsetJackState::Disconnected => 0b00,
                            HeadsetJackState::Headphones => 0b01,
                            HeadsetJackState::HeadsetWithMic => 0b11,
                        };
                        val |= status_bits << 5;
                    }
                    if self.button_pressed {
                        val |= 1 << 4; // Bit 4: Button pressed
                    }
                    val
                }
                // Page 0, Register 36: ADC Flag Register
                36 => {
                    let mut flags = self.page0[36];
                    // If ADCs are unmuted and jack is present, report powered up (bits 7:6)
                    let left_pwr = (self.page0[15] & 0x80) == 0 && self.jack_state != HeadsetJackState::Disconnected;
                    let right_pwr = (self.page0[16] & 0x80) == 0 && self.jack_state != HeadsetJackState::Disconnected;
                    if left_pwr { flags |= 1 << 7; }
                    if right_pwr { flags |= 1 << 6; }
                    flags
                }
                // Page 0, Register 65: Left DAC Status Flag Register
                65 => {
                    let mut flags = self.page0[65];
                    // Bit 7: Left DAC powered up
                    if (self.page0[37] & 0x80) != 0 { flags |= 1 << 7; }
                    flags
                }
                // Page 0, Register 66: Right DAC Status Flag Register
                66 => {
                    let mut flags = self.page0[66];
                    // Bit 7: Right DAC powered up
                    if (self.page0[37] & 0x40) != 0 { flags |= 1 << 7; }
                    flags
                }
                _ => self.page0[reg_idx],
            }
        } else {
            // Page 1: Audio Effects Filter Coefficients (Biquads)
            self.page1[reg_idx]
        }
    }

    /// Writes a register on the active page.
    pub fn write_reg(&mut self, reg: u8, val: u8) {
        let reg_idx = (reg & 0x7F) as usize;

        // Register 0 on any page controls page selection
        if reg_idx == 0 {
            self.active_page = if val == 1 { 1 } else { 0 };
            return;
        }

        if self.active_page == 0 {
            match reg_idx {
                // Page 0, Register 1: Software Reset Register
                1 => {
                    if (val & 0x80) != 0 {
                        self.reset();
                    }
                }
                // Page 0, Register 15: Left ADC PGA Gain
                15 => {
                    self.page0[15] = val;
                    let raw = val & 0x7F;
                    self.left_linear_gain = 10.0f32.powf(((raw as f32) * 0.5) / 20.0);
                }
                // Page 0, Register 16: Right ADC PGA Gain
                16 => {
                    self.page0[16] = val;
                    let raw = val & 0x7F;
                    self.right_linear_gain = 10.0f32.powf(((raw as f32) * 0.5) / 20.0);
                }
                _ => {
                    self.page0[reg_idx] = val;
                }
            }
        } else {
            // Page 1: Digital Filter Coefficients
            self.page1[reg_idx] = val;
        }
    }

    // --- Physical Hardware Controls & Status ---

    /// Sets the physical connection status of the rear-panel 3.5mm headset jack.
    pub fn set_headset_jack(&mut self, state: HeadsetJackState) {
        self.jack_state = state;
    }

    /// Gets the current physical connection status of the rear-panel headset jack.
    pub fn get_headset_jack(&self) -> HeadsetJackState {
        self.jack_state
    }

    /// Sets the physical headset inline button / hook-switch contact state.
    pub fn set_headset_button(&mut self, pressed: bool) {
        self.button_pressed = pressed;
    }

    /// Gets the physical headset inline button contact state.
    pub fn get_headset_button(&self) -> bool {
        self.button_pressed
    }

    /// Configures the selected physical audio input channel.
    pub fn set_audio_input_source(&mut self, source: AudioInputSource) {
        self.input_source = source;
    }

    /// Gets the selected physical audio input channel.
    pub fn get_audio_input_source(&self) -> AudioInputSource {
        self.input_source
    }

    /// Returns the active Left ADC PGA Gain in decibels (0.0 dB to +59.5 dB).
    pub fn get_left_adc_gain_db(&self) -> f32 {
        let raw = self.page0[15] & 0x7F;
        (raw as f32) * 0.5
    }

    /// Returns the active Right ADC PGA Gain in decibels (0.0 dB to +59.5 dB).
    pub fn get_right_adc_gain_db(&self) -> f32 {
        let raw = self.page0[16] & 0x7F;
        (raw as f32) * 0.5
    }

    /// Returns whether the Left ADC is muted.
    pub fn is_left_adc_muted(&self) -> bool {
        (self.page0[15] & 0x80) != 0
    }

    /// Returns whether the Right ADC is muted.
    pub fn is_right_adc_muted(&self) -> bool {
        (self.page0[16] & 0x80) != 0
    }

    /// Returns the Left DAC digital volume attenuation in decibels (0.0 dB down to -63.5 dB).
    pub fn get_left_dac_volume_db(&self) -> f32 {
        let raw = self.page0[43] & 0x7F;
        -(raw as f32) * 0.5
    }

    /// Returns the Right DAC digital volume attenuation in decibels (0.0 dB down to -63.5 dB).
    pub fn get_right_dac_volume_db(&self) -> f32 {
        let raw = self.page0[44] & 0x7F;
        -(raw as f32) * 0.5
    }

    /// Returns whether the Left DAC output is muted.
    pub fn is_left_dac_muted(&self) -> bool {
        (self.page0[43] & 0x80) != 0
    }

    /// Returns whether the Right DAC output is muted.
    pub fn is_right_dac_muted(&self) -> bool {
        (self.page0[44] & 0x80) != 0
    }

    // --- Audio Streaming & Digitization ---

    /// Feeds raw audio PCM samples (-1.0 to +1.0) into the input FIFO for digitization.
    pub fn feed_audio_input(&mut self, source: AudioInputSource, samples: &[f32]) {
        if self.input_source == source {
            for &s in samples {
                if self.input_fifo.len() < 16384 {
                    self.input_fifo.push_back(s.clamp(-1.0, 1.0));
                }
            }
        }
    }

    /// Clears the pending audio input FIFO.
    pub fn clear_input_buffer(&mut self) {
        self.input_fifo.clear();
    }

    /// Returns the number of unconsumed audio samples currently in the input FIFO.
    pub fn input_buffer_len(&self) -> usize {
        self.input_fifo.len()
    }

    /// Reads digitized 16-bit signed stereo PCM audio samples from the codec ADC.
    ///
    /// Applies physical jack insertion gating, input source routing, and programmable
    /// PGA gain (0.0 to +59.5 dB) on each channel.
    pub fn read_adc_stereo_sample(&mut self) -> (i16, i16) {
        // Gating: If nothing is plugged into the jack, physical inputs are grounded
        if self.jack_state == HeadsetJackState::Disconnected {
            return (0, 0);
        }

        // If MicIn is selected but only headphones (TRS, no mic) are plugged in, signal is zero
        if self.input_source == AudioInputSource::MicIn && self.jack_state != HeadsetJackState::HeadsetWithMic {
            return (0, 0);
        }

        let raw_in = match self.input_fifo.pop_front() {
            Some(v) => v,
            None => return (0, 0),
        };

        // Process Left ADC Channel
        let left_pcm = if self.is_left_adc_muted() {
            0
        } else {
            let amplified = (raw_in * self.left_linear_gain).clamp(-1.0, 1.0);
            (amplified * 32767.0) as i16
        };

        // Process Right ADC Channel
        let right_pcm = if self.is_right_adc_muted() {
            0
        } else {
            let amplified = (raw_in * self.right_linear_gain).clamp(-1.0, 1.0);
            (amplified * 32767.0) as i16
        };

        (left_pcm, right_pcm)
    }

    /// Reads a block of stereo 16-bit PCM samples from the ADC into an existing slice without heap allocation.
    pub fn read_adc_block_into(&mut self, dest: &mut [(i16, i16)]) {
        for sample in dest.iter_mut() {
            *sample = self.read_adc_stereo_sample();
        }
    }

    /// Reads a block of stereo 16-bit PCM samples from the ADC.
    pub fn read_adc_block(&mut self, count: usize) -> Vec<(i16, i16)> {
        let mut block = vec![(0i16, 0i16); count];
        self.read_adc_block_into(&mut block);
        block
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlv320aic3104_reset_defaults() {
        let codec = Tlv320aic3104::new();
        assert_eq!(codec.active_page, 0);
        assert_eq!(codec.read_reg(0), 0);
        assert_eq!(codec.read_reg(15), 0x80); // Muted Left ADC
        assert_eq!(codec.read_reg(16), 0x80); // Muted Right ADC
        assert_eq!(codec.read_reg(43), 0x00); // 0 dB Left DAC
        assert_eq!(codec.read_reg(44), 0x00); // 0 dB Right DAC
        assert_eq!(codec.get_headset_jack(), HeadsetJackState::Disconnected);
        assert!(!codec.get_headset_button());
    }

    #[test]
    fn test_page_switching() {
        let mut codec = Tlv320aic3104::new();
        // Switch to Page 1
        codec.write_reg(0, 1);
        assert_eq!(codec.read_reg(0), 1);
        // Write Page 1 filter coefficient
        codec.write_reg(27, 0x28);
        assert_eq!(codec.read_reg(27), 0x28);

        // Switch back to Page 0
        codec.write_reg(0, 0);
        assert_eq!(codec.read_reg(0), 0);
        // Page 0 reg 27 should still be default 0x00
        assert_eq!(codec.read_reg(27), 0x00);

        // Switch back to Page 1 and verify coefficient preserved
        codec.write_reg(0, 1);
        assert_eq!(codec.read_reg(27), 0x28);
    }

    #[test]
    fn test_headset_detection_register() {
        let mut codec = Tlv320aic3104::new();
        codec.set_headset_jack(HeadsetJackState::HeadsetWithMic);

        // Without bit 7 enabled in Reg 13, detection reports 00
        assert_eq!(codec.read_reg(13) & 0x60, 0x00);

        // Enable headset detection (write bit 7 = 1)
        codec.write_reg(13, 0x80);
        // Bits 6:5 should now read 0b11 (0x60) for Headset with microphone
        assert_eq!(codec.read_reg(13) & 0x60, 0x60);

        // Switch to stereo headphones (TRS)
        codec.set_headset_jack(HeadsetJackState::Headphones);
        // Bits 6:5 should read 0b01 (0x20)
        assert_eq!(codec.read_reg(13) & 0x60, 0x20);

        // Disconnect jack
        codec.set_headset_jack(HeadsetJackState::Disconnected);
        assert_eq!(codec.read_reg(13) & 0x60, 0x00);

        // Button press test
        assert_eq!(codec.read_reg(13) & 0x10, 0x00);
        codec.set_headset_button(true);
        assert_eq!(codec.read_reg(13) & 0x10, 0x10);
        codec.set_headset_button(false);
        assert_eq!(codec.read_reg(13) & 0x10, 0x00);
    }

    #[test]
    fn test_adc_pga_gain_and_digitization() {
        let mut codec = Tlv320aic3104::new();
        codec.set_headset_jack(HeadsetJackState::HeadsetWithMic);
        codec.set_audio_input_source(AudioInputSource::MicIn);

        // Unmute Left ADC and set 6.0 dB gain (12 * 0.5 dB)
        codec.write_reg(15, 12);
        assert!(!codec.is_left_adc_muted());
        assert!((codec.get_left_adc_gain_db() - 6.0).abs() < 1e-4);

        // Feed a test sample
        codec.feed_audio_input(AudioInputSource::MicIn, &[0.25]);
        let (left, right) = codec.read_adc_stereo_sample();
        // 0.25 * 10^(6/20) = ~0.25 * 1.995 = ~0.4988 -> * 32767 = ~16345
        assert!(left > 16000 && left < 16500);
        // Right is muted
        assert_eq!(right, 0);

        // Disconnect jack -> reading sample returns 0
        codec.set_headset_jack(HeadsetJackState::Disconnected);
        codec.feed_audio_input(AudioInputSource::MicIn, &[0.5]);
        let (left2, _) = codec.read_adc_stereo_sample();
        assert_eq!(left2, 0);
    }

    #[test]
    fn test_scratchpad_presence_check() {
        let mut codec = Tlv320aic3104::new();
        codec.write_reg(103, 0x5A);
        assert_eq!(codec.read_reg(103), 0x5A);
        codec.write_reg(103, 0x00);
        assert_eq!(codec.read_reg(103), 0x00);
    }
}
