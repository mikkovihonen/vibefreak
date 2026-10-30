use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use vibefreak_core::{AudioInputSource, HeadsetJackState};
use vibefreak_web::ClientCommand;

use crate::backend::EmulatorBackend;
use crate::display::{render_oled_ascii, render_oled_braille, render_panel_state};
use crate::protocol::{CallToolResult, Tool};

pub fn get_tools_list() -> Vec<Tool> {
    vec![
        // -------------------------------------------------------------------
        // Category 1: SWD Debugger & Introspection
        // -------------------------------------------------------------------
        Tool {
            name: "swd_status".into(),
            description: "Get target ARM Cortex-M4 CPU core state (RUNNING or HALTED), Program Counter (PC), Stack Pointer (SP), Link Register (LR), xPSR, SysTick cycle count, and active breakpoints.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_halt".into(),
            description: "Halt ARM Cortex-M4 CPU core execution. Halts the emulation loop and preserves register and memory state for inspection.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_resume".into(),
            description: "Resume target ARM Cortex-M4 CPU core execution.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_step".into(),
            description: "Single-step one or more CPU instructions while CPU core is halted.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "count": {
                        "type": "integer",
                        "description": "Number of instructions to step (default: 1, max: 100).",
                        "default": 1
                    }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_reset".into(),
            description: "Reset ARM Cortex-M4 core registers and reload vector table (VTOR) from reset handler.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "halt": {
                        "type": "boolean",
                        "description": "Whether to halt at reset vector instead of running immediately (default: false).",
                        "default": false
                    }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_read_registers".into(),
            description: "Dump all ARM Cortex-M4 core registers: R0-R12, SP, LR, PC, xPSR, MSP, PSP, and CONTROL.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_read_register".into(),
            description: "Read a specific ARM register value by name (e.g. 'pc', 'sp', 'r0', 'lr', 'xpsr', 'msp', 'psp', 'control').".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Register name (e.g. 'pc', 'sp', 'r0', 'r1', 'lr')."
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_write_register".into(),
            description: "Write a 32-bit value to a specific ARM register.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Register name (e.g. 'r0', 'r1', 'pc', 'sp')."
                    },
                    "value": {
                        "type": ["integer", "string"],
                        "description": "32-bit register value (integer or hex string like '0x08025008')."
                    }
                },
                "required": ["name", "value"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_read_memory".into(),
            description: "Read memory contents from STM32F446 memory bus (Flash, SRAM1, SRAM2, MMIO peripherals, or QSPI Flash).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "address": {
                        "type": ["integer", "string"],
                        "description": "Target address (integer or hex string like '0x20000000')."
                    },
                    "format": {
                        "type": "string",
                        "enum": ["word", "halfword", "byte"],
                        "description": "Access size: 'word' (32-bit), 'halfword' (16-bit), or 'byte' (8-bit). Default: 'word'.",
                        "default": "word"
                    },
                    "count": {
                        "type": "integer",
                        "description": "Number of elements to read (default: 4, max: 64).",
                        "default": 4
                    }
                },
                "required": ["address"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_write_memory".into(),
            description: "Write a 32-bit word, 16-bit halfword, or 8-bit byte into target memory (SRAM or MMIO register).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "address": {
                        "type": ["integer", "string"],
                        "description": "Target address (e.g. '0x20000000')."
                    },
                    "value": {
                        "type": ["integer", "string"],
                        "description": "Value to write (e.g. 0xDEADBEEF or '0x1234')."
                    },
                    "format": {
                        "type": "string",
                        "enum": ["word", "halfword", "byte"],
                        "description": "Write width (default: 'word').",
                        "default": "word"
                    }
                },
                "required": ["address", "value"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_hexdump".into(),
            description: "Hexdump a region of memory with 16-byte aligned rows and ASCII representation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "address": {
                        "type": ["integer", "string"],
                        "description": "Start memory address."
                    },
                    "length": {
                        "type": "integer",
                        "description": "Number of bytes to dump (default: 64, max: 512).",
                        "default": 64
                    }
                },
                "required": ["address"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_breakpoint_set".into(),
            description: "Set an instruction execution breakpoint at the specified address.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "address": {
                        "type": ["integer", "string"],
                        "description": "Breakpoint code address (e.g. '0x08025008')."
                    }
                },
                "required": ["address"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_breakpoint_clear".into(),
            description: "Clear / delete an existing execution breakpoint.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "address": {
                        "type": ["integer", "string"],
                        "description": "Breakpoint address to remove."
                    }
                },
                "required": ["address"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_breakpoint_list".into(),
            description: "List all currently active breakpoints.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_list_symbols".into(),
            description: "List known firmware exported symbols, variables, and introspection pointers (e.g. p_current_preset, p_mod_matrix, p_voice_manager, p_tempo_bpm, p_active_engine).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "string",
                        "description": "Optional substring filter for symbol name or description."
                    }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_read_symbol".into(),
            description: "Inspect and dereference a known firmware symbol or memory pointer (e.g. 'p_current_preset', 'p_tempo_bpm', 'p_paraphonic', 'p_mod_matrix').".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Symbol name or memory address."
                    }
                },
                "required": ["symbol"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_svd_peripheral".into(),
            description: "Inspect peripheral MMIO registers (RCC, GPIOA..GPIOH, TIM1..TIM14, USART1..USART3, SPI, I2C, ADC, DMA, etc.) according to STM32F446 SVD specification.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Peripheral name or register search term (e.g. 'RCC', 'GPIOA', 'TIM2', 'USART1')."
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_nvic_status".into(),
            description: "Show Nested Vectored Interrupt Controller (NVIC) vectors, enabled status, and pending status (e.g. USART MIDI, Coprocessor bus, DMA Audio DAC, EXTI touch keys).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_memmap".into(),
            description: "Display STM32F446 memory map overview (Flash, SRAM1, SRAM2, APB1/APB2/AHB1 peripherals, QSPI flash, etc.).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        Tool {
            name: "swd_execute_command".into(),
            description: "Execute any raw SWD / JTAG command line string (e.g. 'regs', 'mdw 0x20000000 8', 'step 5', 'dump 0x200017E4 32', etc.).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "SWD command line to execute."
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        },

        // -------------------------------------------------------------------
        // Category 2: Synthesizer Physical Controls
        // -------------------------------------------------------------------
        Tool {
            name: "synth_press_key".into(),
            description: "Press a key on the 25-key capacitive keybed (keys 0 to 24, corresponding to 2 octaves C to C). Also sets touch pressure (velocity/aftertouch).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "integer",
                        "description": "Key index 0..24 (0 is lowest C, 24 is highest C).",
                        "minimum": 0,
                        "maximum": 24
                    },
                    "pressure": {
                        "type": "integer",
                        "description": "Touch pressure / aftertouch level (1..255). Default: 127.",
                        "minimum": 1,
                        "maximum": 255,
                        "default": 127
                    }
                },
                "required": ["key"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_release_key".into(),
            description: "Release a key on the 25-key capacitive keybed (sets pressure to 0).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "integer",
                        "description": "Key index 0..24.",
                        "minimum": 0,
                        "maximum": 24
                    }
                },
                "required": ["key"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_press_button".into(),
            description: "Press or release a front panel microswitch/button (e.g. 'octave_down', 'octave_up', 'hold', 'paraphonic', 'panel', 'save', 'utility', 'arp_seq', 'arp_pattern', 'seq_a', 'seq_b', 'matrix_0_0'..'matrix_4_6').".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Button name."
                    },
                    "pressed": {
                        "type": "boolean",
                        "description": "true to press down, false to release."
                    }
                },
                "required": ["name", "pressed"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_click_button".into(),
            description: "Click (press and quickly release) a front panel microswitch button, advancing emulation slightly so the firmware scans and registers the event.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Button name (e.g. 'hold', 'paraphonic', 'panel', 'save', 'utility', 'octave_up', 'octave_down')."
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_step_encoder".into(),
            description: "Rotate one of the 8 front panel endless encoders by delta steps (+ for clockwise, - for counter-clockwise).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "encoder": {
                        "type": "string",
                        "enum": ["preset", "matrix", "type", "rate", "lfo_rate", "wave", "timbre", "shape"],
                        "description": "Encoder name."
                    },
                    "delta": {
                        "type": "integer",
                        "description": "Number of steps to rotate (e.g. +1, -1, +5, -10)."
                    }
                },
                "required": ["encoder", "delta"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_click_encoder".into(),
            description: "Push/click the push-button microswitch integrated into an encoder (e.g. 'preset' to confirm preset or open list, 'matrix' to assign modulation, 'rate', 'lfo_rate').".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "encoder": {
                        "type": "string",
                        "enum": ["preset", "matrix", "rate", "lfo_rate"],
                        "description": "Encoder name."
                    }
                },
                "required": ["encoder"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_set_potentiometer".into(),
            description: "Set an analog potentiometer knob position using 12-bit ADC value (0..4095). Can specify knob by name or channel ID (1..16).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "knob": {
                        "type": "string",
                        "enum": ["cutoff", "resonance", "attack", "decay", "sustain", "glide", "filter_amount", "cycling_rise", "cycling_fall", "cycling_hold", "cycling_amount", "master_volume"],
                        "description": "Named potentiometer knob."
                    },
                    "channel": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 16,
                        "description": "Knob channel number (if not using knob name)."
                    },
                    "value": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 4095,
                        "description": "12-bit potentiometer value (0 = minimum, 4095 = maximum)."
                    }
                },
                "required": ["value"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_set_touch_strip".into(),
            description: "Set position and touch state on the capacitive pitch / spice / dice touch strip.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "position": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 4095,
                        "description": "Touch position (0 = leftmost/bottom, 4095 = rightmost/top)."
                    },
                    "touched": {
                        "type": "boolean",
                        "description": "Whether finger is currently contacting the strip."
                    }
                },
                "required": ["position", "touched"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_set_filter_mode".into(),
            description: "Set the discrete SEM analog filter mode switch (lowpass, bandpass, highpass, notch).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["lowpass", "bandpass", "highpass", "notch"],
                        "description": "Filter mode."
                    }
                },
                "required": ["mode"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_send_midi".into(),
            description: "Send MIDI message bytes to the synthesizer (DIN MIDI In / USART3). Accepts array of byte values or hex string.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "bytes": {
                        "type": ["array", "string"],
                        "description": "MIDI bytes (e.g. [144, 60, 100] for Note On C4 vel 100, or '90 3C 64')."
                    }
                },
                "required": ["bytes"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synth_rear_panel_control".into(),
            description: "Control rear panel inputs: Clock In, Headset Jack insertion, Headset microphone button, or Audio input source selection.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "clock_in": {
                        "type": "boolean",
                        "description": "Set Clock In signal level (true = high, false = low)."
                    },
                    "clock_pulse_ms": {
                        "type": "integer",
                        "description": "Send a clock trigger pulse with duration in milliseconds (default: 10ms)."
                    },
                    "headset_jack": {
                        "type": "string",
                        "enum": ["disconnected", "standard_headphones", "headset_with_mic"],
                        "description": "Headset jack insertion state."
                    },
                    "headset_button": {
                        "type": "boolean",
                        "description": "Press or release headset mic inline button."
                    },
                    "audio_input_source": {
                        "type": "string",
                        "enum": ["rear_jack", "headset_mic"],
                        "description": "Audio input route."
                    }
                },
                "additionalProperties": false
            }),
        },

        // -------------------------------------------------------------------
        // Category 3: Emulator State & Display Inspection
        // -------------------------------------------------------------------
        Tool {
            name: "emu_read_oled".into(),
            description: "Read and render the current 128x64 monochrome OLED display. Returns high-resolution Braille ASCII art showing the synth's screen, plus statistics (active pixels, non-zero bytes).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "format": {
                        "type": "string",
                        "enum": ["braille", "ascii", "raw_hex"],
                        "description": "Display rendering format. Default: 'braille'.",
                        "default": "braille"
                    }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "emu_read_panel".into(),
            description: "Read full panel status: 5x7 Modulation Matrix LED grid, 35 Front Panel LEDs, 8 Capacitive Icon LEDs, and Rear Panel CV/Gate/Clock outputs.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        Tool {
            name: "emu_step".into(),
            description: "Advance emulation by a specific duration or number of audio blocks, stepping both the STM32F446 CPU and STM32F0 coprocessors. Crucial for letting firmware scan keys/encoders and update the screen.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "duration_ms": {
                        "type": "integer",
                        "description": "Milliseconds of simulation time to advance (default: 50, max: 2000).",
                        "default": 50
                    },
                    "blocks": {
                        "type": "integer",
                        "description": "Explicit number of 32-sample audio blocks to render (1 block ≈ 0.66ms)."
                    },
                    "capture_screen": {
                        "type": "boolean",
                        "description": "If true, also returns the updated OLED screen in the response.",
                        "default": false
                    }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "emu_save_preset".into(),
            description: "Save current preset changes by flushing the 16 MB QSPI flash image to disk.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to save user presets (default: 'user_presets.bin')."
                    }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "emu_load_preset".into(),
            description: "Reload user presets and wavetables from disk into QSPI flash memory.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to load presets from (default: 'user_presets.bin')."
                    }
                },
                "additionalProperties": false
            }),
        },
    ]
}

