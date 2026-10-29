//! Physical Analog Audio Emulation Subsystem.
//!
//! Models the downstream analog hardware stages following the audio DAC:
//! - Oberheim SEM-style 12dB/octave State-Variable Filter (SVF) with LP/BP/HP/Notch modes
//! - Voltage-Controlled Amplifier (VCA) with slew-limited gain modulation
//! - Direct mapping from firmware GPIO pins and ADC/potentiometer CV controls

pub mod processor;
pub mod sem_filter;
pub mod vca;

pub use processor::AudioProcessor;
pub use sem_filter::SemFilter;
pub use vca::AnalogVca;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SemFilterMode, AUDIO_BLOCK_BYTES, AUDIO_BLOCK_FRAMES};
    use std::f32::consts::PI;

    #[test]
    fn test_sem_filter_lowpass_response() {
        let mut filter = SemFilter::new(48000.0);
        filter.set_mode(SemFilterMode::LowPass);
        filter.set_cutoff_hz(1000.0);
        filter.set_resonance_q(0.707); // Butterworth Q

        // Generate 200 Hz sine wave (well below 1000 Hz cutoff) -> should pass with ~1.0 gain
        let fs = 48000.0;
        let mut low_sine = Vec::new();
        for n in 0..4800 {
            let sample = (2.0 * PI * 200.0 * (n as f32) / fs).sin();
            low_sine.push(sample);
            low_sine.push(sample);
        }
        filter.process_interleaved(&mut low_sine);

        let max_low = low_sine[2400..].iter().copied().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            (max_low - 1.0).abs() < 0.05,
            "Expected ~1.0 gain for low frequency, got {}",
            max_low
        );

        // Generate 8000 Hz sine wave (well above 1000 Hz cutoff) -> should be heavily attenuated (> -18 dB)
        filter.reset();
        let mut high_sine = Vec::new();
        for n in 0..4800 {
            let sample = (2.0 * PI * 8000.0 * (n as f32) / fs).sin();
            high_sine.push(sample);
            high_sine.push(sample);
        }
        filter.process_interleaved(&mut high_sine);

        let max_high = high_sine[2400..].iter().copied().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            max_high < 0.1,
            "Expected high frequency attenuation (< 0.1), got {}",
            max_high
        );
    }

    #[test]
    fn test_sem_filter_highpass_response() {
        let mut filter = SemFilter::new(48000.0);
        filter.set_mode(SemFilterMode::HighPass);
        filter.set_cutoff_hz(2000.0);
        filter.set_resonance_q(0.707);

        let fs = 48000.0;
        // Low frequency (200 Hz) should be attenuated in High-Pass mode
        let mut low_sine = Vec::new();
        for n in 0..4800 {
            let sample = (2.0 * PI * 200.0 * (n as f32) / fs).sin();
            low_sine.push(sample);
            low_sine.push(sample);
        }
        filter.process_interleaved(&mut low_sine);
        let max_low = low_sine[2400..].iter().copied().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(max_low < 0.15, "Expected low frequency attenuation in HP mode, got {}", max_low);

        // High frequency (8000 Hz) should pass through High-Pass mode
        filter.reset();
        let mut high_sine = Vec::new();
        for n in 0..4800 {
            let sample = (2.0 * PI * 8000.0 * (n as f32) / fs).sin();
            high_sine.push(sample);
            high_sine.push(sample);
        }
        filter.process_interleaved(&mut high_sine);
        let max_high = high_sine[2400..].iter().copied().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!((max_high - 1.0).abs() < 0.1, "Expected ~1.0 gain in HP mode, got {}", max_high);
    }

    #[test]
    fn test_sem_filter_bandpass_resonance_peak() {
        let mut filter = SemFilter::new(48000.0);
        filter.set_mode(SemFilterMode::BandPass);
        filter.set_cutoff_hz(1000.0);
        filter.set_resonance_q(4.0); // High resonance Q
        filter.reset();

        let fs = 48000.0;
        // Frequency at cutoff (1000 Hz) should peak at Q = 4.0
        let mut center_sine = Vec::new();
        for n in 0..4800 {
            let sample = (2.0 * PI * 1000.0 * (n as f32) / fs).sin();
            center_sine.push(sample);
            center_sine.push(sample);
        }
        filter.process_interleaved(&mut center_sine);
        let max_center = center_sine[2400..].iter().copied().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            (max_center - 4.0).abs() < 0.2,
            "Expected peak gain ~4.0 for resonant BP at fc, got {}",
            max_center
        );
    }

    #[test]
    fn test_sem_filter_notch_rejection() {
        let mut filter = SemFilter::new(48000.0);
        filter.set_mode(SemFilterMode::Notch);
        filter.set_cutoff_hz(1000.0);
        filter.set_resonance_q(1.0);
        filter.reset();

        let fs = 48000.0;
        // Frequency at notch center (1000 Hz) should be strongly nullified
        let mut center_sine = Vec::new();
        for n in 0..4800 {
            let sample = (2.0 * PI * 1000.0 * (n as f32) / fs).sin();
            center_sine.push(sample);
            center_sine.push(sample);
        }
        filter.process_interleaved(&mut center_sine);
        let max_center = center_sine[2400..].iter().copied().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            max_center < 0.05,
            "Expected deep notch rejection (< 0.05), got {}",
            max_center
        );
    }

    #[test]
    fn test_analog_vca_slew_limiting() {
        let mut vca = AnalogVca::new(5.0, 48000.0); // 5 ms slew
        vca.reset(0.0);
        vca.set_target_gain(1.0);

        let mut block = vec![1.0f32; 100]; // 50 frames
        vca.process_interleaved(&mut block);

        // First sample should be close to 0, not instantaneous 1
        assert!(block[0] < 0.05, "VCA gain changed too quickly without slew");
        // Gain should smoothly ramp up
        assert!(block[98] > block[0], "VCA gain did not increase smoothly");
    }

    #[test]
    fn test_audio_processor_block_processing() {
        let mut proc = AudioProcessor::new(48000.0);
        proc.set_filter_mode(SemFilterMode::LowPass);
        proc.set_cutoff_raw_12b(4095); // 20 kHz (open)
        proc.set_resonance_raw_12b(0);  // Clean

        let mut raw = [0u8; AUDIO_BLOCK_BYTES];
        // Populate with alternating positive/negative 16-bit PCM
        for i in 0..AUDIO_BLOCK_FRAMES {
            let val: i16 = 16384;
            let bytes = val.to_le_bytes();
            raw[i * 4] = bytes[0];
            raw[i * 4 + 1] = bytes[1];
            raw[i * 4 + 2] = bytes[0];
            raw[i * 4 + 3] = bytes[1];
        }

        proc.process_block(&mut raw);

        // Verify valid output non-zero PCM
        let out_sample = i16::from_le_bytes([raw[0], raw[1]]);
        assert!(out_sample > 10000, "Expected processed audio to pass through open filter");
    }
}
