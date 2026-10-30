use vibefreak_web::EmulatorSnapshot;

pub const OLED_WIDTH: usize = 128;
pub const OLED_HEIGHT: usize = 64;

/// Checks if a pixel at (x, y) is lit in the SSD1306 framebuffer.
pub fn is_pixel_on(fb: &[u8; 1024], x: usize, y: usize) -> bool {
    if x >= OLED_WIDTH || y >= OLED_HEIGHT {
        return false;
    }
    let page = y / 8;
    let bit = y % 8;
    let byte = fb[page * 128 + x];
    (byte & (1 << bit)) != 0
}

/// Renders the 128x64 monochrome OLED framebuffer as 64x16 Unicode Braille patterns (U+2800..U+28FF).
/// Each Braille glyph represents a 2x4 pixel block.
pub fn render_oled_braille(fb: &[u8; 1024]) -> String {
    let mut out = String::new();
    let lit_pixels = count_lit_pixels(fb);
    let pct = (lit_pixels as f64 / (OLED_WIDTH * OLED_HEIGHT) as f64) * 100.0;
    let nz_bytes = fb.iter().filter(|&&b| b != 0).count();

    out.push_str("┌─ VibeFreak OLED Display (128x64) ───────────────────────────────┐\n");

    // 64 vertical slices of 4 rows each -> 16 text lines
    for row in 0..16 {
        out.push('│');
        for col in 0..64 {
            let x0 = col * 2;
            let x1 = col * 2 + 1;
            let y0 = row * 4;
            let y1 = row * 4 + 1;
            let y2 = row * 4 + 2;
            let y3 = row * 4 + 3;

            let mut pattern = 0u32;
            if is_pixel_on(fb, x0, y0) { pattern |= 0x01; } // Dot 1
            if is_pixel_on(fb, x0, y1) { pattern |= 0x02; } // Dot 2
            if is_pixel_on(fb, x0, y2) { pattern |= 0x04; } // Dot 3
            if is_pixel_on(fb, x0, y3) { pattern |= 0x40; } // Dot 7
            if is_pixel_on(fb, x1, y0) { pattern |= 0x08; } // Dot 4
            if is_pixel_on(fb, x1, y1) { pattern |= 0x10; } // Dot 5
            if is_pixel_on(fb, x1, y2) { pattern |= 0x20; } // Dot 6
            if is_pixel_on(fb, x1, y3) { pattern |= 0x80; } // Dot 8

            let ch = char::from_u32(0x2800 + pattern).unwrap_or(' ');
            out.push(ch);
        }
        out.push_str("│\n");
    }

    out.push_str(&format!(
        "└────────────────────────────────────────────────────────────────┘\n\
         Display Stats: {lit_pixels} / 8192 pixels lit ({pct:.1}%) | Non-zero buffer bytes: {nz_bytes}/1024\n"
    ));

    out
}

/// Renders downsampled 64x32 ASCII block view (`#` for lit, `.` for unlit).
pub fn render_oled_ascii(fb: &[u8; 1024]) -> String {
    let mut out = String::new();
    out.push_str("+- VibeFreak OLED ASCII (64x32 downsampled) ---------------------+\n");
    for y in (0..OLED_HEIGHT).step_by(2) {
        out.push('|');
        for x in (0..OLED_WIDTH).step_by(2) {
            let p00 = is_pixel_on(fb, x, y);
            let p10 = is_pixel_on(fb, x + 1, y);
            let p01 = is_pixel_on(fb, x, y + 1);
            let p11 = is_pixel_on(fb, x + 1, y + 1);
            let sum = p00 as u8 + p10 as u8 + p01 as u8 + p11 as u8;
            let ch = match sum {
                0 => '.',
                1 => ':',
                2 => '+',
                3 => '*',
                _ => '#',
            };
            out.push(ch);
        }
        out.push_str("|\n");
    }
    out.push_str("+----------------------------------------------------------------+\n");
    out
}

pub fn count_lit_pixels(fb: &[u8; 1024]) -> usize {
    let mut count = 0;
    for &b in fb.iter() {
        count += b.count_ones() as usize;
    }
    count
}

/// Formats the complete physical panel state into a readable report.
pub fn render_panel_state(snap: &EmulatorSnapshot) -> String {
    let mut out = String::new();

    out.push_str("=== VibeFreak Front Panel & Hardware State ===\n\n");

    // 1. 5x7 Modulation Matrix
    out.push_str("Modulation Matrix (5 Sources x 7 Destinations):\n");
    out.push_str("                  Pitch   Wave    Timbre  Cutoff  Assign1 Assign2 Assign3\n");
    out.push_str("                ┌───────┬───────┬───────┬───────┬───────┬───────┬───────┐\n");

    let row_names = [
        "CYC ENV        ",
        "ENV            ",
        "LFO            ",
        "PRESSURE       ",
        "KEY / ARP      ",
    ];

    for (r, name) in row_names.iter().enumerate() {
        out.push_str(&format!("{name} │"));
        for c in 0..7 {
            let idx = r * 7 + c;
            let val = snap.matrix_leds.get(idx).copied().unwrap_or(0);
            if val == 0 {
                out.push_str("   .   │");
            } else {
                out.push_str(&format!("  {:3}  │", val));
            }
        }
        out.push('\n');
    }
    out.push_str("                └───────┴───────┴───────┴───────┴───────┴───────┴───────┘\n\n");

    // 2. Icon LEDs
    out.push_str("Capacitive Icon LEDs (8):\n  ");
    let icon_names = ["Arp Seq", "Pattern", "Seq A", "Seq B", "Hold", "Paraphonic", "Spice", "Dice"];
    for (i, name) in icon_names.iter().enumerate() {
        let val = snap.icon_leds.get(i).copied().unwrap_or(0);
        let status = if val > 0 { format!("ON({val})") } else { "off".to_string() };
        out.push_str(&format!("[{name}: {status}] "));
    }
    out.push_str("\n\n");

    // 3. Rear Panel CV / Gate / Clock / Audio
    out.push_str("Rear Panel Connections & Outputs:\n");
    out.push_str(&format!("  CV Pitch Out:   {:.3} V\n", snap.rear_panel.pitch_cv_volts));
    out.push_str(&format!("  Gate Out:       {:.3} V (raw: {})\n", snap.rear_panel.gate_volts, snap.rear_panel.gate_raw));
    out.push_str(&format!("  Pressure Out:   {:.3} V\n", snap.rear_panel.pressure_volts));
    out.push_str(&format!("  Clock Out:      {:.1} V (raw: {})\n", snap.rear_panel.clock_out_volts, snap.rear_panel.clock_out_raw));
    out.push_str(&format!("  Clock In:       {}\n", if snap.rear_panel.clock_in_raw { "HIGH" } else { "low" }));
    out.push_str(&format!("  Headset Jack:   {:?}\n", snap.rear_panel.headset_jack));
    out.push_str(&format!("  Headset Button: {}\n", if snap.rear_panel.headset_button { "PRESSED" } else { "released" }));
    out.push_str(&format!("  Audio In Source:{:?}\n", snap.rear_panel.audio_input_source));

    out
}
