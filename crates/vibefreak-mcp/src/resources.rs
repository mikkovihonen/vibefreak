use std::sync::Arc;

use crate::backend::EmulatorBackend;
use crate::display::{render_oled_braille, render_panel_state};
use crate::protocol::{Resource, ResourceContents};

pub fn get_resources_list() -> Vec<Resource> {
    vec![
        Resource {
            uri: "vibefreak://oled/screen".to_string(),
            name: "OLED Display Screen".to_string(),
            description: Some("Current 128x64 monochrome OLED screen rendered in high-resolution Unicode Braille.".to_string()),
            mime_type: Some("text/plain".to_string()),
        },
        Resource {
            uri: "vibefreak://panel/state".to_string(),
            name: "Front Panel & Matrix State".to_string(),
            description: Some("5x7 modulation matrix LED grid, panel LEDs, icon LEDs, and rear panel CV/Gate voltages.".to_string()),
            mime_type: Some("text/plain".to_string()),
        },
        Resource {
            uri: "vibefreak://swd/status".to_string(),
            name: "SWD Target Status & Registers".to_string(),
            description: Some("ARM Cortex-M4 target CPU state, PC, SP, and core register dump.".to_string()),
            mime_type: Some("text/plain".to_string()),
        },
        Resource {
            uri: "vibefreak://swd/symbols".to_string(),
            name: "Firmware Symbols & Pointers".to_string(),
            description: Some("List of known firmware symbols, pointers, and memory locations.".to_string()),
            mime_type: Some("text/plain".to_string()),
        },
    ]
}

pub async fn read_resource(
    backend: &Arc<dyn EmulatorBackend>,
    uri: &str,
) -> Result<ResourceContents, String> {
    match uri {
        "vibefreak://oled/screen" => {
            let fb = backend.get_oled().await?;
            let text = render_oled_braille(&fb);
            Ok(ResourceContents {
                uri: uri.to_string(),
                mime_type: Some("text/plain".to_string()),
                text,
            })
        }
        "vibefreak://panel/state" => {
            let snap = backend.get_snapshot().await?;
            let text = render_panel_state(&snap);
            Ok(ResourceContents {
                uri: uri.to_string(),
                mime_type: Some("text/plain".to_string()),
                text,
            })
        }
        "vibefreak://swd/status" => {
            let status = backend.execute_swd("status").await?;
            let regs = backend.execute_swd("regs").await?;
            let text = format!("{status}\nRegisters:\n{regs}");
            Ok(ResourceContents {
                uri: uri.to_string(),
                mime_type: Some("text/plain".to_string()),
                text,
            })
        }
        "vibefreak://swd/symbols" => {
            let text = backend.execute_swd("sym").await?;
            Ok(ResourceContents {
                uri: uri.to_string(),
                mime_type: Some("text/plain".to_string()),
                text,
            })
        }
        other => Err(format!("Unknown resource URI: {other}")),
    }
}

