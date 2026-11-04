use crate::state::{now_millis, AliceState, BridgeState};
use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const LEDPATTERNS_ROOT: &str = "/system/vendor/quasar/ledpatterns";

pub fn parse_led_pattern_str(raw: &str, ring_leds: usize) -> Vec<(Vec<[u8; 3]>, u32)> {
    protocol::parse_led_pattern_frames(raw, ring_leds)
}

fn collect_led_files_recursive(
    root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<protocol::LedPatternInfo>,
) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_led_files_recursive(root, &path, depth + 1, out);
        } else if path.is_file() {
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel_id = rel.to_string_lossy().replace('\\', "/");
            if !protocol::is_supported_led_pattern_path(&rel_id) {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Some(info) = protocol::build_led_pattern_info(&rel_id, &raw) {
                    out.push(info);
                }
            }
        }
    }
}

pub fn discover_guest_led_patterns() -> Vec<protocol::LedPatternInfo> {
    let root = Path::new(LEDPATTERNS_ROOT);
    let mut out = Vec::new();
    if root.is_dir() {
        collect_led_files_recursive(root, root, 0, &mut out);
        out.sort_by(|a, b| a.id.cmp(&b.id));
    }
    out
}

fn split_led_pattern_kind(kind: &str) -> (&str, &str, &str) {
    let trimmed = kind.trim().trim_start_matches('/');
    let stem = trimmed.trim_end_matches(".led");
    let base_stem = stem.rsplit('/').next().unwrap_or(stem);
    (trimmed, stem, base_stem)
}

fn read_first_matching_pattern(
    candidates: &[String],
    leds: usize,
) -> Option<Vec<(Vec<[u8; 3]>, u32)>> {
    for path in candidates {
        if let Ok(raw) = fs::read_to_string(path) {
            let frames = parse_led_pattern_str(&raw, leds);
            if !frames.is_empty() {
                return Some(frames);
            }
        }
    }
    None
}

fn load_led_pattern(kind: &str, ring_leds: usize) -> Vec<(Vec<[u8; 3]>, u32)> {
    let (trimmed, stem, base_stem) = split_led_pattern_kind(kind);

    let mut candidates = vec![
        format!("{}/{}", LEDPATTERNS_ROOT, trimmed),
        format!("{}/{}.led", LEDPATTERNS_ROOT, stem),
        format!("{}/ring/{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/panel/{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/oknix/{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/ambilight/{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/ring/alice_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/panel/alice_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/oknix/alice_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/alice_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/ring/alice_pro_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/oknix/alice_pro_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/alice_pro_{}.led", LEDPATTERNS_ROOT, base_stem),
    ];

    if base_stem == "mute_mics" || base_stem == "mute" {
        for alt in ["mute_mics", "mute"] {
            candidates.push(format!("{}/ring/{}.led", LEDPATTERNS_ROOT, alt));
            candidates.push(format!("{}/panel/{}.led", LEDPATTERNS_ROOT, alt));
            candidates.push(format!("{}/oknix/{}.led", LEDPATTERNS_ROOT, alt));
            candidates.push(format!("{}/ambilight/{}.led", LEDPATTERNS_ROOT, alt));
            candidates.push(format!("{}/{}.led", LEDPATTERNS_ROOT, alt));
        }
    }

    if let Some(mut frames) = read_first_matching_pattern(&candidates, ring_leds) {
        if base_stem == "mute_mics" || base_stem == "mute" {
            let non_black: Vec<(Vec<[u8; 3]>, u32)> = frames
                .iter()
                .filter(|(f, _)| f.iter().any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0))
                .cloned()
                .collect();
            if !non_black.is_empty() {
                frames = non_black;
            }
        }
        return frames;
    }

    if base_stem == "mute_mics" || base_stem == "mute" {
        return vec![(vec![[255, 0, 0]; ring_leds], 100)];
    }
    if base_stem == "alice_pro_listening" || base_stem == "alice_pro_speaking" {
        let mut pro_fallback = Vec::new();
        for f in 0..20 {
            let mut px = Vec::with_capacity(ring_leds);
            for i in 0..ring_leds {
                let phase = ((i + f) % ring_leds.max(1)) as f32 / (ring_leds.max(1) as f32);
                let g = (120.0 + 135.0 * phase) as u8;
                let b = (80.0 + 90.0 * (1.0 - phase)) as u8;
                px.push([0, g, b]);
            }
            pro_fallback.push((px, 30));
        }
        return pro_fallback;
    }
    if base_stem == "error" {
        return vec![(vec![[255, 30, 0]; ring_leds], 80)];
    }

    let mut fallback = Vec::new();
    for f in 0..20 {
        let mut px = Vec::with_capacity(ring_leds);
        for i in 0..ring_leds {
            let phase = ((i + f) % ring_leds.max(1)) as f32 / (ring_leds.max(1) as f32);
            let r = (107.0 * (0.4 + 0.6 * phase)) as u8;
            let g = (15.0 + 90.0 * (1.0 - phase)) as u8;
            let b = 255u8;
            px.push([r, g, b]);
        }
        fallback.push((px, 30));
    }
    fallback
}

