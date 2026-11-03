pub use crate::gui::display_geom::{
    board_has_matrix_21x12, board_has_segment_clock, build_local_segments_from_text,
    clock_segment_lines, default_board_ring_leds, doa_angle_from_pointer, hit_test_clock_segment,
    hit_test_matrix_pixel, led_ring_position, render_text_to_matrix_21x12, segment_brightness,
};
use crate::gui::state::GuiAppState;
use eframe::egui::{self, Color32, Margin, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use protocol::{BridgeStateSnapshot, HostCommand};
use std::collections::BTreeMap;
use std::time::Duration;

fn draw_ring_led(painter: &egui::Painter, pos: Pos2, radius: f32, rgb: [u8; 3]) {
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

fn draw_matrix_dot(painter: &egui::Painter, pos: Pos2, radius: f32, val: u8) {
    if val > 0 {
        let alpha_glow = (val / 4).max(16);
        painter.circle_filled(
            pos,
            radius * 1.22,
            Color32::from_rgba_unmultiplied(140, 205, 255, alpha_glow),
        );
        let c = val.max(48);
        painter.circle_filled(pos, radius, Color32::from_rgb(c, c, (c as u16 + 15).min(255) as u8));
    } else {
        painter.circle_filled(pos, radius * 0.88, Color32::from_rgb(26, 30, 38));
        painter.circle_stroke(
            pos,
            radius * 0.88,
            Stroke::new(0.8, Color32::from_rgb(44, 50, 62)),
        );
    }
}

fn draw_segment_clock(painter: &egui::Painter, rect: Rect, segments: &BTreeMap<String, u8>) {
    painter.rect_filled(rect, 6.0, Color32::from_rgb(13, 16, 21));
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::from_rgb(51, 59, 72)));

    let seg_color = |br: u8| {
        if br > 0 {
            Color32::from_rgba_unmultiplied(232, 246, 255, br.max(55))
        } else {
            Color32::from_rgb(37, 43, 54)
        }
    };

    for (name, p0, p1) in clock_segment_lines(rect) {
        let br = segment_brightness(segments, &name);
        painter.line_segment([p0, p1], Stroke::new(3.6, seg_color(br)));
    }

    let digit_h = 40.0;
    let start_x = rect.center().x - 66.0;
    let top_y = rect.center().y - digit_h * 0.5;
    let colon_x = start_x + 2.0 * 34.0 - 1.0;

    let p3 = segment_brightness(segments, "p3");
    let q3 = segment_brightness(segments, "q3");
    painter.circle_filled(Pos2::new(colon_x, top_y + 13.0), 2.8, seg_color(p3));
    painter.circle_filled(Pos2::new(colon_x, top_y + 27.0), 2.8, seg_color(q3));

    let deg_br = segment_brightness(segments, "deg");
    painter.circle_stroke(
        Pos2::new(start_x + 4.0 * 34.0 + 14.0, top_y + 6.0),
        4.2,
        Stroke::new(2.4, seg_color(deg_br)),
    );
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

    let board_slug = app
        .latest_state
        .as_ref()
        .map(|s| s.board_slug.clone())
        .unwrap_or_else(|| app.config.selected_board.clone());

    let mut st = app.latest_state.clone().unwrap_or_else(|| {
        let mut def = BridgeStateSnapshot::default();
        def.board_slug = board_slug.clone();
        def.board_name = crate::gui::board_display_label(&board_slug);
        def.ring_leds = default_board_ring_leds(&board_slug);
        def.tof_mm = app.tof_slider;
        def.doa_angle = app.doa_slider;
        def.volume_level = app.volume_slider;
        let mut segs = BTreeMap::new();
        for (k, &on) in &app.clock_active_mask {
            if on && app.clock_brightness > 0 {
                segs.insert(k.clone(), app.clock_brightness);
            }
        }
        def.clock_segments = segs;
        def.screen_pixels = app.matrix_custom_pixels.clone();
        def
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
                                format!("Драйвер LED: Кольцо {} LED", st.ring_leds)
                            } else {
                                format!("Драйвер LED: {}", st.ring_trigger)
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

                        let ring_box_h = if board_slug == "orion" { 265.0 } else { 225.0 };
                        let canvas_size = Vec2::new(ui.available_width().max(260.0), ring_box_h);
                        let (rect, response) =
                            ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
                        let painter = ui.painter_at(rect);
                        let center = rect.center();
                        let outer_r = (rect.height().min(rect.width()) * 0.40).clamp(80.0, 105.0);

                        if response.clicked() || response.dragged() {
                            if let Some(ptr) = response.interact_pointer_pos() {
                                let dist = ptr.distance(center);
                                if dist >= outer_r * 0.50 && dist <= outer_r * 1.32 {
                                    let angle = doa_angle_from_pointer(center, ptr);
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
                        }

                        // Ambilight 12x2 RGBW outer halo (orion)
                        if st.aux_light_kind == "ambilight" || board_slug == "orion" {
                            let zones = st.logo_pixels.len().max(12);
                            painter.circle_stroke(
                                center,
                                outer_r + 16.0,
                                Stroke::new(1.0, Color32::from_rgb(46, 52, 66)),
                            );
                            for i in 0..zones {
                                let rgb = st
                                    .logo_pixels
                                    .get(i)
                                    .copied()
                                    .unwrap_or([90, 45, 130]);
                                let p = led_ring_position(center, outer_r + 16.0, i, zones);
                                draw_ring_led(&painter, p, 4.6, rgb);
                            }
                        }

                        painter.circle_filled(center, outer_r, Color32::from_rgb(24, 28, 36));
                        painter.circle_stroke(
                            center,
                            outer_r,
                            Stroke::new(2.0, Color32::from_rgb(66, 74, 90)),
                        );
                        painter.circle_filled(
                            center,
                            outer_r * 0.65,
                            Color32::from_rgb(16, 19, 25),
                        );

                        let ring_count = st
                            .ring_leds
                            .max(st.ring_pixels.len())
                            .max(default_board_ring_leds(&board_slug))
                            .max(1);
                        let ring_r = if ring_count == 1 { 0.0 } else { outer_r * 0.85 };
                        let led_dot_r = match ring_count {
                            1..=6 => 6.4,
                            7..=16 => 5.2,
                            17..=24 => 4.2,
                            25..=36 => 3.3,
                            _ => 1.9,
                        };
                        for idx in 0..ring_count {
                            let rgb = st.ring_pixels.get(idx).copied().unwrap_or([0, 165, 210]);
                            let pos = led_ring_position(center, ring_r, idx, ring_count);
                            draw_ring_led(&painter, pos, led_dot_r, rgb);
                        }

                        let doa_pos =
                            led_ring_position(center, outer_r + 4.0, st.doa_angle as usize, 360);
                        painter.line_segment(
                            [center, doa_pos],
                            Stroke::new(1.6, Color32::from_rgba_unmultiplied(0, 229, 255, 140)),
                        );
                        painter.circle_filled(doa_pos, 5.2, Color32::from_rgb(0, 229, 255));

                        if board_slug == "yandexmini" && st.tof_mm < 400 {
                            painter.circle_filled(
                                center,
                                outer_r * 0.52,
                                Color32::from_rgba_unmultiplied(229, 165, 10, 55),
                            );
                            painter.circle_stroke(
                                center,
                                outer_r * 0.52,
                                Stroke::new(1.5, Color32::from_rgb(235, 175, 25)),
                            );
                            painter.text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                format!("РУКА: {} мм\nDOA {} град", st.tof_mm, st.doa_angle),
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(255, 230, 110),
                            );
                        } else {
                            painter.text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                format!(
                                    "DOA {} град\n{} LED | Громк {}/10",
                                    st.doa_angle, ring_count, st.volume_level
                                ),
                                egui::FontId::monospace(12.0),
                                Color32::from_rgb(195, 210, 230),
                            );
                        }
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

fn render_segment_clock_faceplate(
    ui: &mut egui::Ui,
    app: &mut GuiAppState,
    board_slug: &str,
    st: &BridgeStateSnapshot,
    bridge_online: bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("7-сегментный дисплей (45 сегментов)")
                .small()
                .color(Color32::from_rgb(165, 175, 190)),
        );
        ui.separator();
        ui.label(
            RichText::new("ЛКМ: рисовать | ПКМ: стирать")
                .small()
                .color(Color32::from_rgb(130, 195, 255)),
        );
        ui.separator();
        ui.label(
            RichText::new(format!("PWM: {}", app.clock_brightness))
                .small()
                .monospace()
                .color(Color32::from_rgb(155, 175, 200)),
        );
    });

    if bridge_online && st.clock_segments.values().any(|&v| v > 0) {
        app.clock_active_mask.clear();
        for (k, &v) in &st.clock_segments {
            if v > 0 {
                app.clock_active_mask.insert(k.clone(), true);
            }
        }
    }

    let mut active_segs = BTreeMap::new();
    if bridge_online && !st.clock_segments.is_empty() {
        active_segs = st.clock_segments.clone();
    } else {
        for (k, &on) in &app.clock_active_mask {
            if on && app.clock_brightness > 0 {
                active_segs.insert(k.clone(), app.clock_brightness);
            }
        }
    }

    let (crect, cresp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(240.0), 64.0),
        Sense::click_and_drag(),
    );
    let cp = ui.painter_at(crect);
    draw_segment_clock(&cp, crect, &active_segs);

    let lmb_down = ui.input(|i| i.pointer.primary_down());
    let rmb_down = ui.input(|i| i.pointer.secondary_down());
    let lmb_clicked = cresp.clicked();
    let rmb_clicked = cresp.secondary_clicked();
    if (lmb_down || rmb_down || lmb_clicked || rmb_clicked)
        && (cresp.hovered() || cresp.dragged() || lmb_clicked || rmb_clicked)
    {
        if let Some(ptr) = ui
            .input(|i| i.pointer.interact_pos())
            .or_else(|| cresp.interact_pointer_pos())
        {
            if let Some(seg_key) = hit_test_clock_segment(crect, ptr) {
                let draw_mode = (lmb_down || lmb_clicked) && !(rmb_down || rmb_clicked);
                let is_on = app.clock_active_mask.get(&seg_key).copied().unwrap_or(false)
                    && segment_brightness(&active_segs, &seg_key) > 0;
                if draw_mode != is_on {
                    let next_val = if draw_mode {
                        if app.clock_brightness == 0 {
                            app.clock_brightness = 255;
                        }
                        app.clock_active_mask.insert(seg_key.clone(), true);
                        app.clock_brightness
                    } else {
                        app.clock_active_mask.remove(&seg_key);
                        0
                    };
                    if let Some(ref mut latest) = app.latest_state {
                        latest.clock_segments.insert(seg_key.clone(), next_val);
                    }
                    let mut map = BTreeMap::new();
                    map.insert(seg_key, next_val);
                    app.send_command(HostCommand::SetClockDisplay {
                        segments: Some(map),
                        replace: Some(false),
                        pixels: None,
                        led_frame: None,
                        brightness: Some(app.clock_brightness),
                        scale_brightness: None,
                        text: None,
                    });
                }
            }
        }
    }

    ui.horizontal_wrapped(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.clock_text_input)
                .desired_width(82.0)
                .hint_text("19:30"),
        );
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter || ui.button("Вывести").clicked() {
            let txt = app.clock_text_input.clone();
            if !txt.is_empty() && app.clock_brightness == 0 {
                app.clock_brightness = 255;
            }
            let built = build_local_segments_from_text(&txt, 255);
            app.clock_active_mask = built
                .into_iter()
                .filter(|(_, v)| *v > 0)
                .map(|(k, _)| (k, true))
                .collect();
            if let Some(ref mut latest) = app.latest_state {
                latest.clock_segments =
                    build_local_segments_from_text(&txt, app.clock_brightness);
            }
            app.send_command(HostCommand::SetClockDisplay {
                segments: None,
                replace: None,
                pixels: None,
                led_frame: None,
                brightness: Some(app.clock_brightness),
                scale_brightness: None,
                text: Some(txt),
            });
        }
        for preset in ["19:30", "+24°", "-05°", ""] {
            let lbl = if preset.is_empty() { "Погасить" } else { preset };
            if ui.button(lbl).clicked() {
                app.clock_text_input = preset.to_string();
                let br = if preset.is_empty() {
                    app.clock_active_mask.clear();
                    0
                } else {
                    if app.clock_brightness == 0 {
                        app.clock_brightness = 255;
                    }
                    let built = build_local_segments_from_text(preset, 255);
                    app.clock_active_mask = built
                        .into_iter()
                        .filter(|(_, v)| *v > 0)
                        .map(|(k, _)| (k, true))
                        .collect();
                    app.clock_brightness
                };
                if let Some(ref mut latest) = app.latest_state {
                    latest.clock_segments = build_local_segments_from_text(preset, br);
                }
                app.send_command(HostCommand::SetClockDisplay {
                    segments: None,
                    replace: None,
                    pixels: None,
                    led_frame: None,
                    brightness: Some(br),
                    scale_brightness: None,
                    text: Some(preset.to_string()),
                });
            }
        }
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Яркость PWM:").small());
        if ui
            .add(egui::Slider::new(&mut app.clock_brightness, 0..=255))
            .changed()
        {
            if let Some(ref mut latest) = app.latest_state {
                for (k, v) in latest.clock_segments.iter_mut() {
                    if app.clock_active_mask.get(k).copied().unwrap_or(false) {
                        *v = app.clock_brightness;
                    } else {
                        *v = 0;
                    }
                }
            }
            app.send_command(HostCommand::SetClockDisplay {
                segments: None,
                replace: None,
                pixels: None,
                led_frame: None,
                brightness: None,
                scale_brightness: Some(app.clock_brightness),
                text: None,
            });
        }
    });

    render_aux_logo_strip(ui, app, board_slug, st);
}

