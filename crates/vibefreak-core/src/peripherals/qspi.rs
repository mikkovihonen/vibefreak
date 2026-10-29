use crate::config::*;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct QspiPeripheral {
    pub cr: u32,
    pub dcr: u32,
    pub sr: u32,
    pub dlr: u32,
    pub ccr: u32,
    pub ar: u32,
    pub abr: u32,
    pub psmkr: u32,
    pub psmar: u32,
    pub pir: u32,
    pub lptr: u32,
    pub dr_fifo: VecDeque<u8>,
    pub wel: bool,
    pub write_bytes_remaining: usize,
    pub status_reg1: u8,
    pub status_reg2: u8,
    pub status_reg3: u8,
    pub flash_memory: Vec<u8>,
    pub dirty: bool,
    pub pending_sync: Vec<(usize, usize)>,
}

impl Default for QspiPeripheral {
    fn default() -> Self {
        Self {
            cr: 0,
            dcr: 0,
            sr: (1 << 1) | (1 << 2), // TCF | FTF initial readiness
            dlr: 0,
            ccr: 0,
            ar: 0,
            abr: 0,
            psmkr: 0,
            psmar: 0,
            pir: 0,
            lptr: 0,
            dr_fifo: VecDeque::new(),
            wel: false,
            write_bytes_remaining: 0,
            status_reg1: 0,
            status_reg2: 0,
            status_reg3: 0,
            flash_memory: vec![0xFFu8; QSPI_FLASH_SIZE],
            dirty: false,
            pending_sync: Vec::new(),
        }
    }
}

impl QspiPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_flash_image(&mut self, data: &[u8]) {
        let len = data.len().min(self.flash_memory.len());
        self.flash_memory[..len].copy_from_slice(&data[..len]);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    pub fn is_irq_active(&self) -> bool {
        let cr = self.cr;
        let sr = self.sr;
        ((cr & (1 << 20)) != 0 && (sr & (1 << 4)) != 0) // TOIE && TOF
            || ((cr & (1 << 19)) != 0 && (sr & (1 << 3)) != 0) // SMIE && SMF
            || ((cr & (1 << 18)) != 0 && (sr & (1 << 2)) != 0) // FTIE && FTF
            || ((cr & (1 << 17)) != 0 && (sr & (1 << 1)) != 0) // TCIE && TCF
            || ((cr & (1 << 16)) != 0 && (sr & (1 << 0)) != 0) // TEIE && TEF
    }

    fn update_fifo_level(&mut self) {
        let level = self.dr_fifo.len().min(31) as u32;
        self.sr = (self.sr & !0x1F00) | (level << 8);

        if self.write_bytes_remaining > 0 {
            // Write transfer in progress: FIFO ready to accept writes
            self.sr |= 1 << 2;
        } else if !self.dr_fifo.is_empty() {
            // Read data available in FIFO
            self.sr |= 1 << 2;
        } else {
            self.sr &= !(1 << 2);
        }
    }

