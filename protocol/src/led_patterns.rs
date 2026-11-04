use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedPatternInfo {
    pub id: String,
    pub label: String,
    pub primary_rgb: [u8; 3],
}

pub fn is_supported_led_pattern_path(rel_id: &str) -> bool {
    let norm = rel_id.replace('\\', "/");
    let clean = norm.trim_start_matches('/').to_ascii_lowercase();
    if !clean.ends_with(".led") {
        return false;
    }
    if clean.starts_with("clock/")
        || clean.starts_with("screen/")
        || clean.contains("emi")
        || clean.contains("rgbw_led_panel_tests")
    {
        return false;
    }
    true
}

fn parse_hex_color_token(tok: &str) -> Option<[u8; 3]> {
    if tok.len() == 6 && tok.chars().all(|c| c.is_ascii_hexdigit()) {
        let rgb = u32::from_str_radix(tok, 16).ok()?;
        Some([
            ((rgb >> 16) & 0xff) as u8,
            ((rgb >> 8) & 0xff) as u8,
            (rgb & 0xff) as u8,
        ])
    } else if tok.len() == 8 && tok.chars().all(|c| c.is_ascii_hexdigit()) {
        let rgbw = u32::from_str_radix(tok, 16).ok()?;
        let r = ((rgbw >> 24) & 0xff) as u8;
        let g = ((rgbw >> 16) & 0xff) as u8;
        let b = ((rgbw >> 8) & 0xff) as u8;
        let w = (rgbw & 0xff) as u8;
        Some([
            r.saturating_add(w / 2),
            g.saturating_add(w / 2),
            b.saturating_add(w / 2),
        ])
    } else {
        None
    }
}

pub fn parse_led_pattern_frames(raw: &str, ring_leds: usize) -> Vec<(Vec<[u8; 3]>, u32)> {
    let mut frames = Vec::new();
    let mut file_has_rotate_or_gradient = false;

    for raw_line in raw.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.eq_ignore_ascii_case("loop") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let mut line_has_directive = false;
        let mut parsed = Vec::with_capacity(parts.len());
        let mut last_decimal: Option<u32> = None;

        for tok in &parts {
            let lower = tok.to_ascii_lowercase();
            if matches!(lower.as_str(), "gradient" | "smooth" | "rotate") {
                line_has_directive = true;
                if matches!(lower.as_str(), "gradient" | "rotate") {
                    file_has_rotate_or_gradient = true;
                }
                continue;
            }
            if let Some(rgb) = parse_hex_color_token(tok) {
                parsed.push(rgb);
            } else if tok.len() != 6
                && tok.len() != 8
                && tok.chars().all(|c| c.is_ascii_digit())
            {
                if let Ok(val) = tok.parse::<u32>() {
                    last_decimal = Some(val);
                }
            }
        }

        if !parsed.is_empty() && ring_leds > 0 {
            let dur_ms = match last_decimal {
                Some(d) if line_has_directive && d < 15 => 40,
                Some(d) => d.clamp(15, 250),
                None => 40,
            };
            let mut resized = Vec::with_capacity(ring_leds);
            for i in 0..ring_leds {
                resized.push(parsed[i % parsed.len()]);
            }
            frames.push((resized, dur_ms));
        }
    }

    if file_has_rotate_or_gradient && frames.len() == 1 && ring_leds > 1 {
        let base_frame = frames[0].0.clone();
        let steps = ring_leds.clamp(6, 24);
        let mut expanded = Vec::with_capacity(steps);
        for step in 0..steps {
            let mut rotated = Vec::with_capacity(ring_leds);
            for i in 0..ring_leds {
                rotated.push(base_frame[(i + ring_leds - (step % ring_leds)) % ring_leds]);
            }
            expanded.push((rotated, 35));
        }
        return expanded;
    }

    frames
}

pub fn extract_led_pattern_primary_rgb(raw: &str) -> Option<[u8; 3]> {
    let frames = parse_led_pattern_frames(raw, 36);
    let mut best_rgb: Option<[u8; 3]> = None;
    let mut best_score: u32 = 0;

    for (px_list, _) in &frames {
        for &rgb in px_list {
            let max_c = rgb[0].max(rgb[1]).max(rgb[2]) as u32;
            let min_c = rgb[0].min(rgb[1]).min(rgb[2]) as u32;
            let chroma = max_c - min_c;
            let score = chroma * 3 + max_c;
            if best_rgb.is_none() || score > best_score {
                best_rgb = Some(rgb);
                best_score = score;
            }
        }
    }

    if best_rgb.is_none() {
        for raw_line in raw.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            for tok in line.split_whitespace() {
                if let Some(rgb) = parse_hex_color_token(tok) {
                    let max_c = rgb[0].max(rgb[1]).max(rgb[2]) as u32;
                    let min_c = rgb[0].min(rgb[1]).min(rgb[2]) as u32;
                    let chroma = max_c - min_c;
                    let score = chroma * 3 + max_c;
                    if best_rgb.is_none() || score > best_score {
                        best_rgb = Some(rgb);
                        best_score = score;
                    }
                }
            }
        }
    }

    best_rgb
}