fn load_ambilight_pattern(kind: &str, zones: usize) -> Vec<(Vec<[u8; 3]>, u32)> {
    if zones == 0 {
        return Vec::new();
    }
    let (_, _, base_stem) = split_led_pattern_kind(kind);

    let mut candidates = vec![
        format!("{}/oknix/{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/oknix/alice_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/oknix/alice_pro_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/ambilight/{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/ambilight/alice_{}.led", LEDPATTERNS_ROOT, base_stem),
        format!("{}/ambilight/alice_pro_{}.led", LEDPATTERNS_ROOT, base_stem),
    ];
    if base_stem == "mute_mics" || base_stem == "mute" {
        for alt in ["mute_mics", "mute"] {
            candidates.push(format!("{}/oknix/{}.led", LEDPATTERNS_ROOT, alt));
            candidates.push(format!("{}/ambilight/{}.led", LEDPATTERNS_ROOT, alt));
        }
    }
    if base_stem == "violet" {
        candidates.push(format!("{}/oknix/alice_speaking.led", LEDPATTERNS_ROOT));
        candidates.push(format!("{}/oknix/boot.led", LEDPATTERNS_ROOT));
        candidates.push(format!("{}/ambilight/boot.led", LEDPATTERNS_ROOT));
    }

    read_first_matching_pattern(&candidates, zones)
        .unwrap_or_else(|| load_led_pattern(kind, zones))
}

fn build_mute_led_frame(
    mute_frames: &[(Vec<[u8; 3]>, u32)],
    ring_leds: usize,
) -> Vec<[u8; 3]> {
    if let Some((frame, _)) = mute_frames
        .iter()
        .rev()
        .find(|(f, _)| f.first().copied() == Some([255, 0, 0]))
    {
        return frame.clone();
    }
    vec![[255, 0, 0]; ring_leds]
}

fn build_volume_led_frame(
    volume_frames: &[(Vec<[u8; 3]>, u32)],
    vol_level: u32,
    ring_leds: usize,
) -> Vec<[u8; 3]> {
    let clamped = vol_level.min(10);
    if !volume_frames.is_empty() {
        let idx = ((clamped as usize) * (volume_frames.len() - 1)) / 10;
        let frame = &volume_frames[idx].0;
        if clamped == 0
            || frame
                .first()
                .map(|p| p[0] > 0 || p[1] > 0 || p[2] > 0)
                .unwrap_or(false)
        {
            return frame.clone();
        }
    }
    let lit = ((clamped as usize) * ring_leds).div_ceil(10);
    let color = match clamped {
        0 => [0, 0, 0],
        1..=3 => [0, 255, 69],
        4..=6 => [255, 219, 49],
        7..=8 => [255, 129, 0],
        _ => [225, 0, 0],
    };
    let mut px = vec![[0u8; 3]; ring_leds];
    for item in px.iter_mut().take(lit.min(ring_leds)) {
        *item = color;
    }
    px
}

fn parse_rgb_hex_pattern(forced: &str) -> Option<[u8; 3]> {
    let hex = forced.strip_prefix("rgb:")?;
    let clean = hex.trim_start_matches('#');
    let rgb = u32::from_str_radix(clean, 16).unwrap_or(0);
    Some([
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    ])
}

