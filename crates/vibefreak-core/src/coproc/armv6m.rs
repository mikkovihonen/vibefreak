//! Pure-Rust ARMv6-M (Cortex-M0 Thumb-1) core emulator for STM32F0 coprocessors.
//! Implements strict hardware emulation: register file, condition flags, arithmetic logic,
//! load/store, branches, hardware exception stacking (EXC_RETURN), and STM32F0 peripherals.

use std::collections::VecDeque;

pub const VECTOR_RESET: u32 = 1;
pub const VECTOR_NMI: u32 = 2;
pub const VECTOR_HARDFAULT: u32 = 3;
pub const VECTOR_SVCALL: u32 = 11;
pub const VECTOR_PENDSV: u32 = 14;
pub const VECTOR_SYSTICK: u32 = 15;
pub const VECTOR_EXTI0_1: u32 = 5 + 16; // IRQ 5 (21 in vector table)
pub const VECTOR_EXTI2_3: u32 = 6 + 16; // IRQ 6 (22 in vector table)
pub const VECTOR_EXTI4_15: u32 = 7 + 16; // IRQ 7 (23 in vector table)
pub const VECTOR_DMA1_CH1: u32 = 9 + 16; // IRQ 9 (25 in vector table)
pub const VECTOR_DMA1_CH2_3: u32 = 10 + 16; // IRQ 10 (26 in vector table)
pub const VECTOR_DMA1_CH4_5: u32 = 11 + 16; // IRQ 11 (27 in vector table)
pub const VECTOR_ADC: u32 = 12 + 16; // IRQ 12 (28 in vector table)
pub const VECTOR_TIM1_BRK_UP_TRG_COM: u32 = 13 + 16; // IRQ 13 (29 in vector table)
pub const VECTOR_TIM1_CC: u32 = 14 + 16; // IRQ 14 (30 in vector table)
pub const VECTOR_TIM2: u32 = 15 + 16; // IRQ 15 (31 in vector table)
pub const VECTOR_TIM3: u32 = 16 + 16; // IRQ 16 (32 in vector table)
pub const VECTOR_TIM6: u32 = 17 + 16; // IRQ 17 (33 in vector table)
pub const VECTOR_TIM7: u32 = 18 + 16; // IRQ 18 (34 in vector table)
pub const VECTOR_TIM14: u32 = 19 + 16; // IRQ 19 (35 in vector table)
pub const VECTOR_TIM15: u32 = 20 + 16; // IRQ 20 (36 in vector table)
pub const VECTOR_TIM16: u32 = 21 + 16; // IRQ 21 (37 in vector table)
pub const VECTOR_TIM17: u32 = 22 + 16; // IRQ 22 (38 in vector table)
pub const VECTOR_USART1: u32 = 27 + 16; // IRQ 27 (43 in vector table)
pub const VECTOR_USART2: u32 = 28 + 16; // IRQ 28 (44 in vector table)
pub const VECTOR_USART3_8: u32 = 29 + 16; // IRQ 29 (45 in vector table)

pub const FLASH_BASE: u32 = 0x0800_0000;
pub const SRAM_BASE: u32 = 0x2000_0000;

/// Physical hardware emulation model of STM32F0 USART peripheral (RM0091 Section 27).
/// Implements 1-byte TDR latch, serialization shift register, and interrupt flag generation.
#[derive(Debug, Clone)]
pub struct Stm32F0Usart {
    pub cr1: u32,
    pub cr2: u32,
    pub cr3: u32,
    pub brr: u32,
    pub isr: u32,
    pub tdr: Option<u8>,
    pub shift_reg: Option<u8>,
    pub shift_cycles_remaining: u32,
    pub rx: VecDeque<u8>,
    pub tx: VecDeque<u8>,
}

impl Stm32F0Usart {
    pub fn new() -> Self {
        Self {
            cr1: 0,
            cr2: 0,
            cr3: 0,
            brr: 0,
            // Reset value per RM0091 Section 27.8.8: TXE (bit 7) = 1, TC (bit 6) = 1
            isr: (1 << 7) | (1 << 6),
            tdr: None,
            shift_reg: None,
            shift_cycles_remaining: 0,
            rx: VecDeque::new(),
            tx: VecDeque::new(),
        }
    }

    #[inline]
    pub fn read_isr(&self) -> u32 {
        let mut val = self.isr;
        if !self.rx.is_empty() {
            val |= 1 << 5; // RXNE = 1
        } else {
            val &= !(1 << 5); // RXNE = 0
        }
        val
    }

    #[inline]
    pub fn read_rdr(&mut self) -> u32 {
        let b = self.rx.pop_front().map(|b| b as u32).unwrap_or(0);
        if self.rx.is_empty() {
            self.isr &= !(1 << 5); // Clear RXNE
        }
        b
    }

    pub fn write_tdr(&mut self, val: u8) {
        self.tdr = Some(val);
        self.isr &= !(1 << 7); // TXE = 0 (TDR occupied)
        self.isr &= !(1 << 6); // TC = 0 (Transmission in progress)

        if self.shift_reg.is_none() {
            self.transfer_tdr_to_shift();
        }
    }

    pub fn write_icr(&mut self, val: u32) {
        if (val & (1 << 6)) != 0 {
            self.isr &= !(1 << 6); // Clear TC flag
        }
    }

    fn transfer_tdr_to_shift(&mut self) {
        if let Some(b) = self.tdr.take() {
            self.shift_reg = Some(b);
            self.isr |= 1 << 7; // TXE = 1 (TDR empty again, can accept next byte)
            let bit_cycles = if self.brr > 0 { self.brr } else { 96 };
            self.shift_cycles_remaining = bit_cycles * 10;
        }
    }

    pub fn step_cycles(&mut self, mut cycles: u32) {
        while self.shift_reg.is_some() && cycles > 0 {
            if cycles >= self.shift_cycles_remaining {
                cycles -= self.shift_cycles_remaining;
                let b = self.shift_reg.take().unwrap();
                self.tx.push_back(b);
                self.shift_cycles_remaining = 0;

                if self.tdr.is_some() {
                    self.transfer_tdr_to_shift();
                } else {
                    self.isr |= 1 << 6; // TC = 1 (Transmission Complete)
                }
            } else {
                self.shift_cycles_remaining -= cycles;
                cycles = 0;
            }
        }
    }

    #[inline]
    pub fn is_interrupt_pending(&self) -> bool {
        let isr = self.read_isr();
        let rxne = (self.cr1 & (1 << 5)) != 0 && (isr & (1 << 5)) != 0;
        let txe = (self.cr1 & (1 << 7)) != 0 && (isr & (1 << 7)) != 0;
        let tc = (self.cr1 & (1 << 6)) != 0 && (isr & (1 << 6)) != 0;
        rxne || txe || tc
    }
}

#[derive(Clone)]
pub struct ArmV6MCpu {
    pub regs: [u32; 16], // R0..R15 (R13=SP, R14=LR, R15=PC)
    pub msp: u32,
    pub psp: u32,
    pub xpsr: u32,
    pub flag_n: bool,
    pub flag_z: bool,
    pub flag_c: bool,
    pub flag_v: bool,
    pub primask: bool,
    pub control_spsel: bool,
    pub in_handler_mode: bool,
    pub sleeping: bool,
    pub cycles: u64,

    pub flash: Vec<u8>,
    pub sram: Vec<u8>,

    // SYSCFG->CFGR1 (0x40010000): MEM_MODE bits [1:0]
    // 00: Main Flash, 01: System Memory, 11: Embedded SRAM
    pub syscfg_cfgr1: u32,

    // STM32F0 Peripherals
    pub usart1: Stm32F0Usart,
    pub usart2: Stm32F0Usart,
    pub usart3: Stm32F0Usart,
    pub usart4: Stm32F0Usart,
    pub usart5: Stm32F0Usart,

    pub dma1_isr: u32,
    pub dma1_ccr1: u32,
    pub dma1_cndtr1: u32,
    pub dma1_cpar1: u32,
    pub dma1_cmar1: u32,
    pub dma1_reload_cndtr1: u32,
    pub dma1_base_cmar1: u32,
    pub dma1_ccr2: u32,
    pub dma1_cndtr2: u32,
    pub dma1_cpar2: u32,
    pub dma1_cmar2: u32,
    pub dma1_completion_delay2: u32,
    pub dma1_ccr3: u32,
    pub dma1_cndtr3: u32,
    pub dma1_cpar3: u32,
    pub dma1_cmar3: u32,
    pub dma1_ccr4: u32,
    pub dma1_cndtr4: u32,
    pub dma1_cpar4: u32,
    pub dma1_cmar4: u32,
    pub dma1_completion_delay4: u32,
    pub dma1_ccr5: u32,
    pub dma1_cndtr5: u32,
    pub dma1_cpar5: u32,
    pub dma1_cmar5: u32,
    pub dma1_completion_delay5: u32,
    pub tim6_sr: u32,

    // RCC (RM0360 §6.4)
    pub rcc_cr: u32,
    pub rcc_cfgr: u32,
    pub rcc_cr2: u32,
    pub rcc_cir: u32,
    pub rcc_apb2rstr: u32,
    pub rcc_apb1rstr: u32,
    pub rcc_ahbenr: u32,
    pub rcc_apb2enr: u32,
    pub rcc_apb1enr: u32,
    pub rcc_bdcr: u32,
    pub rcc_csr: u32,
    pub rcc_ahbrstr: u32,
    pub rcc_cfgr2: u32,
    pub rcc_cfgr3: u32,

    // SPI
    pub spi1_sr: u32,
    pub spi1_dr: u32,
    pub spi1_tx_bytes: Vec<u8>,
    pub dma_frame_ready: bool,
    pub dma1_completion_delay: u32,

    pub spi2_cr1: u32,
    pub spi2_cr2: u32,
    pub spi2_sr: u32,
    pub spi2_dr: u32,

    // SYSCFG EXTI & EXTI (RM0360)
    pub syscfg_exticr: [u16; 4],
    pub exti_imr: u32,
    pub exti_emr: u32,
    pub exti_rtsr: u32,
    pub exti_ftsr: u32,
    pub exti_swier: u32,
    pub exti_pr: u32,

    // Power & Flash
    pub pwr_cr: u32,
    pub pwr_csr: u32,
    pub flash_acr: u32,
    pub dbgmcu_cr: u32,

    // GPIO Ports A, B, C, D, F (RM0360 §8.4)
    pub gpioa_moder: u32,
    pub gpioa_otyper: u32,
    pub gpioa_ospeedr: u32,
    pub gpioa_pupdr: u32,
    pub gpioa_idr: u32,
    pub gpioa_odr: u32,
    pub gpioa_afrl: u32,
    pub gpioa_afrh: u32,

    pub gpiob_moder: u32,
    pub gpiob_otyper: u32,
    pub gpiob_ospeedr: u32,
    pub gpiob_pupdr: u32,
    pub gpiob_idr: u32,
    pub gpiob_odr: u32,
    pub gpiob_afrl: u32,
    pub gpiob_afrh: u32,

    pub gpioc_moder: u32,
    pub gpioc_otyper: u32,
    pub gpioc_ospeedr: u32,
    pub gpioc_pupdr: u32,
    pub gpioc_idr: u32,
    pub gpioc_odr: u32,
    pub gpioc_afrl: u32,
    pub gpioc_afrh: u32,

    pub gpiod_moder: u32,
    pub gpiod_otyper: u32,
    pub gpiod_ospeedr: u32,
    pub gpiod_pupdr: u32,
    pub gpiod_idr: u32,
    pub gpiod_odr: u32,
    pub gpiod_afrl: u32,
    pub gpiod_afrh: u32,

    pub gpiof_moder: u32,
    pub gpiof_otyper: u32,
    pub gpiof_ospeedr: u32,
    pub gpiof_pupdr: u32,
    pub gpiof_idr: u32,
    pub gpiof_odr: u32,
    pub gpiof_afrl: u32,
    pub gpiof_afrh: u32,

    // Timers
    pub tim1_cr1: u32,
    pub tim1_cr2: u32,
    pub tim1_smcr: u32,
    pub tim1_dier: u32,
    pub tim1_sr: u32,
    pub tim1_cnt: u32,
    pub tim1_psc: u32,
    pub tim1_arr: u32,
    pub tim1_rcr: u32,
    pub tim1_ccr1: u32,
    pub tim1_ccr2: u32,
    pub tim1_ccr3: u32,
    pub tim1_ccr4: u32,
    pub tim1_bdtr: u32,

    pub tim2_cr1: u32,
    pub tim2_cr2: u32,
    pub tim2_smcr: u32,
    pub tim2_dier: u32,
    pub tim2_sr: u32,
    pub tim2_cnt: u32,
    pub tim2_psc: u32,
    pub tim2_arr: u32,
    pub tim2_ccr1: u32,
    pub tim2_ccr2: u32,
    pub tim2_ccr3: u32,
    pub tim2_ccr4: u32,

    pub tim3_cr1: u32,
    pub tim3_dier: u32,
    pub tim3_sr: u32,
    pub tim3_cnt: u32,
    pub tim3_arr: u32,

    pub tim6_cr1: u32,
    pub tim6_dier: u32,
    pub tim6_cnt: u32,
    pub tim6_psc: u32,
    pub tim6_arr: u32,

    pub tim7_cr1: u32,
    pub tim7_cr2: u32,
    pub tim7_dier: u32,
    pub tim7_sr: u32,
    pub tim7_cnt: u32,
    pub tim7_psc: u32,
    pub tim7_arr: u32,

    pub tim14_cr1: u32,
    pub tim14_dier: u32,
    pub tim14_sr: u32,
    pub tim14_cnt: u32,
    pub tim14_psc: u32,
    pub tim14_arr: u32,

    pub tim15_cr1: u32,
    pub tim15_dier: u32,
    pub tim15_sr: u32,
    pub tim15_cnt: u32,
    pub tim15_arr: u32,
    pub tim15_psc: u32,
    pub tim15_ccr1: u32,

    pub tim16_cr1: u32,
    pub tim16_dier: u32,
    pub tim16_sr: u32,
    pub tim16_cnt: u32,
    pub tim16_arr: u32,
    pub tim16_psc: u32,
    pub tim16_ccr1: u32,

    pub tim17_cr1: u32,
    pub tim17_dier: u32,
    pub tim17_sr: u32,
    pub tim17_cnt: u32,
    pub tim17_arr: u32,
    pub tim17_psc: u32,
    pub tim17_ccr1: u32,

    // I2C (RM0360 §24.7)
    pub i2c1_cr1: u32,
    pub i2c1_cr2: u32,
    pub i2c1_oar1: u32,
    pub i2c1_oar2: u32,
    pub i2c1_timingr: u32,
    pub i2c1_timeoutr: u32,
    pub i2c1_isr: u32,
    pub i2c1_rxdr: u32,
    pub i2c1_txdr: u32,

    pub i2c2_cr1: u32,
    pub i2c2_cr2: u32,
    pub i2c2_oar1: u32,
    pub i2c2_oar2: u32,
    pub i2c2_timingr: u32,
    pub i2c2_timeoutr: u32,
    pub i2c2_isr: u32,
    pub i2c2_rxdr: u32,
    pub i2c2_txdr: u32,

    // ADC (RM0360 §12.12)
    pub adc_isr: u32,
    pub adc_ier: u32,
    pub adc_cr: u32,
    pub adc_cfgr1: u32,
    pub adc_cfgr2: u32,
    pub adc_smpr: u32,
    pub adc_chselr: u32,
    pub adc_dr: u32,
    pub adc_ccr: u32,

    // Cortex-M0 System Peripherals (NVIC, SCB & SysTick - PM0215)
    pub nvic_iser: u32,
    pub nvic_ispr: u32,
    pub nvic_ipr: [u8; 32],
    pub shpr2: u32,
    pub shpr3: u32,
    pub scb_aircr: u32,
    pub scb_scr: u32,
    pub pendsv_pending: bool,
    pub active_exceptions: Vec<(u8, i16)>,
    pub systick_ctrl: u32,
    pub systick_load: u32,
    pub systick_val: u32,
    pub systick_cycle_accum: u32,
    pub systick_pending: bool,
    pub ff_hits: usize,
    pub ff_cycles: u64,

    pub is_halted: bool,
}

impl ArmV6MCpu {
    pub fn new(flash_data: Vec<u8>, sram_size: usize) -> Self {
        let sram = vec![0u8; sram_size.max(32 * 1024)];
        let mut cpu = Self {
            regs: [0; 16],
            msp: 0,
            psp: 0,
            xpsr: 1 << 24, // Thumb bit
            flag_n: false,
            flag_z: false,
            flag_c: false,
            flag_v: false,
            primask: false,
            control_spsel: false,
            in_handler_mode: false,
            sleeping: false,
            cycles: 0,
            flash: flash_data,
            sram,
            syscfg_cfgr1: 0,
            usart1: Stm32F0Usart::new(),
            usart2: Stm32F0Usart::new(),
            usart3: Stm32F0Usart::new(),
            usart4: Stm32F0Usart::new(),
            usart5: Stm32F0Usart::new(),
            dma1_isr: 1 << 9,
            dma1_ccr1: 0,
            dma1_cndtr1: 0,
            dma1_cpar1: 0x4001_2440,
            dma1_cmar1: 0,
            dma1_reload_cndtr1: 0,
            dma1_base_cmar1: 0,
            dma1_ccr2: 0,
            dma1_cndtr2: 0,
            dma1_cpar2: 0x4001_3828,
            dma1_cmar2: 0,
            dma1_completion_delay2: 0,
            dma1_ccr3: 0,
            dma1_cndtr3: 0,
            dma1_cpar3: 0x4001_300C,
            dma1_cmar3: 0x2000_0068,
            dma1_ccr4: 0,
            dma1_cndtr4: 0,
            dma1_cpar4: 0,
            dma1_cmar4: 0,
            dma1_completion_delay4: 0,
            dma1_ccr5: 0,
            dma1_cndtr5: 0,
            dma1_cpar5: 0,
            dma1_cmar5: 0,
            dma1_completion_delay5: 0,
            dma_frame_ready: false,
            dma1_completion_delay: 0,
            syscfg_exticr: [0; 4],
            exti_imr: 0,
            exti_emr: 0,
            exti_rtsr: 0,
            exti_ftsr: 0,
            exti_swier: 0,
            exti_pr: 0,
            pwr_cr: 0,
            pwr_csr: 0,
            flash_acr: 0x0000_0030,
            dbgmcu_cr: 0,
            rcc_cr: (1 << 1) | (1 << 25), // HSIRDY | PLLRDY
            rcc_cfgr: 8,                  // SWS = PLL
            rcc_cr2: 1 << 1,              // HSI14RDY
            rcc_cir: 0,
            rcc_apb2rstr: 0,
            rcc_apb1rstr: 0,
            rcc_ahbenr: 0x0000_0014,
            rcc_apb2enr: 0,
            rcc_apb1enr: 0,
            rcc_bdcr: 0,
            rcc_csr: 0x0C00_0000,
            rcc_ahbrstr: 0,
            rcc_cfgr2: 0,
            rcc_cfgr3: 0,
            spi1_sr: (1 << 1) | 1,        // TXE | RXNE
            spi1_dr: 0,
            spi1_tx_bytes: Vec::new(),
            spi2_cr1: 0,
            spi2_cr2: 0,
            spi2_sr: 1 << 1,              // TXE
            spi2_dr: 0,
            gpioa_moder: 0x2800_0000,
            gpioa_otyper: 0,
            gpioa_ospeedr: 0x0C00_0000,
            gpioa_pupdr: 0x2400_0000,
            gpioa_idr: 0xFFFF_FFFF,
            gpioa_odr: 0,
            gpioa_afrl: 0,
            gpioa_afrh: 0,
            gpiob_moder: 0,
            gpiob_otyper: 0,
            gpiob_ospeedr: 0,
            gpiob_pupdr: 0,
            gpiob_idr: 0xFFFF_FFFF,
            gpiob_odr: 0,
            gpiob_afrl: 0,
            gpiob_afrh: 0,
            gpioc_moder: 0,
            gpioc_otyper: 0,
            gpioc_ospeedr: 0,
            gpioc_pupdr: 0,
            gpioc_idr: 0xFFFF_FFFF,
            gpioc_odr: 0,
            gpioc_afrl: 0,
            gpioc_afrh: 0,
            gpiod_moder: 0,
            gpiod_otyper: 0,
            gpiod_ospeedr: 0,
            gpiod_pupdr: 0,
            gpiod_idr: 0xFFFF_FFFF,
            gpiod_odr: 0,
            gpiod_afrl: 0,
            gpiod_afrh: 0,
            gpiof_moder: 0,
            gpiof_otyper: 0,
            gpiof_ospeedr: 0,
            gpiof_pupdr: 0,
            gpiof_idr: 0xFFFF_FFFF,
            gpiof_odr: 0,
            gpiof_afrl: 0,
            gpiof_afrh: 0,
            tim1_cr1: 0,
            tim1_cr2: 0,
            tim1_smcr: 0,
            tim1_dier: 0,
            tim1_sr: 0,
            tim1_cnt: 0,
            tim1_psc: 0,
            tim1_arr: 0,
            tim1_rcr: 0,
            tim1_ccr1: 0,
            tim1_ccr2: 0,
            tim1_ccr3: 0,
            tim1_ccr4: 0,
            tim1_bdtr: 0,
            tim2_cr1: 0,
            tim2_cr2: 0,
            tim2_smcr: 0,
            tim2_dier: 0,
            tim2_sr: 0,
            tim2_cnt: 0,
            tim2_psc: 0,
            tim2_arr: 0,
            tim2_ccr1: 0,
            tim2_ccr2: 0,
            tim2_ccr3: 0,
            tim2_ccr4: 0,
            tim3_cr1: 0,
            tim3_dier: 0,
            tim3_sr: 0,
            tim3_cnt: 0,
            tim3_arr: 0,
            tim6_cr1: 0,
            tim6_dier: 0,
            tim6_cnt: 0,
            tim6_psc: 0,
            tim6_arr: 0,
            tim7_cr1: 0,
            tim7_cr2: 0,
            tim7_dier: 0,
            tim7_sr: 0,
            tim7_cnt: 0,
            tim7_psc: 0,
            tim7_arr: 0,
            tim14_cr1: 0,
            tim14_dier: 0,
            tim14_sr: 0,
            tim14_cnt: 0,
            tim14_psc: 0,
            tim14_arr: 0,
            tim15_cr1: 0,
            tim15_dier: 0,
            tim15_sr: 0,
            tim15_cnt: 0,
            tim15_arr: 0,
            tim15_psc: 0,
            tim15_ccr1: 0,
            tim16_cr1: 0,
            tim16_dier: 0,
            tim16_sr: 0,
            tim16_cnt: 0,
            tim16_arr: 0,
            tim16_psc: 0,
            tim16_ccr1: 0,
            tim17_cr1: 0,
            tim17_dier: 0,
            tim17_sr: 0,
            tim17_cnt: 0,
            tim17_arr: 0,
            tim17_psc: 0,
            tim17_ccr1: 0x08C4,
            i2c1_cr1: 0,
            i2c1_cr2: 0,
            i2c1_oar1: 0,
            i2c1_oar2: 0,
            i2c1_timingr: 0,
            i2c1_timeoutr: 0,
            i2c1_isr: (1 << 0) | (1 << 5), // TXE | STOPF
            i2c1_rxdr: 0,
            i2c1_txdr: 0,
            i2c2_cr1: 0,
            i2c2_cr2: 0,
            i2c2_oar1: 0,
            i2c2_oar2: 0,
            i2c2_timingr: 0,
            i2c2_timeoutr: 0,
            i2c2_isr: (1 << 0) | (1 << 5), // TXE | STOPF
            i2c2_rxdr: 0,
            i2c2_txdr: 0,
            adc_isr: 1, // ADRDY
            adc_ier: 0,
            adc_cr: 0,
            adc_cfgr1: 0,
            adc_cfgr2: 0,
            adc_smpr: 0,
            adc_chselr: 0,
            adc_dr: 2048,
            adc_ccr: 0,
            tim6_sr: 1,
            nvic_iser: 0,
            nvic_ispr: 0,
            nvic_ipr: [0; 32],
            shpr2: 0,
            shpr3: 0,
            scb_aircr: 0,
            scb_scr: 0,
            pendsv_pending: false,
            active_exceptions: Vec::new(),
            systick_ctrl: 0,
            systick_load: 0,
            systick_val: 0,
            systick_cycle_accum: 0,
            systick_pending: false,
            ff_hits: 0,
            ff_cycles: 0,
            is_halted: false,
        };
        cpu.reset();
        cpu
    }

