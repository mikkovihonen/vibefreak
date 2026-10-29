use std::collections::VecDeque;

#[derive(Debug, Default)]
pub struct UsartPeripheral {
    pub cr1: u32,
    pub cr2: u32,
    pub cr3: u32,
    pub brr: u32,
    pub rx_queue: VecDeque<u8>,
    pub tx_queue: VecDeque<u8>,
}

impl UsartPeripheral {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read_sr(&self) -> u32 {
        let mut status = (1 << 7) | (1 << 6); // TXE (Transmit data register empty) | TC (Transmission complete)
        if !self.rx_queue.is_empty() {
            status |= 1 << 5; // RXNE (Read data register not empty)
        }
        status
    }

    pub fn read_dr(&mut self) -> u32 {
        self.rx_queue.pop_front().map(|b| b as u32).unwrap_or(0)
    }

    pub fn write_dr(&mut self, _val: u32) {
        // Transmission is captured in EmulatorContext (usart1_tx_bytes / tx_bytes) and forwarded to coprocessor
    }

    pub fn write_cr1(&mut self, val: u32) {
        self.cr1 = val;
    }

    pub fn push_rx(&mut self, byte: u8) {
        self.rx_queue.push_back(byte);
    }

    pub fn is_txeie(&self) -> bool {
        (self.cr1 & (1 << 7)) != 0
    }

    pub fn is_rxneie(&self) -> bool {
        (self.cr1 & (1 << 5)) != 0
    }
}
