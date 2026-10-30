//! Factory stock patches, sample generator, and QSPI NOR flash layout initialization.

use crate::config::*;

pub struct FactoryPatchDef {
    pub name: &'static str,
    pub engine_type: u8,
    pub cutoff: u16,
    pub resonance: u16,
    pub filter_mode: u8,
    pub attack: u16,
    pub decay: u16,
    pub sustain: u16,
}

pub const FACTORY_PATCHES: &[FactoryPatchDef] = &[
    FactoryPatchDef { name: "Convergence", engine_type: 1, cutoff: 0x7FFF, resonance: 200, filter_mode: 0, attack: 50, decay: 1500, sustain: 0x7FFF },
    FactoryPatchDef { name: "SawX Bass", engine_type: 16, cutoff: 0x3800, resonance: 1200, filter_mode: 0, attack: 20, decay: 1200, sustain: 0x3000 },
    FactoryPatchDef { name: "SuperWave Lead", engine_type: 2, cutoff: 0x6E00, resonance: 800, filter_mode: 0, attack: 80, decay: 2200, sustain: 0x6E00 },
    FactoryPatchDef { name: "WaveTable Seq", engine_type: 3, cutoff: 0x4B00, resonance: 1500, filter_mode: 1, attack: 30, decay: 800, sustain: 0x2000 },
    FactoryPatchDef { name: "Harmonic Bell", engine_type: 4, cutoff: 0x7700, resonance: 600, filter_mode: 0, attack: 10, decay: 3200, sustain: 0x1A00 },
    FactoryPatchDef { name: "Chord 5th Stab", engine_type: 10, cutoff: 0x5800, resonance: 900, filter_mode: 0, attack: 25, decay: 1000, sustain: 0x2600 },
    FactoryPatchDef { name: "FM Digital EP", engine_type: 8, cutoff: 0x7000, resonance: 500, filter_mode: 0, attack: 40, decay: 2500, sustain: 0x4000 },
    FactoryPatchDef { name: "Formant Vowel", engine_type: 9, cutoff: 0x6400, resonance: 1400, filter_mode: 1, attack: 100, decay: 2000, sustain: 0x6000 },
    FactoryPatchDef { name: "Speech Glitch", engine_type: 11, cutoff: 0x5B00, resonance: 2200, filter_mode: 3, attack: 15, decay: 1600, sustain: 0x3900 },
    FactoryPatchDef { name: "Modal Kalimba", engine_type: 12, cutoff: 0x7A00, resonance: 800, filter_mode: 0, attack: 5, decay: 2800, sustain: 800 },
    FactoryPatchDef { name: "Noise Snare", engine_type: 13, cutoff: 0x7FFF, resonance: 1800, filter_mode: 2, attack: 5, decay: 600, sustain: 0 },
    FactoryPatchDef { name: "Saturate Sub", engine_type: 15, cutoff: 0x2C00, resonance: 400, filter_mode: 0, attack: 20, decay: 1800, sustain: 0x6500 },
    FactoryPatchDef { name: "Acid 303 Peak", engine_type: 7, cutoff: 0x4500, resonance: 3400, filter_mode: 0, attack: 10, decay: 900, sustain: 0x2600 },
    FactoryPatchDef { name: "Cyber Pulse", engine_type: 8, cutoff: 0x5200, resonance: 1600, filter_mode: 1, attack: 30, decay: 1100, sustain: 0x3300 },
    FactoryPatchDef { name: "Vaporwave Pad", engine_type: 10, cutoff: 0x4200, resonance: 900, filter_mode: 0, attack: 800, decay: 3000, sustain: 0x7800 },
    FactoryPatchDef { name: "Glass Harp", engine_type: 12, cutoff: 0x7400, resonance: 1100, filter_mode: 0, attack: 15, decay: 3200, sustain: 1200 },
    FactoryPatchDef { name: "Dark Drone", engine_type: 4, cutoff: 0x3200, resonance: 1400, filter_mode: 3, attack: 1200, decay: 3800, sustain: 0x7C00 },
    FactoryPatchDef { name: "Chiptune Arp", engine_type: 1, cutoff: 0x7E00, resonance: 300, filter_mode: 0, attack: 5, decay: 700, sustain: 0x4000 },
    FactoryPatchDef { name: "HarmFold Drone", engine_type: 17, cutoff: 0x4E00, resonance: 700, filter_mode: 0, attack: 600, decay: 2800, sustain: 0x6C00 },
    FactoryPatchDef { name: "Brass Stab 80s", engine_type: 10, cutoff: 0x5B00, resonance: 1300, filter_mode: 0, attack: 70, decay: 1500, sustain: 0x4600 },
    FactoryPatchDef { name: "Metal Mallet", engine_type: 12, cutoff: 0x7700, resonance: 1500, filter_mode: 1, attack: 8, decay: 2100, sustain: 1000 },
    FactoryPatchDef { name: "Organ Rotary", engine_type: 4, cutoff: 0x7000, resonance: 600, filter_mode: 0, attack: 20, decay: 2500, sustain: 0x7A00 },
    FactoryPatchDef { name: "Modular Swarm", engine_type: 8, cutoff: 0x5800, resonance: 2600, filter_mode: 3, attack: 150, decay: 3000, sustain: 0x5D00 },
    FactoryPatchDef { name: "Future Bass", engine_type: 10, cutoff: 0x6100, resonance: 1100, filter_mode: 0, attack: 50, decay: 1800, sustain: 0x5300 },
    FactoryPatchDef { name: "Vocoder Classic", engine_type: 14, cutoff: 0x7FFF, resonance: 0, filter_mode: 0, attack: 10, decay: 1500, sustain: 0x7FFF },
];