    pub fn reset(&mut self) {
        let sp = self.read32(FLASH_BASE) & !3;
        let reset_vector = self.read32(FLASH_BASE + 4) & !1;
        self.regs[0..13].fill(0);
        self.regs[13] = sp;
        self.msp = sp;
        self.psp = 0;
        self.regs[14] = 0xFFFF_FFFF;
        self.regs[15] = reset_vector;
        self.xpsr = 1 << 24; // Thumb bit
        self.flag_n = false;
        self.flag_z = false;
        self.flag_c = false;
        self.flag_v = false;
        self.primask = false;
        self.control_spsel = false;
        self.in_handler_mode = false;
        self.sleeping = false;
        self.cycles = 0;
        self.dma1_cndtr3 = 0;
        self.dma1_cpar3 = 0x4001_300C;
        self.spi1_dr = 0;
        self.spi1_tx_bytes.clear();
        self.dma_frame_ready = false;
        self.dma1_completion_delay = 0;
        self.dma1_completion_delay2 = 0;
        self.dma1_completion_delay4 = 0;
        self.dma1_completion_delay5 = 0;
        self.nvic_ispr = 0;
        self.pendsv_pending = false;
        self.active_exceptions.clear();
        self.systick_pending = false;
        self.systick_cycle_accum = 0;
        self.spi2_sr = 1 << 1;
        self.i2c1_isr = (1 << 0) | (1 << 5);
        self.i2c2_isr = (1 << 0) | (1 << 5);
        self.adc_isr = 1;
        self.is_halted = false;
    }

    #[inline(always)]
    pub fn pc(&self) -> u32 {
        self.regs[15]
    }

    #[inline(always)]
    pub fn set_pc(&mut self, val: u32) {
        self.regs[15] = val & !1;
    }

    #[inline(always)]
    pub fn sp(&self) -> u32 {
        self.regs[13]
    }

    #[inline(always)]
    pub fn set_sp(&mut self, val: u32) {
        let sp = val & !3;
        self.regs[13] = sp;
        if self.in_handler_mode || !self.control_spsel {
            self.msp = sp;
        } else {
            self.psp = sp;
        }
    }

    #[inline(always)]
    pub fn lr(&self) -> u32 {
        self.regs[14]
    }

    // --- Memory Bus Read / Write ---

    #[inline(always)]
    pub fn read8(&mut self, addr: u32) -> u8 {
        match addr {
            // Boot alias (0x0000_0000..0x0001_0000)
            0x0000_0000..=0x0000_FFFF if (self.syscfg_cfgr1 & 3) == 3 => {
                let off = addr as usize;
                if off < self.sram.len() {
                    self.sram[off]
                } else {
                    0
                }
            }
            0x0000_0000..=0x0007_FFFF => {
                if (addr as usize) < self.flash.len() {
                    self.flash[addr as usize]
                } else {
                    0xFF
                }
            }
            // Flash (0x0800_0000..0x0808_0000)
            0x0800_0000..=0x0807_FFFF => {
                let off = (addr - FLASH_BASE) as usize;
                if off < self.flash.len() {
                    self.flash[off]
                } else {
                    0xFF
                }
            }
            // SRAM (0x2000_0000..0x2000_FFFF)
            0x2000_0000..=0x2000_FFFF => {
                let off = (addr - SRAM_BASE) as usize;
                if off < self.sram.len() {
                    self.sram[off]
                } else {
                    0
                }
            }
            // MMIO Read
            _ => (self.read_mmio(addr & !3) >> ((addr % 4) * 8)) as u8,
        }
    }

    #[inline(always)]
    pub fn read16(&mut self, addr: u32) -> u16 {
        if addr >= SRAM_BASE && addr + 1 < SRAM_BASE + self.sram.len() as u32 && (addr % 2) == 0 {
            let off = (addr - SRAM_BASE) as usize;
            u16::from_le_bytes(self.sram[off..off + 2].try_into().unwrap())
        } else if addr >= FLASH_BASE && addr + 1 < FLASH_BASE + self.flash.len() as u32 && (addr % 2) == 0 {
            let off = (addr - FLASH_BASE) as usize;
            u16::from_le_bytes(self.flash[off..off + 2].try_into().unwrap())
        } else if (addr >= 0x4000_0000 && addr < 0x5000_0000)
            || (addr >= 0xE000_0000 && addr < 0xF000_0000)
        {
            let val = self.read_mmio(addr & !3);
            (val >> ((addr % 4) * 8)) as u16
        } else {
            let b0 = self.read8(addr) as u16;
            let b1 = self.read8(addr + 1) as u16;
            b0 | (b1 << 8)
        }
    }

    #[inline(always)]
    pub fn read32(&mut self, addr: u32) -> u32 {
        if addr >= SRAM_BASE && addr + 3 < SRAM_BASE + self.sram.len() as u32 && (addr % 4) == 0 {
            let off = (addr - SRAM_BASE) as usize;
            u32::from_le_bytes(self.sram[off..off + 4].try_into().unwrap())
        } else if addr >= FLASH_BASE
            && addr + 3 < FLASH_BASE + self.flash.len() as u32
            && (addr % 4) == 0
        {
            let off = (addr - FLASH_BASE) as usize;
            u32::from_le_bytes(self.flash[off..off + 4].try_into().unwrap())
        } else if addr < 0x0008_0000 && addr + 3 < self.flash.len() as u32 && (addr % 4) == 0 {
            if (self.syscfg_cfgr1 & 3) == 3 && (addr as usize + 3) < self.sram.len() {
                let off = addr as usize;
                u32::from_le_bytes(self.sram[off..off + 4].try_into().unwrap())
            } else {
                u32::from_le_bytes(
                    self.flash[addr as usize..addr as usize + 4]
                        .try_into()
                        .unwrap(),
                )
            }
        } else if (addr >= 0x4000_0000 && addr < 0x5000_0000)
            || (addr >= 0xE000_0000 && addr < 0xF000_0000)
        {
            self.read_mmio(addr)
        } else {
            let b0 = self.read8(addr) as u32;
            let b1 = self.read8(addr + 1) as u32;
            let b2 = self.read8(addr + 2) as u32;
            let b3 = self.read8(addr + 3) as u32;
            b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
        }
    }

    pub fn write8(&mut self, addr: u32, val: u8) {
        match addr {
            0x0000_0000..=0x0000_FFFF if (self.syscfg_cfgr1 & 3) == 3 => {
                let off = addr as usize;
                if off < self.sram.len() {
                    self.sram[off] = val;
                }
            }
            0x2000_0000..=0x2000_FFFF => {
                let off = (addr - SRAM_BASE) as usize;
                if off < self.sram.len() {
                    self.sram[off] = val;
                }
            }
            _ => {
                self.write_mmio(addr, val as u32, 1);
            }
        }
    }

    pub fn write16(&mut self, addr: u32, val: u16) {
        if addr >= SRAM_BASE && addr + 1 < SRAM_BASE + self.sram.len() as u32 && (addr % 2) == 0 {
            let off = (addr - SRAM_BASE) as usize;
            self.sram[off..off + 2].copy_from_slice(&val.to_le_bytes());
        } else if addr < 0x0001_0000
            && (addr as usize + 1) < self.sram.len()
            && (self.syscfg_cfgr1 & 3) == 3
            && (addr % 2) == 0
        {
            let off = addr as usize;
            self.sram[off..off + 2].copy_from_slice(&val.to_le_bytes());
        } else if (addr >= 0x4000_0000 && addr < 0x5000_0000)
            || (addr >= 0xE000_0000 && addr < 0xF000_0000)
        {
            self.write_mmio(addr, val as u32, 2);
        } else {
            self.write8(addr, (val & 0xFF) as u8);
            self.write8(addr + 1, ((val >> 8) & 0xFF) as u8);
        }
    }

    pub fn write32(&mut self, addr: u32, val: u32) {
        if addr >= SRAM_BASE && addr + 3 < SRAM_BASE + self.sram.len() as u32 && (addr % 4) == 0 {
            let off = (addr - SRAM_BASE) as usize;
            self.sram[off..off + 4].copy_from_slice(&val.to_le_bytes());
        } else if addr < 0x0001_0000
            && (addr as usize + 3) < self.sram.len()
            && (self.syscfg_cfgr1 & 3) == 3
            && (addr % 4) == 0
        {
            let off = addr as usize;
            self.sram[off..off + 4].copy_from_slice(&val.to_le_bytes());
        } else if (addr >= 0x4000_0000 && addr < 0x5000_0000)
            || (addr >= 0xE000_0000 && addr < 0xF000_0000)
        {
            self.write_mmio(addr, val, 4);
        } else {
            self.write8(addr, (val & 0xFF) as u8);
            self.write8(addr + 1, ((val >> 8) & 0xFF) as u8);
            self.write8(addr + 2, ((val >> 16) & 0xFF) as u8);
            self.write8(addr + 3, ((val >> 24) & 0xFF) as u8);
        }
    }

    #[inline(always)]
    pub fn current_execution_priority(&self) -> i16 {
        let base = if let Some(&(_, prio)) = self.active_exceptions.last() {
            prio
        } else {
            256 // Thread mode base priority
        };
        if self.primask {
            base.min(0) // PRIMASK raises execution priority to 0
        } else {
            base
        }
    }

    #[inline(always)]
    pub fn exception_priority(&self, exc_num: u8) -> i16 {
        match exc_num {
            1 => -3, // Reset
            2 => -2, // NMI
            3 => -1, // HardFault
            11 => ((self.shpr2 >> 24) & 0xC0) as i16, // SVCall
            14 => ((self.shpr3 >> 16) & 0xC0) as i16, // PendSV
            15 => ((self.shpr3 >> 24) & 0xC0) as i16, // SysTick
            16..=47 => {
                let irq = (exc_num - 16) as usize;
                (self.nvic_ipr[irq] & 0xC0) as i16
            }
            _ => 256,
        }
    }

    pub fn highest_priority_pending_exception(&self) -> Option<u8> {
        let mut best: Option<(u8, i16)> = None;

        let mut check_candidate = |vec_num: u8, prio: i16| {
            match best {
                None => best = Some((vec_num, prio)),
                Some((b_vec, b_prio)) => {
                    if prio < b_prio || (prio == b_prio && vec_num < b_vec) {
                        best = Some((vec_num, prio));
                    }
                }
            }
        };

        // 1. PendSV (Vector 14)
        if self.pendsv_pending {
            check_candidate(VECTOR_PENDSV as u8, self.exception_priority(VECTOR_PENDSV as u8));
        }

        // 2. SysTick (Vector 15)
        if self.systick_pending && (self.systick_ctrl & (1 << 1)) != 0 {
            check_candidate(VECTOR_SYSTICK as u8, self.exception_priority(VECTOR_SYSTICK as u8));
        }

        // 3. EXTI IRQs (if active and unmasked in EXTI_IMR)
        if (self.exti_pr & self.exti_imr & 0x0003) != 0 || (self.nvic_ispr & (1 << 5)) != 0 {
            if (self.nvic_iser & (1 << 5)) != 0 {
                check_candidate(VECTOR_EXTI0_1 as u8, self.exception_priority(VECTOR_EXTI0_1 as u8));
            }
        }
        if (self.exti_pr & self.exti_imr & 0x000C) != 0 || (self.nvic_ispr & (1 << 6)) != 0 {
            if (self.nvic_iser & (1 << 6)) != 0 {
                check_candidate(VECTOR_EXTI2_3 as u8, self.exception_priority(VECTOR_EXTI2_3 as u8));
            }
        }
        if (self.exti_pr & self.exti_imr & 0xFFF0) != 0 || (self.nvic_ispr & (1 << 7)) != 0 {
            if (self.nvic_iser & (1 << 7)) != 0 {
                check_candidate(VECTOR_EXTI4_15 as u8, self.exception_priority(VECTOR_EXTI4_15 as u8));
            }
        }

        // 4. DMA1 Channel 1 (IRQ 9 = Vector 25)
        if ((self.nvic_iser & (1 << 9)) != 0 || (self.dma1_ccr1 & (1 << 1)) != 0)
            && (self.nvic_ispr & (1 << 9)) != 0
        {
            check_candidate(VECTOR_DMA1_CH1 as u8, self.exception_priority(VECTOR_DMA1_CH1 as u8));
        }

        // 5. DMA1 Channel 2 & 3 (IRQ 10 = Vector 26)
        if ((self.nvic_iser & (1 << 10)) != 0 || (self.dma1_ccr3 & (1 << 1)) != 0)
            && (self.nvic_ispr & (1 << 10)) != 0
        {
            check_candidate(VECTOR_DMA1_CH2_3 as u8, self.exception_priority(VECTOR_DMA1_CH2_3 as u8));
        }

        // 6. DMA1 Channel 4 & 5 (IRQ 11 = Vector 27)
        if (self.nvic_iser & (1 << 11)) != 0 && (self.nvic_ispr & (1 << 11)) != 0 {
            check_candidate(VECTOR_DMA1_CH4_5 as u8, self.exception_priority(VECTOR_DMA1_CH4_5 as u8));
        }

        // 7. USART1 (IRQ 27 = Vector 43)
        if self.usart1.is_interrupt_pending() && ((self.nvic_iser & (1 << 27)) != 0 || self.nvic_iser == 0) {
            check_candidate(VECTOR_USART1 as u8, self.exception_priority(VECTOR_USART1 as u8));
        }

        // 8. USART2 (IRQ 28 = Vector 44)
        if self.usart2.is_interrupt_pending() && ((self.nvic_iser & (1 << 28)) != 0 || self.nvic_iser == 0) {
            check_candidate(VECTOR_USART2 as u8, self.exception_priority(VECTOR_USART2 as u8));
        }

        // 9. USART3..5 (IRQ 29 = Vector 45)
        if (self.usart3.is_interrupt_pending()
            || self.usart4.is_interrupt_pending()
            || self.usart5.is_interrupt_pending())
            && ((self.nvic_iser & (1 << 29)) != 0 || self.nvic_iser == 0)
        {
            check_candidate(VECTOR_USART3_8 as u8, self.exception_priority(VECTOR_USART3_8 as u8));
        }

        // 10. ADC (IRQ 12 = Vector 28)
        if ((self.adc_isr & self.adc_ier) != 0)
            && ((self.nvic_iser & (1 << 12)) != 0 || self.nvic_iser == 0)
        {
            check_candidate(VECTOR_ADC as u8, self.exception_priority(VECTOR_ADC as u8));
        }

        // 11. TIM1 BRK/UP/TRG/COM (IRQ 13 = Vector 29)
        if (self.nvic_iser & (1 << 13)) != 0 && (self.tim1_dier & 1) != 0 && (self.tim1_sr & 1) != 0 {
            check_candidate(VECTOR_TIM1_BRK_UP_TRG_COM as u8, self.exception_priority(VECTOR_TIM1_BRK_UP_TRG_COM as u8));
        }

        // 12. TIM2 (IRQ 15 = Vector 31)
        if (self.nvic_iser & (1 << 15)) != 0 && (self.tim2_dier & 1) != 0 && (self.tim2_sr & 1) != 0 {
            check_candidate(VECTOR_TIM2 as u8, self.exception_priority(VECTOR_TIM2 as u8));
        }

        // 13. TIM3 (IRQ 16 = Vector 32)
        if (self.nvic_iser & (1 << 16)) != 0 && (self.tim3_dier & 1) != 0 && (self.tim3_sr & 1) != 0 {
            check_candidate(VECTOR_TIM3 as u8, self.exception_priority(VECTOR_TIM3 as u8));
        }

        // 14. TIM6 (IRQ 17 = Vector 33)
        if (self.nvic_iser & (1 << 17)) != 0 && (self.tim6_dier & 1) != 0 && (self.tim6_sr & 1) != 0 {
            check_candidate(VECTOR_TIM6 as u8, self.exception_priority(VECTOR_TIM6 as u8));
        }

        // 15. TIM7 (IRQ 18 = Vector 34)
        if (self.nvic_iser & (1 << 18)) != 0 && (self.tim7_dier & 1) != 0 && (self.tim7_sr & 1) != 0 {
            check_candidate(VECTOR_TIM7 as u8, self.exception_priority(VECTOR_TIM7 as u8));
        }

        // 16. TIM14 (IRQ 19 = Vector 35)
        if (self.nvic_iser & (1 << 19)) != 0 && (self.tim14_dier & 1) != 0 && (self.tim14_sr & 1) != 0 {
            check_candidate(VECTOR_TIM14 as u8, self.exception_priority(VECTOR_TIM14 as u8));
        }

        // 17. TIM15 (IRQ 20 = Vector 36)
        if (self.nvic_iser & (1 << 20)) != 0 && (self.tim15_dier & 1) != 0 && (self.tim15_sr & 1) != 0 {
            check_candidate(VECTOR_TIM15 as u8, self.exception_priority(VECTOR_TIM15 as u8));
        }

        // 18. TIM16 (IRQ 21 = Vector 37)
        if (self.nvic_iser & (1 << 21)) != 0 && (self.tim16_dier & 1) != 0 && (self.tim16_sr & 1) != 0 {
            check_candidate(VECTOR_TIM16 as u8, self.exception_priority(VECTOR_TIM16 as u8));
        }

        // 19. TIM17 (IRQ 22 = Vector 38)
        if (self.nvic_iser & (1 << 22)) != 0 && (self.tim17_dier & 1) != 0 && (self.tim17_sr & 1) != 0 {
            check_candidate(VECTOR_TIM17 as u8, self.exception_priority(VECTOR_TIM17 as u8));
        }

        // 20. Any other enabled IRQs pending in NVIC_ISPR
        let pending_irqs = self.nvic_ispr & self.nvic_iser;
        if pending_irqs != 0 {
            for irq in 0..32 {
                if (pending_irqs & (1 << irq)) != 0 {
                    let vec_num = (irq + 16) as u8;
                    check_candidate(vec_num, self.exception_priority(vec_num));
                }
            }
        }

        best.map(|(vec_num, _)| vec_num)
    }

    #[inline]
    pub fn read_icsr(&self) -> u32 {
        let vectactive = if let Some(&(exc, _)) = self.active_exceptions.last() {
            (exc as u32) & 0x3F
        } else {
            0
        };
        let rettobase = if self.active_exceptions.len() <= 1 {
            1 << 11
        } else {
            0
        };
        let vectpending = self.highest_priority_pending_exception().unwrap_or(0) as u32;
        let isrpending = if (self.nvic_ispr & self.nvic_iser) != 0 {
            1 << 22
        } else {
            0
        };
        let pendstset = if self.systick_pending { 1 << 26 } else { 0 };
        let pendsvset = if self.pendsv_pending { 1 << 28 } else { 0 };
        vectactive | rettobase | (vectpending << 12) | isrpending | pendstset | pendsvset
    }

    #[inline]
    pub fn has_pending_wakeup(&self) -> bool {
        (self.systick_pending && (self.systick_ctrl & (1 << 1)) != 0)
            || self.pendsv_pending
            || (self.nvic_ispr != 0)
            || self.usart1.is_interrupt_pending()
            || self.usart2.is_interrupt_pending()
            || self.usart3.is_interrupt_pending()
            || self.usart4.is_interrupt_pending()
            || self.usart5.is_interrupt_pending()
            || ((self.adc_isr & self.adc_ier) != 0)
            || (self.exti_pr & self.exti_imr != 0)
    }

