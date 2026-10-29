use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, Json, Response},
    routing::get,
    Router,
};
use crossbeam_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use vibefreak_core::RearPanelState;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct EmulatorSnapshot {
    pub framebuffer: [u8; 1024],
    pub matrix_leds: [u8; 35],
    pub panel_leds: [u8; 35],
    pub icon_leds: [u8; 8],
    pub rear_panel: RearPanelState,
    pub has_banner: bool,
}

impl Default for EmulatorSnapshot {
    fn default() -> Self {
        Self {
            framebuffer: [0u8; 1024],
            matrix_leds: [0u8; 35],
            panel_leds: [0u8; 35],
            icon_leds: [0u8; 8],
            rear_panel: RearPanelState::default(),
            has_banner: false,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub command_tx: Sender<ClientCommand>,
    pub swd_out_tx: tokio::sync::broadcast::Sender<String>,
    pub snapshot: Arc<RwLock<EmulatorSnapshot>>,
}

#[derive(Serialize)]
pub struct ApiStateResponse {
    pub has_banner: bool,
    pub framebuffer: Vec<u8>,
    pub matrix_leds: Vec<u8>,
    pub panel_leds: Vec<u8>,
    pub icon_leds: Vec<u8>,
    pub rear_panel: RearPanelState,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "cmd")]
pub enum ClientCommand {
    #[serde(rename = "key")]
    Key { key: usize, pressure: u8 },

    #[serde(rename = "microswitch")]
    Microswitch {
        name: String,
        #[serde(default)]
        action: Option<String>,
        #[serde(default)]
        pressed: Option<bool>,
    },

    #[serde(rename = "encoder")]
    Encoder { name: String, delta: i16 },

    #[serde(rename = "encoder_with_microswitch")]
    EncoderWithMicroswitch {
        name: String,
        #[serde(default)]
        action: Option<String>,
        #[serde(default)]
        pressed: Option<bool>,
        #[serde(default)]
        delta: Option<i16>,
    },

    #[serde(rename = "potentiometer")]
    Potentiometer { channel: u16, value: u16 },

    #[serde(rename = "touchstrip")]
    Touchstrip { value: u16, touched: bool },

    #[serde(rename = "midi")]
    Midi { bytes: Vec<u8> },

    #[serde(rename = "swd")]
    Swd { command: String },

    #[serde(rename = "clock_in")]
    ClockIn { high: bool },

    #[serde(rename = "clock_pulse")]
    ClockPulse {
        #[serde(default)]
        duration_ms: Option<u32>,
    },

    #[serde(rename = "set_headset_jack")]
    SetHeadsetJack { state: vibefreak_core::HeadsetJackState },

    #[serde(rename = "set_headset_button")]
    SetHeadsetButton { pressed: bool },

    #[serde(rename = "set_audio_input_source")]
    SetAudioInputSource { source: vibefreak_core::AudioInputSource },

    #[serde(rename = "feed_audio_input")]
    FeedAudioInput { samples: Vec<f32> },
}

pub async fn start_web_server(
    command_tx: Sender<ClientCommand>,
    swd_out_tx: tokio::sync::broadcast::Sender<String>,
    snapshot: Arc<RwLock<EmulatorSnapshot>>,
    listener: tokio::net::TcpListener,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        command_tx,
        swd_out_tx,
        snapshot,
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/state", get(get_api_state))
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    if let Ok(local_addr) = listener.local_addr() {
        println!("Web UI listening on http://localhost:{}", local_addr.port());
    }
    axum::serve(listener, app).await?;

    Ok(())
}

async fn serve_index() -> Html<&'static str> {
    const HTML: &str = include_str!("../static/index.html");
    Html(HTML)
}

