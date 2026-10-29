use crate::config::*;
use std::ops::RangeInclusive;

/// Declarative profile describing firmware-specific layout, entry points, and heuristic hooks.
#[derive(Debug, Clone)]
pub struct FirmwareProfile {
    pub name: &'static str,
    pub load_addr: u32,
    pub vtor: u32,
    pub delay_spin_ranges: Vec<RangeInclusive<u32>>,
    pub boot_splash_completed_pc: Option<u32>,
    pub boot_superloop_range: Option<RangeInclusive<u32>>,
    pub audio_ping_addr: u32,
    pub audio_pong_addr: u32,
    pub adc_buffer_addr: u32,
    pub audio_rx_queue_addr: Option<u32>,
    pub usart2_rx_queue_addr: Option<u32>,
    pub qspi_patch_addr: Option<u32>,
    pub qspi_patch_prologue: [u8; 4],
}

impl FirmwareProfile {
    /// Default profile for stock Arturia MicroFreak firmware v5.x.
    pub fn stock_microfreak() -> Self {
        Self {
            name: "Arturia MicroFreak Stock Firmware (v5.x)",
            load_addr: STOCK_FW_LOAD_ADDR,
            vtor: STOCK_FW_LOAD_ADDR,
            delay_spin_ranges: vec![
                0x0805_5EC8..=0x0805_5ECE,
                0x0805_5F0E..=0x0805_5F1A,
                0x0804_54F2..=0x0804_5502,
                0x0804_55DE..=0x0804_55EE,
                0x0804_565C..=0x0804_566E,
                0x0805_569A..=0x0805_56A2,
                0x0805_574C..=0x0805_5758,
                0x0805_45EC..=0x0805_45F4,
                0x0805_46C2..=0x0805_46CC,
                0x0805_4760..=0x0805_4768,
                0x0805_47F4..=0x0805_47FE,
                0x0805_4844..=0x0805_484C,
                0x0805_486C..=0x0805_4876,
            ],
            boot_splash_completed_pc: Some(0x0802_5008),
            boot_superloop_range: Some(0x0802_4F00..=0x0802_5150),
            audio_ping_addr: AUDIO_DMA_PING_ADDR,
            audio_pong_addr: AUDIO_DMA_PONG_ADDR,
            adc_buffer_addr: ADC_DMA_BUFFER_ADDR,
            audio_rx_queue_addr: Some(0x2001_C330),
            usart2_rx_queue_addr: Some(0x2001_E014),
            qspi_patch_addr: None,
            qspi_patch_prologue: [0xF8, 0xB5, 0x05, 0x68],
        }
    }

    /// Generic profile for custom bare-metal or open-source firmware.
    pub fn generic_stm32f446(load_addr: u32) -> Self {
        Self {
            name: "Generic STM32F446 Firmware",
            load_addr,
            vtor: load_addr,
            delay_spin_ranges: Vec::new(),
            boot_splash_completed_pc: None,
            boot_superloop_range: None,
            audio_ping_addr: AUDIO_DMA_PING_ADDR,
            audio_pong_addr: AUDIO_DMA_PONG_ADDR,
            adc_buffer_addr: ADC_DMA_BUFFER_ADDR,
            audio_rx_queue_addr: None,
            usart2_rx_queue_addr: None,
            qspi_patch_addr: None,
            qspi_patch_prologue: [0xF8, 0xB5, 0x05, 0x68],
        }
    }

    /// Auto-detects the appropriate firmware profile from the vector table payload.
    pub fn detect(main_bin: &[u8]) -> Self {
        if main_bin.len() >= 8 {
            let reset_vector = u32::from_le_bytes(main_bin[4..8].try_into().unwrap()) & !1;
            if (0x0800_0000..0x0800_8000).contains(&reset_vector) {
                return Self::generic_stm32f446(0x0800_0000);
            } else if (0x0800_8000..0x0800_C000).contains(&reset_vector) {
                let mut prof = Self::stock_microfreak();
                prof.name = "Arturia MicroFreak Firmware (Base 0x08008000)";
                prof.load_addr = 0x0800_8000;
                prof.vtor = 0x0800_8000;
                let offset_diff = 0x0800_C000 - 0x0800_8000;
                if let Some(patch) = prof.qspi_patch_addr {
                    prof.qspi_patch_addr = Some(patch - offset_diff);
                }
                if let Some(splash) = prof.boot_splash_completed_pc {
                    prof.boot_splash_completed_pc = Some(splash - offset_diff);
                }
                return prof;
            } else if (0x0800_C000..=0x0810_0000).contains(&reset_vector) {
                return Self::stock_microfreak();
            }
        }
        Self::generic_stm32f446(0x0800_0000)
    }

    /// Returns true if the given PC falls within a known delay-spin loop.
    pub fn is_delay_spin(&self, pc: u32) -> bool {
        self.delay_spin_ranges.iter().any(|r| r.contains(&pc))
    }

    /// Returns true if the given PC is within the boot superloop / idle range.
    pub fn is_boot_superloop(&self, pc: u32) -> bool {
        self.boot_superloop_range
            .as_ref()
            .map(|r| r.contains(&pc))
            .unwrap_or(false)
    }
}

impl Default for FirmwareProfile {
    fn default() -> Self {
        Self::stock_microfreak()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stock_profile_defaults() {
        let prof = FirmwareProfile::stock_microfreak();
        assert_eq!(prof.load_addr, 0x0800_C000);
        assert_eq!(prof.vtor, 0x0800_C000);
        assert!(prof.is_delay_spin(0x0805_5EC8));
        assert!(prof.is_delay_spin(0x0804_55E0));
        assert!(!prof.is_delay_spin(0x0800_0000));
        assert!(prof.is_boot_superloop(0x0802_5000));
        assert!(!prof.is_boot_superloop(0x0801_0000));
    }

    #[test]
    fn test_profile_auto_detection() {
        let mut bin = vec![0u8; 16];
        bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes());
        // Reset vector at 0x0800C123
        bin[4..8].copy_from_slice(&0x0800_C123u32.to_le_bytes());
        let prof = FirmwareProfile::detect(&bin);
        assert_eq!(prof.load_addr, 0x0800_C000);

        // Reset vector at 0x08008123
        bin[4..8].copy_from_slice(&0x0800_8123u32.to_le_bytes());
        let prof2 = FirmwareProfile::detect(&bin);
        assert_eq!(prof2.load_addr, 0x0800_8000);

        // Reset vector at 0x08000123 (Generic bare-metal STM32)
        bin[4..8].copy_from_slice(&0x0800_0123u32.to_le_bytes());
        let prof3 = FirmwareProfile::detect(&bin);
        assert_eq!(prof3.load_addr, 0x0800_0000);
        assert_eq!(prof3.vtor, 0x0800_0000);
        assert!(prof3.delay_spin_ranges.is_empty());
    }
}