    fn read_mmio(&mut self, addr: u32) -> u32 {
        match addr {
            0x4001_0000 => self.syscfg_cfgr1,
            // SYSCFG EXTI (0x40010008..0x40010014) & EXTI (0x40010400..0x40010414)
            0x4001_0008 => self.syscfg_exticr[0] as u32,
            0x4001_000C => self.syscfg_exticr[1] as u32,
            0x4001_0010 => self.syscfg_exticr[2] as u32,
            0x4001_0014 => self.syscfg_exticr[3] as u32,
            0x4001_0400 => self.exti_imr,
            0x4001_0404 => self.exti_emr,
            0x4001_0408 => self.exti_rtsr,
            0x4001_040C => self.exti_ftsr,
            0x4001_0410 => self.exti_swier,
            0x4001_0414 => self.exti_pr,

            // Power & Flash & DBGMCU
            0x4000_7000 => self.pwr_cr,
            0x4000_7004 => self.pwr_csr,
            0x4002_2000 => self.flash_acr,
            0x4001_2200 => 0x1001_6448, // DBGMCU_IDCODE (STM32F072)
            0x4001_2204 => self.dbgmcu_cr,

            0x4002_1000 => self.rcc_cr,
            0x4002_1004 => self.rcc_cfgr,
            0x4002_1008 => self.rcc_cir,
            0x4002_100C => self.rcc_apb2rstr,
            0x4002_1010 => self.rcc_apb1rstr,
            0x4002_1014 => self.rcc_ahbenr,
            0x4002_1018 => self.rcc_apb2enr,
            0x4002_101C => self.rcc_apb1enr,
            0x4002_1020 => self.rcc_bdcr,
            0x4002_1024 => self.rcc_csr,
            0x4002_1028 => self.rcc_ahbrstr,
            0x4002_102C => self.rcc_cfgr2,
            0x4002_1030 => self.rcc_cfgr3,
            0x4002_1034 => self.rcc_cr2,

            0x4001_3008 => self.spi1_sr,
            0x4001_300C => self.spi1_dr,

            0x4000_3800 => self.spi2_cr1,
            0x4000_3804 => self.spi2_cr2,
            0x4000_3808 => self.spi2_sr,
            0x4000_380C => self.spi2_dr,

            // DMA1
            0x4002_0000 => self.dma1_isr,
            0x4002_0008 => self.dma1_ccr1,
            0x4002_000C => self.dma1_cndtr1,
            0x4002_0010 => self.dma1_cpar1,
            0x4002_0014 => self.dma1_cmar1,
            0x4002_001C => self.dma1_ccr2,
            0x4002_0020 => self.dma1_cndtr2,
            0x4002_0024 => self.dma1_cpar2,
            0x4002_0028 => self.dma1_cmar2,
            0x4002_0030 => self.dma1_ccr3,
            0x4002_0034 => self.dma1_cndtr3,
            0x4002_0038 => self.dma1_cpar3,
            0x4002_003C => self.dma1_cmar3,
            0x4002_0044 => self.dma1_ccr4,
            0x4002_0048 => self.dma1_cndtr4,
            0x4002_004C => self.dma1_cpar4,
            0x4002_0050 => self.dma1_cmar4,
            0x4002_0058 => self.dma1_ccr5,
            0x4002_005C => self.dma1_cndtr5,
            0x4002_0060 => self.dma1_cpar5,
            0x4002_0064 => self.dma1_cmar5,

            0x4001_3800 => self.usart1.cr1,
            0x4001_3804 => self.usart1.cr2,
            0x4001_3808 => self.usart1.cr3,
            0x4001_380C => self.usart1.brr,
            0x4001_381C => self.usart1.read_isr(),
            0x4001_3824 => self.usart1.read_rdr(),

            // USART2 (0x40004400)
            0x4000_4400 => self.usart2.cr1,
            0x4000_4404 => self.usart2.cr2,
            0x4000_4408 => self.usart2.cr3,
            0x4000_440C => self.usart2.brr,
            0x4000_441C => self.usart2.read_isr(),
            0x4000_4424 => self.usart2.read_rdr(),

            // USART3 (0x40004800)
            0x4000_4800 => self.usart3.cr1,
            0x4000_4804 => self.usart3.cr2,
            0x4000_4808 => self.usart3.cr3,
            0x4000_480C => self.usart3.brr,
            0x4000_481C => self.usart3.read_isr(),
            0x4000_4824 => self.usart3.read_rdr(),

            // USART4 (0x40004C00)
            0x4000_4C00 => self.usart4.cr1,
            0x4000_4C04 => self.usart4.cr2,
            0x4000_4C08 => self.usart4.cr3,
            0x4000_4C0C => self.usart4.brr,
            0x4000_4C1C => self.usart4.read_isr(),
            0x4000_4C24 => self.usart4.read_rdr(),

            // USART5 (0x40005000)
            0x4000_5000 => self.usart5.cr1,
            0x4000_5004 => self.usart5.cr2,
            0x4000_5008 => self.usart5.cr3,
            0x4000_500C => self.usart5.brr,
            0x4000_501C => self.usart5.read_isr(),
            0x4000_5024 => self.usart5.read_rdr(),

            // GPIO Ports A, B, C, D, F
            0x4800_0000 => self.gpioa_moder,
            0x4800_0004 => self.gpioa_otyper,
            0x4800_0008 => self.gpioa_ospeedr,
            0x4800_000C => self.gpioa_pupdr,
            0x4800_0010 => self.gpioa_idr,
            0x4800_0014 => self.gpioa_odr,
            0x4800_0020 => self.gpioa_afrl,
            0x4800_0024 => self.gpioa_afrh,

            0x4800_0400 => self.gpiob_moder,
            0x4800_0404 => self.gpiob_otyper,
            0x4800_0408 => self.gpiob_ospeedr,
            0x4800_040C => self.gpiob_pupdr,
            0x4800_0410 => self.gpiob_idr,
            0x4800_0414 => self.gpiob_odr,
            0x4800_0420 => self.gpiob_afrl,
            0x4800_0424 => self.gpiob_afrh,

            0x4800_0800 => self.gpioc_moder,
            0x4800_0804 => self.gpioc_otyper,
            0x4800_0808 => self.gpioc_ospeedr,
            0x4800_080C => self.gpioc_pupdr,
            0x4800_0810 => self.gpioc_idr,
            0x4800_0814 => self.gpioc_odr,
            0x4800_0820 => self.gpioc_afrl,
            0x4800_0824 => self.gpioc_afrh,

            0x4800_0C00 => self.gpiod_moder,
            0x4800_0C04 => self.gpiod_otyper,
            0x4800_0C08 => self.gpiod_ospeedr,
            0x4800_0C0C => self.gpiod_pupdr,
            0x4800_0C10 => self.gpiod_idr,
            0x4800_0C14 => self.gpiod_odr,
            0x4800_0C20 => self.gpiod_afrl,
            0x4800_0C24 => self.gpiod_afrh,

            0x4800_1400 => self.gpiof_moder,
            0x4800_1404 => self.gpiof_otyper,
            0x4800_1408 => self.gpiof_ospeedr,
            0x4800_140C => self.gpiof_pupdr,
            0x4800_1410 => self.gpiof_idr,
            0x4800_1414 => self.gpiof_odr,
            0x4800_1420 => self.gpiof_afrl,
            0x4800_1424 => self.gpiof_afrh,

            // Timers
            // TIM1 (0x40012C00)
            0x4001_2C00 => self.tim1_cr1,
            0x4001_2C04 => self.tim1_cr2,
            0x4001_2C08 => self.tim1_smcr,
            0x4001_2C0C => self.tim1_dier,
            0x4001_2C10 => self.tim1_sr,
            0x4001_2C14 => 0, // TIM1_EGR write-only
            0x4001_2C24 => self.tim1_cnt,
            0x4001_2C28 => self.tim1_psc,
            0x4001_2C2C => self.tim1_arr,
            0x4001_2C30 => self.tim1_rcr,
            0x4001_2C34 => self.tim1_ccr1,
            0x4001_2C38 => self.tim1_ccr2,
            0x4001_2C3C => self.tim1_ccr3,
            0x4001_2C40 => self.tim1_ccr4,
            0x4001_2C44 => self.tim1_bdtr,

            // TIM2 (0x40000000)
            0x4000_0000 => self.tim2_cr1,
            0x4000_0004 => self.tim2_cr2,
            0x4000_0008 => self.tim2_smcr,
            0x4000_000C => self.tim2_dier,
            0x4000_0010 => self.tim2_sr,
            0x4000_0014 => 0, // TIM2_EGR write-only
            0x4000_0024 => self.tim2_cnt,
            0x4000_0028 => self.tim2_psc,
            0x4000_002C => self.tim2_arr,
            0x4000_0034 => self.tim2_ccr1,
            0x4000_0038 => self.tim2_ccr2,
            0x4000_003C => self.tim2_ccr3,
            0x4000_0040 => self.tim2_ccr4,

            0x4000_0400 => self.tim3_cr1,
            0x4000_040C => self.tim3_dier,
            0x4000_0410 => self.tim3_sr,
            0x4000_0424 => self.tim3_cnt,
            0x4000_042C => self.tim3_arr,

            0x4000_1000 => self.tim6_cr1,
            0x4000_100C => self.tim6_dier,
            0x4000_1010 => self.tim6_sr,
            0x4000_1024 => self.tim6_cnt,
            0x4000_1028 => self.tim6_psc,
            0x4000_102C => self.tim6_arr,

            // TIM7 (0x40001400)
            0x4000_1400 => self.tim7_cr1,
            0x4000_1404 => self.tim7_cr2,
            0x4000_140C => self.tim7_dier,
            0x4000_1410 => self.tim7_sr,
            0x4000_1414 => 0, // TIM7_EGR write-only
            0x4000_1424 => self.tim7_cnt,
            0x4000_1428 => self.tim7_psc,
            0x4000_142C => self.tim7_arr,

            0x4000_2000 => self.tim14_cr1,
            0x4000_200C => self.tim14_dier,
            0x4000_2010 => self.tim14_sr,
            0x4000_2020 => self.tim14_cnt, // TIM14_CNT mirror if accessed
            0x4000_2024 => self.tim14_cnt,
            0x4000_2028 => self.tim14_psc,
            0x4000_202C => self.tim14_arr,

            // TIM15 (0x40014000)
            0x4001_4000 => self.tim15_cr1,
            0x4001_400C => self.tim15_dier,
            0x4001_4010 => self.tim15_sr,
            0x4001_4024 => self.tim15_cnt,
            0x4001_4028 => self.tim15_psc,
            0x4001_402C => self.tim15_arr,
            0x4001_4034 => self.tim15_ccr1,

            // TIM16 (0x40014400)
            0x4001_4400 => self.tim16_cr1,
            0x4001_440C => self.tim16_dier,
            0x4001_4410 => self.tim16_sr,
            0x4001_4424 => self.tim16_cnt,
            0x4001_4428 => self.tim16_psc,
            0x4001_442C => self.tim16_arr,
            0x4001_4434 => self.tim16_ccr1,

            // TIM17 (0x40014800)
            0x4001_4800 => self.tim17_cr1,
            0x4001_480C => self.tim17_dier,
            0x4001_4810 => self.tim17_sr,
            0x4001_4824 => self.tim17_cnt,
            0x4001_4828 => self.tim17_psc,
            0x4001_482C => self.tim17_arr,
            0x4001_4834 => self.tim17_ccr1,

            // I2C
            0x4000_5400 => self.i2c1_cr1,
            0x4000_5404 => self.i2c1_cr2,
            0x4000_5408 => self.i2c1_oar1,
            0x4000_540C => self.i2c1_oar2,
            0x4000_5410 => self.i2c1_timingr,
            0x4000_5414 => self.i2c1_timeoutr,
            0x4000_5418 => self.i2c1_isr,
            0x4000_5424 => self.i2c1_rxdr,
            0x4000_5428 => self.i2c1_txdr,

            0x4000_5800 => self.i2c2_cr1,
            0x4000_5804 => self.i2c2_cr2,
            0x4000_5808 => self.i2c2_oar1,
            0x4000_580C => self.i2c2_oar2,
            0x4000_5810 => self.i2c2_timingr,
            0x4000_5814 => self.i2c2_timeoutr,
            0x4000_5818 => self.i2c2_isr,
            0x4000_5824 => self.i2c2_rxdr,
            0x4000_5828 => self.i2c2_txdr,

            // ADC
            0x4001_2400 => self.adc_isr,
            0x4001_2404 => self.adc_ier,
            0x4001_2408 => self.adc_cr,
            0x4001_240C => self.adc_cfgr1,
            0x4001_2410 => self.adc_cfgr2,
            0x4001_2414 => self.adc_smpr,
            0x4001_2428 => self.adc_chselr,
            0x4001_2440 => self.adc_dr,
            0x4001_2708 => self.adc_ccr,

            // Cortex-M0 System Control Space (NVIC & SCB - PM0215)
            0xE000_E010 => {
                let ctrl = self.systick_ctrl;
                self.systick_ctrl &= !(1 << 16); // Reading CTRL clears COUNTFLAG
                ctrl
            }
            0xE000_E014 => self.systick_load,
            0xE000_E018 => {
                if self.systick_load > 0 {
                    let reload = self.systick_load + 1;
                    let accum = self.systick_cycle_accum % reload;
                    self.systick_load.saturating_sub(accum)
                } else {
                    0
                }
            }
            0xE000_E01C => 0xC000_0000, // STK_CALIB (NOREF=1, SKEW=1)
            0xE000_E100 => self.nvic_iser,
            0xE000_E180 => self.nvic_iser, // ICER reads back ISER
            0xE000_E200 => self.nvic_ispr,
            0xE000_E280 => self.nvic_ispr, // ICPR reads back ISPR
            0xE000_E400..=0xE000_E41C => {
                let reg = ((addr - 0xE000_E400) / 4) as usize;
                let b0 = self.nvic_ipr[reg * 4] as u32;
                let b1 = self.nvic_ipr[reg * 4 + 1] as u32;
                let b2 = self.nvic_ipr[reg * 4 + 2] as u32;
                let b3 = self.nvic_ipr[reg * 4 + 3] as u32;
                b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
            }
            0xE000_ED00 => 0x410C_C200, // CPUID: Cortex-M0
            0xE000_ED04 => self.read_icsr(),
            0xE000_ED0C => (self.scb_aircr & 0x0000_FFFF) | 0xFA05_0000,
            0xE000_ED10 => self.scb_scr,
            0xE000_ED14 => 0x0000_0204, // CCR: STKALIGN=1, UNALIGN_TRP=1
            0xE000_ED1C => self.shpr2,
            0xE000_ED20 => self.shpr3,
            0xE000_ED24 => 0, // SHCSR
            _ => 0,
        }
    }

    fn write_mmio(&mut self, addr: u32, val: u32, size: usize) {
        let aligned = addr & !3;
        let shift = (addr % 4) * 8;
        let mask = match size {
            1 => 0xFFu32,
            2 => 0xFFFFu32,
            _ => 0xFFFF_FFFFu32,
        };

        let effective_val = if size >= 4 {
            val
        } else {
            let cur = self.read_mmio(aligned);
            (cur & !(mask << shift)) | ((val & mask) << shift)
        };

        match aligned {
            0x4001_0000 => self.syscfg_cfgr1 = effective_val,
            // SYSCFG EXTI (0x40010008..0x40010014) & EXTI (0x40010400..0x40010414)
            0x4001_0008 => self.syscfg_exticr[0] = effective_val as u16,
            0x4001_000C => self.syscfg_exticr[1] = effective_val as u16,
            0x4001_0010 => self.syscfg_exticr[2] = effective_val as u16,
            0x4001_0014 => self.syscfg_exticr[3] = effective_val as u16,
            0x4001_0400 => self.exti_imr = effective_val,
            0x4001_0404 => self.exti_emr = effective_val,
            0x4001_0408 => self.exti_rtsr = effective_val,
            0x4001_040C => self.exti_ftsr = effective_val,
            0x4001_0410 => {
                self.exti_swier = effective_val;
                self.exti_pr |= effective_val;
            }
            0x4001_0414 => self.exti_pr &= !effective_val, // RC_W1

            // Power, Flash, DBGMCU
            0x4000_7000 => self.pwr_cr = effective_val,
            0x4000_7004 => self.pwr_csr = effective_val,
            0x4002_2000 => self.flash_acr = effective_val,
            0x4001_2204 => self.dbgmcu_cr = effective_val,

            0x4002_1000 => self.rcc_cr = effective_val,
            0x4002_1004 => self.rcc_cfgr = effective_val,
            0x4002_1008 => self.rcc_cir = effective_val,
            0x4002_100C => self.rcc_apb2rstr = effective_val,
            0x4002_1010 => self.rcc_apb1rstr = effective_val,
            0x4002_1014 => self.rcc_ahbenr = effective_val,
            0x4002_1018 => self.rcc_apb2enr = effective_val,
            0x4002_101C => self.rcc_apb1enr = effective_val,
            0x4002_1020 => self.rcc_bdcr = effective_val,
            0x4002_1024 => self.rcc_csr = effective_val,
            0x4002_1028 => self.rcc_ahbrstr = effective_val,
            0x4002_102C => self.rcc_cfgr2 = effective_val,
            0x4002_1030 => self.rcc_cfgr3 = effective_val,
            0x4002_1034 => self.rcc_cr2 = effective_val,

            0x4000_3800 => self.spi2_cr1 = effective_val,
            0x4000_3804 => self.spi2_cr2 = effective_val,
            0x4000_3808 => self.spi2_sr = effective_val,
            0x4000_380C => self.spi2_dr = effective_val,

            0x4001_3800 => self.usart1.cr1 = effective_val,
            0x4001_3804 => self.usart1.cr2 = effective_val,
            0x4001_3808 => self.usart1.cr3 = effective_val,
            0x4001_380C => self.usart1.brr = effective_val,
            0x4001_3818 => {}, // USART1 RQR (Request Register)
            0x4001_3820 => self.usart1.write_icr(effective_val),
            0x4001_3828 => self.usart1.write_tdr((val & 0xFF) as u8),

            0x4000_4400 => self.usart2.cr1 = effective_val,
            0x4000_4404 => self.usart2.cr2 = effective_val,
            0x4000_4408 => self.usart2.cr3 = effective_val,
            0x4000_440C => self.usart2.brr = effective_val,
            0x4000_4418 => {},
            0x4000_4420 => self.usart2.write_icr(effective_val),
            0x4000_4428 => self.usart2.write_tdr((val & 0xFF) as u8),

            0x4000_4800 => self.usart3.cr1 = effective_val,
            0x4000_4804 => self.usart3.cr2 = effective_val,
            0x4000_4808 => self.usart3.cr3 = effective_val,
            0x4000_480C => self.usart3.brr = effective_val,
            0x4000_4818 => {},
            0x4000_4820 => self.usart3.write_icr(effective_val),
            0x4000_4828 => self.usart3.write_tdr((val & 0xFF) as u8),

            0x4000_4C00 => self.usart4.cr1 = effective_val,
            0x4000_4C04 => self.usart4.cr2 = effective_val,
            0x4000_4C08 => self.usart4.cr3 = effective_val,
            0x4000_4C0C => self.usart4.brr = effective_val,
            0x4000_4C18 => {},
            0x4000_4C20 => self.usart4.write_icr(effective_val),
            0x4000_4C28 => self.usart4.write_tdr((val & 0xFF) as u8),

            0x4000_5000 => self.usart5.cr1 = effective_val,
            0x4000_5004 => self.usart5.cr2 = effective_val,
            0x4000_5008 => self.usart5.cr3 = effective_val,
            0x4000_500C => self.usart5.brr = effective_val,
            0x4000_5018 => {},
            0x4000_5020 => self.usart5.write_icr(effective_val),
            0x4000_5028 => self.usart5.write_tdr((val & 0xFF) as u8),

            // GPIO Port A
            0x4800_0000 => self.gpioa_moder = effective_val,
            0x4800_0004 => self.gpioa_otyper = effective_val,
            0x4800_0008 => self.gpioa_ospeedr = effective_val,
            0x4800_000C => self.gpioa_pupdr = effective_val,
            0x4800_0014 => self.gpioa_odr = effective_val,
            0x4800_0018 => {
                let set = effective_val & 0xFFFF;
                let reset = (effective_val >> 16) & 0xFFFF;
                self.gpioa_odr = (self.gpioa_odr | set) & !reset;
            }
            0x4800_0020 => self.gpioa_afrl = effective_val,
            0x4800_0024 => self.gpioa_afrh = effective_val,
            0x4800_0028 => {
                self.gpioa_odr &= !(effective_val & 0xFFFF);
            }

            // GPIO Port B
            0x4800_0400 => self.gpiob_moder = effective_val,
            0x4800_0404 => self.gpiob_otyper = effective_val,
            0x4800_0408 => self.gpiob_ospeedr = effective_val,
            0x4800_040C => self.gpiob_pupdr = effective_val,
            0x4800_0414 => self.gpiob_odr = effective_val,
            0x4800_0418 => {
                let set = effective_val & 0xFFFF;
                let reset = (effective_val >> 16) & 0xFFFF;
                self.gpiob_odr = (self.gpiob_odr | set) & !reset;
            }
            0x4800_0420 => self.gpiob_afrl = effective_val,
            0x4800_0424 => self.gpiob_afrh = effective_val,
            0x4800_0428 => {
                self.gpiob_odr &= !(effective_val & 0xFFFF);
            }

            // GPIO Port C
            0x4800_0800 => self.gpioc_moder = effective_val,
            0x4800_0804 => self.gpioc_otyper = effective_val,
            0x4800_0808 => self.gpioc_ospeedr = effective_val,
            0x4800_080C => self.gpioc_pupdr = effective_val,
            0x4800_0814 => self.gpioc_odr = effective_val,
            0x4800_0818 => {
                let set = effective_val & 0xFFFF;
                let reset = (effective_val >> 16) & 0xFFFF;
                self.gpioc_odr = (self.gpioc_odr | set) & !reset;
            }
            0x4800_0820 => self.gpioc_afrl = effective_val,
            0x4800_0824 => self.gpioc_afrh = effective_val,
            0x4800_0828 => {
                self.gpioc_odr &= !(effective_val & 0xFFFF);
            }

            // GPIO Port D
            0x4800_0C00 => self.gpiod_moder = effective_val,
            0x4800_0C04 => self.gpiod_otyper = effective_val,
            0x4800_0C08 => self.gpiod_ospeedr = effective_val,
            0x4800_0C0C => self.gpiod_pupdr = effective_val,
            0x4800_0C14 => self.gpiod_odr = effective_val,
            0x4800_0C18 => {
                let set = effective_val & 0xFFFF;
                let reset = (effective_val >> 16) & 0xFFFF;
                self.gpiod_odr = (self.gpiod_odr | set) & !reset;
            }
            0x4800_0C20 => self.gpiod_afrl = effective_val,
            0x4800_0C24 => self.gpiod_afrh = effective_val,
            0x4800_0C28 => {
                self.gpiod_odr &= !(effective_val & 0xFFFF);
            }

            // GPIO Port F
            0x4800_1400 => self.gpiof_moder = effective_val,
            0x4800_1404 => self.gpiof_otyper = effective_val,
            0x4800_1408 => self.gpiof_ospeedr = effective_val,
            0x4800_140C => self.gpiof_pupdr = effective_val,
            0x4800_1414 => self.gpiof_odr = effective_val,
            0x4800_1418 => {
                let set = effective_val & 0xFFFF;
                let reset = (effective_val >> 16) & 0xFFFF;
                self.gpiof_odr = (self.gpiof_odr | set) & !reset;
            }
            0x4800_1420 => self.gpiof_afrl = effective_val,
            0x4800_1424 => self.gpiof_afrh = effective_val,
            0x4800_1428 => {
                self.gpiof_odr &= !(effective_val & 0xFFFF);
            }

            // Timers
            // TIM1 (0x40012C00)
            0x4001_2C00 => self.tim1_cr1 = effective_val,
            0x4001_2C04 => self.tim1_cr2 = effective_val,
            0x4001_2C08 => self.tim1_smcr = effective_val,
            0x4001_2C0C => self.tim1_dier = effective_val,
            0x4001_2C10 => self.tim1_sr &= effective_val,
            0x4001_2C14 => {
                if (effective_val & 1) != 0 {
                    self.tim1_cnt = 0;
                    self.tim1_sr |= 1;
                }
            }
            0x4001_2C24 => self.tim1_cnt = effective_val,
            0x4001_2C28 => self.tim1_psc = effective_val,
            0x4001_2C2C => self.tim1_arr = effective_val,
            0x4001_2C30 => self.tim1_rcr = effective_val,
            0x4001_2C34 => self.tim1_ccr1 = effective_val,
            0x4001_2C38 => self.tim1_ccr2 = effective_val,
            0x4001_2C3C => self.tim1_ccr3 = effective_val,
            0x4001_2C40 => self.tim1_ccr4 = effective_val,
            0x4001_2C44 => self.tim1_bdtr = effective_val,

            // TIM2 (0x40000000)
            0x4000_0000 => self.tim2_cr1 = effective_val,
            0x4000_0004 => self.tim2_cr2 = effective_val,
            0x4000_0008 => self.tim2_smcr = effective_val,
            0x4000_000C => self.tim2_dier = effective_val,
            0x4000_0010 => self.tim2_sr &= effective_val,
            0x4000_0014 => {
                if (effective_val & 1) != 0 {
                    self.tim2_cnt = 0;
                    self.tim2_sr |= 1;
                }
            }
            0x4000_0024 => self.tim2_cnt = effective_val,
            0x4000_0028 => self.tim2_psc = effective_val,
            0x4000_002C => self.tim2_arr = effective_val,
            0x4000_0034 => self.tim2_ccr1 = effective_val,
            0x4000_0038 => self.tim2_ccr2 = effective_val,
            0x4000_003C => self.tim2_ccr3 = effective_val,
            0x4000_0040 => self.tim2_ccr4 = effective_val,

            0x4000_0400 => self.tim3_cr1 = effective_val,
            0x4000_040C => self.tim3_dier = effective_val,
            0x4000_0410 => self.tim3_sr &= effective_val,
            0x4000_0424 => self.tim3_cnt = effective_val,
            0x4000_042C => self.tim3_arr = effective_val,
            0x4000_1000 => self.tim6_cr1 = effective_val,
            0x4000_100C => self.tim6_dier = effective_val,
            0x4000_1010 => self.tim6_sr &= effective_val,
            0x4000_1014 => {
                if (effective_val & 1) != 0 {
                    self.tim6_cnt = 0;
                    self.tim6_sr |= 1;
                }
            }
            0x4000_1024 => self.tim6_cnt = effective_val,
            0x4000_1028 => self.tim6_psc = effective_val,
            0x4000_102C => self.tim6_arr = effective_val,

            // TIM7 (0x40001400)
            0x4000_1400 => self.tim7_cr1 = effective_val,
            0x4000_1404 => self.tim7_cr2 = effective_val,
            0x4000_140C => self.tim7_dier = effective_val,
            0x4000_1410 => self.tim7_sr &= effective_val,
            0x4000_1414 => {
                if (effective_val & 1) != 0 {
                    self.tim7_cnt = 0;
                    self.tim7_sr |= 1;
                }
            }
            0x4000_1424 => self.tim7_cnt = effective_val,
            0x4000_1428 => self.tim7_psc = effective_val,
            0x4000_142C => self.tim7_arr = effective_val,

            0x4000_2000 => self.tim14_cr1 = effective_val,
            0x4000_200C => self.tim14_dier = effective_val,
            0x4000_2010 => self.tim14_sr &= effective_val,
            0x4000_2024 => self.tim14_cnt = effective_val,
            0x4000_2028 => self.tim14_psc = effective_val,
            0x4000_202C => self.tim14_arr = effective_val,
            // I2C1
            0x4000_5400 => self.i2c1_cr1 = effective_val,
            0x4000_5404 => self.i2c1_cr2 = effective_val,
            0x4000_5408 => self.i2c1_oar1 = effective_val,
            0x4000_540C => self.i2c1_oar2 = effective_val,
            0x4000_5410 => self.i2c1_timingr = effective_val,
            0x4000_5414 => self.i2c1_timeoutr = effective_val,
            0x4000_541C => self.i2c1_isr &= !effective_val, // I2C_ICR clears bits in ISR
            0x4000_5428 => self.i2c1_txdr = effective_val,

            // I2C2
            0x4000_5800 => self.i2c2_cr1 = effective_val,
            0x4000_5804 => self.i2c2_cr2 = effective_val,
            0x4000_5808 => self.i2c2_oar1 = effective_val,
            0x4000_580C => self.i2c2_oar2 = effective_val,
            0x4000_5810 => self.i2c2_timingr = effective_val,
            0x4000_5814 => self.i2c2_timeoutr = effective_val,
            0x4000_581C => self.i2c2_isr &= !effective_val,
            0x4000_5828 => self.i2c2_txdr = effective_val,

            // ADC
            0x4001_2400 => self.adc_isr &= !effective_val,
            0x4001_2404 => self.adc_ier = effective_val,
            0x4001_2408 => {
                self.adc_cr = effective_val;
                if (effective_val & 1) != 0 {
                    // ADEN set: ADRDY bit asserted in ISR
                    self.adc_isr |= 1;
                }
                if (effective_val & (1 << 2)) != 0 {
                    // ADSTART set: conversion completes, assert EOC and EOSEQ
                    self.adc_isr |= (1 << 2) | (1 << 3);
                    if (self.adc_ier & ((1 << 2) | (1 << 3))) != 0 {
                        self.nvic_ispr |= 1 << 12; // IRQ 12 (ADC_COMP)
                        self.sleeping = false;
                    }
                }
            }
            0x4001_240C => self.adc_cfgr1 = effective_val,
            0x4001_2410 => self.adc_cfgr2 = effective_val,
            0x4001_2414 => self.adc_smpr = effective_val,
            0x4001_2428 => self.adc_chselr = effective_val,
            0x4001_2708 => self.adc_ccr = effective_val,
            0x4001_3008 => self.spi1_sr = effective_val,
            0x4001_300C => {
                let b = (val & 0xFF) as u8;
                self.spi1_dr = effective_val;
                self.spi1_tx_bytes.push(b);
            }
            0x4002_0004 => {
                // DMA1 IFCR: clear corresponding bits in dma1_isr
                let clear_mask = (val & mask) << shift;
                self.dma1_isr &= !clear_mask;
                if (clear_mask & 0xFu32) != 0 {
                    self.nvic_ispr &= !(1 << 9);
                }
                if (clear_mask & (0xFu32 << 4)) != 0 || (clear_mask & (0xFu32 << 8)) != 0 {
                    self.nvic_ispr &= !(1 << 10);
                }
                if (clear_mask & (0xFu32 << 12)) != 0 || (clear_mask & (0xFu32 << 16)) != 0 {
                    self.nvic_ispr &= !(1 << 11);
                }
            }
            0x4002_0008 => self.dma1_ccr1 = effective_val,
            0x4002_000C => {
                self.dma1_cndtr1 = effective_val;
                self.dma1_reload_cndtr1 = effective_val;
            }
            0x4002_0010 => self.dma1_cpar1 = effective_val,
            0x4002_0014 => {
                self.dma1_cmar1 = effective_val;
                self.dma1_base_cmar1 = effective_val;
            }
            0x4002_001C => {
                // DMA1 CCR2
                let prev_en = (self.dma1_ccr2 & 1) != 0;
                self.dma1_ccr2 = effective_val;
                let new_en = (effective_val & 1) != 0;
                if new_en && !prev_en {
                    self.dma1_completion_delay2 = 64;
                } else if !new_en {
                    self.dma1_completion_delay2 = 0;
                }
            }
            0x4002_0020 => self.dma1_cndtr2 = effective_val,
            0x4002_0024 => self.dma1_cpar2 = effective_val,
            0x4002_0028 => self.dma1_cmar2 = effective_val,
            0x4002_0030 => {
                // DMA1 CCR3
                let prev_en = (self.dma1_ccr3 & 1) != 0;
                self.dma1_ccr3 = effective_val;
                let new_en = (effective_val & 1) != 0;
                if new_en && !prev_en {
                    // Delay setting TCIF3 and pending IRQ 10 by 64 cycles so setup code can finish
                    self.dma1_completion_delay = 64;
                } else if !new_en {
                    self.dma1_completion_delay = 0;
                }
            }
            0x4002_0034 => self.dma1_cndtr3 = effective_val,
            0x4002_0038 => self.dma1_cpar3 = effective_val,
            0x4002_003C => self.dma1_cmar3 = effective_val,
            0x4002_0044 => {
                // DMA1 CCR4
                let prev_en = (self.dma1_ccr4 & 1) != 0;
                self.dma1_ccr4 = effective_val;
                let new_en = (effective_val & 1) != 0;
                if new_en && !prev_en {
                    self.dma1_completion_delay4 = 64;
                } else if !new_en {
                    self.dma1_completion_delay4 = 0;
                }
            }
            0x4002_0048 => self.dma1_cndtr4 = effective_val,
            0x4002_004C => self.dma1_cpar4 = effective_val,
            0x4002_0050 => self.dma1_cmar4 = effective_val,
            0x4002_0058 => {
                // DMA1 CCR5
                let prev_en = (self.dma1_ccr5 & 1) != 0;
                self.dma1_ccr5 = effective_val;
                let new_en = (effective_val & 1) != 0;
                if new_en && !prev_en {
                    self.dma1_completion_delay5 = 64;
                } else if !new_en {
                    self.dma1_completion_delay5 = 0;
                }
            }
            0x4002_005C => self.dma1_cndtr5 = effective_val,
            0x4002_0060 => self.dma1_cpar5 = effective_val,
            0x4002_0064 => self.dma1_cmar5 = effective_val,

            // TIM15
            0x4001_4000 => self.tim15_cr1 = effective_val,
            0x4001_400C => self.tim15_dier = effective_val,
            0x4001_4010 => self.tim15_sr &= effective_val,
            0x4001_4024 => self.tim15_cnt = effective_val,
            0x4001_4028 => self.tim15_psc = effective_val,
            0x4001_402C => self.tim15_arr = effective_val,
            0x4001_4034 => self.tim15_ccr1 = effective_val,

            // TIM16
            0x4001_4400 => self.tim16_cr1 = effective_val,
            0x4001_440C => self.tim16_dier = effective_val,
            0x4001_4410 => self.tim16_sr &= effective_val,
            0x4001_4424 => self.tim16_cnt = effective_val,
            0x4001_4428 => self.tim16_psc = effective_val,
            0x4001_442C => self.tim16_arr = effective_val,
            0x4001_4434 => self.tim16_ccr1 = effective_val,

            // TIM17
            0x4001_4800 => self.tim17_cr1 = effective_val,
            0x4001_480C => self.tim17_dier = effective_val,
            0x4001_4810 => self.tim17_sr &= effective_val,
            0x4001_4824 => self.tim17_cnt = effective_val,
            0x4001_4828 => self.tim17_psc = effective_val,
            0x4001_482C => self.tim17_arr = effective_val,
            0x4001_4834 => self.tim17_ccr1 = effective_val,

            // Cortex-M0 System Control Space (NVIC & SCB - PM0215)
            0xE000_E010 => self.systick_ctrl = effective_val,
            0xE000_E014 => self.systick_load = effective_val,
            0xE000_E018 => {
                self.systick_val = 0;
                self.systick_cycle_accum = 0;
                self.systick_ctrl &= !(1 << 16);
            }
            0xE000_E100 => self.nvic_iser |= (val & mask) << shift,
            0xE000_E180 => self.nvic_iser &= !((val & mask) << shift),
            0xE000_E200 => self.nvic_ispr |= (val & mask) << shift,
            0xE000_E280 => self.nvic_ispr &= !((val & mask) << shift),
            0xE000_E400..=0xE000_E41C => {
                let reg = ((aligned - 0xE000_E400) / 4) as usize;
                self.nvic_ipr[reg * 4] = (effective_val & 0xC0) as u8;
                self.nvic_ipr[reg * 4 + 1] = ((effective_val >> 8) & 0xC0) as u8;
                self.nvic_ipr[reg * 4 + 2] = ((effective_val >> 16) & 0xC0) as u8;
                self.nvic_ipr[reg * 4 + 3] = ((effective_val >> 24) & 0xC0) as u8;
            }
            0xE000_ED04 => {
                if (effective_val & (1 << 25)) != 0 {
                    self.systick_pending = false;
                }
                if (effective_val & (1 << 26)) != 0 {
                    self.systick_pending = true;
                }
                if (effective_val & (1 << 27)) != 0 {
                    self.pendsv_pending = false;
                }
                if (effective_val & (1 << 28)) != 0 {
                    self.pendsv_pending = true;
                }
            }
            0xE000_ED0C => {
                if (effective_val >> 16) == 0x05FA {
                    if (effective_val & (1 << 2)) != 0 {
                        self.reset();
                    } else {
                        if (effective_val & (1 << 1)) != 0 {
                            self.active_exceptions.clear();
                            self.in_handler_mode = false;
                        }
                        self.scb_aircr = effective_val & 0x0000_FFFF;
                    }
                }
            }
            0xE000_ED10 => self.scb_scr = effective_val & 0x16,
            0xE000_ED14 => {}, // CCR read-only
            0xE000_ED1C => self.shpr2 = effective_val & 0xC000_0000,
            0xE000_ED20 => self.shpr3 = effective_val & 0xC0C0_0000,
            0xE000_ED24 => {}, // SHCSR
            _ => {}
        }
    }

