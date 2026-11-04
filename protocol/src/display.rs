use std::collections::BTreeMap;

pub const CLOCK_EXTRA_SYMS: [&str; 17] = [
    "p3", "q3", "p4", "q4", "p5", "q5", "p6", "q6", "p7", "q7", "h1", "h2", "m1", "m2", "l1",
    "l2", "deg",
];

pub const PLUM_CLOCK_EXTRA_SYMS: [&str; 8] = ["p3", "q3", "h1", "h2", "m1", "l1", "l2", "deg"];

pub const COMPACT_32_CLOCK_EXTRA_SYMS: [&str; 4] = ["p3", "q3", "m1", "deg"];

/// Возвращает канонический список всех 45 сегментов часов в фиксированном порядке:
/// `a1..g1`, `a2..g2`, `a3..g3`, `a4..g4` (28) + `p3..deg` (17).
pub fn canonical_clock_segment_names() -> Vec<String> {
    let mut names = Vec::with_capacity(45);
    for pos in 1..=4 {
        for part in ["a", "b", "c", "d", "e", "f", "g"] {
            names.push(format!("{}{}", part, pos));
        }
    }
    for sym in CLOCK_EXTRA_SYMS {
        names.push(sym.to_string());
    }
    names
}

/// Возвращает общее число физических сегментов часов для заданного Профиля платы (`Board Profile`):
/// `45` для `cucumber`, `36` для `plum`, `32` для `bergamot`, `pickle`, `yandexmini2`.
pub fn board_clock_segment_count(board_slug: &str) -> usize {
    match crate::normalize_board_slug(board_slug).as_str() {
        "plum" => 36,
        "bergamot" | "pickle" | "yandexmini2" => 32,
        _ => 45,
    }
}

/// Возвращает список активных канонических ключей сегментов для заданного Профиля платы (`Board Profile`).
pub fn board_clock_segment_names(board_slug: &str) -> Vec<String> {
    let norm = crate::normalize_board_slug(board_slug);
    let extras: &[&str] = match norm.as_str() {
        "plum" => &PLUM_CLOCK_EXTRA_SYMS,
        "bergamot" | "pickle" | "yandexmini2" => &COMPACT_32_CLOCK_EXTRA_SYMS,
        _ => &CLOCK_EXTRA_SYMS,
    };
    let mut names = Vec::with_capacity(28 + extras.len());
    for pos in 1..=4 {
        for part in ["a", "b", "c", "d", "e", "f", "g"] {
            names.push(format!("{}{}", part, pos));
        }
    }
    for &sym in extras {
        names.push(sym.to_string());
    }
    names
}

fn is_canonical_digit_segment(lower: &str) -> bool {
    if lower.len() == 2 {
        let bytes = lower.as_bytes();
        return (b'a'..=b'g').contains(&bytes[0]) && (b'1'..=b'4').contains(&bytes[1]);
    }
    false
}

/// Проверяет, существует ли канонический сегмент `canonical_seg` на заданном Профиле платы (`Board Profile`).
pub fn board_has_clock_segment(board_slug: &str, canonical_seg: &str) -> bool {
    let lower = canonical_seg.trim().to_ascii_lowercase();
    if is_canonical_digit_segment(&lower) {
        return true;
    }
    let norm = crate::normalize_board_slug(board_slug);
    match norm.as_str() {
        "plum" => PLUM_CLOCK_EXTRA_SYMS.contains(&lower.as_str()),
        "bergamot" | "pickle" | "yandexmini2" => {
            COMPACT_32_CLOCK_EXTRA_SYMS.contains(&lower.as_str())
        }
        _ => CLOCK_EXTRA_SYMS.contains(&lower.as_str()),
    }
}

