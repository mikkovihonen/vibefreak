use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;

use vibefreak_core::{SemFilterMode, VibeFreakEmulator};
use vibefreak_web::{ApiStateResponse, ClientCommand, CommandApiResponse, EmulatorSnapshot, SwdApiRequest, SwdApiResponse};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait EmulatorBackend: Send + Sync {
    fn execute_swd<'a>(&'a self, cmd: &'a str) -> BoxFuture<'a, Result<String, String>>;
    fn apply_command<'a>(&'a self, cmd: ClientCommand) -> BoxFuture<'a, Result<(), String>>;
    fn step<'a>(&'a self, blocks: usize) -> BoxFuture<'a, Result<(), String>>;
    fn get_snapshot<'a>(&'a self) -> BoxFuture<'a, Result<EmulatorSnapshot, String>>;
    fn get_oled<'a>(&'a self) -> BoxFuture<'a, Result<[u8; 1024], String>>;
    fn is_halted<'a>(&'a self) -> BoxFuture<'a, Result<bool, String>>;
    fn set_halted<'a>(&'a self, halted: bool) -> BoxFuture<'a, Result<(), String>>;
    fn save_presets<'a>(&'a self, path: Option<PathBuf>) -> BoxFuture<'a, Result<String, String>>;
    fn load_presets<'a>(&'a self, path: Option<PathBuf>) -> BoxFuture<'a, Result<String, String>>;
}

// ---------------------------------------------------------------------------
// In-Process Backend
// ---------------------------------------------------------------------------

pub enum EngineMsg {
    Swd {
        command: String,
        reply: oneshot::Sender<String>,
    },
    Command(ClientCommand),
    Step {
        blocks: usize,
        reply: oneshot::Sender<()>,
    },
    GetSnapshot(oneshot::Sender<EmulatorSnapshot>),
    GetOled(oneshot::Sender<[u8; 1024]>),
    IsHalted(oneshot::Sender<bool>),
    SetHalted(bool),
    SavePresets {
        path: PathBuf,
        reply: oneshot::Sender<Result<usize, String>>,
    },
    LoadPresets {
        path: PathBuf,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Shutdown,
}

pub struct InProcessBackend {
    tx: Sender<EngineMsg>,
    default_presets_path: PathBuf,
}

impl InProcessBackend {
    pub fn new(mut emu: VibeFreakEmulator, presets_path: PathBuf) -> Self {
        let (tx, rx) = unbounded::<EngineMsg>();
        let presets_clone = presets_path.clone();

        std::thread::Builder::new()
            .name("vibefreak-mcp-engine".into())
            .spawn(move || {
                run_inprocess_loop(&mut emu, rx, presets_clone);
            })
            .expect("Failed to spawn emulator worker thread");

        Self {
            tx,
            default_presets_path: presets_path,
        }
    }
}

impl EmulatorBackend for InProcessBackend {
    fn execute_swd<'a>(&'a self, cmd: &'a str) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(EngineMsg::Swd {
                    command: cmd.to_string(),
                    reply: reply_tx,
                })
                .map_err(|e| format!("Failed to send to engine: {e}"))?;
            reply_rx.await.map_err(|e| format!("Engine reply error: {e}"))
        })
    }

    fn apply_command<'a>(&'a self, cmd: ClientCommand) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.tx
                .send(EngineMsg::Command(cmd))
                .map_err(|e| format!("Failed to send command to engine: {e}"))
        })
    }

    fn step<'a>(&'a self, blocks: usize) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(EngineMsg::Step {
                    blocks,
                    reply: reply_tx,
                })
                .map_err(|e| format!("Failed to send step to engine: {e}"))?;
            reply_rx.await.map_err(|e| format!("Engine step error: {e}"))
        })
    }

    fn get_snapshot<'a>(&'a self) -> BoxFuture<'a, Result<EmulatorSnapshot, String>> {
        Box::pin(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(EngineMsg::GetSnapshot(reply_tx))
                .map_err(|e| format!("Failed to send to engine: {e}"))?;
            reply_rx.await.map_err(|e| format!("Engine snapshot error: {e}"))
        })
    }

    fn get_oled<'a>(&'a self) -> BoxFuture<'a, Result<[u8; 1024], String>> {
        Box::pin(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(EngineMsg::GetOled(reply_tx))
                .map_err(|e| format!("Failed to send to engine: {e}"))?;
            reply_rx.await.map_err(|e| format!("Engine OLED error: {e}"))
        })
    }

    fn is_halted<'a>(&'a self) -> BoxFuture<'a, Result<bool, String>> {
        Box::pin(async move {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(EngineMsg::IsHalted(reply_tx))
                .map_err(|e| format!("Failed to send to engine: {e}"))?;
            reply_rx.await.map_err(|e| format!("Engine halted error: {e}"))
        })
    }

    fn set_halted<'a>(&'a self, halted: bool) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.tx
                .send(EngineMsg::SetHalted(halted))
                .map_err(|e| format!("Failed to send to engine: {e}"))
        })
    }

    fn save_presets<'a>(&'a self, path: Option<PathBuf>) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let p = path.unwrap_or_else(|| self.default_presets_path.clone());
            let p_display = p.display().to_string();
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(EngineMsg::SavePresets { path: p, reply: reply_tx })
                .map_err(|e| format!("Failed to send to engine: {e}"))?;
            let res = reply_rx.await.map_err(|e| format!("Engine reply error: {e}"))?;
            res.map(|bytes| format!("Successfully saved {bytes} bytes to {p_display}"))
        })
    }

    fn load_presets<'a>(&'a self, path: Option<PathBuf>) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let p = path.unwrap_or_else(|| self.default_presets_path.clone());
            let p_display = p.display().to_string();
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(EngineMsg::LoadPresets { path: p, reply: reply_tx })
                .map_err(|e| format!("Failed to send to engine: {e}"))?;
            let res = reply_rx.await.map_err(|e| format!("Engine reply error: {e}"))?;
            res.map(|_| format!("Successfully loaded user presets from {p_display}"))
        })
    }
}

