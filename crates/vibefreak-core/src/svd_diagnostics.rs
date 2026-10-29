use std::collections::HashSet;
use std::sync::Mutex;

/// Metadata describing an STM32F446 peripheral register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SvdRegisterInfo {
    pub peripheral_name: &'static str,
    pub register_name: &'static str,
    pub reset_value: u32,
}

/// Static table of STM32F446 peripheral registers and their architectural reset values.
pub static SVD_REGISTERS: &[(u32, SvdRegisterInfo)] = &[
    // --- TIM2 (0x40000000) ---
    (0x4000_0000, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_0004, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_0008, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "SMCR", reset_value: 0x0000_0000 }),
    (0x4000_000C, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4000_0010, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4000_0014, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4000_0018, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "CCMR1", reset_value: 0x0000_0000 }),
    (0x4000_001C, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "CCMR2", reset_value: 0x0000_0000 }),
    (0x4000_0020, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "CCER", reset_value: 0x0000_0000 }),
    (0x4000_0024, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4000_0028, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4000_002C, SvdRegisterInfo { peripheral_name: "TIM2", register_name: "ARR", reset_value: 0x0000_0000 }),
    // --- TIM3 (0x40000400) ---
    (0x4000_0400, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_0404, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_0408, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "SMCR", reset_value: 0x0000_0000 }),
    (0x4000_040C, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4000_0410, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4000_0414, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4000_0418, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CCMR1", reset_value: 0x0000_0000 }),
    (0x4000_041C, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CCMR2", reset_value: 0x0000_0000 }),
    (0x4000_0420, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CCER", reset_value: 0x0000_0000 }),
    (0x4000_0424, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4000_0428, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4000_042C, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "ARR", reset_value: 0x0000_0000 }),
    (0x4000_0434, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CCR1", reset_value: 0x0000_0000 }),
    (0x4000_0438, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CCR2", reset_value: 0x0000_0000 }),
    (0x4000_043C, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CCR3", reset_value: 0x0000_0000 }),
    (0x4000_0440, SvdRegisterInfo { peripheral_name: "TIM3", register_name: "CCR4", reset_value: 0x0000_0000 }),
    // --- TIM5 (0x40000C00) ---
    (0x4000_0C00, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_0C04, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_0C08, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "SMCR", reset_value: 0x0000_0000 }),
    (0x4000_0C0C, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4000_0C10, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4000_0C14, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4000_0C20, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "CCER", reset_value: 0x0000_0000 }),
    (0x4000_0C24, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4000_0C28, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4000_0C2C, SvdRegisterInfo { peripheral_name: "TIM5", register_name: "ARR", reset_value: 0x0000_0000 }),
    // --- TIM6 (0x40001000) ---
    (0x4000_1000, SvdRegisterInfo { peripheral_name: "TIM6", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_1004, SvdRegisterInfo { peripheral_name: "TIM6", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_100C, SvdRegisterInfo { peripheral_name: "TIM6", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4000_1010, SvdRegisterInfo { peripheral_name: "TIM6", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4000_1014, SvdRegisterInfo { peripheral_name: "TIM6", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4000_1024, SvdRegisterInfo { peripheral_name: "TIM6", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4000_1028, SvdRegisterInfo { peripheral_name: "TIM6", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4000_102C, SvdRegisterInfo { peripheral_name: "TIM6", register_name: "ARR", reset_value: 0x0000_0000 }),
    // --- TIM7 (0x40001400) ---
    (0x4000_1400, SvdRegisterInfo { peripheral_name: "TIM7", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_1410, SvdRegisterInfo { peripheral_name: "TIM7", register_name: "SR", reset_value: 0x0000_0000 }),
    // --- TIM12 (0x40001800) ---
    (0x4000_1800, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_1804, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_1808, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "SMCR", reset_value: 0x0000_0000 }),
    (0x4000_180C, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4000_1810, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4000_1814, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4000_1818, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "CCMR1", reset_value: 0x0000_0000 }),
    (0x4000_1820, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "CCER", reset_value: 0x0000_0000 }),
    (0x4000_1824, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4000_1828, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4000_182C, SvdRegisterInfo { peripheral_name: "TIM12", register_name: "ARR", reset_value: 0x0000_0000 }),
    // --- TIM13 (0x40001C00) ---
    (0x4000_1C00, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_1C08, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4000_1C10, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4000_1C14, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4000_1C18, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "CCMR1", reset_value: 0x0000_0000 }),
    (0x4000_1C20, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "CCER", reset_value: 0x0000_0000 }),
    (0x4000_1C24, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4000_1C28, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4000_1C2C, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "ARR", reset_value: 0x0000_0000 }),
    (0x4000_1C34, SvdRegisterInfo { peripheral_name: "TIM13", register_name: "CCR1", reset_value: 0x0000_0000 }),
    // --- TIM14 (0x40002000) ---
    (0x4000_2000, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_2004, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_200C, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4000_2010, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4000_2014, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4000_2018, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "CCMR1", reset_value: 0x0000_0000 }),
    (0x4000_2020, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "CCER", reset_value: 0x0000_0000 }),
    (0x4000_2024, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4000_2028, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4000_202C, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "ARR", reset_value: 0x0000_0000 }),
    (0x4000_2034, SvdRegisterInfo { peripheral_name: "TIM14", register_name: "CCR1", reset_value: 0x0000_0000 }),
    // --- SPI2 (0x40003800) ---
    (0x4000_3800, SvdRegisterInfo { peripheral_name: "SPI2", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_3804, SvdRegisterInfo { peripheral_name: "SPI2", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_3808, SvdRegisterInfo { peripheral_name: "SPI2", register_name: "SR", reset_value: 0x0000_0002 }),
    (0x4000_380C, SvdRegisterInfo { peripheral_name: "SPI2", register_name: "DR", reset_value: 0x0000_0000 }),
    (0x4000_381C, SvdRegisterInfo { peripheral_name: "SPI2", register_name: "I2SCFGR", reset_value: 0x0000_0000 }),
    // --- SPI3 (0x40004000) ---
    (0x4000_4000, SvdRegisterInfo { peripheral_name: "SPI3", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_4004, SvdRegisterInfo { peripheral_name: "SPI3", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_4008, SvdRegisterInfo { peripheral_name: "SPI3", register_name: "SR", reset_value: 0x0000_0002 }),
    (0x4000_400C, SvdRegisterInfo { peripheral_name: "SPI3", register_name: "DR", reset_value: 0x0000_0000 }),
    // --- USART2 (0x40004400) ---
    (0x4000_4400, SvdRegisterInfo { peripheral_name: "USART2", register_name: "SR", reset_value: 0x00C0_0000 }),
    (0x4000_4404, SvdRegisterInfo { peripheral_name: "USART2", register_name: "DR", reset_value: 0x0000_0000 }),
    (0x4000_4408, SvdRegisterInfo { peripheral_name: "USART2", register_name: "BRR", reset_value: 0x0000_0000 }),
    (0x4000_440C, SvdRegisterInfo { peripheral_name: "USART2", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_4410, SvdRegisterInfo { peripheral_name: "USART2", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_4414, SvdRegisterInfo { peripheral_name: "USART2", register_name: "CR3", reset_value: 0x0000_0000 }),
    // --- USART3 (0x40004800) ---
    (0x4000_4800, SvdRegisterInfo { peripheral_name: "USART3", register_name: "SR", reset_value: 0x00C0_0000 }),
    (0x4000_4804, SvdRegisterInfo { peripheral_name: "USART3", register_name: "DR", reset_value: 0x0000_0000 }),
    (0x4000_4808, SvdRegisterInfo { peripheral_name: "USART3", register_name: "BRR", reset_value: 0x0000_0000 }),
    (0x4000_480C, SvdRegisterInfo { peripheral_name: "USART3", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_4810, SvdRegisterInfo { peripheral_name: "USART3", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_4814, SvdRegisterInfo { peripheral_name: "USART3", register_name: "CR3", reset_value: 0x0000_0000 }),
    // --- I2C1 (0x40005400) ---
    (0x4000_5400, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_5404, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_5408, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "OAR1", reset_value: 0x0000_0000 }),
    (0x4000_540C, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "OAR2", reset_value: 0x0000_0000 }),
    (0x4000_5410, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "DR", reset_value: 0x0000_0000 }),
    (0x4000_5414, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "SR1", reset_value: 0x0000_0000 }),
    (0x4000_5418, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "SR2", reset_value: 0x0000_0000 }),
    (0x4000_541C, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "TRISE", reset_value: 0x0000_0002 }),
    (0x4000_5420, SvdRegisterInfo { peripheral_name: "I2C1", register_name: "FLTR", reset_value: 0x0000_0000 }),
    // --- I2C3 (0x40005C00) ---
    (0x4000_5C00, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4000_5C04, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4000_5C08, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "OAR1", reset_value: 0x0000_0000 }),
    (0x4000_5C0C, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "OAR2", reset_value: 0x0000_0000 }),
    (0x4000_5C10, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "DR", reset_value: 0x0000_0000 }),
    (0x4000_5C14, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "SR1", reset_value: 0x0000_0000 }),
    (0x4000_5C18, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "SR2", reset_value: 0x0000_0000 }),
    (0x4000_5C1C, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "TRISE", reset_value: 0x0000_0002 }),
    (0x4000_5C20, SvdRegisterInfo { peripheral_name: "I2C3", register_name: "FLTR", reset_value: 0x0000_0000 }),
    // --- PWR (0x40007000) ---
    (0x4000_7000, SvdRegisterInfo { peripheral_name: "PWR", register_name: "CR", reset_value: 0x0000_C000 }),
    (0x4000_7004, SvdRegisterInfo { peripheral_name: "PWR", register_name: "CSR", reset_value: 0x0000_0000 }),
    // --- DAC (0x40007400) ---
    (0x4000_7400, SvdRegisterInfo { peripheral_name: "DAC", register_name: "CR", reset_value: 0x0000_0000 }),
    (0x4000_7404, SvdRegisterInfo { peripheral_name: "DAC", register_name: "SWTRIGR", reset_value: 0x0000_0000 }),
    (0x4000_7408, SvdRegisterInfo { peripheral_name: "DAC", register_name: "DHR12R1", reset_value: 0x0000_0000 }),
    (0x4000_742C, SvdRegisterInfo { peripheral_name: "DAC", register_name: "DOR1", reset_value: 0x0000_0000 }),
    (0x4000_7434, SvdRegisterInfo { peripheral_name: "DAC", register_name: "SR", reset_value: 0x0000_0000 }),
    // --- TIM1 (0x40010000) ---
    (0x4001_0000, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4001_0004, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4001_0008, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "SMCR", reset_value: 0x0000_0000 }),
    (0x4001_000C, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4001_0010, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4001_0014, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4001_0018, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "CCMR1", reset_value: 0x0000_0000 }),
    (0x4001_001C, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "CCMR2", reset_value: 0x0000_0000 }),
    (0x4001_0020, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "CCER", reset_value: 0x0000_0000 }),
    (0x4001_0024, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4001_0028, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4001_002C, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "ARR", reset_value: 0x0000_0000 }),
    (0x4001_0030, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "RCR", reset_value: 0x0000_0000 }),
    (0x4001_0044, SvdRegisterInfo { peripheral_name: "TIM1", register_name: "BDTR", reset_value: 0x0000_0000 }),
    // --- TIM8 (0x40010400) ---
    (0x4001_0400, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4001_0404, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4001_0408, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "SMCR", reset_value: 0x0000_0000 }),
    (0x4001_040C, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "DIER", reset_value: 0x0000_0000 }),
    (0x4001_0410, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4001_0414, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "EGR", reset_value: 0x0000_0000 }),
    (0x4001_0418, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CCMR1", reset_value: 0x0000_0000 }),
    (0x4001_041C, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CCMR2", reset_value: 0x0000_0000 }),
    (0x4001_0420, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CCER", reset_value: 0x0000_0000 }),
    (0x4001_0424, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CNT", reset_value: 0x0000_0000 }),
    (0x4001_0428, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "PSC", reset_value: 0x0000_0000 }),
    (0x4001_042C, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "ARR", reset_value: 0x0000_0000 }),
    (0x4001_0430, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "RCR", reset_value: 0x0000_0000 }),
    (0x4001_0434, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CCR1", reset_value: 0x0000_0000 }),
    (0x4001_0438, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CCR2", reset_value: 0x0000_0000 }),
    (0x4001_043C, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CCR3", reset_value: 0x0000_0000 }),
    (0x4001_0440, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "CCR4", reset_value: 0x0000_0000 }),
    (0x4001_0444, SvdRegisterInfo { peripheral_name: "TIM8", register_name: "BDTR", reset_value: 0x0000_0000 }),
    // --- USART1 (0x40011000) ---
    (0x4001_1000, SvdRegisterInfo { peripheral_name: "USART1", register_name: "SR", reset_value: 0x00C0_0000 }),
    (0x4001_1004, SvdRegisterInfo { peripheral_name: "USART1", register_name: "DR", reset_value: 0x0000_0000 }),
    (0x4001_1008, SvdRegisterInfo { peripheral_name: "USART1", register_name: "BRR", reset_value: 0x0000_0000 }),
    (0x4001_100C, SvdRegisterInfo { peripheral_name: "USART1", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4001_1010, SvdRegisterInfo { peripheral_name: "USART1", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4001_1014, SvdRegisterInfo { peripheral_name: "USART1", register_name: "CR3", reset_value: 0x0000_0000 }),
    // --- ADC1 (0x40012000) ---
    (0x4001_2000, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4001_2004, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4001_2008, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4001_200C, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "SMPR1", reset_value: 0x0000_0000 }),
    (0x4001_2010, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "SMPR2", reset_value: 0x0000_0000 }),
    (0x4001_202C, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "SQR1", reset_value: 0x0000_0000 }),
    (0x4001_2030, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "SQR2", reset_value: 0x0000_0000 }),
    (0x4001_2034, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "SQR3", reset_value: 0x0000_0000 }),
    (0x4001_204C, SvdRegisterInfo { peripheral_name: "ADC1", register_name: "DR", reset_value: 0x0000_0000 }),
    // --- ADC_Common (0x40012300) ---
    (0x4001_2300, SvdRegisterInfo { peripheral_name: "ADC_Common", register_name: "CSR", reset_value: 0x0000_0000 }),
    (0x4001_2304, SvdRegisterInfo { peripheral_name: "ADC_Common", register_name: "CCR", reset_value: 0x0000_0000 }),
    // --- SPI1 (0x40013000) ---
    (0x4001_3000, SvdRegisterInfo { peripheral_name: "SPI1", register_name: "CR1", reset_value: 0x0000_0000 }),
    (0x4001_3004, SvdRegisterInfo { peripheral_name: "SPI1", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4001_3008, SvdRegisterInfo { peripheral_name: "SPI1", register_name: "SR", reset_value: 0x0000_0002 }),
    (0x4001_300C, SvdRegisterInfo { peripheral_name: "SPI1", register_name: "DR", reset_value: 0x0000_0000 }),
    // --- SYSCFG (0x40013800) ---
    (0x4001_3800, SvdRegisterInfo { peripheral_name: "SYSCFG", register_name: "MEMRMP", reset_value: 0x0000_0000 }),
    (0x4001_3804, SvdRegisterInfo { peripheral_name: "SYSCFG", register_name: "PMC", reset_value: 0x0000_0000 }),
    (0x4001_3808, SvdRegisterInfo { peripheral_name: "SYSCFG", register_name: "EXTICR1", reset_value: 0x0000_0000 }),
    (0x4001_380C, SvdRegisterInfo { peripheral_name: "SYSCFG", register_name: "EXTICR2", reset_value: 0x0000_0000 }),
    (0x4001_3810, SvdRegisterInfo { peripheral_name: "SYSCFG", register_name: "EXTICR3", reset_value: 0x0000_0000 }),
    (0x4001_3814, SvdRegisterInfo { peripheral_name: "SYSCFG", register_name: "EXTICR4", reset_value: 0x0000_0000 }),
    (0x4001_3820, SvdRegisterInfo { peripheral_name: "SYSCFG", register_name: "CMPCR", reset_value: 0x0000_0000 }),
    (0x4001_382C, SvdRegisterInfo { peripheral_name: "SYSCFG", register_name: "CFGR", reset_value: 0x0000_0000 }),
    // --- EXTI (0x40013C00) ---
    (0x4001_3C00, SvdRegisterInfo { peripheral_name: "EXTI", register_name: "IMR", reset_value: 0x0000_0000 }),
    (0x4001_3C04, SvdRegisterInfo { peripheral_name: "EXTI", register_name: "EMR", reset_value: 0x0000_0000 }),
    (0x4001_3C08, SvdRegisterInfo { peripheral_name: "EXTI", register_name: "RTSR", reset_value: 0x0000_0000 }),
    (0x4001_3C0C, SvdRegisterInfo { peripheral_name: "EXTI", register_name: "FTSR", reset_value: 0x0000_0000 }),
    (0x4001_3C10, SvdRegisterInfo { peripheral_name: "EXTI", register_name: "SWIER", reset_value: 0x0000_0000 }),
    (0x4001_3C14, SvdRegisterInfo { peripheral_name: "EXTI", register_name: "PR", reset_value: 0x0000_0000 }),
    // --- SAI1 (0x40015800) ---
    (0x4001_5800, SvdRegisterInfo { peripheral_name: "SAI1", register_name: "GCR", reset_value: 0x0000_0000 }),
    // --- SAI1_Block_A (0x40015804) ---
    (0x4001_5804, SvdRegisterInfo { peripheral_name: "SAI1_Block_A", register_name: "CR1", reset_value: 0x0000_0040 }),
    (0x4001_5808, SvdRegisterInfo { peripheral_name: "SAI1_Block_A", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4001_580C, SvdRegisterInfo { peripheral_name: "SAI1_Block_A", register_name: "FRCR", reset_value: 0x0000_0000 }),
    (0x4001_5810, SvdRegisterInfo { peripheral_name: "SAI1_Block_A", register_name: "SLOTR", reset_value: 0x0000_0000 }),
    (0x4001_5818, SvdRegisterInfo { peripheral_name: "SAI1_Block_A", register_name: "SR", reset_value: 0x0000_0008 }),
    (0x4001_5820, SvdRegisterInfo { peripheral_name: "SAI1_Block_A", register_name: "DR", reset_value: 0x0000_0000 }),
    // --- SAI2 (0x40015C00) ---
    (0x4001_5C00, SvdRegisterInfo { peripheral_name: "SAI2", register_name: "GCR", reset_value: 0x0000_0000 }),
    // --- SAI2_Block_A (0x40015C04) ---
    (0x4001_5C04, SvdRegisterInfo { peripheral_name: "SAI2_Block_A", register_name: "CR1", reset_value: 0x0000_0040 }),
    (0x4001_5C08, SvdRegisterInfo { peripheral_name: "SAI2_Block_A", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4001_5C0C, SvdRegisterInfo { peripheral_name: "SAI2_Block_A", register_name: "FRCR", reset_value: 0x0000_0000 }),
    (0x4001_5C10, SvdRegisterInfo { peripheral_name: "SAI2_Block_A", register_name: "SLOTR", reset_value: 0x0000_0000 }),
    (0x4001_5C18, SvdRegisterInfo { peripheral_name: "SAI2_Block_A", register_name: "SR", reset_value: 0x0000_0008 }),
    (0x4001_5C20, SvdRegisterInfo { peripheral_name: "SAI2_Block_A", register_name: "DR", reset_value: 0x0000_0000 }),
    // --- SAI2_Block_B (0x40015C24) ---
    (0x4001_5C24, SvdRegisterInfo { peripheral_name: "SAI2_Block_B", register_name: "CR1", reset_value: 0x0000_0040 }),
    (0x4001_5C28, SvdRegisterInfo { peripheral_name: "SAI2_Block_B", register_name: "CR2", reset_value: 0x0000_0000 }),
    (0x4001_5C2C, SvdRegisterInfo { peripheral_name: "SAI2_Block_B", register_name: "FRCR", reset_value: 0x0000_0000 }),
    (0x4001_5C30, SvdRegisterInfo { peripheral_name: "SAI2_Block_B", register_name: "SLOTR", reset_value: 0x0000_0000 }),
    // --- GPIOA (0x40020000) ---
    (0x4002_0000, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "MODER", reset_value: 0xA800_0000 }),
    (0x4002_0004, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "OTYPER", reset_value: 0x0000_0000 }),
    (0x4002_0008, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "OSPEEDR", reset_value: 0x0000_0000 }),
    (0x4002_000C, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "PUPDR", reset_value: 0x6400_0000 }),
    (0x4002_0010, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "IDR", reset_value: 0x0000_0000 }),
    (0x4002_0014, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "ODR", reset_value: 0x0000_0000 }),
    (0x4002_0018, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "BSRR", reset_value: 0x0000_0000 }),
    (0x4002_0020, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "AFRL", reset_value: 0x0000_0000 }),
    (0x4002_0024, SvdRegisterInfo { peripheral_name: "GPIOA", register_name: "AFRH", reset_value: 0x0000_0000 }),
    // --- GPIOB (0x40020400) ---
    (0x4002_0400, SvdRegisterInfo { peripheral_name: "GPIOB", register_name: "MODER", reset_value: 0x0000_0280 }),
    (0x4002_040C, SvdRegisterInfo { peripheral_name: "GPIOB", register_name: "PUPDR", reset_value: 0x0000_0100 }),
    (0x4002_0410, SvdRegisterInfo { peripheral_name: "GPIOB", register_name: "IDR", reset_value: 0x0000_0000 }),
    (0x4002_0414, SvdRegisterInfo { peripheral_name: "GPIOB", register_name: "ODR", reset_value: 0x0000_0000 }),
    // --- GPIOC (0x40020800) ---
    (0x4002_0800, SvdRegisterInfo { peripheral_name: "GPIOC", register_name: "MODER", reset_value: 0x0000_0000 }),
    (0x4002_0810, SvdRegisterInfo { peripheral_name: "GPIOC", register_name: "IDR", reset_value: 0x0000_0000 }),
    (0x4002_0814, SvdRegisterInfo { peripheral_name: "GPIOC", register_name: "ODR", reset_value: 0x0000_0000 }),
    // --- GPIOD (0x40020C00) ---
    (0x4002_0C00, SvdRegisterInfo { peripheral_name: "GPIOD", register_name: "MODER", reset_value: 0x0000_0000 }),
    (0x4002_0C10, SvdRegisterInfo { peripheral_name: "GPIOD", register_name: "IDR", reset_value: 0x0000_0000 }),
    (0x4002_0C14, SvdRegisterInfo { peripheral_name: "GPIOD", register_name: "ODR", reset_value: 0x0000_0000 }),
    // --- GPIOE (0x40021000) ---
    (0x4002_1000, SvdRegisterInfo { peripheral_name: "GPIOE", register_name: "MODER", reset_value: 0x0000_0000 }),
    (0x4002_1010, SvdRegisterInfo { peripheral_name: "GPIOE", register_name: "IDR", reset_value: 0x0000_0000 }),
    (0x4002_1014, SvdRegisterInfo { peripheral_name: "GPIOE", register_name: "ODR", reset_value: 0x0000_0000 }),
    // --- CRC (0x40023000) ---
    (0x4002_3000, SvdRegisterInfo { peripheral_name: "CRC", register_name: "DR", reset_value: 0xFFFF_FFFF }),
    (0x4002_3004, SvdRegisterInfo { peripheral_name: "CRC", register_name: "IDR", reset_value: 0x0000_0000 }),
    (0x4002_3008, SvdRegisterInfo { peripheral_name: "CRC", register_name: "CR", reset_value: 0x0000_0000 }),
    // --- RCC (0x40023800) ---
    (0x4002_3800, SvdRegisterInfo { peripheral_name: "RCC", register_name: "CR", reset_value: 0x0000_0083 }),
    (0x4002_3804, SvdRegisterInfo { peripheral_name: "RCC", register_name: "PLLCFGR", reset_value: 0x2400_3010 }),
    (0x4002_3808, SvdRegisterInfo { peripheral_name: "RCC", register_name: "CFGR", reset_value: 0x0000_0000 }),
    (0x4002_380C, SvdRegisterInfo { peripheral_name: "RCC", register_name: "CIR", reset_value: 0x0000_0000 }),
    (0x4002_3830, SvdRegisterInfo { peripheral_name: "RCC", register_name: "AHB1ENR", reset_value: 0x0010_0000 }),
    (0x4002_3834, SvdRegisterInfo { peripheral_name: "RCC", register_name: "AHB2ENR", reset_value: 0x0000_0000 }),
    (0x4002_3838, SvdRegisterInfo { peripheral_name: "RCC", register_name: "AHB3ENR", reset_value: 0x0000_0000 }),
    (0x4002_3840, SvdRegisterInfo { peripheral_name: "RCC", register_name: "APB1ENR", reset_value: 0x0000_0000 }),
    (0x4002_3844, SvdRegisterInfo { peripheral_name: "RCC", register_name: "APB2ENR", reset_value: 0x0000_0000 }),
    (0x4002_3870, SvdRegisterInfo { peripheral_name: "RCC", register_name: "BDCR", reset_value: 0x0000_0000 }),
    (0x4002_3874, SvdRegisterInfo { peripheral_name: "RCC", register_name: "CSR", reset_value: 0x0E00_0000 }),
    (0x4002_3884, SvdRegisterInfo { peripheral_name: "RCC", register_name: "PLLSAICFGR", reset_value: 0x2400_3010 }),
    (0x4002_3888, SvdRegisterInfo { peripheral_name: "RCC", register_name: "PLLI2SCFGR", reset_value: 0x2000_3010 }),
    (0x4002_388C, SvdRegisterInfo { peripheral_name: "RCC", register_name: "DCKCFGR", reset_value: 0x0000_0000 }),
    (0x4002_3894, SvdRegisterInfo { peripheral_name: "RCC", register_name: "DCKCFGR2", reset_value: 0x0000_0000 }),
    // --- FLASH (0x40023C00) ---
    (0x4002_3C00, SvdRegisterInfo { peripheral_name: "FLASH", register_name: "ACR", reset_value: 0x0000_0000 }),
    (0x4002_3C04, SvdRegisterInfo { peripheral_name: "FLASH", register_name: "KEYR", reset_value: 0x0000_0000 }),
    (0x4002_3C08, SvdRegisterInfo { peripheral_name: "FLASH", register_name: "OPTKEYR", reset_value: 0x0000_0000 }),
    (0x4002_3C0C, SvdRegisterInfo { peripheral_name: "FLASH", register_name: "SR", reset_value: 0x0000_0000 }),
    (0x4002_3C10, SvdRegisterInfo { peripheral_name: "FLASH", register_name: "CR", reset_value: 0x8000_0000 }),
    (0x4002_3C14, SvdRegisterInfo { peripheral_name: "FLASH", register_name: "OPTCR", reset_value: 0x0FFF_AAED }),
    // --- DMA1 (0x40026000) ---
    (0x4002_6000, SvdRegisterInfo { peripheral_name: "DMA1", register_name: "LISR", reset_value: 0x0000_0000 }),
    (0x4002_6004, SvdRegisterInfo { peripheral_name: "DMA1", register_name: "HISR", reset_value: 0x0000_0000 }),
    (0x4002_6008, SvdRegisterInfo { peripheral_name: "DMA1", register_name: "LIFCR", reset_value: 0x0000_0000 }),
    (0x4002_600C, SvdRegisterInfo { peripheral_name: "DMA1", register_name: "HIFCR", reset_value: 0x0000_0000 }),
    // --- DMA1_Stream0 (0x40026010) ---
    (0x4002_6010, SvdRegisterInfo { peripheral_name: "DMA1_Stream0", register_name: "CR", reset_value: 0x0000_0000 }),
    (0x4002_6014, SvdRegisterInfo { peripheral_name: "DMA1_Stream0", register_name: "NDTR", reset_value: 0x0000_0000 }),
    (0x4002_6018, SvdRegisterInfo { peripheral_name: "DMA1_Stream0", register_name: "PAR", reset_value: 0x0000_0000 }),
    (0x4002_601C, SvdRegisterInfo { peripheral_name: "DMA1_Stream0", register_name: "M0AR", reset_value: 0x0000_0000 }),
    (0x4002_6020, SvdRegisterInfo { peripheral_name: "DMA1_Stream0", register_name: "M1AR", reset_value: 0x0000_0000 }),
    (0x4002_6024, SvdRegisterInfo { peripheral_name: "DMA1_Stream0", register_name: "FCR", reset_value: 0x0000_0021 }),
    // --- DMA2 (0x40026400) ---
    (0x4002_6400, SvdRegisterInfo { peripheral_name: "DMA2", register_name: "LISR", reset_value: 0x0000_0000 }),
    (0x4002_6404, SvdRegisterInfo { peripheral_name: "DMA2", register_name: "HISR", reset_value: 0x0000_0000 }),
    (0x4002_6408, SvdRegisterInfo { peripheral_name: "DMA2", register_name: "LIFCR", reset_value: 0x0000_0000 }),
    (0x4002_640C, SvdRegisterInfo { peripheral_name: "DMA2", register_name: "HIFCR", reset_value: 0x0000_0000 }),
    // --- DMA2_Stream0 (0x40026410) ---
    (0x4002_6410, SvdRegisterInfo { peripheral_name: "DMA2_Stream0", register_name: "CR", reset_value: 0x0000_0000 }),
    (0x4002_6414, SvdRegisterInfo { peripheral_name: "DMA2_Stream0", register_name: "NDTR", reset_value: 0x0000_0000 }),
    (0x4002_6418, SvdRegisterInfo { peripheral_name: "DMA2_Stream0", register_name: "PAR", reset_value: 0x0000_0000 }),
    (0x4002_641C, SvdRegisterInfo { peripheral_name: "DMA2_Stream0", register_name: "M0AR", reset_value: 0x0000_0000 }),
    // --- DMA2_Stream4 (0x40026470) ---
    (0x4002_6470, SvdRegisterInfo { peripheral_name: "DMA2_Stream4", register_name: "CR", reset_value: 0x0000_0000 }),
    (0x4002_6474, SvdRegisterInfo { peripheral_name: "DMA2_Stream4", register_name: "NDTR", reset_value: 0x0000_0000 }),
    (0x4002_6478, SvdRegisterInfo { peripheral_name: "DMA2_Stream4", register_name: "PAR", reset_value: 0x0000_0000 }),
    (0x4002_647C, SvdRegisterInfo { peripheral_name: "DMA2_Stream4", register_name: "M0AR", reset_value: 0x0000_0000 }),
    (0x4002_6480, SvdRegisterInfo { peripheral_name: "DMA2_Stream4", register_name: "M1AR", reset_value: 0x0000_0000 }),
    // --- QUADSPI (0xA0001000) ---
    (0xA000_1000, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "CR", reset_value: 0x0000_0000 }),
    (0xA000_1004, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "DCR", reset_value: 0x0000_0000 }),
    (0xA000_1008, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "SR", reset_value: 0x0000_0000 }),
    (0xA000_100C, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "FCR", reset_value: 0x0000_0000 }),
    (0xA000_1010, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "DLR", reset_value: 0x0000_0000 }),
    (0xA000_1014, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "CCR", reset_value: 0x0000_0000 }),
    (0xA000_1018, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "AR", reset_value: 0x0000_0000 }),
    (0xA000_101C, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "ABR", reset_value: 0x0000_0000 }),
    (0xA000_1020, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "DR", reset_value: 0x0000_0000 }),
    (0xA000_1024, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "PSMKR", reset_value: 0x0000_0000 }),
    (0xA000_1028, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "PSMAR", reset_value: 0x0000_0000 }),
    (0xA000_102C, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "PIR", reset_value: 0x0000_0000 }),
    (0xA000_1030, SvdRegisterInfo { peripheral_name: "QUADSPI", register_name: "LPTR", reset_value: 0x0000_0000 }),
    // --- SysTick (0xE000E010) ---
    (0xE000_E010, SvdRegisterInfo { peripheral_name: "SysTick", register_name: "CTRL", reset_value: 0x0000_0000 }),
    (0xE000_E014, SvdRegisterInfo { peripheral_name: "SysTick", register_name: "LOAD", reset_value: 0x0000_0000 }),
    (0xE000_E018, SvdRegisterInfo { peripheral_name: "SysTick", register_name: "VAL", reset_value: 0x0000_0000 }),
    (0xE000_E01C, SvdRegisterInfo { peripheral_name: "SysTick", register_name: "CALIB", reset_value: 0x0000_0000 }),
    // --- SCB (0xE000ED00) ---
    (0xE000_ED00, SvdRegisterInfo { peripheral_name: "SCB", register_name: "CPUID", reset_value: 0x410F_C241 }),
    (0xE000_ED04, SvdRegisterInfo { peripheral_name: "SCB", register_name: "ICSR", reset_value: 0x0000_0000 }),
    (0xE000_ED08, SvdRegisterInfo { peripheral_name: "SCB", register_name: "VTOR", reset_value: 0x0000_0000 }),
    (0xE000_ED0C, SvdRegisterInfo { peripheral_name: "SCB", register_name: "AIRCR", reset_value: 0xFA05_0000 }),
    (0xE000_ED10, SvdRegisterInfo { peripheral_name: "SCB", register_name: "SCR", reset_value: 0x0000_0000 }),
    (0xE000_ED14, SvdRegisterInfo { peripheral_name: "SCB", register_name: "CCR", reset_value: 0x0000_0200 }),
    (0xE000_ED18, SvdRegisterInfo { peripheral_name: "SCB", register_name: "SHPR1", reset_value: 0x0000_0000 }),
    (0xE000_ED1C, SvdRegisterInfo { peripheral_name: "SCB", register_name: "SHPR2", reset_value: 0x0000_0000 }),
    (0xE000_ED20, SvdRegisterInfo { peripheral_name: "SCB", register_name: "SHPR3", reset_value: 0x0000_0000 }),
    (0xE000_ED24, SvdRegisterInfo { peripheral_name: "SCB", register_name: "SHCSR", reset_value: 0x0000_0000 }),
    (0xE000_ED88, SvdRegisterInfo { peripheral_name: "SCB", register_name: "CPACR", reset_value: 0x0000_0000 }),
];

