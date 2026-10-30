use serde_json::Value;

use crate::protocol::{ContentItem, Prompt, PromptArgument, PromptMessage};

pub fn get_prompts_list() -> Vec<Prompt> {
    vec![
        Prompt {
            name: "debug_firmware".to_string(),
            description: Some("Guided workflow to inspect ARM Cortex-M4 CPU registers, PC, call stack, and fault registers on VibeFreak.".to_string()),
            arguments: Some(vec![PromptArgument {
                name: "focus".to_string(),
                description: Some("Specific component to debug (e.g. 'crashes', 'interrupts', 'audio_dma').".to_string()),
                required: false,
            }]),
        },
        Prompt {
            name: "inspect_preset".to_string(),
            description: Some("Guided workflow to analyze the currently active synth preset, engine type, parameters, and modulation matrix routing.".to_string()),
            arguments: None,
        },
        Prompt {
            name: "test_patch_audio".to_string(),
            description: Some("Workflow to test audio synthesis: plays keys on the capacitive keybed, tests filter sweeps, and verifies screen updates.".to_string()),
            arguments: Some(vec![PromptArgument {
                name: "note".to_string(),
                description: Some("Note index (0..24, e.g. 12 for middle C).".to_string()),
                required: false,
            }]),
        },
    ]
}

pub fn get_prompt_messages(name: &str, args: Option<&Value>) -> Result<Vec<PromptMessage>, String> {
    match name {
        "debug_firmware" => {
            let focus = args
                .and_then(|a| a.get("focus"))
                .and_then(|v| v.as_str())
                .unwrap_or("general");
            Ok(vec![PromptMessage {
                role: "user".to_string(),
                content: ContentItem::Text {
                    text: format!(
                        "Please perform SWD debugging on the VibeFreak STM32F446 target (focus: {focus}).\n\
                         1. Call `swd_status` and `swd_read_registers` to inspect PC, SP, and core state.\n\
                         2. Check `swd_nvic_status` for active interrupts.\n\
                         3. Call `emu_read_oled` to see what is currently rendered on screen.\n\
                         4. Analyze any faults or anomalies."
                    ),
                },
            }])
        }
        "inspect_preset" => Ok(vec![PromptMessage {
            role: "user".to_string(),
            content: ContentItem::Text {
                text: "Please inspect the active MicroFreak preset in VibeFreak:\n\
                       1. Call `swd_read_symbol` with 'p_current_preset' to find the preset data pointer.\n\
                       2. Read the OLED screen using `emu_read_oled` to get the preset name and icon.\n\
                       3. Read the modulation matrix using `emu_read_panel`.\n\
                       4. Summarize the oscillator type, filter parameters, and modulation routings."
                    .to_string(),
            },
        }]),
        "test_patch_audio" => {
            let note = args
                .and_then(|a| a.get("note"))
                .and_then(|v| v.as_u64())
                .unwrap_or(12);
            Ok(vec![PromptMessage {
                role: "user".to_string(),
                content: ContentItem::Text {
                    text: format!(
                        "Test synthesis on VibeFreak:\n\
                         1. Press key {note} with `synth_press_key` (pressure 180).\n\
                         2. Step emulation for 100ms with `emu_step`.\n\
                         3. Sweep the cutoff filter using `synth_set_potentiometer` (knob: 'cutoff', value: 3000).\n\
                         4. Release key {note} with `synth_release_key` and inspect OLED screen."
                    ),
                },
            }])
        }
        other => Err(format!("Unknown prompt name: {other}")),
    }
}