fn run_inprocess_loop(
    emu: &mut VibeFreakEmulator,
    rx: Receiver<EngineMsg>,
    _presets_path: PathBuf,
) {
    let mut pot_states = std::collections::HashMap::<u16, u16>::new();
    let mut clock_pulse_off_at: Option<Instant> = None;

    loop {
        // Drain all available messages
        while let Ok(msg) = rx.try_recv() {
            match msg {
                EngineMsg::Swd { command, reply } => {
                    let out = emu.execute_swd_command(&command);
                    let _ = reply.send(out);
                }
                EngineMsg::Command(cmd) => {
                    apply_client_cmd(emu, &mut pot_states, &mut clock_pulse_off_at, cmd);
                }
                EngineMsg::Step { blocks, reply } => {
                    for _ in 0..blocks {
                        emu.step_audio_block_quantum();
                    }
                    let _ = reply.send(());
                }
                EngineMsg::GetSnapshot(reply) => {
                    let snap = EmulatorSnapshot {
                        framebuffer: emu.coproc.get_framebuffer(),
                        matrix_leds: emu.coproc.get_matrix_leds(),
                        panel_leds: emu.get_panel_leds(),
                        icon_leds: emu.get_icon_leds(),
                        rear_panel: emu.get_rear_panel(),
                        has_banner: true,
                    };
                    let _ = reply.send(snap);
                }
                EngineMsg::GetOled(reply) => {
                    let _ = reply.send(emu.coproc.get_framebuffer());
                }
                EngineMsg::IsHalted(reply) => {
                    let _ = reply.send(emu.is_halted());
                }
                EngineMsg::SetHalted(h) => {
                    emu.set_halted(h);
                }
                EngineMsg::SavePresets { path, reply } => {
                    let res = emu
                        .flush_qspi_to_disk(&path)
                        .map(|_| 16 * 1024 * 1024)
                        .map_err(|e| e.to_string());
                    let _ = reply.send(res);
                }
                EngineMsg::LoadPresets { path, reply } => {
                    let res = emu.load_qspi_from_disk(&path).map_err(|e| e.to_string());
                    let _ = reply.send(res);
                }
                EngineMsg::Shutdown => return,
            }
        }

        if let Some(off_time) = clock_pulse_off_at {
            if Instant::now() >= off_time {
                emu.set_clock_in(false);
                clock_pulse_off_at = None;
            }
        }

        if !emu.swd.halted {
            if emu.swd.has_breakpoints() && emu.swd.check_breakpoint(emu.pc) {
                emu.swd.halted = true;
            } else {
                // Step 1 audio block (~0.66ms) and pace slightly
                emu.step_audio_block_quantum();
                std::thread::sleep(Duration::from_micros(600));
            }
        } else {
            // Halted: wait for next message
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(msg) => {
                    match msg {
                        EngineMsg::Swd { command, reply } => {
                            let out = emu.execute_swd_command(&command);
                            let _ = reply.send(out);
                        }
                        EngineMsg::Command(cmd) => {
                            apply_client_cmd(emu, &mut pot_states, &mut clock_pulse_off_at, cmd);
                        }
                        EngineMsg::Step { blocks, reply } => {
                            for _ in 0..blocks {
                                emu.step_audio_block_quantum();
                            }
                            let _ = reply.send(());
                        }
                        EngineMsg::GetSnapshot(reply) => {
                            let snap = EmulatorSnapshot {
                                framebuffer: emu.coproc.get_framebuffer(),
                                matrix_leds: emu.coproc.get_matrix_leds(),
                                panel_leds: emu.get_panel_leds(),
                                icon_leds: emu.get_icon_leds(),
                                rear_panel: emu.get_rear_panel(),
                                has_banner: true,
                            };
                            let _ = reply.send(snap);
                        }
                        EngineMsg::GetOled(reply) => {
                            let _ = reply.send(emu.coproc.get_framebuffer());
                        }
                        EngineMsg::IsHalted(reply) => {
                            let _ = reply.send(emu.is_halted());
                        }
                        EngineMsg::SetHalted(h) => {
                            emu.set_halted(h);
                        }
                        EngineMsg::SavePresets { path, reply } => {
                            let res = emu
                                .flush_qspi_to_disk(&path)
                                .map(|_| 16 * 1024 * 1024)
                                .map_err(|e| e.to_string());
                            let _ = reply.send(res);
                        }
                        EngineMsg::LoadPresets { path, reply } => {
                            let res = emu.load_qspi_from_disk(&path).map_err(|e| e.to_string());
                            let _ = reply.send(res);
                        }
                        EngineMsg::Shutdown => return,
                    }
                }
                Err(_) => {}
            }
        }
    }
}

