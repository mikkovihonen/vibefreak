pub mod backend;
pub mod display;
pub mod prompts;
pub mod protocol;
pub mod resources;
pub mod server;
pub mod tools;

pub use backend::{create_backend, EmulatorBackend, InProcessBackend, McpConfig, McpMode, RemoteBackend};
pub use display::{is_pixel_on, render_oled_ascii, render_oled_braille, render_panel_state};
pub use prompts::{get_prompt_messages, get_prompts_list};
pub use protocol::*;
pub use resources::{get_resources_list, read_resource};
pub use server::{run_server_with_io, run_stdio_server};
pub use tools::{get_tools_list, handle_call_tool};

pub async fn run_mcp_server(config: McpConfig) -> Result<(), Box<dyn std::error::Error>> {
    let backend = create_backend(&config).await?;
    run_stdio_server(backend).await
}