pub async fn handle_call_tool(
    backend: &Arc<dyn EmulatorBackend>,
    name: &str,
    args: &Value,
) -> CallToolResult {
    match name {
        // --- SWD Commands ---
        "swd_status" => match backend.execute_swd("status").await {
            Ok(out) => CallToolResult::text(out),
            Err(e) => CallToolResult::error(e),
        },
        "swd_halt" => match backend.execute_swd("halt").await {
            Ok(out) => CallToolResult::text(out),
            Err(e) => CallToolResult::error(e),
        },
        "swd_resume" => match backend.execute_swd("resume").await {
            Ok(out) => CallToolResult::text(out),
            Err(e) => CallToolResult::error(e),
        },
        "swd_step" => {
            let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(1);
            match backend.execute_swd(&format!("step {count}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_reset" => {
            let halt = args.get("halt").and_then(|v| v.as_bool()).unwrap_or(false);
            let arg = if halt { "halt" } else { "run" };
            match backend.execute_swd(&format!("reset {arg}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_read_registers" => match backend.execute_swd("regs").await {
            Ok(out) => CallToolResult::text(out),
            Err(e) => CallToolResult::error(e),
        },
        "swd_read_register" => {
            let Some(reg_name) = args.get("name").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'name'");
            };
            match backend.execute_swd(&format!("reg {reg_name}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_write_register" => {
            let Some(reg_name) = args.get("name").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'name'");
            };
            let val_str = match args.get("value") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => format!("0x{:X}", n.as_u64().unwrap_or(0)),
                _ => return CallToolResult::error("Missing required parameter: 'value'"),
            };
            match backend.execute_swd(&format!("reg {reg_name} {val_str}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_read_memory" => {
            let Some(addr_str) = parse_address_arg(args.get("address")) else {
                return CallToolResult::error("Missing or invalid 'address' parameter");
            };
            let fmt = args.get("format").and_then(|v| v.as_str()).unwrap_or("word");
            let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(4);
            let cmd = match fmt {
                "byte" => format!("mdb {addr_str} {count}"),
                "halfword" => format!("mdh {addr_str} {count}"),
                _ => format!("mdw {addr_str} {count}"),
            };
            match backend.execute_swd(&cmd).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_write_memory" => {
            let Some(addr_str) = parse_address_arg(args.get("address")) else {
                return CallToolResult::error("Missing or invalid 'address' parameter");
            };
            let val_str = match args.get("value") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => format!("0x{:X}", n.as_u64().unwrap_or(0)),
                _ => return CallToolResult::error("Missing required parameter: 'value'"),
            };
            let fmt = args.get("format").and_then(|v| v.as_str()).unwrap_or("word");
            let cmd = match fmt {
                "byte" => format!("mwb {addr_str} {val_str}"),
                "halfword" => format!("mwh {addr_str} {val_str}"),
                _ => format!("mww {addr_str} {val_str}"),
            };
            match backend.execute_swd(&cmd).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_hexdump" => {
            let Some(addr_str) = parse_address_arg(args.get("address")) else {
                return CallToolResult::error("Missing or invalid 'address' parameter");
            };
            let len = args.get("length").and_then(|v| v.as_u64()).unwrap_or(64);
            match backend.execute_swd(&format!("dump {addr_str} {len}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_breakpoint_set" => {
            let Some(addr_str) = parse_address_arg(args.get("address")) else {
                return CallToolResult::error("Missing or invalid 'address' parameter");
            };
            match backend.execute_swd(&format!("break {addr_str}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_breakpoint_clear" => {
            let Some(addr_str) = parse_address_arg(args.get("address")) else {
                return CallToolResult::error("Missing or invalid 'address' parameter");
            };
            match backend.execute_swd(&format!("del {addr_str}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_breakpoint_list" => match backend.execute_swd("breakpoints").await {
            Ok(out) => CallToolResult::text(out),
            Err(e) => CallToolResult::error(e),
        },
        "swd_list_symbols" => {
            let filter = args.get("filter").and_then(|v| v.as_str()).unwrap_or("");
            let cmd = if filter.is_empty() {
                "sym".to_string()
            } else {
                format!("sym {filter}")
            };
            match backend.execute_swd(&cmd).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_read_symbol" => {
            let Some(sym) = args.get("symbol").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'symbol'");
            };
            match backend.execute_swd(&format!("print {sym}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_svd_peripheral" => {
            let Some(name) = args.get("name").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'name'");
            };
            match backend.execute_swd(&format!("periph {name}")).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }
        "swd_nvic_status" => match backend.execute_swd("nvic").await {
            Ok(out) => CallToolResult::text(out),
            Err(e) => CallToolResult::error(e),
        },
        "swd_memmap" => match backend.execute_swd("memmap").await {
            Ok(out) => CallToolResult::text(out),
            Err(e) => CallToolResult::error(e),
        },
        "swd_execute_command" => {
            let Some(cmd) = args.get("command").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'command'");
            };
            match backend.execute_swd(cmd).await {
                Ok(out) => CallToolResult::text(out),
                Err(e) => CallToolResult::error(e),
            }
        }

        // --- Synthesizer Controls ---
        "synth_press_key" => {
            let Some(key) = args.get("key").and_then(|v| v.as_u64()) else {
                return CallToolResult::error("Missing required parameter: 'key'");
            };
            let pressure = args.get("pressure").and_then(|v| v.as_u64()).unwrap_or(127) as u8;
            let cmd = ClientCommand::Key {
                key: key as usize,
                pressure,
            };
            match backend.apply_command(cmd).await {
                Ok(_) => CallToolResult::text(format!("Key {key} pressed with pressure {pressure}")),
                Err(e) => CallToolResult::error(e),
            }
        }
        "synth_release_key" => {
            let Some(key) = args.get("key").and_then(|v| v.as_u64()) else {
                return CallToolResult::error("Missing required parameter: 'key'");
            };
            let cmd = ClientCommand::Key {
                key: key as usize,
                pressure: 0,
            };
            match backend.apply_command(cmd).await {
                Ok(_) => CallToolResult::text(format!("Key {key} released")),
                Err(e) => CallToolResult::error(e),
            }
        }
        "synth_press_button" => {
            let Some(btn_name) = args.get("name").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'name'");
            };
            let Some(pressed) = args.get("pressed").and_then(|v| v.as_bool()) else {
                return CallToolResult::error("Missing required parameter: 'pressed'");
            };
            let cmd = ClientCommand::Microswitch {
                name: btn_name.to_string(),
                action: None,
                pressed: Some(pressed),
            };
            match backend.apply_command(cmd).await {
                Ok(_) => CallToolResult::text(format!(
                    "Button '{btn_name}' set to {}",
                    if pressed { "PRESSED" } else { "RELEASED" }
                )),
                Err(e) => CallToolResult::error(e),
            }
        }
        "synth_click_button" => {
            let Some(btn_name) = args.get("name").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'name'");
            };
            // Down
            let cmd_down = ClientCommand::Microswitch {
                name: btn_name.to_string(),
                action: None,
                pressed: Some(true),
            };
            if let Err(e) = backend.apply_command(cmd_down).await {
                return CallToolResult::error(e);
            }
            let _ = backend.step(15).await; // advance ~10ms for matrix scan
            // Up
            let cmd_up = ClientCommand::Microswitch {
                name: btn_name.to_string(),
                action: None,
                pressed: Some(false),
            };
            if let Err(e) = backend.apply_command(cmd_up).await {
                return CallToolResult::error(e);
            }
            let _ = backend.step(15).await;
            CallToolResult::text(format!("Clicked button '{btn_name}'"))
        }
        "synth_step_encoder" => {
            let Some(enc_name) = args.get("encoder").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'encoder'");
            };
            let Some(delta) = args.get("delta").and_then(|v| v.as_i64()) else {
                return CallToolResult::error("Missing required parameter: 'delta'");
            };
            let cmd = ClientCommand::Encoder {
                name: enc_name.to_string(),
                delta: delta as i16,
            };
            match backend.apply_command(cmd).await {
                Ok(_) => CallToolResult::text(format!("Rotated encoder '{enc_name}' by delta {delta}")),
                Err(e) => CallToolResult::error(e),
            }
        }
        "synth_click_encoder" => {
            let Some(enc_name) = args.get("encoder").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'encoder'");
            };
            let cmd_down = ClientCommand::EncoderWithMicroswitch {
                name: enc_name.to_string(),
                action: None,
                pressed: Some(true),
                delta: None,
            };
            if let Err(e) = backend.apply_command(cmd_down).await {
                return CallToolResult::error(e);
            }
            let _ = backend.step(15).await;
            let cmd_up = ClientCommand::EncoderWithMicroswitch {
                name: enc_name.to_string(),
                action: None,
                pressed: Some(false),
                delta: None,
            };
            if let Err(e) = backend.apply_command(cmd_up).await {
                return CallToolResult::error(e);
            }
            let _ = backend.step(15).await;
            CallToolResult::text(format!("Clicked encoder '{enc_name}' switch"))
        }
        "synth_set_potentiometer" => {
            let val = match args.get("value").and_then(|v| v.as_u64()) {
                Some(v) => (v as u16).min(4095),
                None => return CallToolResult::error("Missing required parameter: 'value'"),
            };

            let channel = if let Some(knob) = args.get("knob").and_then(|v| v.as_str()) {
                match knob.to_lowercase().as_str() {
                    "cutoff" => 2,
                    "resonance" => 3,
                    "attack" => 7,
                    "decay" => 8,
                    "sustain" => 9,
                    "glide" => 10,
                    "filter_amount" | "filter_amt" => 11,
                    "cycling_rise" | "cyc_rise" => 12,
                    "cycling_fall" | "cyc_fall" => 13,
                    "cycling_hold" | "cyc_hold" => 14,
                    "cycling_amount" | "cyc_amount" => 15,
                    "master_volume" | "volume" => 16,
                    other => return CallToolResult::error(format!("Unknown knob name '{other}'")),
                }
            } else if let Some(ch) = args.get("channel").and_then(|v| v.as_u64()) {
                ch as u16
            } else {
                return CallToolResult::error("Either 'knob' name or 'channel' number (1..16) must be provided");
            };

            let cmd = ClientCommand::Potentiometer {
                channel,
                value: val,
            };
            match backend.apply_command(cmd).await {
                Ok(_) => CallToolResult::text(format!("Set potentiometer channel {channel} to {val}")),
                Err(e) => CallToolResult::error(e),
            }
        }
        "synth_set_touch_strip" => {
            let Some(pos) = args.get("position").and_then(|v| v.as_u64()) else {
                return CallToolResult::error("Missing required parameter: 'position'");
            };
            let Some(touched) = args.get("touched").and_then(|v| v.as_bool()) else {
                return CallToolResult::error("Missing required parameter: 'touched'");
            };
            let cmd = ClientCommand::Touchstrip {
                value: (pos as u16).min(4095),
                touched,
            };
            match backend.apply_command(cmd).await {
                Ok(_) => CallToolResult::text(format!("Set touch strip position: {pos}, touched: {touched}")),
                Err(e) => CallToolResult::error(e),
            }
        }
        "synth_set_filter_mode" => {
            let Some(mode) = args.get("mode").and_then(|v| v.as_str()) else {
                return CallToolResult::error("Missing required parameter: 'mode'");
            };
            let cmd = ClientCommand::SetFilterMode {
                mode: mode.to_string(),
            };
            match backend.apply_command(cmd).await {
                Ok(_) => CallToolResult::text(format!("Set SEM filter mode to '{mode}'")),
                Err(e) => CallToolResult::error(e),
            }
        }
        "synth_send_midi" => {
            let bytes = match args.get("bytes") {
                Some(Value::Array(arr)) => {
                    arr.iter().filter_map(|v| v.as_u64().map(|b| b as u8)).collect()
                }
                Some(Value::String(s)) => parse_hex_bytes(s),
                _ => return CallToolResult::error("Missing required parameter: 'bytes'"),
            };
            let len = bytes.len();
            let cmd = ClientCommand::Midi { bytes };
            match backend.apply_command(cmd).await {
                Ok(_) => CallToolResult::text(format!("Sent {len} MIDI bytes to synthesizer")),
                Err(e) => CallToolResult::error(e),
            }
        }
        "synth_rear_panel_control" => {
            let mut msgs = Vec::new();
            if let Some(cin) = args.get("clock_in").and_then(|v| v.as_bool()) {
                let _ = backend.apply_command(ClientCommand::ClockIn { high: cin }).await;
                msgs.push(format!("Clock In = {cin}"));
            }
            if let Some(pulse_ms) = args.get("clock_pulse_ms").and_then(|v| v.as_u64()) {
                let _ = backend.apply_command(ClientCommand::ClockPulse {
                    duration_ms: Some(pulse_ms as u32),
                }).await;
                msgs.push(format!("Clock Pulse {pulse_ms} ms triggered"));
            }
            if let Some(jack_str) = args.get("headset_jack").and_then(|v| v.as_str()) {
                let state = match jack_str.to_lowercase().as_str() {
                    "standard_headphones" | "headphones" => HeadsetJackState::Headphones,
                    "headset_with_mic" | "mic" => HeadsetJackState::HeadsetWithMic,
                    _ => HeadsetJackState::Disconnected,
                };
                let _ = backend.apply_command(ClientCommand::SetHeadsetJack { state }).await;
                msgs.push(format!("Headset Jack = {state:?}"));
            }
            if let Some(btn) = args.get("headset_button").and_then(|v| v.as_bool()) {
                let _ = backend.apply_command(ClientCommand::SetHeadsetButton { pressed: btn }).await;
                msgs.push(format!("Headset Button = {btn}"));
            }
            if let Some(src_str) = args.get("audio_input_source").and_then(|v| v.as_str()) {
                let source = match src_str.to_lowercase().as_str() {
                    "headset_mic" | "mic" => AudioInputSource::MicIn,
                    _ => AudioInputSource::LineIn,
                };
                let _ = backend.apply_command(ClientCommand::SetAudioInputSource { source }).await;
                msgs.push(format!("Audio In Source = {source:?}"));
            }

            if msgs.is_empty() {
                CallToolResult::text("No rear panel parameters specified.")
            } else {
                CallToolResult::text(msgs.join("\n"))
            }
        }

        // --- Emulator & Display Inspection ---
        "emu_read_oled" => {
            let fmt = args.get("format").and_then(|v| v.as_str()).unwrap_or("braille");
            match backend.get_oled().await {
                Ok(fb) => {
                    let out = match fmt {
                        "ascii" => render_oled_ascii(&fb),
                        "raw_hex" => {
                            let mut hex = String::with_capacity(fb.len() * 2);
                            for b in fb {
                                hex.push_str(&format!("{b:02X}"));
                            }
                            hex
                        }
                        _ => render_oled_braille(&fb),
                    };
                    CallToolResult::text(out)
                }
                Err(e) => CallToolResult::error(e),
            }
        }
        "emu_read_panel" => match backend.get_snapshot().await {
            Ok(snap) => CallToolResult::text(render_panel_state(&snap)),
            Err(e) => CallToolResult::error(e),
        },
        "emu_step" => {
            let blocks = if let Some(b) = args.get("blocks").and_then(|v| v.as_u64()) {
                b as usize
            } else {
                let ms = args.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(50);
                // 1 block = 32 samples / 48000 Hz = 0.6666 ms -> 1.5 blocks per ms
                ((ms as f64 * 1.5).round() as usize).max(1)
            };

            let capture = args.get("capture_screen").and_then(|v| v.as_bool()).unwrap_or(false);

            if let Err(e) = backend.step(blocks).await {
                return CallToolResult::error(e);
            }

            let mut out = format!("Advanced emulation by {blocks} blocks (~{:.1} ms)\n", (blocks as f64) / 1.5);
            if capture {
                if let Ok(fb) = backend.get_oled().await {
                    out.push('\n');
                    out.push_str(&render_oled_braille(&fb));
                }
            }
            CallToolResult::text(out)
        }
        "emu_save_preset" => {
            let path = args.get("path").and_then(|v| v.as_str()).map(PathBuf::from);
            match backend.save_presets(path).await {
                Ok(msg) => CallToolResult::text(msg),
                Err(e) => CallToolResult::error(e),
            }
        }
        "emu_load_preset" => {
            let path = args.get("path").and_then(|v| v.as_str()).map(PathBuf::from);
            match backend.load_presets(path).await {
                Ok(msg) => CallToolResult::text(msg),
                Err(e) => CallToolResult::error(e),
            }
        }

        unknown => CallToolResult::error(format!("Unknown tool: '{unknown}'")),
    }
}

fn parse_address_arg(val: Option<&Value>) -> Option<String> {
    match val {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(format!("0x{:X}", n.as_u64().unwrap_or(0))),
        _ => None,
    }
}

fn parse_hex_bytes(s: &str) -> Vec<u8> {
    let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let mut bytes = Vec::new();
    let mut chars = clean.chars();
    while let (Some(c1), Some(c2)) = (chars.next(), chars.next()) {
        let hex_str = format!("{c1}{c2}");
        if let Ok(b) = u8::from_str_radix(&hex_str, 16) {
            bytes.push(b);
        }
    }
    bytes
}
