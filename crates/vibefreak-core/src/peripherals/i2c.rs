use crate::config::*;
use crate::peripherals::tlv320aic3104::{Tlv320aic3104, TLV320AIC3104_I2C_ADDR_WRITE};

#[derive(Debug)]
pub struct I2cPeripheral {
    pub base: u32,
    pub cr1: u32,
    pub cr2: u32,
    pub oar1: u32,
    pub oar2: u32,
    pub ccr: u32,
    pub trise: u32,
    pub fltr: u32,
    pub sr1: u32,
    pub sr2: u32,
    pub sr1_read: bool,
    pub target_addr: u8,
    pub bytes_sent: Vec<u8>,
    pub stop_pending: bool,
    pub codec: Tlv320aic3104,
    pub current_reg: u8,
}

pub type I2c1Peripheral = I2cPeripheral;

impl Default for I2cPeripheral {
    fn default() -> Self {
        Self::with_base(I2C1_BASE)
    }
}

impl I2cPeripheral {
    pub fn new() -> Self {
        Self::with_base(I2C1_BASE)
    }

    pub fn with_base(base: u32) -> Self {
        Self {
            base,
            cr1: 0,
            cr2: 0,
            oar1: 0,
            oar2: 0,
            ccr: 0,
            trise: 0x0002,
            fltr: 0,
            sr1: 0,
            sr2: 0,
            sr1_read: false,
            target_addr: 0,
            bytes_sent: Vec::new(),
            stop_pending: false,
            codec: Tlv320aic3104::new(),
            current_reg: 0,
        }
    }

    pub fn read(&mut self, addr: u32) -> u32 {
        match addr.wrapping_sub(self.base) {
            0x00 => self.cr1 & !0x0300,
            0x04 => self.cr2,
            0x08 => self.oar1,
            0x0C => self.oar2,
            0x10 => {
                let val = if (self.target_addr & 0xFE) == TLV320AIC3104_I2C_ADDR_WRITE {
                    self.codec.read_reg(self.current_reg) as u32
                } else {
                    0
                };
                self.sr1 &= !0x0040; // Reading DR clears RxNE
                if self.stop_pending {
                    self.sr2 = 0;
                    self.sr1 = 0;
                    self.stop_pending = false;
                } else {
                    self.current_reg = self.current_reg.wrapping_add(1);
                    self.sr1 |= 0x0040; // Multi-byte read: re-assert RxNE for next byte
                }
                val
            }
            0x14 => {
                self.sr1_read = true;
                self.sr1
            }
            0x18 => {
                if self.sr1_read {
                    self.sr1 &= !0x0002; // Reading SR2 after SR1 clears ADDR
                    if (self.target_addr & 1) != 0 {
                        self.sr1 |= 0x0040; // RxNE
                    } else {
                        self.sr1 |= 0x0084; // TxE | BTF
                    }
                    self.sr1_read = false;
                }
                self.sr2
            }
            0x1C => self.trise,
            0x20 => self.fltr,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, val: u32) {
        match addr.wrapping_sub(self.base) {
            0x00 => {
                self.cr1 = val & !0x0300;
                if (val & 0x0200) != 0 {
                    // STOP condition
                    if (self.target_addr & 1) != 0 {
                        self.stop_pending = true;
                    } else {
                        self.sr1 = 0;
                        self.sr2 = 0;
                        self.stop_pending = false;
                        self.bytes_sent.clear();
                    }
                } else if (val & 0x0100) != 0 {
                    // START condition
                    self.sr1 |= 0x0001; // SB (Start Bit)
                    self.sr2 |= 0x0003; // MSL | BUSY
                    self.stop_pending = false;
                    self.bytes_sent.clear();
                }
            }
            0x04 => self.cr2 = val,
            0x08 => self.oar1 = val,
            0x0C => self.oar2 = val,
            0x10 => {
                let byte = (val & 0xFF) as u8;
                if (self.sr1 & 0x0001) != 0 {
                    // Address byte following START
                    self.target_addr = byte;
                    self.sr1 &= !0x0001; // Clear SB
                    self.sr1 |= 0x0002; // Set ADDR
                    if (byte & 1) != 0 {
                        self.sr2 = (self.sr2 & !0x0004) | 0x0003; // Master RX: MSL | BUSY
                    } else {
                        self.sr2 |= 0x0007; // Master TX: MSL | BUSY | TRA
                    }
                } else {
                    if self.bytes_sent.is_empty() {
                        self.current_reg = byte;
                    } else {
                        if (self.target_addr & 0xFE) == TLV320AIC3104_I2C_ADDR_WRITE {
                            self.codec.write_reg(self.current_reg, byte);
                        }
                        self.current_reg = self.current_reg.wrapping_add(1);
                    }
                    self.bytes_sent.push(byte);
                    self.sr1 |= 0x0084; // TxE | BTF
                }
            }
            0x14 => {
                self.sr1 &= val & 0xFFFF;
            }
            0x1C => self.trise = val,
            0x20 => self.fltr = val,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i2c1_multi_byte_read() {
        let mut i2c = I2c1Peripheral::new();
        // Setup codec registers on Page 0 (Reg 5 & 6)
        i2c.codec.write_reg(0x05, 0xAA);
        i2c.codec.write_reg(0x06, 0x55);

        // Start write transaction to select register 0x05 on device 0x30
        i2c.write(I2C1_CR1, 0x0100); // START
        assert_ne!(i2c.sr1 & 0x0001, 0); // SB set
        i2c.write(I2C1_DR, 0x30); // Codec slave address write (even)
        i2c.read(I2C1_SR1);
        i2c.read(I2C1_SR2);
        i2c.write(I2C1_DR, 0x05); // Register address

        // Restart transaction to read from device 0x31
        i2c.write(I2C1_CR1, 0x0100); // Repeated START
        i2c.write(I2C1_DR, 0x31); // Codec slave address read (odd)
        i2c.read(I2C1_SR1);
        i2c.read(I2C1_SR2);
        assert_ne!(i2c.sr1 & 0x0040, 0); // RxNE set

        // Multi-byte read: first byte (stop_pending = false)
        let b1 = i2c.read(I2C1_DR);
        assert_eq!(b1, 0xAA);
        assert_ne!(i2c.sr1 & 0x0040, 0); // RxNE re-asserted for multi-byte read
        assert_eq!(i2c.current_reg, 0x06); // Advanced to next register

        // Set STOP pending before reading final byte
        i2c.write(I2C1_CR1, 0x0200); // STOP
        let b2 = i2c.read(I2C1_DR);
        assert_eq!(b2, 0x55);
        assert_eq!(i2c.sr1 & 0x0040, 0); // RxNE cleared, STOP consumed
    }
}
