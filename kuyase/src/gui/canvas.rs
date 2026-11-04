pub use crate::gui::display_geom::{
    board_enclosure_shape, board_has_matrix_21x12, board_has_segment_clock,
    build_local_segments_from_text, circle_enclosure_layout, circle_puck_subtype,
    clock_segment_lines, default_board_ring_leds, doa_angle_from_pointer, enclosure_button_pos,
    enclosure_button_targets, enclosure_inspector_text, hit_test_clock_segment,
    hit_test_enclosure_target, hit_test_matrix_pixel, led_ring_position,
    render_text_to_matrix_21x12, segment_brightness, CircleEnclosureLayout, CirclePuckSubtype,
    EnclosureHitTarget, EnclosureShape,
};
use crate::gui::faceplates::{
    render_aux_logo_strip, render_matrix_21x12_faceplate, render_segment_clock_faceplate,
};
use crate::gui::state::GuiAppState;
use eframe::egui::{self, Color32, Margin, Pos2, RichText, Sense, Stroke, Vec2};
use protocol::{BridgeStateSnapshot, HostCommand};
use std::collections::BTreeMap;
use std::time::Duration;

pub(crate) fn draw_ring_led(painter: &egui::Painter, pos: Pos2, radius: f32, rgb: [u8; 3]) {
    let [r, g, b] = rgb;
    if r > 0 || g > 0 || b > 0 {
        painter.circle_filled(
            pos,
            radius * 1.55,
            Color32::from_rgba_unmultiplied(r, g, b, 52),
        );
        painter.circle_filled(pos, radius, Color32::from_rgb(r, g, b));
        painter.circle_stroke(
            pos,
            radius + 0.5,
            Stroke::new(0.8, Color32::from_rgba_unmultiplied(15, 18, 24, 180)),
        );
    } else {
        painter.circle_filled(pos, radius, Color32::from_rgb(28, 32, 40));
        painter.circle_stroke(pos, radius, Stroke::new(1.0, Color32::from_rgb(56, 64, 78)));
    }
}

/// Рисует круговой световой логотип Алисы («oknix» / `/dev/fb1`: 6 LED на `pickle`/`cucumber`, 3 LED на `mango`).
pub(crate) fn draw_circular_oknix_logo(
    painter: &egui::Painter,
    center: Pos2,
    ring_r: f32,
    led_r: f32,
    pixels: &[[u8; 3]],
    count: usize,
    default_rgb: [u8; 3],
) {
    let n = count.max(1);
    let mut sum_r = 0u32;
    let mut sum_g = 0u32;
    let mut sum_b = 0u32;
    let mut any_lit = false;
    for i in 0..n {
        let [r, g, b] = pixels.get(i).copied().unwrap_or(default_rgb);
        if r > 0 || g > 0 || b > 0 {
            any_lit = true;
            sum_r += r as u32;
            sum_g += g as u32;
            sum_b += b as u32;
        }
    }

    painter.circle_filled(center, ring_r + led_r + 3.5, Color32::from_rgb(18, 21, 28));
    painter.circle_stroke(
        center,
        ring_r + led_r + 3.5,
        Stroke::new(1.0, Color32::from_rgb(52, 60, 74)),
    );

    if any_lit {
        let avg_r = (sum_r / (n as u32)).min(255) as u8;
        let avg_g = (sum_g / (n as u32)).min(255) as u8;
        let avg_b = (sum_b / (n as u32)).min(255) as u8;
        painter.circle_stroke(
            center,
            ring_r,
            Stroke::new(
                led_r * 2.1,
                Color32::from_rgba_unmultiplied(avg_r, avg_g, avg_b, 95),
            ),
        );
    } else {
        painter.circle_stroke(
            center,
            ring_r,
            Stroke::new(led_r * 1.4, Color32::from_rgb(32, 37, 46)),
        );
    }

    for i in 0..n {
        let rgb = pixels.get(i).copied().unwrap_or(default_rgb);
        let p = led_ring_position(center, ring_r, i, n);
        draw_ring_led(painter, p, led_r, rgb);
    }

    let inner_hole = (ring_r - led_r - 2.0).max(3.5);
    painter.circle_filled(center, inner_hole, Color32::from_rgb(14, 16, 22));
}

