use super::armv6m::ArmV6MCpu;
use std::path::Path;

pub const KEY_COUNT: usize = 25;
pub const ICON_BUTTONS_COUNT: usize = 8;
pub const SCAN_TICK_CYCLES: usize = 9_600; // 200 microseconds at 48 MHz

pub const TOUCH_BUTTON_NAMES: &[&str] = &[
    "hold",    // 0: Hold (0x1100)
    "up",      // 1: Up | A (0x1101)
    "order",   // 2: Order | B (0x1102)
    "random",  // 3: Random (0x1103)
    "pattern", // 4: Pattern (0x1104)
    "spice",   // 5: Spice (0x1105)
    "dice",    // 6: Dice (0x1106)
    "bend",    // 7: Bend (0x1107)
];

pub struct KeyboardCoprocessor {
    pub cpu: ArmV6MCpu,
    pub key_pressures: [u8; KEY_COUNT],
    pub icon_buttons: [bool; ICON_BUTTONS_COUNT],
    pub touch_strip: u16,
    pub touch_strip_touched: bool,
    pub is_stock_firmware: bool,
}

impl KeyboardCoprocessor {
    pub fn new<P: AsRef<Path>>(bin_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let bin = std::fs::read(bin_path)?;
        Self::from_bytes(&bin)
    }

    pub fn from_bytes(raw_bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut bin = raw_bytes.to_vec();
        if bin.len() < 8 {
            return Err("Keyboard coprocessor binary payload too small".into());
        }
        if bin.len() < 0x10000 {
            bin.resize(0x10000, 0xFF);
        }

        let is_stock_firmware = bin.len() >= 4 && u32::from_le_bytes(bin[0..4].try_into().unwrap()) == 0x2000_1DE8;
        if is_stock_firmware {
            // Factory / production config sector at 0x0800F800:
            // Magic [0xA5, 0x5A, 0x5A, 0xA6], 0 entries, Chip ID = 3 (Keyboard coprocessor)
            bin[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x03, 0x00, 0x00]);
            bin[0xFC00..0xFD00].fill(0);
        }

        let mut cpu = ArmV6MCpu::new(bin, 16 * 1024);

        // Initial boot burst to reach the main loop
        cpu.step_cycles(500_000);