/// Возвращает короткое нативное имя сегмента в прошивке (например, `"clockDM"`, `"clockMINUS"`, `"clockH0"`, `"clock-p4"`).
pub fn board_clock_native_short_name(board_slug: &str, canonical_seg: &str) -> Option<String> {
    if !board_has_clock_segment(board_slug, canonical_seg) {
        return None;
    }
    let seg = canonical_seg.trim().to_ascii_lowercase();
    let norm = crate::normalize_board_slug(board_slug);
    match norm.as_str() {
        "plum" => {
            if is_canonical_digit_segment(&seg) {
                let bytes = seg.as_bytes();
                let up = (bytes[0] as char).to_ascii_uppercase();
                let idx = bytes[1] - b'1';
                return Some(format!("clock{}{}", up, idx));
            }
            match seg.as_str() {
                "h1" => Some("clockH0".to_string()),
                "h2" => Some("clockH1".to_string()),
                "l1" => Some("clockH2".to_string()),
                "l2" => Some("clockH3".to_string()),
                "p3" => Some("clockDH".to_string()),
                "q3" => Some("clockDL".to_string()),
                "m1" => Some("clockDM".to_string()),
                "deg" => Some("clockDEGREE".to_string()),
                _ => None,
            }
        }
        "bergamot" | "pickle" => {
            if is_canonical_digit_segment(&seg) {
                let bytes = seg.as_bytes();
                let up = (bytes[0] as char).to_ascii_uppercase();
                let idx = bytes[1] - b'1';
                return Some(format!("clock{}{}", up, idx));
            }
            match seg.as_str() {
                "p3" => Some("clockDH".to_string()),
                "q3" => Some("clockDL".to_string()),
                "m1" => Some("clockDM".to_string()),
                "deg" => Some("clockDEGREE".to_string()),
                _ => None,
            }
        }
        "yandexmini2" => {
            if is_canonical_digit_segment(&seg) {
                let bytes = seg.as_bytes();
                let up = (bytes[0] as char).to_ascii_uppercase();
                let idx = bytes[1] - b'0';
                return Some(format!("clock{}{}", up, idx));
            }
            match seg.as_str() {
                "p3" => Some("clockPLUS1".to_string()),
                "q3" => Some("clockPLUS2".to_string()),
                "m1" => Some("clockMINUS".to_string()),
                "deg" => Some("clockDEGREE".to_string()),
                _ => None,
            }
        }
        _ => Some(format!("clock-{}", seg)),
    }
}

/// Возвращает полное нативное имя LED-узла в `/sys/class/leds/` (например, `"bergamot:white:clockDM"`,
/// `"mini2:white:clockMINUS"`, `"plum:white:clockH0"`, `"cucumber:white:clock-p4"`).
pub fn board_clock_native_led_name(board_slug: &str, canonical_seg: &str) -> Option<String> {
    let short = board_clock_native_short_name(board_slug, canonical_seg)?;
    let norm = crate::normalize_board_slug(board_slug);
    let prefix = crate::find_board_descriptor(&norm)
        .map(|b| b.sysfs_prefix)
        .unwrap_or(if norm.is_empty() {
            "cucumber"
        } else {
            norm.as_str()
        });
    Some(format!("{}:white:{}", prefix, short))
}

pub fn empty_clock_segments_map() -> BTreeMap<String, u8> {
    let mut map = BTreeMap::new();
    for name in canonical_clock_segment_names() {
        map.insert(name, 0);
    }
    map
}

/// Нормализует каноническое (`a1..g4`, `p3..deg`) или нативное (`A0..G3`, `DH`, `DL`, `DM`,
/// `PLUS1`, `PLUS2`, `MINUS`, `DEGREE`, `H0..H3`) имя сегмента в канонический ключ.
pub fn normalize_clock_segment_name(name: &str) -> Option<String> {
    let mut raw = name.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some((_, after_white)) = raw.split_once(":white:") {
        raw = after_white;
    }
    let has_clock_prefix = if let Some(rest) = raw.strip_prefix("clock-") {
        raw = rest;
        true
    } else if let Some(rest) = raw.strip_prefix("clock") {
        if !rest.is_empty() {
            raw = rest;
        }
        true
    } else {
        false
    };

    if has_clock_prefix || raw.starts_with('H') {
        match raw {
            "H0" => return Some("h1".to_string()),
            "H1" => return Some("h2".to_string()),
            "H2" => return Some("l1".to_string()),
            "H3" => return Some("l2".to_string()),
            _ => {}
        }
    }

    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "dh" | "plus1" => return Some("p3".to_string()),
        "dl" | "plus2" => return Some("q3".to_string()),
        "dm" | "minus" => return Some("m1".to_string()),
        "degree" => return Some("deg".to_string()),
        "h0" => return Some("h1".to_string()),
        "h3" => return Some("l2".to_string()),
        _ => {}
    }

    if CLOCK_EXTRA_SYMS.contains(&lower.as_str()) {
        return Some(lower);
    }
    if lower.len() == 2 {
        let bytes = lower.as_bytes();
        if (b'a'..=b'g').contains(&bytes[0]) {
            if (b'1'..=b'4').contains(&bytes[1]) {
                return Some(lower);
            }
            if bytes[1] == b'0' {
                return Some(format!("{}1", bytes[0] as char));
            }
        }
    }
    None
}