fn draw_cap_touch_button(
    painter: &egui::Painter,
    pos: Pos2,
    button: &str,
    hovered: bool,
    mics_muted: bool,
) {
    let radius = 10.5;
    let (bg, border, fg) = if button == "mute" && mics_muted {
        (
            Color32::from_rgb(120, 26, 32),
            if hovered {
                Color32::from_rgb(255, 150, 150)
            } else {
                Color32::from_rgb(185, 60, 68)
            },
            Color32::from_rgb(255, 195, 195),
        )
    } else if hovered {
        (
            Color32::from_rgb(32, 46, 66),
            Color32::from_rgb(110, 195, 255),
            Color32::from_rgb(215, 238, 255),
        )
    } else {
        (
            Color32::from_rgb(20, 24, 32),
            Color32::from_rgb(56, 65, 82),
            Color32::from_rgb(148, 162, 184),
        )
    };

    painter.circle_filled(pos, radius, bg);
    painter.circle_stroke(pos, radius, Stroke::new(if hovered { 1.6 } else { 1.0 }, border));

    let icon_stroke = Stroke::new(1.8, fg);
    match button {
        "volume_down" => {
            painter.line_segment(
                [Pos2::new(pos.x - 4.6, pos.y), Pos2::new(pos.x + 4.6, pos.y)],
                icon_stroke,
            );
        }
        "volume_up" => {
            painter.line_segment(
                [Pos2::new(pos.x - 4.6, pos.y), Pos2::new(pos.x + 4.6, pos.y)],
                icon_stroke,
            );
            painter.line_segment(
                [Pos2::new(pos.x, pos.y - 4.6), Pos2::new(pos.x, pos.y + 4.6)],
                icon_stroke,
            );
        }
        "mute" => {
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                "M",
                egui::FontId::monospace(10.0),
                fg,
            );
        }
        "action" => {
            painter.circle_stroke(pos, 4.2, Stroke::new(1.6, fg));
        }
        _ => {}
    }
}

fn draw_doa_compass_ring(
    painter: &egui::Painter,
    center: Pos2,
    outer_r: f32,
    doa_r: f32,
    doa_angle: u32,
    hovered: bool,
) {
    let ring_col = if hovered {
        Color32::from_rgb(72, 160, 200)
    } else {
        Color32::from_rgb(44, 52, 66)
    };
    painter.circle_stroke(center, doa_r, Stroke::new(if hovered { 1.5 } else { 1.0 }, ring_col));

    for cardinal in [0usize, 90, 180, 270] {
        let p_in = led_ring_position(center, doa_r - 3.0, cardinal, 360);
        let p_out = led_ring_position(center, doa_r + 3.0, cardinal, 360);
        painter.line_segment([p_in, p_out], Stroke::new(1.0, ring_col));
    }

    let doa_pos = led_ring_position(center, doa_r, doa_angle as usize, 360);
    let doa_inner = led_ring_position(center, outer_r + 2.0, doa_angle as usize, 360);
    painter.line_segment(
        [doa_inner, doa_pos],
        Stroke::new(1.8, Color32::from_rgba_unmultiplied(0, 229, 255, 175)),
    );
    painter.circle_filled(doa_pos, if hovered { 5.8 } else { 5.0 }, Color32::from_rgb(0, 229, 255));
}

