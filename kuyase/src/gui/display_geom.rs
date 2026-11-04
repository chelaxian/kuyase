use eframe::egui::{Pos2, Rect};
pub use protocol::default_board_ring_leds;
pub use protocol::display::{
    build_segments_map_from_text as build_local_segments_from_text, render_text_to_matrix_21x12,
};
use protocol::BridgeStateSnapshot;
use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, TAU};

pub fn led_ring_position(center: Pos2, radius: f32, index: usize, count: usize) -> Pos2 {
    if count <= 1 {
        return center;
    }
    let angle = -FRAC_PI_2 + TAU * (index as f32) / (count as f32);
    Pos2::new(
        center.x + radius * angle.cos(),
        center.y + radius * angle.sin(),
    )
}

pub fn doa_angle_from_pointer(center: Pos2, pointer: Pos2) -> u32 {
    let dx = pointer.x - center.x;
    let dy = pointer.y - center.y;
    let deg = dy.atan2(dx).to_degrees() + 90.0;
    (deg.rem_euclid(360.0).round() as u32) % 360
}

pub fn segment_brightness(segments: &BTreeMap<String, u8>, key: &str) -> u8 {
    segments.get(key).copied().unwrap_or(0)
}

pub fn board_has_segment_clock(slug: &str, st: &BridgeStateSnapshot) -> bool {
    st.display_kind == "segment_clock" || protocol::board_has_segment_clock(slug, st.has_clock)
}

pub fn board_has_matrix_21x12(slug: &str, st: &BridgeStateSnapshot) -> bool {
    protocol::board_has_matrix_21x12(slug, &st.display_kind)
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let len_sq = ab.length_sq();
    if len_sq <= 1e-5 {
        return ap.length();
    }
    let t = (ap.dot(ab) / len_sq).clamp(0.0, 1.0);
    let proj = a + ab * t;
    p.distance(proj)
}

pub fn clock_segment_alias_info(seg: &str) -> Option<&'static str> {
    match seg {
        "h1" => Some("clockH0"),
        "h2" => Some("clockH1"),
        "l1" => Some("clockH2"),
        "l2" => Some("clockH3"),
        _ => None,
    }
}

pub fn clock_special_positions(rect: Rect) -> (Pos2, Pos2, Pos2) {
    let digit_h = 40.0;
    let start_x = rect.center().x - 66.0;
    let top_y = rect.center().y - digit_h * 0.5;
    let colon_x = start_x + 2.0 * 34.0 - 1.0;

    let p3_pos = Pos2::new(colon_x, top_y + 13.0);
    let q3_pos = Pos2::new(colon_x, top_y + 27.0);
    let deg_pos = Pos2::new(start_x + 4.0 * 34.0 + 14.0, top_y + 6.0);
    (p3_pos, q3_pos, deg_pos)
}

pub fn clock_segment_lines(rect: Rect) -> Vec<(String, Pos2, Pos2)> {
    let digit_w = 24.0;
    let digit_h = 40.0;
    let start_x = rect.center().x - 66.0;
    let top_y = rect.center().y - digit_h * 0.5;
    let ym = top_y + digit_h * 0.5;
    let y1 = top_y + digit_h;

    let mut out = Vec::with_capacity(42);
    let plus_cx = start_x - 18.0;
    // Left sign/icon block (6 segments: h2, h1, m1, m2, l1, l2 with non-overlapping midpoints)
    out.push((
        "h2".to_string(),
        Pos2::new(plus_cx - 7.0, top_y - 1.0),
        Pos2::new(plus_cx + 7.0, top_y - 1.0),
    ));
    out.push((
        "h1".to_string(),
        Pos2::new(plus_cx, ym - 11.0),
        Pos2::new(plus_cx, ym - 2.0),
    ));
    out.push((
        "m1".to_string(),
        Pos2::new(plus_cx - 7.0, ym),
        Pos2::new(plus_cx + 7.0, ym),
    ));
    out.push((
        "m2".to_string(),
        Pos2::new(plus_cx, ym + 2.0),
        Pos2::new(plus_cx, ym + 10.0),
    ));
    out.push((
        "l1".to_string(),
        Pos2::new(plus_cx - 7.0, ym + 15.0),
        Pos2::new(plus_cx + 7.0, ym + 15.0),
    ));
    out.push((
        "l2".to_string(),
        Pos2::new(plus_cx - 7.0, y1 + 1.0),
        Pos2::new(plus_cx + 7.0, y1 + 1.0),
    ));

    for idx in 0..4 {
        let pos = idx + 1;
        let extra_idx = idx + 4;
        let x0 = start_x + (idx as f32) * 34.0 + if idx >= 2 { 12.0 } else { 0.0 };
        let x1 = x0 + digit_w;
        let coords = [
            ("a", Pos2::new(x0 + 2.5, top_y), Pos2::new(x1 - 2.5, top_y)),
            ("b", Pos2::new(x1, top_y + 2.5), Pos2::new(x1, ym - 1.5)),
            ("c", Pos2::new(x1, ym + 1.5), Pos2::new(x1, y1 - 2.5)),
            ("d", Pos2::new(x0 + 2.5, y1), Pos2::new(x1 - 2.5, y1)),
            ("e", Pos2::new(x0, ym + 1.5), Pos2::new(x0, y1 - 2.5)),
            ("f", Pos2::new(x0, top_y + 2.5), Pos2::new(x0, ym - 1.5)),
            ("g", Pos2::new(x0 + 2.5, ym), Pos2::new(x1 - 2.5, ym)),
        ];
        for (part, p0, p1) in coords {
            out.push((format!("{}{}", part, pos), p0, p1));
        }
        // Upper (p4..p7) and lower (q4..q7) indicator segments above/below digits 1..4
        out.push((
            format!("p{}", extra_idx),
            Pos2::new(x0 + 4.0, top_y - 6.5),
            Pos2::new(x1 - 4.0, top_y - 6.5),
        ));
        out.push((
            format!("q{}", extra_idx),
            Pos2::new(x0 + 4.0, y1 + 6.5),
            Pos2::new(x1 - 4.0, y1 + 6.5),
        ));
    }
    out
}