pub fn is_valid_clock_segment(name: &str) -> bool {
    normalize_clock_segment_name(name).is_some()
}

/// Разбирает строку перечисления сегментов вида `"a1,b1,deg"`, `"A0,DH,DM,DEGREE"` или `"a1=255, g2=128, deg:64"`.
pub fn parse_clock_segment_spec(spec: &str, default_brightness: u8) -> Option<BTreeMap<String, u8>> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("all") {
        let mut map = empty_clock_segments_map();
        for val in map.values_mut() {
            *val = default_brightness;
        }
        return Some(map);
    }
    let tokens: Vec<&str> = trimmed
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }

    let mut map = empty_clock_segments_map();
    for tok in tokens {
        let (seg_raw, val_opt) = if let Some((k, v)) = tok.split_once('=') {
            (k.trim(), Some(v.trim()))
        } else if let Some((k, v)) = tok.split_once(':') {
            (k.trim(), Some(v.trim()))
        } else {
            (tok.trim(), None)
        };
        let canonical = normalize_clock_segment_name(seg_raw)?;
        let br = if let Some(v_str) = val_opt {
            v_str.parse::<u16>().ok()?.min(255) as u8
        } else {
            default_brightness
        };
        map.insert(canonical, br);
    }
    Some(map)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedLedDisplayFrame {
    Matrix21x12(Vec<u8>),
    Segments45(BTreeMap<String, u8>),
}

/// Разбирает строку или файл формата Quasar `.led` для матрицы `21×12` (252 токена)
/// или 7-сегментного циферблата (28..45 токенов).
pub fn parse_display_led_frame(raw: &str, scale_brightness: u8) -> Option<ParsedLedDisplayFrame> {
    for line in raw.lines() {
        let clean = line.trim();
        if clean.is_empty() || clean.eq_ignore_ascii_case("loop") || clean.starts_with('#') {
            continue;
        }
        let mut parts: Vec<&str> = clean.split_whitespace().collect();
        if parts.len() < 28 {
            continue;
        }
        if parts.len() > 28 {
            if let Some(&last) = parts.last() {
                if last.len() != 6 && last.chars().all(|c| c.is_ascii_digit()) {
                    parts.pop();
                }
            }
        }
        let mut values = Vec::with_capacity(parts.len());
        let mut all_hex = true;
        for &tok in &parts {
            let hex = tok.trim_start_matches("0x").trim_start_matches('#');
            if hex.len() == 6 {
                if let Ok(rgb) = u32::from_str_radix(hex, 16) {
                    let r = ((rgb >> 16) & 0xff) as u8;
                    let g = ((rgb >> 8) & 0xff) as u8;
                    let b = (rgb & 0xff) as u8;
                    let luma = r.max(g).max(b);
                    let scaled = ((luma as u16 * scale_brightness as u16) / 255) as u8;
                    values.push(scaled);
                    continue;
                }
            } else if let Ok(v) = tok.parse::<u16>() {
                let luma = v.min(255) as u8;
                let scaled = ((luma as u16 * scale_brightness as u16) / 255) as u8;
                values.push(scaled);
                continue;
            }
            all_hex = false;
            break;
        }
        if !all_hex {
            continue;
        }
        if values.len() == 252 {
            return Some(ParsedLedDisplayFrame::Matrix21x12(values));
        } else if (28..=45).contains(&values.len()) {
            let names = canonical_clock_segment_names();
            let mut map = empty_clock_segments_map();
            for (name, val) in names.into_iter().zip(values.into_iter()) {
                map.insert(name, val);
            }
            return Some(ParsedLedDisplayFrame::Segments45(map));
        }
    }
    None
}

