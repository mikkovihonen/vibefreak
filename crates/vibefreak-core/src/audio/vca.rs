//! Slew-limited Analog Voltage-Controlled Amplifier (VCA) emulation.
//!
//! Models the analog amplifier circuit modulating audio amplitude according to
//! incoming envelope/gate CVs with exponential moving average slew-limiting
//! to ensure click-free, artifact-free dynamic responses.

#[derive(Debug, Clone)]
pub struct AnalogVca {
    current_gain: f32,
    target_gain: f32,
    slew_factor: f32,
}

impl Default for AnalogVca {
    fn default() -> Self {
        Self::new(1.0, 48000.0) // 1.0 ms slew time default
    }
}

impl AnalogVca {
    pub fn new(slew_time_ms: f32, sample_rate: f32) -> Self {
        let dt = 1.0 / sample_rate;
        let tau = (slew_time_ms * 0.001).max(0.0001);
        let alpha = 1.0 - (-dt / tau).exp();
        Self {
            current_gain: 1.0,
            target_gain: 1.0,
            slew_factor: alpha,
        }
    }

    #[inline]
    pub fn set_target_gain(&mut self, target: f32) {
        self.target_gain = target.clamp(0.0, 1.0);
    }

    #[inline]
    pub fn get_gain(&self) -> f32 {
        self.current_gain
    }

    #[inline]
    pub fn reset(&mut self, initial_gain: f32) {
        self.current_gain = initial_gain.clamp(0.0, 1.0);
        self.target_gain = self.current_gain;
    }

    /// Processes an interleaved stereo audio block in-place with slew-limited amplitude modulation.
    pub fn process_interleaved(&mut self, samples: &mut [f32]) {
        let num_frames = samples.len() / 2;
        for frame in 0..num_frames {
            self.current_gain += self.slew_factor * (self.target_gain - self.current_gain);
            let idx = frame * 2;
            samples[idx] *= self.current_gain;
            samples[idx + 1] *= self.current_gain;
        }
    }
}

