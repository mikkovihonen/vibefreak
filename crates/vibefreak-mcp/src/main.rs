use std::net::SocketAddr;
use std::path::PathBuf;
use vibefreak_mcp::{run_mcp_server, McpConfig, McpMode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In MCP mode, stdout is reserved exclusively for JSON-RPC messages.
    // Logging MUST go to stderr.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("VibeFreak Model Context Protocol (MCP) Server");
        eprintln!();
        eprintln!("USAGE:");
        eprintln!("    vibefreak-mcp [OPTIONS] [PATH_TO_MFF]");
        eprintln!();
        eprintln!("ARGUMENTS:");
        eprintln!("    [PATH_TO_MFF]            Path to Arturia .mff firmware package");
        eprintln!();
        eprintln!("OPTIONS:");
        eprintln!("    --standalone             Force standalone in-process emulator");
        eprintln!("    --connect <HOST:PORT>    Connect to existing VibeFreak instance");
        eprintln!("    -p, --port <PORT>        Target port for auto-detection / connect (default: 8081)");
        eprintln!("    --presets <PATH>         User preset persistence file (default: user_presets.bin)");
        eprintln!("    --firmware-dir <DIR>     Directory containing loose firmware binaries");
        eprintln!("    --main-bin <PATH>        Path to Main MCU binary (STM32F446)");
        eprintln!("    --qspi-bin <PATH>        Path to QSPI wavetable binary");
        eprintln!("    --oled-bin <PATH>        Path to OLED coprocessor binary (STM32F0)");
        eprintln!("    --matrix-bin <PATH>      Path to Matrix coprocessor binary (STM32F0)");
        eprintln!("    --kbd-bin <PATH>         Path to Keyboard coprocessor binary (STM32F0)");
        eprintln!("    -h, --help               Print this help message");
        return Ok(());
    }

    let mut config = McpConfig::default();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "mcp" => {
                // If invoked as `vibefreak mcp`, skip the 'mcp' subcommand token
                i += 1;
                continue;
            }
            "--standalone" => {
                config.mode = McpMode::Standalone;
            }
            "--connect" if i + 1 < args.len() => {
                let target = &args[i + 1];
                let addr: SocketAddr = if let Ok(sa) = target.parse() {
                    sa
                } else if let Ok(p) = target.parse::<u16>() {
                    SocketAddr::from(([127, 0, 0, 1], p))
                } else {
                    SocketAddr::from(([127, 0, 0, 1], 8081))
                };
                config.mode = McpMode::Remote(addr);
                i += 1;
            }
            "-p" | "--port" if i + 1 < args.len() => {
                if let Ok(p) = args[i + 1].parse::<u16>() {
                    config.port = p;
                }
                i += 1;
            }
            "--presets" if i + 1 < args.len() => {
                config.presets_path = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--mff" if i + 1 < args.len() => {
                config.mff_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--firmware-dir" if i + 1 < args.len() => {
                config.firmware_dir = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--main-bin" if i + 1 < args.len() => {
                config.main_bin = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--qspi-bin" if i + 1 < args.len() => {
                config.qspi_bin = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--oled-bin" if i + 1 < args.len() => {
                config.oled_bin = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--matrix-bin" if i + 1 < args.len() => {
                config.matrix_bin = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--kbd-bin" if i + 1 < args.len() => {
                config.kbd_bin = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            s if !s.starts_with('-') => {
                let p = PathBuf::from(s);
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

    run_mcp_server(config).await
}