pub fn clock_segment_center(rect: Rect, name: &str) -> Option<Pos2> {
    let (p3_pos, q3_pos, deg_pos) = clock_special_positions(rect);
    match name {
        "p3" => return Some(p3_pos),
        "q3" => return Some(q3_pos),
        "deg" => return Some(deg_pos),
        _ => {}
    }
    for (seg_name, p0, p1) in clock_segment_lines(rect) {
        if seg_name == name {
            return Some(Pos2::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5));
        }
    }
    None
}

pub fn hit_test_clock_segment(rect: Rect, pointer: Pos2) -> Option<String> {
    if !rect.contains(pointer) {
        return None;
    }
    let (p3_pos, q3_pos, deg_pos) = clock_special_positions(rect);

    if pointer.distance(p3_pos) <= 6.5 {
        return Some("p3".to_string());
    }
    if pointer.distance(q3_pos) <= 6.5 {
        return Some("q3".to_string());
    }
    if pointer.distance(deg_pos) <= 7.5 {
        return Some("deg".to_string());
    }

    let mut best: Option<(String, f32)> = None;
    for (name, p0, p1) in clock_segment_lines(rect) {
        let d = dist_to_segment(pointer, p0, p1);
        if d <= 6.0 && best.as_ref().map(|(_, b)| d < *b).unwrap_or(true) {
            best = Some((name, d));
        }
    }
    best.map(|(n, _)| n)
}

pub fn clock_inspector_text(
    board_slug: &str,
    hovered_seg: Option<&str>,
    segments: &BTreeMap<String, u8>,
    default_pwm: u8,
) -> String {
    let prefix = protocol::find_board_descriptor(board_slug)
        .map(|b| b.sysfs_prefix)
        .unwrap_or(if board_slug.is_empty() {
            "cucumber"
        } else {
            board_slug
        });
    if let Some(seg) = hovered_seg {
        let br = segment_brightness(segments, seg);
        if let Some(alias) = clock_segment_alias_info(seg) {
            format!(
                "Сегмент {} ({}) | /sys/class/leds/{}:white:clock-{}/brightness | PWM: {} | ЛКМ: зажечь | ПКМ: погасить",
                seg, alias, prefix, seg, br
            )
        } else {
            format!(
                "Сегмент {} | /sys/class/leds/{}:white:clock-{}/brightness | PWM: {} | ЛКМ: зажечь | ПКМ: погасить",
                seg, prefix, seg, br
            )
        }
    } else {
        format!(
            "45 сегментов (a1..g4, p3..q7, h1..l2, deg) | Наведите на сегмент | ЛКМ: зажечь, ПКМ: погасить (PWM: {})",
            default_pwm
        )
    }
}

pub fn matrix_inspector_text(hovered_idx: Option<usize>, pixels: &[u8], default_pwm: u8) -> String {
    if let Some(idx) = hovered_idx.filter(|&i| i < 252) {
        let col = idx % 21;
        let row = idx / 21;
        let br = pixels.get(idx).copied().unwrap_or(0);
        format!(
            "Пиксель [{}, {}] (индекс {}) | /dev/fb1 (gowin_led_device) | PWM: {} | ЛКМ: зажечь | ПКМ: погасить",
            col, row, idx, br
        )
    } else {
        format!(
            "Матрица 21×12 (252 LED, /dev/fb1) | Наведите на пиксель | ЛКМ: рисовать, ПКМ: стирать (PWM: {})",
            default_pwm
        )
    }
}