    // --- Interrupt Handling (ARM Cortex-M Hardware Stacking) ---

    pub fn poll_pending_exception(&mut self) -> Option<u8> {
        if !self.has_pending_wakeup() {
            return None;
        }
        let best_vec = self.highest_priority_pending_exception()?;
        let best_prio = self.exception_priority(best_vec);
        if best_prio < self.current_execution_priority() {
            Some(best_vec)
        } else {
            None
        }
    }

    pub fn feed_usart2_rx(&mut self, byte: u8) {
        self.usart2.rx.push_back(byte);
        if (self.usart2.cr1 & (1 << 5)) != 0 {
            self.nvic_ispr |= 1 << 28;
            self.sleeping = false;
        }
    }

    pub fn drain_usart2_tx(&mut self) -> Vec<u8> {
        self.usart2.tx.drain(..).collect()
    }

    pub fn set_pin_level(&mut self, port: char, pin: u8, high: bool) {
        let mask = 1u32 << (pin & 15);
        let idr = match port {
            'A' | 'a' => &mut self.gpioa_idr,
            'B' | 'b' => &mut self.gpiob_idr,
            'C' | 'c' => &mut self.gpioc_idr,
            'D' | 'd' => &mut self.gpiod_idr,
            'F' | 'f' => &mut self.gpiof_idr,
            _ => return,
        };
        let prev_high = (*idr & mask) != 0;
        if high {
            *idr |= mask;
        } else {
            *idr &= !mask;
        }

        // Check EXTI edge trigger for mapped port (RM0360 Section 11.2)
        let line = (pin & 15) as usize;
        let exticr_idx = line / 4;
        let exticr_shift = (line % 4) * 4;
        let mapped_port = (self.syscfg_exticr[exticr_idx] >> exticr_shift) & 0x7;
        let port_code = match port {
            'A' | 'a' => 0,
            'B' | 'b' => 1,
            'C' | 'c' => 2,
            'D' | 'd' => 3,
            'F' | 'f' => 5,
            _ => 0xFF,
        };
        if port_code == mapped_port {
            let rising = !prev_high && high;
            let falling = prev_high && !high;
            if (rising && (self.exti_rtsr & (1 << line)) != 0)
                || (falling && (self.exti_ftsr & (1 << line)) != 0)
            {
                self.exti_pr |= 1 << line;
                if line <= 1 {
                    self.nvic_ispr |= 1 << 5; // EXTI0_1 (IRQ 5)
                } else if line <= 3 {
                    self.nvic_ispr |= 1 << 6; // EXTI2_3 (IRQ 6)
                } else {
                    self.nvic_ispr |= 1 << 7; // EXTI4_15 (IRQ 7)
                }
                self.sleeping = false;
            }
        }
    }

    pub fn dispatch_vector(&mut self, vector_num: u32) -> bool {
        let isr_addr = self.read32(vector_num * 4);
        if isr_addr == 0 || isr_addr == 0xFFFF_FFFF {
            return false;
        }

        let prio = self.exception_priority(vector_num as u8);
        if vector_num != 11 && prio >= self.current_execution_priority() {
            return false;
        }

        self.sleeping = false;
        self.enter_exception(vector_num as u8);
        true
    }

    #[inline(always)]
    pub fn sync_sp_to_active(&mut self) {
        let sp = self.regs[13] & !3;
        self.regs[13] = sp;
        if self.in_handler_mode || !self.control_spsel {
            self.msp = sp;
        } else {
            self.psp = sp;
        }
    }

    /// ARMv6-M Hardware Exception Entry (pushes 8-word frame onto active stack)
    pub fn enter_exception(&mut self, exc_num: u8) {
        let prio = self.exception_priority(exc_num);
        self.sync_sp_to_active();
        let sp = self.regs[13];
        let frame_ptr = (sp.wrapping_sub(32)) & !7; // 8-byte stack alignment
        let return_pc = self.regs[15];
        let stacked_xpsr = (self.pack_xpsr() & !(1 << 9)) | ((sp & 4) << 7); // Bit 9 records stack alignment

        self.write32(frame_ptr, self.regs[0]);
        self.write32(frame_ptr + 4, self.regs[1]);
        self.write32(frame_ptr + 8, self.regs[2]);
        self.write32(frame_ptr + 12, self.regs[3]);
        self.write32(frame_ptr + 16, self.regs[12]);
        self.write32(frame_ptr + 20, self.regs[14]);
        self.write32(frame_ptr + 24, return_pc & !1);
        self.write32(frame_ptr + 28, stacked_xpsr);

        // Save updated SP and switch to MSP for Handler mode
        if self.in_handler_mode {
            self.msp = frame_ptr;
            self.regs[14] = 0xFFFF_FFF1; // Return to Handler mode (MSP)
        } else if self.control_spsel {
            self.psp = frame_ptr;
            self.regs[14] = 0xFFFF_FFFD; // Return to Thread mode (PSP)
        } else {
            self.msp = frame_ptr;
            self.regs[14] = 0xFFFF_FFF9; // Return to Thread mode (MSP)
        }

        self.active_exceptions.push((exc_num, prio));
        self.in_handler_mode = true;
        self.regs[13] = self.msp & !3;
        self.xpsr = (self.xpsr & !0x3F) | (exc_num as u32);

        // Read vector from 0x0000_0000 (which aliases Flash or SRAM via SYSCFG_CFGR1)
        let vector_addr = (exc_num as u32) * 4;
        let target_isr = self.read32(vector_addr);
        self.regs[15] = target_isr & !1;
        self.cycles += 12;

        if exc_num >= 16 && exc_num < 48 {
            self.nvic_ispr &= !(1 << (exc_num - 16));
        } else if exc_num == VECTOR_SYSTICK as u8 {
            self.systick_pending = false;
        } else if exc_num == VECTOR_PENDSV as u8 {
            self.pendsv_pending = false;
        }
    }

    /// ARMv6-M Hardware Exception Return (triggered when BX/POP sets PC >= 0xFFFFFFF0)
    pub fn exit_exception(&mut self, exc_return: u32) {
        // In Handler Mode, active SP is MSP (which may have been incremented by POP {..., PC})
        self.msp = self.regs[13] & !3;
        self.sync_sp_to_active();

        let (use_psp, return_to_handler) = match exc_return & 0xF {
            0x1 => (false, true),  // Handler mode (MSP)
            0x9 => (false, false), // Thread mode (MSP)
            0xD => (true, false),  // Thread mode (PSP)
            _ => (false, false),
        };

        let frame_ptr = if use_psp { self.psp } else { self.msp };

        self.regs[0] = self.read32(frame_ptr);
        self.regs[1] = self.read32(frame_ptr + 4);
        self.regs[2] = self.read32(frame_ptr + 8);
        self.regs[3] = self.read32(frame_ptr + 12);
        self.regs[12] = self.read32(frame_ptr + 16);
        self.regs[14] = self.read32(frame_ptr + 20);
        self.regs[15] = self.read32(frame_ptr + 24) & !1;
        let popped_xpsr = self.read32(frame_ptr + 28);

        let align_pad = if (popped_xpsr & (1 << 9)) != 0 { 4 } else { 0 };
        let restored_sp = (frame_ptr.wrapping_add(32).wrapping_add(align_pad)) & !3;

        self.active_exceptions.pop();
        self.in_handler_mode = return_to_handler && !self.active_exceptions.is_empty();
        self.control_spsel = use_psp;
        if use_psp {
            self.psp = restored_sp;
            self.regs[13] = self.psp & !3;
        } else {
            self.msp = restored_sp;
            self.regs[13] = self.msp & !3;
        }

        let returning_exc_num = if let Some(&(exc, _)) = self.active_exceptions.last() {
            exc as u32
        } else {
            0
        };
        self.unpack_xpsr((popped_xpsr & 0xF000_0000) | (1 << 24) | returning_exc_num);
        self.cycles += 10;
    }

    #[inline(always)]
    pub fn branch_write_pc(&mut self, target: u32) {
        if (target & 0xF000_0000) == 0xF000_0000 {
            self.exit_exception(target);
        } else {
            self.regs[15] = target & !1;
        }
    }

    #[inline(always)]
    pub fn set_nz(&mut self, res: u32) {
        self.flag_n = (res & 0x8000_0000) != 0;
        self.flag_z = res == 0;
        self.xpsr &= !(0b11 << 30);
        if self.flag_n {
            self.xpsr |= 1 << 31;
        }
        if self.flag_z {
            self.xpsr |= 1 << 30;
        }
    }

    #[inline(always)]
    pub fn set_nzc(&mut self, res: u32, c: bool) {
        self.set_nz(res);
        self.flag_c = c;
        if c {
            self.xpsr |= 1 << 29;
        } else {
            self.xpsr &= !(1 << 29);
        }
    }

    #[inline(always)]
    pub fn set_nzcv(&mut self, res: u32, c: bool, v: bool) {
        self.set_nzc(res, c);
        self.flag_v = v;
        if v {
            self.xpsr |= 1 << 28;
        } else {
            self.xpsr &= !(1 << 28);
        }
    }

    #[inline(always)]
    pub fn pack_xpsr(&self) -> u32 {
        let mut val = (self.xpsr & 0x0FFF_FFFF) | (1 << 24); // Thumb bit
        if self.flag_n {
            val |= 1 << 31;
        }
        if self.flag_z {
            val |= 1 << 30;
        }
        if self.flag_c {
            val |= 1 << 29;
        }
        if self.flag_v {
            val |= 1 << 28;
        }
        val
    }

    #[inline(always)]
    pub fn unpack_xpsr(&mut self, val: u32) {
        self.xpsr = val;
        self.flag_n = (val & (1 << 31)) != 0;
        self.flag_z = (val & (1 << 30)) != 0;
        self.flag_c = (val & (1 << 29)) != 0;
        self.flag_v = (val & (1 << 28)) != 0;
    }

    // --- Core Instruction Stepping Loop (Thumb-1 Decode) ---