pub fn parse_rgb_frame_pattern(forced: &str, ring_leds: usize) -> Option<Vec<[u8; 3]>> {
    let raw_tokens = forced.trim().strip_prefix("frame:")?;
    parse_led_pattern_str(raw_tokens, ring_leds)
        .into_iter()
        .next()
        .map(|(px, _)| px)
}

fn apply_native_frames(state: &BridgeState, ring_px: Vec<[u8; 3]>, aux_px: Option<Vec<[u8; 3]>>) {
    state.peripherals.ring.write_native_frame(&ring_px);
    *state.native_led_frame.lock().unwrap() = ring_px;

    let aux_zones = state.peripherals.aux_light.zone_count();
    if aux_zones > 0 {
        if let Some(zones) = aux_px {
            state.peripherals.aux_light.write_native_frame(&zones);
            *state.native_aux_frame.lock().unwrap() = zones;
        } else {
            *state.native_aux_frame.lock().unwrap() = vec![[0u8; 3]; aux_zones];
        }
    }
}

pub fn sync_immediate_led_state(state: &BridgeState) {
    let ring_leds = state.ring_leds;
    let aux_zones = state.peripherals.aux_light.zone_count();
    let now = now_millis();
    let vol_until = state.volume_overlay_until_ms.load(Ordering::Relaxed);
    let forced = state.forced_led_pattern.lock().unwrap().clone();

    if now < vol_until {
        let lvl = state.volume_level.load(Ordering::Relaxed);
        let volume_frames = load_led_pattern("volume", ring_leds);
        let px = build_volume_led_frame(&volume_frames, lvl, ring_leds);
        apply_native_frames(state, px, None);
        state.peripherals.front_display.render_volume_overlay(lvl, true);
        return;
    }

    if !forced.is_empty() {
        if let Some(c) = parse_rgb_hex_pattern(&forced) {
            let aux = if aux_zones > 0 {
                Some(vec![c; aux_zones])
            } else {
                None
            };
            apply_native_frames(state, vec![c; ring_leds], aux);
            return;
        }
        if let Some(px) = parse_rgb_frame_pattern(&forced, ring_leds) {
            apply_native_frames(state, px, None);
            return;
        }
        let frames = load_led_pattern(&forced, ring_leds);
        let aux_frames = load_ambilight_pattern(&forced, aux_zones);
        let aux_px = aux_frames.first().map(|(px, _)| px.clone());
        if let Some((px, _)) = frames.first() {
            apply_native_frames(state, px.clone(), aux_px);
        }
        return;
    }

    if state.full_mode {
        *state.native_led_frame.lock().unwrap() = vec![[0u8; 3]; ring_leds];
        if aux_zones > 0 {
            *state.native_aux_frame.lock().unwrap() = vec![[0u8; 3]; aux_zones];
        }
        return;
    }

    if state.mics_muted.load(Ordering::Relaxed) {
        let mute_frames = load_led_pattern("mute_mics", ring_leds);
        let px = build_mute_led_frame(&mute_frames, ring_leds);
        let aux = if aux_zones > 0 {
            Some(vec![[180, 0, 0]; aux_zones])
        } else {
            None
        };
        apply_native_frames(state, px, aux);
        return;
    }

    if state.alice_state() == AliceState::Idle {
        let aux_had_native = state
            .native_aux_frame
            .lock()
            .unwrap()
            .iter()
            .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        let aux = if aux_zones > 0 && aux_had_native {
            Some(vec![[0u8; 3]; aux_zones])
        } else {
            None
        };
        apply_native_frames(state, vec![[0u8; 3]; ring_leds], aux);
    }
}

