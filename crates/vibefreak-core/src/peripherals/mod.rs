pub mod adc;
pub mod crc;
pub mod dma;
pub mod exti;
pub mod flash;
pub mod gpio;
pub mod i2c;
pub mod pwr;
pub mod qspi;
pub mod rcc;
pub mod sai;
pub mod spi_dac;
pub mod timer;
pub mod tlv320aic3104;
pub mod usart;
pub mod usb;

pub use self::tlv320aic3104::{AudioInputSource, HeadsetJackState, Tlv320aic3104};

use std::collections::HashMap;

use self::adc::AdcPeripheral;
use self::crc::CrcPeripheral;
use self::dma::DmaController;
use self::exti::ExtiSyscfgPeripheral;
use self::flash::FlashPeripheral;
use self::gpio::GpioPeripheral;
use self::i2c::I2cPeripheral;
use self::pwr::PwrPeripheral;
use self::qspi::QspiPeripheral;
use self::rcc::RccPeripheral;
use self::sai::SaiPeripheral;
use self::spi_dac::{DacPeripheral, SpiBlock};
use self::timer::TimerManager;
use self::usart::UsartPeripheral;
use self::usb::UsbOtgFsPeripheral;
use crate::config::*;

#[derive(Debug)]
pub struct MmioBus {
    pub rcc: RccPeripheral,
    pub pwr: PwrPeripheral,
    pub gpio: GpioPeripheral,
    pub i2c1: I2cPeripheral,
    pub i2c3: I2cPeripheral,
    pub dma1: DmaController,
    pub dma2: DmaController,
    pub qspi: QspiPeripheral,
    pub usart1: UsartPeripheral,
    pub usart2: UsartPeripheral,
    pub usart3: UsartPeripheral,
    pub crc: CrcPeripheral,
    pub flash: FlashPeripheral,
    pub timers: TimerManager,
    pub exti: ExtiSyscfgPeripheral,
    pub sai1: SaiPeripheral,
    pub sai2: SaiPeripheral,
    pub adc: AdcPeripheral,
    pub spi2: SpiBlock,
    pub spi3: SpiBlock,
    pub dac: DacPeripheral,
    pub usb: UsbOtgFsPeripheral,
    pub fallback_regs: HashMap<u32, u32>,
}

impl Default for MmioBus {
    fn default() -> Self {
        Self::new()
    }
}

impl MmioBus {
    pub fn new() -> Self {
        Self {
            rcc: RccPeripheral::new(),
            pwr: PwrPeripheral::new(),
            gpio: GpioPeripheral::new(),
            i2c1: I2cPeripheral::new(),
            i2c3: I2cPeripheral::with_base(I2C3_BASE),
            dma1: DmaController::new(DMA1_BASE),
            dma2: DmaController::new(DMA2_BASE),
            qspi: QspiPeripheral::new(),
            usart1: UsartPeripheral::new(),
            usart2: UsartPeripheral::new(),
            usart3: UsartPeripheral::new(),
            crc: CrcPeripheral::new(),
            flash: FlashPeripheral::new(),
            timers: TimerManager::new(),
            exti: ExtiSyscfgPeripheral::new(),
            sai1: SaiPeripheral::new(),
            sai2: SaiPeripheral::new(),
            adc: AdcPeripheral::new(),
            spi2: SpiBlock::default(),
            spi3: SpiBlock::default(),
            dac: DacPeripheral::default(),
            usb: UsbOtgFsPeripheral::new(),
            fallback_regs: HashMap::new(),
        }
    }

