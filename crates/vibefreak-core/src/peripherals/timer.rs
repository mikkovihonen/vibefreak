//! STM32F446 unified timer peripheral emulation (TIM1..TIM14).

#[derive(Debug, Default, Clone, Copy)]
pub struct TimerBlock {
    pub cr1: u32,
    pub cr2: u32,
    pub smcr: u32,
    pub dier: u32,
    pub sr: u32,
    pub egr: u32,
    pub ccmr1: u32,
    pub ccmr2: u32,
    pub ccer: u32,
    pub cnt: u32,
    pub psc: u32,
    pub arr: u32,
    pub rcr: u32,
    pub ccr1: u32,
    pub ccr2: u32,
    pub ccr3: u32,
    pub ccr4: u32,
    pub bdtr: u32,
    pub dcr: u32,
    pub dmar: u32,

    pub psc_accum: u32,
    pub is_32bit: bool,
}

impl TimerBlock {
    pub fn read(&self, offset: u32) -> u32 {
        match offset {
            0x00 => self.cr1,
            0x04 => self.cr2,
            0x08 => self.smcr,
            0x0C => self.dier,
            0x10 => self.sr,
            0x14 => 0, // EGR is write-only
            0x18 => self.ccmr1,
            0x1C => self.ccmr2,
            0x20 => self.ccer,
            0x24 => self.cnt,
            0x28 => self.psc,
            0x2C => self.arr,
            0x30 => self.rcr,
            0x34 => self.ccr1,
            0x38 => self.ccr2,
            0x3C => self.ccr3,
            0x40 => self.ccr4,
            0x44 => self.bdtr,
            0x48 => self.dcr,
            0x4C => self.dmar,
            _ => 0,
        }
    }

    pub fn write(&mut self, offset: u32, val: u32) {
        let mask = if self.is_32bit { 0xFFFF_FFFF } else { 0xFFFF };
        match offset {
            0x00 => self.cr1 = val,
            0x04 => self.cr2 = val,
            0x08 => self.smcr = val,
            0x0C => self.dier = val,
            0x10 => self.sr &= val, // rc_w0 bits in SR
            0x14 => {
                self.egr = val;
                if (val & 1) != 0 {
                    // Update Generation (UG): resets CNT and prescaler counter
                    self.cnt = 0;
                    self.psc_accum = 0;
                    // and sets UIF if UDIS not set
                    if (self.cr1 & (1 << 1)) == 0 {
                        self.sr |= 1 << 0; // UIF
                    }
                }
            }
            0x18 => self.ccmr1 = val,
            0x1C => self.ccmr2 = val,
            0x20 => self.ccer = val,
            0x24 => self.cnt = val & mask,
            0x28 => self.psc = val & 0xFFFF,
            0x2C => self.arr = val & mask,
            0x30 => self.rcr = val & 0xFFFF,
            0x34 => self.ccr1 = val & mask,
            0x38 => self.ccr2 = val & mask,
            0x3C => self.ccr3 = val & mask,
            0x40 => self.ccr4 = val & mask,
            0x44 => self.bdtr = val,
            0x48 => self.dcr = val,
            0x4C => self.dmar = val,
            _ => {}
        }
    }

    pub fn is_external_clock_mode_1(&self) -> bool {
        (self.smcr & 0x07) == 0x07
    }

    pub fn slave_trigger_selection(&self) -> u32 {
        (self.smcr >> 4) & 0x07
    }

    pub fn step_cycles(&mut self, cycles: u32) -> u64 {
        // In External Clock Mode 1, counter is clocked by TRGI, not internal bus clock
        if self.is_external_clock_mode_1() {
            return 0;
        }

        // Only count if counter is enabled (CEN bit 0 of CR1)
        if (self.cr1 & 1) == 0 || cycles == 0 {
            return 0;
        }

        self.psc_accum += cycles;
        let prescaler_period = (self.psc & 0xFFFF) + 1;
        let ticks = (self.psc_accum / prescaler_period) as u64;
        self.psc_accum %= prescaler_period;

        if ticks == 0 {
            return 0;
        }

        self.step_ticks(ticks)
    }

