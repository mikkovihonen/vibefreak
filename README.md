# VibeFreak

**VibeFreak** is an independent, 100% standalone, cycle-accurate multi-core hardware emulator and native Web HMI for the **Arturia MicroFreak** paraphonic hybrid synthesizer.

Written in Rust, VibeFreak emulates the entire physical hardware architecture:
- **Main MCU**: STMicroelectronics STM32F446 (ARM Cortex-M4 with single-precision FPU @ 180 MHz)
- **OLED Coprocessor**: STMicroelectronics STM32F0 (ARM Cortex-M0) driving the 128x64 monochrome SSD1306 display
- **Matrix Coprocessor**: STMicroelectronics STM32F0 (ARM Cortex-M0) scanning 48 panel contacts and encoders
- **Keyboard Coprocessor**: STMicroelectronics STM32F0 (ARM Cortex-M0) servicing the 25-key capacitive keybed & touch strip
- **Audio Codec**: Texas Instruments TLV320AIC3104 I2S stereo DAC/ADC with PGA preamp
- **Discrete Analog Filter**: Continuous Oberheim SEM 2-pole 12 dB/oct multimode filter (LP, BP, HP, Notch)
- **Debug Interface**: Built-in ARM Cortex-M SWD/JTAG terminal with run control (`halt`, `step`, `resume`, `regs`, `mdw`, `mww`)

---

## Quick Start (Standalone Packaged Firmware Boot)

VibeFreak natively parses Arturia `.mff` update archives directly in memory—no Python scripts, no manual file extraction, and no external dependencies required.

### 1. Installation
Install the standalone binary using Cargo:
```bash
cargo install --path crates/vibefreak-cli
```
This installs the `vibefreak` executable directly into `~/.cargo/bin`.

### 2. Launching Packaged Firmware
Simply pass any `.mff` firmware package (official or custom):
```bash
vibefreak MicroFreak_Firmware_Update_5_0_0_2084.mff
```
Or launch without arguments to auto-detect any `.mff` in the current working directory:
```bash
vibefreak
```

### 3. Web HMI & Audio Output
Once launched:
- Open your browser to **http://localhost:8081** to interact with the full front-panel chassis, OLED display, and SWD console.
- Real-time 48 kHz stereo audio output begins streaming immediately through your system's default audio device.

---

## CLI Usage

```text
USAGE:
    vibefreak [OPTIONS] [PATH_TO_MFF | COMMAND]

ARGUMENTS:
    [PATH_TO_MFF]            Path to Arturia .mff firmware package (e.g. firmware.mff)

COMMANDS:
    test [PATH_TO_MFF]       Run headless boot test and verify OLED output
    build-qspi [PATH]        Generate factory 16 MB QSPI flash image (default: user_presets.bin)
    mcp [OPTIONS] [PATH]     Run Model Context Protocol (MCP) server for coding agents

OPTIONS:
    -p, --port <PORT>        Web server port (default: 8081)
    --presets <PATH>         User preset disk persistence file (default: user_presets.bin)
    --standalone             Force standalone in-process emulator in MCP mode
    --connect <HOST:PORT>    Connect to existing VibeFreak instance in MCP mode
    --firmware-dir <DIR>     Directory containing loose payload binaries
    --open-firmware          Use open-source firmware from build/ directory
    --main-bin <PATH>        Explicit path to Main MCU binary (STM32F446)
    --qspi-bin <PATH>        Explicit path to QSPI wavetable binary
    --oled-bin <PATH>        Explicit path to OLED coprocessor binary (STM32F0)
    --matrix-bin <PATH>      Explicit path to Matrix coprocessor binary (STM32F0)
    --kbd-bin <PATH>         Explicit path to Keyboard coprocessor binary (STM32F0)
    --trace-mmio             Enable verbose MMIO peripheral bus tracing
    -h, --help               Print this help message
```

---

## Integration in Firmware Projects

VibeFreak can be included as an emulator toolchain target in MicroFreak firmware development projects (such as `open-microfreak`):

```makefile
# In firmware Makefile:
VIBEFREAK ?= $(shell command -v vibefreak 2>/dev/null || echo "cargo run --manifest-path emulator/Cargo.toml --release --bin vibefreak --")

emu: package
	$(VIBEFREAK) dist/firmware.mff

emu-debug: package
	$(VIBEFREAK) --trace-mmio dist/firmware.mff
```

---

## Model Context Protocol (MCP) Server for Coding Agents

VibeFreak natively implements the **Model Context Protocol (MCP)** (JSON-RPC 2.0 stdio transport), allowing autonomous AI coding agents (such as Antigravity, Claude Code, Cursor, Claude Desktop, and Cline) to control hardware emulation, perform live ARM Cortex-M SWD debugging, inspect memory/registers, and operate all front panel synthesizer controls.

### 1. Agent Configuration

Add VibeFreak to your AI agent's MCP configuration:

#### Antigravity / Claude Code / Cursor (`mcp.json` / `.cursor/mcp.json` / `claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "vibefreak": {
      "command": "vibefreak",
      "args": ["mcp"]
    }
  }
}
```
Or use the standalone binary:
```json
{
  "mcpServers": {
    "vibefreak": {
      "command": "vibefreak-mcp"
    }
  }
}
```

### 2. Dual Operation Modes (Auto-Detect, Standalone, or Live Attach)

- **Auto Mode (Default)**: Automatically checks if a live VibeFreak emulator is running with Web HMI on port 8081. If running, it attaches seamlessly over HTTP so the coding agent manipulates the live synthesizer while you listen to audio and watch the web GUI. If not running, it automatically boots an in-process native emulator!
- **Standalone Mode (`--standalone`)**: Forces booting a dedicated, headless in-process emulator instance directly.
- **Remote Connect Mode (`--connect 127.0.0.1:8081`)**: Connects to a specific running VibeFreak server instance.

### 3. Available MCP Tools

#### ARM Cortex-M SWD Debugger Tools
| Tool Name | Description |
|---|---|
| `swd_status` | Show CPU state (`RUNNING` / `HALTED`), PC, SP, LR, xPSR, SysTick cycles, and active breakpoints |
| `swd_halt` | Halt the ARM Cortex-M4 CPU core |
| `swd_resume` | Resume CPU core execution |
| `swd_step` | Single-step one or more CPU instructions while halted |
| `swd_reset` | Reset CPU registers and reload vector table (VTOR) |
| `swd_read_registers` | Dump R0-R12, SP, LR, PC, xPSR, MSP, PSP, and CONTROL |
| `swd_read_register` | Read single register by name (e.g. `pc`, `sp`, `r0`, `lr`) |
| `swd_write_register` | Write 32-bit value to register |
| `swd_read_memory` | Read memory words, halfwords, or bytes (Flash, SRAM, MMIO, QSPI) |
| `swd_write_memory` | Write 32-bit word, 16-bit halfword, or 8-bit byte |
| `swd_hexdump` | Hexdump memory region with ASCII representation |
| `swd_breakpoint_set` | Set instruction execution breakpoint at address |
| `swd_breakpoint_clear` | Clear breakpoint at address |
| `swd_breakpoint_list` | List all active breakpoints |
| `swd_list_symbols` | List exported firmware symbols (`p_current_preset`, `p_mod_matrix`, `p_tempo_bpm`, etc.) |
| `swd_read_symbol` | Inspect and dereference firmware symbol or pointer |
| `swd_svd_peripheral` | Inspect peripheral MMIO registers (RCC, GPIO, TIM, USART, DMA) via STM32F446 SVD |
| `swd_nvic_status` | Show NVIC interrupt vector enabled and pending flags |
| `swd_memmap` | Display STM32F446 bus memory regions |
| `swd_execute_command` | Execute any raw SWD command string |

#### Synthesizer Physical Controls
| Tool Name | Description |
|---|---|
| `synth_press_key` | Press a key on the 25-key capacitive keybed (keys 0..24) with pressure/velocity (1..255) |
| `synth_release_key` | Release a key on the keybed |
| `synth_press_button` | Press or release a front panel button/microswitch (`hold`, `paraphonic`, `octave_up`, `octave_down`, etc.) |
| `synth_click_button` | Click (press and release) a front panel button, advancing clock cycles for debounce/scanning |
| `synth_step_encoder` | Rotate an endless encoder (`preset`, `matrix`, `type`, `rate`, `lfo_rate`, `wave`, `timbre`, `shape`) by delta steps |
| `synth_click_encoder` | Push/click an encoder switch (`preset` to confirm, `matrix` to assign, `rate`, `lfo_rate`) |
| `synth_set_potentiometer` | Set 12-bit ADC knob position (0..4095) for `cutoff`, `resonance`, `attack`, `decay`, `sustain`, `glide`, `volume`, etc. |
| `synth_set_touch_strip` | Control pitch bend / spice / dice capacitive touch strip position (0..4095) and touch contact |
| `synth_set_filter_mode` | Set discrete SEM filter switch (`lowpass`, `bandpass`, `highpass`, `notch`) |
| `synth_send_midi` | Send raw MIDI bytes (Note On, CC, Program Change) |
| `synth_rear_panel_control` | Control Clock In, trigger clock pulses, headset jack insertion, and audio in source |

#### Emulator State & Display Inspection
| Tool Name | Description |
|---|---|
| `emu_read_oled` | Render 128x64 monochrome OLED screen in terminal-friendly Unicode Braille (`⠀` to `⣿`) |
| `emu_read_panel` | Read 5x7 Modulation Matrix LED grid, 35 front panel LEDs, 8 icon LEDs, and CV outputs |
| `emu_step` | Advance emulation by simulation milliseconds or audio blocks (with optional OLED capture) |
| `emu_save_preset` | Flush 16 MB QSPI flash image to disk |
| `emu_load_preset` | Reload user presets from disk into QSPI flash memory |

### 4. MCP Resources
- `vibefreak://oled/screen`: High-resolution Braille ASCII rendering of current OLED screen
- `vibefreak://panel/state`: Complete front panel LED matrix and hardware state
- `vibefreak://swd/status`: ARM Cortex-M4 CPU core registers and halted status
- `vibefreak://swd/symbols`: Exported firmware symbols and pointer table

### 5. MCP Prompts
- `debug_firmware`: Interactive workflow to diagnose crashes, inspect registers, and check interrupts
- `inspect_preset`: Guided workflow to analyze active patch, oscillator engine, and modulation matrix routing
- `test_patch_audio`: Automated synthesis test workflow playing keys and adjusting filter cutoff

---

## License

VibeFreak is licensed under the [GNU General Public License v3.0 or later (GPL-3.0-or-later)](LICENSE).