pub fn clock_segs_for_char(c: char) -> &'static [&'static str] {
    match c.to_ascii_uppercase() {
        '0' | 'O' => &["a", "b", "c", "d", "e", "f"],
        '1' | 'I' => &["b", "c"],
        '2' => &["a", "b", "d", "e", "g"],
        '3' => &["a", "b", "c", "d", "g"],
        '4' => &["b", "c", "f", "g"],
        '5' | 'S' => &["a", "c", "d", "f", "g"],
        '6' => &["a", "c", "d", "e", "f", "g"],
        '7' => &["a", "b", "c"],
        '8' => &["a", "b", "c", "d", "e", "f", "g"],
        '9' => &["a", "b", "c", "d", "f", "g"],
        '-' => &["g"],
        'E' => &["a", "d", "e", "f", "g"],
        'R' => &["e", "g"],
        'C' => &["a", "d", "e", "f"],
        'P' => &["a", "b", "e", "f", "g"],
        _ => &[],
    }
}

pub fn build_segments_map_from_text_for_board(
    board_slug: &str,
    text: &str,
    brightness: u8,
) -> BTreeMap<String, u8> {
    let mut map = empty_clock_segments_map();
    let trimmed = text.trim();
    if trimmed.is_empty() || brightness == 0 {
        return map;
    }

    if trimmed.eq_ignore_ascii_case("eyes") || trimmed.to_lowercase() == "глазки" {
        for pos in [1, 4] {
            for part in ["a", "b", "f", "g"] {
                map.insert(format!("{}{}", part, pos), brightness);
            }
        }
        return map;
    }

    let seg_count = board_clock_segment_count(board_slug);
    let mut char_iter = trimmed.chars().peekable();
    let leading_sign = match char_iter.peek().copied() {
        Some('+') | Some('-') => char_iter.next(),
        _ => None,
    };

    if let Some(sign) = leading_sign {
        let mut rest_chars = Vec::new();
        let mut has_colon = false;
        let mut has_degree = false;
        for ch in char_iter {
            match ch {
                ':' => has_colon = true,
                '°' | '*' => has_degree = true,
                _ => {
                    if rest_chars.len() < 4 {
                        rest_chars.push(ch);
                    }
                }
            }
        }
        if !has_colon
            && !rest_chars.is_empty()
            && rest_chars.iter().any(|c| c.is_ascii_alphanumeric())
        {
            if seg_count <= 36 && rest_chars.len() <= 2 {
                if sign == '+' {
                    map.insert("p3".to_string(), brightness);
                    map.insert("q3".to_string(), brightness);
                    map.insert("m1".to_string(), brightness);
                } else {
                    map.insert("m1".to_string(), brightness);
                }
                if rest_chars.len() == 2 {
                    for &part in clock_segs_for_char(rest_chars[0]) {
                        map.insert(format!("{}3", part), brightness);
                    }
                    for &part in clock_segs_for_char(rest_chars[1]) {
                        map.insert(format!("{}4", part), brightness);
                    }
                } else {
                    for &part in clock_segs_for_char(rest_chars[0]) {
                        map.insert(format!("{}4", part), brightness);
                    }
                }
                if has_degree {
                    map.insert("deg".to_string(), brightness);
                }
                return map;
            } else if seg_count == 45 {
                if sign == '+' {
                    map.insert("h1".to_string(), brightness);
                    map.insert("m1".to_string(), brightness);
                } else {
                    map.insert("m1".to_string(), brightness);
                }
                for (idx, &ch) in rest_chars.iter().take(4).enumerate() {
                    let pos = idx + 1;
                    for &part in clock_segs_for_char(ch) {
                        map.insert(format!("{}{}", part, pos), brightness);
                    }
                }
                if has_degree {
                    map.insert("deg".to_string(), brightness);
                }
                return map;
            }
        }
    }

    let mut chars = Vec::new();
    let mut colon = false;
    let mut degree = false;
    let mut plus = false;

    for ch in trimmed.chars() {
        match ch {
            ':' => colon = true,
            '°' | '*' => degree = true,
            '+' => plus = true,
            _ => {
                if chars.len() < 4 {
                    chars.push(ch);
                }
            }
        }
    }

    for (idx, &ch) in chars.iter().take(4).enumerate() {
        let pos = idx + 1;
        for &part in clock_segs_for_char(ch) {
            map.insert(format!("{}{}", part, pos), brightness);
        }
    }
    if colon {
        map.insert("p3".to_string(), brightness);
        map.insert("q3".to_string(), brightness);
    }
    if degree {
        map.insert("deg".to_string(), brightness);
    }
    if plus {
        if seg_count <= 36 {
            map.insert("p3".to_string(), brightness);
            map.insert("q3".to_string(), brightness);
            map.insert("m1".to_string(), brightness);
        } else {
            map.insert("h1".to_string(), brightness);
            map.insert("m1".to_string(), brightness);
        }
    }
    map
}

