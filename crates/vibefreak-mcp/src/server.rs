use std::sync::Arc;
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::backend::EmulatorBackend;
use crate::prompts::{get_prompt_messages, get_prompts_list};
use crate::protocol::*;
use crate::resources::{get_resources_list, read_resource};
use crate::tools::{get_tools_list, handle_call_tool};

pub async fn run_stdio_server(backend: Arc<dyn EmulatorBackend>) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let reader = tokio::io::BufReader::new(stdin);
    let writer = tokio::io::BufWriter::new(stdout);

    eprintln!("[MCP] VibeFreak MCP Server started (stdio transport, protocol version {})...", LATEST_PROTOCOL_VERSION);
    run_server_with_io(backend, reader, writer).await
}

pub async fn run_server_with_io<R, W>(
    backend: Arc<dyn EmulatorBackend>,
    mut reader: R,
    mut writer: W,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            // EOF reached
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = JsonRpcResponse::error(
                    Value::Null,
                    -32700,
                    format!("Parse error: {e}"),
                    None,
                );
                send_response(&mut writer, &err_resp).await?;
                continue;
            }
        };

        // If request has no id, it is a notification (e.g. notifications/initialized)
        let Some(req_id) = req.id else {
            handle_notification(&req.method, req.params.as_ref());
            continue;
        };

        let resp = handle_request(&backend, req_id, &req.method, req.params.as_ref()).await;
        send_response(&mut writer, &resp).await?;
    }

    Ok(())
}

async fn send_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    resp: &JsonRpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let json_bytes = serde_json::to_vec(resp)?;
    writer.write_all(&json_bytes).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

fn handle_notification(method: &str, _params: Option<&Value>) {
    match method {
        "notifications/initialized" => {
            eprintln!("[MCP] Client initialized successfully.");
        }
        "notifications/cancelled" => {
            eprintln!("[MCP] Client cancelled operation.");
        }
        other => {
            eprintln!("[MCP] Received unhandled notification: {other}");
        }
    }
}

async fn handle_request(
    backend: &Arc<dyn EmulatorBackend>,
    id: Value,
    method: &str,
    params: Option<&Value>,
) -> JsonRpcResponse {
    match method {
        "initialize" => {
            let res = InitializeResult {
                protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
                capabilities: ServerCapabilities {
                    tools: Some(ToolsCapability { list_changed: false }),
                    resources: Some(ResourcesCapability {
                        subscribe: false,
                        list_changed: false,
                    }),
                    prompts: Some(PromptsCapability { list_changed: false }),
                },
                server_info: ServerInfo {
                    name: "vibefreak-mcp".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
            };
            JsonRpcResponse::success(id, json!(res))
        }

        "ping" => JsonRpcResponse::success(id, json!({})),

        // --- Tools ---
        "tools/list" => {
            let tools = get_tools_list();
            JsonRpcResponse::success(id, json!({ "tools": tools }))
        }

        "tools/call" => {
            let Some(p) = params else {
                return JsonRpcResponse::invalid_params(id, "Missing params object");
            };
            let Some(tool_name) = p.get("name").and_then(|v| v.as_str()) else {
                return JsonRpcResponse::invalid_params(id, "Missing 'name' field in tools/call");
            };
            let empty_args = json!({});
            let arguments = p.get("arguments").unwrap_or(&empty_args);

            let result = handle_call_tool(backend, tool_name, arguments).await;
            JsonRpcResponse::success(id, json!(result))
        }

        // --- Resources ---
        "resources/list" => {
            let resources = get_resources_list();
            JsonRpcResponse::success(id, json!({ "resources": resources }))
        }

        "resources/read" => {
            let Some(p) = params else {
                return JsonRpcResponse::invalid_params(id, "Missing params object");
            };
            let Some(uri) = p.get("uri").and_then(|v| v.as_str()) else {
                return JsonRpcResponse::invalid_params(id, "Missing 'uri' in resources/read");
            };

            match read_resource(backend, uri).await {
                Ok(contents) => JsonRpcResponse::success(id, json!({ "contents": [contents] })),
                Err(e) => JsonRpcResponse::error(id, -32002, e, None),
            }
        }

        // --- Prompts ---
        "prompts/list" => {
            let prompts = get_prompts_list();
            JsonRpcResponse::success(id, json!({ "prompts": prompts }))
        }

        "prompts/get" => {
            let Some(p) = params else {
                return JsonRpcResponse::invalid_params(id, "Missing params object");
            };
            let Some(prompt_name) = p.get("name").and_then(|v| v.as_str()) else {
                return JsonRpcResponse::invalid_params(id, "Missing 'name' in prompts/get");
            };
            let args = p.get("arguments");

            match get_prompt_messages(prompt_name, args) {
                Ok(messages) => JsonRpcResponse::success(id, json!({ "messages": messages })),
                Err(e) => JsonRpcResponse::error(id, -32002, e, None),
            }
        }

        unknown => JsonRpcResponse::method_not_found(id, unknown),
    }
}