    pub fn step_ticks(&mut self, ticks: u64) -> u64 {
        if (self.cr1 & 1) == 0 || ticks == 0 {
            return 0;
        }

        let mask = if self.is_32bit { 0xFFFF_FFFFu64 } else { 0xFFFFu64 };
        let arr = (self.arr as u64) & mask;
        let period = arr + 1;

        let ccr1 = (self.ccr1 as u64) & mask;
        let ccr2 = (self.ccr2 as u64) & mask;
        let ccr3 = (self.ccr3 as u64) & mask;
        let ccr4 = (self.ccr4 as u64) & mask;

        let start_cnt = (self.cnt as u64) & mask;
        let total_cnt = start_cnt + ticks;
        let wraps = total_cnt / period;
        let new_cnt = total_cnt % period;

        if wraps > 0 {
            if (self.cr1 & (1 << 1)) == 0 {
                self.sr |= 1 << 0; // UIF
            }

            if (self.cr1 & (1 << 3)) != 0 {
                // One-Pulse Mode: stop on update event
                self.cr1 &= !1;
                self.cnt = 0;
                return wraps;
            }

            if wraps >= 2 {
                if ccr1 <= arr { self.sr |= 1 << 1; }
                if ccr2 <= arr { self.sr |= 1 << 2; }
                if ccr3 <= arr { self.sr |= 1 << 3; }
                if ccr4 <= arr { self.sr |= 1 << 4; }
            } else {
                let check_wrap = |ccr: u64| -> bool {
                    ccr <= new_cnt || (start_cnt < ccr && ccr <= arr)
                };
                if check_wrap(ccr1) { self.sr |= 1 << 1; }
                if check_wrap(ccr2) { self.sr |= 1 << 2; }
                if check_wrap(ccr3) { self.sr |= 1 << 3; }
                if check_wrap(ccr4) { self.sr |= 1 << 4; }
            }
        } else {
            let check_no_wrap = |ccr: u64| -> bool {
                if start_cnt == 0 {
                    ccr <= new_cnt
                } else {
                    start_cnt < ccr && ccr <= new_cnt
                }
            };
            if check_no_wrap(ccr1) { self.sr |= 1 << 1; }
            if check_no_wrap(ccr2) { self.sr |= 1 << 2; }
            if check_no_wrap(ccr3) { self.sr |= 1 << 3; }
            if check_no_wrap(ccr4) { self.sr |= 1 << 4; }
        }

        self.cnt = (new_cnt as u32) & (mask as u32);
        wraps
    }
}