    pub fn execute_command(&mut self) {
        let instruction = (self.ccr & 0xFF) as u8;
        let fmode = (self.ccr >> 26) & 0x03;
        let dmode = (self.ccr >> 24) & 0x03;
        let dlen = (self.dlr + 1) as usize;

        if fmode == 0b10 {
            // Automatic polling mode
            let status = match instruction {
                0x35 => self.status_reg2 as u32,
                0x15 => self.status_reg3 as u32,
                _ => (self.status_reg1 as u32) | if self.wel { 0x02 } else { 0x00 },
            };
            if (status & self.psmkr) == (self.psmar & self.psmkr) {
                self.sr |= (1 << 3) | (1 << 1); // SMF | TCF
                self.sr &= !(1 << 5); // Clear BUSY
            }
            self.update_fifo_level();
            return;
        }

        if fmode == 0b11 {
            // Memory-mapped mode
            self.sr &= !(1 << 5); // Clear BUSY
            self.sr |= 1 << 1;    // TCF
            self.update_fifo_level();
            return;
        }

        // Commands that write data (WRSR, Page Program)
        if matches!(instruction, 0x01 | 0x02 | 0x32) && (fmode == 0b00 || dmode != 0) {
            self.write_bytes_remaining = dlen;
            self.sr &= !(1 << 1); // Clear TCF (transfer not yet finished)
            self.sr |= 1 << 5;   // Set BUSY
            self.update_fifo_level();
            return;
        }

        // Commands with no data phase (Write Enable/Disable, Erase, QPI Enter/Exit)
        if matches!(instruction, 0x06 | 0x04 | 0x20 | 0xD7 | 0xD8 | 0xC7 | 0x38 | 0xF5) {
            self.write_bytes_remaining = 0;
            match instruction {
                0x06 => self.wel = true,
                0x04 => self.wel = false,
                0x20 | 0xD7 => {
                    let start = ((self.ar as usize) & !0xFFF).min(self.flash_memory.len());
                    let end = (start + 4096).min(self.flash_memory.len());
                    self.flash_memory[start..end].fill(0xFF);
                    self.wel = false;
                    self.dirty = true;
                    self.pending_sync.push((start, end - start));
                }
                0xD8 => {
                    let start = ((self.ar as usize) & !0xFFFF).min(self.flash_memory.len());
                    let end = (start + 65536).min(self.flash_memory.len());
                    self.flash_memory[start..end].fill(0xFF);
                    self.wel = false;
                    self.dirty = true;
                    self.pending_sync.push((start, end - start));
                }
                0xC7 => {
                    self.flash_memory.fill(0xFF);
                    self.wel = false;
                    self.dirty = true;
                    let len = self.flash_memory.len();
                    self.pending_sync.push((0, len));
                }
                _ => {}
            }
            self.sr |= 1 << 1; // TCF
            self.sr &= !(1 << 5); // Clear BUSY
            self.update_fifo_level();
            return;
        }

        // Read commands (JEDEC ID, Status Register read, Indirect read)
        match instruction {
            // Read JEDEC ID (0x9F SPI mode, 0xAF QPI mode)
            0x9F | 0xAF => {
                self.dr_fifo.clear();
                let jedec = [0x9Du8, 0x60, 0x18, 0x00];
                let count = dlen.min(jedec.len());
                for &b in &jedec[..count] {
                    self.dr_fifo.push_back(b);
                }
                while self.dr_fifo.len() < dlen {
                    self.dr_fifo.push_back(0x00);
                }
                self.sr |= (1 << 1) | (1 << 2); // TCF | FTF
                self.sr &= !(1 << 5); // Clear BUSY
            }

            // Read Status Register 1 (0x05), 2 (0x35), 3 (0x15)
            0x05 | 0x35 | 0x15 => {
                let status = match instruction {
                    0x35 => self.status_reg2 as u32,
                    0x15 => self.status_reg3 as u32,
                    _ => (self.status_reg1 as u32) | if self.wel { 0x02 } else { 0x00 },
                };
                self.dr_fifo.clear();
                for _ in 0..dlen {
                    self.dr_fifo.push_back(status as u8);
                }
                self.sr |= (1 << 1) | (1 << 2); // TCF | FTF
                self.sr &= !(1 << 5); // Clear BUSY
            }

            // Indirect Read: 0x03, 0x0B, 0x3B, 0x6B, 0xEB or any other read command
            _ => {
                self.dr_fifo.clear();
                let start = (self.ar as usize).min(self.flash_memory.len());
                let end = (start + dlen).min(self.flash_memory.len());
                for &b in &self.flash_memory[start..end] {
                    self.dr_fifo.push_back(b);
                }
                while self.dr_fifo.len() < dlen {
                    self.dr_fifo.push_back(0xFF);
                }
                self.sr |= (1 << 1) | (1 << 2); // TCF | FTF
                self.sr &= !(1 << 5); // Clear BUSY
            }
        }

        self.update_fifo_level();
    }

    pub fn read_dr(&mut self, size: usize) -> u32 {
        let mut val = 0u32;
        let bytes_to_read = match size {
            1 => 1,
            2 => 2,
            _ => 4,
        };
        for i in 0..bytes_to_read {
            let b = self.dr_fifo.pop_front().unwrap_or(0xFF);
            val |= (b as u32) << (i * 8);
        }
        self.update_fifo_level();
        val
    }

