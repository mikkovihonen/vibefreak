# VibeFreak

<div align="center">
<img src="logo.png" width="300"/>
</div>

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

VibeFreak is distributed as a 100% clean-room emulator and does **not** ship with copyrighted firmware ROMs or proprietary wavetables. To boot the full synthesizer emulation, supply your own legally obtained official Arturia MicroFreak firmware update package (`.mff`):

1. Download the firmware update archive (`MicroFreak_Firmware_Update_*.mff`) from Arturia's official website.
2. Launch VibeFreak by passing the file path:
```bash
vibefreak MicroFreak_Firmware_Update_5_0_0_2084.mff
```
3. Alternatively, place the firmware package in one of the auto-detected locations:
   - Current directory (`./firmware.mff` or any `*.mff`)
   - User config directory: `~/.config/vibefreak/firmware.mff` (or `~/.vibefreak/firmware.mff`)
   - Or set the environment variable: `export VIBEFREAK_MFF=/path/to/firmware.mff`
   Then simply run:
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

## QSPI NOR Flash Architecture & Valid Memory Specification

The MicroFreak hardware features an external 16 MB ISSI NOR flash connected via STM32 Quad-SPI (mapped in memory-mapped mode at `0x9000_0000..0x90FF_FFFF`). VibeFreak accurately replicates every memory bank, sector trailer, and verification check expected by the stock firmware:

| Memory Region | Address Range | Size | Description & Validation Criteria |
|---|---|---|---|
| **Factory Stock Wavetables** | `0x0000_0000..0x0003_FFFF` | 256 KB | 16 stock tables for Engine 3 (Wavetable). Each table holds 32 single-cycle waveforms with 256 16-bit PCM samples (512 bytes/cycle). Extracted from offset `+0x0001_0000` of the firmware package's 320 KB wavetable target (`0x81`). Clean-room state: `0xFF`. |
| **Reserved Flash** | `0x0004_0000..0x0008_0FFF` | ~260 KB | Unused erased NOR sectors (`0xFF`). |
| **Active & Stored Presets** | `0x0008_1000..0x0028_0FFF` | ~2 MB | Up to 512 patch slots (4096 bytes per sector). Each sector requires valid TLV section tags (`VCO`, `VCF`, `EG1`, `Kbd`, `Arp`, `LFO`, `EG2`, `Gen`, `Sys`, `Mat`, `Seq`) and flash integrity trailer bytes (`0xA5`) at byte offsets 1040, 2082, 3052, and 4095. |
| **User Sample Memory** | `0x0028_1000..0x00F5_0FFF` | ~13 MB | 64 sample slots for Engine 19 (Sample) and Engine 20 (Grain). Each slot begins with a 24-byte header: magic `0xFA55AAEF`, byte length, non-zero token ID, loop start/end offsets, sample slot index, followed by 16-bit signed PCM audio. |
| **User Wavetable Bank A** | `0x00F5_F000..0x00FA_EFFF` | 320 KB | User wavetables for Engine 18 (WaveUser). The first 64 KB contains 16 table descriptors (names and cycle indices); the subsequent 256 KB contains raw 16-bit PCM waveform cycles. Loaded dynamically from target `0x81`. Clean-room state: `0xFF`. |
| **User Wavetable Bank B** | `0x00FA_F000..0x00FE_EFFF` | 256 KB | Alternative user wavetable bank. When cleanly erased (`0xFF`), firmware routes user wavetable queries to Bank A. |
| **QSPI Status Sector** | `0x00FF_F000..0x00FF_FFFF` | 4 KB | Byte `0x00FF_F000` must contain ASCII `'Q'` (`0x51`) to signal formatted flash memory. |

---

## Legal & Trademark Disclaimer

**VibeFreak** is an independent, clean-room hardware emulator and developer tool created for educational and reverse-engineering purposes.

- **No Affiliation**: VibeFreak is not affiliated, associated, authorized, endorsed by, or in any way officially connected with **Arturia SA**, or any of its subsidiaries or affiliates.
- **Trademarks**: The names **"Arturia"** and **"MicroFreak"**, as well as related names, marks, emblems, and images, are registered trademarks of Arturia SA. Their use in this repository is strictly nominative to describe hardware compatibility and interoperability.
- **No Proprietary Assets Distributed**: This software repository does not contain, distribute, or bundle any copyrighted Arturia firmware binaries, proprietary sound ROMs, factory sample banks, or proprietary wavetable files. All emulated operations require the user to provide their own legally obtained firmware archive.

---

## License

VibeFreak is licensed under the [GNU General Public License v3.0 or later (GPL-3.0-or-later)](LICENSE).

