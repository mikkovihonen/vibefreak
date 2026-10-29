use std::collections::VecDeque;

/// STM32F446 USB On-The-Go Full-Speed (OTG_FS) Peripheral Emulation
/// Base Address: 0x5000_0000, Size: 64 KB
#[derive(Debug, Clone)]
pub struct UsbOtgFsPeripheral {
    pub gotgctl: u32,
    pub gotgint: u32,
    pub gahbcfg: u32,
    pub gusbcfg: u32,
    pub grstctl: u32,
    pub gintsts: u32,
    pub gintmsk: u32,
    pub grxfsiz: u32,
    pub dieptxf0: u32,
    pub gccfg: u32,
    pub cid: u32,

    pub dcfg: u32,
    pub dctl: u32,
    pub dsts: u32,
    pub diepmsk: u32,
    pub doepmsk: u32,
    pub daint: u32,
    pub daintmsk: u32,

    pub diepctl0: u32,
    pub diepint0: u32,
    pub dieptsiz0: u32,
    pub dtxfsts0: u32,

    pub diepctl1: u32,
    pub diepint1: u32,
    pub dieptsiz1: u32,
    pub dtxfsts1: u32,

    pub doepctl0: u32,
    pub doepint0: u32,
    pub doeptsiz0: u32,

    pub doepctl1: u32,
    pub doepint1: u32,
    pub doeptsiz1: u32,

    /// Outgoing USB-MIDI 4-byte packets (MicroFreak -> USB Host)
    pub tx_packets: VecDeque<[u8; 4]>,
    /// Incoming USB-MIDI 4-byte packets (USB Host -> MicroFreak)
    pub rx_packets: VecDeque<[u8; 4]>,
}

impl Default for UsbOtgFsPeripheral {
    fn default() -> Self {
        Self {
            gotgctl: 0x0000_0800, // B-device session valid
            gotgint: 0,
            gahbcfg: 0,
            gusbcfg: 0x0000_1440,
            grstctl: 0x8000_0000, // AHB Master Idle = 1, TXFFLSH = 0
            gintsts: 0,
            gintmsk: 0,
            grxfsiz: 0x0000_0200,   // 512 words RX FIFO
            dieptxf0: 0x0200_0200,  // TX FIFO 0
            gccfg: 0x0020_0000,     // NOVBUSSENS = 1
            cid: 0x0000_2000,       // Core ID

            dcfg: 0x0220_0000,
            dctl: 0,
            dsts: 0x0000_0006,      // Full speed device connected
            diepmsk: 0,
            doepmsk: 0,
            daint: 0,
            daintmsk: 0,

            diepctl0: 0,
            diepint0: 0x0000_0080,  // TXFE = 1
            dieptsiz0: 0,
            dtxfsts0: 0x0000_0100,  // 256 words free

            diepctl1: 0,
            diepint1: 0x0000_0080,  // TXFE = 1 (Transmit FIFO Empty / Ready)
            dieptsiz1: 0,
            dtxfsts1: 0x0000_0100,  // 256 words free (> 0x7F)

            doepctl0: 0,
            doepint0: 0,
            doeptsiz0: 0,

            doepctl1: 0,
            doepint1: 0,
            doeptsiz1: 0,

            tx_packets: VecDeque::new(),
            rx_packets: VecDeque::new(),
        }
    }
}

