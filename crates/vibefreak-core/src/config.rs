//! Memory maps, peripheral register addresses, and hardware specifications for the Arturia MicroFreak emulator.

pub const FLASH_BASE: u64 = 0x0800_0000;
pub const FLASH_SIZE: usize = 1024 * 1024; // 1 MB

pub const STOCK_FW_LOAD_ADDR: u32 = 0x0800_C000;

pub const SRAM_BASE: u64 = 0x2000_0000;
pub const SRAM_SIZE: usize = 256 * 1024; // 256 KB (includes stack top guard at 0x20020000)
pub const SRAM_STACK_TOP: u32 = 0x2002_0000;

pub const SRAM_BITBAND_BASE: u64 = 0x2200_0000;
pub const SRAM_BITBAND_SIZE: usize = 32 * 1024 * 1024; // 32 MB SRAM bit-band alias

pub const PERIPH_BITBAND_BASE: u64 = 0x4200_0000;
pub const PERIPH_BITBAND_SIZE: usize = 32 * 1024 * 1024; // 32 MB Peripheral bit-band alias

pub const USB_OTG_FS_BASE: u64 = 0x5000_0000;
pub const USB_OTG_FS_SIZE: usize = 64 * 1024; // 64 KB USB OTG FS registers

// ============================================================================
// QSPI NOR Flash Memory Map Specification
// ============================================================================
// The hardware uses a 16 MB ISSI IS25LP128 / IS25WP128 Quad-SPI NOR Flash mapped
// to memory-mapped address 0x9000_0000 through the STM32F446 QUADSPI controller.
//
// Valid QSPI Memory Layout:
//
// 1. Factory Stock Wavetables (Engine 3 - Wavetable):
//    - Offset: 0x0000_0000..0x0004_0000 (256 KB)
//    - Format: 16 wavetables * 32 single-cycle waveforms * 256 16-bit PCM samples
//      (512 bytes per cycle = 16,384 bytes per table).
//    - Populated dynamically at runtime from offset +0x0001_0000 (64 KB) of the
//      firmware update archive's 320 KB wavetable image (target 0x81).
//    - Uninitialized/clean-room state: 0xFF.
//
// 2. Active Preset Staging & Preset Flash Storage:
//    - Offset: 0x0008_1000..0x0028_0FFF (~2 MB, 512 sectors of 4096 bytes)
//    - Slots: 1..=128 (Factory & User Presets), 208..=223 (Factory Vocoder Presets)
//    - Format: 4096-byte TLV-structured sectors.
//      * Header: Magic 0x1901 (u16), Preset Number (u16), Version '1' (0x31), Name (14 bytes).
//      * TLV Payload: Sections (VCO, VCF, EG1, Kbd, Arp, LFO, EG2, Gen, Sys, Mat, Seq).
//      * Flash Integrity Trailers: 0xA5 at byte offsets 1040 (Pattern A), 2082 (Pattern B),
//        3052 (Metadata), and 4095 (Sector End).
//
// 3. User Sample Storage (Engine 19 - Sample, Engine 20 - Grain):
//    - Offset: 0x0028_1000..0x00F5_0FFF (~13 MB)
//    - Format: 24-byte Arturia container header:
//      * +0x00: Magic 0xFA55AAEF (u32 LE)
//      * +0x04: Sample length in bytes (u32 LE)
//      * +0x08: Non-zero Token/Generation ID (u32 LE)
//      * +0x0C: Loop start offset in bytes (u32 LE)
//      * +0x10: Loop end offset in bytes (u32 LE)
//      * +0x14: Sample ID in high byte ((id & 0x7F) << 24)
//      * +0x18: 16-bit signed PCM audio payload (48 kHz mono)
//
// 4. User Wavetable Bank A (Engine 18 - WaveUser):
//    - Offset: 0x00F5_F000..0x00FA_EFFF (320 KB)
//    - Format: First 64 KB (0x00F5_F000..0x00F6_EFFF) contains 16 table descriptors
//      (4096 bytes each with names and cycle metadata).
//      Remaining 256 KB (0x00F6_F000..0x00FA_EFFF) contains raw 16-bit PCM wavetable cycles.
//    - Populated dynamically from target 0x81 of user-supplied .mff package.
//    - Uninitialized/clean-room state: 0xFF.
//
// 5. User Wavetable Bank B (Engine 18 - Alternative Bank):
//    - Offset: 0x00FA_F000..0x00FE_EFFF (256 KB, 16 tables * 16 KB)
//    - When erased (0xFF), the stock firmware routes WaveUser lookups to Bank A (0x00F5_F000).
//
// 6. QSPI Status Sector:
//    - Offset: 0x00FF_F000..0x00FF_FFFF (4 KB)
//    - Byte 0x00FF_F000: ASCII 'Q' (0x51), indicating the QSPI NOR flash is formatted.
// ============================================================================

