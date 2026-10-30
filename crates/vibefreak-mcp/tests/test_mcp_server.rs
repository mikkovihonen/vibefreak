use std::path::PathBuf;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use vibefreak_mcp::{
    create_backend, display::render_oled_braille, get_tools_list, handle_call_tool,
    protocol::*, resources::read_resource, run_server_with_io, McpConfig, McpMode,
};

#[test]
fn test_display_braille_rendering() {
    let mut fb = [0u8; 1024];
    // Set a few pixels on page 0, col 0 and col 1
    fb[0] = 0x01; // (0, 0) on -> Dot 1
    fb[1] = 0x80; // (1, 7) on -> Dot 8 (x1, y3)

    let out = render_oled_braille(&fb);
    assert!(out.contains("VibeFreak OLED Display (128x64)"));
    assert!(out.contains("pixels lit"));
}

#[tokio::test]
async fn test_mcp_tools_list_completeness() {
    let tools = get_tools_list();
    assert_eq!(tools.len(), 36);

    let required_tool_names = [
        "swd_status",
        "swd_halt",
        "swd_resume",
        "swd_step",
        "swd_reset",
        "swd_read_registers",
        "swd_read_register",
        "swd_write_register",
        "swd_read_memory",
        "swd_write_memory",
        "swd_hexdump",
        "swd_breakpoint_set",
        "swd_breakpoint_clear",
        "swd_breakpoint_list",
        "swd_list_symbols",
        "swd_read_symbol",
        "swd_svd_peripheral",
        "swd_nvic_status",
        "swd_memmap",
        "swd_execute_command",
        "synth_press_key",
        "synth_release_key",
        "synth_press_button",
        "synth_click_button",
        "synth_step_encoder",
        "synth_click_encoder",
        "synth_set_potentiometer",
        "synth_set_touch_strip",
        "synth_set_filter_mode",
        "synth_send_midi",
        "synth_rear_panel_control",
        "emu_read_oled",
        "emu_read_panel",
        "emu_step",
        "emu_save_preset",
        "emu_load_preset",
    ];

    for name in required_tool_names {
        assert!(
            tools.iter().any(|t| t.name == name),
            "Expected tool '{}' to be defined in tools list",
            name
        );
    }
}

#[tokio::test]
async fn test_mcp_jsonrpc_io_session() {
    // Test full JSON-RPC 2.0 handshake and dispatch over an in-memory duplex channel
    let mff_candidate = std::env::var("VIBEFREAK_MFF_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("original_firmware/microfreak/MicroFreak_Firmware_Update_5_0_0_2084.mff"));
    let config = McpConfig {
        mode: McpMode::Standalone,
        mff_path: Some(mff_candidate),
        presets_path: PathBuf::from("target/test_mcp_presets.bin"),
        ..Default::default()
    };

    if !config.mff_path.as_ref().unwrap().exists() {
        eprintln!("Skipping test: test firmware fixture not found (set VIBEFREAK_MFF_PATH to run)");
        return;
    }

    let backend = create_backend(&config).await.expect("Failed to create backend");

    let (client_read, server_write) = tokio::io::duplex(4096);
    let (server_read, client_write) = tokio::io::duplex(4096);

    let backend_server = backend.clone();
    tokio::spawn(async move {
        let buf_reader = tokio::io::BufReader::new(server_read);
        let _ = run_server_with_io(backend_server, buf_reader, server_write).await;
    });

    let mut client_writer = client_write;
    let mut client_reader = tokio::io::BufReader::new(client_read);

    // 1. Send initialize request
    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "antigravity-test", "version": "1.0.0" }
        }
    });
    client_writer.write_all(format!("{}\n", init_req).as_bytes()).await.unwrap();
    client_writer.flush().await.unwrap();

    use tokio::io::AsyncBufReadExt;
    let mut resp_line = String::new();
    client_reader.read_line(&mut resp_line).await.unwrap();
    let init_resp: JsonRpcResponse = serde_json::from_str(&resp_line).unwrap();
    assert_eq!(init_resp.id, json!(1));
    let res = init_resp.result.unwrap();
    assert_eq!(res["protocolVersion"], "2024-11-05");
    assert_eq!(res["serverInfo"]["name"], "vibefreak-mcp");

    // 2. Send notifications/initialized
    let notify = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    client_writer.write_all(format!("{}\n", notify).as_bytes()).await.unwrap();
    client_writer.flush().await.unwrap();

    // 3. Send ping
    let ping_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "ping"
    });
    client_writer.write_all(format!("{}\n", ping_req).as_bytes()).await.unwrap();
    client_writer.flush().await.unwrap();

    resp_line.clear();
    client_reader.read_line(&mut resp_line).await.unwrap();
    let ping_resp: JsonRpcResponse = serde_json::from_str(&resp_line).unwrap();
    assert_eq!(ping_resp.id, json!(2));

    // 4. Call swd_status tool
    let tool_call = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "swd_status",
            "arguments": {}
        }
    });
    client_writer.write_all(format!("{}\n", tool_call).as_bytes()).await.unwrap();
    client_writer.flush().await.unwrap();

    resp_line.clear();
    client_reader.read_line(&mut resp_line).await.unwrap();
    let tool_resp: JsonRpcResponse = serde_json::from_str(&resp_line).unwrap();
    assert_eq!(tool_resp.id, json!(3));
    let call_res: CallToolResult = serde_json::from_value(tool_resp.result.unwrap()).unwrap();
    assert!(!call_res.is_error);
    match &call_res.content[0] {
        ContentItem::Text { text } => {
            assert!(text.contains("STM32F446xE"));
            assert!(text.contains("State:"));
        }
        _ => panic!("Expected Text content"),
    }
}

