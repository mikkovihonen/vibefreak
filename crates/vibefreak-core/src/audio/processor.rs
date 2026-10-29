//! Downstream Analog Audio Processing Chain (SEM VCF -> Analog VCA).
//!
//! Converts raw DAC PCM audio output from the MCU synthesis engine, runs it
//! through the emulated physical 12dB/oct SEM State-Variable Filter and VCA,
//! and packs it back to 16-bit stereo PCM words for streaming to the web client.

use crate::audio::sem_filter::SemFilter;
use crate::audio::vca::AnalogVca;
use crate::config::{SemFilterMode, AUDIO_BLOCK_BYTES, AUDIO_BLOCK_FRAMES, AUDIO_CHANNELS, AUDIO_SAMPLE_RATE};

#[derive(Debug, Clone)]
pub struct AudioProcessor {
    pub filter: SemFilter,
    pub vca: AnalogVca,
    float_buffer: [f32; AUDIO_BLOCK_FRAMES * AUDIO_CHANNELS],
}

impl Default for AudioProcessor {
    fn default() -> Self {
        Self::new(AUDIO_SAMPLE_RATE as f32)
    }
}

impl AudioProcessor {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            filter: SemFilter::new(sample_rate),
            vca: AnalogVca::new(1.0, sample_rate),
            float_buffer: [0.0; AUDIO_BLOCK_FRAMES * AUDIO_CHANNELS],
        }
    }

    pub fn reset(&mut self) {
        self.filter.reset();
        self.vca.reset(1.0);
    }

    /// Sets the physical SEM filter mode based on hardware GPIO pins.
    #[inline]
    pub fn set_filter_mode(&mut self, mode: SemFilterMode) {
        self.filter.set_mode(mode);
    }

    /// Sets the cutoff frequency from a raw 12-bit ADC reading [0..4095].
    #[inline]
    pub fn set_cutoff_raw_12b(&mut self, val: u16) {
        self.filter.set_cutoff_raw_12b(val);
    }

    /// Sets the cutoff frequency from a raw 16-bit Dual-PWM output [0..65535].
    #[inline]
    pub fn set_cutoff_raw_16b(&mut self, val: u16) {
        self.filter.set_cutoff_raw_16b(val);
    }

    /// Sets the resonance Q factor from a raw 12-bit ADC reading [0..4095].
    #[inline]
    pub fn set_resonance_raw_12b(&mut self, val: u16) {
        self.filter.set_resonance_raw_12b(val);
    }

    /// Sets the target VCA gain [0.0 .. 1.0].
    #[inline]
    pub fn set_vca_gain(&mut self, gain: f32) {
        self.vca.set_target_gain(gain);
    }

    /// Processes an authentic 16-frame stereo audio block through the analog SEM filter and VCA.
    pub fn process_block(&mut self, raw_block: &mut [u8; AUDIO_BLOCK_BYTES]) {
        // 1. Decode raw 16-bit signed little-endian PCM into normalized floating-point samples [-1.0 .. 1.0]
        for (i, chunk) in raw_block.chunks_exact(2).enumerate() {
            let sample_i16 = i16::from_le_bytes([chunk[0], chunk[1]]);
            self.float_buffer[i] = (sample_i16 as f32) / 32768.0;
        }

        // 2. Process through Oberheim SEM State-Variable Filter (LP / BP / HP / Notch)
        self.filter.process_interleaved(&mut self.float_buffer);

        // 3. Process through slew-limited Analog VCA
        self.vca.process_interleaved(&mut self.float_buffer);

        // 4. Encode normalized floats back to 16-bit signed PCM words with soft clipping
        for (i, &sample_f32) in self.float_buffer.iter().enumerate() {
            let clamped = sample_f32.clamp(-1.0, 1.0);
            let sample_i16 = (clamped * 32767.0) as i16;
            let bytes = sample_i16.to_le_bytes();
            raw_block[i * 2] = bytes[0];
            raw_block[i * 2 + 1] = bytes[1];
        }
    }
}