    pub fn read(&mut self, addr: u32, size: usize) -> u32 {
        match addr {
            // RCC (0x4002_3800..=0x4002_38FF)
            a if (RCC_CR..=0x4002_38FF).contains(&a) => self.rcc.read(addr),

            // PWR
            PWR_CR | PWR_CSR => self.pwr.read(addr),

            // GPIO (GPIOA..GPIOH: 0x4002_0000..=0x4002_1FFF)
            a if (GPIOA_BASE..=0x4002_1FFF).contains(&a) => self.gpio.read(addr),

            // I2C1 (0x4000_5400..=0x4000_543F)
            a if (I2C1_BASE..=I2C1_BASE + 0x3F).contains(&a) => self.i2c1.read(addr),

            // I2C3 (0x4000_5C00..=0x4000_5C3F)
            a if (I2C3_BASE..=I2C3_BASE + 0x3F).contains(&a) => self.i2c3.read(addr),

            // DMA1
            a if (DMA1_BASE..=DMA1_BASE + 0xD0).contains(&a) => self.dma1.read(addr),

            // DMA2
            a if (DMA2_BASE..=DMA2_BASE + 0xD0).contains(&a) => self.dma2.read(addr),

            // USART1
            USART1_SR => self.usart1.read_sr(),
            USART1_DR => self.usart1.read_dr(),
            USART1_BRR => self.usart1.brr,
            USART1_CR1 => self.usart1.cr1,
            USART1_CR2 => self.usart1.cr2,
            USART1_CR3 => self.usart1.cr3,

            // USART2
            USART2_SR => self.usart2.read_sr(),
            USART2_DR => self.usart2.read_dr(),
            USART2_BRR => self.usart2.brr,
            USART2_CR1 => self.usart2.cr1,
            USART2_CR2 => self.usart2.cr2,
            USART2_CR3 => self.usart2.cr3,

            // USART3
            USART3_SR => self.usart3.read_sr(),
            USART3_DR => self.usart3.read_dr(),
            USART3_BRR => self.usart3.brr,
            USART3_CR1 => self.usart3.cr1,
            USART3_CR2 => self.usart3.cr2,
            USART3_CR3 => self.usart3.cr3,

            // QUADSPI
            QUADSPI_CR..=QUADSPI_LPTR => self.qspi.read(addr, size),

            // CRC
            CRC_DR..=CRC_CR => self.crc.read(addr),

            // FLASH
            FLASH_ACR..=FLASH_OPTCR => self.flash.read(addr),

            // EXTI & SYSCFG (0x4001_3800..=0x4001_3FFF)
            a if (0x4001_3800..=0x4001_3FFF).contains(&a) => self.exti.read(addr),

            // SAI1 (0x4001_5800..=0x4001_58FF)
            a if (0x4001_5800..=0x4001_58FF).contains(&a) => self.sai1.read(addr - 0x4001_5800),

            // SAI2 (0x4001_5C00..=0x4001_5CFF)
            a if (0x4001_5C00..=0x4001_5CFF).contains(&a) => self.sai2.read(addr - 0x4001_5C00),

            // ADC1 & Common (0x4001_2000..=0x4001_23FF)
            a if (0x4001_2000..=0x4001_23FF).contains(&a) => self.adc.read(addr),

            // SPI2 (0x4000_3800..=0x4000_383F)
            a if (0x4000_3800..=0x4000_383F).contains(&a) => self.spi2.read(addr - 0x4000_3800),

            // SPI3 (0x4000_4000..=0x4000_403F)
            a if (0x4000_4000..=0x4000_403F).contains(&a) => self.spi3.read(addr - 0x4000_4000),

            // DAC (0x4000_7400..=0x4000_743F)
            a if (0x4000_7400..=0x4000_743F).contains(&a) => self.dac.read(addr - 0x4000_7400),

            // Timers (TIM1..TIM14)
            _ if self.timers.resolve_mut(addr).is_some() => {
                self.timers.read(addr).unwrap_or(0)
            }

            // USB OTG FS (0x5000_0000..=0x5000_FFFF)
            a if (USB_OTG_FS_BASE as u32..=(USB_OTG_FS_BASE + USB_OTG_FS_SIZE as u64 - 1) as u32).contains(&a) => {
                self.usb.read(addr, size)
            }

            // Fallback unhandled
            _ => {
                crate::svd_diagnostics::log_unhandled_mmio_read(addr, size);
                let aligned = addr & !3;
                if let Some(&val) = self.fallback_regs.get(&aligned) {
                    val
                } else {
                    crate::svd_diagnostics::lookup_svd_register(aligned)
                        .map(|r| r.reset_value)
                        .unwrap_or(0)
                }
            }
        }
    }