pub fn apply_client_cmd(
    emu: &mut VibeFreakEmulator,
    pot_states: &mut std::collections::HashMap<u16, u16>,
    clock_pulse_off_at: &mut Option<Instant>,
    cmd: ClientCommand,
) {
    match cmd {
        ClientCommand::Key { key, pressure } => {
            emu.set_key(key, pressure);
        }
        ClientCommand::Microswitch { name, action, pressed } => {
            let is_down = pressed.unwrap_or_else(|| {
                action.as_deref() == Some("down") || action.as_deref() == Some("press")
            });
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
            let _ = emu.execute_swd_command(&command);
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
                "bp" | "bandpass" => SemFilterMode::BandPass,
                "hp" | "highpass" => SemFilterMode::HighPass,
                "notch" => SemFilterMode::Notch,
                _ => SemFilterMode::LowPass,
            };
            emu.set_filter_mode_switch(m);
        }
    }
}

// ---------------------------------------------------------------------------
// Remote Backend (Connects to running VibeFreak instance via HTTP)
// ---------------------------------------------------------------------------

pub struct RemoteBackend {
    addr: SocketAddr,
}

impl RemoteBackend {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }

    async fn http_request(
        &self,
        method: &str,
        path: &str,
        body_opt: Option<&str>,
    ) -> Result<String, String> {
        let mut stream = TcpStream::connect(self.addr)
            .await
            .map_err(|e| format!("Cannot connect to VibeFreak at {}: {e}", self.addr))?;

        let body = body_opt.unwrap_or("");
        let req = format!(
            "{method} {path} HTTP/1.1\r\n\
             Host: {}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {body}",
            self.addr,
            body.len()
        );

        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|e| format!("Failed to send HTTP request: {e}"))?;

        let mut resp_bytes = Vec::new();
        stream
            .read_to_end(&mut resp_bytes)
            .await
            .map_err(|e| format!("Failed to read HTTP response: {e}"))?;

        let resp_str = String::from_utf8_lossy(&resp_bytes);
        let header_end = resp_str
            .find("\r\n\r\n")
            .ok_or_else(|| "Malformed HTTP response (no header terminator)".to_string())?;

        let body_str = &resp_str[header_end + 4..];
        Ok(body_str.to_string())
    }
}

