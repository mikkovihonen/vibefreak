use super::armv6m::ArmV6MCpu;
use std::path::Path;

pub const MATRIX_ROWS: usize = 5;
pub const MATRIX_COLS: usize = 7;
pub const MATRIX_TOTAL_LEDS: usize = MATRIX_ROWS * MATRIX_COLS;
pub const PANEL_ROWS: usize = 5;
pub const PANEL_COLS: usize = 7;
pub const PANEL_LEDS_COUNT: usize = PANEL_ROWS * PANEL_COLS;

pub const ICON_LEDS_COUNT: usize = 8;

pub struct ContactDef {
    pub name: &'static str,
    pub contact_idx: usize,
    pub port: char,
    pub pin: u8,
    pub group: u8,
    pub id: u8,
}

pub const FRONT_PANEL_CONTACTS: &[ContactDef] = &[
    ContactDef { name: "oct_down", contact_idx: 18, port: 'B', pin: 11, group: 0x03, id: 0x01 },
    ContactDef { name: "oct_up", contact_idx: 20, port: 'B', pin: 12, group: 0x03, id: 0x01 },
    ContactDef { name: "assign_1", contact_idx: 1, port: 'C', pin: 1, group: 0x09, id: 0x02 },
    ContactDef { name: "assign_2", contact_idx: 2, port: 'C', pin: 2, group: 0x09, id: 0x03 },
    ContactDef { name: "shift", contact_idx: 37, port: 'C', pin: 10, group: 0x08, id: 0x04 },
    ContactDef { name: "panel", contact_idx: 30, port: 'B', pin: 8, group: 0x07, id: 0x01 },
    ContactDef { name: "utility", contact_idx: 25, port: 'C', pin: 7, group: 0x08, id: 0x03 },
    ContactDef { name: "save", contact_idx: 24, port: 'C', pin: 6, group: 0x08, id: 0x02 },
    ContactDef { name: "oct_mod", contact_idx: 12, port: 'C', pin: 4, group: 0x04, id: 0x01 },
    ContactDef { name: "rate", contact_idx: 13, port: 'C', pin: 5, group: 0x04, id: 0x05 },
    ContactDef { name: "lfo_rate", contact_idx: 14, port: 'B', pin: 0, group: 0x05, id: 0x03 },
    ContactDef { name: "lfo_shape", contact_idx: 15, port: 'B', pin: 1, group: 0x05, id: 0x00 },
    ContactDef { name: "arp_seq", contact_idx: 0, port: 'C', pin: 0, group: 0x04, id: 0x00 },
    ContactDef { name: "matrix", contact_idx: 9, port: 'A', pin: 5, group: 0x09, id: 0x01 },
    ContactDef { name: "assign_3", contact_idx: 26, port: 'C', pin: 8, group: 0x09, id: 0x04 },
    ContactDef { name: "preset", contact_idx: 8, port: 'A', pin: 4, group: 0x08, id: 0x01 },
    ContactDef { name: "amp_mod", contact_idx: 38, port: 'C', pin: 11, group: 0x06, id: 0x00 },
    ContactDef { name: "paraphonic", contact_idx: 31, port: 'B', pin: 9, group: 0x07, id: 0x00 },
    ContactDef { name: "cyc_mode", contact_idx: 39, port: 'C', pin: 12, group: 0x02, id: 0x00 },
    ContactDef { name: "filter_type", contact_idx: 32, port: 'A', pin: 8, group: 0x01, id: 0x00 },
];