pub fn hit_test_matrix_pixel(rect: Rect, cell: f32, pointer: Pos2) -> Option<usize> {
    if !rect.contains(pointer) {
        return None;
    }
    let col = ((pointer.x - rect.left() - 5.0) / cell).floor() as isize;
    let row = ((pointer.y - rect.top() - 5.0) / cell).floor() as isize;
    if (0..21).contains(&col) && (0..12).contains(&row) {
        Some((row as usize) * 21 + (col as usize))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;

    #[test]
    fn test_led_ring_geometry_and_doa_azimuth() {
        let c = Pos2::new(100.0, 100.0);
        let top = led_ring_position(c, 50.0, 0, 4);
        let right = led_ring_position(c, 50.0, 1, 4);
        assert!((top.x - 100.0).abs() < 1e-3 && (top.y - 50.0).abs() < 1e-3);
        assert!((right.x - 150.0).abs() < 1e-3 && (right.y - 100.0).abs() < 1e-3);
        assert_eq!(doa_angle_from_pointer(c, top), 0);
        assert_eq!(doa_angle_from_pointer(c, right), 90);
        assert_eq!(doa_angle_from_pointer(c, Pos2::new(100.0, 150.0)), 180);
        assert_eq!(doa_angle_from_pointer(c, Pos2::new(50.0, 100.0)), 270);
    }

    #[test]
    fn test_clock_and_matrix_hit_testing() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(260.0, 76.0));
        let names = protocol::display::canonical_clock_segment_names();
        assert_eq!(names.len(), 45);
        assert_eq!(clock_segment_lines(rect).len(), 42);

        for name in &names {
            let center = clock_segment_center(rect, name)
                .unwrap_or_else(|| panic!("Missing center for segment {}", name));
            assert!(
                rect.contains(center),
                "Segment {} center {:?} must lie inside rect {:?}",
                name,
                center,
                rect
            );
            assert_eq!(
                hit_test_clock_segment(rect, center),
                Some(name.clone()),
                "Hit test at center {:?} failed for segment {}",
                center,
                name
            );
        }

        let mrect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(283.0, 166.0));
        assert_eq!(
            hit_test_matrix_pixel(mrect, 13.0, Pos2::new(17.0, 17.0)),
            Some(0)
        );
        assert_eq!(
            hit_test_matrix_pixel(
                mrect,
                13.0,
                Pos2::new(15.0 + 20.5 * 13.0, 15.0 + 11.5 * 13.0)
            ),
            Some(251)
        );
    }

    #[test]
    fn test_clock_and_matrix_inspector_text() {
        assert_eq!(clock_segment_alias_info("h1"), Some("clockH0"));
        assert_eq!(clock_segment_alias_info("h2"), Some("clockH1"));
        assert_eq!(clock_segment_alias_info("l1"), Some("clockH2"));
        assert_eq!(clock_segment_alias_info("l2"), Some("clockH3"));
        assert_eq!(clock_segment_alias_info("a1"), None);

        let mut segs = BTreeMap::new();
        segs.insert("h1".to_string(), 200);
        segs.insert("p4".to_string(), 128);

        let h1_info = clock_inspector_text("yandexmini2", Some("h1"), &segs, 255);
        assert!(h1_info.contains("h1"));
        assert!(h1_info.contains("clockH0"));
        assert!(h1_info.contains("/sys/class/leds/mini2:white:clock-h1/brightness"));
        assert!(h1_info.contains("PWM: 200"));
        assert!(h1_info.contains("ЛКМ: зажечь | ПКМ: погасить"));

        let p4_info = clock_inspector_text("cucumber", Some("p4"), &segs, 255);
        assert!(p4_info.contains("p4"));
        assert!(!p4_info.contains("clockH"));
        assert!(p4_info.contains("/sys/class/leds/cucumber:white:clock-p4/brightness"));
        assert!(p4_info.contains("PWM: 128"));

        let idle_clock = clock_inspector_text("cucumber", None, &segs, 255);
        assert!(idle_clock.contains("45 сегментов"));
        assert!(idle_clock.contains("PWM: 255"));

        let mut px = vec![0u8; 252];
        px[0] = 255;
        px[251] = 96;
        let m0 = matrix_inspector_text(Some(0), &px, 255);
        assert!(m0.contains("[0, 0]"));
        assert!(m0.contains("0"));
        assert!(m0.contains("/dev/fb1 (gowin_led_device)"));
        assert!(m0.contains("PWM: 255"));

        let m251 = matrix_inspector_text(Some(251), &px, 255);
        assert!(m251.contains("[20, 11]"));
        assert!(m251.contains("251"));
        assert!(m251.contains("PWM: 96"));

        let idle_mat = matrix_inspector_text(None, &px, 180);
        assert!(idle_mat.contains("21×12"));
        assert!(idle_mat.contains("PWM: 180"));
    }
}
