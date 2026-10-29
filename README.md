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

OPTIONS:
    -p, --port <PORT>        Web server port (default: 8081)
    --presets <PATH>         User preset disk persistence file (default: user_presets.bin)
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

## License

VibeFreak is licensed under the [GNU General Public License v3.0 or later (GPL-3.0-or-later)](LICENSE).