pub fn classify_rgb_color_ru(rgb: [u8; 3]) -> &'static str {
    let r = rgb[0] as f32;
    let g = rgb[1] as f32;
    let b = rgb[2] as f32;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    if max < 16.0 {
        return "Чёрный";
    }

    let sat = delta / max;
    if sat < 0.18 {
        return if max >= 170.0 { "Белый" } else { "Серый" };
    }

    let mut hue = if (max - r).abs() < f32::EPSILON {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    if hue < 0.0 {
        hue += 360.0;
    }

    if !(16.0..345.0).contains(&hue) {
        "Красный"
    } else if hue < 42.0 {
        "Янтарный"
    } else if hue < 68.0 {
        "Жёлтый"
    } else if hue < 140.0 {
        "Зелёный"
    } else if hue < 172.0 {
        "Изумрудный"
    } else if hue < 202.0 {
        "Бирюзовый"
    } else if hue < 228.0 {
        "Голубой"
    } else if hue < 248.0 {
        "Синий"
    } else if hue < 295.0 {
        "Фиолетовый"
    } else {
        "Пурпурный"
    }
}

pub fn format_rgb_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

pub fn build_led_pattern_info(rel_id: &str, raw: &str) -> Option<LedPatternInfo> {
    let norm = rel_id.replace('\\', "/");
    let clean_id = norm.trim_start_matches('/');
    if !is_supported_led_pattern_path(clean_id) {
        return None;
    }
    let primary_rgb = extract_led_pattern_primary_rgb(raw)?;
    let hex = format_rgb_hex(primary_rgb);
    let color_ru = classify_rgb_color_ru(primary_rgb);
    let label = format!("{} ({} {})", clean_id, hex, color_ru);
    Some(LedPatternInfo {
        id: clean_id.to_string(),
        label,
        primary_rgb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_led_pattern_color_classification_and_info() {
        let listening_raw = "loop\n6b0fff 370fff 6b0fff 370fff 6b0fff 370fff 30\n";
        let info = build_led_pattern_info("ring/alice_listening.led", listening_raw)
            .expect("should build info for alice_listening.led");
        assert_eq!(info.id, "ring/alice_listening.led");
        assert_eq!(info.primary_rgb, [0x6B, 0x0F, 0xFF]);
        assert_eq!(info.label, "ring/alice_listening.led (#6B0FFF Фиолетовый)");

        let listening_dark_raw = "loop\n370fff 370fff 000000 35\n";
        let info_dark = build_led_pattern_info("ring/alice_listening.led", listening_dark_raw)
            .expect("should build info for #370FFF");
        assert_eq!(info_dark.primary_rgb, [0x37, 0x0F, 0xFF]);
        assert_eq!(
            info_dark.label,
            "ring/alice_listening.led (#370FFF Фиолетовый)"
        );

        let pro_raw = "loop\n00ff80 004020 00ff80 30\n";
        let pro_info = build_led_pattern_info("ring/alice_pro_listening.led", pro_raw)
            .expect("should build info for alice_pro_listening.led");
        assert_eq!(pro_info.primary_rgb, [0x00, 0xFF, 0x80]);
        assert_eq!(
            pro_info.label,
            "ring/alice_pro_listening.led (#00FF80 Изумрудный)"
        );

        let mute_raw = "ff0000 ff0000 ff0000 ff0000 ff0000 ff0000 100\n";
        let mute_info = build_led_pattern_info("ring/mute.led", mute_raw)
            .expect("should build info for mute.led");
        assert_eq!(mute_info.primary_rgb, [0xFF, 0x00, 0x00]);
        assert_eq!(mute_info.label, "ring/mute.led (#FF0000 Красный)");

        assert!(!is_supported_led_pattern_path("clock/blink.binscled.zst"));
        assert!(!is_supported_led_pattern_path("clock/colon.led"));
        assert!(!is_supported_led_pattern_path("screen/smile.led"));
        assert!(!is_supported_led_pattern_path("emi/test.led"));
        assert!(!is_supported_led_pattern_path(
            "rgbw_led_panel_tests/white.led"
        ));
        assert!(is_supported_led_pattern_path("oknix/alice_speaking.led"));
    }

    #[test]
    fn test_parse_led_pattern_directives_gradient_smooth_rotate() {
        let rotate_raw = "# timer animation\n6b0fff 000000 000000 000000 000000 000000 rotate 1\n";
        let frames = parse_led_pattern_frames(rotate_raw, 12);
        assert_eq!(frames.len(), 12);
        assert_eq!(frames[0].0[0], [0x6B, 0x0F, 0xFF]);
        assert_eq!(frames[0].0[1], [0x00, 0x00, 0x00]);
        assert_eq!(frames[1].0[1], [0x6B, 0x0F, 0xFF]);
        assert_eq!(frames[0].1, 35);

        let smooth_raw = "00ff80 00e5ff smooth\n00e5ff 00ff80 smooth 45\n";
        let smooth_frames = parse_led_pattern_frames(smooth_raw, 6);
        assert_eq!(smooth_frames.len(), 2);
        assert_eq!(smooth_frames[0].1, 40);
        assert_eq!(smooth_frames[1].1, 45);

        let gradient_raw = "00e5ff 000000 gradient\n";
        let grad_frames = parse_led_pattern_frames(gradient_raw, 8);
        assert_eq!(grad_frames.len(), 8);
    }
}
