use std::collections::HashSet;
use unicorn_engine::{RegisterARM, Unicorn};
use crate::core::{EmulatorContext, VibeFreakEmulator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwdSymbolType {
    Pointer,
    Float,
    Bool,
    Uint32,
    Uint8,
}

#[derive(Debug, Clone)]
pub struct SwdSymbol {
    pub name: &'static str,
    pub addr: u32,
    pub sym_type: SwdSymbolType,
    pub description: &'static str,
}

/// Known firmware exported symbols (from nanowave_main_stm32f446/src/main.cpp)
pub static KNOWN_SYMBOLS: &[SwdSymbol] = &[
    SwdSymbol { name: "p_current_preset",    addr: 0x2000_17E4, sym_type: SwdSymbolType::Pointer, description: "Active synth patch definition & params" },
    SwdSymbol { name: "p_voice_manager",     addr: 0x2000_17E0, sym_type: SwdSymbolType::Pointer, description: "Polyphonic voice manager instance" },
    SwdSymbol { name: "p_mod_matrix",        addr: 0x2000_17DC, sym_type: SwdSymbolType::Pointer, description: "5x7 Modulation matrix routing table" },
    SwdSymbol { name: "p_oled_renderer",     addr: 0x2000_17D8, sym_type: SwdSymbolType::Pointer, description: "OLED graphical screen renderer" },
    SwdSymbol { name: "p_scheduler",         addr: 0x2000_17D4, sym_type: SwdSymbolType::Pointer, description: "FreeRTOS task scheduler" },
    SwdSymbol { name: "p_active_engine",     addr: 0x2000_17D0, sym_type: SwdSymbolType::Pointer, description: "Active DSP oscillator engine pointer" },
    SwdSymbol { name: "p_engine_registry",   addr: 0x2000_17CC, sym_type: SwdSymbolType::Pointer, description: "Standard oscillator engine registry" },
    SwdSymbol { name: "p_analog_inputs",     addr: 0x2000_17C8, sym_type: SwdSymbolType::Pointer, description: "12-bit ADC potentiometer inputs" },
    SwdSymbol { name: "p_envelope",          addr: 0x2000_17C4, sym_type: SwdSymbolType::Pointer, description: "Analog ADSR envelope state" },
    SwdSymbol { name: "p_audio_driver",      addr: 0x2000_17C0, sym_type: SwdSymbolType::Pointer, description: "Audio DMA driver & double-buffer pointers" },
    SwdSymbol { name: "p_ui_render_busy",    addr: 0x2000_17BC, sym_type: SwdSymbolType::Uint32,  description: "UI render task busy flag" },
    SwdSymbol { name: "p_panel_leds",        addr: 0x2000_17B8, sym_type: SwdSymbolType::Pointer, description: "Front panel LED state table" },
    SwdSymbol { name: "p_icon_leds",         addr: 0x2000_17B4, sym_type: SwdSymbolType::Pointer, description: "Capacitive icon LED states" },
    SwdSymbol { name: "p_hold_active",       addr: 0x2000_17B0, sym_type: SwdSymbolType::Bool,    description: "Hold button toggle state" },
    SwdSymbol { name: "p_spice_amount",      addr: 0x2000_17AC, sym_type: SwdSymbolType::Float,   description: "Spice modulation level (0.0 .. 1.0)" },
    SwdSymbol { name: "p_dice_amount",       addr: 0x2000_17A8, sym_type: SwdSymbolType::Float,   description: "Dice randomizer level (0.0 .. 1.0)" },
    SwdSymbol { name: "p_dice_status_gates", addr: 0x2000_17A4, sym_type: SwdSymbolType::Pointer, description: "Dice step gate status flags" },
    SwdSymbol { name: "p_arp_seq_mode",      addr: 0x2000_17A0, sym_type: SwdSymbolType::Uint8,   description: "Arpeggiator / Sequencer mode (0=Off, 1=Arp, 2=Seq)" },
    SwdSymbol { name: "p_seq_active",        addr: 0x2000_179C, sym_type: SwdSymbolType::Bool,    description: "Sequencer running state" },
    SwdSymbol { name: "p_amp_mod_active",    addr: 0x2000_1798, sym_type: SwdSymbolType::Bool,    description: "Amplifier modulation active flag" },
    SwdSymbol { name: "p_panel_mode",        addr: 0x2000_1794, sym_type: SwdSymbolType::Bool,    description: "Panel mode active flag" },
    SwdSymbol { name: "p_paraphonic",        addr: 0x2000_1790, sym_type: SwdSymbolType::Bool,    description: "Paraphonic 4-voice allocation flag" },
    SwdSymbol { name: "p_voice_mode",        addr: 0x2000_178C, sym_type: SwdSymbolType::Uint8,   description: "Voice allocation mode" },
    SwdSymbol { name: "p_tempo_bpm",         addr: 0x2000_1788, sym_type: SwdSymbolType::Float,   description: "Master synth tempo (BPM)" },
    SwdSymbol { name: "p_swing_pct",         addr: 0x2000_1784, sym_type: SwdSymbolType::Uint8,   description: "Sequencer swing percentage (50% .. 75%)" },
    SwdSymbol { name: "p_utility_menu_open", addr: 0x2000_1780, sym_type: SwdSymbolType::Bool,    description: "Utility settings menu open flag" },
    SwdSymbol { name: "p_save_state",        addr: 0x2000_177C, sym_type: SwdSymbolType::Uint8,   description: "Preset save state machine status" },
    SwdSymbol { name: "p_panel_led_tick_ms", addr: 0x2000_1778, sym_type: SwdSymbolType::Uint32,  description: "Panel LED animation tick millisecond counter" },
];

/// ARM Cortex-M SWD / JTAG Debugger Interface
#[derive(Debug, Clone)]
pub struct SwdDebugger {
    pub halted: bool,
    pub breakpoints: HashSet<u32>,
}

impl Default for SwdDebugger {
    fn default() -> Self {
        Self {
            halted: false,
            breakpoints: HashSet::new(),
        }
    }
}

impl SwdDebugger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_halted(&self) -> bool {
        self.halted
    }

    pub fn set_halted(&mut self, halted: bool) {
        self.halted = halted;
    }

    pub fn has_breakpoints(&self) -> bool {
        !self.breakpoints.is_empty()
    }

    pub fn check_breakpoint(&self, pc: u32) -> bool {
        self.breakpoints.contains(&(pc & !1))
    }
}

