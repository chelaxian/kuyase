use eframe::egui::{Pos2, Rect};
pub use protocol::default_board_ring_leds;
pub use protocol::display::{
    build_segments_map_from_text as build_local_segments_from_text, render_text_to_matrix_21x12,
};
use protocol::BridgeStateSnapshot;
use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnclosureShape {
    CirclePuck,       // yandexmini, yandexmini2, yandexmini2nc, bergamot, pickle, yandexmicro, plum, plumnc
    SquircleCube,     // cucumber, orion, saturn
    RectLightPanel,   // station2
    CylinderPortable, // mango
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnclosureHitTarget {
    DoaAzimuth(u32),
    Button(&'static str), // "action", "mute", "volume_down", "volume_up"
    MangoBluetooth,
    MangoNextTrack,
    MangoUsbCPd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CirclePuckSubtype {
    /// pickle: 16 LED по внешнему ободу + центральный круговой 6-LED бублик oknix между кнопками `-` и `+`
    OuterRingAndCenterOknix,
    /// bergamot, yandexmini2, yandexmini2nc: 6 LED в центральном кольце вокруг Action между кнопками `-` и `+` (без внешнего обода)
    CenterRingWithMinusPlus,
    /// yandexmini: центральное окно лазера ToF (VL53L0X) внутри среднего кольца 6 LED (SN3236 BGR) + боковая кнопка Mute
    Mini1TofAndSideMute,
    /// yandexmicro, plum, plumnc: центральный треугольник из 3 LED + 4 сенсорные кнопки крестом (Action, Mute, `-`, `+`)
    LiteTriangleAndCross4,
}

#[derive(Debug, Clone, Copy)]
pub struct CircleEnclosureLayout {
    pub center: Pos2,
    pub outer_r: f32,
    pub inner_cap_r: f32,
    pub doa_r: f32,
    pub ring_r: f32,
    pub center_logo_r: f32,
    pub button_hit_r: f32,
    pub action_hit_r: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct MangoEnclosureLayout {
    pub body_center: Pos2,
    pub doa_r: f32,
    pub body_rect: Rect,
    pub strap_rect: Rect,
    pub ctrl_center: Pos2,
    pub ctrl_rect: Rect,
    pub bt_pos: Pos2,
    pub minus_pos: Pos2,
    pub action_pos: Pos2,
    pub plus_pos: Pos2,
    pub next_pos: Pos2,
    pub side_center: Pos2,
    pub side_rect: Rect,
    pub mute_pos: Pos2,
    pub power_pos: Pos2,
    pub batt_led_positions: [Pos2; 4],
    pub usbc_pos: Pos2,
    pub usbc_rect: Rect,
}

pub fn board_enclosure_shape(slug: &str) -> EnclosureShape {
    match protocol::normalize_board_slug(slug).as_str() {
        "cucumber" | "orion" | "saturn" => EnclosureShape::SquircleCube,
        "station2" => EnclosureShape::RectLightPanel,
        "mango" => EnclosureShape::CylinderPortable,
        _ => EnclosureShape::CirclePuck,
    }
}

pub fn circle_puck_subtype(slug: &str) -> CirclePuckSubtype {
    match protocol::normalize_board_slug(slug).as_str() {
        "pickle" => CirclePuckSubtype::OuterRingAndCenterOknix,
        "yandexmini" => CirclePuckSubtype::Mini1TofAndSideMute,
        "yandexmicro" | "plum" | "plumnc" => CirclePuckSubtype::LiteTriangleAndCross4,
        _ => CirclePuckSubtype::CenterRingWithMinusPlus,
    }
}

pub fn circle_enclosure_layout(board_slug: &str, rect: Rect) -> CircleEnclosureLayout {
    let center = rect.center();
    let min_dim = rect.height().min(rect.width());
    let outer_r = (min_dim * 0.36).clamp(64.0, 92.0);
    let inner_cap_r = outer_r * 0.72;
    let norm = protocol::normalize_board_slug(board_slug);
    let doa_r = if norm == "orion" {
        outer_r + 16.0
    } else {
        outer_r + 14.0
    };
    let ring_r = match circle_puck_subtype(&norm) {
        CirclePuckSubtype::OuterRingAndCenterOknix => outer_r * 0.85,
        CirclePuckSubtype::CenterRingWithMinusPlus => (outer_r * 0.25).clamp(18.0, 24.0),
        CirclePuckSubtype::Mini1TofAndSideMute => outer_r * 0.52,
        CirclePuckSubtype::LiteTriangleAndCross4 => (outer_r * 0.20).clamp(14.0, 19.0),
    };
    let center_logo_r = (outer_r * 0.22).clamp(16.0, 22.0);
    let button_hit_r = 14.0;
    let action_hit_r = (outer_r * 0.28).clamp(20.0, 26.0);

    CircleEnclosureLayout {
        center,
        outer_r,
        inner_cap_r,
        doa_r,
        ring_r,
        center_logo_r,
        button_hit_r,
        action_hit_r,
    }
}

pub fn mango_enclosure_layout(rect: Rect) -> MangoEnclosureLayout {
    let center = rect.center();
    let body_center = Pos2::new(center.x, center.y - 4.0);
    let doa_r = (rect.height().min(rect.width()) * 0.43).clamp(88.0, 102.0);
    let body_rect = Rect::from_center_size(body_center, eframe::egui::Vec2::new(244.0, 114.0));
    let strap_rect = Rect::from_center_size(
        Pos2::new(body_rect.right() + 6.0, body_center.y + 10.0),
        eframe::egui::Vec2::new(18.0, 34.0),
    );

    let ctrl_center = Pos2::new(body_center.x, body_center.y - 21.0);
    let ctrl_rect = Rect::from_center_size(ctrl_center, eframe::egui::Vec2::new(196.0, 44.0));
    let bt_pos = Pos2::new(ctrl_center.x - 74.0, ctrl_center.y);
    let minus_pos = Pos2::new(ctrl_center.x - 42.0, ctrl_center.y);
    let action_pos = ctrl_center;
    let plus_pos = Pos2::new(ctrl_center.x + 42.0, ctrl_center.y);
    let next_pos = Pos2::new(ctrl_center.x + 74.0, ctrl_center.y);

    let side_center = Pos2::new(body_center.x, body_center.y + 26.0);
    let side_rect = Rect::from_center_size(side_center, eframe::egui::Vec2::new(196.0, 32.0));
    let mute_pos = Pos2::new(side_center.x - 72.0, side_center.y);
    let power_pos = Pos2::new(side_center.x - 46.0, side_center.y);
    let batt_start_x = side_center.x - 16.0;
    let batt_led_positions = [
        Pos2::new(batt_start_x, side_center.y),
        Pos2::new(batt_start_x + 13.5, side_center.y),
        Pos2::new(batt_start_x + 27.0, side_center.y),
        Pos2::new(batt_start_x + 40.5, side_center.y),
    ];
    let usbc_pos = Pos2::new(side_center.x + 64.0, side_center.y);
    let usbc_rect = Rect::from_center_size(usbc_pos, eframe::egui::Vec2::new(22.0, 10.0));

    MangoEnclosureLayout {
        body_center,
        doa_r,
        body_rect,
        strap_rect,
        ctrl_center,
        ctrl_rect,
        bt_pos,
        minus_pos,
        action_pos,
        plus_pos,
        next_pos,
        side_center,
        side_rect,
        mute_pos,
        power_pos,
        batt_led_positions,
        usbc_pos,
        usbc_rect,
    }
}

pub fn enclosure_button_targets(board_slug: &str, rect: Rect) -> Vec<(&'static str, Pos2, f32)> {
    let norm = protocol::normalize_board_slug(board_slug);
    let layout = circle_enclosure_layout(&norm, rect);
    let c = layout.center;

    match board_enclosure_shape(&norm) {
        EnclosureShape::CirclePuck => match circle_puck_subtype(&norm) {
            CirclePuckSubtype::OuterRingAndCenterOknix
            | CirclePuckSubtype::CenterRingWithMinusPlus => {
                let off_x = layout.outer_r * 0.52;
                vec![
                    ("action", c, layout.action_hit_r),
                    ("volume_down", Pos2::new(c.x - off_x, c.y), layout.button_hit_r),
                    ("volume_up", Pos2::new(c.x + off_x, c.y), layout.button_hit_r),
                ]
            }
            CirclePuckSubtype::Mini1TofAndSideMute => {
                let mute_r = layout.outer_r * 0.80;
                let mute_pos = Pos2::new(
                    c.x + mute_r * FRAC_PI_4.cos(),
                    c.y + mute_r * FRAC_PI_4.sin(),
                );
                vec![("mute", mute_pos, layout.button_hit_r)]
            }
            CirclePuckSubtype::LiteTriangleAndCross4 => {
                let cross_r = layout.outer_r * 0.66;
                vec![
                    ("action", Pos2::new(c.x, c.y - cross_r), layout.button_hit_r),
                    ("mute", Pos2::new(c.x, c.y + cross_r), layout.button_hit_r),
                    ("volume_down", Pos2::new(c.x - cross_r, c.y), layout.button_hit_r),
                    ("volume_up", Pos2::new(c.x + cross_r, c.y), layout.button_hit_r),
                ]
            }
        },
        EnclosureShape::SquircleCube => {
            if norm == "cucumber" {
                let body_half = layout.outer_r * 0.82;
                let off_x = body_half * 0.56;
                vec![
                    ("action", c, layout.action_hit_r),
                    ("volume_down", Pos2::new(c.x - off_x, c.y), layout.button_hit_r),
                    ("volume_up", Pos2::new(c.x + off_x, c.y), layout.button_hit_r),
                ]
            } else if norm == "orion" {
                let cap_half = layout.outer_r * 0.66;
                let action_pos = Pos2::new(c.x, c.y - cap_half * 0.16);
                let bar_y = c.y + cap_half * 0.36;
                let bar_half_w = cap_half * 0.46;
                vec![
                    ("action", action_pos, 15.5),
                    ("volume_down", Pos2::new(c.x - bar_half_w, bar_y), 13.0),
                    ("volume_up", Pos2::new(c.x + bar_half_w, bar_y), 13.0),
                ]
            } else {
                vec![("action", c, 18.0)]
            }
        }
        EnclosureShape::RectLightPanel => {
            let half_w = layout.outer_r * 0.80;
            let half_h = layout.outer_r * 0.68;
            let off_x = half_w * 0.56;
            let top_y = c.y - half_h * 0.68;
            vec![
                ("action", Pos2::new(c.x, top_y), 13.0),
                ("volume_down", Pos2::new(c.x - off_x, top_y), 13.0),
                ("volume_up", Pos2::new(c.x + off_x, top_y), 13.0),
            ]
        }
        EnclosureShape::CylinderPortable => {
            let m = mango_enclosure_layout(rect);
            vec![
                ("action", m.action_pos, 16.0),
                ("volume_down", m.minus_pos, 13.0),
                ("volume_up", m.plus_pos, 13.0),
                ("mute", m.mute_pos, 12.0),
            ]
        }
    }
}

pub fn enclosure_button_pos(board_slug: &str, rect: Rect, button: &str) -> Option<Pos2> {
    enclosure_button_targets(board_slug, rect)
        .into_iter()
        .find(|(name, _, _)| *name == button)
        .map(|(_, pos, _)| pos)
}

pub fn hit_test_enclosure_target(
    board_slug: &str,
    rect: Rect,
    pointer: Pos2,
) -> Option<EnclosureHitTarget> {
    if !rect.contains(pointer) {
        return None;
    }

    let norm = protocol::normalize_board_slug(board_slug);
    for (btn, pos, hit_r) in enclosure_button_targets(&norm, rect) {
        if pointer.distance(pos) <= hit_r {
            return Some(EnclosureHitTarget::Button(btn));
        }
    }

    if board_enclosure_shape(&norm) == EnclosureShape::CylinderPortable {
        let m = mango_enclosure_layout(rect);
        if pointer.distance(m.bt_pos) <= 13.0 {
            return Some(EnclosureHitTarget::MangoBluetooth);
        }
        if pointer.distance(m.next_pos) <= 13.0 {
            return Some(EnclosureHitTarget::MangoNextTrack);
        }
        if m.usbc_rect.expand(4.0).contains(pointer) || pointer.distance(m.usbc_pos) <= 12.0 {
            return Some(EnclosureHitTarget::MangoUsbCPd);
        }
        let dist = pointer.distance(m.body_center);
        if dist >= m.doa_r - 12.0 && dist <= m.doa_r + 14.0 {
            return Some(EnclosureHitTarget::DoaAzimuth(doa_angle_from_pointer(
                m.body_center,
                pointer,
            )));
        }
        return None;
    }

    let layout = circle_enclosure_layout(&norm, rect);
    let dist = pointer.distance(layout.center);
    let doa_min_r = match board_enclosure_shape(&norm) {
        EnclosureShape::SquircleCube | EnclosureShape::RectLightPanel => layout.doa_r - 12.0,
        _ => layout.outer_r * 0.88,
    };
    if dist >= doa_min_r && dist <= layout.doa_r + 14.0 {
        return Some(EnclosureHitTarget::DoaAzimuth(doa_angle_from_pointer(
            layout.center,
            pointer,
        )));
    }

    None
}

/// Возвращает строку-инспектор верхнего холста корпуса и флаг активности ховера (`true`, если курсор
/// наведён на кнопку, компас DOA, порт, светодиод кольца/Ambilight/матрицы 11×8 или датчик).
pub fn enclosure_hover_inspector_text(
    board_slug: &str,
    rect: Rect,
    pointer: Option<Pos2>,
    st: &BridgeStateSnapshot,
    press_mode: &str,
) -> (String, bool) {
    let norm = protocol::normalize_board_slug(board_slug);
    let Some(ptr) = pointer.filter(|&p| rect.contains(p)) else {
        return (enclosure_inspector_text(&norm, st, None, press_mode), false);
    };

    if let Some(target) = hit_test_enclosure_target(&norm, rect, ptr) {
        return (
            enclosure_inspector_text(&norm, st, Some(target), press_mode),
            true,
        );
    }

    let layout = circle_enclosure_layout(&norm, rect);
    let center = layout.center;
    let outer_r = layout.outer_r;
    let prefix = protocol::find_board_descriptor(&norm)
        .map(|d| d.sysfs_prefix)
        .unwrap_or(norm.as_str());

    match norm.as_str() {
        "orion" => {
            let ambi_half = outer_r * 0.86;
            let ambi_corner_r = ambi_half * 0.30;
            for idx in 0..12 {
                let pos = squircle_ring_position(
                    center,
                    ambi_half * 0.90,
                    ambi_corner_r * 0.90,
                    idx,
                    12,
                );
                if ptr.distance(pos) <= 7.5 {
                    let [r, g, b] = st.logo_pixels.get(idx).copied().unwrap_or([110, 45, 220]);
                    return (
                        format!(
                            "Зона Ambilight #{idx} (orion:*:ambilight{idx}a/b, /dev/fb0) | RGB({r}, {g}, {b})"
                        ),
                        true,
                    );
                }
            }
            let cap_half = outer_r * 0.66;
            let cap_corner_r = cap_half * 0.28;
            for idx in 0..36 {
                let pos = squircle_ring_position(
                    center,
                    cap_half * 0.88,
                    cap_corner_r * 0.88,
                    idx,
                    36,
                );
                if ptr.distance(pos) <= 6.5 {
                    let [r, g, b] = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                    return (
                        format!(
                            "Светодиод кольца #{idx} (orion:*:ring{idx}, /dev/fb2) | RGB({r}, {g}, {b})"
                        ),
                        true,
                    );
                }
            }
        }
        "cucumber" => {
            let body_half = outer_r * 0.82;
            let body_corner_r = body_half * 0.30;
            for idx in 0..24 {
                let pos = squircle_ring_position(
                    center,
                    body_half * 0.87,
                    body_corner_r * 0.87,
                    idx,
                    24,
                );
                if ptr.distance(pos) <= 7.0 {
                    let [r, g, b] = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                    return (
                        format!(
                            "Светодиод кольца #{idx} (cucumber:*:ring{idx}, /dev/fb0) | RGB({r}, {g}, {b})"
                        ),
                        true,
                    );
                }
            }
        }
        "station2" => {
            let half_w = outer_r * 0.80;
            let half_h = outer_r * 0.68;
            let body_rect = Rect::from_center_size(
                center,
                eframe::egui::Vec2::new(half_w * 2.0, half_h * 2.0),
            );
            let grid_top = body_rect.top() + 34.0;
            let grid_bottom = body_rect.bottom() - 10.0;
            let grid_left = body_rect.left() + 12.0;
            let grid_right = body_rect.right() - 12.0;
            let grid_rect = Rect::from_min_max(
                Pos2::new(grid_left, grid_top),
                Pos2::new(grid_right, grid_bottom),
            );
            if grid_rect.contains(ptr) {
                let cols = 11usize;
                let rows = 8usize;
                let col = (((ptr.x - grid_rect.left()) / grid_rect.width()) * (cols as f32))
                    .floor()
                    .clamp(0.0, (cols - 1) as f32) as usize;
                let row = (((ptr.y - grid_rect.top()) / grid_rect.height()) * (rows as f32))
                    .floor()
                    .clamp(0.0, (rows - 1) as f32) as usize;
                let idx = row * cols + col;
                let [r, g, b] = st.ring_pixels.get(idx).copied().unwrap_or([95, 45, 220]);
                return (
                    format!(
                        "Световая панель 11×8 [{col}, {row}] (LED #{idx}, station2:*:ring{idx}, /dev/fb0) | RGB({r}, {g}, {b})"
                    ),
                    true,
                );
            }
        }
        "pickle" => {
            for idx in 0..16 {
                let pos = led_ring_position(center, layout.ring_r, idx, 16);
                if ptr.distance(pos) <= 7.5 {
                    let [r, g, b] = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                    return (
                        format!(
                            "Светодиод обода #{idx} (pickle:*:ring{idx}, /dev/fb0) | RGB({r}, {g}, {b})"
                        ),
                        true,
                    );
                }
            }
        }
        "yandexmini" => {
            if ptr.distance(center) <= outer_r * 0.26 {
                return (
                    format!(
                        "Окно лазерного дальномера ToF (STM VL53L0X) | Дистанция: {} мм",
                        st.tof_mm
                    ),
                    true,
                );
            }
            for idx in 0..6 {
                let pos = led_ring_position(center, layout.ring_r, idx, 6);
                if ptr.distance(pos) <= 8.5 {
                    let [r, g, b] = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                    return (
                        format!(
                            "Светодиод SN3236 #{idx} (I2C 0x3c, BGR + защёлка 0x25) | RGB({r}, {g}, {b})"
                        ),
                        true,
                    );
                }
            }
        }
        "yandexmicro" | "plum" | "plumnc" => {
            for idx in 0..3 {
                let pos = led_ring_position(center, layout.ring_r, idx, 3);
                if ptr.distance(pos) <= 8.0 {
                    let [r, g, b] = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                    return (
                        format!(
                            "Светодиод кластера #{idx} ({prefix}:*:ring{idx}, sysfs) | RGB({r}, {g}, {b})"
                        ),
                        true,
                    );
                }
            }
        }
        "mango" => {
            let m = mango_enclosure_layout(rect);
            if ptr.distance(m.power_pos) <= 10.5 {
                return (
                    "Индикатор кнопки питания Power (mango, PMIC / GPIO)".to_string(),
                    true,
                );
            }
            for (idx, &pos) in m.batt_led_positions.iter().enumerate() {
                if ptr.distance(pos) <= 6.8 {
                    let default_batt = if idx < 3 {
                        [215, 232, 255]
                    } else {
                        [0, 0, 0]
                    };
                    let [r, g, b] = st.ring_pixels.get(idx).copied().unwrap_or(default_batt);
                    let ring_num = idx + 1;
                    return (
                        format!(
                            "Шкала заряда АКБ #{ring_num}/4 (mango:*:ring{ring_num}, AW9110 /dev/fb0) | RGB({r}, {g}, {b})"
                        ),
                        true,
                    );
                }
            }
        }
        _ => {}
    }

    (enclosure_inspector_text(&norm, st, None, press_mode), false)
}

pub fn enclosure_inspector_text(
    board_slug: &str,
    st: &BridgeStateSnapshot,
    hovered: Option<EnclosureHitTarget>,
    press_mode: &str,
) -> String {
    let norm = protocol::normalize_board_slug(board_slug);
    match hovered {
        Some(EnclosureHitTarget::DoaAzimuth(angle)) => format!(
            "Компас DOA (yio_audio :9850) | Азимут: {}° (текущий: {}°) | ЛКМ/Перетаскивание: повернуть источник звука",
            angle, st.doa_angle
        ),
        Some(EnclosureHitTarget::Button("action")) => match norm.as_str() {
            "pickle" | "cucumber" => format!(
                "Центральная кнопка Action + круговой логотип oknix (/dev/fb1, 6 LED) | ЛКМ: uinput action ({})",
                press_mode
            ),
            "orion" => format!(
                "Сенсорная кнопка Action (вызов Алисы / пауза, крышка 36 LED /dev/fb2) | ЛКМ: uinput action ({})",
                press_mode
            ),
            "station2" => format!(
                "Сенсорная кнопка Action (верхняя планка над панелью 88 LED 11×8 /dev/fb0) | ЛКМ: uinput action ({})",
                press_mode
            ),
            "saturn" => format!(
                "Центральная кнопка Action + статусный индикатор (1 LED, sysfs) | ЛКМ: uinput action ({})",
                press_mode
            ),
            "mango" => format!(
                "Центральная кнопка Action + круговой логотип oknix (AW21012 /dev/fb1, 3 LED) | ЛКМ: uinput action ({})",
                press_mode
            ),
            "bergamot" | "yandexmini2" | "yandexmini2nc" => format!(
                "Центральная кнопка Action + кольцо 6 LED (/dev/fb0 / sysfs) | ЛКМ: uinput action ({})",
                press_mode
            ),
            _ => format!(
                "Сенсорная кнопка Action (вызов Алисы / пауза) | ЛКМ: uinput action ({})",
                press_mode
            ),
        },
        Some(EnclosureHitTarget::Button("mute")) => {
            let mute_state = if st.mics_muted {
                "микрофоны ВЫКЛ"
            } else {
                "микрофоны ВКЛ"
            };
            if norm == "yandexmini" {
                format!(
                    "Физическая боковая кнопка Mute ({}) | ЛКМ: uinput mute ({})",
                    mute_state, press_mode
                )
            } else {
                format!(
                    "Кнопка отключения микрофонов Mute ({}) | ЛКМ: uinput mute ({})",
                    mute_state, press_mode
                )
            }
        }
        Some(EnclosureHitTarget::Button("volume_down")) => match norm.as_str() {
            "orion" => format!(
                "Сенсорная полоса громкости «−» (громкость {}/10, крышка /dev/fb2) | ЛКМ: uinput volume_down ({})",
                st.volume_level, press_mode
            ),
            "station2" => format!(
                "Сенсорная кнопка громкости «−» (громкость {}/10, панель 88 LED 11×8 /dev/fb0) | ЛКМ: uinput volume_down ({})",
                st.volume_level, press_mode
            ),
            _ => format!(
                "Кнопка уменьшения громкости «−» (громкость {}/10) | ЛКМ: uinput volume_down ({})",
                st.volume_level, press_mode
            ),
        },
        Some(EnclosureHitTarget::Button("volume_up")) => match norm.as_str() {
            "orion" => format!(
                "Сенсорная полоса громкости «+» (громкость {}/10, крышка /dev/fb2) | ЛКМ: uinput volume_up ({})",
                st.volume_level, press_mode
            ),
            "station2" => format!(
                "Сенсорная кнопка громкости «+» (громкость {}/10, панель 88 LED 11×8 /dev/fb0) | ЛКМ: uinput volume_up ({})",
                st.volume_level, press_mode
            ),
            _ => format!(
                "Кнопка увеличения громкости «+» (громкость {}/10) | ЛКМ: uinput volume_up ({})",
                st.volume_level, press_mode
            ),
        },
        Some(EnclosureHitTarget::Button(other)) => format!(
            "Кнопка корпуса {} | ЛКМ: uinput {} ({})",
            other, other, press_mode
        ),
        Some(EnclosureHitTarget::MangoBluetooth) => {
            "Кнопка Bluetooth [BT] (mango, AW21012 /dev/fb1) | ЛКМ: переключить паттерн bluetooth_playing"
                .to_string()
        }
        Some(EnclosureHitTarget::MangoNextTrack) => {
            "Кнопка следующего трека «>|» (mango) | ЛКМ: остановить TTS (stop_tts) / переключить трек (uinput)"
                .to_string()
        }
        Some(EnclosureHitTarget::MangoUsbCPd) => {
            "Разъём USB-C PD (IP2366 PD + CW2217 АКБ, шкала AW9110 /dev/fb0) | ЛКМ: переключить зарядку PD 12 В ↔ АКБ"
                .to_string()
        }
        None => match norm.as_str() {
            "cucumber" => format!(
                "DOA {}° | Контур 24 LED (/dev/fb0) + oknix 6 LED (/dev/fb1) | Кнопки [−] [Action] [+]: ЛКМ ({}) | Обод: DOA",
                st.doa_angle, press_mode
            ),
            "orion" => format!(
                "DOA {}° | Кольцо 36 LED (/dev/fb2) + Ambilight 12×2 RGBW (/dev/fb0) | [Action] [− ═══ +]: ЛКМ ({}) | Обод: DOA",
                st.doa_angle, press_mode
            ),
            "station2" => format!(
                "DOA {}° | Световая панель 88 LED 11×8 (/dev/fb0) | Верхняя планка [−] [Action] [+]: ЛКМ ({}) | Обод: DOA",
                st.doa_angle, press_mode
            ),
            "saturn" => format!(
                "DOA {}° | Индикатор 1 LED (sysfs saturn:*:ring0) | Центральная кнопка [Action]: ЛКМ ({}) | Обод: DOA",
                st.doa_angle, press_mode
            ),
            "pickle" => format!(
                "DOA {}° | Кольцо 16 LED (/dev/fb0) + oknix 6 LED (/dev/fb1) | Кнопки [−] [Action] [+]: ЛКМ ({}) | Обод: DOA",
                st.doa_angle, press_mode
            ),
            "bergamot" | "yandexmini2" | "yandexmini2nc" => format!(
                "DOA {}° | Центральное кольцо 6 LED (/dev/fb0 / sysfs) | Кнопки [−] [Action] [+]: ЛКМ ({}) | Обод: DOA",
                st.doa_angle, press_mode
            ),
            "yandexmini" => format!(
                "DOA {}° | SN3236 (0x3c, 6 LED BGR) + ToF {} мм | Боковая кнопка [Mute]: ЛКМ ({}) | Обод: DOA",
                st.doa_angle, st.tof_mm, press_mode
            ),
            "yandexmicro" | "plum" | "plumnc" => format!(
                "DOA {}° | Кластер 3 LED (sysfs) | Кнопки [Action] [Mute] [−] [+]: ЛКМ ({}) | Обод: DOA",
                st.doa_angle, press_mode
            ),
            "mango" => format!(
                "DOA {}° | АКБ 4 LED (AW9110 /dev/fb0, CW2217) + oknix 3 LED (AW21012 /dev/fb1) | [BT] [−] [Action] [+] [>|] [Mute] [USB-C PD (IP2366)]: ЛКМ ({})",
                st.doa_angle, press_mode
            ),
            _ => format!(
                "DOA {}° | Громк {}/10 | Кольцо-компас: перетащите для DOA | Кнопки крышки: ЛКМ ({})",
                st.doa_angle, st.volume_level, press_mode
            ),
        },
    }
}

pub fn squircle_ring_position(
    center: Pos2,
    half_extent: f32,
    corner_radius: f32,
    index: usize,
    count: usize,
) -> Pos2 {
    if count == 0 {
        return center;
    }
    if count == 1 {
        return Pos2::new(center.x, center.y - half_extent);
    }

    let eff_corner = corner_radius.clamp(half_extent * 0.15, half_extent * 0.65);
    let span = half_extent - eff_corner * 0.55;
    let corner_inset = eff_corner * 0.12;
    let idx = index % count;

    let (side, u) = if count % 4 == 0 {
        let per_side = count / 4;
        let side = idx / per_side;
        let k = idx % per_side;
        let u = if per_side <= 1 {
            0.0
        } else {
            -1.0 + 2.0 * (k as f32) / ((per_side - 1) as f32)
        };
        (side, u)
    } else {
        let t = (idx as f32 + 0.5) / (count as f32);
        let side_f = t * 4.0;
        let side = (side_f.floor() as usize).min(3);
        let frac = side_f - (side as f32);
        (side, frac * 2.0 - 1.0)
    };

    let along = u * span;
    let perp = half_extent - corner_inset * u * u;

    match side {
        0 => Pos2::new(center.x + along, center.y - perp),
        1 => Pos2::new(center.x + perp, center.y + along),
        2 => Pos2::new(center.x - along, center.y + perp),
        _ => Pos2::new(center.x - perp, center.y - along),
    }
}

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
    fn test_board_enclosure_shape_for_all_13_boards() {
        assert_eq!(protocol::BOARD_CATALOG.len(), 13);
        let mut circle_count = 0;
        let mut squircle_count = 0;
        let mut rect_count = 0;
        let mut cylinder_count = 0;

        for desc in protocol::BOARD_CATALOG {
            match board_enclosure_shape(desc.slug) {
                EnclosureShape::CirclePuck => {
                    circle_count += 1;
                    assert!(matches!(
                        desc.slug,
                        "yandexmini"
                            | "yandexmini2"
                            | "yandexmini2nc"
                            | "bergamot"
                            | "pickle"
                            | "yandexmicro"
                            | "plum"
                            | "plumnc"
                    ));
                }
                EnclosureShape::SquircleCube => {
                    squircle_count += 1;
                    assert!(matches!(desc.slug, "cucumber" | "orion" | "saturn"));
                }
                EnclosureShape::RectLightPanel => {
                    rect_count += 1;
                    assert_eq!(desc.slug, "station2");
                }
                EnclosureShape::CylinderPortable => {
                    cylinder_count += 1;
                    assert_eq!(desc.slug, "mango");
                }
            }
        }

        assert_eq!(circle_count, 8);
        assert_eq!(squircle_count, 3);
        assert_eq!(rect_count, 1);
        assert_eq!(cylinder_count, 1);
    }

    #[test]
    fn test_circle_puck_hit_testing_and_no_doa_conflict() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(320.0, 232.0));

        // 1. pickle, bergamot, yandexmini2, yandexmini2nc: center Action + left/right volume buttons
        for slug in ["pickle", "bergamot", "yandexmini2", "yandexmini2nc"] {
            let layout = circle_enclosure_layout(slug, rect);
            assert_eq!(
                hit_test_enclosure_target(slug, rect, layout.center),
                Some(EnclosureHitTarget::Button("action")),
                "center should hit action on {slug}"
            );
            let minus_pos = enclosure_button_pos(slug, rect, "volume_down").unwrap();
            let plus_pos = enclosure_button_pos(slug, rect, "volume_up").unwrap();
            assert_eq!(
                hit_test_enclosure_target(slug, rect, minus_pos),
                Some(EnclosureHitTarget::Button("volume_down"))
            );
            assert_eq!(
                hit_test_enclosure_target(slug, rect, plus_pos),
                Some(EnclosureHitTarget::Button("volume_up"))
            );

            // Outer DOA compass ring at 0, 90, 180, 270 degrees
            let top_doa = Pos2::new(layout.center.x, layout.center.y - layout.doa_r);
            let right_doa = Pos2::new(layout.center.x + layout.doa_r, layout.center.y);
            assert_eq!(
                hit_test_enclosure_target(slug, rect, top_doa),
                Some(EnclosureHitTarget::DoaAzimuth(0))
            );
            assert_eq!(
                hit_test_enclosure_target(slug, rect, right_doa),
                Some(EnclosureHitTarget::DoaAzimuth(90))
            );
        }

        // 2. yandexmini: central ToF window (not a button), side Mute button, NO volume buttons on cap
        {
            let slug = "yandexmini";
            let layout = circle_enclosure_layout(slug, rect);
            assert_eq!(hit_test_enclosure_target(slug, rect, layout.center), None);
            assert!(enclosure_button_pos(slug, rect, "volume_down").is_none());
            assert!(enclosure_button_pos(slug, rect, "volume_up").is_none());
            let mute_pos = enclosure_button_pos(slug, rect, "mute").unwrap();
            assert_eq!(
                hit_test_enclosure_target(slug, rect, mute_pos),
                Some(EnclosureHitTarget::Button("mute"))
            );
            let left_doa = Pos2::new(layout.center.x - layout.doa_r, layout.center.y);
            assert_eq!(
                hit_test_enclosure_target(slug, rect, left_doa),
                Some(EnclosureHitTarget::DoaAzimuth(270))
            );
        }

        // 3. yandexmicro, plum, plumnc: 4 cross buttons (action, mute, volume_down, volume_up) + outer DOA
        for slug in ["yandexmicro", "plum", "plumnc"] {
            let layout = circle_enclosure_layout(slug, rect);
            for btn in ["action", "mute", "volume_down", "volume_up"] {
                let bpos = enclosure_button_pos(slug, rect, btn).unwrap();
                assert_eq!(
                    hit_test_enclosure_target(slug, rect, bpos),
                    Some(EnclosureHitTarget::Button(btn)),
                    "button {btn} should hit on {slug}"
                );
            }
            let bottom_doa = Pos2::new(layout.center.x, layout.center.y + layout.doa_r);
            assert_eq!(
                hit_test_enclosure_target(slug, rect, bottom_doa),
                Some(EnclosureHitTarget::DoaAzimuth(180))
            );
        }
    }

    #[test]
    fn test_enclosure_inspector_text_resting_and_hovered() {
        let mut st = BridgeStateSnapshot::default();
        st.doa_angle = 135;
        st.volume_level = 6;
        st.tof_mm = 320;

        for slug in ["pickle", "bergamot", "yandexmini2", "yandexmini", "yandexmicro", "plum"] {
            let rest = enclosure_inspector_text(slug, &st, None, "single");
            assert!(!rest.is_empty());
            assert!(rest.contains("135°"));
            if slug == "yandexmini" {
                assert!(rest.contains("SN3236") && rest.contains("ToF"));
            } else {
                assert!(!rest.contains("ToF"));
            }

            let hov_btn = enclosure_inspector_text(
                slug,
                &st,
                Some(EnclosureHitTarget::Button("action")),
                "double",
            );
            assert!(hov_btn.contains("uinput action (double)"));

            let hov_doa = enclosure_inspector_text(
                slug,
                &st,
                Some(EnclosureHitTarget::DoaAzimuth(240)),
                "single",
            );
            assert!(hov_doa.contains("240°") && hov_doa.contains("DOA"));
        }
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

    #[test]
    fn test_squircle_ring_position_geometry() {
        let center = Pos2::new(160.0, 120.0);
        let half_extent = 70.0;
        let corner_radius = 18.0;

        for count in [12usize, 24, 36] {
            let per_side = count / 4;
            let pts: Vec<Pos2> = (0..count)
                .map(|idx| squircle_ring_position(center, half_extent, corner_radius, idx, count))
                .collect();

            // 1. Strict clockwise angular progression around center
            for i in 0..count {
                let p0 = pts[i];
                let p1 = pts[(i + 1) % count];
                let a0 = (p0.y - center.y).atan2(p0.x - center.x);
                let a1 = (p1.y - center.y).atan2(p1.x - center.x);
                let step = (a1 - a0).rem_euclid(TAU);
                assert!(
                    step > 0.01 && step < std::f32::consts::PI,
                    "Non-clockwise step at index {i} for count {count}: step={step}"
                );
            }

            // 2. Symmetry across all 4 sides (top, right, bottom, left)
            for k in 0..per_side {
                let top = pts[k];
                let right = pts[per_side + k];
                let bottom = pts[2 * per_side + k];
                let left = pts[3 * per_side + k];

                assert!(top.y < center.y, "top LED {k} must lie above center");
                assert!(right.x > center.x, "right LED {k} must lie right of center");
                assert!(bottom.y > center.y, "bottom LED {k} must lie below center");
                assert!(left.x < center.x, "left LED {k} must lie left of center");

                // Mirror symmetry within each side
                let top_mirror = pts[per_side - 1 - k];
                assert!(
                    ((top.x - center.x) + (top_mirror.x - center.x)).abs() < 1e-3
                        && (top.y - top_mirror.y).abs() < 1e-3,
                    "top side must be horizontally symmetric at k={k} for count={count}"
                );

                // 90-degree rotational symmetry across all 4 sides
                let top_dx = top.x - center.x;
                let top_dy = top.y - center.y;
                assert!(
                    ((right.x - center.x) - (-top_dy)).abs() < 1e-3
                        && ((right.y - center.y) - top_dx).abs() < 1e-3,
                    "right side must be 90° CW rotation of top side at k={k}"
                );
                assert!(
                    ((bottom.x - center.x) - (-top_dx)).abs() < 1e-3
                        && ((bottom.y - center.y) - (-top_dy)).abs() < 1e-3,
                    "bottom side must be 180° rotation of top side at k={k}"
                );
                assert!(
                    ((left.x - center.x) - top_dy).abs() < 1e-3
                        && ((left.y - center.y) - (-top_dx)).abs() < 1e-3,
                    "left side must be 270° CW rotation of top side at k={k}"
                );
            }
        }
    }

    #[test]
    fn test_squircle_and_rect_panel_hit_testing() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(320.0, 252.0));

        // 1. cucumber, orion, station2: action, volume_down, volume_up + outer DOA compass ring
        for slug in ["cucumber", "orion", "station2"] {
            let layout = circle_enclosure_layout(slug, rect);
            for btn in ["action", "volume_down", "volume_up"] {
                let bpos = enclosure_button_pos(slug, rect, btn)
                    .unwrap_or_else(|| panic!("missing button {btn} on {slug}"));
                assert_eq!(
                    hit_test_enclosure_target(slug, rect, bpos),
                    Some(EnclosureHitTarget::Button(btn)),
                    "button {btn} should hit on {slug}"
                );
            }

            for (cardinal_pos, expected_deg) in [
                (Pos2::new(layout.center.x, layout.center.y - layout.doa_r), 0),
                (Pos2::new(layout.center.x + layout.doa_r, layout.center.y), 90),
                (Pos2::new(layout.center.x, layout.center.y + layout.doa_r), 180),
                (Pos2::new(layout.center.x - layout.doa_r, layout.center.y), 270),
            ] {
                assert_eq!(
                    hit_test_enclosure_target(slug, rect, cardinal_pos),
                    Some(EnclosureHitTarget::DoaAzimuth(expected_deg)),
                    "DOA at {expected_deg}° should hit on {slug}"
                );
            }
        }

        // 2. saturn: central action button + outer DOA compass ring (no volume buttons on cap)
        {
            let slug = "saturn";
            let layout = circle_enclosure_layout(slug, rect);
            let action_pos = enclosure_button_pos(slug, rect, "action").unwrap();
            assert_eq!(
                hit_test_enclosure_target(slug, rect, action_pos),
                Some(EnclosureHitTarget::Button("action"))
            );
            assert!(enclosure_button_pos(slug, rect, "volume_down").is_none());
            assert!(enclosure_button_pos(slug, rect, "volume_up").is_none());
            assert_eq!(
                hit_test_enclosure_target(
                    slug,
                    rect,
                    Pos2::new(layout.center.x, layout.center.y - layout.doa_r)
                ),
                Some(EnclosureHitTarget::DoaAzimuth(0))
            );
        }

        // 3. Inspector text verification for cucumber, orion, station2, saturn
        let mut st = BridgeStateSnapshot::default();
        st.doa_angle = 45;
        st.volume_level = 7;

        let cuc_rest = enclosure_inspector_text("cucumber", &st, None, "single");
        assert!(cuc_rest.contains("24 LED") && cuc_rest.contains("oknix 6 LED"));

        let ori_rest = enclosure_inspector_text("orion", &st, None, "single");
        assert!(
            ori_rest.contains("36 LED")
                && ori_rest.contains("/dev/fb2")
                && ori_rest.contains("Ambilight 12×2 RGBW")
                && ori_rest.contains("/dev/fb0")
        );

        let st2_rest = enclosure_inspector_text("station2", &st, None, "single");
        assert!(st2_rest.contains("88 LED 11×8") && st2_rest.contains("/dev/fb0"));

        let sat_rest = enclosure_inspector_text("saturn", &st, None, "single");
        assert!(sat_rest.contains("1 LED") && sat_rest.contains("saturn"));

        for slug in ["cucumber", "orion", "station2", "saturn"] {
            let hov_act = enclosure_inspector_text(
                slug,
                &st,
                Some(EnclosureHitTarget::Button("action")),
                "long",
            );
            assert!(hov_act.contains("uinput action (long)"));
        }
    }

    #[test]
    fn test_mango_cylinder_portable_hit_testing_and_inspector() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(320.0, 232.0));
        let m = mango_enclosure_layout(rect);
        assert_eq!(board_enclosure_shape("mango"), EnclosureShape::CylinderPortable);

        assert_eq!(
            hit_test_enclosure_target("mango", rect, m.bt_pos),
            Some(EnclosureHitTarget::MangoBluetooth)
        );
        assert_eq!(
            hit_test_enclosure_target("mango", rect, m.minus_pos),
            Some(EnclosureHitTarget::Button("volume_down"))
        );
        assert_eq!(
            hit_test_enclosure_target("mango", rect, m.action_pos),
            Some(EnclosureHitTarget::Button("action"))
        );
        assert_eq!(
            hit_test_enclosure_target("mango", rect, m.plus_pos),
            Some(EnclosureHitTarget::Button("volume_up"))
        );
        assert_eq!(
            hit_test_enclosure_target("mango", rect, m.next_pos),
            Some(EnclosureHitTarget::MangoNextTrack)
        );
        assert_eq!(
            hit_test_enclosure_target("mango", rect, m.mute_pos),
            Some(EnclosureHitTarget::Button("mute"))
        );
        assert_eq!(
            hit_test_enclosure_target("mango", rect, m.usbc_pos),
            Some(EnclosureHitTarget::MangoUsbCPd)
        );
        assert_eq!(hit_test_enclosure_target("mango", rect, m.power_pos), None);

        let top_doa = Pos2::new(m.body_center.x, m.body_center.y - m.doa_r);
        let right_doa = Pos2::new(m.body_center.x + m.doa_r, m.body_center.y);
        assert_eq!(
            hit_test_enclosure_target("mango", rect, top_doa),
            Some(EnclosureHitTarget::DoaAzimuth(0))
        );
        assert_eq!(
            hit_test_enclosure_target("mango", rect, right_doa),
            Some(EnclosureHitTarget::DoaAzimuth(90))
        );

        let mut st = BridgeStateSnapshot::default();
        st.doa_angle = 45;
        st.volume_level = 7;

        let rest = enclosure_inspector_text("mango", &st, None, "single");
        assert!(rest.contains("AW9110") && rest.contains("AW21012") && rest.contains("IP2366") && rest.contains("CW2217"));
        assert!(!rest.contains("ToF"));

        let bt_txt = enclosure_inspector_text(
            "mango",
            &st,
            Some(EnclosureHitTarget::MangoBluetooth),
            "single",
        );
        assert!(bt_txt.contains("AW21012") && bt_txt.contains("bluetooth_playing"));

        let act_txt = enclosure_inspector_text(
            "mango",
            &st,
            Some(EnclosureHitTarget::Button("action")),
            "single",
        );
        assert!(act_txt.contains("AW21012") && act_txt.contains("/dev/fb1") && act_txt.contains("uinput"));

        let next_txt = enclosure_inspector_text(
            "mango",
            &st,
            Some(EnclosureHitTarget::MangoNextTrack),
            "single",
        );
        assert!(next_txt.contains("uinput"));

        let usbc_txt = enclosure_inspector_text(
            "mango",
            &st,
            Some(EnclosureHitTarget::MangoUsbCPd),
            "single",
        );
        assert!(
            usbc_txt.contains("IP2366")
                && usbc_txt.contains("CW2217")
                && usbc_txt.contains("AW9110")
        );
    }

    #[test]
    fn test_enclosure_hover_inspector_for_leds_ambilight_matrix_and_tof() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(320.0, 252.0));
        let mut st = BridgeStateSnapshot::default();
        st.tof_mm = 210;

        // 1. orion: Ambilight zone 0 & LED Ring diode 0
        {
            let layout = circle_enclosure_layout("orion", rect);
            let ambi_half = layout.outer_r * 0.86;
            let ambi_pos = squircle_ring_position(
                layout.center,
                ambi_half * 0.90,
                ambi_half * 0.30 * 0.90,
                0,
                12,
            );
            let (txt, active) =
                enclosure_hover_inspector_text("orion", rect, Some(ambi_pos), &st, "single");
            assert!(active && txt.contains("Ambilight #0") && txt.contains("/dev/fb0"));

            let cap_half = layout.outer_r * 0.66;
            let ring_pos = squircle_ring_position(
                layout.center,
                cap_half * 0.88,
                cap_half * 0.28 * 0.88,
                0,
                36,
            );
            let (txt_r, active_r) =
                enclosure_hover_inspector_text("orion", rect, Some(ring_pos), &st, "single");
            assert!(active_r && txt_r.contains("orion:*:ring0") && txt_r.contains("/dev/fb2"));
        }

        // 2. cucumber: LED Ring diode 0
        {
            let layout = circle_enclosure_layout("cucumber", rect);
            let body_half = layout.outer_r * 0.82;
            let ring_pos = squircle_ring_position(
                layout.center,
                body_half * 0.87,
                body_half * 0.30 * 0.87,
                0,
                24,
            );
            let (txt, active) =
                enclosure_hover_inspector_text("cucumber", rect, Some(ring_pos), &st, "single");
            assert!(active && txt.contains("cucumber:*:ring0") && txt.contains("/dev/fb0"));
        }

        // 3. station2: 11x8 light panel center pixel
        {
            let layout = circle_enclosure_layout("station2", rect);
            let panel_pt = Pos2::new(layout.center.x, layout.center.y + 12.0);
            let (txt, active) =
                enclosure_hover_inspector_text("station2", rect, Some(panel_pt), &st, "single");
            assert!(active && txt.contains("11×8") && txt.contains("/dev/fb0"));
        }

        // 4. yandexmini: central ToF window and SN3236 LED 0
        {
            let layout = circle_enclosure_layout("yandexmini", rect);
            let (tof_txt, tof_act) = enclosure_hover_inspector_text(
                "yandexmini",
                rect,
                Some(layout.center),
                &st,
                "single",
            );
            assert!(tof_act && tof_txt.contains("VL53L0X") && tof_txt.contains("210 мм"));

            let sn_pos = led_ring_position(layout.center, layout.ring_r, 0, 6);
            let (sn_txt, sn_act) =
                enclosure_hover_inspector_text("yandexmini", rect, Some(sn_pos), &st, "single");
            assert!(sn_act && sn_txt.contains("SN3236 #0") && sn_txt.contains("0x3c"));
        }

        // 5. mango: battery scale LED 0 and Power indicator
        {
            let m = mango_enclosure_layout(rect);
            let (b_txt, b_act) = enclosure_hover_inspector_text(
                "mango",
                rect,
                Some(m.batt_led_positions[0]),
                &st,
                "single",
            );
            assert!(b_act && b_txt.contains("mango:*:ring1") && b_txt.contains("AW9110"));

            let (p_txt, p_act) =
                enclosure_hover_inspector_text("mango", rect, Some(m.power_pos), &st, "single");
            assert!(p_act && p_txt.contains("Power"));
        }
    }
}
