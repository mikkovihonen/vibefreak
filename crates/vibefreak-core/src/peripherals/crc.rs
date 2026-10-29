//! STM32 Hardware CRC-32 unit emulation (0x4002_3000)

#[derive(Debug, Clone)]
pub struct CrcPeripheral {
    pub dr: u32,
    pub idr: u8,
    pub cr: u32,
}

impl Default for CrcPeripheral {
    fn default() -> Self {
        Self {
            dr: 0xFFFF_FFFF,
            idr: 0,
            cr: 0,
        }
    }
}

impl CrcPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.dr = 0xFFFF_FFFF;
        self.cr = 0;
    }

    pub fn write_dr(&mut self, data: u32) {
        let mut crc = self.dr ^ data;
        for _ in 0..32 {
            if (crc & 0x8000_0000) != 0 {
                crc = (crc << 1) ^ 0x04C1_1DB7;
            } else {
                crc <<= 1;
            }
        }
        self.dr = crc;
    }

    pub fn write_cr(&mut self, val: u32) {
        self.cr = val & 0x01;
        if (val & 0x01) != 0 {
            self.dr = 0xFFFF_FFFF;
        }
    }

    pub fn read(&self, addr: u32) -> u32 {
        match addr & 0xFF {
            0x00 => self.dr,
            0x04 => self.idr as u32,
            0x08 => self.cr,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        match addr & 0xFF {
            0x00 => self.write_dr(val),
            0x04 => self.idr = (val & 0xFF) as u8,
            0x08 => self.write_cr(val),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc_reset_and_calculation() {
        let mut crc = CrcPeripheral::new();
        assert_eq!(crc.dr, 0xFFFF_FFFF);

        crc.write_dr(0x1234_5678);
        assert_ne!(crc.dr, 0xFFFF_FFFF);

        // Reset via CR bit 0
        crc.write_cr(0x01);
        assert_eq!(crc.dr, 0xFFFF_FFFF);
    }
}