        if is_stock_firmware {
            // Calibrate baseline = 400 and thresholds for the 25 capacitive keys:
            // Steady-state 4-sample boxcar accumulator sum is 4 * 100 = 400.
            // Release threshold Th1 = 396, Th2 = 412, Touch threshold Th3 = 425.
            for k in 0..KEY_COUNT {
                let k_addr = (0x200008C4 + k * 0x54 - 0x20000000) as usize;
                let r1 = k_addr + 0x14;
                let base = 400u16;
                cpu.sram[r1..r1 + 2].copy_from_slice(&base.to_le_bytes());
                let th1 = (base * 127 / 128).to_le_bytes();
                cpu.sram[r1 + 0xC..r1 + 0xE].copy_from_slice(&th1);
                let th3 = (base * 17 / 16).to_le_bytes();
                cpu.sram[r1 + 2..r1 + 4].copy_from_slice(&th3);
                let th2 = ((base * 17 / 16 + base) / 2).to_le_bytes();
                cpu.sram[r1 + 4..r1 + 6].copy_from_slice(&th2);
                cpu.sram[r1 + 0xF] = 0;
                cpu.sram[r1 + 0x16] = 0;
            }

            // Calibrate baseline = 400 and thresholds for the 8 Capacitive Icon Buttons at 0x20000740:
            // (Hold, Up, Order, Random, Pattern, Spice, Dice, Bend)
            for s in 0..ICON_BUTTONS_COUNT {
                let r = (0x20000740 + s * 0x2E + 0x10 - 0x20000000) as usize;
                let base = 400u16;
                cpu.sram[r..r + 2].copy_from_slice(&base.to_le_bytes());
                cpu.sram[r + 0x14..r + 0x16].copy_from_slice(&base.to_le_bytes());
                let th1 = (base * 127 / 128).to_le_bytes();
                cpu.sram[r + 0xC..r + 0xE].copy_from_slice(&th1);
                let th3 = (base * 17 / 16).to_le_bytes();
                cpu.sram[r + 2..r + 4].copy_from_slice(&th3);
                let th2 = ((base * 17 / 16 + base) / 2).to_le_bytes();
                cpu.sram[r + 4..r + 6].copy_from_slice(&th2);
                cpu.sram[r + 0xF] = 0;
                cpu.sram[r + 0x16] = 0;
                cpu.sram[r + 0x17] = 0;
                cpu.sram[r + 0x1E] = 1;
                cpu.sram[r + 0x28] = 1;
                for i in 0..4 {
                    cpu.sram[r + 0x20 + i * 2..r + 0x20 + (i + 1) * 2].copy_from_slice(&100u16.to_le_bytes());
                }
            }

            // Calibrate baseline = 400 and thresholds for the 7 Touch Strip Segments at 0x200005BC:
            for s in 0..7 {
                let r = (0x200005BC + s * 0x2C + 0x10 - 0x20000000) as usize;
                let base = 400u16;
                cpu.sram[r..r + 2].copy_from_slice(&base.to_le_bytes());
                cpu.sram[r + 0x14..r + 0x16].copy_from_slice(&base.to_le_bytes());
                let th1 = (base * 127 / 128).to_le_bytes();
                cpu.sram[r + 0xC..r + 0xE].copy_from_slice(&th1);
                let th3 = (base * 17 / 16).to_le_bytes();
                cpu.sram[r + 2..r + 4].copy_from_slice(&th3);
                let th2 = ((base * 17 / 16 + base) / 2).to_le_bytes();
                cpu.sram[r + 4..r + 6].copy_from_slice(&th2);
                cpu.sram[r + 0xF] = 0;
                cpu.sram[r + 0x16] = 0;
                cpu.sram[r + 0x17] = 0;
                cpu.sram[r + 0x1E] = 1;
                cpu.sram[r + 0x28] = 1;
                for i in 0..4 {
                    cpu.sram[r + 0x20 + i * 2..r + 0x20 + (i + 1) * 2].copy_from_slice(&100u16.to_le_bytes());
                }
            }
        }

        // Assert CTS (GPIOA Pin 11 LOW) to indicate readiness to receive
        cpu.gpioa_idr &= !(1 << 11);

        let mut kbd = Self {
            cpu,
            key_pressures: [0; KEY_COUNT],
            icon_buttons: [false; ICON_BUTTONS_COUNT],
            touch_strip: 2048,
            touch_strip_touched: false,
            is_stock_firmware,
        };

        // Warm up initial scan cycles to let firmware reach calm steady state and drain startup announcement
        kbd.step(SCAN_TICK_CYCLES * 80);
        kbd.drain_usart_tx();

