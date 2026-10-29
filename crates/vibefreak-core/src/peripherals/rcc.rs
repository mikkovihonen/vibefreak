use crate::config::*;

#[derive(Debug, Default)]
pub struct RccPeripheral {
    pub cr: u32,
    pub pllcfgr: u32,
    pub cfgr: u32,
    pub ahb1enr: u32,
    pub ahb2enr: u32,
    pub ahb3enr: u32,
    pub apb1enr: u32,
    pub apb2enr: u32,
    pub pllsaicfgr: u32,
    pub plli2scfgr: u32,
    pub dckcfgr: u32,
    pub dckcfgr2: u32,
}

impl RccPeripheral {
    pub fn new() -> Self {
        Self {
            // HSION and HSIRDY enabled at reset
            cr: (1 << 0) | (1 << 1),
            pllsaicfgr: 0x2400_3010,
            plli2scfgr: 0x2000_3010,
            ..Default::default()
        }
    }

    pub fn read(&mut self, addr: u32) -> u32 {
        match addr {
            RCC_CR => {
                let mut val = self.cr & !((1 << 1) | (1 << 17) | (1 << 25) | (1 << 27) | (1 << 29));
                if (self.cr & (1 << 0)) != 0 {
                    val |= 1 << 1;
                } // HSIRDY follows HSION
                if (self.cr & (1 << 16)) != 0 {
                    val |= 1 << 17;
                } // HSERDY follows HSEON
                if (self.cr & (1 << 24)) != 0 {
                    val |= 1 << 25;
                } // PLLRDY follows PLLON
                if (self.cr & (1 << 26)) != 0 {
                    val |= 1 << 27;
                } // PLLI2SRDY follows PLLI2SON
                if (self.cr & (1 << 28)) != 0 {
                    val |= 1 << 29;
                } // PLLSAIRDY follows PLLSAION
                self.cr = val;
                val
            }
            RCC_PLLCFGR => self.pllcfgr,
            RCC_CFGR => {
                let sw = self.cfgr & 0x03;
                let val = (self.cfgr & !0x0C) | (sw << 2); // SWS matches SW
                self.cfgr = val;
                val
            }
            RCC_AHB1ENR => self.ahb1enr,
            RCC_AHB2ENR => self.ahb2enr,
            RCC_AHB3ENR => self.ahb3enr,
            RCC_APB1ENR => self.apb1enr,
            RCC_APB2ENR => self.apb2enr,
            RCC_PLLSAICFGR => self.pllsaicfgr,
            RCC_PLLI2SCFGR => self.plli2scfgr,
            RCC_DCKCFGR => self.dckcfgr,
            RCC_DCKCFGR2 => self.dckcfgr2,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        match addr {
            RCC_CR => {
                let mut filtered =
                    val & !((1 << 1) | (1 << 17) | (1 << 25) | (1 << 27) | (1 << 29));
                if (val & (1 << 0)) != 0 {
                    filtered |= 1 << 1;
                }
                if (val & (1 << 16)) != 0 {
                    filtered |= 1 << 17;
                }
                if (val & (1 << 24)) != 0 {
                    filtered |= 1 << 25;
                }
                if (val & (1 << 26)) != 0 {
                    filtered |= 1 << 27;
                }
                if (val & (1 << 28)) != 0 {
                    filtered |= 1 << 29;
                }
                self.cr = filtered;
            }
            RCC_PLLCFGR => self.pllcfgr = val,
            RCC_CFGR => {
                let sw = val & 0x03;
                self.cfgr = (val & !0x0C) | (sw << 2);
            }
            RCC_AHB1ENR => self.ahb1enr = val,
            RCC_AHB2ENR => self.ahb2enr = val,
            RCC_AHB3ENR => self.ahb3enr = val,
            RCC_APB1ENR => self.apb1enr = val,
            RCC_APB2ENR => self.apb2enr = val,
            RCC_PLLSAICFGR => self.pllsaicfgr = val,
            RCC_PLLI2SCFGR => self.plli2scfgr = val,
            RCC_DCKCFGR => self.dckcfgr = val,
            RCC_DCKCFGR2 => self.dckcfgr2 = val,
            _ => {}
        }
    }
}