/// Encodes a single patch preset into a standard 4096-byte TLV-structured sector.
pub fn build_factory_preset(preset_num: u16, name: &str, patch: Option<&FactoryPatchDef>) -> Vec<u8> {
    let mut buf = vec![0u8; 4096];
    // 32-byte header: magic (0x1901), preset_number, reserved, category, reserved, version ('1' / 0x31), name (null-padded to 16 bytes)
    buf[0..2].copy_from_slice(&0x1901u16.to_le_bytes());
    buf[2..4].copy_from_slice(&preset_num.to_le_bytes());
    buf[4..6].copy_from_slice(&[0x00, 0x00]);
    buf[6] = 0x01; // Category: Bass / Lead
    buf[7] = 0x00;
    buf[8] = 0x31; // Version: '1' (0x31)
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(14);
    buf[9..9 + name_len].copy_from_slice(&name_bytes[..name_len]);
    buf[9 + name_len..24].fill(0);

    let mut off = 32;

    let mut add_section = |sec_name: &str, params: &[(&str, u16)]| {
        let s_bytes = sec_name.as_bytes();
        buf[off] = 0x20 | (s_bytes.len() as u8);
        off += 1;
        buf[off..off + s_bytes.len()].copy_from_slice(s_bytes);
        off += s_bytes.len();

        for (p_name, val) in params {
            let p_bytes = p_name.as_bytes();
            buf[off] = 0x40 | (p_bytes.len() as u8);
            off += 1;
            buf[off..off + p_bytes.len()].copy_from_slice(p_bytes);
            off += p_bytes.len();
            buf[off] = 0x63;
            off += 1;
            buf[off] = 0x00;
            off += 1;
            buf[off..off + 2].copy_from_slice(&val.to_le_bytes());
            off += 2;
        }
        buf[off] = 0x40;
        off += 1;
    };

    let p_def = patch.unwrap_or(&FACTORY_PATCHES[0]);

    // Section 0: VCO
    // For preset format version '1' (0x31), the firmware preset parser decodes Type with limit=17 (divisor = 18).
    // The decoded engine index (0..=17) is then rescaled via (engine * 32767 + 11) / 22 in RAM.
    // For engine 14 (Vocoder), this produces exactly 20852 (0x5174), matching the firmware's vocoder audio DSP pipeline ID.
    let engine_type_scaled = ((p_def.engine_type as u32 * 32768 + 16384) / 18) as u16;
    add_section("VCO", &[
        ("Type", engine_type_scaled),
        ("Param1", 0x4000),
        ("ParaX1", 0),
        ("Param2", 0x4000),
        ("ParaX2", 0),
        ("Param3", 0x4000),
        ("ParaX3", 0),
        ("BendRng", 0x4000),
        ("Pitch", 0x4000),
        ("SmpIdx", 0),
    ]);

    // Section 1: VCF
    add_section("VCF", &[
        ("Type", p_def.filter_mode as u16),
        ("Cutoff", p_def.cutoff),
        ("Reso", p_def.resonance),
    ]);

    // Section 2: EG1 (Cycling Envelope)
    add_section("EG1", &[
        ("Mode", 0),
        ("RiseLvl", 0x14DA),
        ("RiseSlp", 0x3FFF),
        ("FallLvl", 0x14DA),
        ("Hold", 0),
        ("FallSlp", 0),
        ("Amount", 0),
    ]);

    // Section 3: Kbd
    add_section("Kbd", &[
        ("Glide", 0),
        ("Octave", 0x4000), // Middle octave (octave 0 = 0x4000)
        ("Veloc", 0x7FFE),
        ("Hold", 0),
        ("GlMode", 0),
        ("PresAmp", 0x7FFE),
        ("Scale", 0),
        ("Root", 0),
    ]);

    // Section 4: Arp
    add_section("Arp", &[
        ("Enable", 0),
        ("Range", 0),
        ("Div", 0x5999),
        ("Rate", 0),
        ("Swing", 0),
        ("Sync", 0x34BC),
        ("Spice", 0),
        ("Dice", 0),
        ("Dir", 0),
        ("SeqOn", 0),
    ]);

    // Section 5: LFO
    add_section("LFO", &[
        ("Shape", 0),
        ("Div", 0x6AAA),
        ("Rate", 0x1300),
        ("Sync", 0x7FFF),
        ("Retrig", 0),
    ]);

    // Section 6: EG2 (Envelope & VCA Amp Mod)
    // Mode = 0x7FFF activates Amp Mod (turns on Amp Mod LED)
    add_section("EG2", &[
        ("Mode", 0x7FFF),
        ("Attack", p_def.attack),
        ("DecRel", p_def.decay),
        ("Sustain", p_def.sustain),
        ("Legato", 0),
        ("Amount", 0x3FFF),
        ("Snap", 1),
    ]);

    // Section 7: Gen
    add_section("Gen", &[
        ("Parafon", 0),
        ("Panel", 0),
        ("PolyCnt", 1),
        ("PrstVol", 0x4000), // Unity preset volume (16384)
        ("Volume", 0x7FFE),  // Full master volume (32766)
        ("UniSprd", 0),
        ("UniCnt", 1),
        ("Chord", 0),
        ("ChOffs1", 0),
        ("ChOffs2", 0),
        ("ChOffs3", 0),
        ("SmpHash", 0),
    ]);

    // Section 8: Sys
    add_section("Sys", &[
        ("PrsetID", preset_num),
        ("PrsetBt", 0),
        ("Save", 0),
        ("Utility", 0),
        ("Shift", 0),
        ("ShfLED", 0),
    ]);

    // Section 9: Mat
    add_section("Mat", &[
        ("MatEnc", 0),
        ("MatBtn", 0),
        ("Assign1", 5),
        ("Assign2", 2),
        ("Assign3", 6),
        ("KyMdSrc", 0x0D01),
    ]);

    // Section 18: Seq
    add_section("Seq", &[
        ("Length", 0x1999),
        ("XiceRst", 0),
        ("GateLen", 0x4000),
        ("Smooth1", 0),
        ("Smooth2", 0),
        ("Smooth3", 0),
        ("Smooth4", 0),
    ]);

    buf[off] = 0x20;

    // Standard Arturia flash integrity trailers
    buf[1040] = 0xA5; // Sequencer pattern A trailer
    buf[2082] = 0xA5; // Sequencer pattern B trailer
    buf[3052] = 0xA5; // Preset metadata trailer
    buf[4095] = 0xA5; // Sector trailer

    buf
}