impl UsbOtgFsPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_irq_active(&self) -> bool {
        let unmasked = self.gintsts & self.gintmsk;
        unmasked != 0
    }

    pub fn enqueue_rx_packet(&mut self, packet: [u8; 4]) {
        self.rx_packets.push_back(packet);
        self.gintsts |= 1 << 4; // RXFLVL
    }

    pub fn enqueue_rx_midi(&mut self, cin: u8, status: u8, d1: u8, d2: u8) {
        self.enqueue_rx_packet([cin & 0x0F, status, d1, d2]);
    }

    pub fn drain_tx_packets(&mut self) -> Vec<[u8; 4]> {
        self.tx_packets.drain(..).collect()
    }

    pub fn read(&mut self, addr: u32, _size: usize) -> u32 {
        let offset = addr & 0xFFFF;
        match offset {
            0x000 => self.gotgctl,
            0x004 => self.gotgint,
            0x008 => self.gahbcfg,
            0x00C => self.gusbcfg,
            0x010 => self.grstctl,
            0x014 => {
                let mut status = self.gintsts;
                if !self.rx_packets.is_empty() {
                    status |= 1 << 4; // RXFLVL
                }
                status
            }
            0x018 => self.gintmsk,
            0x01C => {
                // GRXSTSR: debug receive status
                if let Some(packet) = self.rx_packets.front() {
                    // Endpoint 1, byte count = 4, OUT data packet (2)
                    let _ = packet;
                    1 | (4 << 4) | (2 << 17)
                } else {
                    0
                }
            }
            0x020 => {
                // GRXSTSP: receive status read and pop
                if let Some(packet) = self.rx_packets.front() {
                    let _ = packet;
                    1 | (4 << 4) | (2 << 17)
                } else {
                    0
                }
            }
            0x024 => self.grxfsiz,
            0x028 => self.dieptxf0,
            0x038 => self.gccfg,
            0x03C => self.cid,

            0x800 => self.dcfg,
            0x804 => self.dctl,
            0x808 => self.dsts,
            0x810 => self.diepmsk,
            0x814 => self.doepmsk,
            0x818 => self.daint,
            0x81C => self.daintmsk,

            0x900 => self.diepctl0,
            0x908 => self.diepint0,
            0x910 => self.dieptsiz0,
            0x918 => self.dtxfsts0,

            0x920 => self.diepctl1,
            0x928 => self.diepint1,
            0x930 => self.dieptsiz1,
            0x938 => self.dtxfsts1,

            0xB00 => self.doepctl0,
            0xB08 => self.doepint0,
            0xB10 => self.doeptsiz0,

            0xB20 => self.doepctl1,
            0xB28 => self.doepint1,
            0xB30 => self.doeptsiz1,

            // FIFO read: EP0 or EP1
            0x1000..=0x3FFF => {
                if let Some(packet) = self.rx_packets.pop_front() {
                    if self.rx_packets.is_empty() {
                        self.gintsts &= !(1 << 4); // Clear RXFLVL
                    }
                    u32::from_le_bytes(packet)
                } else {
                    0
                }
            }

            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u32, _size: usize, val: u32) {
        let offset = addr & 0xFFFF;
        match offset {
            0x000 => self.gotgctl = val,
            0x004 => self.gotgint &= !val, // W1C
            0x008 => self.gahbcfg = val,
            0x00C => self.gusbcfg = val,
            0x010 => {
                // GRSTCTL: Core Soft Reset (bit 0) or TX FIFO Flush (bit 5)
                // Always clear reset/flush bits immediately to avoid busy loops
                self.grstctl = 0x8000_0000;
            }
            0x014 => self.gintsts &= !val, // W1C
            0x018 => self.gintmsk = val,
            0x024 => self.grxfsiz = val,
            0x028 => self.dieptxf0 = val,
            0x038 => self.gccfg = val,

            0x800 => self.dcfg = val,
            0x804 => self.dctl = val,
            0x810 => self.diepmsk = val,
            0x814 => self.doepmsk = val,
            0x81C => self.daintmsk = val,

            0x900 => self.diepctl0 = val,
            0x908 => self.diepint0 &= !val,
            0x910 => self.dieptsiz0 = val,

            0x920 => self.diepctl1 = val,
            0x928 => self.diepint1 &= !val,
            0x930 => self.dieptsiz1 = val,

            0xB00 => self.doepctl0 = val,
            0xB08 => self.doepint0 &= !val,
            0xB10 => self.doeptsiz0 = val,

            0xB20 => self.doepctl1 = val,
            0xB28 => self.doepint1 &= !val,
            0xB30 => self.doeptsiz1 = val,

            // FIFO write: Packet transmitted by firmware
            0x1000..=0x3FFF => {
                self.tx_packets.push_back(val.to_le_bytes());
                self.diepint1 |= 1 << 0; // XFRC
            }

            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usb_otg_fs_initial_state_and_fifo_transfer() {
        let mut usb = UsbOtgFsPeripheral::new();

        // 1. Verify reset flags required by firmware
        assert_eq!(usb.read(0x5000_0010, 4) & (1 << 5), 0, "GRSTCTL TXFFLSH must be 0");
        assert_ne!(usb.read(0x5000_0928, 4) & (1 << 7), 0, "DIEPINT1 TXFE bit 7 must be set");
        assert!(usb.read(0x5000_0938, 4) > 0x7F, "DTXFSTS1 must report > 0x7F words available");

        // 2. Transmit USB-MIDI packet from firmware into FIFO
        let midi_note_on = [0x09, 0x90, 0x3C, 0x7F]; // CIN 9 (Note On), Status 0x90, Note 60, Vel 127
        usb.write(0x5000_1000, 4, u32::from_le_bytes(midi_note_on));

        let tx = usb.drain_tx_packets();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0], midi_note_on);

        // 3. Ingest USB-MIDI packet from host into RX FIFO
        let midi_cc = [0x0B, 0xB0, 0x01, 0x40]; // Mod wheel
        usb.enqueue_rx_packet(midi_cc);

        // Verify interrupt status bit 4 (RXFLVL)
        assert_ne!(usb.read(0x5000_0014, 4) & (1 << 4), 0, "RXFLVL must be set when packets present");

        // Read packet from FIFO
        let received = usb.read(0x5000_1000, 4);
        assert_eq!(received.to_le_bytes(), midi_cc);

        // FIFO should now be empty and RXFLVL cleared
        assert_eq!(usb.read(0x5000_0014, 4) & (1 << 4), 0, "RXFLVL must be clear when empty");
    }
}
