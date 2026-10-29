use crate::config::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct DmaStream {
    pub cr: u32,
    pub ndtr: u32,
    pub reload_ndtr: u32,
    pub par: u32,
    pub m0ar: u32,
    pub m1ar: u32,
    pub fcr: u32,
}

#[derive(Debug, Clone)]
pub struct DmaController {
    pub base_addr: u32,
    pub lisr: u32,
    pub hisr: u32,
    pub streams: [DmaStream; 8],
}

pub type Dma2Peripheral = DmaController;

impl Default for DmaController {
    fn default() -> Self {
        Self::new(DMA2_BASE)
    }
}

impl DmaController {
    pub fn new(base_addr: u32) -> Self {
        Self {
            base_addr,
            lisr: 0,
            hisr: 0,
            streams: [DmaStream::default(); 8],
        }
    }

    pub fn read(&self, addr: u32) -> u32 {
        if addr == self.base_addr + 0x00 {
            self.lisr
        } else if addr == self.base_addr + 0x04 {
            self.hisr
        } else if addr >= self.base_addr + 0x10 && addr <= self.base_addr + 0xD0 {
            let offset = addr - (self.base_addr + 0x10);
            let stream_idx = (offset / 0x18) as usize;
            let reg_offset = offset % 0x18;
            if stream_idx < 8 {
                match reg_offset {
                    0x00 => self.streams[stream_idx].cr,
                    0x04 => self.streams[stream_idx].ndtr,
                    0x08 => self.streams[stream_idx].par,
                    0x0C => self.streams[stream_idx].m0ar,
                    0x10 => self.streams[stream_idx].m1ar,
                    0x14 => self.streams[stream_idx].fcr,
                    _ => 0,
                }
            } else {
                0
            }
        } else {
            0
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        if addr == self.base_addr + 0x00 {
            self.lisr = val;
        } else if addr == self.base_addr + 0x04 {
            self.hisr = val;
        } else if addr == self.base_addr + 0x08 {
            self.lisr &= !val;
        } else if addr == self.base_addr + 0x0C {
            self.hisr &= !val;
        } else if addr >= self.base_addr + 0x10 && addr <= self.base_addr + 0xD0 {
            let offset = addr - (self.base_addr + 0x10);
            let stream_idx = (offset / 0x18) as usize;
            let reg_offset = offset % 0x18;
            if stream_idx < 8 {
                match reg_offset {
                    0x00 => {
                        let prev_en = (self.streams[stream_idx].cr & 1) != 0;
                        self.streams[stream_idx].cr = val;
                        let new_en = (val & 1) != 0;
                        if new_en && !prev_en {
                            if self.streams[stream_idx].reload_ndtr == 0 {
                                self.streams[stream_idx].reload_ndtr = self.streams[stream_idx].ndtr;
                            }
                        }
                    }
                    0x04 => {
                        self.streams[stream_idx].ndtr = val;
                        self.streams[stream_idx].reload_ndtr = val;
                    }
                    0x08 => self.streams[stream_idx].par = val,
                    0x0C => self.streams[stream_idx].m0ar = val,
                    0x10 => self.streams[stream_idx].m1ar = val,
                    0x14 => self.streams[stream_idx].fcr = val,
                    _ => {}
                }
            }
        }
    }

    pub fn get_stream_flags(&self, stream: usize) -> (bool, bool) {
        if stream >= 8 {
            return (false, false);
        }
        let is_high = stream >= 4;
        let reg = if is_high { self.hisr } else { self.lisr };
        let shift = match stream % 4 {
            0 => 0,
            1 => 6,
            2 => 16,
            3 => 22,
            _ => 0,
        };
        let tcif = (reg & (1 << (shift + 5))) != 0;
        let htif = (reg & (1 << (shift + 4))) != 0;
        (tcif, htif)
    }

    pub fn set_tcif(&mut self, stream: usize) {
        if stream >= 8 {
            return;
        }
        let is_high = stream >= 4;
        let shift = match stream % 4 {
            0 => 0,
            1 => 6,
            2 => 16,
            3 => 22,
            _ => 0,
        };
        if is_high {
            self.hisr |= 1 << (shift + 5);
        } else {
            self.lisr |= 1 << (shift + 5);
        }
    }

    pub fn set_htif(&mut self, stream: usize) {
        if stream >= 8 {
            return;
        }
        let is_high = stream >= 4;
        let shift = match stream % 4 {
            0 => 0,
            1 => 6,
            2 => 16,
            3 => 22,
            _ => 0,
        };
        if is_high {
            self.hisr |= 1 << (shift + 4);
        } else {
            self.lisr |= 1 << (shift + 4);
        }
    }

    pub fn clear_flags(&mut self, stream: usize) {
        if stream >= 8 {
            return;
        }
        let is_high = stream >= 4;
        let shift = match stream % 4 {
            0 => 0,
            1 => 6,
            2 => 16,
            3 => 22,
            _ => 0,
        };
        let mask = 0x3D << shift;
        if is_high {
            self.hisr &= !mask;
        } else {
            self.lisr &= !mask;
        }
    }

    pub fn is_stream_irq_active(&self, stream: usize) -> bool {
        if stream >= 8 {
            return false;
        }
        let (tcif, htif) = self.get_stream_flags(stream);
        let cr = self.streams[stream].cr;
        let tcie = (cr & (1 << 4)) != 0;
        let htie = (cr & (1 << 3)) != 0;
        (tcif && tcie) || (htif && htie)
    }

    pub fn execute_stream_transfer<R, W>(
        &mut self,
        stream_idx: usize,
        mut read_mem: R,
        mut write_mem: W,
    ) -> bool
    where
        R: FnMut(u32, usize) -> Option<u32>,
        W: FnMut(u32, usize, u32) -> bool,
    {
        if stream_idx >= 8 {
            return false;
        }
        let stream = &mut self.streams[stream_idx];
        if (stream.cr & 1) == 0 || stream.ndtr == 0 {
            return false;
        }

        let dir = (stream.cr >> 6) & 0x03;
        let pinc = (stream.cr & (1 << 9)) != 0;
        let minc = (stream.cr & (1 << 10)) != 0;
        let psize = match (stream.cr >> 11) & 0x03 {
            0 => 1,
            1 => 2,
            _ => 4,
        };
        let msize = match (stream.cr >> 13) & 0x03 {
            0 => 1,
            1 => 2,
            _ => 4,
        };
        let circ = (stream.cr & (1 << 8)) != 0;
        let dbm = (stream.cr & (1 << 18)) != 0;
        let ct = (stream.cr >> 19) & 1;

        let mut mem_addr = if dbm && ct == 1 {
            stream.m1ar
        } else {
            stream.m0ar
        };
        let mut periph_addr = stream.par;
        let count = stream.ndtr;

        for _ in 0..count {
            match dir {
                0b00 => {
                    // Peripheral to memory
                    if let Some(val) = read_mem(periph_addr, psize) {
                        write_mem(mem_addr, msize, val);
                    }
                }
                0b01 => {
                    // Memory to peripheral
                    if let Some(val) = read_mem(mem_addr, msize) {
                        write_mem(periph_addr, psize, val);
                    }
                }
                0b10 => {
                    // Memory to memory
                    if let Some(val) = read_mem(periph_addr, psize) {
                        write_mem(mem_addr, msize, val);
                    }
                }
                _ => {}
            }
            if pinc {
                periph_addr = periph_addr.wrapping_add(psize as u32);
            }
            if minc {
                mem_addr = mem_addr.wrapping_add(msize as u32);
            }
        }

        stream.ndtr = 0;
        if circ || dbm {
            stream.ndtr = if stream.reload_ndtr > 0 {
                stream.reload_ndtr
            } else {
                count
            };
            if dbm {
                stream.cr ^= 1 << 19; // Toggle CT
            }
        } else {
            stream.cr &= !1; // Disable stream
        }

        self.set_tcif(stream_idx);
        true
    }

    pub fn audio_ping_addr(&self) -> Option<u32> {
        let m0 = self.streams[4].m0ar;
        if m0 != 0 {
            Some(m0)
        } else {
            None
        }
    }

    pub fn audio_pong_addr(&self) -> Option<u32> {
        let m1 = self.streams[4].m1ar;
        if m1 != 0 {
            Some(m1)
        } else {
            None
        }
    }

    pub fn adc_buffer_addr(&self) -> Option<u32> {
        let m0 = self.streams[0].m0ar;
        if m0 != 0 {
            Some(m0)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dma_stream_configuration_and_ndtr_reload() {
        let mut dma = DmaController::new(DMA2_BASE);
        // Configure stream 0 NDTR
        dma.write(DMA2_BASE + 0x10 + 0x04, 128); // NDTR = 128
        assert_eq!(dma.streams[0].ndtr, 128);
        assert_eq!(dma.streams[0].reload_ndtr, 128);

        // Read back NDTR
        assert_eq!(dma.read(DMA2_BASE + 0x10 + 0x04), 128);
    }

    #[test]
    fn test_dma_stream_flag_setting_and_clearing() {
        let mut dma = DmaController::new(DMA2_BASE);

        // Test Stream 0 (in LISR, shift 0)
        dma.set_tcif(0);
        assert!(dma.get_stream_flags(0).0);
        assert_eq!(dma.read(DMA2_LISR) & (1 << 5), 1 << 5);

        dma.set_htif(0);
        assert!(dma.get_stream_flags(0).1);
        assert_eq!(dma.read(DMA2_LISR) & (1 << 4), 1 << 4);

        dma.clear_flags(0);
        assert!(!dma.get_stream_flags(0).0);
        assert!(!dma.get_stream_flags(0).1);

        // Test Stream 4 (in HISR, shift 0)
        dma.set_tcif(4);
        assert!(dma.get_stream_flags(4).0);
        assert_eq!(dma.read(DMA2_HISR) & (1 << 5), 1 << 5);

        // Enable TCIE on stream 4 and test is_stream_irq_active
        dma.streams[4].cr = 1 << 4; // TCIE
        assert!(dma.is_stream_irq_active(4));

        dma.clear_flags(4);
        assert!(!dma.is_stream_irq_active(4));
    }

    #[test]
    fn test_dma_memory_to_memory_transfer() {
        use std::cell::RefCell;
        let mut dma = DmaController::new(DMA1_BASE);
        let memory = RefCell::new([0u8; 256]);
        // Source data at 0x10..0x18
        for i in 0..8 {
            memory.borrow_mut()[0x10 + i] = (i + 1) as u8;
        }

        // Stream 1: Mem-to-Mem (DIR=0b10), PINC=1, MINC=1, EN=1
        dma.streams[1].cr = (1 << 0) | (0b10 << 6) | (1 << 9) | (1 << 10);
        dma.streams[1].par = 0x10; // Source address
        dma.streams[1].m0ar = 0x40; // Destination address
        dma.streams[1].ndtr = 8;
        dma.streams[1].reload_ndtr = 8;

        let success = dma.execute_stream_transfer(
            1,
            |addr, _size| Some(memory.borrow()[addr as usize] as u32),
            |addr, _size, val| {
                memory.borrow_mut()[addr as usize] = val as u8;
                true
            },
        );

        assert!(success);
        assert_eq!(&memory.borrow()[0x40..0x48], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(dma.streams[1].ndtr, 0);
        assert_eq!(dma.streams[1].cr & 1, 0); // Stream disabled
        assert!(dma.get_stream_flags(1).0); // TCIF set
    }

    #[test]
    fn test_dma_circular_and_double_buffer_mode() {
        use std::cell::RefCell;
        let mut dma = DmaController::new(DMA2_BASE);
        let memory = RefCell::new([0u8; 128]);
        for i in 0..4 {
            memory.borrow_mut()[i] = (0xAA + i) as u8;
        }

        // Stream 2: CIRC=1, DBM=1, CT=0, Mem-to-Mem
        dma.streams[2].cr = (1 << 0) | (0b10 << 6) | (1 << 8) | (1 << 9) | (1 << 10) | (1 << 18);
        dma.streams[2].par = 0x00; // Source
        dma.streams[2].m0ar = 0x20; // Target 0
        dma.streams[2].m1ar = 0x30; // Target 1
        dma.streams[2].ndtr = 4;
        dma.streams[2].reload_ndtr = 4;

        // First transfer writes to M0AR
        dma.execute_stream_transfer(
            2,
            |addr, _size| Some(memory.borrow()[addr as usize] as u32),
            |addr, _size, val| {
                memory.borrow_mut()[addr as usize] = val as u8;
                true
            },
        );

        assert_eq!(&memory.borrow()[0x20..0x24], &[0xAA, 0xAB, 0xAC, 0xAD]);
        assert_eq!(dma.streams[2].ndtr, 4); // Reloaded
        assert_eq!((dma.streams[2].cr >> 19) & 1, 1); // CT toggled to 1
        assert_eq!(dma.streams[2].cr & 1, 1); // Still enabled

        // Second transfer writes to M1AR
        dma.execute_stream_transfer(
            2,
            |addr, _size| Some(memory.borrow()[addr as usize] as u32),
            |addr, _size, val| {
                memory.borrow_mut()[addr as usize] = val as u8;
                true
            },
        );

        assert_eq!(&memory.borrow()[0x30..0x34], &[0xAA, 0xAB, 0xAC, 0xAD]);
        assert_eq!((dma.streams[2].cr >> 19) & 1, 0); // CT toggled back to 0
    }
}