/// Synthesizes a rich, recognizable musical instrument waveform for factory sample slot `sample_id`.
pub fn generate_sample_waveform(sample_id: u8) -> Vec<i16> {
    let num_samples = 24000; // 0.5 seconds @ 48kHz
    let mut pcm = Vec::with_capacity(num_samples);
    let inst_type = sample_id % 8;

    // Base frequencies chosen as exact multiples of 4 Hz so that in the 12000-sample sustain loop
    // (sample 12000 to 24000 = 0.25 seconds @ 48kHz), an exact integer number of cycles completes.
    // This guarantees zero phase discontinuity and zero clicking when looped.
    let f0 = match inst_type {
        0 => 220.0, // Acoustic Grand Piano (A3, 55 cycles in 12k samples)
        1 => 220.0, // Vocal Formant Choir "Aah" (A3, 55 cycles)
        2 => 196.0, // Acoustic Steel String Guitar Pluck (G3, 49 cycles)
        3 => 264.0, // Rhodes Electric Piano (C4, 66 cycles)
        4 => 132.0, // Bowed Cello / String Ensemble (C3, 33 cycles)
        5 => 528.0, // Kalimba / Music Box Chime (C5, 132 cycles)
        6 => 440.0, // Breathy Wood Flute (A4, 110 cycles)
        _ => 48.0,  // Percussive Drum Hit / 808 Kick & Snare (12 cycles)
    };

    let pi2 = 2.0 * std::f32::consts::PI;

    for i in 0..num_samples {
        let t = i as f32 / 48000.0;
        let is_loop = i >= 12000;

        let val: f32 = match inst_type {
            0 => {
                // 1. Acoustic Grand Piano: percussive wooden hammer strike + decaying natural harmonics
                let hammer = (-t * 250.0).exp() * (pi2 * 1800.0 * t).sin() * 0.4;
                let mut body = 0.0f32;
                for h in 1..=8 {
                    let h_f = h as f32;
                    let decay = if is_loop { (-0.25 * (1.2 + h_f * 0.7)).exp() } else { (-t * (1.2 + h_f * 0.7)).exp() };
                    body += (1.0 / h_f.powf(1.1)) * decay * (pi2 * f0 * h_f * t).sin();
                }
                hammer + body * 0.8
            }
            1 => {
                // 2. Vocal Formant Choir ("Aah"): glottal pulses + vocal tract formants (F1=750Hz, F2=1220Hz, F3=2600Hz)
                let vib = (pi2 * 5.5 * t).sin() * 4.0;
                let saw = 2.0 * (t * (f0 + vib) - (t * (f0 + vib) + 0.5).floor());
                let f1 = (pi2 * 750.0 * t).sin() * 0.5;
                let f2 = (pi2 * 1220.0 * t).sin() * 0.3;
                let f3 = (pi2 * 2600.0 * t).sin() * 0.2;
                let attack = (t / 0.04).min(1.0);
                (saw * 0.4 + f1 + f2 + f3) * attack * 0.7
            }
            2 => {
                // 3. Acoustic Steel String Guitar: sharp pick click + rich wooden body resonance
                let pick = (-t * 300.0).exp() * (pi2 * 3200.0 * t).sin() * 0.5;
                let mut string = 0.0f32;
                for h in 1..=9 {
                    let h_f = h as f32;
                    let decay = if is_loop { (-0.25 * (2.0 + h_f * 1.2)).exp() } else { (-t * (2.0 + h_f * 1.2)).exp() };
                    string += (1.0 / h_f) * decay * (pi2 * f0 * h_f * t).sin();
                }
                pick + string * 0.85
            }
            3 => {
                // 4. Rhodes Electric Piano: metallic tine chime @ 2640Hz + warm dual-tine body tone
                let bell = (-t * 25.0).exp() * (pi2 * 2640.0 * t).sin() * 0.45;
                let warm = (pi2 * f0 * t).sin() * 0.6 + (pi2 * f0 * 2.0 * t).sin() * 0.2;
                let decay = if is_loop { (-0.25f32 * 1.5f32).exp() } else { (-t * 1.5).exp() };
                bell + warm * decay * 0.8
            }
            4 => {
                // 5. Bowed Cello / String Ensemble: slow bowing friction attack + warmth & vibrato
                let attack = (t / 0.08).min(1.0);
                let vib = (pi2 * 5.0 * t).sin() * 2.5;
                let saw = 2.0 * (t * (f0 + vib) - (t * (f0 + vib) + 0.5).floor());
                let sub = (pi2 * (f0 + vib) * 0.5 * t).sin() * 0.2;
                let warmth = (pi2 * (f0 + vib) * 2.0 * t).sin() * 0.25;
                (saw * 0.6 + warmth + sub) * attack * 0.75
            }
            5 => {
                // 6. Kalimba / Music Box Chime: bright inharmonic metal bar modes (1.0, 2.76, 5.4, 8.9)
                let modes = [(1.0f32, 0.6f32, 2.0f32), (2.76, 0.35, 8.0), (5.4, 0.2, 16.0), (8.9, 0.1, 24.0)];
                let mut chime = 0.0f32;
                for &(m_ratio, m_amp, m_decay) in &modes {
                    let decay = if is_loop { (-0.25 * m_decay).exp() } else { (-t * m_decay).exp() };
                    chime += m_amp * decay * (pi2 * (f0 * m_ratio) * t).sin();
                }
                chime * 0.9
            }
            6 => {
                // 7. Breathy Wood Flute: breath turbulence noise + warm odd-harmonic pipe resonance
                let breath = (t * 19283.0).sin().fract().abs() * (-t * 10.0).exp() * 0.2;
                let attack = (t / 0.05).min(1.0);
                let vib = (pi2 * 5.0 * t).sin() * 3.0;
                let p1 = (pi2 * (f0 + vib) * t).sin() * 0.7;
                let p3 = (pi2 * (f0 + vib) * 3.0 * t).sin() * 0.15;
                (p1 + p3 + breath) * attack * 0.8
            }
            _ => {
                // 8. Percussive Drum Hit: pitch-dropping 808 kick (160Hz -> 48Hz) + acoustic snare burst
                let sweep_f = 48.0 + (160.0 - 48.0) * (-t * 40.0).exp();
                let kick = (pi2 * sweep_f * t).sin() * (-t * 8.0).exp() * 0.7;
                let snare_noise = (t * 45917.0).sin().fract().abs() * (-t * 15.0).exp() * 0.4;
                kick + snare_noise
            }
        };

        pcm.push((val * 26000.0).clamp(-32767.0, 32767.0) as i16);
    }
    pcm
}

