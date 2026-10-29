use crate::config::*;

#[derive(Debug, Default)]
pub struct PwrPeripheral {
    pub cr: u32,
    pub csr: u32,
}

impl PwrPeripheral {
    pub fn new() -> Self {
        Self {
            cr: 0,
            csr: 1 << 14, // VOSRDY
        }
    }

    pub fn read(&mut self, addr: u32) -> u32 {
        match addr {
            PWR_CR => self.cr,
            PWR_CSR => {
                let mut val = 1 << 14; // VOSRDY
                if (self.cr & (1 << 16)) != 0 {
                    val |= (1 << 9) | (1 << 16);
                } // ODEN -> ODRDY
                if (self.cr & (1 << 17)) != 0 {
                    val |= 1 << 17;
                } // ODSWEN -> ODSWRDY
                self.csr = val;
                val
            }
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        match addr {
            PWR_CR => self.cr = val,
            PWR_CSR => self.csr = val,
            _ => {}
        }
    }
}