pub const AUTHENTIC_CONTACT_DESCRIPTORS: [[u8; 6]; 48] = [
    [0x00, 0x04, 0x0C, 0x00, 0x03, 0x00], // 0: arp_seq
    [0x02, 0x09, 0x0C, 0x00, 0x03, 0x00], // 1: assign_1
    [0x03, 0x09, 0x0C, 0x00, 0x03, 0x00], // 2: assign_2
    [0x01, 0x02, 0x01, 0x00, 0x03, 0x00], // 3: cyc_rise
    [0x03, 0x02, 0x01, 0x00, 0x03, 0x00], // 4
    [0x04, 0x02, 0x01, 0x00, 0x03, 0x00], // 5
    [0x06, 0x02, 0x01, 0x00, 0x03, 0x00], // 6
    [0x08, 0x04, 0x01, 0x00, 0x03, 0x00], // 7
    [0x01, 0x08, 0x0C, 0x00, 0x03, 0x00], // 8: preset (encoder click)
    [0x01, 0x09, 0x0C, 0x00, 0x03, 0x00], // 9: matrix (encoder click)
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 10
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 11
    [0x01, 0x04, 0x0C, 0x00, 0x03, 0x00], // 12: oct_mod
    [0x05, 0x04, 0x0C, 0x00, 0x03, 0x00], // 13: rate (encoder push switch)
    [0x03, 0x05, 0x0C, 0x00, 0x03, 0x00], // 14: lfo_rate (encoder push switch)
    [0x00, 0x05, 0x01, 0x00, 0x03, 0x00], // 15: lfo_shape
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 16
    [0x01, 0x03, 0x01, 0x00, 0x03, 0x00], // 17: octave (display/sync)
    [0x01, 0x03, 0x02, 0x00, 0x03, 0x00], // 18: oct_down
    [0x00, 0x00, 0x01, 0x01, 0x28, 0x02], // 19
    [0x01, 0x03, 0x03, 0x00, 0x03, 0x00], // 20: oct_up
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 21
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x01], // 22
    [0x00, 0x06, 0xFF, 0xFF, 0x02, 0x00], // 23
    [0x02, 0x08, 0x0C, 0x00, 0x03, 0x00], // 24: save
    [0x03, 0x08, 0x0C, 0x00, 0x03, 0x00], // 25: utility
    [0x04, 0x09, 0x0C, 0x00, 0x03, 0x00], // 26: assign_3
    [0x00, 0x07, 0x0C, 0x00, 0x03, 0x00], // 27
    [0x02, 0x08, 0x00, 0x00, 0x03, 0x00], // 28
    [0x03, 0x08, 0x00, 0x00, 0x03, 0x00], // 29
    [0x01, 0x07, 0x0D, 0x00, 0x03, 0x00], // 30: panel
    [0x00, 0x07, 0x0C, 0x00, 0x03, 0x00], // 31: paraphonic
    [0x00, 0x01, 0x01, 0x00, 0x03, 0x00], // 32: filter_type
    [0x04, 0x09, 0x0C, 0x00, 0x03, 0x00], // 33: assign_3
    [0x00, 0x00, 0x00, 0x00, 0x01, 0x01], // 34
    [0x28, 0x02, 0x00, 0x00, 0x00, 0x00], // 35
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 36
    [0x04, 0x08, 0x0C, 0x00, 0x03, 0x00], // 37: shift
    [0x00, 0x06, 0x01, 0x00, 0x03, 0x00], // 38: amp_mod
    [0x00, 0x02, 0x01, 0x00, 0x03, 0x00], // 39: cyc_mode
    [0x02, 0x00, 0x00, 0x00, 0x01, 0x00], // 40
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 41
    [0x00, 0x00, 0x01, 0x01, 0x28, 0x02], // 42
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 43
    [0x00, 0x07, 0x01, 0x00, 0x02, 0x00], // 44
    [0x02, 0x08, 0x01, 0x00, 0x02, 0x01], // 45
    [0x03, 0x08, 0x01, 0x00, 0x02, 0x00], // 46
    [0x01, 0x07, 0x0D, 0x00, 0x02, 0x00], // 47
];

pub const AUTHENTIC_POTENTIOMETERS: &[(u8, u8)] = &[
    (0x07, 0x04), // Master Volume
    (0x01, 0x01), // Cutoff
    (0x01, 0x02), // Resonance
    (0x06, 0x01), // Attack
    (0x06, 0x02), // Decay
    (0x06, 0x03), // Sustain
    (0x06, 0x05), // Filter Amount
    (0x03, 0x00), // Glide
    (0x02, 0x01), // Cycling Rise
    (0x02, 0x03), // Cycling Fall
    (0x02, 0x04), // Cycling Hold
    (0x02, 0x06), // Cycling Amount
];



pub struct MatrixCoprocessor {
    pub cpu: ArmV6MCpu,
    pub leds: [u8; MATRIX_TOTAL_LEDS],
    pub panel_leds: [u8; PANEL_LEDS_COUNT],
    pub icon_leds: [u8; ICON_LEDS_COUNT],
    pub button_state: u16,
    pub encoder_delta: i16,
    pub global_brightness: u8,
    pub is_stock_firmware: bool,
}

impl MatrixCoprocessor {
    pub fn new<P: AsRef<Path>>(bin_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let bin = std::fs::read(bin_path)?;
        Self::from_bytes(&bin)
    }

    pub fn from_bytes(raw_bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut bin = raw_bytes.to_vec();
        if bin.len() < 8 {
            return Err("Matrix coprocessor binary payload too small".into());
        }
        if bin.len() < 0x10000 {
            bin.resize(0x10000, 0xFF);
        }

        let is_stock_firmware = bin.len() >= 4 && u32::from_le_bytes(bin[0..4].try_into().unwrap()) == 0x2000_1800;
        if is_stock_firmware {
            // Factory / production config sector at 0x0800F800:
            // Magic [0xA5, 0x5A, 0x5A, 0xA6], 0 entries, Chip ID = 2 (Matrix coprocessor)
            bin[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x02, 0x00, 0x00]);
        }