/// Looks up an architectural register description by aligned physical address.
pub fn lookup_svd_register(addr: u32) -> Option<&'static SvdRegisterInfo> {
    let aligned = addr & !3;
    SVD_REGISTERS
        .binary_search_by_key(&aligned, |(a, _)| *a)
        .ok()
        .map(|idx| &SVD_REGISTERS[idx].1)
}

#[derive(Default)]
pub struct SvdDiagnosticsTracker {
    seen_accesses: HashSet<(u32, bool)>, // (aligned_addr, is_write)
    seen_unmapped_pages: HashSet<u64>,   // page_base_addr
}

impl SvdDiagnosticsTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an MMIO access and returns true if this is the first time it has been observed.
    pub fn record_mmio_access(&mut self, addr: u32, is_write: bool) -> bool {
        let aligned = addr & !3;
        self.seen_accesses.insert((aligned, is_write))
    }

    /// Records an unmapped 4KB page dynamic allocation and returns true if first observed.
    pub fn record_unmapped_page(&mut self, page_addr: u64) -> bool {
        self.seen_unmapped_pages.insert(page_addr)
    }

    /// Clears the tracker (useful for unit testing).
    pub fn reset(&mut self) {
        self.seen_accesses.clear();
        self.seen_unmapped_pages.clear();
    }
}