fn render_circle_puck_enclosure(
    painter: &egui::Painter,
    board_slug: &str,
    rect: egui::Rect,
    st: &BridgeStateSnapshot,
    hovered_target: Option<EnclosureHitTarget>,
) {
    let layout = circle_enclosure_layout(board_slug, rect);
    let center = layout.center;
    let outer_r = layout.outer_r;

    draw_doa_compass_ring(
        painter,
        center,
        outer_r,
        layout.doa_r,
        st.doa_angle,
        matches!(hovered_target, Some(EnclosureHitTarget::DoaAzimuth(_))),
    );

    // Основной круглый корпус колонки («шайба»)
    painter.circle_filled(center, outer_r, Color32::from_rgb(24, 28, 36));
    painter.circle_stroke(
        center,
        outer_r,
        Stroke::new(2.0, Color32::from_rgb(66, 74, 90)),
    );
    painter.circle_filled(
        center,
        layout.inner_cap_r,
        Color32::from_rgb(16, 19, 25),
    );

    let is_btn_hovered = |name: &'static str| hovered_target == Some(EnclosureHitTarget::Button(name));

    match circle_puck_subtype(board_slug) {
        CirclePuckSubtype::OuterRingAndCenterOknix => {
            // pickle: 16 светодиодов по внешнему ободу + центральный круговой 6-LED бублик oknix между `-` и `+`
            let ring_count = st
                .ring_leds
                .max(st.ring_pixels.len())
                .max(default_board_ring_leds(board_slug))
                .max(16);
            for idx in 0..ring_count {
                let rgb = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                let pos = led_ring_position(center, layout.ring_r, idx, ring_count);
                draw_ring_led(painter, pos, 4.8, rgb);
            }

            if let Some(minus_pos) = enclosure_button_pos(board_slug, rect, "volume_down") {
                draw_cap_touch_button(
                    painter,
                    minus_pos,
                    "volume_down",
                    is_btn_hovered("volume_down"),
                    st.mics_muted,
                );
            }
            if let Some(plus_pos) = enclosure_button_pos(board_slug, rect, "volume_up") {
                draw_cap_touch_button(
                    painter,
                    plus_pos,
                    "volume_up",
                    is_btn_hovered("volume_up"),
                    st.mics_muted,
                );
            }

            let logo_count = if !st.logo_pixels.is_empty() {
                st.logo_pixels.len()
            } else {
                6
            };
            draw_circular_oknix_logo(
                painter,
                center,
                layout.center_logo_r,
                4.2,
                &st.logo_pixels,
                logo_count,
                [135, 35, 255],
            );
            if is_btn_hovered("action") {
                painter.circle_stroke(
                    center,
                    layout.center_logo_r + 8.5,
                    Stroke::new(1.5, Color32::from_rgb(110, 195, 255)),
                );
            }
        }
        CirclePuckSubtype::CenterRingWithMinusPlus => {
            // bergamot, yandexmini2, yandexmini2nc: без внешнего обода; 6 LED в центре вокруг Action между `-` и `+`
            painter.circle_stroke(
                center,
                outer_r * 0.86,
                Stroke::new(1.0, Color32::from_rgb(32, 37, 46)),
            );

            if let Some(minus_pos) = enclosure_button_pos(board_slug, rect, "volume_down") {
                draw_cap_touch_button(
                    painter,
                    minus_pos,
                    "volume_down",
                    is_btn_hovered("volume_down"),
                    st.mics_muted,
                );
            }
            if let Some(plus_pos) = enclosure_button_pos(board_slug, rect, "volume_up") {
                draw_cap_touch_button(
                    painter,
                    plus_pos,
                    "volume_up",
                    is_btn_hovered("volume_up"),
                    st.mics_muted,
                );
            }

            let ring_count = st
                .ring_leds
                .max(st.ring_pixels.len())
                .max(default_board_ring_leds(board_slug))
                .max(6);
            let default_rgb = if board_slug == "bergamot" {
                [135, 35, 255]
            } else {
                [0, 165, 210]
            };
            draw_circular_oknix_logo(
                painter,
                center,
                layout.ring_r,
                4.6,
                &st.ring_pixels,
                ring_count,
                default_rgb,
            );
            if is_btn_hovered("action") {
                painter.circle_stroke(
                    center,
                    layout.ring_r + 9.0,
                    Stroke::new(1.5, Color32::from_rgb(110, 195, 255)),
                );
            }
        }
        CirclePuckSubtype::Mini1TofAndSideMute => {
            // yandexmini: центральное окно лазера ToF (VL53L0X) внутри среднего кольца 6 LED (SN3236 BGR) + боковая кнопка Mute
            let ring_count = st
                .ring_leds
                .max(st.ring_pixels.len())
                .max(default_board_ring_leds(board_slug))
                .max(6);
            for idx in 0..ring_count {
                let rgb = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                let pos = led_ring_position(center, layout.ring_r, idx, ring_count);
                draw_ring_led(painter, pos, 6.0, rgb);
            }

            let tof_r = outer_r * 0.26;
            let tof_near = st.tof_mm < 400;
            if tof_near {
                painter.circle_filled(
                    center,
                    tof_r,
                    Color32::from_rgba_unmultiplied(229, 165, 10, 75),
                );
                painter.circle_stroke(
                    center,
                    tof_r,
                    Stroke::new(1.8, Color32::from_rgb(245, 185, 35)),
                );
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    format!("ToF\n{} мм", st.tof_mm),
                    egui::FontId::monospace(9.5),
                    Color32::from_rgb(255, 230, 110),
                );
            } else {
                painter.circle_filled(center, tof_r, Color32::from_rgb(10, 12, 18));
                painter.circle_stroke(
                    center,
                    tof_r,
                    Stroke::new(1.4, Color32::from_rgb(58, 68, 86)),
                );
                painter.circle_filled(
                    Pos2::new(center.x - 4.5, center.y - 3.0),
                    2.2,
                    Color32::from_rgb(42, 24, 54),
                );
                painter.circle_filled(
                    Pos2::new(center.x + 4.5, center.y - 3.0),
                    2.2,
                    Color32::from_rgb(42, 24, 54),
                );
                painter.text(
                    Pos2::new(center.x, center.y + 6.5),
                    egui::Align2::CENTER_CENTER,
                    "ToF",
                    egui::FontId::monospace(9.0),
                    Color32::from_rgb(140, 155, 178),
                );
            }

            if let Some(mute_pos) = enclosure_button_pos(board_slug, rect, "mute") {
                draw_cap_touch_button(
                    painter,
                    mute_pos,
                    "mute",
                    is_btn_hovered("mute"),
                    st.mics_muted,
                );
            }
        }
        CirclePuckSubtype::LiteTriangleAndCross4 => {
            // yandexmicro, plum, plumnc: центральный треугольник из 3 LED + 4 сенсорные кнопки крестом
            painter.circle_filled(
                center,
                layout.ring_r + 9.0,
                Color32::from_rgb(13, 16, 22),
            );
            painter.circle_stroke(
                center,
                layout.ring_r + 9.0,
                Stroke::new(1.0, Color32::from_rgb(42, 49, 62)),
            );

            for idx in 0..3 {
                let rgb = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                let pos = led_ring_position(center, layout.ring_r, idx, 3);
                draw_ring_led(painter, pos, 5.6, rgb);
            }

            for btn in ["action", "mute", "volume_down", "volume_up"] {
                if let Some(bpos) = enclosure_button_pos(board_slug, rect, btn) {
                    draw_cap_touch_button(
                        painter,
                        bpos,
                        btn,
                        is_btn_hovered(btn),
                        st.mics_muted,
                    );
                }
            }
        }
    }
}

