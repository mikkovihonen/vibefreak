//! STM32F446 Embedded Flash Memory Interface emulation (0x4002_3C00)

pub const FLASH_KEY1: u32 = 0x4567_0123;
pub const FLASH_KEY2: u32 = 0xCDEF_89AB;

#[derive(Debug, Clone)]
pub struct FlashPeripheral {
    pub acr: u32,
    pub key_state: u8, // 0 = initial, 1 = key1 received
    pub locked: bool,
    pub sr: u32,
    pub cr: u32,
    pub optcr: u32,
    pub pending_erase_sector: Option<u8>,
}

impl Default for FlashPeripheral {
    fn default() -> Self {
        Self {
            acr: 0,
            key_state: 0,
            locked: true,
            sr: 0,
            cr: 0x8000_0000, // LOCK bit (bit 31) is set on reset
            optcr: 0x0FFF_AAED,
            pending_erase_sector: None,
        }
    }
}

impl FlashPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self, addr: u32) -> u32 {
        match addr & 0xFF {
            0x00 => self.acr,
            0x04 => 0, // KEYR is write-only
            0x08 => 0, // OPTKEYR is write-only
            0x0C => self.sr,
            0x10 => self.cr,
            0x14 => self.optcr,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        match addr & 0xFF {
            0x00 => self.acr = val,
            0x04 => {
                // FLASH_KEYR unlock sequence
                if self.locked {
                    if self.key_state == 0 && val == FLASH_KEY1 {
                        self.key_state = 1;
                    } else if self.key_state == 1 && val == FLASH_KEY2 {
                        self.locked = false;
                        self.cr &= !0x8000_0000; // Clear LOCK bit
                        self.key_state = 0;
                    } else {
                        self.key_state = 0;
                    }
                }
            }
            0x0C => {
                // Write 1 to clear flags in SR (EOP, errors)
                self.sr &= !val;
            }
            0x10 => {
                if !self.locked {
                    // Check if relock requested (bit 31)
                    if (val & 0x8000_0000) != 0 {
                        self.locked = true;
                        self.cr = 0x8000_0000;
                        self.key_state = 0;
                        return;
                    }
                    self.cr = val;
                    // Check if STRT (bit 16) and SER (bit 1) are set
                    if (val & (1 << 16)) != 0 && (val & (1 << 1)) != 0 {
                        let sector = ((val >> 3) & 0x1F) as u8;
                        self.pending_erase_sector = Some(sector);
                        // Set EOP (End of operation), clear BSY (bit 16)
                        self.sr |= 1 << 0;
                        self.cr &= !(1 << 16); // STRT bit automatically cleared by hardware
                    }
                }
            }
            0x14 => self.optcr = val,
            _ => {}
        }
    }

    pub fn sector_address_range(sector: u8) -> Option<(u32, u32)> {
        match sector {
            0 => Some((0x0800_0000, 0x0800_3FFF)), // 16 KB
            1 => Some((0x0800_4000, 0x0800_7FFF)), // 16 KB
            2 => Some((0x0800_8000, 0x0800_BFFF)), // 16 KB
            3 => Some((0x0800_C000, 0x0800_FFFF)), // 16 KB
            4 => Some((0x0801_0000, 0x0801_FFFF)), // 64 KB
            5 => Some((0x0802_0000, 0x0803_FFFF)), // 128 KB
            6 => Some((0x0804_0000, 0x0805_FFFF)), // 128 KB
            7 => Some((0x0806_0000, 0x0807_FFFF)), // 128 KB
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flash_unlock_and_sector_erase() {
        let mut flash = FlashPeripheral::new();
        assert!(flash.locked);
        assert_eq!(flash.cr & 0x8000_0000, 0x8000_0000);

        // Unlock sequence
        flash.write(0x04, FLASH_KEY1);
        assert!(flash.locked);
        flash.write(0x04, FLASH_KEY2);
        assert!(!flash.locked);
        assert_eq!(flash.cr & 0x8000_0000, 0);

        // Request sector 5 erase: SER (bit 1), SNB=5 (bits 3..6 = 5 << 3 = 0x28), STRT (bit 16 = 0x10000)
        let cr_val = (1 << 1) | (5 << 3) | (1 << 16);
        flash.write(0x10, cr_val);
        assert_eq!(flash.pending_erase_sector, Some(5));
        assert_eq!(flash.sr & 1, 1); // EOP set

        // Relock
        flash.write(0x10, 0x8000_0000);
        assert!(flash.locked);
    }
}
