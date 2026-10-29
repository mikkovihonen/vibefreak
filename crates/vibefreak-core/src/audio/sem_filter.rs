//! 12 dB/octave (2-pole) Oberheim SEM State-Variable Filter (SVF) emulation.
//!
//! Implements a zero-delay feedback (ZDF) / Topology-Preserving Transform (TPT)
//! state-variable filter model with Low-Pass, Band-Pass, High-Pass, and Notch modes,
//! matching the physical analog SEM circuit on the Arturia MicroFreak front panel.

use crate::config::SemFilterMode;

#[derive(Debug, Clone, Copy)]
pub struct SvfChannel {
    pub s1: f32,
    pub s2: f32,
}

impl Default for SvfChannel {
    fn default() -> Self {
        Self { s1: 0.0, s2: 0.0 }
    }
}

impl SvfChannel {
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// Processes a single sample through the 2-pole ZDF/TPT SVF.
    ///
    /// - `g` is the prewarped frequency coefficient: `tan(pi * fc / fs)`
    /// - `k` is the damping factor: `1.0 / Q` (where `Q >= 0.5`)
    /// - `mode` is the active physical filter mode (LP, BP, HP, Notch)
    #[inline(always)]
    pub fn process_sample(&mut self, input: f32, g: f32, k: f32, mode: SemFilterMode) -> f32 {
        let denom = 1.0 + k * g + g * g;
        let u = (input - (k + g) * self.s1 - self.s2) / denom;
        let v1 = self.s1 + g * u;
        let v2 = self.s2 + g * v1;

        // Trapezoidal integrator state updates
        self.s1 += 2.0 * g * u;
        self.s2 += 2.0 * g * v1;

        // Soft-clip internal states to model analog diode saturation
        // and prevent numerical blowup at extreme self-oscillation
        if self.s1.abs() > 4.0 {
            self.s1 = self.s1.clamp(-4.0, 4.0);
        }
        if self.s2.abs() > 4.0 {
            self.s2 = self.s2.clamp(-4.0, 4.0);
        }

        match mode {
            SemFilterMode::LowPass => v2,
            SemFilterMode::BandPass => v1,
            SemFilterMode::HighPass => u,
            SemFilterMode::Notch => u + v2,
        }
    }
}

/// Stereo 2-pole Oberheim SEM State-Variable Filter with audio-rate parameter interpolation.
#[derive(Debug, Clone)]
pub struct SemFilter {
    channels: [SvfChannel; 2],
    sample_rate: f32,
    cutoff_hz: f32,
    target_cutoff_hz: f32,
    resonance_q: f32,
    target_resonance_q: f32,
    mode: SemFilterMode,
}

impl SemFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            channels: [SvfChannel::default(); 2],
            sample_rate,
            cutoff_hz: 20000.0,
            target_cutoff_hz: 20000.0,
            resonance_q: 0.707,
            target_resonance_q: 0.707,
            mode: SemFilterMode::LowPass,
        }
    }

    pub fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.reset();
        }
        self.cutoff_hz = self.target_cutoff_hz;
        self.resonance_q = self.target_resonance_q;
    }

    #[inline]
    pub fn set_mode(&mut self, mode: SemFilterMode) {
        self.mode = mode;
    }

    #[inline]
    pub fn get_mode(&self) -> SemFilterMode {
        self.mode
    }

    /// Sets the target cutoff frequency in Hz [20.0 .. 20480.0].
    #[inline]
    pub fn set_cutoff_hz(&mut self, fc: f32) {
        let clamped = fc.clamp(20.0, (self.sample_rate * 0.49).min(20480.0));
        self.target_cutoff_hz = clamped;
        if self.channels[0].s1 == 0.0 && self.channels[0].s2 == 0.0 && self.channels[1].s1 == 0.0 && self.channels[1].s2 == 0.0 {
            self.cutoff_hz = clamped;
        }
    }

    /// Maps a 12-bit raw ADC/potentiometer reading [0..4095] exponentially to [20.0 .. 20480.0 Hz].
    #[inline]
    pub fn set_cutoff_raw_12b(&mut self, raw_12b: u16) {
        let norm = (raw_12b.min(4095) as f32) / 4095.0;
        // 20 Hz * 2^(10.0 * norm) covers 20 Hz to 20,480 Hz (10 octaves, ~1V/octave tracking)
        let fc = 20.0 * 2.0f32.powf(9.965784 * norm);
        self.set_cutoff_hz(fc);
    }

    /// Maps a 16-bit raw Dual-PWM DAC output [0..65535] from TIM3 CCR3/CCR4 exponentially to [20.0 .. 20480.0 Hz].
    #[inline]
    pub fn set_cutoff_raw_16b(&mut self, raw_16b: u16) {
        let norm = (raw_16b as f32) / 65535.0;
        let fc = 20.0 * 2.0f32.powf(9.965784 * norm);
        self.set_cutoff_hz(fc);
    }

    /// Sets the target resonance Q factor [0.5 .. 25.0].
    #[inline]
    pub fn set_resonance_q(&mut self, q: f32) {
        let clamped = q.clamp(0.5, 25.0);
        self.target_resonance_q = clamped;
        if self.channels[0].s1 == 0.0 && self.channels[0].s2 == 0.0 && self.channels[1].s1 == 0.0 && self.channels[1].s2 == 0.0 {
            self.resonance_q = clamped;
        }
    }

    /// Maps a 12-bit raw ADC/potentiometer reading [0..4095] to resonance Q [0.5 .. 25.0].
    #[inline]
    pub fn set_resonance_raw_12b(&mut self, raw_12b: u16) {
        let norm = (raw_12b.min(4095) as f32) / 4095.0;
        // Quadratic taper for musical control over subtle resonance to self-oscillation
        let q = 0.5 + 24.5 * (norm * norm);
        self.set_resonance_q(q);
    }

    /// Processes an interleaved stereo block of floating-point audio samples in-place.
    /// Smoothly interpolates cutoff frequency and resonance across the block to eliminate zipper noise.
    pub fn process_interleaved(&mut self, samples: &mut [f32]) {
        let num_frames = samples.len() / 2;
        if num_frames == 0 {
            return;
        }

        let cutoff_step = (self.target_cutoff_hz - self.cutoff_hz) / (num_frames as f32);
        let q_step = (self.target_resonance_q - self.resonance_q) / (num_frames as f32);

        for frame in 0..num_frames {
            self.cutoff_hz += cutoff_step;
            self.resonance_q += q_step;

            let g = (std::f32::consts::PI * self.cutoff_hz / self.sample_rate).tan();
            let k = 1.0 / self.resonance_q;

            let idx = frame * 2;
            let left_in = samples[idx];
            let right_in = samples[idx + 1];

            samples[idx] = self.channels[0].process_sample(left_in, g, k, self.mode);
            samples[idx + 1] = self.channels[1].process_sample(right_in, g, k, self.mode);
        }

        self.cutoff_hz = self.target_cutoff_hz;
        self.resonance_q = self.target_resonance_q;
    }
}