    #[inline(always)]
    pub fn step(&mut self) -> usize {
        let pc = self.regs[15];
        if pc >= 0xFFFF_FFE0 {
            self.exit_exception(pc);
            return 12;
        }

        let insn = if pc >= FLASH_BASE && (pc + 1) < FLASH_BASE + self.flash.len() as u32 {
            let off = (pc - FLASH_BASE) as usize;
            u16::from_le_bytes([self.flash[off], self.flash[off + 1]]) as u32
        } else {
            self.read16(pc) as u32
        };
        self.regs[15] = pc + 2;

        let op_top5 = (insn >> 11) & 0x1F;
        match op_top5 {
            // 0b000_00: LSLS Rd, Rm, #imm5 (or MOVS Rd, Rm if imm5 == 0)
            0b00000 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rm = (insn >> 3) & 0x7;
                let rd = insn & 0x7;
                let val = self.regs[rm as usize];
                if imm5 == 0 {
                    self.regs[rd as usize] = val;
                    self.flag_n = (val >> 31) != 0;
                    self.flag_z = val == 0;
                } else {
                    self.flag_c = ((val >> (32 - imm5)) & 1) != 0;
                    let res = val << imm5;
                    self.regs[rd as usize] = res;
                    self.flag_n = (res >> 31) != 0;
                    self.flag_z = res == 0;
                }
                1
            }
            // 0b000_01: LSRS Rd, Rm, #imm5
            0b00001 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rm = (insn >> 3) & 0x7;
                let rd = insn & 0x7;
                let val = self.regs[rm as usize];
                if imm5 == 0 {
                    // LSRS #32
                    self.flag_c = (val >> 31) != 0;
                    self.regs[rd as usize] = 0;
                    self.flag_n = false;
                    self.flag_z = true;
                } else {
                    self.flag_c = ((val >> (imm5 - 1)) & 1) != 0;
                    let res = val >> imm5;
                    self.regs[rd as usize] = res;
                    self.flag_n = (res >> 31) != 0;
                    self.flag_z = res == 0;
                }
                1
            }
            // 0b000_10: ASRS Rd, Rm, #imm5
            0b00010 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rm = (insn >> 3) & 0x7;
                let rd = insn & 0x7;
                let val = self.regs[rm as usize] as i32;
                if imm5 == 0 {
                    self.flag_c = (val as u32 >> 31) != 0;
                    let res = if val < 0 { 0xFFFF_FFFF } else { 0 };
                    self.regs[rd as usize] = res;
                    self.flag_n = (res >> 31) != 0;
                    self.flag_z = res == 0;
                } else {
                    self.flag_c = ((val >> (imm5 - 1)) & 1) != 0;
                    let res = (val >> imm5) as u32;
                    self.regs[rd as usize] = res;
                    self.flag_n = (res >> 31) != 0;
                    self.flag_z = res == 0;
                }
                1
            }
            // 0b000_11: Add/Subtract
            0b00011 => {
                let op = (insn >> 9) & 0x3;
                let rn_or_imm = (insn >> 6) & 0x7;
                let rn = (insn >> 3) & 0x7;
                let rd = insn & 0x7;
                match op {
                    0 => {
                        // ADDS Rd, Rn, Rm
                        let rm = rn_or_imm;
                        let a = self.regs[rn as usize];
                        let b = self.regs[rm as usize];
                        let res = self.alu_add(a, b, false);
                        self.regs[rd as usize] = res;
                    }
                    1 => {
                        // SUBS Rd, Rn, Rm
                        let rm = rn_or_imm;
                        let a = self.regs[rn as usize];
                        let b = self.regs[rm as usize];
                        let res = self.alu_sub(a, b, false);
                        self.regs[rd as usize] = res;
                    }
                    2 => {
                        // ADDS Rd, Rn, #imm3
                        let imm3 = rn_or_imm;
                        let a = self.regs[rn as usize];
                        let res = self.alu_add(a, imm3, false);
                        self.regs[rd as usize] = res;
                    }
                    3 => {
                        // SUBS Rd, Rn, #imm3
                        let imm3 = rn_or_imm;
                        let a = self.regs[rn as usize];
                        let res = self.alu_sub(a, imm3, false);
                        self.regs[rd as usize] = res;
                    }
                    _ => unreachable!(),
                }
                1
            }
            // 0b001_00: MOVS Rd, #imm8
            0b00100 => {
                let rd = (insn >> 8) & 0x7;
                let imm8 = insn & 0xFF;
                self.regs[rd as usize] = imm8;
                self.flag_n = false;
                self.flag_z = imm8 == 0;
                1
            }
            // 0b001_01: CMP Rn, #imm8
            0b00101 => {
                let rn = (insn >> 8) & 0x7;
                let imm8 = insn & 0xFF;
                let a = self.regs[rn as usize];
                let _ = self.alu_sub(a, imm8, false);
                1
            }
            // 0b001_10: ADDS Rd, #imm8
            0b00110 => {
                let rd = (insn >> 8) & 0x7;
                let imm8 = insn & 0xFF;
                let a = self.regs[rd as usize];
                let res = self.alu_add(a, imm8, false);
                self.regs[rd as usize] = res;
                1
            }
            // 0b001_11: SUBS Rd, #imm8
            0b00111 => {
                let rd = (insn >> 8) & 0x7;
                let imm8 = insn & 0xFF;
                let a = self.regs[rd as usize];
                let res = self.alu_sub(a, imm8, false);
                self.regs[rd as usize] = res;
                1
            }
            // 0b01000: Data-processing or Special data / Branch exchange
            0b01000 => {
                if (insn & (1 << 10)) == 0 {
                    // ALU operations (010000)
                    let op = (insn >> 6) & 0xF;
                    let rm = (insn >> 3) & 0x7;
                    let rdn = insn & 0x7;
                    let a = self.regs[rdn as usize];
                    let b = self.regs[rm as usize];
                    match op {
                        0 => {
                            // ANDS
                            let res = a & b;
                            self.regs[rdn as usize] = res;
                            self.flag_n = (res >> 31) != 0;
                            self.flag_z = res == 0;
                        }
                        1 => {
                            // EORS
                            let res = a ^ b;
                            self.regs[rdn as usize] = res;
                            self.flag_n = (res >> 31) != 0;
                            self.flag_z = res == 0;
                        }
                        2 => {
                            // LSLS
                            let shift = b & 0xFF;
                            if shift == 0 {
                                self.regs[rdn as usize] = a;
                                self.set_nz(a);
                                // C unchanged
                            } else if shift < 32 {
                                self.flag_c = ((a >> (32 - shift)) & 1) != 0;
                                let res = a << shift;
                                self.regs[rdn as usize] = res;
                                self.set_nz(res);
                            } else if shift == 32 {
                                self.flag_c = (a & 1) != 0;
                                self.regs[rdn as usize] = 0;
                                self.set_nz(0);
                            } else {
                                self.flag_c = false;
                                self.regs[rdn as usize] = 0;
                                self.set_nz(0);
                            }
                        }
                        3 => {
                            // LSRS
                            let shift = b & 0xFF;
                            if shift == 0 {
                                self.regs[rdn as usize] = a;
                                self.set_nz(a);
                                // C unchanged
                            } else if shift < 32 {
                                self.flag_c = ((a >> (shift - 1)) & 1) != 0;
                                let res = a >> shift;
                                self.regs[rdn as usize] = res;
                                self.set_nz(res);
                            } else if shift == 32 {
                                self.flag_c = (a >> 31) != 0;
                                self.regs[rdn as usize] = 0;
                                self.set_nz(0);
                            } else {
                                self.flag_c = false;
                                self.regs[rdn as usize] = 0;
                                self.set_nz(0);
                            }
                        }
                        4 => {
                            // ASRS
                            let shift = b & 0xFF;
                            let a_i32 = a as i32;
                            if shift == 0 {
                                self.regs[rdn as usize] = a;
                                self.set_nz(a);
                                // C unchanged
                            } else if shift < 32 {
                                self.flag_c = ((a_i32 >> (shift - 1)) & 1) != 0;
                                let res = (a_i32 >> shift) as u32;
                                self.regs[rdn as usize] = res;
                                self.set_nz(res);
                            } else {
                                let sign = (a >> 31) != 0;
                                self.flag_c = sign;
                                let res = if sign { 0xFFFF_FFFF } else { 0 };
                                self.regs[rdn as usize] = res;
                                self.set_nz(res);
                            }
                        }
                        5 => {
                            // ADCS
                            let res = self.alu_add(a, b, self.flag_c);
                            self.regs[rdn as usize] = res;
                        }
                        6 => {
                            // SBCS
                            let res = self.alu_sub(a, b, !self.flag_c);
                            self.regs[rdn as usize] = res;
                        }
                        7 => {
                            // RORS
                            let shift_byte = (b & 0xFF) as u32;
                            if shift_byte == 0 {
                                self.regs[rdn as usize] = a;
                                self.set_nz(a);
                                // C unchanged
                            } else {
                                let rot = shift_byte & 0x1F;
                                let res = a.rotate_right(rot);
                                self.flag_c = ((res >> 31) & 1) != 0;
                                self.regs[rdn as usize] = res;
                                self.set_nz(res);
                            }
                        }
                        8 => {
                            // TST
                            let res = a & b;
                            self.flag_n = (res >> 31) != 0;
                            self.flag_z = res == 0;
                        }
                        9 => {
                            // RSBS Rd, Rn, #0
                            let res = self.alu_sub(0, b, false);
                            self.regs[rdn as usize] = res;
                        }
                        10 => {
                            // CMP
                            let _ = self.alu_sub(a, b, false);
                        }
                        11 => {
                            // CMN
                            let _ = self.alu_add(a, b, false);
                        }
                        12 => {
                            // ORRS
                            let res = a | b;
                            self.regs[rdn as usize] = res;
                            self.flag_n = (res >> 31) != 0;
                            self.flag_z = res == 0;
                        }
                        13 => {
                            // MULS
                            let res = a.wrapping_mul(b);
                            self.regs[rdn as usize] = res;
                            self.flag_n = (res >> 31) != 0;
                            self.flag_z = res == 0;
                        }
                        14 => {
                            // BICS
                            let res = a & !b;
                            self.regs[rdn as usize] = res;
                            self.flag_n = (res >> 31) != 0;
                            self.flag_z = res == 0;
                        }
                        15 => {
                            // MVNS
                            let res = !b;
                            self.regs[rdn as usize] = res;
                            self.flag_n = (res >> 31) != 0;
                            self.flag_z = res == 0;
                        }
                        _ => unreachable!(),
                    }
                } else {
                    // Special data instructions and branch exchange (010001)
                    let op = (insn >> 8) & 0x3;
                    let rm = ((insn >> 3) & 0xF) as usize;
                    let rd = (((insn >> 4) & 0x8) | (insn & 0x7)) as usize;
                    let a = if rd == 15 {
                        pc.wrapping_add(4)
                    } else {
                        self.regs[rd]
                    };
                    let b = if rm == 15 {
                        pc.wrapping_add(4)
                    } else {
                        self.regs[rm]
                    };
                    match op {
                        0 => {
                            // ADD (register)
                            let res = a.wrapping_add(b);
                            if rd == 15 {
                                self.branch_write_pc(res);
                            } else if rd == 13 {
                                self.set_sp(res);
                            } else {
                                self.regs[rd] = res;
                            }
                        }
                        1 => {
                            // CMP (register)
                            let _ = self.alu_sub(a, b, false);
                        }
                        2 => {
                            // MOV (register)
                            if rd == 15 {
                                self.branch_write_pc(b);
                            } else if rd == 13 {
                                self.set_sp(b);
                            } else {
                                self.regs[rd] = b;
                            }
                        }
                        3 => {
                            // BX / BLX
                            let target = b;
                            if (insn & (1 << 7)) != 0 {
                                // BLX
                                self.regs[14] = self.regs[15] | 1;
                            }
                            self.branch_write_pc(target);
                        }
                        _ => unreachable!(),
                    }
                }
                1
            }
            // 0b01001: LDR Rt, [PC, #imm8 * 4]
            0b01001 => {
                let rt = ((insn >> 8) & 0x7) as usize;
                let imm8 = insn & 0xFF;
                let base = (pc + 4) & !3;
                let addr = base + imm8 * 4;
                self.regs[rt] = self.read32(addr);
                2
            }
            // 0b0101x: Load/Store register offset
            0b01010 | 0b01011 => {
                let op = (insn >> 9) & 0x7;
                let rm = ((insn >> 6) & 0x7) as usize;
                let rn = ((insn >> 3) & 0x7) as usize;
                let rt = (insn & 0x7) as usize;
                let addr = self.regs[rn].wrapping_add(self.regs[rm]);
                match op {
                    0 => self.write32(addr, self.regs[rt]),        // STR
                    1 => self.write16(addr, self.regs[rt] as u16), // STRH
                    2 => self.write8(addr, self.regs[rt] as u8),   // STRB
                    3 => self.regs[rt] = (self.read8(addr) as i8) as i32 as u32, // LDRSB
                    4 => self.regs[rt] = self.read32(addr),        // LDR
                    5 => self.regs[rt] = self.read16(addr) as u32, // LDRH
                    6 => self.regs[rt] = self.read8(addr) as u32,  // LDRB
                    7 => self.regs[rt] = (self.read16(addr) as i16) as i32 as u32, // LDRSH
                    _ => unreachable!(),
                }
                2
            }
            // 0b01100: STR Rt, [Rn, #imm5 * 4]
            0b01100 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rn = ((insn >> 3) & 0x7) as usize;
                let rt = (insn & 0x7) as usize;
                let addr = self.regs[rn].wrapping_add(imm5 * 4);
                self.write32(addr, self.regs[rt]);
                2
            }
            // 0b01101: LDR Rt, [Rn, #imm5 * 4]
            0b01101 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rn = ((insn >> 3) & 0x7) as usize;
                let rt = (insn & 0x7) as usize;
                let addr = self.regs[rn].wrapping_add(imm5 * 4);
                self.regs[rt] = self.read32(addr);
                2
            }
            // 0b01110: STRB Rt, [Rn, #imm5]
            0b01110 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rn = ((insn >> 3) & 0x7) as usize;
                let rt = (insn & 0x7) as usize;
                let addr = self.regs[rn].wrapping_add(imm5);
                self.write8(addr, self.regs[rt] as u8);
                2
            }
            // 0b01111: LDRB Rt, [Rn, #imm5]
            0b01111 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rn = ((insn >> 3) & 0x7) as usize;
                let rt = (insn & 0x7) as usize;
                let addr = self.regs[rn].wrapping_add(imm5);
                self.regs[rt] = self.read8(addr) as u32;
                2
            }
            // 0b10000: STRH Rt, [Rn, #imm5 * 2]
            0b10000 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rn = ((insn >> 3) & 0x7) as usize;
                let rt = (insn & 0x7) as usize;
                let addr = self.regs[rn].wrapping_add(imm5 * 2);
                self.write16(addr, self.regs[rt] as u16);
                2
            }
            // 0b10001: LDRH Rt, [Rn, #imm5 * 2]
            0b10001 => {
                let imm5 = (insn >> 6) & 0x1F;
                let rn = ((insn >> 3) & 0x7) as usize;
                let rt = (insn & 0x7) as usize;
                let addr = self.regs[rn].wrapping_add(imm5 * 2);
                self.regs[rt] = self.read16(addr) as u32;
                2
            }
            // 0b10010: STR Rt, [SP, #imm8 * 4]
            0b10010 => {
                let rt = ((insn >> 8) & 0x7) as usize;
                let imm8 = insn & 0xFF;
                let addr = self.regs[13].wrapping_add(imm8 * 4);
                self.write32(addr, self.regs[rt]);
                2
            }
            // 0b10011: LDR Rt, [SP, #imm8 * 4]
            0b10011 => {
                let rt = ((insn >> 8) & 0x7) as usize;
                let imm8 = insn & 0xFF;
                let addr = self.regs[13].wrapping_add(imm8 * 4);
                self.regs[rt] = self.read32(addr);
                2
            }
            // 0b10100: ADR Rd, [PC, #imm8 * 4]
            0b10100 => {
                let rd = ((insn >> 8) & 0x7) as usize;
                let imm8 = insn & 0xFF;
                let base = (pc + 4) & !3;
                self.regs[rd] = base + imm8 * 4;
                1
            }
            // 0b10101: ADD Rd, SP, #imm8 * 4
            0b10101 => {
                let rd = ((insn >> 8) & 0x7) as usize;
                let imm8 = insn & 0xFF;
                self.regs[rd] = self.regs[13].wrapping_add(imm8 * 4);
                1
            }
            // 0b10110 | 0b10111: Miscellaneous
            0b10110 | 0b10111 => {
                let op = (insn >> 8) & 0xF;
                match op {
                    0x0 => {
                        // ADD SP, #imm7 * 4 / SUB SP, #imm7 * 4
                        let imm7 = insn & 0x7F;
                        if (insn & (1 << 7)) != 0 {
                            self.set_sp(self.regs[13].wrapping_sub(imm7 * 4));
                        } else {
                            self.set_sp(self.regs[13].wrapping_add(imm7 * 4));
                        }
                    }
                    0x2 => {
                        // SXTH / SXTB / UXTH / UXTB
                        let sub_op = (insn >> 6) & 0x3;
                        let rm = ((insn >> 3) & 0x7) as usize;
                        let rd = (insn & 0x7) as usize;
                        let val = self.regs[rm];
                        match sub_op {
                            0 => self.regs[rd] = (val as i16) as i32 as u32, // SXTH
                            1 => self.regs[rd] = (val as i8) as i32 as u32,  // SXTB
                            2 => self.regs[rd] = (val as u16) as u32,        // UXTH
                            3 => self.regs[rd] = (val & 0xFF) as u32,        // UXTB
                            _ => unreachable!(),
                        }
                    }
                    0x4 | 0x5 => {
                        // PUSH
                        let has_lr = (insn & (1 << 8)) != 0;
                        let rlist = insn & 0xFF;
                        let count = rlist.count_ones() + if has_lr { 1 } else { 0 };
                        let mut sp = (self.regs[13] - count * 4) & !3;
                        self.set_sp(sp);
                        for i in 0..8 {
                            if (rlist & (1 << i)) != 0 {
                                self.write32(sp, self.regs[i]);
                                sp += 4;
                            }
                        }
                        if has_lr {
                            self.write32(sp, self.regs[14]);
                        }
                    }
                    0x6 => {
                        // CPSID i / CPSIE i
                        if (insn & 0xFF) == 0x62 {
                            self.primask = false; // CPSIE i (0xB662)
                        } else if (insn & 0xFF) == 0x72 {
                            self.primask = true; // CPSID i (0xB672)
                        }
                    }
                    0xA => {
                        // REV / REV16 / REVSH
                        let sub_op = (insn >> 6) & 0x3;
                        let rm = ((insn >> 3) & 0x7) as usize;
                        let rd = (insn & 0x7) as usize;
                        let val = self.regs[rm];
                        match sub_op {
                            0 => self.regs[rd] = val.swap_bytes(), // REV
                            1 => {
                                // REV16
                                let lo = ((val & 0xFFFF) as u16).swap_bytes() as u32;
                                let hi = ((val >> 16) as u16).swap_bytes() as u32;
                                self.regs[rd] = lo | (hi << 16);
                            }
                            3 => self.regs[rd] = (val as u16).swap_bytes() as i16 as i32 as u32, // REVSH
                            _ => {}
                        }
                    }
                    0xC | 0xD => {
                        // POP
                        let has_pc = (insn & (1 << 8)) != 0;
                        let rlist = insn & 0xFF;
                        let mut sp = self.regs[13] & !3;
                        for i in 0..8 {
                            if (rlist & (1 << i)) != 0 {
                                self.regs[i] = self.read32(sp);
                                sp += 4;
                            }
                        }
                        if has_pc {
                            let pc_val = self.read32(sp);
                            sp += 4;
                            self.set_sp(sp);
                            self.branch_write_pc(pc_val);
                            return 2;
                        }
                        self.set_sp(sp);
                    }
                    0xE => {
                        // BKPT
                        self.is_halted = true;
                    }
                    0xF => {
                        // NOP, YIELD, WFE, WFI, SEV
                        let hint = insn & 0xFF;
                        if hint == 0x20 || hint == 0x30 {
                            // WFE (0xBF20) / WFI (0xBF30)
                            self.sleeping = true;
                        }
                    }
                    _ => {}
                }
                1
            }
            // 0b11000: STMIA Rn!, {registers}
            0b11000 => {
                let rn = ((insn >> 8) & 0x7) as usize;
                let rlist = insn & 0xFF;
                let mut addr = self.regs[rn];
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 {
                        self.write32(addr, self.regs[i]);
                        addr += 4;
                    }
                }
                self.regs[rn] = addr;
                2
            }
            // 0b11001: LDMIA Rn!, {registers}
            0b11001 => {
                let rn = ((insn >> 8) & 0x7) as usize;
                let rlist = insn & 0xFF;
                let mut addr = self.regs[rn];
                for i in 0..8 {
                    if (rlist & (1 << i)) != 0 {
                        self.regs[i] = self.read32(addr);
                        addr += 4;
                    }
                }
                if (rlist & (1 << rn)) == 0 {
                    self.regs[rn] = addr;
                }
                2
            }
            // 0b1101x: Conditional Branch & SVC
            0b11010 | 0b11011 => {
                let cond = (insn >> 8) & 0xF;
                let imm8 = (insn & 0xFF) as u8;
                if cond < 14 {
                    if self.eval_cond(cond) {
                        let offset = (imm8 as i8 as i32) * 2;
                        self.regs[15] = (pc + 4).wrapping_add(offset as u32) & !1;
                    }
                } else if cond == 15 {
                    // SVC #imm8
                    self.dispatch_vector(11); // SVCall
                }
                1
            }
            // 0b11100: Unconditional Branch (B label)
            0b11100 => {
                let imm11 = (insn & 0x7FF) as u16;
                let sign_ext = if (imm11 & 0x400) != 0 {
                    (imm11 | 0xF800) as i16 as i32
                } else {
                    imm11 as i32
                };
                let offset = sign_ext * 2;
                self.regs[15] = (pc + 4).wrapping_add(offset as u32) & !1;
                1
            }
            // 0b1111x: 32-bit instructions (BL, DMB/DSB/ISB, MRS/MSR)
            0b11101 | 0b11110 | 0b11111 => {
                let next_insn = self.read16(pc + 2) as u32;
                self.regs[15] = pc + 4;

                if (insn & 0xF800) == 0xF000 && (next_insn & 0xD000) == 0xD000 {
                    // BL <label>
                    let s = ((insn >> 10) & 1) as i32;
                    let imm10 = (insn & 0x03FF) as i32;
                    let j1 = ((next_insn >> 13) & 1) as i32;
                    let j2 = ((next_insn >> 11) & 1) as i32;
                    let imm11 = (next_insn & 0x07FF) as i32;
                    let i1 = !(j1 ^ s) & 1;
                    let i2 = !(j2 ^ s) & 1;
                    let raw = (s << 24) | (i1 << 23) | (i2 << 22) | (imm10 << 12) | (imm11 << 1);
                    let offset = (raw << 7) >> 7; // Sign-extend 25-bit to i32
                    let pc_base = pc + 4; // (instr_addr + 4)
                    self.regs[14] = pc_base | 1;
                    self.regs[15] = pc_base.wrapping_add(offset as u32);
                } else if (insn & 0xFFF0) == 0xF3B0 && (next_insn & 0xFFF0) == 0x8F50 {
                    // DMB / DSB / ISB
                } else if (insn & 0xFFF0) == 0xF3E0 && (next_insn & 0xF000) == 0x8000 {
                    // MRS Rd, spec_reg
                    self.sync_sp_to_active();
                    let rd = ((next_insn >> 8) & 0xF) as usize;
                    let sysm = (next_insn & 0xFF) as u8;
                    self.regs[rd] = match sysm {
                        0..=3 => self.pack_xpsr(),
                        8 => self.msp,
                        9 => self.psp,
                        16 => self.primask as u32,
                        20 => (self.control_spsel as u32) << 1,
                        _ => 0,
                    };
                } else if (insn & 0xFFF0) == 0xF380 && (next_insn & 0xFF00) == 0x8800 {
                    // MSR spec_reg, Rn
                    self.sync_sp_to_active();
                    let rn = (insn & 0xF) as usize;
                    let val = self.regs[rn];
                    let sysm = (next_insn & 0xFF) as u8;
                    match sysm {
                        0..=3 => {
                            self.unpack_xpsr((self.xpsr & 0x0FFF_FFFF) | (val & 0xF000_0000));
                        }
                        8 => {
                            self.msp = val & !3;
                            if self.in_handler_mode || !self.control_spsel {
                                self.regs[13] = self.msp & !3;
                            }
                        }
                        9 => {
                            self.psp = val & !3;
                            if !self.in_handler_mode && self.control_spsel {
                                self.regs[13] = self.psp & !3;
                            }
                        }
                        16 => self.primask = (val & 1) != 0,
                        20 => {
                            let new_spsel = (val & 2) != 0;
                            if !self.in_handler_mode && new_spsel != self.control_spsel {
                                self.control_spsel = new_spsel;
                                self.regs[13] = (if self.control_spsel { self.psp } else { self.msp }) & !3;
                            } else {
                                self.control_spsel = new_spsel;
                            }
                        }
                        _ => {}
                    }
                }
                2
            }
            _ => 1,
        }
    }

    #[inline(always)]
    fn alu_add(&mut self, a: u32, b: u32, carry_in: bool) -> u32 {
        let (res, c, v) = add_with_carry(a, b, carry_in);
        self.set_nzcv(res, c, v);
        res
    }

    #[inline(always)]
    fn alu_sub(&mut self, a: u32, b: u32, borrow: bool) -> u32 {
        let (res, c, v) = add_with_carry(a, !b, !borrow);
        self.set_nzcv(res, c, v);
        res
    }

    #[inline(always)]
    pub fn cond_passed(&self, cond: u8) -> bool {
        let n = self.flag_n;
        let z = self.flag_z;
        let c = self.flag_c;
        let v = self.flag_v;
        match cond {
            0x0 => z,              // EQ
            0x1 => !z,             // NE
            0x2 => c,              // CS / HS
            0x3 => !c,             // CC / LO
            0x4 => n,              // MI
            0x5 => !n,             // PL
            0x6 => v,              // VS
            0x7 => !v,             // VC
            0x8 => c && !z,        // HI
            0x9 => !c || z,        // LS
            0xA => n == v,         // GE
            0xB => n != v,         // LT
            0xC => !z && (n == v), // GT
            0xD => z || (n != v),  // LE
            _ => true,             // AL
        }
    }

    #[inline(always)]
    fn eval_cond(&self, cond: u32) -> bool {
        self.cond_passed(cond as u8)
    }

    #[inline]
    fn dma_write_byte(&mut self, dst: u32, b: u8) {
        match dst {
            0x4001_3828 => self.usart1.write_tdr(b),
            0x4000_4428 => self.usart2.write_tdr(b),
            0x4000_4828 => self.usart3.write_tdr(b),
            0x4000_4C28 => self.usart4.write_tdr(b),
            0x4000_5028 => self.usart5.write_tdr(b),
            0x4001_300C => {
                self.spi1_dr = b as u32;
                self.spi1_tx_bytes.push(b);
            }
            0x4000_380C => self.spi2_dr = b as u32,
            _ => self.write8(dst, b),
        }
    }

    pub fn cycles_until_timer_update(&self, timer_base: u32) -> u32 {
        match timer_base {
            0x4001_2C00 => {
                if (self.tim1_cr1 & 1) != 0 && (self.tim1_sr & 1) == 0 {
                    let step_period = (self.tim1_psc + 1) * (if self.tim1_arr > 0 { self.tim1_arr } else { 499 });
                    step_period.saturating_sub(self.tim1_cnt)
                } else {
                    0
                }
            }
            0x4000_0000 => {
                if (self.tim2_cr1 & 1) != 0 && (self.tim2_sr & 1) == 0 {
                    let step_period = (self.tim2_psc + 1) * (if self.tim2_arr > 0 { self.tim2_arr } else { 499 });
                    step_period.saturating_sub(self.tim2_cnt)
                } else {
                    0
                }
            }
            0x4000_0400 => {
                if (self.tim3_cr1 & 1) != 0 && (self.tim3_sr & 1) == 0 {
                    let arr = if self.tim3_arr > 0 { self.tim3_arr } else { 499 };
                    arr.saturating_sub(self.tim3_cnt)
                } else {
                    0
                }
            }
            0x4000_1000 => {
                if (self.tim6_cr1 & 1) != 0 && (self.tim6_sr & 1) == 0 {
                    let step_period = (self.tim6_psc + 1) * (if self.tim6_arr > 0 { self.tim6_arr } else { 499 });
                    step_period.saturating_sub(self.tim6_cnt)
                } else {
                    0
                }
            }
            0x4000_1400 => {
                if (self.tim7_cr1 & 1) != 0 && (self.tim7_sr & 1) == 0 {
                    let step_period = (self.tim7_psc + 1) * (if self.tim7_arr > 0 { self.tim7_arr } else { 499 });
                    step_period.saturating_sub(self.tim7_cnt)
                } else {
                    0
                }
            }
            0x4000_2000 => {
                if (self.tim14_cr1 & 1) != 0 && (self.tim14_sr & 1) == 0 {
                    let step_period = (self.tim14_psc + 1) * (if self.tim14_arr > 0 { self.tim14_arr } else { 200 });
                    step_period.saturating_sub(self.tim14_cnt)
                } else {
                    0
                }
            }
            0x4001_4000 => {
                if (self.tim15_cr1 & 1) != 0 && (self.tim15_sr & 1) == 0 {
                    let arr = if self.tim15_arr > 0 { self.tim15_arr } else { 499 };
                    let step_period = (self.tim15_psc + 1) * arr;
                    step_period.saturating_sub(self.tim15_cnt)
                } else {
                    0
                }
            }
            0x4001_4400 => {
                if (self.tim16_cr1 & 1) != 0 && (self.tim16_sr & 1) == 0 {
                    let arr = if self.tim16_arr > 0 { self.tim16_arr } else { 499 };
                    let step_period = (self.tim16_psc + 1) * arr;
                    step_period.saturating_sub(self.tim16_cnt)
                } else {
                    0
                }
            }
            0x4001_4800 => {
                if (self.tim17_cr1 & 1) != 0 && (self.tim17_sr & 1) == 0 {
                    let arr = if self.tim17_arr > 0 { self.tim17_arr } else { 499 };
                    let step_period = (self.tim17_psc + 1) * arr;
                    step_period.saturating_sub(self.tim17_cnt)
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    pub fn advance_timers(&mut self, cycles: u32) {
        // SysTick timer
        if (self.systick_ctrl & 1) != 0 {
            self.systick_cycle_accum += cycles;
            let reload = if self.systick_load > 0 {
                self.systick_load + 1
            } else {
                48_000
            };
            while self.systick_cycle_accum >= reload {
                self.systick_cycle_accum -= reload;
                self.systick_ctrl |= 1 << 16; // COUNTFLAG
                if (self.systick_ctrl & (1 << 1)) != 0 {
                    self.systick_pending = true;
                    self.sleeping = false;
                }
            }
        }

        // Fast path: if no general-purpose or basic timer is enabled, skip them all
        let any_timer = (self.tim1_cr1
            | self.tim2_cr1
            | self.tim3_cr1
            | self.tim6_cr1
            | self.tim7_cr1
            | self.tim14_cr1
            | self.tim15_cr1
            | self.tim16_cr1
            | self.tim17_cr1)
            & 1;
        if any_timer == 0 {
            return;
        }

        // TIM1 (Advanced Control Timer - IRQ 13)
        if (self.tim1_cr1 & 1) != 0 {
            self.tim1_cnt += cycles;
            let step_period = (self.tim1_psc + 1) * (if self.tim1_arr > 0 { self.tim1_arr } else { 499 });
            if self.tim1_cnt >= step_period {
                self.tim1_cnt = 0;
                self.tim1_sr |= 1;
                if (self.tim1_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 13;
                    self.sleeping = false;
                }
            }
        }

        // TIM2 (General-Purpose 32-bit Timer - IRQ 15)
        if (self.tim2_cr1 & 1) != 0 {
            self.tim2_cnt += cycles;
            let step_period = (self.tim2_psc + 1) * (if self.tim2_arr > 0 { self.tim2_arr } else { 499 });
            if self.tim2_cnt >= step_period {
                self.tim2_cnt = 0;
                self.tim2_sr |= 1;
                if (self.tim2_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 15;
                    self.sleeping = false;
                }
            }
        }

        // TIM3 (General-Purpose Timer - IRQ 16)
        if (self.tim3_cr1 & 1) != 0 {
            self.tim3_cnt += cycles;
            let arr = if self.tim3_arr > 0 { self.tim3_arr } else { 499 };
            if self.tim3_cnt >= arr {
                self.tim3_cnt = 0;
                self.tim3_sr |= 1;
                if (self.tim3_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 16;
                    self.sleeping = false;
                }
            }
        }

        // TIM6 (Basic Timer - IRQ 17)
        if (self.tim6_cr1 & 1) != 0 {
            self.tim6_cnt += cycles;
            let step_period = (self.tim6_psc + 1) * (if self.tim6_arr > 0 { self.tim6_arr } else { 499 });
            if self.tim6_cnt >= step_period {
                self.tim6_cnt = 0;
                self.tim6_sr |= 1;
                if (self.tim6_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 17;
                    self.sleeping = false;
                }
            }
        }

        // TIM7 (Basic Timer - IRQ 18)
        if (self.tim7_cr1 & 1) != 0 {
            self.tim7_cnt += cycles;
            let step_period = (self.tim7_psc + 1) * (if self.tim7_arr > 0 { self.tim7_arr } else { 499 });
            if self.tim7_cnt >= step_period {
                self.tim7_cnt = 0;
                self.tim7_sr |= 1;
                if (self.tim7_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 18;
                    self.sleeping = false;
                }
            }
        }

        // TIM14 (General-Purpose Timer - IRQ 19)
        if (self.tim14_cr1 & 1) != 0 {
            self.tim14_cnt += cycles;
            let step_period = (self.tim14_psc + 1) * (if self.tim14_arr > 0 { self.tim14_arr } else { 200 });
            if self.tim14_cnt >= step_period {
                self.tim14_cnt = 0;
                self.tim14_sr |= 1;
                if (self.tim14_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 19;
                    self.sleeping = false;
                }
            }
        }

        // TIM15 (General-Purpose Timer - IRQ 20)
        if (self.tim15_cr1 & 1) != 0 {
            self.tim15_cnt += cycles;
            let arr = if self.tim15_arr > 0 { self.tim15_arr } else { 499 };
            let step_period = (self.tim15_psc + 1) * arr;
            if self.tim15_cnt >= step_period {
                self.tim15_cnt = 0;
                self.tim15_sr |= 1;
                if (self.tim15_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 20;
                    self.sleeping = false;
                }
            }
        }

        // TIM16 (General-Purpose Timer - IRQ 21)
        if (self.tim16_cr1 & 1) != 0 {
            self.tim16_cnt += cycles;
            let arr = if self.tim16_arr > 0 { self.tim16_arr } else { 499 };
            let step_period = (self.tim16_psc + 1) * arr;
            if self.tim16_cnt >= step_period {
                self.tim16_cnt = 0;
                self.tim16_sr |= 1;
                if (self.tim16_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 21;
                    self.sleeping = false;
                }
            }
        }

        // TIM17 (Input Capture Timer - IRQ 22) & DMA1 Channel 1 Streaming
        if (self.tim17_cr1 & 1) != 0 {
            self.tim17_cnt += cycles;
            let arr = if self.tim17_arr > 0 { self.tim17_arr } else { 499 };
            let step_period = (self.tim17_psc + 1) * arr;
            if self.tim17_cnt >= step_period {
                self.tim17_cnt = 0;
                self.tim17_sr |= 1;
                if (self.tim17_dier & 1) != 0 {
                    self.nvic_ispr |= 1 << 22;
                    self.sleeping = false;
                }

                // DMA1 Channel 1: stream capture counts from TIM17->CCR1 into SRAM buffer
                if (self.dma1_ccr1 & 1) != 0 {
                    let sample = (self.tim17_ccr1 & 0xFFFF) as u16;
                    let dst_addr = self.dma1_cmar1;
                    if dst_addr >= 0x2000_0000 {
                        let off = (dst_addr - 0x2000_0000) as usize;
                        if off + 2 <= self.sram.len() {
                            self.sram[off..off + 2].copy_from_slice(&sample.to_le_bytes());
                        }
                    }
                    if (self.dma1_ccr1 & (1 << 7)) != 0 {
                        // MINC: Memory increment mode (16-bit)
                        self.dma1_cmar1 = self.dma1_cmar1.wrapping_add(2);
                    }
                    if self.dma1_cndtr1 > 0 {
                        self.dma1_cndtr1 -= 1;
                    }
                    if self.dma1_cndtr1 == 0 {
                        self.dma1_isr |= 1 << 1; // TCIF1
                        if (self.dma1_ccr1 & (1 << 1)) != 0 {
                            self.nvic_ispr |= 1 << 9; // IRQ 9
                            self.sleeping = false;
                        }
                        if (self.dma1_ccr1 & (1 << 5)) != 0 {
                            // CIRC: Circular mode reload
                            self.dma1_cndtr1 = self.dma1_reload_cndtr1;
                            self.dma1_cmar1 = self.dma1_base_cmar1;
                        }
                    }
                }
            }
        }
    }

    pub fn step_cycles(&mut self, target_cycles: usize) -> usize {
        let start_cycles = self.cycles;
        let target = self.cycles + (target_cycles as u64);
        while self.cycles < target && !self.is_halted {
            // 1. Check pending exceptions and wakeup from sleep
            if let Some(exc_num) = self.poll_pending_exception() {
                self.sleeping = false;
                self.enter_exception(exc_num);
            } else if self.sleeping && self.has_pending_wakeup() {
                self.sleeping = false;
            }

            if self.sleeping {
                if self.dma1_completion_delay2 > 0 {
                    let delay = self.dma1_completion_delay2 as u64;
                    self.cycles += delay;
                    self.dma1_completion_delay2 = 0;
                    self.dma1_isr |= (1 << 5) | (1 << 4); // Set TCIF2 and GIF2

                    let count = self.dma1_cndtr2 as usize;
                    let src_addr = self.dma1_cmar2;
                    let dst_addr = self.dma1_cpar2;
                    for i in 0..count {
                        let b = self.read8(src_addr.wrapping_add(i as u32));
                        self.dma_write_byte(dst_addr, b);
                    }
                    self.dma1_cndtr2 = 0;

                    if (self.dma1_ccr2 & (1 << 1)) != 0 {
                        self.nvic_ispr |= 1 << 10;
                    }
                    self.sleeping = false;
                    continue;
                }
                if self.dma1_completion_delay > 0 {
                    let delay = self.dma1_completion_delay as u64;
                    self.cycles += delay;
                    self.dma1_completion_delay = 0;
                    self.dma1_isr |= (1 << 9) | (1 << 8); // Set TCIF3 and GIF3
                    self.dma_frame_ready = true;

                    let count = if self.dma1_cndtr3 > 0 {
                        self.dma1_cndtr3 as usize
                    } else {
                        1024
                    };
                    let src_addr = self.dma1_cmar3;
                    for i in 0..count {
                        let b = self.read8(src_addr.wrapping_add(i as u32));
                        self.spi1_dr = b as u32;
                        self.spi1_tx_bytes.push(b);
                    }
                    self.dma1_cndtr3 = 0;

                    if (self.dma1_ccr3 & (1 << 1)) != 0 {
                        self.nvic_ispr |= 1 << 10;
                    }
                    self.sleeping = false;
                    continue;
                }
                if self.dma1_completion_delay4 > 0 {
                    let delay = self.dma1_completion_delay4 as u64;
                    self.cycles += delay;
                    self.dma1_completion_delay4 = 0;
                    self.dma1_isr |= (1 << 13) | (1 << 12); // Set TCIF4 and GIF4

                    let count = self.dma1_cndtr4 as usize;
                    let dir_from_mem = (self.dma1_ccr4 & (1 << 4)) != 0;
                    let minc = (self.dma1_ccr4 & (1 << 7)) != 0;
                    let pinc = (self.dma1_ccr4 & (1 << 6)) != 0;
                    for i in 0..count {
                        let src = if dir_from_mem {
                            self.dma1_cmar4.wrapping_add(if minc { i as u32 } else { 0 })
                        } else {
                            self.dma1_cpar4.wrapping_add(if pinc { i as u32 } else { 0 })
                        };
                        let dst = if dir_from_mem {
                            self.dma1_cpar4.wrapping_add(if pinc { i as u32 } else { 0 })
                        } else {
                            self.dma1_cmar4.wrapping_add(if minc { i as u32 } else { 0 })
                        };
                        let b = self.read8(src);
                        self.dma_write_byte(dst, b);
                    }
                    self.dma1_cndtr4 = 0;

                    if (self.dma1_ccr4 & (1 << 1)) != 0 {
                        self.nvic_ispr |= 1 << 11;
                    }
                    self.sleeping = false;
                    continue;
                }
                if self.dma1_completion_delay5 > 0 {
                    let delay = self.dma1_completion_delay5 as u64;
                    self.cycles += delay;
                    self.dma1_completion_delay5 = 0;
                    self.dma1_isr |= (1 << 17) | (1 << 16); // Set TCIF5 and GIF5

                    let count = self.dma1_cndtr5 as usize;
                    let dir_from_mem = (self.dma1_ccr5 & (1 << 4)) != 0;
                    let minc = (self.dma1_ccr5 & (1 << 7)) != 0;
                    let pinc = (self.dma1_ccr5 & (1 << 6)) != 0;
                    for i in 0..count {
                        let src = if dir_from_mem {
                            self.dma1_cmar5.wrapping_add(if minc { i as u32 } else { 0 })
                        } else {
                            self.dma1_cpar5.wrapping_add(if pinc { i as u32 } else { 0 })
                        };
                        let dst = if dir_from_mem {
                            self.dma1_cpar5.wrapping_add(if pinc { i as u32 } else { 0 })
                        } else {
                            self.dma1_cmar5.wrapping_add(if minc { i as u32 } else { 0 })
                        };
                        let b = self.read8(src);
                        self.dma_write_byte(dst, b);
                    }
                    self.dma1_cndtr5 = 0;

                    if (self.dma1_ccr5 & (1 << 1)) != 0 {
                        self.nvic_ispr |= 1 << 11;
                    }
                    self.sleeping = false;
                    continue;
                }

                let sleep_cycles = (target - self.cycles) as u32;
                self.usart1.step_cycles(sleep_cycles);
                self.usart2.step_cycles(sleep_cycles);
                self.usart3.step_cycles(sleep_cycles);
                self.usart4.step_cycles(sleep_cycles);
                self.usart5.step_cycles(sleep_cycles);
                self.advance_timers(sleep_cycles);
                self.cycles = target;
                break;
            }

            let pc = self.regs[15];
            if (matches!(pc, 0x0800_98D0..=0x0800_98D4 | 0x0800_56E0..=0x0800_56E4 | 0x0800_4CF0..=0x0800_4CF4)) && !self.has_pending_wakeup() {
                let rem_timer = self.cycles_until_timer_update(self.regs[0]);
                if rem_timer > 0 {
                    let idle_cycles = ((target - self.cycles) as u32).min(rem_timer);
                    self.cycles += idle_cycles as u64;
                    self.ff_hits += 1;
                    self.ff_cycles += idle_cycles as u64;
                    self.advance_peripherals(idle_cycles);
                    continue;
                }
            }

            // Run an execution chunk of up to 48 cycles
            let chunk_target = (self.cycles + 48).min(target);
            let mut chunk_cycles = 0u32;
            while self.cycles < chunk_target && !self.sleeping && !self.is_halted {
                let c = self.step();
                self.cycles += c as u64;
                chunk_cycles += c as u32;
            }
            self.advance_peripherals(chunk_cycles);
        }
        (self.cycles - start_cycles) as usize
    }

    pub fn advance_peripherals(&mut self, c: u32) {
        if c == 0 {
            return;
        }
        if self.usart1.shift_reg.is_some() { self.usart1.step_cycles(c); }
        if self.usart2.shift_reg.is_some() { self.usart2.step_cycles(c); }
        if self.usart3.shift_reg.is_some() { self.usart3.step_cycles(c); }
        if self.usart4.shift_reg.is_some() { self.usart4.step_cycles(c); }
        if self.usart5.shift_reg.is_some() { self.usart5.step_cycles(c); }

        // 2a. Advance DMA1 Channel 2 completion delay
        if self.dma1_completion_delay2 > 0 {
            if self.dma1_completion_delay2 <= c {
                self.dma1_completion_delay2 = 0;
                self.dma1_isr |= (1 << 5) | (1 << 4); // Set TCIF2 and GIF2

                let count = self.dma1_cndtr2 as usize;
                let src_addr = self.dma1_cmar2;
                let dst_addr = self.dma1_cpar2;
                for i in 0..count {
                    let b = self.read8(src_addr.wrapping_add(i as u32));
                    self.dma_write_byte(dst_addr, b);
                }
                self.dma1_cndtr2 = 0;

                if (self.dma1_ccr2 & (1 << 1)) != 0 {
                    // TCIE set: trigger DMA1_Channel2_3 interrupt (IRQ10 = Vector 26)
                    self.nvic_ispr |= 1 << 10;
                    self.sleeping = false;
                }
            } else {
                self.dma1_completion_delay2 -= c;
            }
        }

        // 2b. Advance DMA1 Channel 3 completion delay
        if self.dma1_completion_delay > 0 {
            if self.dma1_completion_delay <= c {
                self.dma1_completion_delay = 0;
                self.dma1_isr |= (1 << 9) | (1 << 8); // Set TCIF3 and GIF3
                self.dma_frame_ready = true;

                let count = if self.dma1_cndtr3 > 0 {
                    self.dma1_cndtr3 as usize
                } else {
                    1024
                };
                let src_addr = self.dma1_cmar3;
                for i in 0..count {
                    let b = self.read8(src_addr.wrapping_add(i as u32));
                    self.spi1_dr = b as u32;
                    self.spi1_tx_bytes.push(b);
                }
                self.dma1_cndtr3 = 0;

                if (self.dma1_ccr3 & (1 << 1)) != 0 {
                    // TCIE set: trigger DMA1_Channel2_3 interrupt (IRQ10 = Vector 26)
                    self.nvic_ispr |= 1 << 10;
                    self.sleeping = false;
                }
            } else {
                self.dma1_completion_delay -= c;
            }
        }

        // 2c. Advance DMA1 Channel 4 completion delay
        if self.dma1_completion_delay4 > 0 {
            if self.dma1_completion_delay4 <= c {
                self.dma1_completion_delay4 = 0;
                self.dma1_isr |= (1 << 13) | (1 << 12); // Set TCIF4 and GIF4

                let count = self.dma1_cndtr4 as usize;
                let dir_from_mem = (self.dma1_ccr4 & (1 << 4)) != 0;
                let minc = (self.dma1_ccr4 & (1 << 7)) != 0;
                let pinc = (self.dma1_ccr4 & (1 << 6)) != 0;
                for i in 0..count {
                    let src = if dir_from_mem {
                        self.dma1_cmar4.wrapping_add(if minc { i as u32 } else { 0 })
                    } else {
                        self.dma1_cpar4.wrapping_add(if pinc { i as u32 } else { 0 })
                    };
                    let dst = if dir_from_mem {
                        self.dma1_cpar4.wrapping_add(if pinc { i as u32 } else { 0 })
                    } else {
                        self.dma1_cmar4.wrapping_add(if minc { i as u32 } else { 0 })
                    };
                    let b = self.read8(src);
                    self.dma_write_byte(dst, b);
                }
                self.dma1_cndtr4 = 0;

                if (self.dma1_ccr4 & (1 << 1)) != 0 {
                    // TCIE set: trigger DMA1_Channel4_5 interrupt (IRQ11 = Vector 27)
                    self.nvic_ispr |= 1 << 11;
                    self.sleeping = false;
                }
            } else {
                self.dma1_completion_delay4 -= c;
            }
        }

        // 2d. Advance DMA1 Channel 5 completion delay
        if self.dma1_completion_delay5 > 0 {
            if self.dma1_completion_delay5 <= c {
                self.dma1_completion_delay5 = 0;
                self.dma1_isr |= (1 << 17) | (1 << 16); // Set TCIF5 and GIF5

                let count = self.dma1_cndtr5 as usize;
                let dir_from_mem = (self.dma1_ccr5 & (1 << 4)) != 0;
                let minc = (self.dma1_ccr5 & (1 << 7)) != 0;
                let pinc = (self.dma1_ccr5 & (1 << 6)) != 0;
                for i in 0..count {
                    let src = if dir_from_mem {
                        self.dma1_cmar5.wrapping_add(if minc { i as u32 } else { 0 })
                    } else {
                        self.dma1_cpar5.wrapping_add(if pinc { i as u32 } else { 0 })
                    };
                    let dst = if dir_from_mem {
                        self.dma1_cpar5.wrapping_add(if pinc { i as u32 } else { 0 })
                    } else {
                        self.dma1_cmar5.wrapping_add(if minc { i as u32 } else { 0 })
                    };
                    let b = self.read8(src);
                    self.dma_write_byte(dst, b);
                }
                self.dma1_cndtr5 = 0;

                if (self.dma1_ccr5 & (1 << 1)) != 0 {
                    // TCIE set: trigger DMA1_Channel4_5 interrupt (IRQ11 = Vector 27)
                    self.nvic_ispr |= 1 << 11;
                    self.sleeping = false;
                }
            } else {
                self.dma1_completion_delay5 -= c;
            }
        }

        // 3. Advance Timers
        self.advance_timers(c);
    }

    pub fn feed_usart1_rx(&mut self, byte: u8) {
        self.usart1.rx.push_back(byte);
        if (self.usart1.cr1 & (1 << 5)) != 0 {
            self.nvic_ispr |= 1 << 27;
        }
        self.sleeping = false;
    }

    pub fn drain_usart1_tx(&mut self) -> Vec<u8> {
        self.usart1.tx.drain(..).collect()
    }

    pub fn feed_usart3_rx(&mut self, byte: u8) {
        self.usart3.rx.push_back(byte);
        if (self.usart3.cr1 & (1 << 5)) != 0 {
            self.nvic_ispr |= 1 << 29;
        }
        self.sleeping = false;
    }

    pub fn drain_usart3_tx(&mut self) -> Vec<u8> {
        self.usart3.tx.drain(..).collect()
    }

    pub fn feed_usart4_rx(&mut self, byte: u8) {
        self.usart4.rx.push_back(byte);
        if (self.usart4.cr1 & (1 << 5)) != 0 {
            self.nvic_ispr |= 1 << 29;
        }
        self.sleeping = false;
    }

    pub fn drain_usart4_tx(&mut self) -> Vec<u8> {
        self.usart4.tx.drain(..).collect()
    }

    pub fn feed_usart5_rx(&mut self, byte: u8) {
        self.usart5.rx.push_back(byte);
        if (self.usart5.cr1 & (1 << 5)) != 0 {
            self.nvic_ispr |= 1 << 29;
        }
        self.sleeping = false;
    }

    pub fn drain_usart5_tx(&mut self) -> Vec<u8> {
        self.usart5.tx.drain(..).collect()
    }
}

#[inline(always)]
pub fn add_with_carry(x: u32, y: u32, carry_in: bool) -> (u32, bool, bool) {
    let c = carry_in as u64;
    let unsigned_sum = (x as u64) + (y as u64) + c;
    let result = unsigned_sum as u32;
    let carry_out = (unsigned_sum >> 32) != 0;
    let overflow = ((!(x ^ y) & (x ^ result)) & 0x8000_0000) != 0;
    (result, carry_out, overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_armv6m_alu_and_flags() {
        let mut flash = vec![0u8; 1024];
        // Reset vector table
        flash[0..4].copy_from_slice(&(0x2000_1000u32).to_le_bytes()); // SP
        flash[4..8].copy_from_slice(&(0x0800_0009u32).to_le_bytes()); // Reset handler at 0x08000008

        // Code:
        // MOVS R0, #10      -> 0x200A
        // MOVS R1, #20      -> 0x2114
        // ADDS R2, R0, R1   -> 0x1842 (op=0, rm=1, rn=0, rd=2)
        // SUBS R3, R2, R1   -> 0x1A53 (op=1, rm=1, rn=2, rd=3)
        // CMP  R2, #30      -> 0x2A1E
        // BNE  #fail        -> 0xD1FE
        // BKPT #0           -> 0xBE00
        let code: [u16; 7] = [0x200A, 0x2114, 0x1842, 0x1A53, 0x2A1E, 0xD1FE, 0xBE00];

        for (i, &w) in code.iter().enumerate() {
            flash[8 + i * 2..10 + i * 2].copy_from_slice(&w.to_le_bytes());
        }

        let mut cpu = ArmV6MCpu::new(flash, 4096);
        assert_eq!(cpu.regs[13], 0x2000_1000);
        assert_eq!(cpu.regs[15], 0x0800_0008);

        cpu.step_cycles(20);

        assert_eq!(cpu.regs[0], 10);
        assert_eq!(cpu.regs[1], 20);
        assert_eq!(cpu.regs[2], 30);
        assert_eq!(cpu.regs[3], 10);
        assert!(cpu.flag_z);
        assert!(!cpu.flag_n);
        assert!(cpu.is_halted);
    }

    #[test]
    fn test_armv6m_oled_binary_boot() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");

        if !path.exists() {
            return;
        }

        let mut bin = std::fs::read(&path).unwrap();
        if bin.len() < 0x10000 {
            bin.resize(0x10000, 0xFF);
        }
        // Config sector: magic + chip id 1
        bin[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x01, 0x00, 0x00]);

        let mut cpu = ArmV6MCpu::new(bin, 32 * 1024);
        assert_eq!(cpu.regs[13], 0x2000_20F0);
        assert_eq!(cpu.regs[15], 0x0800_00D4);

        // Run for 100,000 instructions
        let mut steps = 0;
        while steps < 100_000 && !cpu.is_halted {
            let prev_pc = cpu.regs[15];
            if steps >= 23620 && steps <= 23650 {
                let insn = cpu.read16(prev_pc);
                eprintln!("Step {}: PC=0x{:08X} (insn=0x{:04X}), R0=0x{:08X}, R1=0x{:08X}, R2=0x{:08X}, R3=0x{:08X}, R4=0x{:08X}, R5=0x{:08X}, LR=0x{:08X}, SP=0x{:08X}",
                    steps, prev_pc, insn, cpu.regs[0], cpu.regs[1], cpu.regs[2], cpu.regs[3], cpu.regs[4], cpu.regs[5], cpu.regs[14], cpu.regs[13]);
            }
            cpu.step();
            steps += 1;
            if cpu.regs[15] == 0x0800241A {
                eprintln!(
                    "Reached 0x0800241A at step {}! Prev PC=0x{:08X}, R0=0x{:08X}, LR=0x{:08X}",
                    steps, prev_pc, cpu.regs[0], cpu.regs[14]
                );
                break;
            }
        }

        eprintln!(
            "OLED CPU stepped {} times. Current PC = 0x{:08X}, SP = 0x{:08X}",
            steps, cpu.regs[15], cpu.regs[13]
        );
        assert!(
            steps > 1000,
            "CPU should execute hundreds of instructions during initialization"
        );
        assert_ne!(
            cpu.regs[15], 0x0800_00D4,
            "CPU should have advanced past reset handler"
        );
    }

    #[test]
    fn test_armv6m_matrix_and_kbd_boot() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("original_firmware/unpacked");

        let matrix_path = repo.join("nanowave_iochip_matrix_payload_stm32f0.bin");
        let kbd_path = repo.join("nanowave_iochip_kbd_payload_stm32f0.bin");

        if !matrix_path.exists() || !kbd_path.exists() {
            return;
        }

        let mut matrix_bin = std::fs::read(&matrix_path).unwrap();
        if matrix_bin.len() < 0x10000 {
            matrix_bin.resize(0x10000, 0xFF);
        }
        matrix_bin[0xF800..0xF808]
            .copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x02, 0x00, 0x00]);

        let mut kbd_bin = std::fs::read(&kbd_path).unwrap();
        if kbd_bin.len() < 0x10000 {
            kbd_bin.resize(0x10000, 0xFF);
        }
        kbd_bin[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x03, 0x00, 0x00]);

        let mut matrix_cpu = ArmV6MCpu::new(matrix_bin, 16 * 1024);
        let mut kbd_cpu = ArmV6MCpu::new(kbd_bin, 16 * 1024);

        assert_eq!(matrix_cpu.regs[13], 0x2000_1800);
        assert_eq!(kbd_cpu.regs[13], 0x2000_1DE8);

        for _ in 0..50_000 {
            matrix_cpu.step();
            kbd_cpu.step();
        }

        eprintln!(
            "Matrix PC = 0x{:08X}, SP = 0x{:08X}",
            matrix_cpu.regs[15], matrix_cpu.regs[13]
        );
        eprintln!(
            "Kbd PC = 0x{:08X}, SP = 0x{:08X}",
            kbd_cpu.regs[15], kbd_cpu.regs[13]
        );

        assert_ne!(matrix_cpu.regs[15], 0x0800_00D0);
        assert_ne!(kbd_cpu.regs[15], 0x0800_00D0);
    }

    #[test]
    fn test_trace_oled_usart1_isr() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");

        if !path.exists() {
            return;
        }

        let mut bin = std::fs::read(&path).unwrap();
        if bin.len() < 0x10000 {
            bin.resize(0x10000, 0xFF);
        }
        bin[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x01, 0x00, 0x00]);

        let mut cpu = ArmV6MCpu::new(bin, 32 * 1024);
        cpu.step_cycles(800_000);

        eprintln!(
            "Pre-ISR: PC=0x{:08X}, SP=0x{:08X}, PRIMASK={}",
            cpu.regs[15], cpu.regs[13], cpu.primask
        );
        cpu.feed_usart1_rx(0xAA);
        cpu.dispatch_vector(VECTOR_USART1);
        eprintln!(
            "After dispatch: PC=0x{:08X}, SP=0x{:08X}, LR=0x{:08X}, rx_len={}",
            cpu.regs[15],
            cpu.regs[13],
            cpu.regs[14],
            cpu.usart1.rx.len()
        );

        for step_idx in 0..100 {
            let pc = cpu.regs[15];
            let insn = cpu.read16(pc);
            eprintln!("Step {:02}: PC=0x{:08X} (insn=0x{:04X}), SP=0x{:08X}, R0=0x{:08X}, R1=0x{:08X}, LR=0x{:08X}",
                step_idx, pc, insn, cpu.regs[13], cpu.regs[0], cpu.regs[1], cpu.regs[14]);
            cpu.step();
            if cpu.regs[15] == 0x0800241A {
                eprintln!(
                    "Returned to main loop at step {}! rx_len={}",
                    step_idx,
                    cpu.usart1.rx.len()
                );
                break;
            }
        }
    }

    #[test]
    fn test_trace_oled_boot_start() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");

        if !path.exists() {
            return;
        }

        let mut bin = std::fs::read(&path).unwrap();
        if bin.len() < 0x10000 {
            bin.resize(0x10000, 0xFF);
        }
        bin[0xF800..0xF808].copy_from_slice(&[0xA5, 0x5A, 0x5A, 0xA6, 0x00, 0x01, 0x00, 0x00]);

        let mut cpu = ArmV6MCpu::new(bin, 32 * 1024);

        for step_idx in 0..60 {
            let pc = cpu.regs[15];
            let insn = cpu.read16(pc);
            eprintln!(
                "Boot {:02}: PC=0x{:08X} (insn=0x{:04X}), SP=0x{:08X}, R0=0x{:08X}, R1=0x{:08X}",
                step_idx, pc, insn, cpu.regs[13], cpu.regs[0], cpu.regs[1]
            );
            cpu.step();
        }
    }

    #[test]
    fn test_armv6m_svc_with_primask_and_sp_alignment() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&(0x2000_1003u32).to_le_bytes()); // Unaligned initial SP
        flash[4..8].copy_from_slice(&(0x0800_0009u32).to_le_bytes()); // Reset vector
        flash[44..48].copy_from_slice(&(0x0800_0021u32).to_le_bytes()); // Vector 11 (SVC)

        let mut cpu = ArmV6MCpu::new(flash, 4096);
        // SP bits [1:0] should be cleared on reset
        assert_eq!(cpu.regs[13], 0x2000_1000);

        // Setting unaligned SP via set_sp
        cpu.set_sp(0x2000_1007);
        assert_eq!(cpu.regs[13], 0x2000_1004);

        // Set PRIMASK
        cpu.primask = true;

        // Normal interrupt vector (SysTick = 15) must be masked by PRIMASK
        assert!(!cpu.dispatch_vector(15));

        // SVC (vector 11) must execute even if PRIMASK is set
        assert!(cpu.dispatch_vector(11));
        assert!(cpu.in_handler_mode);
        assert_eq!(cpu.regs[15], 0x0800_0020);
        // Stack pointer after exception entry must remain 4-byte aligned
        assert_eq!(cpu.regs[13] & 3, 0);
    }

    #[test]
    fn test_armv6m_dma1_completion_delay() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&(0x2000_1000u32).to_le_bytes());
        flash[4..8].copy_from_slice(&(0x0800_0009u32).to_le_bytes());

        // Infinite loop: B . (0xE7FE)
        flash[8..10].copy_from_slice(&0xE7FEu16.to_le_bytes());

        let mut cpu = ArmV6MCpu::new(flash, 4096);
        cpu.dma1_isr = 0;
        cpu.nvic_ispr = 0;

        // Enable DMA1 Channel 3 with TCIE (bits 0 and 1)
        cpu.write_mmio(0x4002_0030, 0x03, 4);

        // Immediately after write, DMA should NOT be completed
        assert_eq!(cpu.dma1_isr & (1 << 9), 0);
        assert_eq!(cpu.nvic_ispr & (1 << 10), 0);
        assert_eq!(cpu.dma1_completion_delay, 64);

        // Step 30 cycles (< 64 cycles)
        cpu.step_cycles(30);
        assert_eq!(cpu.dma1_isr & (1 << 9), 0);
        assert_eq!(cpu.nvic_ispr & (1 << 10), 0);

        // Step remaining cycles
        cpu.step_cycles(40);
        assert_ne!(cpu.dma1_isr & (1 << 9), 0); // TCIF3 set
        assert!(cpu.dma_frame_ready);
        // The DMA ISR was dispatched (clearing nvic_ispr and entering handler mode)
        assert!(cpu.in_handler_mode || (cpu.nvic_ispr & (1 << 10)) != 0);
    }

    #[test]
    fn test_armv6m_exit_exception_saves_msp() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&(0x2000_1000u32).to_le_bytes()); // Initial MSP
        flash[4..8].copy_from_slice(&(0x0800_0009u32).to_le_bytes());
        flash[44..48].copy_from_slice(&(0x0800_0021u32).to_le_bytes()); // SVC handler

        let mut cpu = ArmV6MCpu::new(flash, 4096);
        // Switch Thread mode to PSP
        cpu.control_spsel = true;
        cpu.psp = 0x2000_0800;
        cpu.regs[13] = cpu.psp;

        // Dispatch SVC (enters exception, pushes 32-byte frame onto PSP, switches SP to MSP)
        assert!(cpu.dispatch_vector(11));
        assert!(cpu.in_handler_mode);
        assert_eq!(cpu.regs[13], 0x2000_1000); // In handler, SP is MSP

        // Handler pushes some registers onto MSP: SP moves down to 0x2000_0FE0
        cpu.regs[13] = 0x2000_0FE0;
        // Now handler does POP {..., PC} where PC is EXC_RETURN_THREAD_PSP (0xFFFF_FFFD)
        // POP increments MSP back up to 0x2000_1000 before jumping to EXC_RETURN
        cpu.regs[13] = 0x2000_1000;
        cpu.branch_write_pc(0xFFFF_FFFD);

        // Exception returned to Thread mode with PSP
        assert!(!cpu.in_handler_mode);
        assert_eq!(cpu.regs[13], 0x2000_0800); // Restored unstacked PSP
        // MSP MUST have been preserved at 0x2000_1000 without leaking
        assert_eq!(cpu.msp, 0x2000_1000);
    }

    #[test]
    fn test_armv6m_high_reg_mov_add_sp_alignment() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&(0x2000_1000u32).to_le_bytes());
        flash[4..8].copy_from_slice(&(0x0800_0009u32).to_le_bytes());

        // MOVS R1, #7; MOV SP, R1; ADDS R2, #5; ADD SP, R2; BKPT #0
        let code: [u16; 5] = [
            0x2107, // MOVS R1, #7
            0x468D, // MOV SP, R1 (high reg MOV: op=2, rd=13, rm=1)
            0x2205, // MOVS R2, #5
            0x4495, // ADD SP, R2 (high reg ADD: op=0, rd=13, rm=2)
            0xBE00, // BKPT #0
        ];
        for (i, &w) in code.iter().enumerate() {
            flash[8 + i * 2..10 + i * 2].copy_from_slice(&w.to_le_bytes());
        }

        let mut cpu = ArmV6MCpu::new(flash, 4096);
        // Step MOV SP, R1
        cpu.step(); // MOVS R1, #7
        cpu.step(); // MOV SP, R1
        assert_eq!(cpu.regs[13], 0x0000_0004); // 7 & !3 = 4

        // Step ADD SP, R2
        cpu.step(); // MOVS R2, #5
        cpu.step(); // ADD SP, R2
        assert_eq!(cpu.regs[13], 0x0000_0008); // (4 + 5) & !3 = 8
    }

    #[test]
    fn test_armv6m_zero_shift_flag_updates() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&(0x2000_1000u32).to_le_bytes());
        flash[4..8].copy_from_slice(&(0x0800_0009u32).to_le_bytes());

        // MOVS R1, #0; MOVS R0, #0; LSLS R0, R1; BKPT #0
        // LSLS R0, R1: format 010000, op=2, rm=1, rdn=0 -> 010000 0010 001 000 = 0x4088
        let code: [u16; 4] = [
            0x2100, // MOVS R1, #0
            0x2000, // MOVS R0, #0
            0x4088, // LSLS R0, R1
            0xBE00, // BKPT #0
        ];
        for (i, &w) in code.iter().enumerate() {
            flash[8 + i * 2..10 + i * 2].copy_from_slice(&w.to_le_bytes());
        }

        let mut cpu = ArmV6MCpu::new(flash, 4096);
        cpu.flag_c = true; // Set C flag initially to verify it remains unchanged
        cpu.flag_z = false;
        cpu.flag_n = true;

        cpu.step(); // MOVS R1, #0
        cpu.step(); // MOVS R0, #0
        cpu.step(); // LSLS R0, R1 (shift by 0 of value 0)

        assert!(cpu.flag_z); // Z must be updated to true (0 == 0)
        assert!(!cpu.flag_n); // N must be updated to false
        assert!(cpu.flag_c); // C must remain unchanged (true)
    }

    #[test]
    fn test_armv6m_sp_shadow_sync() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&0x2000_0800u32.to_le_bytes()); // Initial SP
        flash[4..8].copy_from_slice(&0x0800_0009u32.to_le_bytes()); // Reset handler

        // SUB SP, #16 (0xB084: imm7=4 -> 16 bytes); PUSH {R0} (0xB401); POP {R0} (0xBC01); ADD SP, #16 (0xB004)
        let code: [u16; 5] = [
            0xB084, // SUB SP, #16
            0xB401, // PUSH {R0}
            0xBC01, // POP {R0}
            0xB004, // ADD SP, #16
            0xBE00, // BKPT #0
        ];
        for (i, &w) in code.iter().enumerate() {
            flash[8 + i * 2..10 + i * 2].copy_from_slice(&w.to_le_bytes());
        }

        let mut cpu = ArmV6MCpu::new(flash, 4096);
        assert_eq!(cpu.regs[13], 0x2000_0800);
        assert_eq!(cpu.msp, 0x2000_0800);

        cpu.step(); // SUB SP, #16
        assert_eq!(cpu.regs[13], 0x2000_07F0);
        assert_eq!(cpu.msp, 0x2000_07F0);

        cpu.step(); // PUSH {R0}
        assert_eq!(cpu.regs[13], 0x2000_07EC);
        assert_eq!(cpu.msp, 0x2000_07EC);

        cpu.step(); // POP {R0}
        assert_eq!(cpu.regs[13], 0x2000_07F0);
        assert_eq!(cpu.msp, 0x2000_07F0);

        cpu.step(); // ADD SP, #16
        assert_eq!(cpu.regs[13], 0x2000_0800);
        assert_eq!(cpu.msp, 0x2000_0800);
    }

    #[test]
    fn test_stm32f0_usart_reset_and_double_buffering() {
        let mut usart = Stm32F0Usart::new();
        // 1. Initial reset flags per RM0091: TXE = 1, TC = 1, RXNE = 0
        let isr = usart.read_isr();
        assert_ne!(isr & (1 << 7), 0, "TXE must be 1 on reset");
        assert_ne!(isr & (1 << 6), 0, "TC must be 1 on reset");
        assert_eq!(isr & (1 << 5), 0, "RXNE must be 0 on reset");

        // 2. Configure 48 MHz / 500k baud (BRR = 96 cycles/bit, 960 cycles/byte)
        usart.brr = 96;
        usart.cr1 = (1 << 3) | (1 << 2) | (1 << 0); // TE | RE | UE

        // 3. Write first byte: immediately latches into shift register, TXE stays 1, TC becomes 0
        usart.write_tdr(0xAA);
        assert_ne!(usart.read_isr() & (1 << 7), 0, "TXE must remain 1 (TDR transferred to shift reg)");
        assert_eq!(usart.read_isr() & (1 << 6), 0, "TC must be 0 while transmitting");
        assert_eq!(usart.shift_cycles_remaining, 960);
        assert!(usart.tx.is_empty());

        // 4. Write second byte while first is serializing: fills TDR, TXE becomes 0!
        usart.write_tdr(0x30);
        assert_eq!(usart.read_isr() & (1 << 7), 0, "TXE must drop to 0 when both TDR and shift reg are occupied");

        // 5. Advance 500 cycles: first byte still serializing, TDR still occupied
        usart.step_cycles(500);
        assert_eq!(usart.read_isr() & (1 << 7), 0);
        assert!(usart.tx.is_empty());

        // 6. Advance another 500 cycles (total 1000): first byte finishes and is emitted to wire!
        usart.step_cycles(500);
        assert_eq!(usart.tx.len(), 1);
        assert_eq!(usart.tx[0], 0xAA);

        // Second byte automatically moved from TDR into shift register: TXE becomes 1 again!
        assert_ne!(usart.read_isr() & (1 << 7), 0, "TXE must return to 1 as second byte entered shift reg");
        assert_eq!(usart.read_isr() & (1 << 6), 0, "TC must remain 0 while second byte is serializing");

        // 7. Advance 960 cycles: second byte finishes and is emitted to wire, TC becomes 1!
        usart.step_cycles(960);
        assert_eq!(usart.tx.len(), 2);
        assert_eq!(usart.tx[1], 0x30);
        assert_ne!(usart.read_isr() & (1 << 6), 0, "TC must become 1 when all bytes finish transmission");

        // 8. Test ICR clear flag: writing bit 6 clears TC
        usart.write_icr(1 << 6);
        assert_eq!(usart.read_isr() & (1 << 6), 0, "ICR bit 6 must clear TC flag");
    }

    #[test]
    fn test_stm32f0_usart_interrupt_pending_conditions() {
        let mut usart = Stm32F0Usart::new();
        assert!(!usart.is_interrupt_pending(), "No interrupts should be pending without enable bits");

        // TXE interrupt
        usart.cr1 = 1 << 7; // TXEIE
        assert!(usart.is_interrupt_pending(), "TXEIE + TXE=1 must assert interrupt");

        // Fill buffer so TXE=0
        usart.brr = 96;
        usart.write_tdr(0x01);
        usart.write_tdr(0x02); // TDR now full, TXE=0
        assert!(!usart.is_interrupt_pending(), "TXEIE with TXE=0 must not assert interrupt");

        // TC interrupt
        usart.cr1 = 1 << 6; // TCIE
        assert!(!usart.is_interrupt_pending(), "TCIE while transmitting must not assert interrupt");
        usart.step_cycles(2000); // drain both bytes
        assert!(usart.is_interrupt_pending(), "TCIE when transmission completes (TC=1) must assert interrupt");

        // RXNE interrupt
        usart.cr1 = 1 << 5; // RXNEIE
        assert!(!usart.is_interrupt_pending(), "RXNEIE with empty RX FIFO must not assert interrupt");
        usart.rx.push_back(0x42);
        assert!(usart.is_interrupt_pending(), "RXNEIE with bytes in RX FIFO must assert interrupt");
        assert_eq!(usart.read_rdr(), 0x42);
        assert!(!usart.is_interrupt_pending(), "Reading RDR must clear RXNE interrupt");
    }

    #[test]
    fn test_stm32f0_timer_input_capture_and_dma1_channel1() {
        let dummy_flash = vec![0u8; 1024];
        let mut cpu = ArmV6MCpu::new(dummy_flash, 16 * 1024);

        // 1. Configure TIM17 with ARR = 200 and simulated key electrode decay timestamp in CCR1
        cpu.tim17_cr1 = 1;      // CEN
        cpu.tim17_dier = 1;     // UIE
        cpu.tim17_arr = 200;
        cpu.tim17_ccr1 = 0x0950; // Measured RC decay timestamp (baseline + delta)

        // 2. Configure DMA1 Channel 1 in Circular mode with MINC and TCIE:
        // CPAR1 points to TIM17->CCR1 (0x40014834), CMAR1 points to SRAM buffer (0x20001000)
        cpu.write32(0x4002_0008, 0x05AB); // CCR1 = EN | TCIE | CIRC | MINC | PSIZE=16b | MSIZE=16b
        cpu.write32(0x4002_000C, 3);      // CNDTR1 = 3 transfers
        cpu.write32(0x4002_0010, 0x4001_4834); // CPAR1 = TIM17->CCR1
        cpu.write32(0x4002_0014, 0x2000_1000); // CMAR1 = 0x20001000

        // Enable IRQ 9 (DMA1_CH1) in NVIC ISER
        cpu.nvic_iser = 1 << 9;

        // 3. Step CPU through 4 timer periods to complete all 3 DMA transfers
        for _ in 0..4 {
            cpu.step_cycles(200);
        }

        // Verify DMA1 Channel 1 streamed 3 halfwords into SRAM at 0x20001000:
        assert_eq!(u16::from_le_bytes(cpu.sram[0x1000..0x1002].try_into().unwrap()), 0x0950);
        assert_eq!(u16::from_le_bytes(cpu.sram[0x1002..0x1004].try_into().unwrap()), 0x0950);
        assert_eq!(u16::from_le_bytes(cpu.sram[0x1004..0x1006].try_into().unwrap()), 0x0950);

        // Verify TCIF1 flag set in DMA1_ISR
        assert_ne!(cpu.dma1_isr & (1 << 1), 0, "TCIF1 must be set upon completing 3 transfers");

        // Verify CPU entered NVIC IRQ 9 (VECTOR_DMA1_CH1) exception handler
        assert!(cpu.in_handler_mode, "CPU must be in handler mode executing DMA1_CH1 ISR");
        assert_eq!(cpu.xpsr & 0x3F, VECTOR_DMA1_CH1 as u32);

        // Verify Circular mode reload: CNDTR1 reloaded to 3, CMAR1 reset to base 0x20001000
        assert_eq!(cpu.dma1_cndtr1, 3);
        assert_eq!(cpu.dma1_cmar1, 0x2000_1000);

        // 4. Test TIM15 and TIM16 interrupt generation
        cpu.tim15_cr1 = 1;
        cpu.tim15_dier = 1;
        cpu.tim15_arr = 100;
        cpu.nvic_iser |= 1 << 20;

        cpu.tim16_cr1 = 1;
        cpu.tim16_dier = 1;
        cpu.tim16_arr = 150;
        cpu.nvic_iser |= 1 << 21;

        cpu.step_cycles(150);
        assert_ne!(cpu.tim15_sr & 1, 0, "TIM15 UIF must be set");
        assert_ne!(cpu.tim16_sr & 1, 0, "TIM16 UIF must be set");
        assert_ne!(cpu.nvic_ispr & (1 << 20), 0, "IRQ 20 (TIM15) must be pended");
        assert_ne!(cpu.nvic_ispr & (1 << 21), 0, "IRQ 21 (TIM16) must be pended");
    }

    #[test]
    fn test_armv6m_nvic_ipr_sub_byte_and_word_access() {
        let dummy_flash = vec![0u8; 1024];
        let mut cpu = ArmV6MCpu::new(dummy_flash, 4096);

        // 1. 32-bit word write to IPR0 (0xE000_E400)
        // Values with varying lower bits: 0x78, 0x56, 0x34, 0x12
        // Lower 6 bits must be masked to 0; only bits [7:6] preserved per PM0215
        cpu.write32(0xE000_E400, 0x1234_5678);
        assert_eq!(cpu.nvic_ipr[0], 0x78 & 0xC0); // 0x40
        assert_eq!(cpu.nvic_ipr[1], 0x56 & 0xC0); // 0x40
        assert_eq!(cpu.nvic_ipr[2], 0x34 & 0xC0); // 0x00
        assert_eq!(cpu.nvic_ipr[3], 0x12 & 0xC0); // 0x00
        assert_eq!(cpu.read32(0xE000_E400), 0x0000_4040);

        // 2. 8-bit byte writes to IPR1 (0xE000_E404..0xE000_E407)
        cpu.write8(0xE000_E404, 0x80); // IRQ 4: 0x80
        cpu.write8(0xE000_E405, 0xC0); // IRQ 5: 0xC0
        cpu.write8(0xE000_E406, 0x3F); // IRQ 6: 0x3F -> 0x00
        cpu.write8(0xE000_E407, 0x40); // IRQ 7: 0x40
        assert_eq!(cpu.nvic_ipr[4], 0x80);
        assert_eq!(cpu.nvic_ipr[5], 0xC0);
        assert_eq!(cpu.nvic_ipr[6], 0x00);
        assert_eq!(cpu.nvic_ipr[7], 0x40);
        assert_eq!(cpu.read32(0xE000_E404), 0x4000_C080);
        assert_eq!(cpu.read8(0xE000_E404), 0x80);
        assert_eq!(cpu.read8(0xE000_E405), 0xC0);
        assert_eq!(cpu.read8(0xE000_E406), 0x00);
        assert_eq!(cpu.read8(0xE000_E407), 0x40);
    }

    #[test]
    fn test_armv6m_priority_preemption_and_nesting() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&0x2000_1000u32.to_le_bytes()); // Initial SP
        flash[4..8].copy_from_slice(&0x0800_0101u32.to_le_bytes()); // Reset handler
        flash[26 * 4..26 * 4 + 4].copy_from_slice(&0x0800_0201u32.to_le_bytes()); // Vector 26: IRQ 10
        flash[21 * 4..21 * 4 + 4].copy_from_slice(&0x0800_0301u32.to_le_bytes()); // Vector 21: IRQ 5

        let mut cpu = ArmV6MCpu::new(flash, 4096);

        // Configure priorities: IRQ 10 priority = 192 (0xC0), IRQ 5 priority = 64 (0x40)
        cpu.write8(0xE000_E400 + 10, 0xC0);
        cpu.write8(0xE000_E400 + 5, 0x40);

        // Enable both IRQs in NVIC ISER
        cpu.write32(0xE000_E100, (1 << 10) | (1 << 5));

        // Pend lower priority IRQ 10
        cpu.nvic_ispr |= 1 << 10;
        assert!(cpu.dispatch_vector(26));

        // Core is now executing IRQ 10 in Handler mode
        assert!(cpu.in_handler_mode);
        assert_eq!(cpu.active_exceptions.len(), 1);
        assert_eq!(cpu.current_execution_priority(), 192);
        assert_eq!(cpu.xpsr & 0x3F, 26);
        assert_eq!(cpu.regs[14], 0xFFFF_FFF9); // EXC_RETURN to Thread mode with MSP

        // Now higher priority IRQ 5 is pended while inside IRQ 10 handler
        cpu.nvic_ispr |= 1 << 5;
        let preempt_target = cpu.poll_pending_exception();
        assert_eq!(preempt_target, Some(21)); // Must preempt because 64 < 192!

        cpu.enter_exception(21);
        // Nested exception entry
        assert!(cpu.in_handler_mode);
        assert_eq!(cpu.active_exceptions.len(), 2);
        assert_eq!(cpu.current_execution_priority(), 64);
        assert_eq!(cpu.xpsr & 0x3F, 21);
        assert_eq!(cpu.regs[14], 0xFFFF_FFF1); // EXC_RETURN to nested Handler mode with MSP

        // Exit IRQ 5 handler
        cpu.branch_write_pc(0xFFFF_FFF1);
        // Should return back to outer IRQ 10 handler
        assert!(cpu.in_handler_mode);
        assert_eq!(cpu.active_exceptions.len(), 1);
        assert_eq!(cpu.current_execution_priority(), 192);
        assert_eq!(cpu.xpsr & 0x3F, 26);

        // Exit IRQ 10 handler
        cpu.branch_write_pc(0xFFFF_FFF9);
        // Should return back to Thread mode
        assert!(!cpu.in_handler_mode);
        assert!(cpu.active_exceptions.is_empty());
        assert_eq!(cpu.current_execution_priority(), 256);
        assert_eq!(cpu.regs[13], 0x2000_1000); // Fully unwound stack
    }

    #[test]
    fn test_armv6m_scb_aircr_reset_and_sysresetreq() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&0x2000_0800u32.to_le_bytes());
        flash[4..8].copy_from_slice(&0x0800_0051u32.to_le_bytes());

        let mut cpu = ArmV6MCpu::new(flash, 4096);
        cpu.regs[0] = 0xDEAD_BEEF;
        cpu.regs[15] = 0x0800_0200;

        // Write SYSRESETREQ without VECTKEY: ignored
        cpu.write32(0xE000_ED0C, 0x0000_0004);
        assert_eq!(cpu.regs[0], 0xDEAD_BEEF);
        assert_eq!(cpu.regs[15], 0x0800_0200);

        // Write SYSRESETREQ with wrong VECTKEY: ignored
        cpu.write32(0xE000_ED0C, 0x1234_0004);
        assert_eq!(cpu.regs[0], 0xDEAD_BEEF);
        assert_eq!(cpu.regs[15], 0x0800_0200);

        // Write SYSRESETREQ with valid VECTKEY (0x05FA): executes hardware reset
        cpu.write32(0xE000_ED0C, 0x05FA_0004);
        assert_eq!(cpu.regs[0], 0);
        assert_eq!(cpu.regs[13], 0x2000_0800);
        assert_eq!(cpu.regs[15], 0x0800_0050);
        assert_eq!(cpu.read32(0xE000_ED0C), 0xFA05_0000); // Read returns VECTKEYSTAT 0xFA05
    }

    #[test]
    fn test_armv6m_scb_icsr_and_pendsv() {
        let dummy_flash = vec![0u8; 1024];
        let mut cpu = ArmV6MCpu::new(dummy_flash, 4096);

        // Assert initial ICSR
        let icsr0 = cpu.read32(0xE000_ED04);
        assert_eq!(icsr0 & 0x3F, 0); // VECTACTIVE = 0
        assert_ne!(icsr0 & (1 << 11), 0); // RETTOBASE = 1

        // Trigger PendSV via ICSR PENDSVSET (bit 28)
        cpu.write32(0xE000_ED04, 1 << 28);
        assert!(cpu.pendsv_pending);

        let icsr1 = cpu.read32(0xE000_ED04);
        assert_ne!(icsr1 & (1 << 28), 0, "PENDSVSET must read 1 when PendSV is pending");
        assert_eq!((icsr1 >> 12) & 0x3F, 14, "VECTPENDING must be 14 (PendSV)");

        // Clear PendSV via ICSR PENDSVCLR (bit 27)
        cpu.write32(0xE000_ED04, 1 << 27);
        assert!(!cpu.pendsv_pending);
        let icsr2 = cpu.read32(0xE000_ED04);
        assert_eq!(icsr2 & (1 << 28), 0, "PENDSVSET must read 0 after PENDSVCLR");
    }

    #[test]
    fn test_armv6m_exti_and_syscfg_routing() {
        let dummy_flash = vec![0u8; 1024];
        let mut cpu = ArmV6MCpu::new(dummy_flash, 4096);

        // Configure EXTI line 1:
        // EXTI_IMR: unmask line 1
        // EXTI_RTSR: enable rising edge trigger on line 1
        // SYSCFG_EXTICR1: route line 1 to Port B (code 1, bits [7:4] = 0x1)
        cpu.write32(0x4001_0400, 0x0002);
        cpu.write32(0x4001_0408, 0x0002);
        cpu.write32(0x4001_0008, 0x0010);
        cpu.write32(0xE000_E100, 1 << 5); // Enable EXTI0_1 (IRQ 5) in NVIC

        // 1. Toggle Port A pin 1 (not routed to EXTI1)
        cpu.set_pin_level('A', 1, false);
        cpu.set_pin_level('A', 1, true);
        assert_eq!(cpu.exti_pr & 0x0002, 0, "Port A pin 1 must not trigger EXTI1 when Port B is mapped");
        assert_eq!(cpu.nvic_ispr & (1 << 5), 0);

        // 2. Toggle Port B pin 1 (routed to EXTI1)
        cpu.set_pin_level('B', 1, false);
        cpu.set_pin_level('B', 1, true);
        assert_ne!(cpu.exti_pr & 0x0002, 0, "Port B pin 1 rising edge must set EXTI_PR bit 1");
        assert_ne!(cpu.nvic_ispr & (1 << 5), 0, "EXTI0_1 (IRQ 5) must be pended in NVIC");

        // 3. Clear pending flag in EXTI_PR (RC_W1)
        cpu.write32(0x4001_0414, 0x0002);
        assert_eq!(cpu.exti_pr & 0x0002, 0, "Writing 1 to EXTI_PR must clear the pending bit");
    }

    #[test]
    fn test_armv6m_dma1_channels_4_and_5() {
        let dummy_flash = vec![0u8; 1024];
        let mut cpu = ArmV6MCpu::new(dummy_flash, 16 * 1024);

        // Write test source pattern in SRAM at 0x2000_0100
        for i in 0..16u8 {
            cpu.sram[0x0100 + i as usize] = 0x50 + i;
        }

        // Configure DMA1 Channel 4 for memory-to-memory copy:
        // CPAR4 = 0x2000_0200 (destination)
        // CMAR4 = 0x2000_0100 (source)
        // CNDTR4 = 16
        // CCR4: MEM2MEM (bit 14) | MINC (bit 7) | PINC (bit 6) | DIR=from_mem (bit 4) | TCIE (bit 1) | EN (bit 0)
        cpu.write32(0x4002_0048, 16);
        cpu.write32(0x4002_004C, 0x2000_0200);
        cpu.write32(0x4002_0050, 0x2000_0100);
        cpu.write32(0x4002_0044, (1 << 14) | (1 << 7) | (1 << 6) | (1 << 4) | (1 << 1) | 1);

        // Step 64 cycles to let DMA completion delay expire
        cpu.step_cycles(64);

        // Verify data was copied
        for i in 0..16u8 {
            assert_eq!(cpu.sram[0x0200 + i as usize], 0x50 + i, "DMA1 Channel 4 transfer mismatch at offset {}", i);
        }

        // Verify TCIF4 flag set in DMA1_ISR (bit 13)
        assert_ne!(cpu.dma1_isr & (1 << 13), 0, "TCIF4 must be set on DMA completion");
        assert_ne!(cpu.nvic_ispr & (1 << 11), 0, "IRQ 11 (DMA1_Channel4_5) must be pended");

        // Clear TCIF4 via DMA1_IFCR (0x4002_0004)
        cpu.write32(0x4002_0004, 1 << 13);
        assert_eq!(cpu.dma1_isr & (1 << 13), 0, "Writing to IFCR must clear TCIF4");
        assert_eq!(cpu.nvic_ispr & (1 << 11), 0, "IFCR clear must de-assert NVIC pending bit");
    }

    #[test]
    fn test_armv6m_hardware_timers_tim1_tim2_tim7() {
        let dummy_flash = vec![0u8; 1024];
        let mut cpu = ArmV6MCpu::new(dummy_flash, 4096);

        // TIM1: PSC = 0, ARR = 100, CEN = 1, UIE = 1
        cpu.write32(0x4001_2C28, 0);
        cpu.write32(0x4001_2C2C, 100);
        cpu.write32(0x4001_2C0C, 1);
        cpu.write32(0x4001_2C00, 1);

        // TIM2: PSC = 0, ARR = 120, CEN = 1, UIE = 1
        cpu.write32(0x4000_0028, 0);
        cpu.write32(0x4000_002C, 120);
        cpu.write32(0x4000_000C, 1);
        cpu.write32(0x4000_0000, 1);

        // TIM7: PSC = 0, ARR = 140, CEN = 1, UIE = 1
        cpu.write32(0x4000_1428, 0);
        cpu.write32(0x4000_142C, 140);
        cpu.write32(0x4000_140C, 1);
        cpu.write32(0x4000_1400, 1);

        cpu.step_cycles(140);

        // Verify TIM1 UIF set and IRQ 13 pended
        assert_ne!(cpu.tim1_sr & 1, 0, "TIM1 UIF must be set");
        assert_ne!(cpu.nvic_ispr & (1 << 13), 0, "IRQ 13 (TIM1) must be pended");

        // Verify TIM2 UIF set and IRQ 15 pended
        assert_ne!(cpu.tim2_sr & 1, 0, "TIM2 UIF must be set");
        assert_ne!(cpu.nvic_ispr & (1 << 15), 0, "IRQ 15 (TIM2) must be pended");

        // Verify TIM7 UIF set and IRQ 18 pended
        assert_ne!(cpu.tim7_sr & 1, 0, "TIM7 UIF must be set");
        assert_ne!(cpu.nvic_ispr & (1 << 18), 0, "IRQ 18 (TIM7) must be pended");
    }

    #[test]
    fn test_armv6m_adc_conversion_and_interrupt() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&(0x2000_1000u32).to_le_bytes()); // MSP
        flash[4..8].copy_from_slice(&(0x0800_0009u32).to_le_bytes()); // Reset PC
        flash[VECTOR_ADC as usize * 4..VECTOR_ADC as usize * 4 + 4].copy_from_slice(&(0x0800_0081u32).to_le_bytes()); // ADC ISR

        let mut cpu = ArmV6MCpu::new(flash, 4096);

        // 1. Enable ADC via ADEN bit 0 in ADC_CR (0x4001_2408)
        cpu.write32(0x4001_2408, 1);
        let isr = cpu.read32(0x4001_2400);
        assert_ne!(isr & 1, 0, "ADRDY (bit 0) must be asserted after ADEN write");

        // 2. Unmask EOCIE (bit 2) in ADC_IER (0x4001_2404)
        cpu.write32(0x4001_2404, 1 << 2);
        assert_eq!(cpu.read32(0x4001_2404), 1 << 2);

        // 3. Trigger conversion via ADSTART bit 2 in ADC_CR
        cpu.write32(0x4001_2408, 1 | (1 << 2));
        let isr_post = cpu.read32(0x4001_2400);
        assert_ne!(isr_post & (1 << 2), 0, "EOC (bit 2) must be asserted");
        assert_ne!(isr_post & (1 << 3), 0, "EOSEQ (bit 3) must be asserted");
        assert_ne!(cpu.nvic_ispr & (1 << 12), 0, "IRQ 12 (ADC) must be pended in NVIC");

        // 4. Exception polling should select VECTOR_ADC (28)
        assert_eq!(cpu.poll_pending_exception(), Some(VECTOR_ADC as u8));

        // 5. Clear EOC by writing 1 to bit 2 in ADC_ISR (RC_W1)
        cpu.write32(0x4001_2400, 1 << 2);
        assert_eq!(cpu.read32(0x4001_2400) & (1 << 2), 0, "EOC bit must be cleared by writing 1");
    }

    #[test]
    fn test_armv6m_gpio_extended_registers_read_modify_write() {
        let dummy_flash = vec![0u8; 1024];
        let mut cpu = ArmV6MCpu::new(dummy_flash, 4096);

        // Test GPIOA MODER, OTYPER, OSPEEDR, PUPDR
        cpu.write32(0x4800_0000, 0x5555_AAAA);
        assert_eq!(cpu.read32(0x4800_0000), 0x5555_AAAA);

        // Halfword write to upper 16 bits of GPIOA MODER
        cpu.write16(0x4800_0002, 0x1234);
        assert_eq!(cpu.read32(0x4800_0000), 0x1234_AAAA);

        // Byte writes to GPIOA OTYPER (0x4800_0004)
        cpu.write8(0x4800_0004, 0x78);
        assert_eq!(cpu.read32(0x4800_0004), 0x0000_0078);
        cpu.write8(0x4800_0005, 0x56);
        assert_eq!(cpu.read32(0x4800_0004), 0x0000_5678);

        // Test GPIOB AFRL (0x4800_0420) and AFRH (0x4800_0424)
        cpu.write32(0x4800_0420, 0x1234_5678);
        cpu.write32(0x4800_0424, 0x9ABC_DEF0);
        assert_eq!(cpu.read32(0x4800_0420), 0x1234_5678);
        assert_eq!(cpu.read32(0x4800_0424), 0x9ABC_DEF0);

        // Test GPIOC, GPIOD, and GPIOF registers
        cpu.write32(0x4800_0800, 0xAAAA_5555); // GPIOC MODER
        cpu.write32(0x4800_0C00, 0xFFFF_0000); // GPIOD MODER
        cpu.write32(0x4800_1400, 0x0000_FFFF); // GPIOF MODER
        assert_eq!(cpu.read32(0x4800_0800), 0xAAAA_5555);
        assert_eq!(cpu.read32(0x4800_0C00), 0xFFFF_0000);
        assert_eq!(cpu.read32(0x4800_1400), 0x0000_FFFF);
    }

    #[test]
    fn test_armv6m_rcc_clock_gating_registers() {
        let dummy_flash = vec![0u8; 1024];
        let mut cpu = ArmV6MCpu::new(dummy_flash, 4096);

        // AHBENR (0x4002_1014)
        cpu.write32(0x4002_1014, 0x007E_0001); // Enable DMA1, Ports A..F
        assert_eq!(cpu.read32(0x4002_1014), 0x007E_0001);

        // APB2ENR (0x4002_1018)
        cpu.write32(0x4002_1018, 0x0005_4801); // SYSCFG, TIM1, SPI1, USART1
        assert_eq!(cpu.read32(0x4002_1018), 0x0005_4801);

        // APB1ENR (0x4002_101C)
        cpu.write32(0x4002_101C, 0x1A06_4833); // TIM2, TIM3, TIM6, TIM7, SPI2, USART2, USART3, USART4, USART5, I2C1, I2C2
        assert_eq!(cpu.read32(0x4002_101C), 0x1A06_4833);

        // CFGR3 (0x4002_1030)
        cpu.write32(0x4002_1030, 0x0003_0000);
        assert_eq!(cpu.read32(0x4002_1030), 0x0003_0000);
    }

    #[test]
    fn test_armv6m_usart3_to_5_rx_and_interrupt() {
        let mut flash = vec![0u8; 1024];
        flash[0..4].copy_from_slice(&(0x2000_1000u32).to_le_bytes());
        flash[4..8].copy_from_slice(&(0x0800_0009u32).to_le_bytes());
        flash[VECTOR_USART3_8 as usize * 4..VECTOR_USART3_8 as usize * 4 + 4]
            .copy_from_slice(&(0x0800_0091u32).to_le_bytes());

        let mut cpu = ArmV6MCpu::new(flash, 4096);

        // 1. Enable RXNEIE in USART3 CR1 (0x4000_4800)
        cpu.write32(0x4000_4800, 1 << 5);
        cpu.sleeping = true;

        // Feed byte into USART3
        cpu.feed_usart3_rx(0x33);
        assert!(!cpu.sleeping, "USART3 RX with RXNEIE must wake CPU from sleep");
        assert_ne!(cpu.nvic_ispr & (1 << 29), 0, "IRQ 29 (USART3_8) must be pended");
        assert_eq!(cpu.poll_pending_exception(), Some(VECTOR_USART3_8 as u8));

        // Read byte from USART3 RDR (0x4000_4824)
        assert_eq!(cpu.read32(0x4000_4824), 0x33);

        // 2. Test USART4 TX and drain
        cpu.write32(0x4000_4C28, 0x44);
        cpu.usart4.step_cycles(1000);
        let drained4 = cpu.drain_usart4_tx();
        assert_eq!(drained4, vec![0x44]);

        // 3. Test USART5 TX and drain
        cpu.write32(0x4000_5028, 0x55);
        cpu.usart5.step_cycles(1000);
        let drained5 = cpu.drain_usart5_tx();
        assert_eq!(drained5, vec![0x55]);
    }
}
