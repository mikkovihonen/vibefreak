//! STM32F446 EXTI and SYSCFG peripheral emulation.

pub const SYSCFG_BASE: u32 = 0x4001_3800;
pub const EXTI_BASE: u32 = 0x4001_3C00;

#[derive(Debug, Default)]
pub struct ExtiSyscfgPeripheral {
    // SYSCFG
    pub memrmp: u32,
    pub pmc: u32,
    pub exticr: [u32; 4],
    pub cmpcr: u32,
    pub cfgr: u32,

    // EXTI
    pub imr: u32,
    pub emr: u32,
    pub rtsr: u32,
    pub ftsr: u32,
    pub swier: u32,
    pub pr: u32,
}

impl ExtiSyscfgPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self, addr: u32) -> u32 {
        match addr {
            // SYSCFG
            0x4001_3800 => self.memrmp,
            0x4001_3804 => self.pmc,
            0x4001_3808 => self.exticr[0],
            0x4001_380C => self.exticr[1],
            0x4001_3810 => self.exticr[2],
            0x4001_3814 => self.exticr[3],
            0x4001_3820 => self.cmpcr,
            0x4001_382C => self.cfgr,

            // EXTI
            0x4001_3C00 => self.imr,
            0x4001_3C04 => self.emr,
            0x4001_3C08 => self.rtsr,
            0x4001_3C0C => self.ftsr,
            0x4001_3C10 => self.swier,
            0x4001_3C14 => self.pr,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        match addr {
            // SYSCFG
            0x4001_3800 => self.memrmp = val,
            0x4001_3804 => self.pmc = val,
            0x4001_3808 => self.exticr[0] = val,
            0x4001_380C => self.exticr[1] = val,
            0x4001_3810 => self.exticr[2] = val,
            0x4001_3814 => self.exticr[3] = val,
            0x4001_3820 => self.cmpcr = val,
            0x4001_382C => self.cfgr = val,

            // EXTI
            0x4001_3C00 => self.imr = val,
            0x4001_3C04 => self.emr = val,
            0x4001_3C08 => self.rtsr = val,
            0x4001_3C0C => self.ftsr = val,
            0x4001_3C10 => self.swier = val,
            0x4001_3C14 => self.pr &= !val, // rc_w1: write 1 to clear pending bit
            _ => {}
        }
    }
}