        let mut cpu = ArmV6MCpu::new(bin, 16 * 1024);
        // Initial boot burst to reach the main loop
        cpu.step_cycles(500_000);

        if is_stock_firmware {
            // Configure contact descriptors in SRAM for the matrix scanner at 0x20000704
            for (i, desc) in AUTHENTIC_CONTACT_DESCRIPTORS.iter().enumerate() {
                let off = 0x704 + i * 6;
                cpu.sram[off..off + 6].copy_from_slice(desc);
            }
        }

        // Assert CTS (GPIOA Pin 11 LOW) to indicate readiness to receive
        cpu.gpioa_idr &= !(1 << 11);

        // Step coprocessor cycles to establish baseline pin state for all contacts
        cpu.step_cycles(250_000);

        Ok(Self {
            cpu,
            leds: [0; MATRIX_TOTAL_LEDS],
            panel_leds: [0; PANEL_LEDS_COUNT],
            icon_leds: [0; ICON_LEDS_COUNT],
            button_state: 0,
            encoder_delta: 0,
            global_brightness: 255,
            is_stock_firmware,
        })
    }

    pub fn set_microswitch(&mut self, name: &str, is_pressed: bool) {
        if let Some(contact) = FRONT_PANEL_CONTACTS.iter().find(|c| c.name == name) {
            // Update physical pin level on the STM32F0 GPIO port
            self.cpu.set_pin_level(contact.port, contact.pin, !is_pressed);
        }
    }

    pub fn set_pin_level(&mut self, port: char, pin: u8, is_pressed: bool) {
        self.cpu.set_pin_level(port, pin, !is_pressed);
    }

    pub fn step_encoder(&mut self, name: &str, delta: i16) {
        let (cmd, group, id) = match name {
            "preset" => (0x0F, 0x08, 0x00),
            "matrix" => (0x0F, 0x09, 0x00),
            "type" => (0x0F, 0x00, 0x00),
            "rate" => (0x0F, 0x04, 0x03),
            "lfo_rate" => (0x0F, 0x05, 0x02),
            "wave" => (0x0F, 0x00, 0x01),
            "timbre" => (0x0F, 0x00, 0x03),
            "shape" => (0x0F, 0x00, 0x05),
            _ => return,
        };

        self.step_encoder_raw(cmd, group, id, delta);
    }

    pub fn set_potentiometer(&mut self, group: u8, id: u8, value_12b: u16) {
        if !self.is_stock_firmware {
            return;
        }
        let val_15b = ((value_12b as u32 * 32767) / 4095).min(32767) as u16;
        let b4 = ((val_15b >> 8) & 0xFF) as u8;
        let b5 = (val_15b & 0xFF) as u8;

        let write_idx = u32::from_le_bytes(
            self.cpu.sram[0x5D0..0x5D4].try_into().unwrap_or([0; 4]),
        ) as usize;
        let slot_offset = 0x5D8 + (write_idx & 0x0F) * 6;
        let event = [0x00, 0x03, group, id, b4, b5];
        self.cpu.sram[slot_offset..slot_offset + 6].copy_from_slice(&event);

        let next_write_idx = ((write_idx + 1) & 0x0F) as u32;
        self.cpu.sram[0x5D0..0x5D4].copy_from_slice(&next_write_idx.to_le_bytes());
    }

    pub fn step_encoder_raw(&mut self, cmd: u8, group: u8, id: u8, delta: i16) {
        if !self.is_stock_firmware {
            return;
        }
        let b4 = ((delta >> 8) & 0xFF) as u8;
        let b5 = (delta & 0xFF) as u8;

        let write_idx = u32::from_le_bytes(
            self.cpu.sram[0x5D0..0x5D4].try_into().unwrap_or([0; 4]),
        ) as usize;
        let slot_offset = 0x5D8 + (write_idx & 0x0F) * 6;
        let event = [0x00, cmd, group, id, b4, b5];
        self.cpu.sram[slot_offset..slot_offset + 6].copy_from_slice(&event);

        let next_write_idx = ((write_idx + 1) & 0x0F) as u32;
        self.cpu.sram[0x5D0..0x5D4].copy_from_slice(&next_write_idx.to_le_bytes());
    }

    pub fn handle_hardware_frame(&mut self, cmd: u8, data: &[u8]) {
        if cmd == 0xF7 && data.len() >= 5 && data[0] == 0x10 {
            let row = data[1] as usize;
            let col = data[2] as usize;
            let val_16b = ((data[3] as u16) << 8) | (data[4] as u16);
            let brightness = if val_16b <= 0x7FFF {
                (val_16b >> 7).min(255) as u8
            } else {
                (val_16b >> 8).min(255) as u8
            };

            if row < MATRIX_ROWS && col < MATRIX_COLS {
                let idx = row * MATRIX_COLS + col;
                self.leds[idx] = brightness;
            } else if row >= 5 && (row - 5) < PANEL_ROWS && col < PANEL_COLS {
                let panel_idx = (row - 5) * PANEL_COLS + col;
                if panel_idx < PANEL_LEDS_COUNT {
                    self.panel_leds[panel_idx] = brightness;
                }
            }
        } else if cmd == 0xF5 && data.len() >= 5 {
            let group = data[1];
            let id = data[2];
            let val_16b = ((data[3] as u16) << 8) | (data[4] as u16);
            let brightness = if val_16b == 0 {
                0
            } else if val_16b == 1 || val_16b >= 0x7FFF {
                255
            } else if val_16b <= 255 {
                val_16b as u8
            } else {
                (val_16b >> 7).min(255) as u8
            };
            match (group, id) {
                (0x09, 0x02) => self.panel_leds[21] = brightness, // Assign 1
                (0x09, 0x03) => self.panel_leds[22] = brightness, // Assign 2
                (0x09, 0x04) => self.panel_leds[23] = brightness, // Assign 3
                (0x07, 0x00) => {
                    // Paraphonic LED is driven with 0x00FF (ON) and 0x0000 (OFF).
                    // val_16b == 1 is emitted as an analog multiplexer / ADC channel frame and must not light the LED.
                    if val_16b >= 0x00FF {
                        self.panel_leds[28] = 255;
                    } else if val_16b == 0 {
                        self.panel_leds[28] = 0;
                    }
                }
                (0x08, 0x01) | (0x07, 0x01) | (0x08, 0x04) => self.panel_leds[29] = brightness, // Panel
                (0x08, 0x03) => self.panel_leds[30] = brightness, // Utility
                (0x08, 0x02) => self.panel_leds[31] = brightness, // Save
                (0x06, 0x00) => self.panel_leds[32] = brightness, // Amp Mod
                (0x04, 0x00) => self.panel_leds[33] = brightness, // Arp/Seq White (Arpeggiator)
                (0x04, 0x09) => self.panel_leds[34] = brightness, // Arp/Seq Blue (Sequencer)
                (0x05, 0x03) => self.panel_leds[6] = brightness,  // LFO Sync
                (0x03, 0x01) => {
                    // Octave Down / Up LEDs:
                    // val_16b == 1: Octave Down LED ON (panel_leds[24])
                    // val_16b == 2: Octave Up LED ON (panel_leds[25])
                    // val_16b == 0: Both Octave LEDs OFF
                    match val_16b {
                        1 => {
                            self.panel_leds[24] = 255;
                            self.panel_leds[25] = 0;
                        }
                        2 => {
                            self.panel_leds[24] = 0;
                            self.panel_leds[25] = 255;
                        }
                        _ => {
                            self.panel_leds[24] = 0;
                            self.panel_leds[25] = 0;
                        }
                    }
                }
                (0x11, 0x08) => self.icon_leds[0] = brightness,   // Hold Icon LED
                (0x11, 0x09) => self.icon_leds[1] = brightness,   // Up | A Icon LED
                (0x11, 0x0A) => self.icon_leds[2] = brightness,   // Order | B Icon LED
                (0x11, 0x0B) => self.icon_leds[3] = brightness,   // Random | O Icon LED
                (0x11, 0x0C) => self.icon_leds[4] = brightness,   // Pattern | > Icon LED
                (0x11, 0x0D) => self.icon_leds[5] = brightness,   // Spice Icon LED
                (0x11, 0x0E) => self.icon_leds[6] = brightness,   // Dice Icon LED
                (0x11, 0x0F) => self.icon_leds[7] = brightness,   // Bend Icon LED
                _ => {}
            }
        } else if cmd == 0xF6 && data.len() >= 5 {
            let col = data[0] as usize;
            if col < MATRIX_COLS {
                for r in 0..4.min(MATRIX_ROWS) {
                    let idx = r * MATRIX_COLS + col;
                    self.leds[idx] = data[r + 1];
                }
            }
        }
    }

    pub fn get_led(&self, row: usize, col: usize) -> u8 {
        if row < MATRIX_ROWS && col < MATRIX_COLS {
            self.leds[row * MATRIX_COLS + col]
        } else {
            0
        }
    }

    pub fn feed_usart_rx(&mut self, byte: u8) {
        self.cpu.feed_usart1_rx(byte);
    }

    pub fn step(&mut self, count: usize) {
        // Hardware CTS flow control handshake with transmitter FSM:
        let fsm_state = self.cpu.sram[0x5CD];
        if fsm_state == 2 {
            self.cpu.gpioa_idr |= 1 << 11;
        } else {
            self.cpu.gpioa_idr &= !(1 << 11);
        }
        self.cpu.step_cycles(count);
    }

    pub fn drain_usart_tx(&mut self) -> Vec<u8> {
        self.cpu.drain_usart1_tx()
    }
}