pub const QSPI_FLASH_BASE: u64 = 0x9000_0000;
pub const QSPI_FLASH_SIZE: usize = 16 * 1024 * 1024; // 16 MB NOR Flash

pub const QSPI_STOCK_WAVETABLE_DEST_ADDR: usize = 0x0000_0000;
pub const QSPI_STOCK_WAVETABLE_SIZE_BYTES: usize = 0x0004_0000; // 256 KB (16 factory tables * 32 cycles * 512 bytes)

pub const QSPI_STAGING_SECTOR_BASE: usize = 0x0008_1000;
pub const QSPI_SAMPLE_MEMORY_BASE: usize = 0x0028_1000;
pub const QSPI_SAMPLE_MEMORY_END: usize = 0x00F5_0FFF;
pub const QSPI_WAVETABLE_DEST_ADDR: usize = 0x00F5_F000;
pub const QSPI_WAVETABLE_SIZE_BYTES: usize = 0x0005_0000; // 320 KB (64 KB headers + 256 KB user tables)
pub const QSPI_USER_WAVETABLE_BANK_B_ADDR: usize = 0x00FA_F000;
pub const QSPI_USER_WAVETABLE_BANK_B_STRIDE: usize = 0x4000; // 16 KB per table (16 tables * 16 KB = 256 KB)
pub const QSPI_STATUS_SECTOR_BASE: usize = 0x00FF_F000;

pub const QUADSPI_REG_BASE: u64 = 0xA000_0000;
pub const QUADSPI_REG_SIZE: usize = 64 * 1024;

pub const PERIPH_MMIO_BASE: u64 = 0x4000_0000;
pub const PERIPH_MMIO_SIZE: usize = 1024 * 1024; // 1 MB covering APB1, APB2, AHB1, AHB2, AHB3

pub const BOOT_ALIAS_BASE: u64 = 0x0000_0000;
pub const BOOT_ALIAS_SIZE: usize = 1024 * 1024;

pub const SYSTEM_MEM_BASE: u64 = 0x1FFF_0000;
pub const SYSTEM_MEM_SIZE: usize = 64 * 1024;

pub const PPB_BASE: u64 = 0xE000_0000;
pub const PPB_SIZE: usize = 1024 * 1024; // 1 MB Cortex-M System Control Space

// Cortex-M4 Core Registers
pub const SCB_ICSR: u32 = 0xE000_ED04;
pub const VTOR_ADDR: u32 = 0xE000_ED08;
pub const AIRCR_ADDR: u32 = 0xE000_ED0C;
pub const CPACR_ADDR: u32 = 0xE000_ED88;

// Architectural ARM Cortex-M EXC_RETURN Values & Trap Page
pub const EXC_RETURN_PAGE_BASE: u64 = 0xFFFF_F000;
pub const EXC_RETURN_PAGE_SIZE: usize = 4096;
pub const EXC_RETURN_MSP_TRAP: u64 = 0xFFFF_FFF8;
pub const EXC_RETURN_MSP: u32 = 0xFFFF_FFF9;
pub const EXC_RETURN_HANDLER_MSP: u32 = 0xFFFF_FFF1;
pub const EXC_RETURN_THREAD_MSP: u32 = 0xFFFF_FFF9;
pub const EXC_RETURN_THREAD_PSP: u32 = 0xFFFF_FFFD;
pub const EXC_RETURN_HANDLER_MSP_FPU: u32 = 0xFFFF_FFE1;
pub const EXC_RETURN_THREAD_MSP_FPU: u32 = 0xFFFF_FFE9;
pub const EXC_RETURN_THREAD_PSP_FPU: u32 = 0xFFFF_FFED;

