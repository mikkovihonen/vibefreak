use crate::config::*;
use unicorn_engine::{RegisterARM, Unicorn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpbAction {
    None,
    SysResetReq,
}

#[derive(Debug, Clone)]
pub struct NvicController {
    pub vtor: u32,
    pub iser: [u32; 8],
    pub ispr: [u32; 8],
    pub iabr: [u32; 8],
    pub ipr: [u8; 240],
    pub shpr: [u8; 12],
    pub shcsr: u32,
    pub ccr: u32,
    pub cpacr: u32,
    pub aircr: u32,
    pub scr: u32,
    // SysTick
    pub systick_ctrl: u32,
    pub systick_load: u32,
    pub systick_val: u32,
    pub systick_calib: u32,
    // Unified bitmasks & active tracking
    pub pending_vectors: u128,
    pub active_vectors: u128,
    pub active_stack: Vec<u32>,
    pub is_nested: bool,
    pub isr_cache: [u32; 128],
}

impl Default for NvicController {
    fn default() -> Self {
        Self {
            vtor: FLASH_BASE as u32,
            iser: [0; 8],
            ispr: [0; 8],
            iabr: [0; 8],
            ipr: [0; 240],
            shpr: [0; 12],
            shcsr: 0,
            ccr: 0x0000_0200,   // STKALIGN = 1
            cpacr: 0x00F0_0000, // Full CP10 and CP11 access for FPU
            aircr: 0xFA05_0000,
            scr: 0,
            systick_ctrl: 0,
            systick_load: 0,
            systick_val: 0,
            systick_calib: 0x0000_2328,
            pending_vectors: 0,
            active_vectors: 0,
            active_stack: Vec::new(),
            is_nested: false,
            isr_cache: [0; 128],
        }
    }
}

impl NvicController {
    pub fn new(vtor: u32) -> Self {
        let mut s = Self::default();
        s.vtor = vtor;
        s
    }

    #[inline(always)]
    pub fn enable_vector(&mut self, vector: u32) {
        if vector >= 16 {
            let irq = (vector - 16) as usize;
            if irq < 240 {
                self.iser[irq / 32] |= 1 << (irq % 32);
            }
        }
    }

    #[inline(always)]
    pub fn disable_vector(&mut self, vector: u32) {
        if vector >= 16 {
            let irq = (vector - 16) as usize;
            if irq < 240 {
                self.iser[irq / 32] &= !(1 << (irq % 32));
            }
        }
    }

    #[inline(always)]
    pub fn set_pending(&mut self, vector: u32) {
        if vector < 128 {
            self.pending_vectors |= 1u128 << vector;
        }
        if vector >= 16 {
            let irq = (vector - 16) as usize;
            if irq < 240 {
                self.ispr[irq / 32] |= 1 << (irq % 32);
            }
        }
    }

    #[inline(always)]
    pub fn clear_pending(&mut self, vector: u32) {
        if vector < 128 {
            self.pending_vectors &= !(1u128 << vector);
        }
        if vector >= 16 {
            let irq = (vector - 16) as usize;
            if irq < 240 {
                self.ispr[irq / 32] &= !(1 << (irq % 32));
            }
        }
    }

    #[inline(always)]
    pub fn is_pending(&self, vector: u32) -> bool {
        if vector < 128 {
            (self.pending_vectors & (1u128 << vector)) != 0
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn is_active(&self, vector: u32) -> bool {
        if vector < 128 {
            (self.active_vectors & (1u128 << vector)) != 0
        } else {
            false
        }
    }

    pub fn is_enabled(&self, vector: u32) -> bool {
        match vector {
            1..=3 => true, // Reset, NMI, HardFault
            4..=6 => {
                let shift = match vector {
                    4 => 16,
                    5 => 17,
                    6 => 18,
                    _ => 0,
                };
                (self.shcsr & (1 << shift)) != 0
            }
            11 | 14 => true, // SVCall, PendSV
            15 => (self.systick_ctrl & 0x03) == 0x03, // SysTick ENABLE + TICKINT
            v if v >= 16 => {
                let irq = (v - 16) as usize;
                if irq < 240 {
                    (self.iser[irq / 32] & (1 << (irq % 32))) != 0
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn get_priority(&self, vector: u32) -> u32 {
        match vector {
            1 => 0, // Reset (highest)
            2 => 1, // NMI
            3 => 2, // HardFault
            4 => self.shpr[0] as u32,  // MemManage
            5 => self.shpr[1] as u32,  // BusFault
            6 => self.shpr[2] as u32,  // UsageFault
            11 => self.shpr[7] as u32, // SVCall
            12 => self.shpr[8] as u32, // DebugMonitor
            14 => self.shpr[10] as u32, // PendSV
            15 => self.shpr[11] as u32, // SysTick
            v if v >= 16 => {
                let irq = (v - 16) as usize;
                if irq < 240 {
                    self.ipr[irq] as u32
                } else {
                    255
                }
            }
            _ => 255,
        }
    }

    pub fn get_highest_pending_vector(&self) -> Option<u32> {
        let mut best_vector = None;
        let mut best_pri = 256;

        for v in 1..=127 {
            if (self.pending_vectors & (1u128 << v)) != 0 && self.is_enabled(v) {
                let pri = self.get_priority(v);
                if pri < best_pri {
                    best_pri = pri;
                    best_vector = Some(v);
                }
            }
        }
        best_vector
    }

    pub fn get_next_preempting_vector<D>(&self, uc: &Unicorn<D>) -> Option<u32> {
        if self.pending_vectors == 0 {
            return None;
        }
        let primask = uc.reg_read(RegisterARM::PRIMASK).unwrap_or(0);
        let basepri = uc.reg_read(RegisterARM::BASEPRI).unwrap_or(0) as u32;

        let current_pri = if let Some(&active_vec) = self.active_stack.last() {
            self.get_priority(active_vec)
        } else {
            256
        };

        let mut best_vector = None;
        let mut best_pri = current_pri;

        for v in 1..=127 {
            if (self.pending_vectors & (1u128 << v)) != 0 {
                if !self.is_enabled(v) {
                    continue;
                }
                let pri = self.get_priority(v);
                if v > 3 {
                    if (primask & 1) != 0 {
                        continue;
                    }
                    if basepri != 0 && pri >= basepri {
                        continue;
                    }
                }
                // Preemption requires strictly higher urgency (lower numerical priority)
                if pri < best_pri {
                    best_pri = pri;
                    best_vector = Some(v);
                }
            }
        }

        best_vector
    }

    pub fn read_icsr(&self) -> u32 {
        let vect_active = self.active_stack.last().copied().unwrap_or(0) & 0x1FF;
        let rettobase = if self.active_stack.len() <= 1 { 1 << 11 } else { 0 };
        let vect_pending = (self.get_highest_pending_vector().unwrap_or(0) & 0x1FF) << 12;
        let isrpending = if (self.pending_vectors >> 16) != 0 { 1 << 22 } else { 0 };
        let pendstset = if self.is_pending(VECTOR_SYSTICK) { 1 << 26 } else { 0 };
        let pendsvset = if self.is_pending(VECTOR_PENDSV) { 1 << 28 } else { 0 };
        let nmipendset = if self.is_pending(2) { 1 << 31 } else { 0 };

        vect_active | rettobase | vect_pending | isrpending | pendstset | pendsvset | nmipendset
    }

    pub fn write_icsr(&mut self, val: u32) {
        if (val & (1 << 25)) != 0 {
            self.clear_pending(VECTOR_SYSTICK);
        }
        if (val & (1 << 26)) != 0 {
            self.set_pending(VECTOR_SYSTICK);
        }
        if (val & (1 << 27)) != 0 {
            self.clear_pending(VECTOR_PENDSV);
        }
        if (val & (1 << 28)) != 0 {
            self.set_pending(VECTOR_PENDSV);
        }
        if (val & (1 << 31)) != 0 {
            self.set_pending(2); // NMI
        }
    }

    pub fn read_ppb(&mut self, addr: u32, size: usize) -> u32 {
        match addr {
            // SysTick: 0xE000_E010..0xE000_E01C
            0xE000_E010 => {
                let v = self.systick_ctrl;
                self.systick_ctrl &= !(1 << 16); // Reading clears COUNTFLAG
                v
            }
            0xE000_E014 => self.systick_load,
            0xE000_E018 => self.systick_val,
            0xE000_E01C => self.systick_calib,

            // NVIC ISER: 0xE000_E100..0xE000_E11F
            0xE000_E100..=0xE000_E11F => {
                let idx = ((addr - 0xE000_E100) / 4) as usize;
                if idx < 8 { self.iser[idx] } else { 0 }
            }

            // NVIC ICER: 0xE000_E180..0xE000_E19F (returns current ISER state)
            0xE000_E180..=0xE000_E19F => {
                let idx = ((addr - 0xE000_E180) / 4) as usize;
                if idx < 8 { self.iser[idx] } else { 0 }
            }

            // NVIC ISPR: 0xE000_E200..0xE000_E21F
            0xE000_E200..=0xE000_E21F => {
                let idx = ((addr - 0xE000_E200) / 4) as usize;
                if idx < 8 { self.ispr[idx] } else { 0 }
            }

            // NVIC ICPR: 0xE000_E280..0xE000_E29F (returns current ISPR state)
            0xE000_E280..=0xE000_E29F => {
                let idx = ((addr - 0xE000_E280) / 4) as usize;
                if idx < 8 { self.ispr[idx] } else { 0 }
            }

            // NVIC IABR: 0xE000_E300..0xE000_E31F
            0xE000_E300..=0xE000_E31F => {
                let idx = ((addr - 0xE000_E300) / 4) as usize;
                if idx < 8 { self.iabr[idx] } else { 0 }
            }

            // NVIC IPR: 0xE000_E400..0xE000_E4EF (240 bytes)
            0xE000_E400..=0xE000_E4EF => {
                let off = (addr - 0xE000_E400) as usize;
                if size == 1 {
                    if off < 240 { self.ipr[off] as u32 } else { 0 }
                } else if size == 2 {
                    if off + 1 < 240 {
                        u16::from_le_bytes([self.ipr[off], self.ipr[off + 1]]) as u32
                    } else {
                        0
                    }
                } else {
                    if off + 3 < 240 {
                        u32::from_le_bytes(self.ipr[off..off + 4].try_into().unwrap())
                    } else {
                        0
                    }
                }
            }

            // SCB:
            0xE000_ED00 => 0x410F_C241, // CPUID (Cortex-M4 r0p1)
            0xE000_ED04 => self.read_icsr(),
            0xE000_ED08 => self.vtor,
            0xE000_ED0C => 0xFA05_0000 | (self.aircr & 0x0700),
            0xE000_ED10 => self.scr,
            0xE000_ED14 => self.ccr,
            0xE000_ED18..=0xE000_ED23 => {
                let off = (addr - 0xE000_ED18) as usize;
                if size == 1 {
                    if off < 12 { self.shpr[off] as u32 } else { 0 }
                } else if size == 2 {
                    if off + 1 < 12 {
                        u16::from_le_bytes([self.shpr[off], self.shpr[off + 1]]) as u32
                    } else {
                        0
                    }
                } else {
                    if off + 3 < 12 {
                        u32::from_le_bytes(self.shpr[off..off + 4].try_into().unwrap())
                    } else {
                        0
                    }
                }
            }
            0xE000_ED24 => self.shcsr,
            0xE000_ED88 => self.cpacr,

            _ => 0,
        }
    }

    pub fn write_ppb(&mut self, addr: u32, size: usize, val: u32) -> PpbAction {
        match addr {
            // SysTick
            0xE000_E010 => {
                self.systick_ctrl = (self.systick_ctrl & !0x07) | (val & 0x07);
            }
            0xE000_E014 => {
                self.systick_load = val & 0x00FF_FFFF;
            }
            0xE000_E018 => {
                self.systick_val = 0;
                self.systick_ctrl &= !(1 << 16);
            }
            0xE000_E01C => {} // CALIB read-only

            // NVIC ISER
            0xE000_E100..=0xE000_E11F => {
                let idx = ((addr - 0xE000_E100) / 4) as usize;
                if idx < 8 {
                    let shift = (addr & 3) * 8;
                    let mask = match size { 1 => 0xFF, 2 => 0xFFFF, _ => 0xFFFF_FFFF };
                    self.iser[idx] |= (val & mask) << shift;
                }
            }

            // NVIC ICER
            0xE000_E180..=0xE000_E19F => {
                let idx = ((addr - 0xE000_E180) / 4) as usize;
                if idx < 8 {
                    let shift = (addr & 3) * 8;
                    let mask = match size { 1 => 0xFF, 2 => 0xFFFF, _ => 0xFFFF_FFFF };
                    self.iser[idx] &= !((val & mask) << shift);
                }
            }

            // NVIC ISPR
            0xE000_E200..=0xE000_E21F => {
                let idx = ((addr - 0xE000_E200) / 4) as usize;
                if idx < 8 {
                    let shift = (addr & 3) * 8;
                    let mask = match size { 1 => 0xFF, 2 => 0xFFFF, _ => 0xFFFF_FFFF };
                    let word_val = (val & mask) << shift;
                    self.ispr[idx] |= word_val;
                    for b in 0..32 {
                        if (word_val & (1 << b)) != 0 {
                            let vec = 16 + (idx * 32 + b) as u32;
                            if vec < 128 {
                                self.pending_vectors |= 1u128 << vec;
                            }
                        }
                    }
                }
            }

            // NVIC ICPR
            0xE000_E280..=0xE000_E29F => {
                let idx = ((addr - 0xE000_E280) / 4) as usize;
                if idx < 8 {
                    let shift = (addr & 3) * 8;
                    let mask = match size { 1 => 0xFF, 2 => 0xFFFF, _ => 0xFFFF_FFFF };
                    let word_val = (val & mask) << shift;
                    self.ispr[idx] &= !word_val;
                    for b in 0..32 {
                        if (word_val & (1 << b)) != 0 {
                            let vec = 16 + (idx * 32 + b) as u32;
                            if vec < 128 {
                                self.pending_vectors &= !(1u128 << vec);
                            }
                        }
                    }
                }
            }

            // NVIC IABR
            0xE000_E300..=0xE000_E31F => {} // Read-only

            // NVIC IPR (240 bytes)
            0xE000_E400..=0xE000_E4EF => {
                let off = (addr - 0xE000_E400) as usize;
                for i in 0..size.min(4) {
                    if off + i < 240 {
                        self.ipr[off + i] = ((val >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }

            // SCB ICSR
            0xE000_ED04 => {
                self.write_icsr(val);
            }

            // SCB VTOR
            0xE000_ED08 => {
                self.vtor = val & 0xFFFF_FE00;
            }

            // SCB AIRCR
            0xE000_ED0C => {
                if (val & 0xFFFF_0000) == 0x05FA_0000 {
                    self.aircr = val & 0x0700;
                    if (val & 0x04) != 0 {
                        return PpbAction::SysResetReq;
                    }
                }
            }

            // SCB SCR
            0xE000_ED10 => {
                self.scr = val;
            }

            // SCB CCR
            0xE000_ED14 => {
                self.ccr = val | (1 << 9); // Preserve STKALIGN
            }

            // SCB SHPR1..3
            0xE000_ED18..=0xE000_ED23 => {
                let off = (addr - 0xE000_ED18) as usize;
                for i in 0..size.min(4) {
                    if off + i < 12 {
                        self.shpr[off + i] = ((val >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }

            // SCB SHCSR
            0xE000_ED24 => {
                self.shcsr = val;
            }

            // SCB CPACR
            0xE000_ED88 => {
                self.cpacr = val;
            }

            _ => {}
        }
        PpbAction::None
    }

    /// Dispatches an interrupt vector using hardware-accurate ARM Cortex-M stack framing.
    pub fn dispatch_vector<D>(
        &mut self,
        uc: &mut Unicorn<D>,
        vector_num: u32,
        max_instructions: usize,
    ) -> Result<bool, unicorn_engine::unicorn_const::uc_error> {
        let mut vtor_bytes = [0u8; 4];
        let vtor = if uc.mem_read(VTOR_ADDR as u64, &mut vtor_bytes).is_ok() {
            let v = u32::from_le_bytes(vtor_bytes);
            if v != 0 {
                v
            } else {
                self.vtor
            }
        } else {
            self.vtor
        };
        let vector_addr_loc = vtor + 4 * vector_num;
        let mut addr_bytes = [0u8; 4];
        uc.mem_read(vector_addr_loc as u64, &mut addr_bytes)?;
        let isr_addr = u32::from_le_bytes(addr_bytes) & !1;

        let is_valid_addr = (isr_addr >= FLASH_BASE as u32
            && isr_addr < (FLASH_BASE + FLASH_SIZE as u64) as u32)
            || (isr_addr >= SRAM_BASE as u32
                && isr_addr < (SRAM_BASE + SRAM_SIZE as u64) as u32)
            || (isr_addr >= BOOT_ALIAS_BASE as u32
                && isr_addr < (BOOT_ALIAS_BASE + FLASH_SIZE as u64) as u32);

        if isr_addr == 0 || !is_valid_addr {
            return Ok(false);
        }

        let primask = uc.reg_read(RegisterARM::PRIMASK).unwrap_or(0);
        if (primask & 1) != 0 {
            return Ok(false);
        }

        let basepri = uc.reg_read(RegisterARM::BASEPRI).unwrap_or(0) as u32;
        if basepri != 0 {
            let prio_addr = match vector_num {
                1..=3 => None,
                11 => Some(0xE000_ED1F),
                14 => Some(0xE000_ED22),
                15 => Some(0xE000_ED23),
                v if v >= 16 => Some(0xE000_E400 + (v - 16)),
                _ => None,
            };
            if let Some(addr) = prio_addr {
                let mut b = [0u8; 1];
                if uc.mem_read(addr as u64, &mut b).is_ok() {
                    let vec_prio = b[0] as u32;
                    if vec_prio >= basepri {
                        return Ok(false);
                    }
                }
            }
        }

        if vector_num == VECTOR_SYSTICK {
            let mut ctrl_bytes = [0u8; 4];
            if uc.mem_read(0xE000_E010, &mut ctrl_bytes).is_ok() {
                let ctrl = u32::from_le_bytes(ctrl_bytes);
                if (ctrl & 0x03) != 0x03 {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }

        const S_REGS: [RegisterARM; 16] = [
            RegisterARM::S0,
            RegisterARM::S1,
            RegisterARM::S2,
            RegisterARM::S3,
            RegisterARM::S4,
            RegisterARM::S5,
            RegisterARM::S6,
            RegisterARM::S7,
            RegisterARM::S8,
            RegisterARM::S9,
            RegisterARM::S10,
            RegisterARM::S11,
            RegisterARM::S12,
            RegisterARM::S13,
            RegisterARM::S14,
            RegisterARM::S15,
        ];

        let control = uc.reg_read(RegisterARM::CONTROL).unwrap_or(0) as u32;
        let was_using_psp = (control & 2) != 0 && self.active_stack.is_empty();
        let is_fpu_active = (control & 4) != 0;
        let frame_size: u32 = if is_fpu_active { 104 } else { 32 };

        let sp = uc.reg_read(RegisterARM::SP)? as u32;

        let r0 = uc.reg_read(RegisterARM::R0)? as u32;
        let r1 = uc.reg_read(RegisterARM::R1)? as u32;
        let r2 = uc.reg_read(RegisterARM::R2)? as u32;
        let r3 = uc.reg_read(RegisterARM::R3)? as u32;
        let r12 = uc.reg_read(RegisterARM::R12)? as u32;
        let lr = uc.reg_read(RegisterARM::LR)? as u32;
        let pc = uc.reg_read(RegisterARM::PC)? as u32;
        let cpsr = uc.reg_read(RegisterARM::CPSR)? as u32;

        let new_sp = (sp.wrapping_sub(frame_size)) & !7;
        let stacked_xpsr = (cpsr & !(1 << 9)) | ((sp & 4) << 7);

        let mut frame = vec![0u8; frame_size as usize];
        frame[0..4].copy_from_slice(&r0.to_le_bytes());
        frame[4..8].copy_from_slice(&r1.to_le_bytes());
        frame[8..12].copy_from_slice(&r2.to_le_bytes());
        frame[12..16].copy_from_slice(&r3.to_le_bytes());
        frame[16..20].copy_from_slice(&r12.to_le_bytes());
        frame[20..24].copy_from_slice(&lr.to_le_bytes());
        frame[24..28].copy_from_slice(&pc.to_le_bytes());
        frame[28..32].copy_from_slice(&stacked_xpsr.to_le_bytes());

        if is_fpu_active {
            for (i, &s_reg) in S_REGS.iter().enumerate() {
                let s_val = uc.reg_read(s_reg).unwrap_or(0) as u32;
                let off = 32 + i * 4;
                frame[off..off + 4].copy_from_slice(&s_val.to_le_bytes());
            }
            let fpscr = uc.reg_read(RegisterARM::FPSCR).unwrap_or(0) as u32;
            frame[96..100].copy_from_slice(&fpscr.to_le_bytes());
            let _ = uc.reg_write(RegisterARM::CONTROL, (control & !4) as u64);
        }

        uc.mem_write(new_sp as u64, &frame)?;
        if was_using_psp {
            uc.reg_write(RegisterARM::PSP, new_sp as u64)?;
            let current_ctrl = uc.reg_read(RegisterARM::CONTROL).unwrap_or(0) as u32;
            let _ = uc.reg_write(RegisterARM::CONTROL, (current_ctrl & !2) as u64);
            let msp = uc.reg_read(RegisterARM::MSP).unwrap_or(0);
            uc.reg_write(RegisterARM::SP, msp)?;
        } else {
            uc.reg_write(RegisterARM::MSP, new_sp as u64)?;
            uc.reg_write(RegisterARM::SP, new_sp as u64)?;
        }

        let isr_cpsr = (cpsr & !0x0600_FC00) | (1 << 24);
        uc.reg_write(RegisterARM::CPSR, isr_cpsr as u64)?;

        let is_nested = !self.active_stack.is_empty();
        let exc_return = if is_nested {
            if is_fpu_active { 0xFFFF_FFE1 } else { EXC_RETURN_HANDLER_MSP }
        } else if was_using_psp {
            if is_fpu_active { 0xFFFF_FFED } else { EXC_RETURN_THREAD_PSP }
        } else {
            if is_fpu_active { 0xFFFF_FFE9 } else { EXC_RETURN_THREAD_MSP }
        };
        uc.reg_write(RegisterARM::LR, exc_return as u64)?;

        self.active_stack.push(vector_num);
        if vector_num < 128 {
            self.active_vectors |= 1u128 << vector_num;
        }
        self.is_nested = self.active_stack.len() > 1;

        let until_pc = (exc_return as u64) & !1;
        let mut total_instructions = 0;
        let hard_cap = 500_000;
        let chunk_size = max_instructions.max(10_000);
        let mut start_pc = (isr_addr | 1) as u64;

        loop {
            let res = uc.emu_start(start_pc, until_pc, 0, chunk_size);
            total_instructions += chunk_size;
            let ret_pc = uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
            if (ret_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                break;
            }
            if res.is_err() || total_instructions >= hard_cap {
                eprintln!(
                    "[NVIC ERROR] Vector {} at 0x{:08X} did not return to EXC_RETURN within {} instructions (PC=0x{:08X}, res={:?})",
                    vector_num, isr_addr, total_instructions, ret_pc, res
                );
                self.active_stack.pop();
                if vector_num < 128 {
                    self.active_vectors &= !(1u128 << vector_num);
                }
                self.is_nested = self.active_stack.len() > 1;
                return Ok(false);
            }
            start_pc = (ret_pc | 1) as u64;
        }

        let ret_pc = uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;

        if (ret_pc & 0xFFFF_0000) == 0xFFFF_0000 {
            let use_psp = (ret_pc & 4) != 0;
            let is_extended = (ret_pc & (1 << 4)) == 0;
            let curr_sp = if use_psp {
                uc.reg_read(RegisterARM::PSP).unwrap_or(0) as u32
            } else {
                uc.reg_read(RegisterARM::SP).unwrap_or(0) as u32
            };

            let frame_size = if is_extended { 104 } else { 32 };
            let mut popped = vec![0u8; frame_size];
            if uc.mem_read(curr_sp as u64, &mut popped).is_ok() {
                let r0 = u32::from_le_bytes(popped[0..4].try_into().unwrap());
                let r1 = u32::from_le_bytes(popped[4..8].try_into().unwrap());
                let r2 = u32::from_le_bytes(popped[8..12].try_into().unwrap());
                let r3 = u32::from_le_bytes(popped[12..16].try_into().unwrap());
                let r12 = u32::from_le_bytes(popped[16..20].try_into().unwrap());
                let lr = u32::from_le_bytes(popped[20..24].try_into().unwrap());
                let pc = u32::from_le_bytes(popped[24..28].try_into().unwrap());
                let xpsr = u32::from_le_bytes(popped[28..32].try_into().unwrap());

                uc.reg_write(RegisterARM::R0, r0 as u64)?;
                uc.reg_write(RegisterARM::R1, r1 as u64)?;
                uc.reg_write(RegisterARM::R2, r2 as u64)?;
                uc.reg_write(RegisterARM::R3, r3 as u64)?;
                uc.reg_write(RegisterARM::R12, r12 as u64)?;
                uc.reg_write(RegisterARM::LR, lr as u64)?;
                uc.reg_write(RegisterARM::PC, (pc | 1) as u64)?;
                uc.reg_write(RegisterARM::CPSR, (xpsr | 0x20) as u64)?;

                if is_extended {
                    for (i, &s_reg) in S_REGS.iter().enumerate() {
                        let off = 32 + i * 4;
                        let s_val = u32::from_le_bytes(popped[off..off + 4].try_into().unwrap());
                        let _ = uc.reg_write(s_reg, s_val as u64);
                    }
                    let fpscr = u32::from_le_bytes(popped[96..100].try_into().unwrap());
                    let _ = uc.reg_write(RegisterARM::FPSCR, fpscr as u64);
                }

                let align_pad = if (xpsr & (1 << 9)) != 0 { 4 } else { 0 };
                let final_sp = curr_sp + (frame_size as u32) + align_pad;
                if use_psp {
                    uc.reg_write(RegisterARM::PSP, final_sp as u64)?;
                    uc.reg_write(RegisterARM::SP, final_sp as u64)?;
                } else {
                    uc.reg_write(RegisterARM::MSP, final_sp as u64)?;
                    uc.reg_write(RegisterARM::SP, final_sp as u64)?;
                }

                self.active_stack.pop();
                if vector_num < 128 {
                    self.active_vectors &= !(1u128 << vector_num);
                }
                self.is_nested = !self.active_stack.is_empty();

                if (ret_pc & 8) != 0 {
                    let mut ctrl = uc.reg_read(RegisterARM::CONTROL).unwrap_or(0) as u32;
                    if use_psp {
                        ctrl |= 2;
                    } else {
                        ctrl &= !2;
                    }
                    if is_extended {
                        ctrl |= 4;
                    } else {
                        ctrl &= !4;
                    }
                    let _ = uc.reg_write(RegisterARM::CONTROL, ctrl as u64);
                }
            }
        } else {
            self.active_stack.pop();
            if vector_num < 128 {
                self.active_vectors &= !(1u128 << vector_num);
            }
            self.is_nested = !self.active_stack.is_empty();
            return Ok(false);
        }

        if vector_num != VECTOR_PENDSV && self.active_stack.is_empty() {
            let mut icsr_bytes = [0u8; 4];
            if uc.mem_read(SCB_ICSR as u64, &mut icsr_bytes).is_ok() {
                let icsr = u32::from_le_bytes(icsr_bytes);
                if (icsr & (1 << 28)) != 0 {
                    let cleared = icsr & !(1 << 28);
                    let _ = uc.mem_write(SCB_ICSR as u64, &cleared.to_le_bytes());
                    let _ = self.dispatch_vector(uc, VECTOR_PENDSV, max_instructions);
                }
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicorn_engine::unicorn_const::{Arch, Mode, Prot};

    #[test]
    fn test_nvic_vector_bounds_acceptance() {
        let mut uc = Unicorn::new(Arch::ARM, Mode::THUMB | Mode::MCLASS).unwrap();
        uc.mem_map(FLASH_BASE, 0x1000, Prot::ALL).unwrap();
        uc.mem_map(SRAM_BASE, 0x1000, Prot::ALL).unwrap();
        uc.mem_map(BOOT_ALIAS_BASE, 0x1000, Prot::ALL).unwrap();
        uc.mem_map(0xE000_E000, 0x1000, Prot::ALL).unwrap();

        let mut nvic = NvicController::new(FLASH_BASE as u32);

        let sram_isr = (SRAM_BASE as u32) + 0x101;
        uc.mem_write((FLASH_BASE as u64) + 15 * 4, &sram_isr.to_le_bytes()).unwrap();
        uc.mem_write(0xE000_E010, &3u32.to_le_bytes()).unwrap();
        uc.mem_write(sram_isr as u64 & !1, &[0x70, 0x47]).unwrap();

        uc.reg_write(RegisterARM::SP, 0x2000_0800).unwrap();
        uc.reg_write(RegisterARM::PC, (FLASH_BASE | 1) as u64).unwrap();

        uc.mem_map(EXC_RETURN_PAGE_BASE, 0x1000, Prot::ALL).unwrap();
        let trap_bytes = [0xFE, 0xE7].repeat(512);
        uc.mem_write(EXC_RETURN_PAGE_BASE, &trap_bytes).unwrap();

        let res = nvic.dispatch_vector(&mut uc, VECTOR_SYSTICK, 100);
        assert!(res.is_ok());
        assert!(res.unwrap());
    }

    #[test]
    fn test_nvic_8byte_stack_alignment() {
        let mut uc = Unicorn::new(Arch::ARM, Mode::THUMB | Mode::MCLASS).unwrap();
        uc.mem_map(FLASH_BASE, 0x1000, Prot::ALL).unwrap();
        uc.mem_map(SRAM_BASE, 0x2000, Prot::ALL).unwrap();
        uc.mem_map(0xE000_E000, 0x1000, Prot::ALL).unwrap();
        uc.mem_map(EXC_RETURN_PAGE_BASE, 0x1000, Prot::ALL).unwrap();
        let trap_bytes = [0xFE, 0xE7].repeat(512);
        uc.mem_write(EXC_RETURN_PAGE_BASE, &trap_bytes).unwrap();

        let mut nvic = NvicController::new(FLASH_BASE as u32);

        let isr_addr = (FLASH_BASE as u32) + 0x201;
        uc.mem_write((FLASH_BASE as u64) + 15 * 4, &isr_addr.to_le_bytes()).unwrap();
        uc.mem_write(0xE000_E010, &3u32.to_le_bytes()).unwrap();
        uc.mem_write(isr_addr as u64 & !1, &[0x70, 0x47]).unwrap(); // BX LR

        // Case 1: SP is unaligned (4-byte aligned: 0x2000_0804) -> requires 4-byte pad and bit 9 set in xPSR
        let unaligned_sp = 0x2000_0804u32;
        uc.reg_write(RegisterARM::SP, unaligned_sp as u64).unwrap();
        uc.reg_write(RegisterARM::MSP, unaligned_sp as u64).unwrap();
        uc.reg_write(RegisterARM::PC, (FLASH_BASE | 1) as u64).unwrap();

        let res = nvic.dispatch_vector(&mut uc, VECTOR_SYSTICK, 100);
        assert!(res.is_ok());
        assert!(res.unwrap());

        let final_sp = uc.reg_read(RegisterARM::SP).unwrap() as u32;
        assert_eq!(final_sp, unaligned_sp);

        // Case 2: SP is already 8-byte aligned (0x2000_0800) -> no pad (bit 9 cleared in xPSR)
        let aligned_sp = 0x2000_0800u32;
        uc.reg_write(RegisterARM::SP, aligned_sp as u64).unwrap();
        uc.reg_write(RegisterARM::MSP, aligned_sp as u64).unwrap();
        uc.reg_write(RegisterARM::PC, (FLASH_BASE | 1) as u64).unwrap();

        let res2 = nvic.dispatch_vector(&mut uc, VECTOR_SYSTICK, 100);
        assert!(res2.is_ok());
        assert!(res2.unwrap());

        let final_sp2 = uc.reg_read(RegisterARM::SP).unwrap() as u32;
        assert_eq!(final_sp2, aligned_sp, "8-byte aligned SP must not leak padding bytes on exception return");
    }

    #[test]
    fn test_nvic_pendsv_gated_on_thread_mode() {
        let mut uc = Unicorn::new(Arch::ARM, Mode::THUMB | Mode::MCLASS).unwrap();
        uc.mem_map(FLASH_BASE, 0x1000, Prot::ALL).unwrap();
        uc.mem_map(SRAM_BASE, 0x2000, Prot::ALL).unwrap();
        uc.mem_map(0xE000_E000, 0x1000, Prot::ALL).unwrap();
        uc.mem_map(EXC_RETURN_PAGE_BASE, 0x1000, Prot::ALL).unwrap();
        let trap_bytes = [0xFE, 0xE7].repeat(512);
        uc.mem_write(EXC_RETURN_PAGE_BASE, &trap_bytes).unwrap();

        let mut nvic = NvicController::new(FLASH_BASE as u32);

        let systick_addr = (FLASH_BASE as u32) + 0x201;
        let pendsv_addr = (FLASH_BASE as u32) + 0x301;
        uc.mem_write((FLASH_BASE as u64) + 15 * 4, &systick_addr.to_le_bytes()).unwrap();
        uc.mem_write((FLASH_BASE as u64) + 14 * 4, &pendsv_addr.to_le_bytes()).unwrap();
        uc.mem_write(0xE000_E010, &3u32.to_le_bytes()).unwrap();
        uc.mem_write(systick_addr as u64 & !1, &[0x70, 0x47]).unwrap();
        uc.mem_write(pendsv_addr as u64 & !1, &[0x70, 0x47]).unwrap();

        uc.mem_write(SCB_ICSR as u64, &(1u32 << 28).to_le_bytes()).unwrap();

        uc.reg_write(RegisterARM::SP, 0x2000_0800).unwrap();
        uc.reg_write(RegisterARM::MSP, 0x2000_0800).unwrap();
        uc.reg_write(RegisterARM::PC, (FLASH_BASE | 1) as u64).unwrap();

        let res = nvic.dispatch_vector(&mut uc, VECTOR_SYSTICK, 100);
        assert!(res.is_ok());
        assert!(res.unwrap());

        let mut icsr_bytes = [0u8; 4];
        uc.mem_read(SCB_ICSR as u64, &mut icsr_bytes).unwrap();
        let icsr = u32::from_le_bytes(icsr_bytes);
        assert_eq!(icsr & (1 << 28), 0);
    }

    #[test]
    fn test_nvic_ppb_iser_icer_ispr_icpr() {
        let mut nvic = NvicController::new(FLASH_BASE as u32);

        // ISER0 / ICER0: Enable IRQ 5 (vector 21)
        let _ = nvic.write_ppb(0xE000_E100, 4, 1 << 5);
        assert_eq!(nvic.read_ppb(0xE000_E100, 4) & (1 << 5), 1 << 5);
        assert_eq!(nvic.read_ppb(0xE000_E180, 4) & (1 << 5), 1 << 5);
        assert!(nvic.is_enabled(21));

        // Disable IRQ 5 via ICER0
        let _ = nvic.write_ppb(0xE000_E180, 4, 1 << 5);
        assert_eq!(nvic.read_ppb(0xE000_E100, 4) & (1 << 5), 0);
        assert!(!nvic.is_enabled(21));

        // ISPR1 / ICPR1: Pend IRQ 42 (IRQ 42 is bit 10 in word 1, vector = 16 + 42 = 58)
        let _ = nvic.write_ppb(0xE000_E204, 4, 1 << 10);
        assert_eq!(nvic.read_ppb(0xE000_E204, 4) & (1 << 10), 1 << 10);
        assert!(nvic.is_pending(58));

        // Clear pending via ICPR1
        let _ = nvic.write_ppb(0xE000_E284, 4, 1 << 10);
        assert_eq!(nvic.read_ppb(0xE000_E204, 4) & (1 << 10), 0);
        assert!(!nvic.is_pending(58));
    }

    #[test]
    fn test_nvic_ppb_icsr_pendsv_and_vectactive() {
        let mut nvic = NvicController::new(FLASH_BASE as u32);

        // Initially in Thread Mode, no pending exceptions
        let icsr0 = nvic.read_icsr();
        assert_eq!(icsr0 & 0x1FF, 0); // VECTACTIVE == 0
        assert_ne!(icsr0 & (1 << 11), 0); // RETTOBASE == 1

        // Pend PendSV via ICSR bit 28 (PENDSVSET)
        let _ = nvic.write_ppb(0xE000_ED04, 4, 1 << 28);
        assert!(nvic.is_pending(VECTOR_PENDSV));
        let icsr1 = nvic.read_icsr();
        assert_ne!(icsr1 & (1 << 28), 0); // PENDSVSET is 1
        assert_eq!((icsr1 >> 12) & 0x1FF, VECTOR_PENDSV); // VECTPENDING == 14

        // Clear PendSV via ICSR bit 27 (PENDSVCLR)
        let _ = nvic.write_ppb(0xE000_ED04, 4, 1 << 27);
        assert!(!nvic.is_pending(VECTOR_PENDSV));
        let icsr2 = nvic.read_icsr();
        assert_eq!(icsr2 & (1 << 28), 0);

        // Pend SysTick via bit 26 (PENDSTSET)
        let _ = nvic.write_ppb(0xE000_ED04, 4, 1 << 26);
        assert!(nvic.is_pending(VECTOR_SYSTICK));
        let _ = nvic.write_ppb(0xE000_ED04, 4, 1 << 25);
        assert!(!nvic.is_pending(VECTOR_SYSTICK));
    }

    #[test]
    fn test_nvic_ipr_and_preemption_priority() {
        let uc = Unicorn::new(Arch::ARM, Mode::THUMB | Mode::MCLASS).unwrap();
        let mut nvic = NvicController::new(FLASH_BASE as u32);

        // Vector 53 (USART1, IRQ 37) and Vector 54 (USART2, IRQ 38)
        // Set priority for IRQ 37 to 0x40 (lower priority / higher numerical value)
        let _ = nvic.write_ppb(0xE000_E400 + 37, 1, 0x40);
        // Set priority for IRQ 38 to 0x20 (higher priority / lower numerical value)
        let _ = nvic.write_ppb(0xE000_E400 + 38, 1, 0x20);

        assert_eq!(nvic.get_priority(53), 0x40);
        assert_eq!(nvic.get_priority(54), 0x20);

        // Enable both in ISER
        nvic.enable_vector(53);
        nvic.enable_vector(54);

        // Pend both
        nvic.set_pending(53);
        nvic.set_pending(54);

        // Vector 54 should preempt because it has higher priority (0x20 < 0x40)
        let preempt = nvic.get_next_preempting_vector(&uc);
        assert_eq!(preempt, Some(54));

        // When vector 54 is active, vector 53 cannot preempt
        nvic.active_stack.push(54);
        let preempt_nested = nvic.get_next_preempting_vector(&uc);
        assert_eq!(preempt_nested, None);

        // Add IRQ 10 (vector 26) with priority 0x10 (higher than 0x20)
        let _ = nvic.write_ppb(0xE000_E400 + 10, 1, 0x10);
        nvic.enable_vector(26);
        nvic.set_pending(26);

        // Vector 26 can preempt vector 54
        let preempt_higher = nvic.get_next_preempting_vector(&uc);
        assert_eq!(preempt_higher, Some(26));
    }
}