async fn get_api_state(State(state): State<AppState>) -> Json<ApiStateResponse> {
    let snap = state.snapshot.read().unwrap().clone();
    Json(ApiStateResponse {
        has_banner: snap.has_banner,
        framebuffer: snap.framebuffer.to_vec(),
        matrix_leds: snap.matrix_leds.to_vec(),
        panel_leds: snap.panel_leds.to_vec(),
        icon_leds: snap.icon_leds.to_vec(),
        rear_panel: snap.rear_panel,
    })
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<Message>(128);

    // Dedicated outbound socket writer task
    let mut ws_send_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Task 1: 60 FPS display & panel state writer
    let snapshot_reader = state.snapshot.clone();
    let out_tx_snap = out_tx.clone();
    let snap_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(16)); // ~60 FPS
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_framebuffer: Option<[u8; 1024]> = None;
        let mut last_matrix_leds: Option<[u8; 35]> = None;
        let mut last_panel_leds: Option<[u8; 35]> = None;
        let mut last_icon_leds: Option<[u8; 8]> = None;
        let mut last_rear_panel: Option<RearPanelState> = None;
        let mut tick_counter: usize = 0;

        loop {
            interval.tick().await;

            let snap = if let Ok(guard) = snapshot_reader.read() {
                guard.clone()
            } else {
                break;
            };

            tick_counter = tick_counter.wrapping_add(1);
            let heartbeat = (tick_counter % 60) == 1;

            // 1. Send OLED Framebuffer (Subtype 0x01 + 1024 bytes) when changed or heartbeat
            if heartbeat || last_framebuffer.as_ref() != Some(&snap.framebuffer) {
                let mut oled_msg = Vec::with_capacity(1025);
                oled_msg.push(0x01);
                oled_msg.extend_from_slice(&snap.framebuffer);
                if out_tx_snap.send(Message::Binary(oled_msg)).await.is_err() {
                    break;
                }
                last_framebuffer = Some(snap.framebuffer);
            }

            // 2. Send LED and Rear Panel state updates (Subtype 0x03 + JSON) when changed or heartbeat
            let leds_changed = last_matrix_leds.as_ref() != Some(&snap.matrix_leds)
                || last_panel_leds.as_ref() != Some(&snap.panel_leds)
                || last_icon_leds.as_ref() != Some(&snap.icon_leds)
                || last_rear_panel.as_ref() != Some(&snap.rear_panel);

            if heartbeat || leds_changed {
                #[derive(Serialize)]
                struct StateUpdate {
                    matrix_leds: Vec<u8>,
                    panel_leds: Vec<u8>,
                    icon_leds: Vec<u8>,
                    rear_panel: RearPanelState,
                }
                let update = StateUpdate {
                    matrix_leds: snap.matrix_leds.to_vec(),
                    panel_leds: snap.panel_leds.to_vec(),
                    icon_leds: snap.icon_leds.to_vec(),
                    rear_panel: snap.rear_panel,
                };
                if let Ok(json_bytes) = serde_json::to_vec(&update) {
                    let mut state_msg = Vec::with_capacity(1 + json_bytes.len());
                    state_msg.push(0x03);
                    state_msg.extend_from_slice(&json_bytes);
                    if out_tx_snap.send(Message::Binary(state_msg)).await.is_err() {
                        break;
                    }
                }
                last_matrix_leds = Some(snap.matrix_leds);
                last_panel_leds = Some(snap.panel_leds);
                last_icon_leds = Some(snap.icon_leds);
                last_rear_panel = Some(snap.rear_panel);
            }
        }
    });

    // Task 2: Inbound command receiver
    let cmd_tx = state.command_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(cmd) = serde_json::from_str::<ClientCommand>(&text) {
                    let _ = cmd_tx.send(cmd);
                }
            }
        }
    });

    // Task 3: SWD debugger terminal output writer (Subtype 0x04)
    let mut swd_rx = state.swd_out_tx.subscribe();
    let out_tx_swd = out_tx.clone();
    let mut swd_task = tokio::spawn(async move {
        while let Ok(text) = swd_rx.recv().await {
            let mut msg = Vec::with_capacity(1 + text.len());
            msg.push(0x04);
            msg.extend_from_slice(text.as_bytes());
            if out_tx_swd.send(Message::Binary(msg)).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = (&mut ws_send_task) => {
            snap_task.abort();
            recv_task.abort();
            swd_task.abort();
        }
        _ = (&mut recv_task) => {
            ws_send_task.abort();
            snap_task.abort();
            swd_task.abort();
        }
        _ = (&mut swd_task) => {
            ws_send_task.abort();
            snap_task.abort();
            recv_task.abort();
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microswitch_deserialization() {
        let json_switch = r#"{"cmd":"microswitch","name":"oct_up","action":"down"}"#;
        let cmd: ClientCommand = serde_json::from_str(json_switch).unwrap();
        match cmd {
            ClientCommand::Microswitch { name, action, pressed } => {
                assert_eq!(name, "oct_up");
                assert_eq!(action.as_deref(), Some("down"));
                assert_eq!(pressed, None);
            }
            _ => panic!("Expected Microswitch"),
        }
    }

    #[test]
    fn test_encoder_with_microswitch_deserialization() {
        let json_matrix = r#"{"cmd":"encoder_with_microswitch","name":"matrix","delta":2}"#;
        let cmd: ClientCommand = serde_json::from_str(json_matrix).unwrap();
        match cmd {
            ClientCommand::EncoderWithMicroswitch { name, delta, action, pressed } => {
                assert_eq!(name, "matrix");
                assert_eq!(delta, Some(2));
                assert_eq!(action, None);
                assert_eq!(pressed, None);
            }
            _ => panic!("Expected EncoderWithMicroswitch"),
        }

        let json_preset = r#"{"cmd":"encoder_with_microswitch","name":"preset","action":"down"}"#;
        let cmd: ClientCommand = serde_json::from_str(json_preset).unwrap();
        match cmd {
            ClientCommand::EncoderWithMicroswitch { name, delta, action, pressed } => {
                assert_eq!(name, "preset");
                assert_eq!(action.as_deref(), Some("down"));
                assert_eq!(delta, None);
                assert_eq!(pressed, None);
            }
            _ => panic!("Expected EncoderWithMicroswitch"),
        }

        let json_rate = r#"{"cmd":"encoder_with_microswitch","name":"rate","pressed":true}"#;
        let cmd: ClientCommand = serde_json::from_str(json_rate).unwrap();
        match cmd {
            ClientCommand::EncoderWithMicroswitch { name, delta, action, pressed } => {
                assert_eq!(name, "rate");
                assert_eq!(pressed, Some(true));
                assert_eq!(delta, None);
                assert_eq!(action, None);
            }
            _ => panic!("Expected EncoderWithMicroswitch"),
        }

        let json_lfo_rate = r#"{"cmd":"encoder_with_microswitch","name":"lfo_rate","delta":-1,"action":"up"}"#;
        let cmd: ClientCommand = serde_json::from_str(json_lfo_rate).unwrap();
        match cmd {
            ClientCommand::EncoderWithMicroswitch { name, delta, action, pressed } => {
                assert_eq!(name, "lfo_rate");
                assert_eq!(delta, Some(-1));
                assert_eq!(action.as_deref(), Some("up"));
                assert_eq!(pressed, None);
            }
            _ => panic!("Expected EncoderWithMicroswitch"),
        }

        let json_lfo_rate_press = r#"{"cmd":"encoder_with_microswitch","name":"lfo_rate","action":"down"}"#;
        let cmd: ClientCommand = serde_json::from_str(json_lfo_rate_press).unwrap();
        match cmd {
            ClientCommand::EncoderWithMicroswitch { name, delta, action, pressed } => {
                assert_eq!(name, "lfo_rate");
                assert_eq!(delta, None);
                assert_eq!(action.as_deref(), Some("down"));
                assert_eq!(pressed, None);
            }
            _ => panic!("Expected EncoderWithMicroswitch"),
        }
    }

    #[test]
    fn test_potentiometer_deserialization() {
        let json_pot = r#"{"cmd":"potentiometer","channel":2,"value":3500}"#;
        let cmd: ClientCommand = serde_json::from_str(json_pot).unwrap();
        match cmd {
            ClientCommand::Potentiometer { channel, value } => {
                assert_eq!(channel, 2);
                assert_eq!(value, 3500);
            }
            _ => panic!("Expected Potentiometer"),
        }
    }

    #[test]
    fn test_midi_command_deserialization() {
        let json_midi = r#"{"cmd":"midi","bytes":[144,60,127]}"#;
        let cmd: ClientCommand = serde_json::from_str(json_midi).unwrap();
        match cmd {
            ClientCommand::Midi { bytes } => {
                assert_eq!(bytes, vec![0x90, 0x3C, 0x7F]);
            }
            _ => panic!("Expected Midi"),
        }
    }

    #[test]
    fn test_swd_deserialization() {
        let json_swd = r#"{"cmd":"swd","command":"regs"}"#;
        let cmd: ClientCommand = serde_json::from_str(json_swd).unwrap();
        match cmd {
            ClientCommand::Swd { command } => {
                assert_eq!(command, "regs");
            }
            _ => panic!("Expected Swd command"),
        }

        let json_mdw = r#"{"cmd":"swd","command":"mdw 0x20000000 4"}"#;
        let cmd2: ClientCommand = serde_json::from_str(json_mdw).unwrap();
        match cmd2 {
            ClientCommand::Swd { command } => {
                assert_eq!(command, "mdw 0x20000000 4");
            }
            _ => panic!("Expected Swd command"),
        }
    }

    #[test]
    fn test_assign_microswitch_deserialization() {
        for assign_name in ["assign_1", "assign_2", "assign_3"] {
            let json = format!(r#"{{"cmd":"microswitch","name":"{}","action":"down"}}"#, assign_name);
            let cmd: ClientCommand = serde_json::from_str(&json).unwrap();
            match cmd {
                ClientCommand::Microswitch { name, action, pressed } => {
                    assert_eq!(name, assign_name);
                    assert_eq!(action.as_deref(), Some("down"));
                    assert_eq!(pressed, None);
                }
                _ => panic!("Expected Microswitch"),
            }
        }
    }

    #[test]
    fn test_codec_commands_deserialization() {
        let json_jack = r#"{"cmd":"set_headset_jack","state":"headset_with_mic"}"#;
        let cmd: ClientCommand = serde_json::from_str(json_jack).unwrap();
        match cmd {
            ClientCommand::SetHeadsetJack { state } => {
                assert_eq!(state, vibefreak_core::HeadsetJackState::HeadsetWithMic);
            }
            _ => panic!("Expected SetHeadsetJack"),
        }

        let json_btn = r#"{"cmd":"set_headset_button","pressed":true}"#;
        let cmd: ClientCommand = serde_json::from_str(json_btn).unwrap();
        match cmd {
            ClientCommand::SetHeadsetButton { pressed } => {
                assert!(pressed);
            }
            _ => panic!("Expected SetHeadsetButton"),
        }

        let json_src = r#"{"cmd":"set_audio_input_source","source":"line_in"}"#;
        let cmd: ClientCommand = serde_json::from_str(json_src).unwrap();
        match cmd {
            ClientCommand::SetAudioInputSource { source } => {
                assert_eq!(source, vibefreak_core::AudioInputSource::LineIn);
            }
            _ => panic!("Expected SetAudioInputSource"),
        }

        let json_feed = r#"{"cmd":"feed_audio_input","samples":[0.1, -0.2, 0.5]}"#;
        let cmd: ClientCommand = serde_json::from_str(json_feed).unwrap();
        match cmd {
            ClientCommand::FeedAudioInput { samples } => {
                assert_eq!(samples, vec![0.1, -0.2, 0.5]);
            }
            _ => panic!("Expected FeedAudioInput"),
        }
    }
}


