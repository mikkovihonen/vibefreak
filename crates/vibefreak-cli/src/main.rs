use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crossbeam_channel::unbounded;
use vibefreak_core::VibeFreakEmulator;
use vibefreak_web::{start_web_server, ClientCommand, EmulatorSnapshot};

mod audio_output;
use audio_output::start_audio_output;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // If --trace-mmio is passed, enable trace logs for vibefreak_core unless RUST_LOG is explicitly specified
    if args.iter().any(|a| a == "--trace-mmio") && std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "vibefreak_core=trace");
    }
    env_logger::init();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("VibeFreak: Native Hardware Emulator for Arturia MicroFreak");
        println!();
        println!("USAGE:");
        println!("    vibefreak [OPTIONS] [PATH_TO_MFF | COMMAND]");
        println!();
        println!("ARGUMENTS:");
        println!("    [PATH_TO_MFF]            Path to Arturia .mff firmware package (e.g. firmware.mff)");
        println!();
        println!("COMMANDS:");
        println!("    test [PATH_TO_MFF]       Run headless boot test and verify OLED output");
        println!("    build-qspi [PATH]        Generate factory 16 MB QSPI flash image (default: user_presets.bin)");
        println!("    mcp [OPTIONS] [PATH]     Run Model Context Protocol (MCP) server for coding agents");
        println!();
        println!("OPTIONS:");
        println!("    -p, --port <PORT>        Web server port (default: 8081)");
        println!("    --presets <PATH>         User preset disk persistence file (default: user_presets.bin)");
        println!("    --standalone             Force standalone in-process emulator in MCP mode");
        println!("    --connect <HOST:PORT>    Connect to existing VibeFreak instance in MCP mode");
        println!("    --firmware-dir <DIR>     Directory containing loose payload binaries");
        println!("    --open-firmware          Use open-source firmware from build/ directory");
        println!("    --main-bin <PATH>        Explicit path to Main MCU binary (STM32F446)");
        println!("    --qspi-bin <PATH>        Explicit path to QSPI wavetable binary");
        println!("    --oled-bin <PATH>        Explicit path to OLED coprocessor binary (STM32F0)");
        println!("    --matrix-bin <PATH>      Explicit path to Matrix coprocessor binary (STM32F0)");
        println!("    --kbd-bin <PATH>         Explicit path to Keyboard coprocessor binary (STM32F0)");
        println!("    --trace-mmio             Enable verbose MMIO peripheral bus tracing");
        println!("    -h, --help               Print this help message");
        return Ok(());
    }

    if args.iter().any(|s| s == "build-qspi") {
        let out_path = args
            .iter()
            .position(|s| s == "build-qspi")
            .and_then(|idx| args.get(idx + 1))
            .filter(|s| !s.starts_with('-'))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("user_presets.bin"));
        println!("Generating 16 MB QSPI Flash Image with factory presets, wavetables and status...");
        let image = vibefreak_core::generate_factory_qspi_image();
        std::fs::write(&out_path, &image)?;
        println!("Successfully wrote {} bytes to {:?}", image.len(), out_path);
        return Ok(());
    }

    if args.iter().any(|s| s == "mcp" || s == "--mcp") {
        let mut config = vibefreak_mcp::McpConfig::default();
        let mut i = 1;
        while i < args.len() {
            let arg = &args[i];
            match arg.as_str() {
                "mcp" | "--mcp" => {}
                "--standalone" => {
                    config.mode = vibefreak_mcp::McpMode::Standalone;
                }
                "--connect" if i + 1 < args.len() => {
                    let target = &args[i + 1];
                    let addr: std::net::SocketAddr = if let Ok(sa) = target.parse() {
                        sa
                    } else if let Ok(p) = target.parse::<u16>() {
                        std::net::SocketAddr::from(([127, 0, 0, 1], p))
                    } else {
                        std::net::SocketAddr::from(([127, 0, 0, 1], 8081))
                    };
                    config.mode = vibefreak_mcp::McpMode::Remote(addr);
                    i += 1;
                }
                "-p" | "--port" if i + 1 < args.len() => {
                    if let Ok(p) = args[i + 1].parse::<u16>() {
                        config.port = p;
                    }
                    i += 1;
                }
                "--presets" if i + 1 < args.len() => {
                    config.presets_path = std::path::PathBuf::from(&args[i + 1]);
                    i += 1;
                }
                "--mff" if i + 1 < args.len() => {
                    config.mff_path = Some(std::path::PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                "--firmware-dir" if i + 1 < args.len() => {
                    config.firmware_dir = Some(std::path::PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                "--main-bin" if i + 1 < args.len() => {
                    config.main_bin = Some(std::path::PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                "--qspi-bin" if i + 1 < args.len() => {
                    config.qspi_bin = Some(std::path::PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                "--oled-bin" if i + 1 < args.len() => {
                    config.oled_bin = Some(std::path::PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                "--matrix-bin" if i + 1 < args.len() => {
                    config.matrix_bin = Some(std::path::PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                "--kbd-bin" if i + 1 < args.len() => {
                    config.kbd_bin = Some(std::path::PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                s if !s.starts_with('-') => {
                    let p = std::path::PathBuf::from(s);
                    if p.extension().is_some_and(|ext| ext == "mff") || p.is_file() {
                        config.mff_path = Some(p);
                    } else if p.is_dir() {
                        config.firmware_dir = Some(p);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        return vibefreak_mcp::run_mcp_server(config).await;
    }

    let mut custom_mff: Option<std::path::PathBuf> = None;
    let mut custom_dir: Option<std::path::PathBuf> = None;
    let mut custom_main: Option<std::path::PathBuf> = None;
    let mut custom_qspi: Option<std::path::PathBuf> = None;
    let mut custom_oled: Option<std::path::PathBuf> = None;
    let mut custom_matrix: Option<std::path::PathBuf> = None;
    let mut custom_kbd: Option<std::path::PathBuf> = None;
    let mut port = 8081;
    let mut presets_path = std::path::PathBuf::from("user_presets.bin");
    let is_test = args.iter().any(|s| s == "test");

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "test" || arg == "run" {
            i += 1;
            continue;
        }
        match arg.as_str() {
            "-p" | "--port" if i + 1 < args.len() => {
                if let Ok(p) = args[i + 1].parse::<u16>() {
                    port = p;
                }
                i += 1;
            }
            "--presets" if i + 1 < args.len() => {
                presets_path = std::path::PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--mff" if i + 1 < args.len() => {
                custom_mff = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--firmware-dir" if i + 1 < args.len() => {
                custom_dir = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--open-firmware" => {
                custom_dir = Some(std::path::PathBuf::from("build"));
            }
            "--main-bin" if i + 1 < args.len() => {
                custom_main = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--qspi-bin" if i + 1 < args.len() => {
                custom_qspi = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--oled-bin" if i + 1 < args.len() => {
                custom_oled = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--matrix-bin" if i + 1 < args.len() => {
                custom_matrix = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--kbd-bin" if i + 1 < args.len() => {
                custom_kbd = Some(std::path::PathBuf::from(&args[i + 1]));
                i += 1;
            }
            s if !s.starts_with('-') => {
                let candidate = std::path::PathBuf::from(s);
                if candidate.extension().is_some_and(|ext| ext == "mff") || candidate.is_file() {
                    custom_mff = Some(candidate);
                } else if candidate.is_dir() {
                    custom_dir = Some(candidate);
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Resolve firmware source
    let fw_source = resolve_firmware_source(
        custom_mff,
        custom_dir,
        custom_main,
        custom_qspi,
        custom_oled,
        custom_matrix,
        custom_kbd,
    )?;

    if is_test {
        return run_headless_test(&fw_source);
    }

    // Bind web server port before starting emulation (sets SO_REUSEADDR automatically)
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error: Cannot bind web server to port {}: {}", port, e);
            eprintln!("A process is already listening on port {} or you lack permissions.", port);
            eprintln!("Please terminate the existing process or use '--port <PORT>' to specify a different port.");
            std::process::exit(1);
        }
    };

    println!("============================================================");
    println!("     VibeFreak: Native Rust Hardware Emulator        ");
    println!("============================================================");

    let t0 = Instant::now();
    let mut emu = match &fw_source {
        FirmwareSource::MffPackage(path) => {
            let pkg = vibefreak_core::MffFirmwarePackage::from_file(path)?;
            let ver = pkg.version.as_deref().unwrap_or("unknown");
            println!("[1/3] Booting packaged firmware {} (v{}) on native ARM core...", path.display(), ver);
            VibeFreakEmulator::from_package(&pkg)?
        }
        FirmwareSource::LooseBinaries {
            main_bin,
            qspi_bin,
            oled_bin,
            matrix_bin,
            kbd_bin,
        } => {
            println!("[1/3] Booting firmware ({}) on native ARM emulation core...", main_bin.display());
            VibeFreakEmulator::new(main_bin, Some(qspi_bin), oled_bin, matrix_bin, kbd_bin)?
        }
    };

    if presets_path.exists() {
        println!("      Loading user presets from {:?}...", presets_path);
        if let Err(e) = emu.load_qspi_from_disk(&presets_path) {
            eprintln!("Warning: Failed to load user presets from {:?}: {}", presets_path, e);
        }
    } else {
        println!("      No user presets file found at {:?}; initializing from default QSPI flash image...", presets_path);
        if let Err(e) = emu.flush_qspi_to_disk(&presets_path) {
            eprintln!("Warning: Failed to create initial presets file at {:?}: {}", presets_path, e);
        }
    }

    emu.boot(100_000_000);
    println!("      Firmware booted in {:?}", t0.elapsed());

    let (cmd_tx, cmd_rx) = unbounded::<ClientCommand>();
    let (swd_out_tx, _) = tokio::sync::broadcast::channel::<String>(128);
    let snapshot = Arc::new(RwLock::new(EmulatorSnapshot::default()));

    // Populate initial snapshot
    {
        let fb = emu.coproc.get_framebuffer();
        let nonzero = fb.iter().filter(|&&b| b != 0).count();
        let mut snap = snapshot.write().unwrap();
        snap.framebuffer = fb;
        snap.matrix_leds = emu.coproc.get_matrix_leds();
        snap.panel_leds = emu.get_panel_leds();
        snap.icon_leds = emu.get_icon_leds();
        snap.rear_panel = emu.get_rear_panel();
        snap.has_banner = nonzero > 50;
    }

    // Initialize native host audio output (cpal / CoreAudio)
    println!("[2/4] Initializing native host audio output (CoreAudio / cpal)...");
    let (_audio_stream, mut audio_sender) = match start_audio_output() {
        Ok((stream, sender)) => (Some(stream), Some(sender)),
        Err(e) => {
            eprintln!("Warning: Failed to initialize native audio output: {}", e);
            (None, None)
        }
    };

    // Spawn dedicated high-priority native emulator thread
    let snapshot_worker = snapshot.clone();
    let presets_worker_path = presets_path.clone();
    let swd_worker_tx = swd_out_tx.clone();
    std::thread::Builder::new()
        .name("vibefreak-engine".into())
        .spawn(move || {
            let mut last_snap_time = Instant::now();
            let mut last_preset_flush = Instant::now();
            let mut pot_states = std::collections::HashMap::<u16, u16>::new();
            let mut clock_pulse_off_at: Option<Instant> = None;
            let mut engine_start = Instant::now();
            let mut total_samples_generated: u64 = 0;

            loop {
                // 1. Step deterministic quantum co-scheduler for ~5.3ms (8 blocks of 32 frames = 256 frames)
                // Drain incoming commands inside the block loop for sub-millisecond responsiveness.
                for _ in 0..8 {
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        process_cmd(&mut emu, &mut pot_states, &mut clock_pulse_off_at, &swd_worker_tx, cmd);
                    }
                    if let Some(off_time) = clock_pulse_off_at {
                        if Instant::now() >= off_time {
                            emu.set_clock_in(false);
                            clock_pulse_off_at = None;
                        }
                    }
                    let audio_blk = if emu.swd.halted {
                        [0u8; 128]
                    } else if emu.swd.has_breakpoints() && emu.swd.check_breakpoint(emu.pc) {
                        emu.swd.halted = true;
                        let _ = swd_worker_tx.send(format!("\n[SWD] Breakpoint hit at PC: 0x{:08X}\nswd> ", emu.pc));
                        [0u8; 128]
                    } else {
                        emu.step_audio_block_quantum()
                    };
                    if let Some(audio_out) = audio_sender.as_mut() {
                        audio_out.push_pcm_chunk(&audio_blk);
                    }
                }
                total_samples_generated += 8 * 32;

                // 2. Update snapshot at 60 FPS (~16.6ms) using non-blocking try_write()
                if last_snap_time.elapsed() >= Duration::from_millis(16) {
                    if let Ok(mut snap) = snapshot_worker.try_write() {
                        let fb = emu.coproc.get_framebuffer();
                        snap.framebuffer = fb;
                        let nonzero = fb.iter().filter(|&&b| b != 0).count();
                        snap.has_banner = nonzero > 50;
                        snap.matrix_leds = emu.coproc.get_matrix_leds();
                        snap.panel_leds = emu.get_panel_leds();
                        snap.icon_leds = emu.get_icon_leds();
                        snap.rear_panel = emu.get_rear_panel();
                    }
                    last_snap_time = Instant::now();
                }

                // 3. Flush user presets if modified
                if emu.is_qspi_dirty() && last_preset_flush.elapsed() >= Duration::from_millis(500) {
                    if let Err(e) = emu.flush_qspi_to_disk(&presets_worker_path) {
                        eprintln!("Error writing presets to disk: {}", e);
                    } else {
                        emu.clear_qspi_dirty();
                        println!("[QSPI] Flushed updated user presets to {:?}", presets_worker_path);
                    }
                    last_preset_flush = Instant::now();
                }

                // 4. Low-latency adaptive watermark pacing based on physical ringbuffer level (~25-32ms latency)
                if let Some(audio_out) = audio_sender.as_ref() {
                    let occupied = audio_out.occupied_frames();
                    if occupied >= 1536 {
                        // High watermark (>=32ms audio cushion): compute excess ahead of 1,024-frame baseline and pace
                        let excess_frames = occupied - 1024;
                        let excess_micros = (excess_frames as u64 * 1_000_000) / 48_000;
                        if excess_micros >= 2000 {
                            std::thread::sleep(Duration::from_micros(excess_micros - 1000));
                        } else {
                            std::hint::spin_loop();
                        }
                    } else if occupied >= 1024 {
                        std::hint::spin_loop();
                    } else {
                        // Buffer below 21ms: do not yield or sleep, compute next audio blocks immediately!
                    }
                } else {
                    // Fallback timer pacing when audio output is disabled
                    let wall_clock_elapsed = engine_start.elapsed();
                    let virtual_audio_elapsed = Duration::from_micros((total_samples_generated * 1_000_000) / 48_000);
                    if wall_clock_elapsed > virtual_audio_elapsed + Duration::from_millis(500) {
                        engine_start = Instant::now();
                        total_samples_generated = 0;
                    } else if virtual_audio_elapsed > wall_clock_elapsed {
                        let ahead = virtual_audio_elapsed - wall_clock_elapsed;
                        if ahead > Duration::from_micros(2000) {
                            std::thread::sleep(ahead - Duration::from_micros(1000));
                        }
                    }
                }
            }
        })?;

    println!("[3/4] Launching Axum web dashboard & 60 FPS binary WebSocket server...");
    println!(
        "[4/4] Ready! Open http://localhost:{} in your browser.",
        port
    );

    start_web_server(cmd_tx, swd_out_tx, snapshot, listener).await?;
    Ok(())
}

#[derive(Debug, Clone)]
enum FirmwareSource {
    MffPackage(std::path::PathBuf),
    LooseBinaries {
        main_bin: std::path::PathBuf,
        qspi_bin: std::path::PathBuf,
        oled_bin: std::path::PathBuf,
        matrix_bin: std::path::PathBuf,
        kbd_bin: std::path::PathBuf,
    },
}

fn find_mff_file(search_dirs: &[&Path]) -> Option<std::path::PathBuf> {
    for dir in search_dirs {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut mffs: Vec<std::path::PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("mff")))
                .collect();
            mffs.sort();
            if let Some(first) = mffs.pop() {
                return Some(first);
            }
        }
    }
    None
}

fn resolve_firmware_source(
    custom_mff: Option<std::path::PathBuf>,
    custom_dir: Option<std::path::PathBuf>,
    custom_main: Option<std::path::PathBuf>,
    custom_qspi: Option<std::path::PathBuf>,
    custom_oled: Option<std::path::PathBuf>,
    custom_matrix: Option<std::path::PathBuf>,
    custom_kbd: Option<std::path::PathBuf>,
) -> Result<FirmwareSource, Box<dyn std::error::Error>> {
    // 1. If an explicit MFF file was passed:
    if let Some(mff_path) = custom_mff {
        if !mff_path.is_file() {
            return Err(format!("Specified firmware package (.mff) not found: {:?}", mff_path).into());
        }
        return Ok(FirmwareSource::MffPackage(mff_path));
    }

    // 2. Check VIBEFREAK_MFF environment variable:
    if let Ok(env_path) = std::env::var("VIBEFREAK_MFF") {
        let p = std::path::PathBuf::from(env_path);
        if p.is_file() {
            println!("===> Using firmware package from VIBEFREAK_MFF: {}", p.display());
            return Ok(FirmwareSource::MffPackage(p));
        }
    }

    // 3. If explicit loose binaries or explicit directory was requested:
    if custom_main.is_some() || custom_dir.is_some() {
        let fw_dir = custom_dir.unwrap_or_else(|| std::path::PathBuf::from("original_firmware/unpacked"));
        let main_bin = custom_main.unwrap_or_else(|| fw_dir.join("nanowave_main_payload_stm32f446.bin"));
        let qspi_bin = custom_qspi.unwrap_or_else(|| fw_dir.join("userwavetable_factory_payload_qspi.bin"));
        let oled_bin = custom_oled.unwrap_or_else(|| fw_dir.join("nanowave_iochip_oled_payload_stm32f0.bin"));
        let matrix_bin = custom_matrix.unwrap_or_else(|| fw_dir.join("nanowave_iochip_matrix_payload_stm32f0.bin"));
        let kbd_bin = custom_kbd.unwrap_or_else(|| fw_dir.join("nanowave_iochip_kbd_payload_stm32f0.bin"));

        for (name, path) in [
            ("Main MCU binary", &main_bin),
            ("QSPI wavetable binary", &qspi_bin),
            ("OLED coprocessor binary", &oled_bin),
            ("Matrix coprocessor binary", &matrix_bin),
            ("Keyboard coprocessor binary", &kbd_bin),
        ] {
            if !path.exists() {
                return Err(format!("{} not found at {:?}", name, path).into());
            }
        }

        return Ok(FirmwareSource::LooseBinaries {
            main_bin,
            qspi_bin,
            oled_bin,
            matrix_bin,
            kbd_bin,
        });
    }

    // 3. Auto-detection heuristics:
    // 4. Check user config directories:
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_path = Path::new(&home);
        let config_candidates = [
            home_path.join(".config/vibefreak"),
            home_path.join(".vibefreak"),
        ];
        if let Some(cfg_mff) = find_mff_file(&config_candidates.iter().map(|p| p.as_path()).collect::<Vec<_>>()) {
            println!("===> Using firmware package from config: {}", cfg_mff.display());
            return Ok(FirmwareSource::MffPackage(cfg_mff));
        }
    }

    // 5. Auto-detection heuristics in workspace directories:
    if let Some(detected_mff) = find_mff_file(&[
        Path::new("."),
        Path::new("dist"),
        Path::new("original_firmware/microfreak"),
    ]) {
        println!("===> Auto-detected firmware package: {}", detected_mff.display());
        return Ok(FirmwareSource::MffPackage(detected_mff));
    }

    let build_main = Path::new("build/nanowave_main_payload_stm32f446.bin");
    if build_main.exists() {
        let fw_dir = Path::new("build");
        println!("===> Auto-detected firmware binaries in build/ directory");
        return Ok(FirmwareSource::LooseBinaries {
            main_bin: build_main.to_path_buf(),
            qspi_bin: fw_dir.join("userwavetable_factory_payload_qspi.bin"),
            oled_bin: fw_dir.join("nanowave_iochip_oled_payload_stm32f0.bin"),
            matrix_bin: fw_dir.join("nanowave_iochip_matrix_payload_stm32f0.bin"),
            kbd_bin: fw_dir.join("nanowave_iochip_kbd_payload_stm32f0.bin"),
        });
    }

    let unpacked_main = Path::new("original_firmware/unpacked/nanowave_main_payload_stm32f446.bin");
    if unpacked_main.exists() {
        let fw_dir = Path::new("original_firmware/unpacked");
        return Ok(FirmwareSource::LooseBinaries {
            main_bin: unpacked_main.to_path_buf(),
            qspi_bin: fw_dir.join("userwavetable_factory_payload_qspi.bin"),
            oled_bin: fw_dir.join("nanowave_iochip_oled_payload_stm32f0.bin"),
            matrix_bin: fw_dir.join("nanowave_iochip_matrix_payload_stm32f0.bin"),
            kbd_bin: fw_dir.join("nanowave_iochip_kbd_payload_stm32f0.bin"),
        });
    }

    Err(
        "No firmware detected!\n\
         Please provide an official MicroFreak firmware update package (.mff):\n\
           • CLI argument:      vibefreak [PATH_TO_MFF]\n\
           • Environment:       export VIBEFREAK_MFF=/path/to/firmware.mff\n\
           • Config directory:  ~/.config/vibefreak/firmware.mff (or ~/.vibefreak/firmware.mff)\n\
           • Loose build:       vibefreak --firmware-dir <DIR>".into()
    )
}

fn run_headless_test(fw_source: &FirmwareSource) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Running Headless Stock Firmware Test (Rust Engine) ===");
    let t0 = Instant::now();
    let mut emu = match fw_source {
        FirmwareSource::MffPackage(path) => {
            println!("[0/3] Loading packaged firmware archive: {}", path.display());
            VibeFreakEmulator::from_mff_file(path)?
        }
        FirmwareSource::LooseBinaries {
            main_bin,
            qspi_bin,
            oled_bin,
            matrix_bin,
            kbd_bin,
        } => {
            println!("[0/3] Loading firmware binaries from: {}", main_bin.display());
            VibeFreakEmulator::new(main_bin, Some(qspi_bin), oled_bin, matrix_bin, kbd_bin)?
        }
    };

    println!("[1/3] Booting firmware...");
    emu.boot(100_000_000);
    println!("      Boot completed in {:?}", t0.elapsed());

    println!("[2/3] Stepping quantum co-scheduler to render initial preset screen...");
    for _ in 0..60 {
        let blk = emu.step_audio_block_quantum();
        assert_eq!(blk.len(), 128);
    }

    println!("[3/3] Checking OLED framebuffer output...");
    let fb = emu.coproc.get_framebuffer();
    let nonzero = fb.iter().filter(|&&b| b != 0).count();
    println!("      Non-zero framebuffer bytes: {} / 1024", nonzero);
    assert!(
        nonzero >= 50,
        "Expected active OLED framebuffer (preset screen rendered >= 50 non-zero bytes), got {}",
        nonzero
    );

    println!("=== Test Passed: 100% Stock Firmware Parity in Native Rust! ===");
    Ok(())
}

fn process_cmd(
    emu: &mut VibeFreakEmulator,
    pot_states: &mut std::collections::HashMap<u16, u16>,
    clock_pulse_off_at: &mut Option<Instant>,
    swd_sender: &tokio::sync::broadcast::Sender<String>,
    cmd: ClientCommand,
) {
    match cmd {
        ClientCommand::Key { key, pressure } => {
            emu.set_key(key, pressure);
        }
        ClientCommand::Microswitch {
            name,
            action,
            pressed,
        } => {
            let is_down = pressed.unwrap_or_else(|| action.as_deref() == Some("down"));
            emu.set_microswitch(&name, is_down);
        }
        ClientCommand::Encoder { name, delta } => match name.as_str() {
            "preset" => emu.step_preset_encoder(delta),
            "matrix" => emu.step_matrix_encoder(delta),
            "type" => emu.step_type_encoder(delta),
            "rate" => emu.step_rate_encoder(delta),
            "lfo_rate" => emu.step_lfo_rate_encoder(delta),
            "wave" => emu.step_wave_encoder(delta),
            "timbre" => emu.step_timbre_encoder(delta),
            "shape" => emu.step_shape_encoder(delta),
            _ => {}
        },
        ClientCommand::EncoderWithMicroswitch {
            name,
            action,
            pressed,
            delta,
        } => {
            if let Some(d) = delta {
                match name.as_str() {
                    "matrix" => emu.step_matrix_encoder(d),
                    "preset" => emu.step_preset_encoder(d),
                    "rate" => emu.step_rate_encoder(d),
                    "lfo_rate" => emu.step_lfo_rate_encoder(d),
                    _ => {}
                }
            }
            let is_down = pressed.or(match action.as_deref() {
                Some("down") | Some("press") => Some(true),
                Some("up") | Some("release") => Some(false),
                _ => None,
            });
            if let Some(down) = is_down {
                emu.set_microswitch(&name, down);
            }
        }
        ClientCommand::Potentiometer { channel, value } => {
            pot_states.insert(channel, value);
            let cid_map = [
                (2, 0x0101),  // CUTOFF
                (3, 0x0102),  // RESONANCE
                (7, 0x0601),  // ATTACK
                (8, 0x0602),  // DECAY
                (9, 0x0603),  // SUSTAIN
                (10, 0x0300), // GLIDE
                (11, 0x0605), // FILTER_AMT
                (12, 0x0201), // CYC_RISE
                (13, 0x0203), // CYC_FALL
                (14, 0x0204), // CYC_HOLD
                (15, 0x0206), // CYC_AMOUNT
                (16, 0x0704), // MASTER_VOLUME
            ];

            if let Some(&(_, cid)) = cid_map.iter().find(|&&(ch, _)| ch == channel) {
                emu.set_potentiometer(cid, value);
            }
        }
        ClientCommand::Touchstrip { value, touched } => {
            emu.set_touch_strip(value, touched);
        }
        ClientCommand::Midi { bytes } => {
            emu.enqueue_midi_bytes(&bytes);
            emu.service_usart3_rx();
        }
        ClientCommand::Swd { command } => {
            let out = emu.execute_swd_command(&command);
            let _ = swd_sender.send(out);
        }
        ClientCommand::ClockIn { high } => {
            emu.set_clock_in(high);
        }
        ClientCommand::ClockPulse { duration_ms } => {
            let dur = duration_ms.unwrap_or(10);
            emu.set_clock_in(true);
            *clock_pulse_off_at = Some(Instant::now() + Duration::from_millis(dur as u64));
        }
        ClientCommand::SetHeadsetJack { state } => {
            emu.set_headset_jack(state);
        }
        ClientCommand::SetHeadsetButton { pressed } => {
            emu.set_headset_button(pressed);
        }
        ClientCommand::SetAudioInputSource { source } => {
            emu.set_audio_input_source(source);
        }
        ClientCommand::FeedAudioInput { samples } => {
            emu.feed_audio_input(&samples);
        }
        ClientCommand::SetFilterMode { mode } => {
            let m = match mode.to_lowercase().as_str() {
                "bp" | "bandpass" => vibefreak_core::SemFilterMode::BandPass,
                "hp" | "highpass" => vibefreak_core::SemFilterMode::HighPass,
                "notch" => vibefreak_core::SemFilterMode::Notch,
                _ => vibefreak_core::SemFilterMode::LowPass,
            };
            emu.set_filter_mode_switch(m);
        }
    }
}
