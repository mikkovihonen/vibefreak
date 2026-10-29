//! STM32F446 SPI (SPI2, SPI3) and DAC peripheral emulation.

pub const SPI2_BASE: u32 = 0x4000_3800;
pub const SPI3_BASE: u32 = 0x4000_4000;
pub const DAC_BASE: u32 = 0x4000_7400;

#[derive(Debug, Clone)]
pub struct SpiBlock {
    pub cr1: u32,
    pub cr2: u32,
    pub sr: u32,
    pub dr: u32,
    pub i2scfgr: u32,
    pub i2spr: u32,
}

impl Default for SpiBlock {
    fn default() -> Self {
        Self {
            cr1: 0,
            cr2: 0,
            sr: 0x0000_0002, // TXE = 1 (Transmit buffer empty)
            dr: 0,
            i2scfgr: 0,
            i2spr: 0x0002,
        }
    }
}

impl SpiBlock {
    pub fn read(&self, offset: u32) -> u32 {
        match offset {
            0x00 => self.cr1,
            0x04 => self.cr2,
            0x08 => self.sr,
            0x0C => self.dr,
            0x1C => self.i2scfgr,
            0x20 => self.i2spr,
            _ => 0,
        }
    }

    pub fn write(&mut self, offset: u32, val: u32) {
        match offset {
            0x00 => self.cr1 = val,
            0x04 => self.cr2 = val,
            0x08 => self.sr = val,
            0x0C => self.dr = val,
            0x1C => self.i2scfgr = val,
            0x20 => self.i2spr = val,
            _ => {}
        }
    }
}

#[derive(Debug, Default)]
pub struct DacPeripheral {
    pub cr: u32,
    pub swtrigr: u32,
    pub dhr12r1: u32,
    pub dhr12l1: u32,
    pub dhr8r1: u32,
    pub dhr12r2: u32,
    pub dhr12l2: u32,
    pub dhr8r2: u32,
    pub dhr12rd: u32,
    pub dhr12ld: u32,
    pub dhr8rd: u32,
    pub dor1: u32,
    pub dor2: u32,
    pub sr: u32,
}

impl DacPeripheral {
    pub fn read(&self, offset: u32) -> u32 {
        match offset {
            0x00 => self.cr,
            0x04 => self.swtrigr,
            0x08 => self.dhr12r1,
            0x0C => self.dhr12l1,
            0x10 => self.dhr8r1,
            0x14 => self.dhr12r2,
            0x18 => self.dhr12l2,
            0x1C => self.dhr8r2,
            0x20 => self.dhr12rd,
            0x24 => self.dhr12ld,
            0x28 => self.dhr8rd,
            0x2C => self.dor1,
            0x30 => self.dor2,
            0x34 => self.sr,
            _ => 0,
        }
    }

    pub fn write(&mut self, offset: u32, val: u32) {
        match offset {
            0x00 => self.cr = val,
            0x04 => self.swtrigr = val,
            0x08 => {
                self.dhr12r1 = val;
                self.dor1 = val;
            }
            0x0C => self.dhr12l1 = val,
            0x10 => self.dhr8r1 = val,
            0x14 => {
                self.dhr12r2 = val;
                self.dor2 = val;
            }
            0x18 => self.dhr12l2 = val,
            0x1C => self.dhr8r2 = val,
            0x20 => self.dhr12rd = val,
            0x24 => self.dhr12ld = val,
            0x28 => self.dhr8rd = val,
            0x2C => self.dor1 = val,
            0x30 => self.dor2 = val,
            0x34 => self.sr = val,
            _ => {}
        }
    }
}