static TRACKER: Mutex<Option<SvdDiagnosticsTracker>> = Mutex::new(None);

fn with_tracker<R>(f: impl FnOnce(&mut SvdDiagnosticsTracker) -> R) -> R {
    let mut guard = TRACKER.lock().unwrap();
    let tracker = guard.get_or_insert_with(SvdDiagnosticsTracker::default);
    f(tracker)
}

/// Logs an unhandled MMIO read with deduplication.
pub fn log_unhandled_mmio_read(addr: u32, size: usize) {
    let first = with_tracker(|t| t.record_mmio_access(addr, false));
    if first {
        if let Some(info) = lookup_svd_register(addr) {
            log::trace!(
                "[MMIO TRACE] Read unhandled 0x{:08X} (size {}) -> {}->{} (Reset: 0x{:08X})",
                addr, size, info.peripheral_name, info.register_name, info.reset_value
            );
        } else {
            log::trace!(
                "[MMIO TRACE] Read unhandled unmodeled address 0x{:08X} (size {})",
                addr, size
            );
        }
    }
}

/// Logs an unhandled MMIO write with deduplication.
pub fn log_unhandled_mmio_write(addr: u32, size: usize, val: u32) {
    let first = with_tracker(|t| t.record_mmio_access(addr, true));
    if first {
        if let Some(info) = lookup_svd_register(addr) {
            log::trace!(
                "[MMIO TRACE] Write unhandled 0x{:08X} (size {}) = 0x{:08X} -> {}->{} (Reset: 0x{:08X})",
                addr, size, val, info.peripheral_name, info.register_name, info.reset_value
            );
        } else {
            log::trace!(
                "[MMIO TRACE] Write unhandled unmodeled address 0x{:08X} (size {}) = 0x{:08X}",
                addr, size, val
            );
        }
    }
}

