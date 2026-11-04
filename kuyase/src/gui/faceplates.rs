use crate::gui::canvas::{draw_circular_oknix_logo, draw_ring_led};
use crate::gui::display_geom::{
    build_station2_preset_frame, clock_inspector_text, clock_segment_lines_for_board,
    clock_special_positions, format_station2_frame_command, hit_test_clock_segment_for_board,
    hit_test_matrix_pixel, hit_test_station2_faceplate_pixel, mango_battery_led_rgb,
    matrix_inspector_text, scale_rgb_brightness, scale_station2_frame_brightness,
    station2_faceplate_layout, station2_faceplate_pixel_center, station2_panel_inspector_text,
    STATION2_PANEL_COLS, STATION2_PANEL_LEDS, STATION2_PANEL_ROWS,
};
use crate::gui::state::GuiAppState;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use protocol::display::{
    board_clock_profile, build_segments_map_from_text_for_board, render_text_to_matrix_21x12,
};
use protocol::{BridgeStateSnapshot, HostCommand};
use std::collections::BTreeMap;

fn pointer_draw_erase_mode(ui: &egui::Ui, resp: &egui::Response) -> Option<(Pos2, bool)> {
    let lmb_down = ui.input(|i| i.pointer.primary_down());
    let rmb_down = ui.input(|i| i.pointer.secondary_down());
    let lmb_clicked = resp.clicked();
    let rmb_clicked = resp.secondary_clicked();
    if (lmb_down || rmb_down || lmb_clicked || rmb_clicked)
        && (resp.hovered() || resp.dragged() || lmb_clicked || rmb_clicked)
    {
        let ptr = ui
            .input(|i| i.pointer.interact_pos())
            .or_else(|| resp.interact_pointer_pos())?;
        let draw_mode = (lmb_down || lmb_clicked) && !(rmb_down || rmb_clicked);
        Some((ptr, draw_mode))
    } else {
        None
    }
}

fn apply_clock_text(app: &mut GuiAppState, board_slug: &str, text: &str) {
    let (br, built) = if text.is_empty() {
        app.clock_active_mask.clear();
        (0, BTreeMap::new())
    } else {
        if app.clock_brightness == 0 {
            app.clock_brightness = 255;
        }
        let built =
            build_segments_map_from_text_for_board(board_slug, text, app.clock_brightness);
        app.clock_active_mask = built
            .iter()
            .filter(|(_, &v)| v > 0)
            .map(|(k, _)| (k.clone(), true))
            .collect();
        (app.clock_brightness, built)
    };
    if let Some(ref mut latest) = app.latest_state {
        latest.clock_segments = built;
    }
    app.send_command(HostCommand::SetClockDisplay {
        segments: None,
        replace: None,
        pixels: None,
        led_frame: None,
        brightness: Some(br),
        scale_brightness: None,
        text: Some(text.to_string()),
    });
}

fn apply_matrix_text(app: &mut GuiAppState, text: &str) {
    let br = if text.is_empty() {
        app.matrix_active_mask.fill(false);
        app.matrix_custom_pixels.fill(0);
        0
    } else {
        if app.clock_brightness == 0 {
            app.clock_brightness = 255;
        }
        let px = render_text_to_matrix_21x12(text, app.clock_brightness);
        app.matrix_active_mask = px.iter().map(|&v| v > 0).collect();
        app.matrix_custom_pixels = px;
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
        text: Some(text.to_string()),
    });
}