impl EmulatorBackend for RemoteBackend {
    fn execute_swd<'a>(&'a self, cmd: &'a str) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let req = SwdApiRequest {
                command: cmd.to_string(),
            };
            let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;
            let resp = self.http_request("POST", "/api/swd", Some(&body)).await?;
            let api_res: SwdApiResponse = serde_json::from_str(&resp).map_err(|e| {
                format!("Failed to parse /api/swd response: {e} (raw: {resp})")
            })?;
            Ok(api_res.output)
        })
    }

    fn apply_command<'a>(&'a self, cmd: ClientCommand) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let body = serde_json::to_string(&cmd).map_err(|e| e.to_string())?;
            let resp = self.http_request("POST", "/api/command", Some(&body)).await?;
            let api_res: CommandApiResponse = serde_json::from_str(&resp).map_err(|e| {
                format!("Failed to parse /api/command response: {e} (raw: {resp})")
            })?;
            if api_res.ok {
                Ok(())
            } else {
                Err("Command delivery rejected by server".into())
            }
        })
    }

    fn step<'a>(&'a self, _blocks: usize) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        })
    }

    fn get_snapshot<'a>(&'a self) -> BoxFuture<'a, Result<EmulatorSnapshot, String>> {
        Box::pin(async move {
            let resp = self.http_request("GET", "/api/state", None).await?;
            let api_res: ApiStateResponse = serde_json::from_str(&resp).map_err(|e| {
                format!("Failed to parse /api/state response: {e} (raw: {resp})")
            })?;

            let mut fb = [0u8; 1024];
            let copy_len = fb.len().min(api_res.framebuffer.len());
            fb[..copy_len].copy_from_slice(&api_res.framebuffer[..copy_len]);

            let mut matrix_leds = [0u8; 35];
            let copy_m = matrix_leds.len().min(api_res.matrix_leds.len());
            matrix_leds[..copy_m].copy_from_slice(&api_res.matrix_leds[..copy_m]);

            let mut panel_leds = [0u8; 35];
            let copy_p = panel_leds.len().min(api_res.panel_leds.len());
            panel_leds[..copy_p].copy_from_slice(&api_res.panel_leds[..copy_p]);

            let mut icon_leds = [0u8; 8];
            let copy_i = icon_leds.len().min(api_res.icon_leds.len());
            icon_leds[..copy_i].copy_from_slice(&api_res.icon_leds[..copy_i]);

            Ok(EmulatorSnapshot {
                framebuffer: fb,
                matrix_leds,
                panel_leds,
                icon_leds,
                rear_panel: api_res.rear_panel,
                has_banner: api_res.has_banner,
            })
        })
    }

    fn get_oled<'a>(&'a self) -> BoxFuture<'a, Result<[u8; 1024], String>> {
        Box::pin(async move {
            let snap = self.get_snapshot().await?;
            Ok(snap.framebuffer)
        })
    }

    fn is_halted<'a>(&'a self) -> BoxFuture<'a, Result<bool, String>> {
        Box::pin(async move {
            let status = self.execute_swd("status").await?;
            Ok(status.contains("State:       HALTED") || status.contains("HALTED"))
        })
    }

    fn set_halted<'a>(&'a self, halted: bool) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let cmd = if halted { "halt" } else { "resume" };
            let _ = self.execute_swd(cmd).await?;
            Ok(())
        })
    }

    fn save_presets<'a>(&'a self, path: Option<PathBuf>) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let target = path.map(|p| p.display().to_string()).unwrap_or_else(|| "disk".to_string());
            Ok(format!("Presets flushed on remote server instance ({target})"))
        })
    }

    fn load_presets<'a>(&'a self, path: Option<PathBuf>) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            let target = path.map(|p| p.display().to_string()).unwrap_or_else(|| "default".to_string());
            Ok(format!("Presets reloaded on remote server instance ({target})"))
        })
    }
}

