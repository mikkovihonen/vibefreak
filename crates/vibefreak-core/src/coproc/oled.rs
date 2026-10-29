use super::armv6m::{ArmV6MCpu, VECTOR_USART1};
use std::path::Path;

pub const OLED_FRAMEBUFFER_OFFSET: usize = 0x68;
pub const OLED_FRAMEBUFFER_ADDR: u64 = 0x2000_0068;
pub const OLED_FRAMEBUFFER_SIZE: usize = 1024;

pub struct OledCoprocessor {
    pub cpu: ArmV6MCpu,
    pub pc: u32,
    pub is_booted: bool,
    pub is_stock_firmware: bool,
    pub display_framebuffer: [u8; OLED_FRAMEBUFFER_SIZE],
    pub cycle_accumulator: usize,
}

impl OledCoprocessor {
    pub fn new<P: AsRef<Path>>(bin_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let bin_data = std::fs::read(bin_path)?;
        Self::from_bytes(&bin_data)
    }

    pub fn from_bytes(raw_bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut bin_data = raw_bytes.to_vec();
        if bin_data.len() < 8 {
            return Err("OLED binary too small".into());
        }

        if bin_data.len() < 0x10000 {
            bin_data.resize(0x10000, 0xFF);
        }

        let is_stock_firmware = bin_data.len() >= 4
            && u32::from_le_bytes(bin_data[0..4].try_into().unwrap()) == 0x2000_20F0;

        if is_stock_firmware {
            // Factory / production config sector at 0x0800F800:
            // Magic [0xA5, 0x5A, 0x5A, 0xA6], 0 entries, Chip ID = 1 (OLED coprocessor)
            bin_data[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x01, 0x00, 0x00]);
        }

        let mut cpu = ArmV6MCpu::new(bin_data, 32 * 1024);

        // Initial boot burst to reach main loop
        cpu.step_cycles(800_000);
        let pc = cpu.regs[15];

        let mut display_framebuffer = [0u8; OLED_FRAMEBUFFER_SIZE];
        if cpu.sram.len() >= OLED_FRAMEBUFFER_OFFSET + OLED_FRAMEBUFFER_SIZE {
            display_framebuffer.copy_from_slice(
                &cpu.sram[OLED_FRAMEBUFFER_OFFSET..OLED_FRAMEBUFFER_OFFSET + OLED_FRAMEBUFFER_SIZE],
            );
        }
        cpu.dma_frame_ready = false;

        Ok(Self {
            cpu,
            pc,
            is_booted: true,
            is_stock_firmware,
            display_framebuffer,
            cycle_accumulator: 0,
        })
    }

    pub fn feed_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.cpu.feed_usart1_rx(b);
        }
    }

    pub fn service_usart1_rx(&mut self) {
        if (self.cpu.usart1.cr1 & (1 << 5)) == 0 {
            return;
        }
        while !self.cpu.usart1.rx.is_empty() && !self.cpu.is_halted {
            if (self.cpu.usart1.cr1 & (1 << 5)) == 0 {
                break;
            }
            if !self.cpu.dispatch_vector(VECTOR_USART1) {
                break;
            }
            let mut steps = 0;
            while self.cpu.in_handler_mode && steps < 5_000 && !self.cpu.is_halted {
                self.cpu.step_cycles(1);
                steps += 1;
            }
            if self.cpu.in_handler_mode {
                break;
            }
        }
        self.pc = self.cpu.regs[15];
    }

    pub fn update_latched_framebuffer(&mut self) {
        if self.cpu.dma_frame_ready {
            let offset = if self.cpu.dma1_cmar3 >= 0x2000_0000 {
                (self.cpu.dma1_cmar3 - 0x2000_0000) as usize
            } else {
                OLED_FRAMEBUFFER_OFFSET
            };
            if self.cpu.sram.len() >= offset + OLED_FRAMEBUFFER_SIZE {
                self.display_framebuffer.copy_from_slice(
                    &self.cpu.sram[offset..offset + OLED_FRAMEBUFFER_SIZE],
                );
            }
            self.cpu.dma_frame_ready = false;
        }
    }

    pub fn step(&mut self, count: usize) {
        if !self.is_booted {
            return;
        }

        self.cpu.step_cycles(count);
        self.pc = self.cpu.regs[15];

        // Latch completed frame on DMA completion
        self.update_latched_framebuffer();
    }

    pub fn get_framebuffer(&self) -> [u8; OLED_FRAMEBUFFER_SIZE] {
        self.display_framebuffer
    }

    pub fn drain_usart_tx(&mut self) -> Vec<u8> {
        self.cpu.drain_usart1_tx()
    }
}