pub(crate) fn render_display_inspector_bar(ui: &mut egui::Ui, text: &str, is_active_hover: bool) {
    egui::Frame::none()
        .fill(Color32::from_rgb(16, 20, 27))
        .stroke(Stroke::new(
            1.0,
            if is_active_hover {
                Color32::from_rgb(65, 115, 165)
            } else {
                Color32::from_rgb(40, 47, 58)
            },
        ))
        .rounding(4.0)
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width().max(230.0));
            ui.add(
                egui::Label::new(
                    RichText::new(text)
                        .small()
                        .monospace()
                        .color(if is_active_hover {
                            Color32::from_rgb(185, 225, 255)
                        } else {
                            Color32::from_rgb(135, 148, 168)
                        }),
                )
                .truncate(),
            );
        });
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
        painter.circle_filled(
            pos,
            radius,
            Color32::from_rgb(c, c, (c as u16 + 15).min(255) as u8),
        );
    } else {
        painter.circle_filled(pos, radius * 0.88, Color32::from_rgb(26, 30, 38));
        painter.circle_stroke(
            pos,
            radius * 0.88,
            Stroke::new(0.8, Color32::from_rgb(44, 50, 62)),
        );
    }
}

fn draw_segment_clock(
    painter: &egui::Painter,
    board_slug: &str,
    rect: Rect,
    segments: &BTreeMap<String, u8>,
    hovered_seg: Option<&str>,
) {
    painter.rect_filled(rect, 6.0, Color32::from_rgb(13, 16, 21));
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, Color32::from_rgb(51, 59, 72)));

    let seg_color = |br: u8, hovered: bool| {
        if br > 0 {
            Color32::from_rgba_unmultiplied(232, 246, 255, br.max(55))
        } else if hovered {
            Color32::from_rgb(95, 170, 230)
        } else {
            Color32::from_rgb(37, 43, 54)
        }
    };

    for (name, p0, p1) in clock_segment_lines_for_board(board_slug, rect) {
        let br = segments.get(&name).copied().unwrap_or(0);
        let is_hovered = hovered_seg == Some(name.as_str());
        if is_hovered {
            painter.line_segment(
                [p0, p1],
                Stroke::new(6.2, Color32::from_rgba_unmultiplied(80, 195, 255, 95)),
            );
        }
        let is_aux_bar = matches!(
            name.as_str(),
            "p4" | "p5" | "p6" | "p7" | "q4" | "q5" | "q6" | "q7" | "h1" | "h2" | "l1" | "l2"
        );
        let stroke_w = if is_aux_bar { 3.0 } else { 3.6 };
        painter.line_segment([p0, p1], Stroke::new(stroke_w, seg_color(br, is_hovered)));
    }

    let (p3_pos, q3_pos, deg_pos) = clock_special_positions(rect);

    let p3 = segments.get("p3").copied().unwrap_or(0);
    let p3_hov = hovered_seg == Some("p3");
    if p3_hov {
        painter.circle_stroke(p3_pos, 5.0, Stroke::new(1.4, Color32::from_rgb(95, 195, 255)));
    }
    painter.circle_filled(p3_pos, 2.8, seg_color(p3, p3_hov));

    let q3 = segments.get("q3").copied().unwrap_or(0);
    let q3_hov = hovered_seg == Some("q3");
    if q3_hov {
        painter.circle_stroke(q3_pos, 5.0, Stroke::new(1.4, Color32::from_rgb(95, 195, 255)));
    }
    painter.circle_filled(q3_pos, 2.8, seg_color(q3, q3_hov));

    let deg_br = segments.get("deg").copied().unwrap_or(0);
    let deg_hov = hovered_seg == Some("deg");
    if deg_hov {
        painter.circle_stroke(deg_pos, 6.2, Stroke::new(1.4, Color32::from_rgb(95, 195, 255)));
    }
    painter.circle_stroke(deg_pos, 4.2, Stroke::new(2.4, seg_color(deg_br, deg_hov)));
}

