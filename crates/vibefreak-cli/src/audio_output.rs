use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapProd, HeapRb};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct AudioStream {
    _stream: cpal::Stream,
    _running: Arc<AtomicBool>,
}

pub struct AudioSender {
    producer: HeapProd<f32>,
    target_sample_rate: u32,
    is_primed: Arc<AtomicBool>,
}

impl AudioSender {
    /// Push raw 48 kHz stereo 16-bit signed PCM bytes (e.g. from step_audio_block_quantum)
    pub fn push_pcm_chunk(&mut self, pcm_bytes: &[u8]) {
        let num_frames = pcm_bytes.len() / 4;
        if num_frames == 0 {
            return;
        }

        if self.target_sample_rate == 48000 {
            // Direct 1:1 transfer
            for chunk in pcm_bytes.chunks_exact(4) {
                let l = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
                let r = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 32768.0;
                let _ = self.producer.try_push(l);
                let _ = self.producer.try_push(r);
            }
        } else {
            // Linear interpolation to target sample rate
            let ratio = 48000.0 / self.target_sample_rate as f64;
            let out_frames = ((num_frames as f64) * (self.target_sample_rate as f64) / 48000.0).round() as usize;
            for i in 0..out_frames {
                let src_pos = i as f64 * ratio;
                let idx = src_pos.floor() as usize;
                let frac = (src_pos - idx as f64) as f32;
                let idx_next = (idx + 1).min(num_frames - 1);

                let chunk_a = &pcm_bytes[idx * 4..(idx + 1) * 4];
                let l0 = i16::from_le_bytes([chunk_a[0], chunk_a[1]]) as f32 / 32768.0;
                let r0 = i16::from_le_bytes([chunk_a[2], chunk_a[3]]) as f32 / 32768.0;

                let chunk_b = &pcm_bytes[idx_next * 4..(idx_next + 1) * 4];
                let l1 = i16::from_le_bytes([chunk_b[0], chunk_b[1]]) as f32 / 32768.0;
                let r1 = i16::from_le_bytes([chunk_b[2], chunk_b[3]]) as f32 / 32768.0;

                let l = l0 + (l1 - l0) * frac;
                let r = r0 + (r1 - r0) * frac;

                let _ = self.producer.try_push(l);
                let _ = self.producer.try_push(r);
            }
        }

        // Pre-buffer threshold: prime consumer once at least 1,024 samples (~10.6ms stereo) are in the buffer
        if !self.is_primed.load(Ordering::Relaxed) && self.producer.occupied_len() >= 1024 {
            self.is_primed.store(true, Ordering::Release);
        }
    }

    /// Number of stereo audio frames currently queued in the ringbuffer
    pub fn occupied_frames(&self) -> usize {
        self.producer.occupied_len() / 2
    }
}

pub fn start_audio_output() -> Result<(AudioStream, AudioSender), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("No default audio output device available")?;

    let device_name = device.name().unwrap_or_else(|_| "Unknown".into());
    log::info!("Connecting native audio output to device: {}", device_name);

    // Try opening 48 kHz stereo (standard MicroFreak physical DAC rate)
    let desired_config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(48000),
        buffer_size: cpal::BufferSize::Default,
    };

    let (config, target_rate) = match device.build_output_stream(
        &desired_config,
        |_: &mut [f32], _| {},
        |err| log::warn!("Audio output stream error: {}", err),
        None,
    ) {
        Ok(_) => (desired_config, 48000),
        Err(_) => {
            // Fallback to default output config
            let default_cfg = device.default_output_config()?;
            let rate = default_cfg.sample_rate().0;
            log::info!("Device does not directly support 48000 Hz; using native rate: {} Hz", rate);
            let cfg = cpal::StreamConfig {
                channels: 2,
                sample_rate: cpal::SampleRate(rate),
                buffer_size: cpal::BufferSize::Default,
            };
            (cfg, rate)
        }
    };

    // Ring buffer for ~680ms of stereo audio at 48kHz (65536 f32 samples)
    let rb = HeapRb::<f32>::new(65536);
    let (producer, mut consumer) = rb.split();

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let is_primed = Arc::new(AtomicBool::new(false));
    let is_primed_clone = is_primed.clone();

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if !running_clone.load(Ordering::Relaxed) || !is_primed_clone.load(Ordering::Relaxed) {
                for sample in data.iter_mut() {
                    *sample = 0.0;
                }
                return;
            }
            let mut last = 0.0f32;
            for sample in data.iter_mut() {
                if let Some(s) = consumer.try_pop() {
                    last = s;
                    *sample = s;
                } else {
                    last *= 0.95;
                    *sample = last;
                }
            }
        },
        |err| log::warn!("Native audio stream error: {}", err),
        None,
    )?;

    stream.play()?;
    log::info!("Native host audio stream started at {} Hz stereo on '{}'", target_rate, device_name);

    Ok((
        AudioStream {
            _stream: stream,
            _running: running,
        },
        AudioSender {
            producer,
            target_sample_rate: target_rate,
            is_primed,
        },
    ))
}