/// Helper functions for register and memory access
pub fn parse_num(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        u32::from_str_radix(&s[2..], 16).ok()
    } else if s.chars().all(|c| c.is_ascii_digit()) {
        s.parse::<u32>().ok()
    } else {
        u32::from_str_radix(s, 16).ok()
    }
}

pub fn arm_reg_by_name(name: &str) -> Option<(RegisterARM, &'static str)> {
    match name.to_lowercase().as_str() {
        "r0" => Some((RegisterARM::R0, "R0")),
        "r1" => Some((RegisterARM::R1, "R1")),
        "r2" => Some((RegisterARM::R2, "R2")),
        "r3" => Some((RegisterARM::R3, "R3")),
        "r4" => Some((RegisterARM::R4, "R4")),
        "r5" => Some((RegisterARM::R5, "R5")),
        "r6" => Some((RegisterARM::R6, "R6")),
        "r7" => Some((RegisterARM::R7, "R7")),
        "r8" => Some((RegisterARM::R8, "R8")),
        "r9" => Some((RegisterARM::R9, "R9")),
        "r10" => Some((RegisterARM::R10, "R10")),
        "r11" => Some((RegisterARM::R11, "R11")),
        "r12" => Some((RegisterARM::R12, "R12")),
        "sp" | "r13" => Some((RegisterARM::SP, "SP")),
        "lr" | "r14" => Some((RegisterARM::LR, "LR")),
        "pc" | "r15" => Some((RegisterARM::PC, "PC")),
        "cpsr" | "xpsr" | "psr" => Some((RegisterARM::CPSR, "xPSR")),
        "msp" => Some((RegisterARM::MSP, "MSP")),
        "psp" => Some((RegisterARM::PSP, "PSP")),
        "control" => Some((RegisterARM::CONTROL, "CONTROL")),
        "fpscr" => Some((RegisterARM::FPSCR, "FPSCR")),
        _ => None,
    }
}