pub fn render_segment_clock_faceplate(
    ui: &mut egui::Ui,
    app: &mut GuiAppState,
    board_slug: &str,
    st: &BridgeStateSnapshot,
    bridge_online: bool,
) {
    let clock_profile = board_clock_profile(board_slug);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(clock_profile.header_label)
                .small()
                .color(Color32::from_rgb(165, 175, 190)),
        )
        .on_hover_text(clock_profile.tooltip);
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
        Vec2::new(ui.available_width().max(240.0), 76.0),
        Sense::click_and_drag(),
    );
    let hover_pos = cresp
        .hover_pos()
        .or_else(|| cresp.interact_pointer_pos());
    let hovered_seg = if cresp.hovered() || cresp.dragged() {
        hover_pos.and_then(|ptr| hit_test_clock_segment_for_board(board_slug, crect, ptr))
    } else {
        None
    };
    if hovered_seg.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let cp = ui.painter_at(crect);
    draw_segment_clock(&cp, board_slug, crect, &active_segs, hovered_seg.as_deref());

    if let Some((ptr, draw_mode)) = pointer_draw_erase_mode(ui, &cresp) {
        if let Some(seg_key) = hit_test_clock_segment_for_board(board_slug, crect, ptr) {
            let is_on = app.clock_active_mask.get(&seg_key).copied().unwrap_or(false)
                && active_segs.get(&seg_key).copied().unwrap_or(0) > 0;
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
                active_segs.insert(seg_key.clone(), next_val);
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

    let inspector_line = clock_inspector_text(
        board_slug,
        hovered_seg.as_deref(),
        &active_segs,
        app.clock_brightness,
    );
    render_display_inspector_bar(ui, &inspector_line, hovered_seg.is_some());

    ui.horizontal_wrapped(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.clock_text_input)
                .desired_width(82.0)
                .hint_text("19:30"),
        );
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter || ui.button("Вывести").clicked() {
            let txt = app.clock_text_input.clone();
            apply_clock_text(app, board_slug, &txt);
        }
        for (lbl, preset) in [
            ("19:30", "19:30"),
            ("+24°", "+24°"),
            ("-05°", "-05°"),
            ("Глазки", "eyes"),
            ("Погасить", ""),
        ] {
            if ui.button(lbl).clicked() {
                app.clock_text_input = preset.to_string();
                apply_clock_text(app, board_slug, preset);
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

pub fn render_aux_logo_strip(
    ui: &mut egui::Ui,
    app: &mut GuiAppState,
    board_slug: &str,
    st: &BridgeStateSnapshot,
) {
    if board_slug == "mango" {
        let batt_desc = "4-LED линейная шкала заряда АКБ на боковой панели Яндекс Станции Стрит (mango):\n\
             • Аппаратный драйвер: Awinic AW9110 + фреймбуфер /dev/fb0 (name=\"ring\", 12 байт RGB24)\n\
             • Поканальное управление: /sys/class/leds/mango:{red,green,blue}:ring{1..4}/brightness\n\
             • Штатные паттерны: /system/vendor/quasar/ledpatterns/battery/*.led";

        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Шкала АКБ (/dev/fb0, 4 LED): (?)")
                    .small()
                    .strong()
                    .color(Color32::from_rgb(160, 185, 215)),
            )
            .on_hover_text(batt_desc);

            let (brect, _bresp) = ui.allocate_exact_size(Vec2::new(72.0, 22.0), Sense::hover());
            let bp = ui.painter_at(brect);
            bp.rect_filled(brect, 11.0, Color32::from_rgb(15, 18, 24));
            bp.rect_stroke(brect, 11.0, Stroke::new(1.0, Color32::from_rgb(50, 58, 72)));
            for idx in 0..4 {
                let rgb = mango_battery_led_rgb(&st.ring_pixels, idx);
                let x = brect.left() + 12.0 + (idx as f32) * 16.0;
                draw_ring_led(&bp, Pos2::new(x, brect.center().y), 3.8, rgb);
            }

            for (lbl, lit) in [("4/4", 4usize), ("3/4", 3usize), ("1/4", 1usize), ("0/4", 0usize)] {
                if ui.small_button(lbl).clicked() {
                    let mut px = vec![[0u8, 0, 0]; 4];
                    for item in px.iter_mut().take(lit) {
                        *item = [235, 245, 255];
                    }
                    let latest = app.latest_state.get_or_insert_with(|| st.clone());
                    latest.ring_pixels = px;
                    if app.session.is_some() {
                        app.send_serial_command(&format!(
                            "for i in 1 2 3 4; do echo none > /sys/class/leds/mango:red:ring$i/trigger; \
                             v=0; [ $i -le {lit} ] && v=255; \
                             for c in red green blue; do echo $v > /sys/class/leds/mango:$c:ring$i/brightness; done; done"
                        ));
                    }
                }
            }
        });
        ui.add_space(2.0);
    }

    if st.aux_light_kind == "logo" || matches!(board_slug, "cucumber" | "pickle" | "mango") {
        let count = if !st.logo_pixels.is_empty() {
            st.logo_pixels.len()
        } else if board_slug == "mango" {
            3
        } else {
            6
        };
        let hover_desc = if board_slug == "mango" {
            "Круговой 3-LED RGBW логотип Алисы «oknix» между кнопками «-» и «+» на Станции Стрит (mango):\n\
             • Аппаратный драйвер: Awinic AW21012 (leds-aw2101x.ko) + фреймбуфер quasar_led_ring.ko\n\
             • Запись через фреймбуфер /dev/fb1 (name=\"logo\" / \"oknix\"): 9 байт RGB24 (3×3) или 12 байт RGBW32 (3×4)\n\
             • Поканальное управление через sysfs: /sys/class/leds/mango:{red,green,blue}:logo{0..2}/brightness (0..255)\n\
             • Штатные анимации Алисы: /system/vendor/quasar/ledpatterns/oknix/*.led"
        } else {
            "Центральный круговой 6-LED RGB-логотип Алисы («oknix») на верхней крышке:\n\
             • Запись кадра через фреймбуфер /dev/fb1 (name=\"logo\"): 18 байт RGB24 (6×3)\n\
             • Поканальное управление через sysfs: /sys/class/leds/<prefix>:{red,green,blue}:logo{0..5}/brightness (0..255)\n\
             • Штатные анимации Алисы: /system/vendor/quasar/ledpatterns/oknix/*.led"
        };

        ui.horizontal_wrapped(|ui| {
            let lbl_text = format!("Круговой логотип oknix (/dev/fb1, {} LED): (?)", count);
            ui.label(
                RichText::new(lbl_text)
                    .small()
                    .strong()
                    .color(Color32::from_rgb(160, 185, 215)),
            )
            .on_hover_text(hover_desc);

            let (lrect, _lresp) = ui.allocate_exact_size(Vec2::new(36.0, 34.0), Sense::hover());
            let lp = ui.painter_at(lrect);
            draw_circular_oknix_logo(
                &lp,
                lrect.center(),
                9.5,
                3.4,
                &st.logo_pixels,
                count,
                [135, 35, 255],
            );

            if ui.small_button("Фиолетовый").clicked() {
                let latest = app.latest_state.get_or_insert_with(|| st.clone());
                latest.logo_pixels = vec![[0x6b, 0x0f, 0xff]; count];
                if app.session.is_some() {
                    let hex_rep = "\\x6b\\x0f\\xff".repeat(count);
                    app.send_serial_command(&format!("printf '{}' > /dev/fb1", hex_rep));
                }
            }
            if ui.small_button("Бирюзовый").clicked() {
                let latest = app.latest_state.get_or_insert_with(|| st.clone());
                latest.logo_pixels = vec![[0x22, 0xcc, 0xff]; count];
                if app.session.is_some() {
                    let hex_rep = "\\x22\\xcc\\xff".repeat(count);
                    app.send_serial_command(&format!("printf '{}' > /dev/fb1", hex_rep));
                }
            }
            if ui.small_button("Алиса Про").clicked() {
                let mut grad = Vec::with_capacity(count);
                let mut hex_rep = String::new();
                for i in 0..count {
                    if i % 2 == 0 {
                        grad.push([0x00, 0xff, 0x80]);
                        hex_rep.push_str("\\x00\\xff\\x80");
                    } else {
                        grad.push([0x6b, 0x0f, 0xff]);
                        hex_rep.push_str("\\x6b\\x0f\\xff");
                    }
                }
                let latest = app.latest_state.get_or_insert_with(|| st.clone());
                latest.logo_pixels = grad;
                if app.session.is_some() {
                    app.send_serial_command(&format!("printf '{}' > /dev/fb1", hex_rep));
                }
            }
            if ui.small_button("Погасить").clicked() {
                let latest = app.latest_state.get_or_insert_with(|| st.clone());
                latest.logo_pixels = vec![[0, 0, 0]; count];
                if app.session.is_some() {
                    app.send_serial_command(&format!(
                        "for d in /sys/class/leds/*:logo*; do [ -d \"$d\" ] && echo 0 > \"$d/brightness\"; done; dd if=/dev/zero of=/dev/fb1 bs={} count=1 2>/dev/null || true",
                        count * 3
                    ));
                }
            }
        });
    }
}