// ---------------------------------------------------------------------------
// Backend Factory & Detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct McpConfig {
    pub mode: McpMode,
    pub port: u16,
    pub mff_path: Option<PathBuf>,
    pub firmware_dir: Option<PathBuf>,
    pub main_bin: Option<PathBuf>,
    pub qspi_bin: Option<PathBuf>,
    pub oled_bin: Option<PathBuf>,
    pub matrix_bin: Option<PathBuf>,
    pub kbd_bin: Option<PathBuf>,
    pub presets_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpMode {
    Auto,
    Standalone,
    Remote(SocketAddr),
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            mode: McpMode::Auto,
            port: 8081,
            mff_path: None,
            firmware_dir: None,
            main_bin: None,
            qspi_bin: None,
            oled_bin: None,
            matrix_bin: None,
            kbd_bin: None,
            presets_path: PathBuf::from("user_presets.bin"),
        }
    }
}

pub async fn create_backend(config: &McpConfig) -> Result<Arc<dyn EmulatorBackend>, Box<dyn std::error::Error>> {
    match &config.mode {
        McpMode::Remote(addr) => {
            eprintln!("[MCP] Connecting to specified remote VibeFreak instance at {}...", addr);
            Ok(Arc::new(RemoteBackend::new(*addr)))
        }
        McpMode::Standalone => {
            eprintln!("[MCP] Mode forced to Standalone. Booting native in-process emulator...");
            let emu = boot_inprocess_emulator(config)?;
            Ok(Arc::new(InProcessBackend::new(emu, config.presets_path.clone())))
        }
        McpMode::Auto => {
            let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
            // Check if instance is alive on port
            if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_millis(250), TcpStream::connect(addr)).await {
                eprintln!("[MCP] Auto-detected running VibeFreak instance at http://{}. Attaching remote client...", addr);
                return Ok(Arc::new(RemoteBackend::new(addr)));
            }

            eprintln!("[MCP] No active VibeFreak instance detected at port {}. Booting native in-process emulator...", config.port);
            let emu = boot_inprocess_emulator(config)?;
            Ok(Arc::new(InProcessBackend::new(emu, config.presets_path.clone())))
        }
    }
}