fn render_aux_logo_strip(
    ui: &mut egui::Ui,
    app: &mut GuiAppState,
    board_slug: &str,
    st: &BridgeStateSnapshot,
) {
    if st.aux_light_kind == "logo" || matches!(board_slug, "cucumber" | "pickle" | "mango") {
        let count = if !st.logo_pixels.is_empty() {
            st.logo_pixels.len()
        } else if board_slug == "mango" {
            3
        } else {
            6
        };
        let hover_desc = if board_slug == "mango" {
            "3-LED RGBW панель логотипа «oknix» на корпусе Яндекс Станции Стрит (mango):\n\
             • Аппаратный драйвер: Awinic AW21012 (leds-aw2101x.ko) + фреймбуфер quasar_led_ring.ko\n\
             • Запись через фреймбуфер /dev/fb1 (name=\"logo\" / \"oknix\"): 9 байт RGB24 (3×3) или 12 байт RGBW32 (3×4)\n\
             • Поканальное управление через sysfs: /sys/class/leds/mango:{red,green,blue}:logo{0..2}/brightness (0..255)\n\
             • Штатные анимации Алисы: /system/vendor/quasar/ledpatterns/oknix/*.led"
        } else {
            "Дополнительная адресная RGB-подсветка логотипа Алисы на фронтальной панели:\n\
             • Запись кадра через фреймбуфер /dev/fb1 (name=\"logo\"): 3 байта RGB на каждый светодиод\n\
             • Поканальное управление через sysfs: /sys/class/leds/<prefix>:{red,green,blue}:logo{0..N}/brightness (0..255)"
        };

        ui.horizontal_wrapped(|ui| {
            let lbl_text = if board_slug == "mango" {
                format!("Логотип oknix (/dev/fb1, {} LED): (?)", count)
            } else {
                format!("Логотип (/dev/fb1, {} LED): (?)", count)
            };
            ui.label(
                RichText::new(lbl_text)
                    .small()
                    .strong()
                    .color(Color32::from_rgb(160, 185, 215)),
            )
            .on_hover_text(hover_desc);

            let (lrect, lresp) =
                ui.allocate_exact_size(Vec2::new((count as f32) * 20.0 + 16.0, 20.0), Sense::hover());
            lresp.on_hover_text(hover_desc);
            let lp = ui.painter_at(lrect);
            for i in 0..count {
                let rgb = st
                    .logo_pixels
                    .get(i)
                    .copied()
                    .unwrap_or([0, 185, 235]);
                let x = lrect.left() + 10.0 + (i as f32) * 20.0;
                draw_ring_led(&lp, Pos2::new(x, lrect.center().y), 4.6, rgb);
            }

            if app.session.is_some() {
                if ui
                    .small_button("Фиолетовый")
                    .on_hover_text("Записать фиолетовый цвет Алисы (#6B0FFF) в /dev/fb1")
                    .clicked()
                {
                    app.send_serial_command(
                        "printf '\\x6b\\x0f\\xff\\x6b\\x0f\\xff\\x6b\\x0f\\xff\\x6b\\x0f\\xff\\x6b\\x0f\\xff\\x6b\\x0f\\xff' > /dev/fb1",
                    );
                }
                if ui
                    .small_button("Бирюзовый")
                    .on_hover_text("Записать бирюзовый цвет (#22CCFF) в /dev/fb1")
                    .clicked()
                {
                    app.send_serial_command(
                        "printf '\\x22\\xcc\\xff\\x22\\xcc\\xff\\x22\\xcc\\xff\\x22\\xcc\\xff\\x22\\xcc\\xff\\x22\\xcc\\xff' > /dev/fb1",
                    );
                }
                if ui
                    .small_button("Погасить")
                    .on_hover_text("Очистить /dev/fb1 и сбросить яркость logo0..N в sysfs в 0")
                    .clicked()
                {
                    app.send_serial_command(
                        "for d in /sys/class/leds/*:logo*; do [ -d \"$d\" ] && echo 0 > \"$d/brightness\"; done; dd if=/dev/zero of=/dev/fb1 bs=18 count=1 2>/dev/null || true",
                    );
                }
            }
        });
    }
}

