use crate::config::*;

#[derive(Debug, Clone, Copy)]
pub struct GpioPort {
    pub moder: u32,
    pub otyper: u32,
    pub ospeedr: u32,
    pub pupdr: u32,
    pub idr: u32,
    pub odr: u32,
    pub lckr: u32,
    pub afrl: u32,
    pub afrh: u32,
}

impl Default for GpioPort {
    fn default() -> Self {
        Self {
            moder: 0,
            otyper: 0,
            ospeedr: 0,
            pupdr: 0,
            idr: 0,
            odr: 0,
            lckr: 0,
            afrl: 0,
            afrh: 0,
        }
    }
}

#[derive(Debug)]
pub struct GpioPeripheral {
    pub ports: [GpioPort; 8],
    pub pin_pd4_lp: bool,
    pub pin_pd5_hp: bool,
    pub pin_pa15_bp: bool,
    pub filter_mode: SemFilterMode,
}

impl Default for GpioPeripheral {
    fn default() -> Self {
        let mut ports = [GpioPort::default(); 8];
        ports[2].idr = 1 << 4; // PC4 (LPF switch input) is high by default on startup
        ports[2].odr = 1 << 8; // PC8 (LPF LED) is high by default on startup
        ports[3].odr = 1 << SEM_LP_PIN; // PD4 (LP) is high by default on startup
        Self {
            ports,
            pin_pd4_lp: true,
            pin_pd5_hp: false,
            pin_pa15_bp: false,
            filter_mode: SemFilterMode::LowPass,
        }
    }
}

impl GpioPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self, addr: u32) -> u32 {
        if addr < GPIOA_BASE {
            return 0;
        }
        let port_idx = ((addr - GPIOA_BASE) / 0x400) as usize;
        if port_idx >= self.ports.len() {
            return 0;
        }
        let offset = (addr - GPIOA_BASE) % 0x400;
        let port = &self.ports[port_idx];
        match offset {
            GPIO_MODER_OFFSET => port.moder,
            GPIO_OTYPER_OFFSET => port.otyper,
            GPIO_OSPEEDR_OFFSET => port.ospeedr,
            GPIO_PUPDR_OFFSET => port.pupdr,
            GPIO_IDR_OFFSET => port.idr | port.odr,
            GPIO_ODR_OFFSET => port.odr,
            GPIO_BSRR_OFFSET => 0,
            0x1C => port.lckr,
            GPIO_AFRL_OFFSET => port.afrl,
            GPIO_AFRH_OFFSET => port.afrh,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        if addr < GPIOA_BASE {
            return;
        }
        let port_idx = ((addr - GPIOA_BASE) / 0x400) as usize;
        if port_idx >= self.ports.len() {
            return;
        }
        let offset = (addr - GPIOA_BASE) % 0x400;
        match offset {
            GPIO_MODER_OFFSET => self.ports[port_idx].moder = val,
            GPIO_OTYPER_OFFSET => self.ports[port_idx].otyper = val,
            GPIO_OSPEEDR_OFFSET => self.ports[port_idx].ospeedr = val,
            GPIO_PUPDR_OFFSET => self.ports[port_idx].pupdr = val,
            GPIO_IDR_OFFSET => self.ports[port_idx].idr = val,
            GPIO_ODR_OFFSET => {
                self.ports[port_idx].odr = val;
                self.sync_filter_pins(port_idx);
            }
            GPIO_BSRR_OFFSET => {
                let set = val & 0xFFFF;
                let reset = (val >> 16) & 0xFFFF;
                self.ports[port_idx].odr = (self.ports[port_idx].odr | set) & !reset;
                self.sync_filter_pins(port_idx);
            }
            0x1C => self.ports[port_idx].lckr = val,
            GPIO_AFRL_OFFSET => self.ports[port_idx].afrl = val,
            GPIO_AFRH_OFFSET => self.ports[port_idx].afrh = val,
            _ => {}
        }
    }

    fn sync_filter_pins(&mut self, port_idx: usize) {
        if port_idx == 0 {
            self.pin_pa15_bp = (self.ports[0].odr & (1 << SEM_BP_PIN)) != 0;
            self.update_filter_mode();
        } else if port_idx == 3 {
            self.pin_pd4_lp = (self.ports[3].odr & (1 << SEM_LP_PIN)) != 0;
            self.pin_pd5_hp = (self.ports[3].odr & (1 << SEM_HP_PIN)) != 0;
            self.update_filter_mode();
        }
    }

    pub fn update_filter_mode(&mut self) -> SemFilterMode {
        let mode = if self.pin_pd4_lp && self.pin_pd5_hp {
            SemFilterMode::Notch
        } else if self.pin_pd4_lp {
            SemFilterMode::LowPass
        } else if self.pin_pa15_bp {
            SemFilterMode::BandPass
        } else if self.pin_pd5_hp {
            SemFilterMode::HighPass
        } else {
            SemFilterMode::LowPass
        };
        self.filter_mode = mode;
        mode
    }

    pub fn write_gpioa_bsrr(&mut self, val: u32) {
        self.write(GPIOA_BASE + GPIO_BSRR_OFFSET, val);
    }

    pub fn write_gpioa_odr(&mut self, val: u32) {
        self.write(GPIOA_BASE + GPIO_ODR_OFFSET, val);
    }

    pub fn write_gpiod_bsrr(&mut self, val: u32) {
        self.write(GPIOD_BASE + GPIO_BSRR_OFFSET, val);
    }

    pub fn write_gpiod_odr(&mut self, val: u32) {
        self.write(GPIOD_BASE + GPIO_ODR_OFFSET, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpio_read_back_and_modify() {
        let mut gpio = GpioPeripheral::new();

        // Check startup state: PD4 is high
        assert_eq!(
            gpio.read(GPIOD_BASE + GPIO_ODR_OFFSET) & (1 << SEM_LP_PIN),
            1 << SEM_LP_PIN
        );

        // Read-modify-write: Set PD5 (HP) without resetting PD4 (LP)
        let odr = gpio.read(GPIOD_BASE + GPIO_ODR_OFFSET);
        gpio.write(GPIOD_BASE + GPIO_ODR_OFFSET, odr | (1 << SEM_HP_PIN));

        assert_eq!(
            gpio.read(GPIOD_BASE + GPIO_ODR_OFFSET) & (1 << SEM_LP_PIN),
            1 << SEM_LP_PIN
        );
        assert_eq!(
            gpio.read(GPIOD_BASE + GPIO_ODR_OFFSET) & (1 << SEM_HP_PIN),
            1 << SEM_HP_PIN
        );
        assert_eq!(gpio.filter_mode, SemFilterMode::Notch);

        // Configure MODER, OSPEEDR, AFRL
        gpio.write(GPIOA_BASE + GPIO_MODER_OFFSET, 0x5555_AAAA);
        assert_eq!(gpio.read(GPIOA_BASE + GPIO_MODER_OFFSET), 0x5555_AAAA);
    }
}