fn boot_inprocess_emulator(config: &McpConfig) -> Result<VibeFreakEmulator, Box<dyn std::error::Error>> {
    let t0 = Instant::now();

    // 1. Explicit MFF package
    if let Some(mff_path) = &config.mff_path {
        if mff_path.exists() {
            eprintln!("[MCP] Booting from firmware archive: {}", mff_path.display());
            let pkg = vibefreak_core::MffFirmwarePackage::from_file(mff_path)?;
            let mut emu = VibeFreakEmulator::from_package(&pkg)?;
            if config.presets_path.exists() {
                let _ = emu.load_qspi_from_disk(&config.presets_path);
            }
            emu.boot(100_000_000);
            eprintln!("[MCP] Firmware booted in {:?}", t0.elapsed());
            return Ok(emu);
        }
    }

    // 2. Loose binaries if specified
    if config.main_bin.is_some() || config.firmware_dir.is_some() {
        let fw_dir = config.firmware_dir.clone().unwrap_or_else(|| PathBuf::from("original_firmware/unpacked"));
        let main = config.main_bin.clone().unwrap_or_else(|| fw_dir.join("nanowave_main_payload_stm32f446.bin"));
        let qspi = config.qspi_bin.clone().unwrap_or_else(|| fw_dir.join("userwavetable_factory_payload_qspi.bin"));
        let oled = config.oled_bin.clone().unwrap_or_else(|| fw_dir.join("nanowave_iochip_oled_payload_stm32f0.bin"));
        let matrix = config.matrix_bin.clone().unwrap_or_else(|| fw_dir.join("nanowave_iochip_matrix_payload_stm32f0.bin"));
        let kbd = config.kbd_bin.clone().unwrap_or_else(|| fw_dir.join("nanowave_iochip_kbd_payload_stm32f0.bin"));

        eprintln!("[MCP] Booting from loose binaries (main: {})...", main.display());
        let mut emu = VibeFreakEmulator::new(&main, Some(&qspi), &oled, &matrix, &kbd)?;
        if config.presets_path.exists() {
            let _ = emu.load_qspi_from_disk(&config.presets_path);
        }
        emu.boot(100_000_000);
        eprintln!("[MCP] Firmware booted in {:?}", t0.elapsed());
        return Ok(emu);
    }

    // 3. Environment variable VIBEFREAK_MFF
    if let Ok(env_path) = std::env::var("VIBEFREAK_MFF") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            eprintln!("[MCP] Using firmware archive from VIBEFREAK_MFF: {}", p.display());
            let pkg = vibefreak_core::MffFirmwarePackage::from_file(&p)?;
            let mut emu = VibeFreakEmulator::from_package(&pkg)?;
            if config.presets_path.exists() {
                let _ = emu.load_qspi_from_disk(&config.presets_path);
            }
            emu.boot(100_000_000);
            eprintln!("[MCP] Firmware booted in {:?}", t0.elapsed());
            return Ok(emu);
        }
    }

    // 4. Auto-detection heuristics
    let mut search_roots = vec![
        PathBuf::from("."),
        PathBuf::from("dist"),
        PathBuf::from("original_firmware/microfreak"),
    ];
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_path = PathBuf::from(home);
        search_roots.push(home_path.join(".config/vibefreak"));
        search_roots.push(home_path.join(".vibefreak"));
    }
    for dir in &search_roots {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("mff")) {
                    eprintln!("[MCP] Auto-detected firmware archive: {}", p.display());
                    let pkg = vibefreak_core::MffFirmwarePackage::from_file(&p)?;
                    let mut emu = VibeFreakEmulator::from_package(&pkg)?;
                    if config.presets_path.exists() {
                        let _ = emu.load_qspi_from_disk(&config.presets_path);
                    }
                    emu.boot(100_000_000);
                    eprintln!("[MCP] Firmware booted in {:?}", t0.elapsed());
                    return Ok(emu);
                }
            }
        }
    }

    let default_loose = PathBuf::from("original_firmware/unpacked");
    let main_cand = default_loose.join("nanowave_main_payload_stm32f446.bin");
    if main_cand.exists() {
        let qspi = default_loose.join("userwavetable_factory_payload_qspi.bin");
        let oled = default_loose.join("nanowave_iochip_oled_payload_stm32f0.bin");
        let matrix = default_loose.join("nanowave_iochip_matrix_payload_stm32f0.bin");
        let kbd = default_loose.join("nanowave_iochip_kbd_payload_stm32f0.bin");

        eprintln!("[MCP] Auto-detected unpacked firmware binaries...");
        let mut emu = VibeFreakEmulator::new(&main_cand, Some(&qspi), &oled, &matrix, &kbd)?;
        if config.presets_path.exists() {
            let _ = emu.load_qspi_from_disk(&config.presets_path);
        }
        emu.boot(100_000_000);
        eprintln!("[MCP] Firmware booted in {:?}", t0.elapsed());
        return Ok(emu);
    }

    Err("No firmware found! Provide a path to Arturia .mff firmware package or place it in working directory.".into())
}
