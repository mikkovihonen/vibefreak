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

#[derive(Debug, Clone)]
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

impl Default for DacPeripheral {
    fn default() -> Self {
        Self {
            cr: 0,
            swtrigr: 0,
            dhr12r1: 0x0FFF,
            dhr12l1: 0x0FFF << 4,
            dhr8r1: 0xFF,
            dhr12r2: 0,
            dhr12l2: 0,
            dhr8r2: 0,
            dhr12rd: 0x0FFF,
            dhr12ld: 0x0FFF << 4,
            dhr8rd: 0xFF,
            // Default DOR1 to 0x0FFF: DAC Channel 1 drives SEM Filter Resonance with inverted analog polarity
            // (0x0FFF = 3.3V = minimum resonance Q=0.707). Initializing to 0x0FFF prevents full self-oscillation
            // before the firmware boots and loads the active patch resonance value.
            dor1: 0x0FFF,
            // Default DOR2 to 0: DAC Channel 2 drives Rear Panel Pressure CV Output (0V when no key pressed).
            dor2: 0,
            sr: 0,
        }
    }
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
                self.dor1 = val & 0x0FFF;
            }
            0x0C => {
                self.dhr12l1 = val;
                self.dor1 = (val >> 4) & 0x0FFF;
            }
            0x10 => {
                self.dhr8r1 = val;
                self.dor1 = (val & 0xFF) << 4;
            }
            0x14 => {
                self.dhr12r2 = val;
                self.dor2 = val & 0x0FFF;
            }
            0x18 => {
                self.dhr12l2 = val;
                self.dor2 = (val >> 4) & 0x0FFF;
            }
            0x1C => {
                self.dhr8r2 = val;
                self.dor2 = (val & 0xFF) << 4;
            }
            0x20 => {
                self.dhr12rd = val;
                self.dor1 = val & 0x0FFF;
                self.dor2 = (val >> 16) & 0x0FFF;
            }
            0x24 => {
                self.dhr12ld = val;
                self.dor1 = (val >> 4) & 0x0FFF;
                self.dor2 = (val >> 20) & 0x0FFF;
            }
            0x28 => {
                self.dhr8rd = val;
                self.dor1 = (val & 0xFF) << 4;
                self.dor2 = ((val >> 8) & 0xFF) << 4;
            }
            0x2C => self.dor1 = val & 0x0FFF,
            0x30 => self.dor2 = val & 0x0FFF,
            0x34 => self.sr = val,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dac_dhr12r_and_dor_updates() {
        let mut dac = DacPeripheral::default();
        assert_eq!(dac.dor1, 0x0FFF);
        assert_eq!(dac.dor2, 0);

        // Channel 1: 12-bit right-aligned write (Resonance)
        dac.write(0x08, 0x082F);
        assert_eq!(dac.dhr12r1, 0x082F);
        assert_eq!(dac.dor1, 0x082F);

        // Channel 2: 12-bit left-aligned write (Pressure CV)
        dac.write(0x18, 0x7C80);
        assert_eq!(dac.dhr12l2, 0x7C80);
        assert_eq!(dac.dor2, 0x07C8);

        // Dual 12-bit right-aligned write
        dac.write(0x20, 0x0345_0123);
        assert_eq!(dac.dor1, 0x0123);
        assert_eq!(dac.dor2, 0x0345);
    }
}