/// Logs on-demand mapping of an unmapped memory page with deduplication.
pub fn log_unmapped_page_allocation(page_addr: u64, access_addr: u64, is_write: bool, pc: u64) {
    let first = with_tracker(|t| t.record_unmapped_page(page_addr));
    if first {
        let op = if is_write { "Write" } else { "Read" };
        log::trace!(
            "[UNMAPPED MMIO] Dynamically mapped 4KB zeroed page at 0x{:08X} for {} at 0x{:08X} (PC: 0x{:08X})",
            page_addr, op, access_addr, pc
        );
    }
}

/// Resets the diagnostic tracker.
pub fn reset_diagnostics_tracker() {
    with_tracker(|t| t.reset());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svd_lookup_known_registers() {
        let tim2_sr = lookup_svd_register(0x4000_0010).unwrap();
        assert_eq!(tim2_sr.peripheral_name, "TIM2");
        assert_eq!(tim2_sr.register_name, "SR");
        assert_eq!(tim2_sr.reset_value, 0);

        let quadspi_dr = lookup_svd_register(0xA000_1020).unwrap();
        assert_eq!(quadspi_dr.peripheral_name, "QUADSPI");
        assert_eq!(quadspi_dr.register_name, "DR");

        let unaligned_tim6_arr = lookup_svd_register(0x4000_102D).unwrap();
        assert_eq!(unaligned_tim6_arr.peripheral_name, "TIM6");
        assert_eq!(unaligned_tim6_arr.register_name, "ARR");

        assert!(lookup_svd_register(0x1234_5678).is_none());
    }

    #[test]
    fn test_diagnostics_tracker_deduplication() {
        let mut tracker = SvdDiagnosticsTracker::new();
        // First access should record true
        assert!(tracker.record_mmio_access(0x4000_0010, false));
        // Second identical access should return false (deduplicated)
        assert!(!tracker.record_mmio_access(0x4000_0010, false));
        // Different access type (write) should record true
        assert!(tracker.record_mmio_access(0x4000_0010, true));
        // Second write should be deduplicated
        assert!(!tracker.record_mmio_access(0x4000_0010, true));

        // Unmapped page deduplication
        assert!(tracker.record_unmapped_page(0x6000_0000));
        assert!(!tracker.record_unmapped_page(0x6000_0000));
    }
}