pub fn spawn_native_led_animator(state: Arc<BridgeState>) {
    thread::spawn(move || {
        let ring_leds = state.ring_leds;
        let aux_zones = state.peripherals.aux_light.zone_count();
        let listening_frames = load_led_pattern("listening", ring_leds);
        let speaking_frames = load_led_pattern("speaking", ring_leds);
        let mute_frames = load_led_pattern("mute_mics", ring_leds);
        let volume_frames = load_led_pattern("volume", ring_leds);
        let ambilight_violet = load_ambilight_pattern("violet", aux_zones);

        let mut cached_forced_name = String::new();
        let mut cached_forced_ring: Vec<(Vec<[u8; 3]>, u32)> = Vec::new();
        let mut cached_forced_aux: Vec<(Vec<[u8; 3]>, u32)> = Vec::new();

        let mut frame_idx = 0usize;
        let mut was_active = false;
        let mut was_aux_active = false;

        loop {
            let now = now_millis();
            let vol_until = state.volume_overlay_until_ms.load(Ordering::Relaxed);
            let forced = state.forced_led_pattern.lock().unwrap().clone();

            // 1. Оверлей громкости (1.5 с: шкала на кольце + кадр громкости на матрице 21×12)
            if now < vol_until {
                was_active = true;
                let lvl = state.volume_level.load(Ordering::Relaxed);
                let px = build_volume_led_frame(&volume_frames, lvl, ring_leds);
                apply_native_frames(&state, px, None);
                state.peripherals.front_display.render_volume_overlay(lvl, true);
                thread::sleep(Duration::from_millis(45));
                continue;
            } else {
                state.peripherals.front_display.render_volume_overlay(0, false);
            }

            // 2. Ручной выбор паттерна / цвета из UI
            if !forced.is_empty() {
                was_active = true;
                if let Some(c) = parse_rgb_hex_pattern(&forced) {
                    let aux = if aux_zones > 0 {
                        was_aux_active = true;
                        Some(vec![c; aux_zones])
                    } else {
                        None
                    };
                    apply_native_frames(&state, vec![c; ring_leds], aux);
                    thread::sleep(Duration::from_millis(60));
                    continue;
                }
                if let Some(px) = parse_rgb_frame_pattern(&forced, ring_leds) {
                    apply_native_frames(&state, px, None);
                    thread::sleep(Duration::from_millis(60));
                    continue;
                }
                if cached_forced_name != forced || cached_forced_ring.is_empty() {
                    cached_forced_ring = load_led_pattern(&forced, ring_leds);
                    cached_forced_aux = load_ambilight_pattern(&forced, aux_zones);
                    cached_forced_name = forced.clone();
                    frame_idx = 0;
                }
                let (ref px, dur_ms) = cached_forced_ring[frame_idx % cached_forced_ring.len()];
                let aux = if aux_zones > 0 && !cached_forced_aux.is_empty() {
                    was_aux_active = true;
                    Some(cached_forced_aux[frame_idx % cached_forced_aux.len()].0.clone())
                } else {
                    None
                };
                apply_native_frames(&state, px.clone(), aux);
                frame_idx = frame_idx.wrapping_add(1);
                thread::sleep(Duration::from_millis(dur_ms as u64));
                continue;
            }

            if state.full_mode {
                if was_active {
                    was_active = false;
                    was_aux_active = false;
                    frame_idx = 0;
                    *state.native_led_frame.lock().unwrap() = vec![[0u8; 3]; ring_leds];
                    if aux_zones > 0 {
                        *state.native_aux_frame.lock().unwrap() = vec![[0u8; 3]; aux_zones];
                    }
                }
                thread::sleep(Duration::from_millis(80));
                continue;
            }

            // 3. Активный диалог Алисы (LISTENING / SPEAKING)
            match state.alice_state() {
                alice_state @ (AliceState::Listening | AliceState::Speaking) => {
                    was_active = true;
                    let frames = if alice_state == AliceState::Listening {
                        &listening_frames
                    } else {
                        &speaking_frames
                    };
                    let (ref px, dur_ms) = frames[frame_idx % frames.len()];
                    let aux = if aux_zones > 0 && !ambilight_violet.is_empty() {
                        was_aux_active = true;
                        Some(ambilight_violet[frame_idx % ambilight_violet.len()].0.clone())
                    } else {
                        None
                    };
                    apply_native_frames(&state, px.clone(), aux);
                    frame_idx = frame_idx.wrapping_add(1);
                    thread::sleep(Duration::from_millis(dur_ms as u64));
                    continue;
                }
                AliceState::Idle => {}
            }

            // 4. Отключённые микрофоны (MUTE MIC -> красное кольцо)
            if state.mics_muted.load(Ordering::Relaxed) {
                was_active = true;
                let px = build_mute_led_frame(&mute_frames, ring_leds);
                let aux = if aux_zones > 0 {
                    was_aux_active = true;
                    Some(vec![[180, 0, 0]; aux_zones])
                } else {
                    None
                };
                apply_native_frames(&state, px, aux);
                thread::sleep(Duration::from_millis(80));
                continue;
            }

            // 5. Состояние покоя (IDLE)
            if was_active {
                was_active = false;
                frame_idx = 0;
                let aux = if aux_zones > 0 && was_aux_active {
                    was_aux_active = false;
                    Some(vec![[0u8; 3]; aux_zones])
                } else {
                    None
                };
                apply_native_frames(&state, vec![[0u8; 3]; ring_leds], aux);
            }
            thread::sleep(Duration::from_millis(40));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_led_pattern_str() {
        let sample = "loop\n6b0fff 6b0fff 6b0fff 6b0fff 6b0fff 6b0fff 20\n00ff80 00ff80 35\n";
        let frames = parse_led_pattern_str(sample, 6);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].0.len(), 6);
        assert_eq!(frames[0].0[0], [0x6b, 0x0f, 0xff]);
        assert_eq!(frames[0].1, 20);
        assert_eq!(frames[1].0[0], [0x00, 0xff, 0x80]);
        assert_eq!(frames[1].1, 35);

        let rgbw_sample = "710ccc00 009f0020 20\n";
        let rgbw_frames = parse_led_pattern_str(rgbw_sample, 88);
        assert_eq!(rgbw_frames.len(), 1);
        assert_eq!(rgbw_frames[0].0.len(), 88);
        assert_eq!(rgbw_frames[0].0[0], [0x71, 0x0c, 0xcc]);
        assert_eq!(rgbw_frames[0].0[1], [0x10, 0xaf, 0x10]);
    }

    #[test]
    fn test_volume_led_arc_fallback() {
        let px0 = build_volume_led_frame(&[], 0, 6);
        assert_eq!(px0, vec![[0, 0, 0]; 6]);
        let px5 = build_volume_led_frame(&[], 5, 6);
        assert_eq!(px5[0], [255, 219, 49]);
        assert_eq!(px5[2], [255, 219, 49]);
        assert_eq!(px5[3], [0, 0, 0]);
        let px10 = build_volume_led_frame(&[], 10, 6);
        assert_eq!(px10, vec![[225, 0, 0]; 6]);
    }

    #[test]
    fn test_parse_rgb_hex_pattern() {
        assert_eq!(parse_rgb_hex_pattern("rgb:#00E5FF"), Some([0x00, 0xE5, 0xFF]));
        assert_eq!(parse_rgb_hex_pattern("rgb:ff1020"), Some([0xFF, 0x10, 0x20]));
        assert_eq!(parse_rgb_hex_pattern("listening"), None);
    }

    #[test]
    fn test_parse_rgb_frame_pattern_station2_88_leds() {
        let mut tokens = Vec::with_capacity(88);
        for i in 0..88usize {
            if i == 0 {
                tokens.push("6b0fff".to_string());
            } else if i == 1 {
                tokens.push("00ff80".to_string());
            } else if i == 87 {
                tokens.push("ff9933".to_string());
            } else {
                tokens.push(format!("{:02x}{:02x}{:02x}", i as u8, (i * 2) as u8, 255 - i as u8));
            }
        }
        let cmd = format!("frame:{}", tokens.join(" "));
        let parsed = parse_rgb_frame_pattern(&cmd, 88).expect("should parse 88-LED frame");
        assert_eq!(parsed.len(), 88);
        assert_eq!(parsed[0], [0x6B, 0x0F, 0xFF]);
        assert_eq!(parsed[1], [0x00, 0xFF, 0x80]);
        assert_eq!(parsed[87], [0xFF, 0x99, 0x33]);

        assert_eq!(parse_rgb_frame_pattern("rgb:#6B0FFF", 88), None);
        assert_eq!(parse_rgb_frame_pattern("panel/candle.led", 88), None);
        assert_eq!(parse_rgb_frame_pattern("frame:", 88), None);
    }
}