/// Internal Trigger (ITR) master lookup table for STM32F446 (Reference Manual RM0390 Table 94).
fn get_itr_master(slave_idx: usize, ts: u32) -> Option<usize> {
    match slave_idx {
        0 /* TIM1 */ => match ts { 0 => Some(4), 1 => Some(1), 2 => Some(2), 3 => Some(3), _ => None },
        7 /* TIM8 */ => match ts { 0 => Some(0), 1 => Some(1), 2 => Some(3), 3 => Some(4), _ => None },
        1 /* TIM2 */ => match ts { 0 => Some(0), 1 => Some(7), 2 => Some(2), 3 => Some(3), _ => None },
        2 /* TIM3 */ => match ts { 0 => Some(0), 1 => Some(1), 2 => Some(4), 3 => Some(3), _ => None },
        3 /* TIM4 */ => match ts { 0 => Some(0), 1 => Some(1), 2 => Some(2), 3 => Some(7), _ => None },
        4 /* TIM5 */ => match ts { 0 => Some(1), 1 => Some(2), 2 => Some(3), 3 => Some(7), _ => None },
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct TimerManager {
    // 14 timer instances (TIM1..TIM14, index 0..13)
    pub timers: [TimerBlock; 14],
    pub apb1_cycle_accum: u32,
}

impl Default for TimerManager {
    fn default() -> Self {
        let mut mgr = Self {
            timers: [TimerBlock::default(); 14],
            apb1_cycle_accum: 0,
        };
        mgr.timers[1].is_32bit = true; // TIM2
        mgr.timers[4].is_32bit = true; // TIM5
        mgr
    }
}

impl TimerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn resolve_mut(&mut self, addr: u32) -> Option<(&mut TimerBlock, u32)> {
        let (idx, offset) = match addr {
            0x4000_0000..=0x4000_03FF => (1, addr - 0x4000_0000), // TIM2
            0x4000_0400..=0x4000_07FF => (2, addr - 0x4000_0400), // TIM3
            0x4000_0800..=0x4000_0BFF => (3, addr - 0x4000_0800), // TIM4
            0x4000_0C00..=0x4000_0FFF => (4, addr - 0x4000_0C00), // TIM5
            0x4000_1000..=0x4000_13FF => (5, addr - 0x4000_1000), // TIM6
            0x4000_1400..=0x4000_17FF => (6, addr - 0x4000_1400), // TIM7
            0x4000_1800..=0x4000_1BFF => (11, addr - 0x4000_1800), // TIM12
            0x4000_1C00..=0x4000_1FFF => (12, addr - 0x4000_1C00), // TIM13
            0x4000_2000..=0x4000_23FF => (13, addr - 0x4000_2000), // TIM14
            0x4001_0000..=0x4001_03FF => (0, addr - 0x4001_0000), // TIM1
            0x4001_0400..=0x4001_07FF => (7, addr - 0x4001_0400), // TIM8
            0x4001_4000..=0x4001_43FF => (8, addr - 0x4001_4000), // TIM9
            0x4001_4400..=0x4001_47FF => (9, addr - 0x4001_4400), // TIM10
            0x4001_4800..=0x4001_4BFF => (10, addr - 0x4001_4800), // TIM11
            _ => return None,
        };
        Some((&mut self.timers[idx], offset))
    }

    pub fn read(&mut self, addr: u32) -> Option<u32> {
        let (timer, offset) = self.resolve_mut(addr)?;
        Some(timer.read(offset))
    }

    pub fn write(&mut self, addr: u32, val: u32) -> bool {
        if let Some((timer, offset)) = self.resolve_mut(addr) {
            timer.write(offset, val);
            true
        } else {
            false
        }
    }

    pub fn step_cycles(&mut self, cpu_cycles: u32) {
        if cpu_cycles == 0 {
            return;
        }

        let mut trgo_pulses: Vec<(usize, u64)> = Vec::new();

        // APB2 Timers (TIM1, TIM8, TIM9, TIM10, TIM11) clock at 180 MHz (1:1 with CPU cycles)
        for &idx in &[0, 7, 8, 9, 10] {
            let wraps = self.timers[idx].step_cycles(cpu_cycles);
            if wraps > 0 {
                let mms = (self.timers[idx].cr2 >> 4) & 0x07;
                if mms == 0b010 {
                    trgo_pulses.push((idx, wraps));
                }
            }
        }

        // APB1 Timers (TIM2, TIM3, TIM4, TIM5, TIM6, TIM7, TIM12, TIM13, TIM14) clock at 90 MHz (1 timer tick per 2 CPU cycles)
        let total_apb1 = self.apb1_cycle_accum + cpu_cycles;
        let apb1_cycles = total_apb1 / 2;
        self.apb1_cycle_accum = total_apb1 % 2;

        if apb1_cycles > 0 {
            for &idx in &[1, 2, 3, 4, 5, 6, 11, 12, 13] {
                let wraps = self.timers[idx].step_cycles(apb1_cycles);
                if wraps > 0 {
                    let mms = (self.timers[idx].cr2 >> 4) & 0x07;
                    if mms == 0b010 {
                        trgo_pulses.push((idx, wraps));
                    }
                }
            }
        }

        // Propagate TRGO pulses to slaved timers in External Clock Mode 1
        while let Some((master_idx, pulses)) = trgo_pulses.pop() {
            for slave_idx in 0..self.timers.len() {
                if self.timers[slave_idx].is_external_clock_mode_1() {
                    let ts = self.timers[slave_idx].slave_trigger_selection();
                    if get_itr_master(slave_idx, ts) == Some(master_idx) {
                        let s_psc = (self.timers[slave_idx].psc & 0xFFFF) + 1;
                        self.timers[slave_idx].psc_accum += pulses as u32;
                        let s_ticks = (self.timers[slave_idx].psc_accum / s_psc) as u64;
                        self.timers[slave_idx].psc_accum %= s_psc;
                        if s_ticks > 0 {
                            let s_wraps = self.timers[slave_idx].step_ticks(s_ticks);
                            if s_wraps > 0 {
                                let s_mms = (self.timers[slave_idx].cr2 >> 4) & 0x07;
                                if s_mms == 0b010 {
                                    trgo_pulses.push((slave_idx, s_wraps));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn is_tim5_irq_active(&self) -> bool {
        (self.timers[4].sr & self.timers[4].dier & 1) != 0
    }

    pub fn is_tim8_cc_irq_active(&self) -> bool {
        (self.timers[7].sr & self.timers[7].dier & 0x1E) != 0
    }

    pub fn is_tim8_trg_com_irq_active(&self) -> bool {
        (self.timers[7].sr & self.timers[7].dier & 0x60) != 0
    }

    pub fn is_tim8_up_irq_active(&self) -> bool {
        (self.timers[7].sr & self.timers[7].dier & 1) != 0
    }
}