#[tokio::test]
async fn test_inprocess_swd_and_synth_operations() {
    let mff_path = std::env::var("VIBEFREAK_MFF_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("original_firmware/microfreak/MicroFreak_Firmware_Update_5_0_0_2084.mff"));
    if !mff_path.exists() {
        eprintln!("Skipping test: test firmware fixture not found (set VIBEFREAK_MFF_PATH to run)");
        return;
    }

    let config = McpConfig {
        mode: McpMode::Standalone,
        mff_path: Some(mff_path),
        presets_path: PathBuf::from("target/test_mcp_presets2.bin"),
        ..Default::default()
    };

    let backend = create_backend(&config).await.expect("Failed to create backend");

    // 1. Test swd_status
    let status_res = handle_call_tool(&backend, "swd_status", &json!({})).await;
    assert!(!status_res.is_error);

    // 2. Test swd_read_registers
    let regs_res = handle_call_tool(&backend, "swd_read_registers", &json!({})).await;
    assert!(!regs_res.is_error);
    if let ContentItem::Text { text } = &regs_res.content[0] {
        assert!(text.contains("R0:"));
        assert!(text.contains("PC:"));
        assert!(text.contains("SP:"));
    }

    // 3. Test swd_write_register & swd_read_register
    let write_res = handle_call_tool(
        &backend,
        "swd_write_register",
        &json!({ "name": "r0", "value": "0x1234" }),
    )
    .await;
    assert!(!write_res.is_error);

    let read_res = handle_call_tool(&backend, "swd_read_register", &json!({ "name": "r0" })).await;
    assert!(!read_res.is_error);
    if let ContentItem::Text { text } = &read_res.content[0] {
        assert!(text.contains("0x00001234"));
    }

    // 4. Test memory read / write / dump
    let mww_res = handle_call_tool(
        &backend,
        "swd_write_memory",
        &json!({ "address": "0x20000000", "value": "0xDEADBEEF", "format": "word" }),
    )
    .await;
    assert!(!mww_res.is_error);

    let mdw_res = handle_call_tool(
        &backend,
        "swd_read_memory",
        &json!({ "address": "0x20000000", "format": "word", "count": 1 }),
    )
    .await;
    assert!(!mdw_res.is_error);
    if let ContentItem::Text { text } = &mdw_res.content[0] {
        assert!(text.contains("0xDEADBEEF"));
    }

    let dump_res = handle_call_tool(
        &backend,
        "swd_hexdump",
        &json!({ "address": "0x20000000", "length": 16 }),
    )
    .await;
    assert!(!dump_res.is_error);

    // 5. Test breakpoints
    let bp_set = handle_call_tool(
        &backend,
        "swd_breakpoint_set",
        &json!({ "address": "0x08025008" }),
    )
    .await;
    assert!(!bp_set.is_error);

    let bp_list = handle_call_tool(&backend, "swd_breakpoint_list", &json!({})).await;
    assert!(!bp_list.is_error);
    if let ContentItem::Text { text } = &bp_list.content[0] {
        assert!(text.contains("0x08025008"));
    }

    let bp_clear = handle_call_tool(
        &backend,
        "swd_breakpoint_clear",
        &json!({ "address": "0x08025008" }),
    )
    .await;
    assert!(!bp_clear.is_error);

    // 6. Test symbols
    let sym_res = handle_call_tool(&backend, "swd_list_symbols", &json!({})).await;
    assert!(!sym_res.is_error);
    if let ContentItem::Text { text } = &sym_res.content[0] {
        assert!(text.contains("p_current_preset"));
        assert!(text.contains("p_tempo_bpm"));
    }

    let sym_read = handle_call_tool(
        &backend,
        "swd_read_symbol",
        &json!({ "symbol": "p_tempo_bpm" }),
    )
    .await;
    assert!(!sym_read.is_error);

    // 7. Test peripherals & NVIC & memmap
    let periph_res = handle_call_tool(&backend, "swd_svd_peripheral", &json!({ "name": "rcc" })).await;
    assert!(!periph_res.is_error);

    let nvic_res = handle_call_tool(&backend, "swd_nvic_status", &json!({})).await;
    assert!(!nvic_res.is_error);

    let memmap_res = handle_call_tool(&backend, "swd_memmap", &json!({})).await;
    assert!(!memmap_res.is_error);

    // 8. Test Synth controls: keys, buttons, encoders, pots, filter mode
    let press_key = handle_call_tool(
        &backend,
        "synth_press_key",
        &json!({ "key": 12, "pressure": 200 }),
    )
    .await;
    assert!(!press_key.is_error);

    let step_res = handle_call_tool(
        &backend,
        "emu_step",
        &json!({ "duration_ms": 30, "capture_screen": true }),
    )
    .await;
    assert!(!step_res.is_error);

    let rel_key = handle_call_tool(&backend, "synth_release_key", &json!({ "key": 12 })).await;
    assert!(!rel_key.is_error);

    let click_btn = handle_call_tool(
        &backend,
        "synth_click_button",
        &json!({ "name": "paraphonic" }),
    )
    .await;
    assert!(!click_btn.is_error);

    let step_enc = handle_call_tool(
        &backend,
        "synth_step_encoder",
        &json!({ "encoder": "preset", "delta": 1 }),
    )
    .await;
    assert!(!step_enc.is_error);

    let set_pot = handle_call_tool(
        &backend,
        "synth_set_potentiometer",
        &json!({ "knob": "cutoff", "value": 2048 }),
    )
    .await;
    assert!(!set_pot.is_error);

    let set_filter = handle_call_tool(
        &backend,
        "synth_set_filter_mode",
        &json!({ "mode": "bandpass" }),
    )
    .await;
    assert!(!set_filter.is_error);

    let set_touch = handle_call_tool(
        &backend,
        "synth_set_touch_strip",
        &json!({ "position": 1500, "touched": true }),
    )
    .await;
    assert!(!set_touch.is_error);

    // 9. Test OLED reading & panel reading
    let oled_res = handle_call_tool(&backend, "emu_read_oled", &json!({ "format": "braille" })).await;
    assert!(!oled_res.is_error);
    if let ContentItem::Text { text } = &oled_res.content[0] {
        assert!(text.contains("VibeFreak OLED Display"));
    }

    let panel_res = handle_call_tool(&backend, "emu_read_panel", &json!({})).await;
    assert!(!panel_res.is_error);
    if let ContentItem::Text { text } = &panel_res.content[0] {
        assert!(text.contains("Modulation Matrix"));
        assert!(text.contains("Rear Panel"));
    }

    // 10. Test resources
    let res_oled = read_resource(&backend, "vibefreak://oled/screen").await.unwrap();
    assert!(res_oled.text.contains("VibeFreak OLED Display"));

    let res_panel = read_resource(&backend, "vibefreak://panel/state").await.unwrap();
    assert!(res_panel.text.contains("Modulation Matrix"));

    let res_swd = read_resource(&backend, "vibefreak://swd/status").await.unwrap();
    assert!(res_swd.text.contains("STM32F446xE"));
}