pub fn render_matrix_21x12_faceplate(
    ui: &mut egui::Ui,
    app: &mut GuiAppState,
    st: &BridgeStateSnapshot,
    bridge_online: bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("Матрица 21x12 LED (/dev/fb1) (?)")
                .small()
                .color(Color32::from_rgb(165, 175, 190)),
        )
        .on_hover_text(
            "Монохромная светодиодная матрица 21×12 (252 LED) Яндекс Станции 3 (orion):\n\
             • Аппаратный драйвер: FPGA Gowin (gowin_led_device) + фреймбуфер /dev/fb1 (name=\"YNDX LED screen\")\n\
             • Формат кадра: 21×12 пикселей, 24 bpp RGB (756 байт), яркость каждого пикселя 0..255\n\
             • Штатные анимации экрана: /system/vendor/quasar/ledpatterns/screen/*.led",
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
    let hover_pos = mresp
        .hover_pos()
        .or_else(|| mresp.interact_pointer_pos());
    let hovered_idx = if mresp.hovered() || mresp.dragged() {
        hover_pos.and_then(|p| hit_test_matrix_pixel(mrect, cell, p))
    } else {
        None
    };
    if hovered_idx.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let mp = ui.painter_at(mrect);
    mp.rect_filled(mrect, 6.0, Color32::from_rgb(13, 15, 20));
    mp.rect_stroke(mrect, 6.0, Stroke::new(1.0, Color32::from_rgb(48, 56, 70)));

    if let Some((p, draw_mode)) = pointer_draw_erase_mode(ui, &mresp) {
        if let Some(idx) = hit_test_matrix_pixel(mrect, cell, p) {
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
            if hovered_idx == Some(idx) {
                mp.circle_stroke(dot_pos, 5.6, Stroke::new(1.3, Color32::from_rgb(95, 195, 255)));
            }
            draw_matrix_dot(&mp, dot_pos, 4.2, v);
        }
    }

    let inspector_line = matrix_inspector_text(
        hovered_idx,
        &app.matrix_custom_pixels,
        app.clock_brightness,
    );
    render_display_inspector_bar(ui, &inspector_line, hovered_idx.is_some());

    ui.horizontal_wrapped(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.matrix_text_input)
                .desired_width(82.0)
                .hint_text("19:30"),
        );
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter || ui.button("Вывести").clicked() {
            let txt = app.matrix_text_input.clone();
            apply_matrix_text(app, &txt);
        }
        for preset in ["19:30", "+24°", "-05°", ""] {
            let lbl = if preset.is_empty() { "Погасить" } else { preset };
            if ui.button(lbl).clicked() {
                app.matrix_text_input = preset.to_string();
                apply_matrix_text(app, preset);
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

fn send_station2_custom_frame(app: &mut GuiAppState, st: &BridgeStateSnapshot) {
    let pattern = format_station2_frame_command(&app.station2_custom_pixels);
    let latest = app.latest_state.get_or_insert_with(|| st.clone());
    latest.ring_pixels = app.station2_custom_pixels.clone();
    latest.forced_led_pattern = pattern.clone();
    app.apply_preset_led_pattern(&pattern);
}

fn apply_station2_preset(app: &mut GuiAppState, st: &BridgeStateSnapshot, preset: &str) {
    if preset != "clear" && app.clock_brightness == 0 {
        app.clock_brightness = 255;
    }
    app.station2_base_pixels = build_station2_preset_frame(preset, app.station2_brush_rgb, 255);
    let eff_br = if preset == "clear" {
        0
    } else {
        app.clock_brightness
    };
    app.station2_custom_pixels =
        scale_station2_frame_brightness(&app.station2_base_pixels, eff_br);
    send_station2_custom_frame(app, st);
}

fn build_station2_show_preview_frame(pat_id: &str) -> Vec<[u8; 3]> {
    let mut out = Vec::with_capacity(STATION2_PANEL_LEDS);
    for row in 0..STATION2_PANEL_ROWS {
        for col in 0..STATION2_PANEL_COLS {
            let tx = (col as f32) / ((STATION2_PANEL_COLS - 1) as f32);
            let ty = (row as f32) / ((STATION2_PANEL_ROWS - 1) as f32);
            let base = match pat_id {
                "panel/candle.led" => [
                    255,
                    (90.0 + 95.0 * ty).round() as u8,
                    (15.0 + 40.0 * ty * (1.0 - tx)).round() as u8,
                ],
                "panel/lavalamp.led" => [
                    (220.0 + 35.0 * (1.0 - ty)).round() as u8,
                    (25.0 + 110.0 * tx).round() as u8,
                    (140.0 + 115.0 * ty).round() as u8,
                ],
                "panel/polar_lights.led" => [
                    (20.0 + 95.0 * (1.0 - ty)).round() as u8,
                    (175.0 + 80.0 * tx).round() as u8,
                    (160.0 + 95.0 * ty).round() as u8,
                ],
                "panel/night_light.led" => [
                    255,
                    (155.0 + 35.0 * (1.0 - ty)).round() as u8,
                    (55.0 + 25.0 * tx).round() as u8,
                ],
                _ => [
                    (215.0 + 40.0 * (1.0 - tx)).round() as u8,
                    (35.0 + 65.0 * ty).round() as u8,
                    (145.0 + 110.0 * tx).round() as u8,
                ],
            };
            out.push(base);
        }
    }
    out
}

pub fn render_station2_light_panel_faceplate(
    ui: &mut egui::Ui,
    app: &mut GuiAppState,
    st: &BridgeStateSnapshot,
    bridge_online: bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("Верхняя световая панель 11×8 (/dev/fb0, gowin_led_device) (?)")
                .small()
                .strong()
                .color(Color32::from_rgb(175, 190, 215)),
        )
        .on_hover_text(
            "Полноцветная верхняя световая панель Яндекс Станции 2 (station2 / YNDX-00051):\n\
             • Аппаратный драйвер: SPI FPGA Gowin (gowin_led_device) + фреймбуфер /dev/fb0 (88 RGB24 пикселей, 264 байта)\n\
             • Поканальное управление sysfs: /sys/class/leds/station2:{red,green,blue}:ring0..87/brightness (0..255)\n\
             • Штатные полноцветные анимации панели: /system/vendor/quasar/ledpatterns/panel/*.led",
        );

        ui.add(
            egui::Button::new(
                RichText::new("ВИД СВЕРХУ (КРЫШКА) · Фронтального экрана нет")
                    .small()
                    .strong()
                    .color(Color32::from_rgb(255, 215, 120)),
            )
            .fill(Color32::from_rgb(58, 44, 20))
            .stroke(Stroke::new(1.0, Color32::from_rgb(135, 100, 42)))
            .rounding(99.0)
            .sense(Sense::hover()),
        )
        .on_hover_text(
            "На Яндекс Станции 2 (station2) физически отсутствует боковой/фронтальный дисплей.\n\
             Этот блок представляет собой увеличенную проекцию («лупу») верхней горизонтальной крышки колонки\n\
             прямо под сенсорной планкой [ − ] [ ○ ] [ + ] для удобного попиксельного RGB-рисования и запуска шоу.",
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

    if bridge_online
        && st.ring_pixels.len() >= STATION2_PANEL_LEDS
        && !st.forced_led_pattern.starts_with("frame:")
    {
        app.station2_custom_pixels
            .copy_from_slice(&st.ring_pixels[..STATION2_PANEL_LEDS]);
        app.station2_base_pixels
            .copy_from_slice(&st.ring_pixels[..STATION2_PANEL_LEDS]);
    }

    let canvas_size = Vec2::new(ui.available_width().max(260.0), 146.0);
    let (outer_rect, resp) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
    let layout = station2_faceplate_layout(outer_rect);

    let hover_pos = resp
        .hover_pos()
        .or_else(|| resp.interact_pointer_pos());
    let hovered_idx = if resp.hovered() || resp.dragged() {
        hover_pos
            .and_then(|p| hit_test_station2_faceplate_pixel(layout.grid_rect, layout.cell_size, p))
    } else {
        None
    };
    if hovered_idx.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if let Some((ptr, draw_mode)) = pointer_draw_erase_mode(ui, &resp) {
        if let Some(idx) =
            hit_test_station2_faceplate_pixel(layout.grid_rect, layout.cell_size, ptr)
        {
            if draw_mode && app.clock_brightness == 0 {
                app.clock_brightness = 255;
            }
            let next_base = if draw_mode {
                app.station2_brush_rgb
            } else {
                [0, 0, 0]
            };
            let next_scaled = if draw_mode {
                scale_rgb_brightness(app.station2_brush_rgb, app.clock_brightness.max(1))
            } else {
                [0, 0, 0]
            };
            if app.station2_custom_pixels[idx] != next_scaled
                || app.station2_base_pixels[idx] != next_base
            {
                app.station2_base_pixels[idx] = next_base;
                app.station2_custom_pixels[idx] = next_scaled;
                send_station2_custom_frame(app, st);
            }
        }
    }

    let painter = ui.painter_at(outer_rect);
    painter.rect_filled(outer_rect, 8.0, Color32::from_rgb(17, 20, 27));
    painter.rect_stroke(
        outer_rect,
        8.0,
        Stroke::new(1.2, Color32::from_rgb(56, 64, 80)),
    );

    // Крупная световая панель 11×8 (88 RGBW LED, /dev/fb0)
    painter.rect_filled(layout.panel_rect, 6.0, Color32::from_rgb(11, 14, 19));
    painter.rect_stroke(
        layout.panel_rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(40, 48, 60)),
    );

    let dot_r = (layout.cell_size * 0.30).clamp(4.0, 5.2);
    for row in 0..STATION2_PANEL_ROWS {
        for col in 0..STATION2_PANEL_COLS {
            let idx = row * STATION2_PANEL_COLS + col;
            let rgb = app
                .station2_custom_pixels
                .get(idx)
                .copied()
                .unwrap_or([0, 0, 0]);
            let pos =
                station2_faceplate_pixel_center(layout.grid_rect, layout.cell_size, col, row);
            if hovered_idx == Some(idx) {
                painter.circle_stroke(
                    pos,
                    dot_r + 2.4,
                    Stroke::new(1.4, Color32::from_rgb(95, 195, 255)),
                );
            }
            draw_ring_led(&painter, pos, dot_r, rgb);
        }
    }

    let inspector_line = station2_panel_inspector_text(
        hovered_idx,
        &app.station2_custom_pixels,
        app.station2_brush_rgb,
        app.clock_brightness,
    );
    render_display_inspector_bar(ui, &inspector_line, hovered_idx.is_some());

    // Строка 1: Палитра RGB-кисти, 5 фирменных свотчей, Залить / Погасить
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Кисть:").small());
        ui.color_edit_button_srgb(&mut app.station2_brush_rgb);
        for (lbl, swatch_rgb, hex_code) in [
            ("Фиолетовый Алисы", [0x6B, 0x0F, 0xFF], "#6B0FFF"),
            ("Алиса Про", [0x00, 0xFF, 0x80], "#00FF80"),
            ("Бирюзовый", [0x00, 0xE5, 0xFF], "#00E5FF"),
            ("Свеча", [0xFF, 0x99, 0x33], "#FF9933"),
            ("Белый", [0xFF, 0xFF, 0xFF], "#FFFFFF"),
        ] {
            let is_selected = app.station2_brush_rgb == swatch_rgb;
            let btn = egui::Button::new(
                RichText::new(lbl).small().color(if is_selected {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(205, 215, 230)
                }),
            )
            .stroke(Stroke::new(
                if is_selected { 1.5 } else { 1.0 },
                Color32::from_rgb(swatch_rgb[0], swatch_rgb[1], swatch_rgb[2]),
            ));
            if ui
                .add(btn)
                .on_hover_text(format!("Выбрать цвет кисти {lbl} ({hex_code})"))
                .clicked()
            {
                app.station2_brush_rgb = swatch_rgb;
            }
        }
        ui.separator();
        if ui
            .small_button("Залить")
            .on_hover_text("Залить все 88 светодиодов выбранным цветом кисти")
            .clicked()
        {
            apply_station2_preset(app, st, "fill");
        }
        if ui
            .small_button("Погасить")
            .on_hover_text("Погасить все 88 светодиодов верхней панели (/dev/fb0)")
            .clicked()
        {
            apply_station2_preset(app, st, "clear");
        }
    });

    // Строка 2: Яркость PWM 0..255
    ui.horizontal(|ui| {
        ui.label(RichText::new("Яркость PWM:").small());
        if ui
            .add(egui::Slider::new(&mut app.clock_brightness, 0..=255))
            .changed()
        {
            app.station2_custom_pixels =
                scale_station2_frame_brightness(&app.station2_base_pixels, app.clock_brightness);
            send_station2_custom_frame(app, st);
        }
    });

    // Строка 3: Процедурные RGB-фигуры и штатные фоновые шоу прошивки (panel/*.led)
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("Фигуры:")
                .small()
                .color(Color32::from_rgb(155, 170, 190)),
        );
        for (lbl, preset) in [
            ("Волна", "wave"),
            ("Лайк ♥", "heart"),
            ("Радуга", "rainbow"),
        ] {
            if ui.small_button(lbl).clicked() {
                apply_station2_preset(app, st, preset);
            }
        }
        ui.separator();
        ui.label(
            RichText::new("Шоу panel/*.led:")
                .small()
                .color(Color32::from_rgb(155, 170, 190)),
        );
        for (lbl, pat_id) in [
            ("Свеча", "panel/candle.led"),
            ("Лава-лампа", "panel/lavalamp.led"),
            ("Сияние", "panel/polar_lights.led"),
            ("Ночник", "panel/night_light.led"),
            ("Романтика", "panel/romantic.led"),
        ] {
            let active = app.selected_firmware_led_pattern == pat_id;
            let btn = egui::Button::new(RichText::new(lbl).small()).selected(active);
            if ui
                .add(btn)
                .on_hover_text(format!(
                    "Запустить штатную фоновую анимацию /system/vendor/quasar/ledpatterns/{pat_id}"
                ))
                .clicked()
            {
                if app.clock_brightness == 0 {
                    app.clock_brightness = 255;
                }
                app.station2_base_pixels = build_station2_show_preview_frame(pat_id);
                app.station2_custom_pixels = scale_station2_frame_brightness(
                    &app.station2_base_pixels,
                    app.clock_brightness,
                );
                let latest = app.latest_state.get_or_insert_with(|| st.clone());
                latest.ring_pixels = app.station2_custom_pixels.clone();
                latest.forced_led_pattern = pat_id.to_string();
                app.apply_firmware_led_pattern(pat_id);
            }
        }
    });
}