// CRC Peripheral Registers (0x4002_3000)
pub const CRC_BASE: u32 = 0x4002_3000;
pub const CRC_DR: u32 = CRC_BASE + 0x00;
pub const CRC_IDR: u32 = CRC_BASE + 0x04;
pub const CRC_CR: u32 = CRC_BASE + 0x08;

// Flash Memory Interface Registers (0x4002_3C00)
pub const FLASH_REG_BASE: u32 = 0x4002_3C00;
pub const FLASH_ACR: u32 = FLASH_REG_BASE + 0x00;
pub const FLASH_KEYR: u32 = FLASH_REG_BASE + 0x04;
pub const FLASH_OPTKEYR: u32 = FLASH_REG_BASE + 0x08;
pub const FLASH_SR: u32 = FLASH_REG_BASE + 0x0C;
pub const FLASH_CR: u32 = FLASH_REG_BASE + 0x10;
pub const FLASH_OPTCR: u32 = FLASH_REG_BASE + 0x14;

// RCC Registers
pub const RCC_BASE: u32 = 0x4002_3800;
pub const RCC_CR: u32 = RCC_BASE + 0x00;
pub const RCC_PLLCFGR: u32 = RCC_BASE + 0x04;
pub const RCC_CFGR: u32 = RCC_BASE + 0x08;
pub const RCC_AHB1ENR: u32 = RCC_BASE + 0x30;
pub const RCC_AHB2ENR: u32 = RCC_BASE + 0x34;
pub const RCC_AHB3ENR: u32 = RCC_BASE + 0x38;
pub const RCC_APB1ENR: u32 = RCC_BASE + 0x40;
pub const RCC_APB2ENR: u32 = RCC_BASE + 0x44;
pub const RCC_PLLSAICFGR: u32 = RCC_BASE + 0x84;
pub const RCC_PLLI2SCFGR: u32 = RCC_BASE + 0x88;
pub const RCC_DCKCFGR: u32 = RCC_BASE + 0x8C;
pub const RCC_DCKCFGR2: u32 = RCC_BASE + 0x94;

// GPIO Registers
pub const GPIOA_BASE: u32 = 0x4002_0000;
pub const GPIOB_BASE: u32 = 0x4002_0400;
pub const GPIOC_BASE: u32 = 0x4002_0800;
pub const GPIOD_BASE: u32 = 0x4002_0C00;
pub const GPIOE_BASE: u32 = 0x4002_1000;

pub const GPIO_MODER_OFFSET: u32 = 0x00;
pub const GPIO_OTYPER_OFFSET: u32 = 0x04;
pub const GPIO_OSPEEDR_OFFSET: u32 = 0x08;
pub const GPIO_PUPDR_OFFSET: u32 = 0x0C;
pub const GPIO_IDR_OFFSET: u32 = 0x10;
pub const GPIO_ODR_OFFSET: u32 = 0x14;
pub const GPIO_BSRR_OFFSET: u32 = 0x18;
pub const GPIO_AFRL_OFFSET: u32 = 0x20;
pub const GPIO_AFRH_OFFSET: u32 = 0x24;

// SEM Filter Control Pins
pub const SEM_LP_PORT_BASE: u32 = GPIOD_BASE;
pub const SEM_LP_PIN: u32 = 4; // PD4
pub const SEM_HP_PORT_BASE: u32 = GPIOD_BASE;
pub const SEM_HP_PIN: u32 = 5; // PD5
pub const SEM_BP_PORT_BASE: u32 = GPIOA_BASE;
pub const SEM_BP_PIN: u32 = 15; // PA15

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemFilterMode {
    LowPass = 0,
    BandPass = 1,
    HighPass = 2,
    Notch = 3,
}

// DMA1 Registers
pub const DMA1_BASE: u32 = 0x4002_6000;
pub const DMA1_LISR: u32 = DMA1_BASE + 0x00;
pub const DMA1_HISR: u32 = DMA1_BASE + 0x04;
pub const DMA1_LIFCR: u32 = DMA1_BASE + 0x08;
pub const DMA1_HIFCR: u32 = DMA1_BASE + 0x0C;