        Ok(kbd)
    }

    pub fn handle_hardware_frame(&mut self, _cmd: u8, _data: &[u8]) {
        // Handle keyboard-specific hardware frames
    }

    pub fn set_key(&mut self, key_idx: usize, pressure: u8) {
        if key_idx < KEY_COUNT {
            self.key_pressures[key_idx] = pressure;
        }
    }

    pub fn touch_button_index(name: &str) -> Option<usize> {
        TOUCH_BUTTON_NAMES.iter().position(|&n| n == name)
    }

    pub fn set_touch_button(&mut self, idx: usize, pressed: bool) {
        if idx < ICON_BUTTONS_COUNT {
            self.icon_buttons[idx] = pressed;
        }
    }

    pub fn set_touch_strip(&mut self, val: u16, touched: bool) {
        self.touch_strip = val;
        self.touch_strip_touched = touched;
    }

    pub fn feed_usart_rx(&mut self, byte: u8) {
        self.cpu.feed_usart1_rx(byte);
    }

    fn update_electrode_dma(&mut self) {
        let next_group = self.cpu.sram[(0x20000518 - 0x20000000) as usize] as usize;
        let subdevice = self.cpu.sram[(0x2000051A - 0x20000000) as usize];
        let off = (0x20001114 - 0x20000000) as usize;

        let b_amp = 100u16;
        let s0 = 2000u16;
        let s2 = 2000u16;

        let k0 = next_group;
        let k1 = next_group + 8;
        let k2 = next_group + 16;

        let p0 = if k0 < KEY_COUNT { self.key_pressures[k0] as u16 } else { 0 };
        let p1 = if k1 < KEY_COUNT { self.key_pressures[k1] as u16 } else { 0 };
        let p2 = if k2 < KEY_COUNT { self.key_pressures[k2] as u16 } else { 0 };

        let s1_0 = s0 + 2 * (b_amp + p0);
        let s1_1 = s0 + 2 * (b_amp + p1);
        let s1_2 = s0 + 2 * (b_amp + p2);

        self.cpu.sram[off..off + 2].copy_from_slice(&s0.to_le_bytes());
        self.cpu.sram[off + 2..off + 4].copy_from_slice(&s1_0.to_le_bytes());
        self.cpu.sram[off + 4..off + 6].copy_from_slice(&s2.to_le_bytes());

        self.cpu.sram[off + 6..off + 8].copy_from_slice(&s0.to_le_bytes());
        self.cpu.sram[off + 8..off + 10].copy_from_slice(&s1_1.to_le_bytes());
        self.cpu.sram[off + 10..off + 12].copy_from_slice(&s2.to_le_bytes());

        self.cpu.sram[off + 12..off + 14].copy_from_slice(&s0.to_le_bytes());
        self.cpu.sram[off + 14..off + 16].copy_from_slice(&s1_2.to_le_bytes());
        self.cpu.sram[off + 16..off + 18].copy_from_slice(&s2.to_le_bytes());

        let p3 = if subdevice == 0 {
            // Subdevice 0: 8 Capacitive Icon Buttons (Hold, Up, Order, Random, Pattern, Spice, Dice, Bend)
            if next_group < ICON_BUTTONS_COUNT && self.icon_buttons[next_group] {
                120u16
            } else {
                0
            }
        } else {
            // Subdevice 1: Key 24 + Touch Strip (7 segments)
            if next_group == 0 {
                if 24 < KEY_COUNT { self.key_pressures[24] as u16 } else { 0 }
            } else {
                if self.touch_strip_touched {
                    let seg = next_group - 1; // 0..6
                    let x = (self.touch_strip as f32 * 6.0) / 4095.0;
                    let s0_idx = (x.floor() as usize).min(6);
                    let s1_idx = (s0_idx + 1).min(6);
                    let f = x - s0_idx as f32;
                    if seg == s0_idx {
                        ((1.0 - f) * 120.0) as u16
                    } else if seg == s1_idx {
                        (f * 120.0) as u16
                    } else {
                        0
                    }
                } else {
                    0
                }
            }
        };

        let s1_3 = s0 + 2 * (b_amp + p3);
        self.cpu.sram[off + 18..off + 20].copy_from_slice(&s0.to_le_bytes());
        self.cpu.sram[off + 20..off + 22].copy_from_slice(&s1_3.to_le_bytes());
        self.cpu.sram[off + 22..off + 24].copy_from_slice(&s2.to_le_bytes());
    }

    pub fn step(&mut self, mut count: usize) {
        while count > 0 {
            let chunk = count.min(SCAN_TICK_CYCLES);

            if self.is_stock_firmware {
                // Physical capacitive electrode decay modeling
                self.update_electrode_dma();

                // Hardware CTS flow control handshake on GPIOA Pin 11
                let fsm_state = self.cpu.sram[(0x20000151 - 0x20000000) as usize];
                if fsm_state == 2 {
                    self.cpu.gpioa_idr |= 1 << 11;
                } else {
                    self.cpu.gpioa_idr &= !(1 << 11);
                }
            }

            self.cpu.step_cycles(chunk);
            count -= chunk;
        }
    }

    pub fn drain_usart_tx(&mut self) -> Vec<u8> {
        self.cpu.drain_usart1_tx()
    }
}