fn render_matrix_21x12_faceplate(
    ui: &mut egui::Ui,
    app: &mut GuiAppState,
    st: &BridgeStateSnapshot,
    bridge_online: bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("Матрица 21x12 LED (/dev/fb1)")
                .small()
                .color(Color32::from_rgb(165, 175, 190)),
        );
        ui.separator();
        ui.label(
            RichText::new("ЛКМ: рисовать | ПКМ: стирать")
                .small()
                .color(Color32::from_rgb(130, 195, 255)),
        );
        ui.separator();
        ui.label(
            RichText::new(format!("PWM: {}", app.clock_brightness))
                .small()
                .monospace()
                .color(Color32::from_rgb(155, 175, 200)),
        );
    });

    if app.matrix_custom_pixels.len() < 252 {
        app.matrix_custom_pixels.resize(252, 0);
    }
    if app.matrix_active_mask.len() < 252 {
        app.matrix_active_mask.resize(252, false);
    }

    if bridge_online && st.screen_pixels.len() == 252 && app.clock_brightness > 0 {
        app.matrix_custom_pixels = st.screen_pixels.clone();
        if st.screen_pixels.iter().any(|&v| v > 0) {
            app.matrix_active_mask = st.screen_pixels.iter().map(|&v| v > 0).collect();
        }
    }

    let cell = 13.0;
    let msize = Vec2::new(21.0 * cell + 10.0, 12.0 * cell + 10.0);
    let (mrect, mresp) = ui.allocate_exact_size(msize, Sense::click_and_drag());
    let mp = ui.painter_at(mrect);
    mp.rect_filled(mrect, 6.0, Color32::from_rgb(13, 15, 20));
    mp.rect_stroke(mrect, 6.0, Stroke::new(1.0, Color32::from_rgb(48, 56, 70)));

    let lmb_down = ui.input(|i| i.pointer.primary_down());
    let rmb_down = ui.input(|i| i.pointer.secondary_down());
    if (lmb_down || rmb_down) && (mresp.hovered() || mresp.dragged() || mresp.clicked()) {
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
            if let Some(idx) = hit_test_matrix_pixel(mrect, cell, p) {
                let draw_mode = lmb_down && !rmb_down;
                if draw_mode && app.clock_brightness == 0 {
                    app.clock_brightness = 255;
                }
                let next_v = if draw_mode {
                    app.clock_brightness.max(1)
                } else {
                    0
                };
                if app.matrix_custom_pixels[idx] != next_v
                    || app.matrix_active_mask[idx] != draw_mode
                {
                    app.matrix_active_mask[idx] = draw_mode;
                    app.matrix_custom_pixels[idx] = next_v;
                    if let Some(ref mut latest) = app.latest_state {
                        if latest.screen_pixels.len() < 252 {
                            latest.screen_pixels.resize(252, 0);
                        }
                        latest.screen_pixels[idx] = next_v;
                    }
                    app.send_command(HostCommand::SetClockDisplay {
                        segments: None,
                        replace: None,
                        pixels: Some(app.matrix_custom_pixels.clone()),
                        led_frame: None,
                        brightness: Some(app.clock_brightness),
                        scale_brightness: None,
                        text: None,
                    });
                }
            }
        }
    }

    for row in 0..12 {
        for col in 0..21 {
            let idx = row * 21 + col;
            let v = if app.clock_brightness == 0 {
                0
            } else {
                app.matrix_custom_pixels.get(idx).copied().unwrap_or(0)
            };
            let dot_pos = Pos2::new(
                mrect.left() + 5.0 + (col as f32 + 0.5) * cell,
                mrect.top() + 5.0 + (row as f32 + 0.5) * cell,
            );
            draw_matrix_dot(&mp, dot_pos, 4.2, v);
        }
    }

    ui.horizontal_wrapped(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.matrix_text_input)
                .desired_width(82.0)
                .hint_text("19:30"),
        );
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter || ui.button("Вывести").clicked() {
            let txt = app.matrix_text_input.clone();
            if !txt.is_empty() && app.clock_brightness == 0 {
                app.clock_brightness = 255;
            }
            let mask_px = render_text_to_matrix_21x12(&txt, 255);
            app.matrix_active_mask = mask_px.iter().map(|&v| v > 0).collect();
            app.matrix_custom_pixels = render_text_to_matrix_21x12(&txt, app.clock_brightness);
            if let Some(ref mut latest) = app.latest_state {
                latest.screen_pixels = app.matrix_custom_pixels.clone();
            }
            app.send_command(HostCommand::SetClockDisplay {
                segments: None,
                replace: None,
                pixels: None,
                led_frame: None,
                brightness: Some(app.clock_brightness),
                scale_brightness: None,
                text: Some(txt),
            });
        }
        for preset in ["19:30", "+24°", "-05°", ""] {
            let lbl = if preset.is_empty() { "Погасить" } else { preset };
            if ui.button(lbl).clicked() {
                app.matrix_text_input = preset.to_string();
                let br = if preset.is_empty() {
                    app.matrix_active_mask.fill(false);
                    app.matrix_custom_pixels.fill(0);
                    0
                } else {
                    if app.clock_brightness == 0 {
                        app.clock_brightness = 255;
                    }
                    let mask_px = render_text_to_matrix_21x12(preset, 255);
                    app.matrix_active_mask = mask_px.iter().map(|&v| v > 0).collect();
                    app.matrix_custom_pixels =
                        render_text_to_matrix_21x12(preset, app.clock_brightness);
                    app.clock_brightness
                };
                if let Some(ref mut latest) = app.latest_state {
                    latest.screen_pixels = app.matrix_custom_pixels.clone();
                }
                app.send_command(HostCommand::SetClockDisplay {
                    segments: None,
                    replace: None,
                    pixels: None,
                    led_frame: None,
                    brightness: Some(br),
                    scale_brightness: None,
                    text: Some(preset.to_string()),
                });
            }
        }
        if ui.button("Волна").clicked() {
            if app.clock_brightness == 0 {
                app.clock_brightness = 255;
            }
            app.matrix_active_mask.fill(false);
            app.matrix_custom_pixels.fill(0);
            for c in 0..21 {
                let h = (3.0 + ((c as f32) * 0.7).sin() * 3.0).round() as usize;
                for r in (11usize.saturating_sub(h))..11 {
                    let idx = r * 21 + c;
                    app.matrix_active_mask[idx] = true;
                    app.matrix_custom_pixels[idx] = app.clock_brightness;
                }
            }
            if let Some(ref mut latest) = app.latest_state {
                latest.screen_pixels = app.matrix_custom_pixels.clone();
            }
            app.send_command(HostCommand::SetClockDisplay {
                segments: None,
                replace: None,
                pixels: Some(app.matrix_custom_pixels.clone()),
                led_frame: None,
                brightness: Some(app.clock_brightness),
                scale_brightness: None,
                text: None,
            });
        }
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Яркость PWM:").small());
        if ui
            .add(egui::Slider::new(&mut app.clock_brightness, 0..=255))
            .changed()
        {
            for idx in 0..252 {
                app.matrix_custom_pixels[idx] = if app.matrix_active_mask[idx] {
                    app.clock_brightness
                } else {
                    0
                };
            }
            if let Some(ref mut latest) = app.latest_state {
                latest.screen_pixels = app.matrix_custom_pixels.clone();
            }
            app.send_command(HostCommand::SetClockDisplay {
                segments: None,
                replace: None,
                pixels: None,
                led_frame: None,
                brightness: None,
                scale_brightness: Some(app.clock_brightness),
                text: None,
            });
        }
    });
}