pub fn build_segments_map_from_text(text: &str, brightness: u8) -> BTreeMap<String, u8> {
    build_segments_map_from_text_for_board("cucumber", text, brightness)
}

/// 4×7 растровый шрифт цифр и символов (`OrionFrontPanelLedFrameBuilder`) для матрицы 21×12.
pub fn glyph_4x7(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        '0' | 'O' => [0b0110, 0b1001, 0b1001, 0b1001, 0b1001, 0b1001, 0b0110],
        '1' | 'I' => [0b0010, 0b0110, 0b0010, 0b0010, 0b0010, 0b0010, 0b0111],
        '2' => [0b0110, 0b1001, 0b0001, 0b0010, 0b0100, 0b1000, 0b1111],
        '3' => [0b1110, 0b0001, 0b0001, 0b0110, 0b0001, 0b0001, 0b1110],
        '4' => [0b1001, 0b1001, 0b1001, 0b1111, 0b0001, 0b0001, 0b0001],
        '5' | 'S' => [0b1111, 0b1000, 0b1000, 0b1110, 0b0001, 0b0001, 0b1110],
        '6' => [0b0111, 0b1000, 0b1000, 0b1110, 0b1001, 0b1001, 0b0110],
        '7' => [0b1111, 0b0001, 0b0010, 0b0010, 0b0100, 0b0100, 0b0100],
        '8' => [0b0110, 0b1001, 0b1001, 0b0110, 0b1001, 0b1001, 0b0110],
        '9' => [0b0110, 0b1001, 0b1001, 0b0111, 0b0001, 0b0001, 0b1110],
        '-' => [0b0000, 0b0000, 0b0000, 0b1111, 0b0000, 0b0000, 0b0000],
        '+' => [0b0000, 0b0100, 0b0100, 0b1110, 0b0100, 0b0100, 0b0000],
        _ => [0; 7],
    }
}