// DMA2 Registers
pub const DMA2_BASE: u32 = 0x4002_6400;
pub const DMA2_LISR: u32 = DMA2_BASE + 0x00;
pub const DMA2_HISR: u32 = DMA2_BASE + 0x04;
pub const DMA2_LIFCR: u32 = DMA2_BASE + 0x08;
pub const DMA2_HIFCR: u32 = DMA2_BASE + 0x0C;

pub const DMA2_STREAM4_BASE: u32 = DMA2_BASE + 0x70;
pub const DMA2_STREAM0_BASE: u32 = DMA2_BASE + 0x10;

// USART1 (APB2: 0x4001_1000)
pub const USART1_BASE: u32 = 0x4001_1000;
pub const USART1_SR: u32 = USART1_BASE + 0x00;
pub const USART1_DR: u32 = USART1_BASE + 0x04;
pub const USART1_BRR: u32 = USART1_BASE + 0x08;
pub const USART1_CR1: u32 = USART1_BASE + 0x0C;
pub const USART1_CR2: u32 = USART1_BASE + 0x10;
pub const USART1_CR3: u32 = USART1_BASE + 0x14;

// USART2
pub const USART2_BASE: u32 = 0x4000_4400;
pub const USART2_SR: u32 = USART2_BASE + 0x00;
pub const USART2_DR: u32 = USART2_BASE + 0x04;
pub const USART2_BRR: u32 = USART2_BASE + 0x08;
pub const USART2_CR1: u32 = USART2_BASE + 0x0C;
pub const USART2_CR2: u32 = USART2_BASE + 0x10;
pub const USART2_CR3: u32 = USART2_BASE + 0x14;

// USART3
pub const USART3_BASE: u32 = 0x4000_4800;
pub const USART3_SR: u32 = USART3_BASE + 0x00;
pub const USART3_DR: u32 = USART3_BASE + 0x04;
pub const USART3_BRR: u32 = USART3_BASE + 0x08;
pub const USART3_CR1: u32 = USART3_BASE + 0x0C;
pub const USART3_CR2: u32 = USART3_BASE + 0x10;
pub const USART3_CR3: u32 = USART3_BASE + 0x14;

// QUADSPI (0xA000_1000..=0xA000_1030)
pub const QUADSPI_CR: u32 = 0xA000_1000;
pub const QUADSPI_DCR: u32 = 0xA000_1004;
pub const QUADSPI_SR: u32 = 0xA000_1008;
pub const QUADSPI_FCR: u32 = 0xA000_100C;
pub const QUADSPI_DLR: u32 = 0xA000_1010;
pub const QUADSPI_CCR: u32 = 0xA000_1014;
pub const QUADSPI_AR: u32 = 0xA000_1018;
pub const QUADSPI_ABR: u32 = 0xA000_101C;
pub const QUADSPI_DR: u32 = 0xA000_1020;
pub const QUADSPI_PSMKR: u32 = 0xA000_1024;
pub const QUADSPI_PSMAR: u32 = 0xA000_1028;
pub const QUADSPI_PIR: u32 = 0xA000_102C;
pub const QUADSPI_LPTR: u32 = 0xA000_1030;

// PWR
pub const PWR_BASE: u32 = 0x4000_7000;
pub const PWR_CR: u32 = PWR_BASE + 0x00;
pub const PWR_CSR: u32 = PWR_BASE + 0x04;

// I2C1
pub const I2C1_BASE: u32 = 0x4000_5400;
pub const I2C1_CR1: u32 = I2C1_BASE + 0x00;
pub const I2C1_CR2: u32 = I2C1_BASE + 0x04;
pub const I2C1_OAR1: u32 = I2C1_BASE + 0x08;
pub const I2C1_OAR2: u32 = I2C1_BASE + 0x0C;
pub const I2C1_DR: u32 = I2C1_BASE + 0x10;
pub const I2C1_SR1: u32 = I2C1_BASE + 0x14;
pub const I2C1_SR2: u32 = I2C1_BASE + 0x18;
pub const I2C1_TRISE: u32 = I2C1_BASE + 0x1C;
pub const I2C1_FLTR: u32 = I2C1_BASE + 0x20;