pub fn read_core_registers(uc: &Unicorn<'static, EmulatorContext>) -> String {
    let r0 = uc.reg_read(RegisterARM::R0).unwrap_or(0);
    let r1 = uc.reg_read(RegisterARM::R1).unwrap_or(0);
    let r2 = uc.reg_read(RegisterARM::R2).unwrap_or(0);
    let r3 = uc.reg_read(RegisterARM::R3).unwrap_or(0);
    let r4 = uc.reg_read(RegisterARM::R4).unwrap_or(0);
    let r5 = uc.reg_read(RegisterARM::R5).unwrap_or(0);
    let r6 = uc.reg_read(RegisterARM::R6).unwrap_or(0);
    let r7 = uc.reg_read(RegisterARM::R7).unwrap_or(0);
    let r8 = uc.reg_read(RegisterARM::R8).unwrap_or(0);
    let r9 = uc.reg_read(RegisterARM::R9).unwrap_or(0);
    let r10 = uc.reg_read(RegisterARM::R10).unwrap_or(0);
    let r11 = uc.reg_read(RegisterARM::R11).unwrap_or(0);
    let r12 = uc.reg_read(RegisterARM::R12).unwrap_or(0);
    let sp = uc.reg_read(RegisterARM::SP).unwrap_or(0);
    let lr = uc.reg_read(RegisterARM::LR).unwrap_or(0);
    let pc = uc.reg_read(RegisterARM::PC).unwrap_or(0);
    let xpsr = uc.reg_read(RegisterARM::CPSR).unwrap_or(0);
    let msp = uc.reg_read(RegisterARM::MSP).unwrap_or(0);
    let psp = uc.reg_read(RegisterARM::PSP).unwrap_or(0);
    let ctrl = uc.reg_read(RegisterARM::CONTROL).unwrap_or(0);

    format!(
        "R0:  0x{r0:08X}   R1:  0x{r1:08X}   R2:  0x{r2:08X}   R3:  0x{r3:08X}\n\
         R4:  0x{r4:08X}   R5:  0x{r5:08X}   R6:  0x{r6:08X}   R7:  0x{r7:08X}\n\
         R8:  0x{r8:08X}   R9:  0x{r9:08X}   R10: 0x{r10:08X}   R11: 0x{r11:08X}\n\
         R12: 0x{r12:08X}   SP:  0x{sp:08X}   LR:  0x{lr:08X}   PC:  0x{pc:08X}\n\
         xPSR: 0x{xpsr:08X}  MSP: 0x{msp:08X}   PSP: 0x{psp:08X}   CTRL: 0x{ctrl:02X}\n"
    )
}

pub fn format_hexdump(uc: &Unicorn<'static, EmulatorContext>, addr: u32, len: usize) -> String {
    let mut out = String::new();
    let aligned_addr = addr & !15;
    let end_addr = (addr + len as u32).min(0xFFFF_FFFF);
    let total_bytes = ((end_addr - aligned_addr + 15) & !15) as usize;

    let mut buf = vec![0u8; total_bytes];
    let _ = uc.mem_read(aligned_addr as u64, &mut buf);

    for (i, chunk) in buf.chunks(16).enumerate() {
        let cur_addr = aligned_addr + (i * 16) as u32;
        let mut hex_part = String::new();
        let mut ascii_part = String::new();

        for (j, &b) in chunk.iter().enumerate() {
            if j == 8 {
                hex_part.push(' ');
            }
            hex_part.push_str(&format!("{b:02X} "));
            if (0x20..=0x7E).contains(&b) {
                ascii_part.push(b as char);
            } else {
                ascii_part.push('.');
            }
        }
        out.push_str(&format!("0x{cur_addr:08X}:  {hex_part:<49} |{ascii_part}|\n"));
    }
    out
}

