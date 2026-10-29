//! STM32F446 Serial Audio Interface (SAI1 and SAI2) peripheral emulation.

pub const SAI1_BASE: u32 = 0x4001_5800;
pub const SAI2_BASE: u32 = 0x4001_5C00;

#[derive(Debug, Clone)]
pub struct SaiBlock {
    pub cr1: u32,
    pub cr2: u32,
    pub frcr: u32,
    pub slotr: u32,
    pub imr: u32,
    pub sr: u32,
    pub dr: u32,
}

impl Default for SaiBlock {
    fn default() -> Self {
        Self {
            cr1: 0x0000_0040,
            cr2: 0,
            frcr: 0,
            slotr: 0,
            imr: 0,
            sr: 0x0000_0008, // FIFO empty / ready
            dr: 0,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SaiPeripheral {
    pub gcr: u32,
    pub block_a: SaiBlock,
    pub block_b: SaiBlock,
}

impl SaiPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self, offset: u32) -> u32 {
        match offset {
            0x00 => self.gcr,
            // Block A
            0x04 => self.block_a.cr1,
            0x08 => self.block_a.cr2,
            0x0C => self.block_a.frcr,
            0x10 => self.block_a.slotr,
            0x14 => self.block_a.imr,
            0x18 => self.block_a.sr,
            0x20 => self.block_a.dr,
            // Block B
            0x24 => self.block_b.cr1,
            0x28 => self.block_b.cr2,
            0x2C => self.block_b.frcr,
            0x30 => self.block_b.slotr,
            0x34 => self.block_b.imr,
            0x38 => self.block_b.sr,
            0x40 => self.block_b.dr,
            _ => 0,
        }
    }

    pub fn write(&mut self, offset: u32, val: u32) {
        match offset {
            0x00 => self.gcr = val,
            // Block A
            0x04 => self.block_a.cr1 = val,
            0x08 => self.block_a.cr2 = val,
            0x0C => self.block_a.frcr = val,
            0x10 => self.block_a.slotr = val,
            0x14 => self.block_a.imr = val,
            0x1C => self.block_a.sr &= !val, // CLRFR
            0x20 => self.block_a.dr = val,
            // Block B
            0x24 => self.block_b.cr1 = val,
            0x28 => self.block_b.cr2 = val,
            0x2C => self.block_b.frcr = val,
            0x30 => self.block_b.slotr = val,
            0x34 => self.block_b.imr = val,
            0x3C => self.block_b.sr &= !val, // CLRFR
            0x40 => self.block_b.dr = val,
            _ => {}
        }
    }
}

