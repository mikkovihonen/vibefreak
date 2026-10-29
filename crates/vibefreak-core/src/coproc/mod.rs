pub mod armv6m;
pub mod keyboard;
pub mod matrix;
pub mod oled;

use self::keyboard::KeyboardCoprocessor;
use self::matrix::{MatrixCoprocessor, ICON_LEDS_COUNT, MATRIX_TOTAL_LEDS, PANEL_LEDS_COUNT};
use self::oled::OledCoprocessor;
use std::collections::VecDeque;
use std::path::Path;

pub const SYNC_BYTE: u8 = 0xAA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsartBusId {
    Usart1,
    Usart2,
}

pub struct CoProcessorBusRouter {
    pub oled_bus: UsartBusId,
    pub matrix_bus: UsartBusId,
    pub keyboard_bus: UsartBusId,

    pub oled: OledCoprocessor,
    pub matrix: MatrixCoprocessor,
    pub keyboard: KeyboardCoprocessor,

    pub time_oled: std::time::Duration,
    pub time_matrix: std::time::Duration,
    pub time_kbd: std::time::Duration,

    matrix_rx_buf: VecDeque<u8>,
}

impl CoProcessorBusRouter {
    pub fn from_buffers(
        oled_bin: &[u8],
        matrix_bin: &[u8],
        kbd_bin: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let oled = OledCoprocessor::from_bytes(oled_bin)?;
        let matrix = MatrixCoprocessor::from_bytes(matrix_bin)?;
        let keyboard = KeyboardCoprocessor::from_bytes(kbd_bin)?;

        Ok(Self {
            oled_bus: UsartBusId::Usart2,
            matrix_bus: UsartBusId::Usart2,
            keyboard_bus: UsartBusId::Usart2,
            oled,
            matrix,
            keyboard,
            time_oled: std::time::Duration::ZERO,
            time_matrix: std::time::Duration::ZERO,
            time_kbd: std::time::Duration::ZERO,
            matrix_rx_buf: VecDeque::new(),
        })
    }

    pub fn new<P1: AsRef<Path>, P2: AsRef<Path>, P3: AsRef<Path>>(
        oled_bin: P1,
        matrix_bin: P2,
        kbd_bin: P3,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let oled = std::fs::read(oled_bin)?;
        let matrix = std::fs::read(matrix_bin)?;
        let kbd = std::fs::read(kbd_bin)?;
        Self::from_buffers(&oled, &matrix, &kbd)
    }

    fn process_matrix_stream(
        rx_buf: &mut VecDeque<u8>,
        matrix: &mut MatrixCoprocessor,
    ) {
        loop {
            while !rx_buf.is_empty() && rx_buf[0] != SYNC_BYTE {
                rx_buf.pop_front();
            }
            if rx_buf.len() < 8 {
                break;
            }

            let mut sum: u32 = 0;
            for i in 0..7 {
                sum = sum.wrapping_add(rx_buf[i] as u32);
            }
            let chk_calc = (sum & 0xFF) as u8;

            if chk_calc == rx_buf[7] {
                let cmd = rx_buf[1];
                let mut data = [0u8; 5];
                for i in 0..5 {
                    data[i] = rx_buf[2 + i];
                }
                for _ in 0..8 {
                    rx_buf.pop_front();
                }
                matrix.handle_hardware_frame(cmd, &data);
            } else {
                rx_buf.pop_front();
            }
        }
    }

    pub fn feed_usart2_byte(&mut self, byte: u8) {
        self.oled.feed_bytes(&[byte]);
        self.matrix_rx_buf.push_back(byte);
        Self::process_matrix_stream(&mut self.matrix_rx_buf, &mut self.matrix);
        self.keyboard.feed_usart_rx(byte);
    }

    pub fn drain_usart2_tx(&mut self) -> Vec<u8> {
        let mut tx = self.oled.drain_usart_tx();
        tx.extend(self.matrix.drain_usart_tx());
        tx.extend(self.keyboard.drain_usart_tx());
        tx
    }

    pub fn step_all(&mut self, count: usize) {
        let oled = &mut self.oled;
        let matrix = &mut self.matrix;
        let keyboard = &mut self.keyboard;

        let mut d_oled = std::time::Duration::ZERO;
        let mut d_matrix = std::time::Duration::ZERO;
        let mut d_kbd = std::time::Duration::ZERO;

        let d_oled_ref = &mut d_oled;
        let d_matrix_ref = &mut d_matrix;
        let d_kbd_ref = &mut d_kbd;

        rayon::join(
            || {
                let t0 = std::time::Instant::now();
                oled.step(count);
                *d_oled_ref = t0.elapsed();
            },
            || {
                rayon::join(
                    || {
                        let t1 = std::time::Instant::now();
                        matrix.step(count);
                        *d_matrix_ref = t1.elapsed();
                    },
                    || {
                        let t2 = std::time::Instant::now();
                        keyboard.step(count);
                        *d_kbd_ref = t2.elapsed();
                    },
                );
            },
        );

        self.time_oled += d_oled;
        self.time_matrix += d_matrix;
        self.time_kbd += d_kbd;
    }

    pub fn set_microswitch(&mut self, name: &str, is_pressed: bool) {
        if let Some(idx) = KeyboardCoprocessor::touch_button_index(name) {
            self.keyboard.set_touch_button(idx, is_pressed);
        } else {
            self.matrix.set_microswitch(name, is_pressed);
        }
    }

    pub fn set_pin_level(&mut self, port: char, pin: u8, is_pressed: bool) {
        self.matrix.set_pin_level(port, pin, is_pressed);
    }

    pub fn step_encoder(&mut self, name: &str, delta: i16) {
        self.matrix.step_encoder(name, delta);
    }

    pub fn set_potentiometer(&mut self, group: u8, id: u8, value: u16) {
        self.matrix.set_potentiometer(group, id, value);
    }

    pub fn step_encoder_raw(&mut self, cmd: u8, group: u8, id: u8, delta: i16) {
        self.matrix.step_encoder_raw(cmd, group, id, delta);
    }

    pub fn set_key(&mut self, key_idx: usize, pressure: u8) {
        self.keyboard.set_key(key_idx, pressure);
    }

    pub fn set_touch_strip(&mut self, val: u16, touched: bool) {
        self.keyboard.set_touch_strip(val, touched);
    }

    pub fn get_framebuffer(&self) -> [u8; 1024] {
        self.oled.get_framebuffer()
    }

    pub fn get_matrix_leds(&self) -> [u8; MATRIX_TOTAL_LEDS] {
        self.matrix.leds
    }

    pub fn get_panel_leds(&self) -> [u8; PANEL_LEDS_COUNT] {
        self.matrix.panel_leds
    }

    pub fn get_icon_leds(&self) -> [u8; ICON_LEDS_COUNT] {
        self.matrix.icon_leds
    }
}