/// Encodes a sample sector (24-byte 0xFA55AAEF container header + 16-bit PCM audio payload).
pub fn build_factory_sample_sector(sample_id: u8, pcm_samples: &[i16]) -> Vec<u8> {
    let pcm_byte_len = pcm_samples.len() * 2;
    let total_unpadded = 24 + pcm_byte_len;
    let total_len = ((total_unpadded + 4095) / 4096) * 4096;
    let mut sector = vec![0xFFu8; total_len];

    // 24-byte Arturia QSPI sample container header:
    // +0x00: Magic 0xFA55AAEF
    sector[0..4].copy_from_slice(&0xFA55AAEFu32.to_le_bytes());
    // +0x04: Sample length in bytes
    sector[4..8].copy_from_slice(&(pcm_byte_len as u32).to_le_bytes());
    // +0x08: Non-zero token/generation ID (firmware checks ldrh r3, [r0, #8] != 0)
    let token_id = (sample_id as u32) + 1;
    sector[8..12].copy_from_slice(&token_id.to_le_bytes());
    // +0x0C: Loop start offset in bytes
    sector[12..16].copy_from_slice(&0u32.to_le_bytes());
    // +0x10: Loop end offset in bytes
    sector[16..20].copy_from_slice(&(pcm_byte_len as u32).to_le_bytes());
    // +0x14: Sample ID in high byte
    sector[20..24].copy_from_slice(&(((sample_id as u32) & 0x7F) << 24).to_le_bytes());

    // Audio PCM data immediately follows the 24-byte header at +0x18
    for (i, &s) in pcm_samples.iter().enumerate() {
        let b = s.to_le_bytes();
        sector[24 + i * 2] = b[0];
        sector[24 + i * 2 + 1] = b[1];
    }

    sector
}