fn stage_box_frame(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(Color32::from_rgb(23, 25, 30))
        .stroke(Stroke::new(1.0, Color32::from_rgb(56, 62, 74)))
        .rounding(8.0)
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        });
}

pub fn render_twin_canvas(ui: &mut egui::Ui, app: &mut GuiAppState) {
    let bridge_online = app
        .bridge
        .as_ref()
        .map(|b| b.is_connected())
        .unwrap_or(false);
    if bridge_online {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    if !bridge_online
        && app
            .latest_state
            .as_ref()
            .map(|s| s.board_slug != app.config.selected_board)
            .unwrap_or(false)
    {
        app.latest_state = None;
    }

    let board_slug = app
        .latest_state
        .as_ref()
        .map(|s| s.board_slug.clone())
        .unwrap_or_else(|| app.config.selected_board.clone());

    let mut st = app.latest_state.clone().unwrap_or_else(|| {
        let mut segs = BTreeMap::new();
        for (k, &on) in &app.clock_active_mask {
            if on && app.clock_brightness > 0 {
                segs.insert(k.clone(), app.clock_brightness);
            }
        }
        BridgeStateSnapshot {
            board_slug: board_slug.clone(),
            board_name: crate::gui::board_display_label(&board_slug),
            ring_leds: default_board_ring_leds(&board_slug),
            tof_mm: app.tof_slider,
            doa_angle: app.doa_slider,
            volume_level: app.volume_slider,
            clock_segments: segs,
            screen_pixels: app.matrix_custom_pixels.clone(),
            ..Default::default()
        }
    });
    if !bridge_online {
        st.tof_mm = app.tof_slider;
        st.doa_angle = app.doa_slider;
        st.volume_level = app.volume_slider;
    }

    egui::ScrollArea::vertical()
        .id_salt("device_stage_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::none()
                .fill(Color32::from_rgb(43, 43, 43))
                .stroke(Stroke::new(1.0, Color32::from_rgb(60, 60, 60)))
                .rounding(10.0)
                .inner_margin(Margin::same(10.0))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!("{} ({})", st.board_name, st.board_slug))
                                .strong(),
                        );
                        ui.add(
                            egui::Button::new(
                                RichText::new(format!("Alice: {}", st.alice_state))
                                    .small()
                                    .strong()
                                    .color(Color32::from_rgb(130, 190, 255)),
                            )
                            .fill(Color32::from_rgb(28, 52, 82))
                            .rounding(99.0)
                            .sense(Sense::hover()),
                        );
                    });

                    ui.add_space(6.0);

                    // 1. Top Enclosure Box (Spacious LED Ring + Ambilight + DOA + ToF Hand)
                    stage_box_frame(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let trig_txt = if st.ring_trigger.is_empty() {
                                format!("Драйвер LED: Кольцо {} LED (?)", st.ring_leds)
                            } else {
                                format!("Драйвер LED: {} (?)", st.ring_trigger)
                            };
                            let trig_col = if st.ring_trigger.contains("none") {
                                Color32::from_rgb(120, 210, 255)
                            } else if st.ring_trigger.contains("Stealth") {
                                Color32::from_rgb(255, 175, 70)
                            } else {
                                Color32::from_rgb(165, 175, 190)
                            };
                            ui.label(RichText::new(trig_txt).small().color(trig_col))
                                .on_hover_text(
                                    "Режим драйвера светового кольца:\n\
                                     • ledfb-ring (Quasar Native) — штатная анимация прошивки через фреймбуфер\n\
                                     • none (Custom Override) — прямое ручное управление яркостью через /sys/class/leds/*:ring*/brightness\n\
                                     • SN3236 (0x3c) — аппаратный I2C-контроллер на yandexmini",
                                );

                            if board_slug == "yandexmini" {
                                let tof_near = st.tof_mm < 400;
                                let (tof_fg, tof_bg) = if tof_near {
                                    (
                                        Color32::from_rgb(255, 225, 100),
                                        Color32::from_rgb(66, 52, 18),
                                    )
                                } else {
                                    (
                                        Color32::from_rgb(155, 205, 250),
                                        Color32::from_rgb(28, 42, 60),
                                    )
                                };
                                let tof_label = if tof_near {
                                    format!("Датчик ToF (VL53L0X): {} мм (РУКА)", st.tof_mm)
                                } else {
                                    format!("Датчик ToF (VL53L0X): {} мм", st.tof_mm)
                                };
                                ui.add(
                                    egui::Button::new(
                                        RichText::new(tof_label).small().monospace().color(tof_fg),
                                    )
                                    .fill(tof_bg)
                                    .rounding(99.0)
                                    .sense(Sense::hover()),
                                )
                                .on_hover_text(
                                    "Оптический лазерный дальномер приближения STM VL53L0X (устанавливался только в Яндекс Станцию Мини 1 / yandexmini).\n\
                                     Дистанция < 400 мм означает поднесённую ладонь над крышкой колонки (жест управления громкостью / остановки).\n\
                                     Дистанция 8190 мм — препятствий над колонкой нет.",
                                );
                            } else if board_slug == "orion" {
                                let us_near = st.tof_mm < 1500;
                                let (us_fg, us_bg) = if us_near {
                                    (
                                        Color32::from_rgb(255, 225, 100),
                                        Color32::from_rgb(66, 52, 18),
                                    )
                                } else {
                                    (
                                        Color32::from_rgb(155, 205, 250),
                                        Color32::from_rgb(28, 42, 60),
                                    )
                                };
                                let us_label = format!("Ультразвук 38.4 кГц: эхо {} мм", st.tof_mm);
                                ui.add(
                                    egui::Button::new(
                                        RichText::new(us_label).small().monospace().color(us_fg),
                                    )
                                    .fill(us_bg)
                                    .rounding(99.0)
                                    .sense(Sense::hover()),
                                )
                                .on_hover_text(
                                    "На Станции 3 (orion) нет оптического датчика VL53L0X (ToF).\n\
                                     Вместо него используется отдельный ультразвуковой TDM-тракт pcm.ultrasound (96 кГц, несущая 38.4 кГц) через четвёртый усилитель ACM8625S (2-002c) и микрофонную матрицу.",
                                );
                            } else {
                                let als_label =
                                    format!("Датчик света (ALS): {} лк", st.als_lux);
                                ui.add(
                                    egui::Button::new(
                                        RichText::new(als_label)
                                            .small()
                                            .monospace()
                                            .color(Color32::from_rgb(155, 205, 250)),
                                    )
                                    .fill(Color32::from_rgb(28, 42, 60))
                                    .rounding(99.0)
                                    .sense(Sense::hover()),
                                )
                                .on_hover_text(
                                    "Датчик внешней освещённости ALS (ltr308als01 в /sys/bus/iio/devices/iio:device0/in_illuminance_raw).\n\
                                     Используется прошивкой для автояркости светового кольца и дисплея часов (оптического датчика жестов VL53L0X на этой модели нет).",
                                );

                                if board_slug == "mango" {
                                    ui.add(
                                        egui::Button::new(
                                            RichText::new("АКБ CW2217 + IP2366 PD")
                                                .small()
                                                .monospace()
                                                .color(Color32::from_rgb(135, 235, 165)),
                                        )
                                        .fill(Color32::from_rgb(24, 52, 36))
                                        .rounding(99.0)
                                        .sense(Sense::hover()),
                                    )
                                    .on_hover_text(
                                        "Портативная подсистема питания Яндекс Станции Стрит (mango):\n\
                                         • Кулонометр АКБ CW2217: /sys/devices/virtual/cw2217_cls/cw2217_coulomb/{capacity,battery_vol,battery_current,battery_temp,soh,cycle}\n\
                                         • Контроллер зарядки IP2366 USB-C PD: /sys/devices/virtual/ip2366_cls/ip2366_charger/{charge_status,is_pd,charge_voltage,pd_protocol,switch_charge}\n\
                                         • Стандартный интерфейс Linux: /sys/class/power_supply/battery/*\n\
                                         • Усилитель TI ACM8625S (2-002c): профили mango-acm8625.cfg (в помещении) и mango-acm8625-outside.cfg (на улице)\n\
                                         Управлять параметрами в 1 клик можно справа в блоке «3. Специфичная периферия платы (mango)».",
                                    );
                                }
                            }
                        });

                        if st.led_trigger_conflict_count > 0 {
                            ui.add_space(2.0);
                            ui.horizontal_wrapped(|ui| {
                                let conflict_badge = egui::Button::new(
                                    RichText::new(format!(
                                        "Конфликт записи sysfs LED: {} (?)",
                                        st.led_trigger_conflict_count
                                    ))
                                    .small()
                                    .strong()
                                    .color(Color32::from_rgb(255, 130, 130)),
                                )
                                .fill(Color32::from_rgb(68, 26, 30))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(150, 55, 60)))
                                .rounding(6.0)
                                .sense(Sense::hover());

                                ui.add(conflict_badge).on_hover_text(
                                    "Что означает «Конфликт записи sysfs LED» (Trigger Conflict):\n\
                                     Была выполнена прямая запись яркости в /sys/class/leds/*:ring*/brightness,\n\
                                     пока световое кольцо находилось под управлением триггера драйвера [ledfb-ring].\n\n\
                                     При активном [ledfb-ring] драйвер отклоняет прямую запись в sysfs и сбрасывает brightness в 0.\n\
                                     Чтобы управлять светодиодами вручную через sysfs, сначала переключите триггер в режим 'none':\n\
                                     echo none > /sys/class/leds/*:ring*/trigger",
                                );

                                if app.session.is_some() {
                                    let is_custom = st.ring_trigger.contains("none");
                                    if !is_custom
                                        && ui
                                            .small_button("Переключить в sysfs (none)")
                                            .on_hover_text("Отключить триггер ledfb-ring для прямой записи яркости через sysfs")
                                            .clicked()
                                    {
                                        app.send_serial_command(
                                            "for d in /sys/class/leds/*:ring*; do echo none > \"$d/trigger\"; done",
                                        );
                                    }
                                }
                            });
                        }

                        let ring_box_h = if board_slug == "orion" {
                            252.0
                        } else if board_slug == "mango" {
                            232.0
                        } else {
                            222.0
                        };
                        let canvas_size = Vec2::new(ui.available_width().max(260.0), ring_box_h);
                        let (rect, response) =
                            ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
                        let painter = ui.painter_at(rect);

                        let hovered_target = response
                            .hover_pos()
                            .or_else(|| response.interact_pointer_pos())
                            .and_then(|ptr| hit_test_enclosure_target(&board_slug, rect, ptr));

                        if response.hovered() || response.dragged() {
                            match hovered_target {
                                Some(
                                    EnclosureHitTarget::Button(_)
                                    | EnclosureHitTarget::MangoBluetooth
                                    | EnclosureHitTarget::MangoNextTrack
                                    | EnclosureHitTarget::MangoUsbCPd,
                                ) => {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                Some(EnclosureHitTarget::DoaAzimuth(_)) => {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                                }
                                None => {}
                            }
                        }

                        if response.clicked() {
                            if let Some(target) = response
                                .interact_pointer_pos()
                                .and_then(|ptr| hit_test_enclosure_target(&board_slug, rect, ptr))
                            {
                                match target {
                                    EnclosureHitTarget::Button(btn) => {
                                        app.send_command(HostCommand::PressButton {
                                            button: btn.to_string(),
                                            action: app.button_press_action.clone(),
                                        });
                                    }
                                    EnclosureHitTarget::DoaAzimuth(angle) => {
                                        app.doa_slider = angle;
                                        app.send_command(HostCommand::SetSensor {
                                            doa_angle: Some(angle),
                                            mic_rms: None,
                                            als_lux: None,
                                            temp_c: None,
                                            tof_mm: None,
                                            swipe: None,
                                            swipe_direction: None,
                                        });
                                    }
                                    EnclosureHitTarget::MangoBluetooth
                                    | EnclosureHitTarget::MangoNextTrack
                                    | EnclosureHitTarget::MangoUsbCPd => {}
                                }
                            }
                        } else if response.dragged() {
                            if let Some(EnclosureHitTarget::DoaAzimuth(angle)) = response
                                .interact_pointer_pos()
                                .and_then(|ptr| hit_test_enclosure_target(&board_slug, rect, ptr))
                            {
                                app.doa_slider = angle;
                                app.send_command(HostCommand::SetSensor {
                                    doa_angle: Some(angle),
                                    mic_rms: None,
                                    als_lux: None,
                                    temp_c: None,
                                    tof_mm: None,
                                    swipe: None,
                                    swipe_direction: None,
                                });
                            }
                        }

                        match board_enclosure_shape(&board_slug) {
                            EnclosureShape::CirclePuck => {
                                render_circle_puck_enclosure(
                                    &painter,
                                    &board_slug,
                                    rect,
                                    &st,
                                    hovered_target,
                                );
                            }
                            _ => {
                                // Базовый рендер некруглых плат (расширяется в тикетах 02 и 03)
                                render_circle_puck_enclosure(
                                    &painter,
                                    &board_slug,
                                    rect,
                                    &st,
                                    hovered_target,
                                );
                            }
                        }

                        ui.add_space(4.0);
                        let inspector_txt = enclosure_inspector_text(
                            &board_slug,
                            &st,
                            hovered_target,
                            &app.button_press_action,
                        );
                        let inspector_fg = if hovered_target.is_some() {
                            Color32::from_rgb(145, 215, 255)
                        } else {
                            Color32::from_rgb(160, 172, 190)
                        };
                        egui::Frame::none()
                            .fill(Color32::from_rgb(16, 19, 24))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(44, 52, 64)))
                            .rounding(5.0)
                            .inner_margin(Margin::symmetric(8.0, 4.0))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(inspector_txt)
                                            .small()
                                            .monospace()
                                            .color(inspector_fg),
                                    )
                                    .truncate(),
                                );
                            });
                    });

                    ui.add_space(6.0);

                    // 2. Dynamic Front Faceplate Display (Segment Clock OR 21x12 Matrix)
                    stage_box_frame(ui, |ui| {
                        if board_has_matrix_21x12(&board_slug, &st) {
                            render_matrix_21x12_faceplate(ui, app, &st, bridge_online);
                        } else if board_has_segment_clock(&board_slug, &st) {
                            render_segment_clock_faceplate(ui, app, &board_slug, &st, bridge_online);
                        } else if board_slug == "mango" {
                            ui.label(
                                RichText::new(
                                    "Индикация корпуса Станции Стрит (mango): 4-LED шкала заряда (/dev/fb0) + 3-LED логотип oknix (/dev/fb1)",
                                )
                                .small()
                                .strong()
                                .color(Color32::from_rgb(175, 190, 210)),
                            );
                            ui.label(
                                RichText::new(
                                    "• Верхний блок (4 LED, AW9110): индикатор заряда АКБ (/dev/fb0 и /sys/class/leds/mango:*:ring1..4, паттерны ledpatterns/battery/*.led)\n\
                                     • Логотип oknix ниже (3 RGBW LED, AW21012): фронтальный индикатор Алисы (/dev/fb1 и /sys/class/leds/mango:*:logo0..2, паттерны ledpatterns/oknix/*.led)",
                                )
                                .small()
                                .color(Color32::from_rgb(140, 150, 168)),
                            );
                            ui.add_space(4.0);
                            render_aux_logo_strip(ui, app, &board_slug, &st);
                        } else {
                            ui.label(
                                RichText::new(format!(
                                    "Фронтальный экран отсутствует на плате {}",
                                    board_slug
                                ))
                                .small()
                                .color(Color32::from_rgb(155, 165, 180)),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "Индикация выполняется через верхнее кольцо ({} LED).",
                                    st.ring_leds.max(default_board_ring_leds(&board_slug))
                                ))
                                .small()
                                .color(Color32::from_rgb(135, 145, 160)),
                            );
                            render_aux_logo_strip(ui, app, &board_slug, &st);
                        }
                    });

                    ui.add_space(6.0);

                    // 3. Physical Enclosure Buttons (Linux uinput)
                    stage_box_frame(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("Кнопки корпуса (uinput):")
                                    .small()
                                    .color(Color32::from_rgb(165, 175, 190)),
                            );
                            egui::ComboBox::from_id_salt("enclosure_press_mode")
                                .selected_text(match app.button_press_action.as_str() {
                                    "double" => "Двойное (double)",
                                    "triple" => "Тройное (triple)",
                                    "long" => "Удержание (long)",
                                    _ => "Одиночное (single)",
                                })
                                .width(155.0)
                                .show_ui(ui, |ui| {
                                    for (val, lbl) in [
                                        ("single", "Одиночное (single)"),
                                        ("double", "Двойное (double)"),
                                        ("triple", "Тройное (triple)"),
                                        ("long", "Удержание (long)"),
                                    ] {
                                        ui.selectable_value(
                                            &mut app.button_press_action,
                                            val.to_string(),
                                            lbl,
                                        );
                                    }
                                });
                        });
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            let mute_lbl = if st.mics_muted {
                                "Вкл. Микрофоны"
                            } else {
                                "Mute (Микрофоны)"
                            };
                            for (label, btn) in [
                                ("Action (Алиса)", "action"),
                                (mute_lbl, "mute"),
                                ("Громк -", "volume_down"),
                                ("Громк +", "volume_up"),
                            ] {
                                if ui.button(label).clicked() {
                                    app.send_command(HostCommand::PressButton {
                                        button: btn.to_string(),
                                        action: app.button_press_action.clone(),
                                    });
                                }
                            }
                        });
                    });
                });
        });
}