/// Рендерит текстовую строку (например, `"19:30"`, `"+21°"`, `"8"`) в буфер `21×12` (252 байта).
pub fn render_text_to_matrix_21x12(text: &str, brightness: u8) -> Vec<u8> {
    let mut grid = vec![0u8; 252];
    let trimmed = text.trim();
    if trimmed.is_empty() || brightness == 0 {
        return grid;
    }

    let mut chars = Vec::new();
    let mut has_colon = false;
    let mut has_degree = false;
    for ch in trimmed.chars() {
        match ch {
            ':' => has_colon = true,
            '°' | '*' => has_degree = true,
            _ => {
                if chars.len() < 4 {
                    chars.push(ch);
                }
            }
        }
    }

    let y0 = 2usize;
    let put_px = |grid: &mut [u8], x: usize, y: usize, val: u8| {
        if x < 21 && y < 12 {
            grid[y * 21 + x] = val;
        }
    };

    let x_positions: &[usize] = match (chars.len(), has_colon) {
        (4, true) => &[0, 5, 12, 17],
        (4, false) => &[0, 5, 10, 15],
        (3, _) => &[1, 6, 11],
        (2, _) => &[4, 10],
        (1, _) => &[8],
        _ => &[],
    };

    for (idx, &ch) in chars.iter().enumerate() {
        let Some(&x0) = x_positions.get(idx) else {
            break;
        };
        let rows = glyph_4x7(ch);
        for (ry, &bits) in rows.iter().enumerate() {
            for rx in 0..4 {
                if (bits & (1 << (3 - rx))) != 0 {
                    put_px(&mut grid, x0 + rx, y0 + ry, brightness);
                }
            }
        }
    }

    if has_colon {
        put_px(&mut grid, 10, y0 + 2, brightness);
        put_px(&mut grid, 10, y0 + 4, brightness);
    }

    if has_degree {
        if chars.len() >= 4 {
            put_px(&mut grid, 20, 0, brightness);
        } else {
            let deg_x = x_positions
                .last()
                .map(|&x| (x + 5).min(18))
                .unwrap_or(16);
            put_px(&mut grid, deg_x + 1, y0, brightness);
            put_px(&mut grid, deg_x, y0 + 1, brightness);
            put_px(&mut grid, deg_x + 2, y0 + 1, brightness);
            put_px(&mut grid, deg_x + 1, y0 + 2, brightness);
        }
    }

    grid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_segments_matrix_and_led_frame_parsing() {
        let names = canonical_clock_segment_names();
        assert_eq!(names.len(), 45);

        let spec = parse_clock_segment_spec("a1, b2=128, deg:64", 200).unwrap();
        assert_eq!(spec.get("a1"), Some(&200));
        assert_eq!(spec.get("b2"), Some(&128));
        assert_eq!(spec.get("deg"), Some(&64));
        assert_eq!(spec.get("c1"), Some(&0));

        let map_time = build_segments_map_from_text("19:30°", 255);
        assert_eq!(map_time.get("b1"), Some(&255));
        assert_eq!(map_time.get("p3"), Some(&255));
        assert_eq!(map_time.get("deg"), Some(&255));
        assert_eq!(map_time.get("h1"), Some(&0));

        let map_temp = build_segments_map_from_text("+24°", 200);
        assert_eq!(map_temp.get("h1"), Some(&200));
        assert_eq!(map_temp.get("m1"), Some(&200));
        assert_eq!(map_temp.get("deg"), Some(&200));

        let grid = render_text_to_matrix_21x12("19:30°", 255);
        assert_eq!(grid.len(), 252);
        assert!(grid.iter().filter(|&&px| px > 0).count() > 25);
        assert_eq!(grid[4 * 21 + 10], 255);
        assert_eq!(grid[6 * 21 + 10], 255);
        assert_eq!(grid[20], 255);

        let mut tokens45 = vec!["000000"; 45];
        tokens45[0] = "ffffff";
        tokens45[44] = "808080";
        let raw_seg_led = format!("loop\n{} 40\n", tokens45.join(" "));
        match parse_display_led_frame(&raw_seg_led, 255) {
            Some(ParsedLedDisplayFrame::Segments45(map)) => {
                assert_eq!(map.get("a1"), Some(&255));
                assert_eq!(map.get("deg"), Some(&128));
                assert_eq!(map.get("b1"), Some(&0));
            }
            _ => panic!("Expected Segments45 frame"),
        }

        let mut tokens252 = vec!["000000"; 252];
        tokens252[0] = "ffffff";
        tokens252[251] = "404040";
        let raw_mat_led = format!("loop\n{} 35\n", tokens252.join(" "));
        match parse_display_led_frame(&raw_mat_led, 255) {
            Some(ParsedLedDisplayFrame::Matrix21x12(px)) => {
                assert_eq!(px.len(), 252);
                assert_eq!(px[0], 255);
                assert_eq!(px[251], 64);
            }
            _ => panic!("Expected Matrix21x12 frame"),
        }
    }

    #[test]
    fn test_board_clock_profiles_and_native_names() {
        assert_eq!(board_clock_segment_count("cucumber"), 45);
        assert_eq!(board_clock_segment_count("plum"), 36);
        assert_eq!(board_clock_segment_count("bergamot"), 32);
        assert_eq!(board_clock_segment_count("pickle"), 32);
        assert_eq!(board_clock_segment_count("yandexmini2"), 32);

        let cuc_names = board_clock_segment_names("cucumber");
        assert_eq!(cuc_names.len(), 45);
        assert!(board_has_clock_segment("cucumber", "p4"));
        assert!(board_has_clock_segment("cucumber", "q7"));
        assert!(board_has_clock_segment("cucumber", "m2"));
        assert_eq!(
            board_clock_native_led_name("cucumber", "p4").as_deref(),
            Some("cucumber:white:clock-p4")
        );
        assert_eq!(
            board_clock_native_short_name("cucumber", "p4").as_deref(),
            Some("clock-p4")
        );

        let plum_names = board_clock_segment_names("plum");
        assert_eq!(plum_names.len(), 36);
        assert!(board_has_clock_segment("plum", "h1"));
        assert!(board_has_clock_segment("plum", "l2"));
        assert!(!board_has_clock_segment("plum", "p4"));
        assert!(!board_has_clock_segment("plum", "q4"));
        assert!(!board_has_clock_segment("plum", "m2"));
        assert_eq!(
            board_clock_native_led_name("plum", "a1").as_deref(),
            Some("plum:white:clockA0")
        );
        assert_eq!(
            board_clock_native_led_name("plum", "g4").as_deref(),
            Some("plum:white:clockG3")
        );
        assert_eq!(
            board_clock_native_led_name("plum", "h1").as_deref(),
            Some("plum:white:clockH0")
        );
        assert_eq!(
            board_clock_native_led_name("plum", "l2").as_deref(),
            Some("plum:white:clockH3")
        );
        assert_eq!(
            board_clock_native_led_name("plum", "m1").as_deref(),
            Some("plum:white:clockDM")
        );
        assert_eq!(board_clock_native_led_name("plum", "p4"), None);

        for slug in ["bergamot", "pickle"] {
            let names = board_clock_segment_names(slug);
            assert_eq!(names.len(), 32);
            assert!(!board_has_clock_segment(slug, "h1"));
            assert!(!board_has_clock_segment(slug, "p4"));
            assert_eq!(
                board_clock_native_led_name(slug, "p3").as_deref(),
                Some(format!("{}:white:clockDH", slug)).as_deref()
            );
            assert_eq!(
                board_clock_native_led_name(slug, "q3").as_deref(),
                Some(format!("{}:white:clockDL", slug)).as_deref()
            );
            assert_eq!(
                board_clock_native_led_name(slug, "m1").as_deref(),
                Some(format!("{}:white:clockDM", slug)).as_deref()
            );
            assert_eq!(
                board_clock_native_led_name(slug, "deg").as_deref(),
                Some(format!("{}:white:clockDEGREE", slug)).as_deref()
            );
        }

        let mini2_names = board_clock_segment_names("yandexmini2");
        assert_eq!(mini2_names.len(), 32);
        assert!(!board_has_clock_segment("yandexmini2", "h1"));
        assert_eq!(
            board_clock_native_led_name("yandexmini2", "a1").as_deref(),
            Some("mini2:white:clockA1")
        );
        assert_eq!(
            board_clock_native_led_name("yandexmini2", "g4").as_deref(),
            Some("mini2:white:clockG4")
        );
        assert_eq!(
            board_clock_native_led_name("yandexmini2", "p3").as_deref(),
            Some("mini2:white:clockPLUS1")
        );
        assert_eq!(
            board_clock_native_led_name("yandexmini2", "q3").as_deref(),
            Some("mini2:white:clockPLUS2")
        );
        assert_eq!(
            board_clock_native_led_name("yandexmini2", "m1").as_deref(),
            Some("mini2:white:clockMINUS")
        );
        assert_eq!(
            board_clock_native_led_name("yandexmini2", "deg").as_deref(),
            Some("mini2:white:clockDEGREE")
        );

        let native_spec = parse_clock_segment_spec(
            "A0=210, G3=190, DH=180, DL=170, DM=160, PLUS1=150, PLUS2=140, MINUS=130, DEGREE=120, H0=110, H1=100, H2=90, H3=80",
            255,
        )
        .unwrap();
        assert_eq!(native_spec.get("a1"), Some(&210));
        assert_eq!(native_spec.get("g3"), Some(&190));
        assert_eq!(native_spec.get("p3"), Some(&150));
        assert_eq!(native_spec.get("q3"), Some(&140));
        assert_eq!(native_spec.get("m1"), Some(&130));
        assert_eq!(native_spec.get("deg"), Some(&120));
        assert_eq!(native_spec.get("h1"), Some(&110));
        assert_eq!(native_spec.get("h2"), Some(&100));
        assert_eq!(native_spec.get("l1"), Some(&90));
        assert_eq!(native_spec.get("l2"), Some(&80));
    }

    #[test]
    fn test_build_segments_map_from_text_for_board_temperatures_and_eyes() {
        for slug in ["bergamot", "pickle", "yandexmini2", "plum"] {
            let plus_map = build_segments_map_from_text_for_board(slug, "+24°", 220);
            // Central separator + (p3 + q3 + m1), left sign h1 off
            assert_eq!(plus_map.get("p3"), Some(&220), "p3 on {slug}");
            assert_eq!(plus_map.get("q3"), Some(&220), "q3 on {slug}");
            assert_eq!(plus_map.get("m1"), Some(&220), "m1 on {slug}");
            assert_eq!(plus_map.get("h1"), Some(&0), "h1 off on {slug}");
            assert_eq!(plus_map.get("deg"), Some(&220), "deg on {slug}");
            // Digits 1 and 2 off, digits 3 ('2') and 4 ('4') lit
            assert_eq!(plus_map.get("a1"), Some(&0));
            assert_eq!(plus_map.get("a2"), Some(&0));
            assert_eq!(plus_map.get("a3"), Some(&220));
            assert_eq!(plus_map.get("g3"), Some(&220));
            assert_eq!(plus_map.get("b4"), Some(&220));
            assert_eq!(plus_map.get("f4"), Some(&220));

            let minus_map = build_segments_map_from_text_for_board(slug, "-05°", 180);
            assert_eq!(minus_map.get("p3"), Some(&0));
            assert_eq!(minus_map.get("q3"), Some(&0));
            assert_eq!(minus_map.get("m1"), Some(&180));
            assert_eq!(minus_map.get("deg"), Some(&180));
            // '0' on digit 3, '5' on digit 4
            assert_eq!(minus_map.get("a1"), Some(&0));
            assert_eq!(minus_map.get("a3"), Some(&180));
            assert_eq!(minus_map.get("g3"), Some(&0));
            assert_eq!(minus_map.get("a4"), Some(&180));
            assert_eq!(minus_map.get("g4"), Some(&180));
        }

        // Cucumber (45 segments): left sign block h1+m1 for +, m1 for -, digits on pos 1..2
        let cuc_plus = build_segments_map_from_text_for_board("cucumber", "+24°", 200);
        assert_eq!(cuc_plus.get("h1"), Some(&200));
        assert_eq!(cuc_plus.get("m1"), Some(&200));
        assert_eq!(cuc_plus.get("p3"), Some(&0));
        assert_eq!(cuc_plus.get("q3"), Some(&0));
        assert_eq!(cuc_plus.get("a1"), Some(&200));
        assert_eq!(cuc_plus.get("b2"), Some(&200));
        assert_eq!(cuc_plus.get("deg"), Some(&200));

        let cuc_minus = build_segments_map_from_text_for_board("cucumber", "-05°", 200);
        assert_eq!(cuc_minus.get("h1"), Some(&0));
        assert_eq!(cuc_minus.get("m1"), Some(&200));
        assert_eq!(cuc_minus.get("a1"), Some(&200));
        assert_eq!(cuc_minus.get("a2"), Some(&200));
        assert_eq!(cuc_minus.get("deg"), Some(&200));

        // Preset "eyes" / "глазки": upper squares of digits 1 and 4 (a1,b1,f1,g1 + a4,b4,f4,g4)
        for preset in ["eyes", "EYES", "глазки", "Глазки"] {
            for slug in ["cucumber", "plum", "bergamot", "pickle", "yandexmini2"] {
                let eyes = build_segments_map_from_text_for_board(slug, preset, 240);
                for lit in ["a1", "b1", "f1", "g1", "a4", "b4", "f4", "g4"] {
                    assert_eq!(eyes.get(lit), Some(&240), "{lit} should be lit for {preset} on {slug}");
                }
                for off in ["c1", "d1", "e1", "a2", "a3", "c4", "d4", "e4", "p3", "q3", "m1", "deg"] {
                    assert_eq!(eyes.get(off), Some(&0), "{off} should be off for {preset} on {slug}");
                }
            }
        }
    }
}