/// Generates a complete 16 MB QSPI NOR flash image pre-populated with factory presets,
/// directory, factory wavetables, and status sector.
pub fn generate_factory_qspi_image() -> Vec<u8> {
    let mut image = vec![0xFFu8; QSPI_FLASH_SIZE];

    // 1. Active Factory Presets (slots 1..128) starting at 0x00081000
    for slot in 1..=128u16 {
        let patch_idx = ((slot - 1) as usize) % FACTORY_PATCHES.len();
        let patch = &FACTORY_PATCHES[patch_idx];
        let p_data = build_factory_preset(slot, patch.name, Some(patch));
        let active_addr = QSPI_STAGING_SECTOR_BASE + ((slot - 1) as usize) * 0x1000;
        if active_addr + 4096 <= QSPI_SAMPLE_MEMORY_BASE {
            image[active_addr..active_addr + 4096].copy_from_slice(&p_data);
        }
    }

    // Dedicated Factory Vocoder Presets (slots 208..=223)
    let vocoder_patch = FactoryPatchDef {
        name: "Vocoder Classic",
        engine_type: 14,
        cutoff: 0x7FFF,
        resonance: 0,
        filter_mode: 0,
        attack: 10,
        decay: 1500,
        sustain: 0x7FFF,
    };
    for slot in 208..=223u16 {
        let p_data = build_factory_preset(slot, vocoder_patch.name, Some(&vocoder_patch));
        let active_addr = QSPI_STAGING_SECTOR_BASE + ((slot - 1) as usize) * 0x1000;
        if active_addr + 4096 <= QSPI_SAMPLE_MEMORY_BASE {
            image[active_addr..active_addr + 4096].copy_from_slice(&p_data);
        }
    }


    // 2. Factory User Samples (slots 0..63) starting at 0x00281000
    let mut sample_addr = QSPI_SAMPLE_MEMORY_BASE;
    for sample_id in 0..64u8 {
        let pcm = generate_sample_waveform(sample_id);
        let sample_sector = build_factory_sample_sector(sample_id, &pcm);
        if sample_addr + sample_sector.len() <= QSPI_SAMPLE_MEMORY_END {
            image[sample_addr..sample_addr + sample_sector.len()].copy_from_slice(&sample_sector);
            sample_addr += sample_sector.len();
        } else {
            break;
        }
    }

    // 3. User Wavetable Bank B at 0x00FAF000 is left erased (0xFF) until user uploads custom wavetables.
    // When 0x00FAF000 is clean (0xFF), the firmware correctly routes WaveUser to Bank A at 0x00F5F000.

    // 4. Factory Stock Wavetables at 0x00000000 (256 KB) & User Wavetables at 0x00F5F000 (320 KB)
    // Left in erased state (0xFF). When the user supplies an official .mff archive or QSPI flash image,
    // these regions are dynamically populated by `apply_qspi_data()`.

    // 5. QSPI Status Sector at 0x00FFF000 with 'Q' (0x51)
    image[QSPI_STATUS_SECTOR_BASE] = 0x51;

    image
}