    pub fn write(&mut self, addr: u32, size: usize, val: u32) {
        match addr {
            // RCC
            a if (RCC_CR..=0x4002_38FF).contains(&a) => self.rcc.write(addr, val),

            // PWR
            PWR_CR | PWR_CSR => self.pwr.write(addr, val),

            // GPIO (GPIOA..GPIOH: 0x4002_0000..=0x4002_1FFF)
            a if (GPIOA_BASE..=0x4002_1FFF).contains(&a) => self.gpio.write(addr, val),

            // I2C1
            a if (I2C1_BASE..=I2C1_BASE + 0x3F).contains(&a) => self.i2c1.write(addr, val),

            // I2C3
            a if (I2C3_BASE..=I2C3_BASE + 0x3F).contains(&a) => self.i2c3.write(addr, val),

            // DMA1
            a if (DMA1_BASE..=DMA1_BASE + 0xD0).contains(&a) => self.dma1.write(addr, val),

            // DMA2
            a if (DMA2_BASE..=DMA2_BASE + 0xD0).contains(&a) => self.dma2.write(addr, val),

            // USART1
            USART1_DR => self.usart1.write_dr(val),
            USART1_BRR => self.usart1.brr = val,
            USART1_CR1 => self.usart1.write_cr1(val),
            USART1_CR2 => self.usart1.cr2 = val,
            USART1_CR3 => self.usart1.cr3 = val,

            // USART2
            USART2_DR => self.usart2.write_dr(val),
            USART2_BRR => self.usart2.brr = val,
            USART2_CR1 => self.usart2.write_cr1(val),
            USART2_CR2 => self.usart2.cr2 = val,
            USART2_CR3 => self.usart2.cr3 = val,

            // USART3
            USART3_DR => self.usart3.write_dr(val),
            USART3_BRR => self.usart3.brr = val,
            USART3_CR1 => self.usart3.write_cr1(val),
            USART3_CR2 => self.usart3.cr2 = val,
            USART3_CR3 => self.usart3.cr3 = val,

            // QUADSPI
            QUADSPI_CR..=QUADSPI_LPTR => self.qspi.write(addr, size, val),

            // CRC
            CRC_DR..=CRC_CR => self.crc.write(addr, val),

            // FLASH
            FLASH_ACR..=FLASH_OPTCR => self.flash.write(addr, val),

            // EXTI & SYSCFG (0x4001_3800..=0x4001_3FFF)
            a if (0x4001_3800..=0x4001_3FFF).contains(&a) => self.exti.write(addr, val),

            // SAI1 (0x4001_5800..=0x4001_58FF)
            a if (0x4001_5800..=0x4001_58FF).contains(&a) => self.sai1.write(addr - 0x4001_5800, val),

            // SAI2 (0x4001_5C00..=0x4001_5CFF)
            a if (0x4001_5C00..=0x4001_5CFF).contains(&a) => self.sai2.write(addr - 0x4001_5C00, val),

            // ADC1 & Common (0x4001_2000..=0x4001_23FF)
            a if (0x4001_2000..=0x4001_23FF).contains(&a) => self.adc.write(addr, val),

            // SPI2 (0x4000_3800..=0x4000_383F)
            a if (0x4000_3800..=0x4000_383F).contains(&a) => self.spi2.write(addr - 0x4000_3800, val),

            // SPI3 (0x4000_4000..=0x4000_403F)
            a if (0x4000_4000..=0x4000_403F).contains(&a) => self.spi3.write(addr - 0x4000_4000, val),

            // DAC (0x4000_7400..=0x4000_743F)
            a if (0x4000_7400..=0x4000_743F).contains(&a) => self.dac.write(addr - 0x4000_7400, val),

            // Timers (TIM1..TIM14)
            _ if self.timers.write(addr, val) => {}

            // USB OTG FS (0x5000_0000..=0x5000_FFFF)
            a if (USB_OTG_FS_BASE as u32..=(USB_OTG_FS_BASE + USB_OTG_FS_SIZE as u64 - 1) as u32).contains(&a) => {
                self.usb.write(addr, size, val);
            }

            _ => {
                crate::svd_diagnostics::log_unhandled_mmio_write(addr, size, val);
                self.fallback_regs.insert(addr & !3, val);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmio_bus_unmodeled_register_svd_reset_value() {
        let mut bus = MmioBus::new();

        // SPI1_SR is not directly mapped on APB1 SPI2/3 bus, so it falls through to SVD lookup
        // SVD reset value for SPI1_SR (0x4001_3008) is 0x0000_0002 (TXE = 1)
        let val = bus.read(0x4001_3008, 4);
        assert_eq!(val, 0x0000_0002);

        // SCB_CPUID (0xE000_ED00) reset value is 0x410F_C241 (Cortex-M4 r0p1)
        let cpuid = bus.read(0xE000_ED00, 4);
        assert_eq!(cpuid, 0x410F_C241);

        // Entirely unmodeled/unknown address returns 0
        let unknown = bus.read(0x1234_5678, 4);
        assert_eq!(unknown, 0);

        // Writing to unmodeled address persists via read-after-write fallback storage
        bus.write(0x4000_7010, 4, 0x1234);
        assert_eq!(bus.read(0x4000_7010, 4), 0x1234);
    }

    #[test]
    fn test_mmio_bus_timer_and_peripheral_routing() {
        let mut bus = MmioBus::new();

        // 1. Timer routing (TIM3 PSC, ARR, CR1)
        bus.write(0x4000_0428, 4, 84); // TIM3 PSC
        bus.write(0x4000_042C, 4, 1000); // TIM3 ARR
        bus.write(0x4000_0400, 4, 1); // TIM3 CR1 (CEN = 1)
        assert_eq!(bus.read(0x4000_0428, 4), 84);
        assert_eq!(bus.read(0x4000_042C, 4), 1000);
        assert_eq!(bus.read(0x4000_0400, 4), 1);

        // 2. EXTI & SYSCFG routing
        bus.write(0x4001_3814, 4, 0x0020); // SYSCFG_EXTICR4
        bus.write(0x4001_3C00, 4, 0x2000); // EXTI_IMR
        assert_eq!(bus.read(0x4001_3814, 4), 0x0020);
        assert_eq!(bus.read(0x4001_3C00, 4), 0x2000);

        // 3. Extended RCC routing (PLLSAICFGR)
        assert_eq!(bus.read(0x4002_3884, 4), 0x2400_3010); // SVD reset value
        bus.write(0x4002_3884, 4, 0x2200_5607);
        assert_eq!(bus.read(0x4002_3884, 4), 0x2200_5607);

        // 4. USART CR2 / CR3 routing
        bus.write(USART1_CR2, 4, 0x2000);
        bus.write(USART1_CR3, 4, 0x0080);
        assert_eq!(bus.read(USART1_CR2, 4), 0x2000);
        assert_eq!(bus.read(USART1_CR3, 4), 0x0080);

        // 5. SAI1 / SAI2 routing
        bus.write(0x4001_5C04, 4, 0x0140); // SAI2_Block_A CR1
        assert_eq!(bus.read(0x4001_5C04, 4), 0x0140);

        // 6. ADC1 routing
        bus.write(0x4001_2004, 4, 0x0100); // ADC1 CR1
        assert_eq!(bus.read(0x4001_2004, 4), 0x0100);

        // 7. I2C3 routing
        bus.write(I2C3_CR1, 4, 0x0001);
        bus.write(I2C3_TRISE, 4, 0x00E1);
        assert_eq!(bus.read(I2C3_CR1, 4), 0x0001);
        assert_eq!(bus.read(I2C3_TRISE, 4), 0x00E1);
    }
}