/// Dispatches an interactive JTAG/SWD command against the VibeFreakEmulator instance.
pub fn dispatch_swd_command(emu: &mut VibeFreakEmulator, line: &str) -> String {
    let raw_trimmed = line.trim();
    if raw_trimmed.is_empty() {
        return String::new();
    }

    let parts: Vec<&str> = raw_trimmed.split_whitespace().collect();
    let cmd = parts[0].to_lowercase();

    match cmd.as_str() {
        "help" => {
            let mut out = String::from("VibeFreak ARM Cortex-M SWD / JTAG Debugger Commands:\n");
            out.push_str("  Target Control:\n");
            out.push_str("    status                 Show target state, PC, SP, SysTick, active breakpoints\n");
            out.push_str("    halt                   Halt target CPU core execution\n");
            out.push_str("    resume, c, cont        Resume target CPU execution\n");
            out.push_str("    step, si [count]       Single-step instruction(s) while halted\n");
            out.push_str("    reset [halt|run]       Reset CPU registers and reload vector table (VTOR)\n\n");
            out.push_str("  Registers:\n");
            out.push_str("    regs                   Dump R0-R12, SP, LR, PC, xPSR, MSP, PSP, CONTROL\n");
            out.push_str("    reg <name>             Read register value (e.g. 'reg pc', 'reg sp')\n");
            out.push_str("    reg <name> <val>       Write register (e.g. 'reg r0 0x1234')\n");
            out.push_str("    set <reg>=<val>        Set register (e.g. 'set pc=0x08025008')\n\n");
            out.push_str("  Memory Access:\n");
            out.push_str("    mdw <addr> [count]     Read 32-bit words (default 4)\n");
            out.push_str("    mdh <addr> [count]     Read 16-bit halfwords (default 8)\n");
            out.push_str("    mdb <addr> [count]     Read 8-bit bytes (default 16)\n");
            out.push_str("    mww <addr> <val>       Write 32-bit word to memory\n");
            out.push_str("    mwh <addr> <val>       Write 16-bit halfword to memory\n");
            out.push_str("    mwb <addr> <val>       Write 8-bit byte to memory\n");
            out.push_str("    dump <addr> [len]      Hexdump memory region (default 64 bytes)\n\n");
            out.push_str("  Breakpoints:\n");
            out.push_str("    break, bp <addr>       Set execution breakpoint\n");
            out.push_str("    del, rbp <addr>        Clear execution breakpoint\n");
            out.push_str("    breakpoints            List active breakpoints\n\n");
            out.push_str("  Firmware Introspection:\n");
            out.push_str("    sym [filter]           List exported firmware symbols & pointers\n");
            out.push_str("    print, p <symbol>      Inspect & dereference symbol or memory address\n");
            out.push_str("    periph <name>          Inspect peripheral MMIO registers (SVD)\n");
            out.push_str("    nvic                   Show active & enabled interrupt vectors\n");
            out.push_str("    memmap                 Display STM32F446 memory bus regions\n");
            out.push_str("    clear, cls             Clear console screen\n");
            out
        }

        "status" => {
            let pc = emu.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
            let sp = emu.uc.reg_read(RegisterARM::SP).unwrap_or(0) as u32;
            let lr = emu.uc.reg_read(RegisterARM::LR).unwrap_or(0) as u32;
            let xpsr = emu.uc.reg_read(RegisterARM::CPSR).unwrap_or(0) as u32;
            let state_str = if emu.swd.halted { "HALTED" } else { "RUNNING" };
            format!(
                "Target:      STM32F446xE (ARM Cortex-M4F @ 168 MHz)\n\
                 State:       {state_str}\n\
                 PC:          0x{pc:08X}\n\
                 SP:          0x{sp:08X}\n\
                 LR:          0x{lr:08X}\n\
                 xPSR:        0x{xpsr:08X}\n\
                 SysTick:     {} cycles\n\
                 Breakpoints: {} active\n",
                emu.systick_cycle_accum,
                emu.swd.breakpoints.len()
            )
        }

        "halt" | "stop" => {
            emu.swd.set_halted(true);
            let pc = emu.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
            format!("[SWD] Target halted at PC: 0x{pc:08X}\n")
        }

        "resume" | "c" | "cont" | "continue" => {
            emu.swd.set_halted(false);
            "[SWD] Target resumed (running)\n".to_string()
        }

        "step" | "si" | "stepi" => {
            let count = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).min(100);
            emu.swd.set_halted(true);

            let mut stepped = 0;
            for _ in 0..count {
                let cur_pc = emu.uc.reg_read(RegisterARM::PC).unwrap_or(0);
                if emu.uc.emu_start(cur_pc, 0xFFFF_FFFF, 0, 1).is_err() {
                    break;
                }
                stepped += 1;
            }
            let new_pc = emu.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
            emu.pc = new_pc;
            format!("[SWD] Stepped {stepped} instruction(s). Current PC: 0x{new_pc:08X}\n")
        }

        "reset" => {
            let halt = parts.get(1).map(|s| s.eq_ignore_ascii_case("halt")).unwrap_or(false);
            let vtor = emu.profile.vtor;
            let mut sp_bytes = [0u8; 4];
            let mut pc_bytes = [0u8; 4];
            let _ = emu.uc.mem_read(vtor as u64, &mut sp_bytes);
            let _ = emu.uc.mem_read((vtor + 4) as u64, &mut pc_bytes);
            let sp = u32::from_le_bytes(sp_bytes);
            let reset_handler = u32::from_le_bytes(pc_bytes);

            let _ = emu.uc.reg_write(RegisterARM::SP, sp as u64);
            let _ = emu.uc.reg_write(RegisterARM::MSP, sp as u64);
            let _ = emu.uc.reg_write(RegisterARM::PC, (reset_handler | 1) as u64);
            emu.pc = reset_handler;
            emu.swd.set_halted(halt);
            format!(
                "[SWD] Core reset. SP: 0x{sp:08X}, PC: 0x{reset_handler:08X} (state: {})\n",
                if halt { "HALTED" } else { "RUNNING" }
            )
        }

        "regs" => read_core_registers(&emu.uc),

        "reg" => {
            if parts.len() < 2 {
                return "[SWD] Usage: reg <name> [val]\n".to_string();
            }
            let reg_name = parts[1];
            if let Some((reg, display_name)) = arm_reg_by_name(reg_name) {
                if parts.len() == 2 {
                    let val = emu.uc.reg_read(reg).unwrap_or(0);
                    format!("{display_name} = 0x{val:08X}\n")
                } else {
                    if let Some(val) = parse_num(parts[2]) {
                        if emu.uc.reg_write(reg, val as u64).is_ok() {
                            format!("[SWD] Set {display_name} = 0x{val:08X}\n")
                        } else {
                            format!("[SWD] Error: Failed to write register {display_name}\n")
                        }
                    } else {
                        format!("[SWD] Error: Invalid register value '{}'\n", parts[2])
                    }
                }
            } else {
                format!("[SWD] Unknown register name '{reg_name}'. Type 'regs' to see available registers.\n")
            }
        }

        "set" => {
            if parts.len() < 2 {
                return "[SWD] Usage: set <reg>=<val>\n".to_string();
            }
            let assignment = parts[1..].join("");
            let mut split = assignment.split('=');
            let reg_name = split.next().unwrap_or("").trim();
            let val_str = split.next().unwrap_or("").trim();
            if let Some((reg, display_name)) = arm_reg_by_name(reg_name) {
                if let Some(val) = parse_num(val_str) {
                    if emu.uc.reg_write(reg, val as u64).is_ok() {
                        format!("[SWD] Set {display_name} = 0x{val:08X}\n")
                    } else {
                        format!("[SWD] Error: Failed to write {display_name}\n")
                    }
                } else {
                    format!("[SWD] Error: Invalid numeric value '{val_str}'\n")
                }
            } else {
                format!("[SWD] Unknown register name '{reg_name}'\n")
            }
        }

        "mdw" | "x/w" => {
            if parts.len() < 2 {
                return "[SWD] Usage: mdw <addr> [count]\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let count = parts.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(4).min(64);
            let aligned = addr & !3;
            let mut out = String::new();

            for row in 0..((count + 3) / 4) {
                let row_addr = aligned + (row * 16) as u32;
                out.push_str(&format!("0x{row_addr:08X}:"));
                for col in 0..4 {
                    let word_idx = row * 4 + col;
                    if word_idx >= count {
                        break;
                    }
                    let word_addr = row_addr + (col * 4) as u32;
                    let mut b = [0u8; 4];
                    let _ = emu.uc.mem_read(word_addr as u64, &mut b);
                    let val = u32::from_le_bytes(b);
                    out.push_str(&format!(" 0x{val:08X}"));
                }
                out.push('\n');
            }
            out
        }

        "mdh" => {
            if parts.len() < 2 {
                return "[SWD] Usage: mdh <addr> [count]\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let count = parts.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(8).min(128);
            let aligned = addr & !1;
            let mut out = String::new();

            for row in 0..((count + 7) / 8) {
                let row_addr = aligned + (row * 16) as u32;
                out.push_str(&format!("0x{row_addr:08X}:"));
                for col in 0..8 {
                    let half_idx = row * 8 + col;
                    if half_idx >= count {
                        break;
                    }
                    let half_addr = row_addr + (col * 2) as u32;
                    let mut b = [0u8; 2];
                    let _ = emu.uc.mem_read(half_addr as u64, &mut b);
                    let val = u16::from_le_bytes(b);
                    out.push_str(&format!(" 0x{val:04X}"));
                }
                out.push('\n');
            }
            out
        }

        "mdb" => {
            if parts.len() < 2 {
                return "[SWD] Usage: mdb <addr> [count]\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let count = parts.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(16).min(256);
            let mut out = String::new();

            for row in 0..((count + 15) / 16) {
                let row_addr = addr + (row * 16) as u32;
                out.push_str(&format!("0x{row_addr:08X}:"));
                for col in 0..16 {
                    let byte_idx = row * 16 + col;
                    if byte_idx >= count {
                        break;
                    }
                    let byte_addr = row_addr + col as u32;
                    let mut b = [0u8; 1];
                    let _ = emu.uc.mem_read(byte_addr as u64, &mut b);
                    out.push_str(&format!(" {:02X}", b[0]));
                }
                out.push('\n');
            }
            out
        }

        "mww" => {
            if parts.len() < 3 {
                return "[SWD] Usage: mww <addr> <val>\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let Some(val) = parse_num(parts[2]) else {
                return format!("[SWD] Invalid value: {}\n", parts[2]);
            };
            let aligned = addr & !3;
            if emu.uc.mem_write(aligned as u64, &val.to_le_bytes()).is_ok() {
                format!("[SWD] Wrote 0x{val:08X} to 0x{aligned:08X}\n")
            } else {
                format!("[SWD] Error writing to 0x{aligned:08X}\n")
            }
        }

        "mwh" => {
            if parts.len() < 3 {
                return "[SWD] Usage: mwh <addr> <val>\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let Some(val) = parse_num(parts[2]) else {
                return format!("[SWD] Invalid value: {}\n", parts[2]);
            };
            let aligned = addr & !1;
            let val16 = (val & 0xFFFF) as u16;
            if emu.uc.mem_write(aligned as u64, &val16.to_le_bytes()).is_ok() {
                format!("[SWD] Wrote 0x{val16:04X} to 0x{aligned:08X}\n")
            } else {
                format!("[SWD] Error writing to 0x{aligned:08X}\n")
            }
        }

        "mwb" => {
            if parts.len() < 3 {
                return "[SWD] Usage: mwb <addr> <val>\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let Some(val) = parse_num(parts[2]) else {
                return format!("[SWD] Invalid value: {}\n", parts[2]);
            };
            let val8 = (val & 0xFF) as u8;
            if emu.uc.mem_write(addr as u64, &[val8]).is_ok() {
                format!("[SWD] Wrote 0x{val8:02X} to 0x{addr:08X}\n")
            } else {
                format!("[SWD] Error writing to 0x{addr:08X}\n")
            }
        }

        "dump" => {
            if parts.len() < 2 {
                return "[SWD] Usage: dump <addr> [len]\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let len = parts.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(64).min(512);
            format_hexdump(&emu.uc, addr, len)
        }

        "break" | "bp" => {
            if parts.len() < 2 {
                return "[SWD] Usage: break <addr>\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let bp_addr = addr & !1;
            emu.swd.breakpoints.insert(bp_addr);
            format!("[SWD] Breakpoint set at 0x{bp_addr:08X}\n")
        }

        "del" | "delete" | "rbp" => {
            if parts.len() < 2 {
                return "[SWD] Usage: del <addr>\n".to_string();
            }
            let Some(addr) = parse_num(parts[1]) else {
                return format!("[SWD] Invalid address: {}\n", parts[1]);
            };
            let bp_addr = addr & !1;
            if emu.swd.breakpoints.remove(&bp_addr) {
                format!("[SWD] Breakpoint at 0x{bp_addr:08X} removed\n")
            } else {
                format!("[SWD] No breakpoint found at 0x{bp_addr:08X}\n")
            }
        }

        "breakpoints" => {
            if emu.swd.breakpoints.is_empty() {
                "[SWD] No active breakpoints.\n".to_string()
            } else {
                let mut out = String::from("Active Breakpoints:\n");
                for bp in &emu.swd.breakpoints {
                    out.push_str(&format!("  * 0x{bp:08X}\n"));
                }
                out
            }
        }

        "sym" | "symbols" => {
            let filter = parts.get(1).map(|s| s.to_lowercase());
            let mut out = String::from(
                "Exported Firmware Symbols & Introspection Pointers:\n\
                 Symbol Name                 Address     Target/Val  Description\n\
                 --------------------------------------------------------------------------------\n"
            );
            for sym in KNOWN_SYMBOLS {
                if let Some(f) = &filter {
                    if !sym.name.to_lowercase().contains(f) && !sym.description.to_lowercase().contains(f) {
                        continue;
                    }
                }

                let mut b = [0u8; 4];
                let _ = emu.uc.mem_read(sym.addr as u64, &mut b);
                let val32 = u32::from_le_bytes(b);

                let target_str = match sym.sym_type {
                    SwdSymbolType::Pointer => {
                        format!("-> 0x{val32:08X}")
                    }
                    SwdSymbolType::Float => {
                        let f = f32::from_le_bytes(b);
                        format!("{f:.2}")
                    }
                    SwdSymbolType::Bool => {
                        if (val32 & 0xFF) != 0 { "true".to_string() } else { "false".to_string() }
                    }
                    SwdSymbolType::Uint32 => {
                        format!("{val32}")
                    }
                    SwdSymbolType::Uint8 => {
                        format!("{}", val32 & 0xFF)
                    }
                };

                out.push_str(&format!(
                    "{:<26}  0x{:08X}  {:<10}  {}\n",
                    sym.name, sym.addr, target_str, sym.description
                ));
            }
            out
        }

        "print" | "p" => {
            if parts.len() < 2 {
                return "[SWD] Usage: print <symbol|addr>\n".to_string();
            }
            let target = parts[1];
            if let Some(sym) = KNOWN_SYMBOLS.iter().find(|s| s.name.eq_ignore_ascii_case(target)) {
                let mut b = [0u8; 4];
                let _ = emu.uc.mem_read(sym.addr as u64, &mut b);
                let val32 = u32::from_le_bytes(b);
                match sym.sym_type {
                    SwdSymbolType::Pointer => {
                        let mut target_data = [0u8; 16];
                        let _ = emu.uc.mem_read(val32 as u64, &mut target_data);
                        format!(
                            "{} @ 0x{:08X} -> pointer to 0x{:08X}\nTarget memory: {:02X?}\nDescription: {}\n",
                            sym.name, sym.addr, val32, &target_data[..8], sym.description
                        )
                    }
                    SwdSymbolType::Float => {
                        let f = f32::from_le_bytes(b);
                        format!("{} @ 0x{:08X} = {f:.3} (float)\n", sym.name, sym.addr)
                    }
                    SwdSymbolType::Bool => {
                        let b_val = (val32 & 0xFF) != 0;
                        format!("{} @ 0x{:08X} = {b_val} (bool)\n", sym.name, sym.addr)
                    }
                    SwdSymbolType::Uint32 => {
                        format!("{} @ 0x{:08X} = 0x{val32:08X} ({val32})\n", sym.name, sym.addr)
                    }
                    SwdSymbolType::Uint8 => {
                        let b_val = (val32 & 0xFF) as u8;
                        format!("{} @ 0x{:08X} = 0x{b_val:02X} ({b_val})\n", sym.name, sym.addr)
                    }
                }
            } else if let Some(addr) = parse_num(target) {
                let mut b = [0u8; 4];
                let _ = emu.uc.mem_read(addr as u64, &mut b);
                let val32 = u32::from_le_bytes(b);
                format!("*0x{addr:08X} = 0x{val32:08X} ({val32})\n")
            } else {
                format!("[SWD] Symbol '{target}' not found. Type 'sym' to list available symbols.\n")
            }
        }

        "periph" | "svd" => {
            if parts.len() < 2 {
                return "[SWD] Usage: periph <name> (e.g. periph rcc, periph gpioa, periph tim2)\n".to_string();
            }
            let query = parts[1].to_uppercase();
            let mut matches = 0;
            let mut out = format!("STM32F446 Peripheral Registers ({query}):\n");

            for &(addr, ref info) in crate::svd_diagnostics::SVD_REGISTERS {
                if info.peripheral_name.eq_ignore_ascii_case(&query)
                    || format!("{}_{}", info.peripheral_name, info.register_name).to_uppercase().contains(&query)
                {
                    let mut b = [0u8; 4];
                    let _ = emu.uc.mem_read(addr as u64, &mut b);
                    let val = u32::from_le_bytes(b);
                    out.push_str(&format!(
                        "  {}->{:<6} (0x{:08X}): 0x{:08X}  (reset: 0x{:08X})\n",
                        info.peripheral_name, info.register_name, addr, val, info.reset_value
                    ));
                    matches += 1;
                    if matches >= 32 {
                        out.push_str("  ... (truncated at 32 registers)\n");
                        break;
                    }
                }
            }

            if matches == 0 {
                format!("[SWD] No peripheral registers found matching '{query}'.\n")
            } else {
                out
            }
        }

        "nvic" => {
            let mut out = String::from("NVIC Interrupt Vector Status:\n");
            let irqs = [
                (38, "USART1 (MIDI IN)"),
                (39, "USART2 (Coprocessor Bus)"),
                (40, "USART3 (DIN MIDI)"),
                (67, "OTG_FS (USB MIDI)"),
                (81, "DMA2_Stream4 (Audio DAC PING)"),
                (83, "DMA2_Stream6 (Audio DAC PONG)"),
                (85, "DMA2_Stream0 (ADC Scanner)"),
                (40, "EXTI15_10 (Touch Keys)"),
            ];
            for &(irq, name) in &irqs {
                let enabled = emu.nvic().is_enabled(irq);
                let pending = emu.nvic().is_pending(irq);
                out.push_str(&format!(
                    "  IRQ {:<2} - {:<28}: enabled = {:<5} pending = {}\n",
                    irq, name, enabled, pending
                ));
            }
            out
        }

        "memmap" => {
            String::from(
                "STM32F446 Memory Map Overview:\n\
                 Region              Base Address   Size       Description\n\
                 ------------------------------------------------------------------\n\
                 FLASH (Internal)    0x0800_0000    512 KB     Main firmware payload\n\
                 SRAM1               0x2000_0000    112 KB     Main system SRAM & heap\n\
                 SRAM2               0x2001_C000    16 KB      Audio & DMA ringbuffers\n\
                 PERIPHERALS (APB1)  0x4000_0000    512 KB     Timers, USART, I2C, DAC\n\
                 PERIPHERALS (APB2)  0x4001_0000    512 KB     SPI, SAI, ADC, EXTI\n\
                 PERIPHERALS (AHB1)  0x4002_0000    512 KB     GPIO Ports A..H, DMA1/2, RCC\n\
                 QSPI FLASH          0x9000_0000    16 MB      Factory wavetables & user presets\n\
                 SYSTEM ROM          0x1FFF_0000    30 KB      STM32 bootloader ROM\n\
                 SYSTEM CONTROL      0xE000_E000    4 KB       Cortex-M4 NVIC & SysTick\n"
            )
        }

        "clear" | "cls" => "\x1b[2J\x1b[H".to_string(),

        unknown => {
            format!("[SWD] Unknown command: '{unknown}'. Type 'help' for available commands.\n")
        }
    }
}