    pub fn write_dr(&mut self, size: usize, val: u32) {
        let bytes_to_write = match size {
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let instruction = (self.ccr & 0xFF) as u8;
        let dlen = (self.dlr + 1) as usize;

        for i in 0..bytes_to_write {
            let b = ((val >> (i * 8)) & 0xFF) as u8;
            if self.write_bytes_remaining > 0 {
                self.write_bytes_remaining -= 1;
            }

            match instruction {
                0x02 | 0x32 => {
                    let offset = self.ar as usize;
                    if offset < self.flash_memory.len() {
                        self.flash_memory[offset] &= b;
                        self.dirty = true;
                        self.pending_sync.push((offset, 1));
                    }
                    self.ar = self.ar.wrapping_add(1);
                }
                0x01 => {
                    if self.write_bytes_remaining == dlen.saturating_sub(1) {
                        self.status_reg1 = b;
                    } else {
                        self.status_reg2 = b;
                    }
                }
                _ => {
                    self.dr_fifo.push_back(b);
                }
            }

            if self.write_bytes_remaining == 0 {
                self.sr |= 1 << 1; // TCF
                self.sr &= !(1 << 5); // Clear BUSY
                self.sr &= !(1 << 2); // Clear FTF
                if matches!(instruction, 0x01 | 0x02 | 0x32) {
                    self.wel = false;
                }
                break;
            }
        }
        self.update_fifo_level();
    }

    pub fn read(&mut self, addr: u32, size: usize) -> u32 {
        match addr {
            QUADSPI_CR => self.cr & !(1 << 1), // ensure ABORT bit is 0
            QUADSPI_DCR => self.dcr,
            QUADSPI_SR => self.sr,
            QUADSPI_FCR => 0,
            QUADSPI_DLR => self.dlr,
            QUADSPI_CCR => self.ccr,
            QUADSPI_AR => self.ar,
            QUADSPI_ABR => self.abr,
            QUADSPI_DR => self.read_dr(size),
            QUADSPI_PSMKR => self.psmkr,
            QUADSPI_PSMAR => self.psmar,
            QUADSPI_PIR => self.pir,
            QUADSPI_LPTR => self.lptr,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, size: usize, val: u32) {
        match addr {
            QUADSPI_CR => {
                self.cr = val & !(1 << 1);
                if (val & (1 << 1)) != 0 {
                    self.dr_fifo.clear();
                    self.write_bytes_remaining = 0;
                    self.sr &= !((1 << 5) | (1 << 1) | (1 << 2));
                    self.sr |= 1 << 1; // TCF on abort
                    self.update_fifo_level();
                }
            }
            QUADSPI_DCR => self.dcr = val,
            QUADSPI_SR => self.sr = val,
            QUADSPI_FCR => {
                if (val & (1 << 0)) != 0 {
                    self.sr &= !(1 << 0); // Clear TEF
                }
                if (val & (1 << 1)) != 0 {
                    self.sr &= !(1 << 1); // Clear TCF
                }
                if (val & (1 << 3)) != 0 {
                    self.sr &= !(1 << 3); // Clear SMF
                }
                if (val & (1 << 4)) != 0 {
                    self.sr &= !(1 << 4); // Clear TOF
                }
            }
            QUADSPI_DLR => self.dlr = val,
            QUADSPI_CCR => {
                self.ccr = val;
                let admode = (val >> 10) & 0x03;
                if admode == 0 {
                    self.execute_command();
                }
            }
            QUADSPI_AR => {
                self.ar = val;
                let admode = (self.ccr >> 10) & 0x03;
                if admode != 0 {
                    self.execute_command();
                }
            }
            QUADSPI_ABR => self.abr = val,
            QUADSPI_DR => self.write_dr(size, val),
            QUADSPI_PSMKR => self.psmkr = val,
            QUADSPI_PSMAR => self.psmar = val,
            QUADSPI_PIR => self.pir = val,
            QUADSPI_LPTR => self.lptr = val,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qspi_jedec_id_command() {
        let mut qspi = QspiPeripheral::new();
        // DLR = 2 (transfer length = 3 bytes)
        qspi.write(QUADSPI_DLR, 4, 2);
        // CCR = 0x9F (Read JEDEC ID), ADMODE = 0
        qspi.write(QUADSPI_CCR, 4, 0x9F);

        // Verify TCF (bit 1) and FTF (bit 2) are asserted
        assert_ne!(qspi.read(QUADSPI_SR, 4) & (1 << 1), 0, "TCF must be set");
        assert_ne!(qspi.read(QUADSPI_SR, 4) & (1 << 2), 0, "FTF must be set");

        // Read 3 bytes individually via DR (size = 1)
        let b0 = qspi.read(QUADSPI_DR, 1) as u8;
        let b1 = qspi.read(QUADSPI_DR, 1) as u8;
        let b2 = qspi.read(QUADSPI_DR, 1) as u8;

        assert_eq!(b0, 0x9D);
        assert_eq!(b1, 0x60);
        assert_eq!(b2, 0x18);

        // FIFO should now be empty and FTF cleared
        assert_eq!(qspi.read(QUADSPI_SR, 4) & (1 << 2), 0, "FTF must be cleared when empty");
    }

    #[test]
    fn test_qspi_auto_polling_status_register() {
        let mut qspi = QspiPeripheral::new();
        // Status register polling: mask = 0x02, match = 0x02 (waiting for WEL)
        qspi.write(QUADSPI_PSMKR, 4, 0x02);
        qspi.write(QUADSPI_PSMAR, 4, 0x02);

        // Functional mode = 0b10 (Auto-polling), Instruction = 0x05, ADMODE = 0
        let ccr_poll = (0b10 << 26) | 0x05;
        qspi.write(QUADSPI_CCR, 4, ccr_poll);

        // Initially WEL is false, so no match yet (SMF = 0)
        assert_eq!(qspi.read(QUADSPI_SR, 4) & (1 << 3), 0, "SMF must not be set when WEL is false");

        // Now issue Write Enable (0x06)
        qspi.write(QUADSPI_CCR, 4, 0x06);
        assert!(qspi.wel);

        // Issue auto-polling command again
        qspi.write(QUADSPI_CCR, 4, ccr_poll);
        // Now WEL is true (0x02 & 0x02 == 0x02), SMF and TCF must be set
        assert_ne!(qspi.read(QUADSPI_SR, 4) & (1 << 3), 0, "SMF must be set on match");
        assert_ne!(qspi.read(QUADSPI_SR, 4) & (1 << 1), 0, "TCF must be set on match");

        // Clear SMF and TCF via FCR: CSMFF (bit 3) | CTCFF (bit 1)
        qspi.write(QUADSPI_FCR, 4, (1 << 3) | (1 << 1));
        assert_eq!(qspi.read(QUADSPI_SR, 4) & (1 << 3), 0, "SMF cleared");
        assert_eq!(qspi.read(QUADSPI_SR, 4) & (1 << 1), 0, "TCF cleared");
    }

    #[test]
    fn test_qspi_indirect_read_and_erase() {
        let mut qspi = QspiPeripheral::new();
        let test_data = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
        qspi.load_flash_image(&test_data);

        // Configure Indirect Read: DLR = 3 (4 bytes), ADMODE = 1 (Single line address), Instruction = 0x03
        qspi.write(QUADSPI_DLR, 4, 3);
        let ccr = (1 << 10) | 0x03; // ADMODE = 1, Instruction = 0x03
        qspi.write(QUADSPI_CCR, 4, ccr);
        // Write address AR = 0x0000_0000 -> triggers command
        qspi.write(QUADSPI_AR, 4, 0);

        // Read 4 bytes as word (size = 4)
        let word = qspi.read(QUADSPI_DR, 4);
        assert_eq!(word, 0x44332211);

        // Sector Erase at AR = 0: Instruction = 0x20
        qspi.write(QUADSPI_CCR, 4, (1 << 10) | 0x20);
        qspi.write(QUADSPI_AR, 4, 0);

        // Read again from 0
        qspi.write(QUADSPI_CCR, 4, (1 << 10) | 0x03);
        qspi.write(QUADSPI_AR, 4, 0);
        let erased_word = qspi.read(QUADSPI_DR, 4);
        assert_eq!(erased_word, 0xFFFF_FFFF);
    }

    #[test]
    fn test_qspi_indirect_write_status_register() {
        let mut qspi = QspiPeripheral::new();
        // Step 1: Write Enable (0x06)
        qspi.write(QUADSPI_CCR, 4, 0x06);
        assert!(qspi.wel);

        // Step 2: Configure WRSR (0x01) with 1 data byte: DLR = 0, DMODE = 1, FMODE = 00
        qspi.write(QUADSPI_DLR, 4, 0);
        let ccr_wrsr = (1 << 24) | 0x01; // DMODE = 1, FMODE = 00, Instruction = 0x01
        qspi.write(QUADSPI_CCR, 4, ccr_wrsr);

        // FTF must be asserted (ready to accept write) and TCF must be 0 (transfer in progress)
        let sr = qspi.read(QUADSPI_SR, 4);
        assert_ne!(sr & (1 << 2), 0, "FTF must be asserted during indirect write");
        assert_eq!(sr & (1 << 1), 0, "TCF must be 0 while write transfer is in progress");
        assert_ne!(sr & (1 << 5), 0, "BUSY must be 1 while write transfer is in progress");

        // Write 1 byte with QE bit (0x40) to QUADSPI_DR
        qspi.write(QUADSPI_DR, 1, 0x40);

        // After all bytes written: TCF must be 1, FTF must be 0, BUSY must be 0
        let sr_after = qspi.read(QUADSPI_SR, 4);
        assert_ne!(sr_after & (1 << 1), 0, "TCF must be asserted after write completes");
        assert_eq!(sr_after & (1 << 2), 0, "FTF must be 0 after write completes");
        assert_eq!(sr_after & (1 << 5), 0, "BUSY must be 0 after write completes");

        // Clear TCF via FCR (CTCFF = bit 1)
        qspi.write(QUADSPI_FCR, 4, 1 << 1);
        assert_eq!(qspi.read(QUADSPI_SR, 4) & (1 << 1), 0, "TCF must be cleared");

        // Read Status Register 1 (0x05) to verify QE bit is set
        qspi.write(QUADSPI_DLR, 4, 0);
        qspi.write(QUADSPI_CCR, 4, (1 << 24) | (1 << 26) | 0x05); // DMODE = 1, FMODE = 01 (read), Instruction = 0x05
        let status = qspi.read(QUADSPI_DR, 1);
        assert_eq!(status, 0x40, "Status register 1 must retain QE bit");
    }
}