// I2C3
pub const I2C3_BASE: u32 = 0x4000_5C00;
pub const I2C3_CR1: u32 = I2C3_BASE + 0x00;
pub const I2C3_CR2: u32 = I2C3_BASE + 0x04;
pub const I2C3_OAR1: u32 = I2C3_BASE + 0x08;
pub const I2C3_OAR2: u32 = I2C3_BASE + 0x0C;
pub const I2C3_DR: u32 = I2C3_BASE + 0x10;
pub const I2C3_SR1: u32 = I2C3_BASE + 0x14;
pub const I2C3_SR2: u32 = I2C3_BASE + 0x18;
pub const I2C3_TRISE: u32 = I2C3_BASE + 0x1C;
pub const I2C3_FLTR: u32 = I2C3_BASE + 0x20;

// Interrupt Vector table indices
pub const VECTOR_RESET: u32 = 1;
pub const VECTOR_SVC: u32 = 11;
pub const VECTOR_PENDSV: u32 = 14;
pub const VECTOR_SYSTICK: u32 = 15;
pub const VECTOR_USART1: u32 = 53; // IRQ 37
pub const VECTOR_USART2: u32 = 54; // IRQ 38
pub const VECTOR_USART3: u32 = 55; // IRQ 39
pub const VECTOR_EXTI15_10: u32 = 56; // IRQ 40 (External Clock EXTI 15..10)
pub const VECTOR_TIM8_UP: u32 = 60; // IRQ 44
pub const VECTOR_TIM8_TRG_COM: u32 = 61; // IRQ 45
pub const VECTOR_TIM8_CC: u32 = 62; // IRQ 46
pub const VECTOR_TIM5: u32 = 66; // IRQ 50
pub const VECTOR_DMA2_STREAM0: u32 = 72; // IRQ 56 (ADC1)
pub const VECTOR_DMA2_STREAM1: u32 = 73; // IRQ 57
pub const VECTOR_DMA2_STREAM2: u32 = 74; // IRQ 58
pub const VECTOR_DMA2_STREAM3: u32 = 75; // IRQ 59
pub const VECTOR_DMA2_STREAM4: u32 = 76; // IRQ 60 (Audio SAI2)
pub const VECTOR_DMA2_STREAM5: u32 = 84; // IRQ 68
pub const VECTOR_DMA2_STREAM6: u32 = 85; // IRQ 69
pub const VECTOR_DMA2_STREAM7: u32 = 86; // IRQ 70 (QUADSPI DMA)
pub const VECTOR_OTG_FS: u32 = 83; // IRQ 67 (USB OTG FS)
pub const VECTOR_QUADSPI: u32 = 108; // IRQ 92 (QUADSPI)

use crate::peripherals::tlv320aic3104::{AudioInputSource, HeadsetJackState};

/// Physical rear-panel CV, Gate, Clock, and Headset / Codec I/O state
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct RearPanelState {
    pub pitch_cv_raw: u16,
    pub pitch_cv_volts: f32,
    pub gate_raw: bool,
    pub gate_mode_12v: bool,
    pub gate_volts: f32,
    pub pressure_raw: u16,
    pub pressure_volts: f32,
    pub clock_out_raw: bool,
    pub clock_out_volts: f32,
    pub clock_in_raw: bool,
    pub headset_jack: HeadsetJackState,
    pub headset_button: bool,
    pub audio_input_source: AudioInputSource,
    pub adc_gain_left_db: f32,
    pub adc_gain_right_db: f32,
    pub dac_volume_left_db: f32,
    pub dac_volume_right_db: f32,
}

// Audio Parameters
pub const AUDIO_SAMPLE_RATE: u32 = 48000;
pub const AUDIO_BLOCK_FRAMES: usize = 32;
pub const AUDIO_CHANNELS: usize = 2;
pub const AUDIO_BLOCK_BYTES: usize = AUDIO_BLOCK_FRAMES * AUDIO_CHANNELS * 2; // 128 bytes

pub const AUDIO_DMA_PING_ADDR: u32 = 0x2001_B6A2;
pub const AUDIO_DMA_PONG_ADDR: u32 = 0x2001_B722; // 0x2001_B6A2 + 128
pub const ADC_DMA_BUFFER_ADDR: u32 = 0x2000_0960;
