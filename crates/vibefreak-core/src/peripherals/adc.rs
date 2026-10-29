//! STM32F446 ADC1 and Common ADC registers peripheral emulation.

pub const ADC1_BASE: u32 = 0x4001_2000;
pub const ADC_COMMON_BASE: u32 = 0x4001_2300;

#[derive(Debug, Default)]
pub struct AdcPeripheral {
    pub sr: u32,
    pub cr1: u32,
    pub cr2: u32,
    pub smpr1: u32,
    pub smpr2: u32,
    pub sqr1: u32,
    pub sqr2: u32,
    pub sqr3: u32,
    pub dr: u32,

    // Common registers (0x4001_2300)
    pub csr: u32,
    pub ccr: u32,
}

impl AdcPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self, addr: u32) -> u32 {
        match addr {
            0x4001_2000 => self.sr | 0x02, // EOC (End of Conversion) ready
            0x4001_2004 => self.cr1,
            0x4001_2008 => self.cr2,
            0x4001_200C => self.smpr1,
            0x4001_2010 => self.smpr2,
            0x4001_202C => self.sqr1,
            0x4001_2030 => self.sqr2,
            0x4001_2034 => self.sqr3,
            0x4001_204C => self.dr,

            0x4001_2300 => self.csr,
            0x4001_2304 => self.ccr,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        match addr {
            0x4001_2000 => self.sr &= val,
            0x4001_2004 => self.cr1 = val,
            0x4001_2008 => self.cr2 = val,
            0x4001_200C => self.smpr1 = val,
            0x4001_2010 => self.smpr2 = val,
            0x4001_202C => self.sqr1 = val,
            0x4001_2030 => self.sqr2 = val,
            0x4001_2034 => self.sqr3 = val,
            0x4001_204C => self.dr = val,

            0x4001_2300 => self.csr = val,
            0x4001_2304 => self.ccr = val,
            _ => {}
        }
    }
}

