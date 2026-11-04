use crate::gui::canvas::{draw_circular_oknix_logo, draw_ring_led};
use crate::gui::display_geom::{
    build_local_segments_from_text, clock_inspector_text, clock_segment_lines,
    clock_special_positions, hit_test_clock_segment, hit_test_matrix_pixel, matrix_inspector_text,
    render_text_to_matrix_21x12, segment_brightness,
};
use crate::gui::state::GuiAppState;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use protocol::{BridgeStateSnapshot, HostCommand};
use std::collections::BTreeMap;

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

    for (name, p0, p1) in clock_segment_lines(rect) {
        let br = segment_brightness(segments, &name);
        let is_hovered = hovered_seg == Some(name.as_str());
        if is_hovered {
            painter.line_segment(
                [p0, p1],
                Stroke::new(6.2, Color32::from_rgba_unmultiplied(80, 195, 255, 95)),
            );
        }
        let is_aux_bar = matches!(
            name.as_str(),
            "p4" | "p5" | "p6" | "p7" | "q4" | "q5" | "q6" | "q7" | "h2" | "l1" | "l2"
        );
        let stroke_w = if is_aux_bar { 3.0 } else { 3.6 };
        painter.line_segment([p0, p1], Stroke::new(stroke_w, seg_color(br, is_hovered)));
    }

    let (p3_pos, q3_pos, deg_pos) = clock_special_positions(rect);

    let p3 = segment_brightness(segments, "p3");
    let p3_hov = hovered_seg == Some("p3");
    if p3_hov {
        painter.circle_stroke(p3_pos, 5.0, Stroke::new(1.4, Color32::from_rgb(95, 195, 255)));
    }
    painter.circle_filled(p3_pos, 2.8, seg_color(p3, p3_hov));

    let q3 = segment_brightness(segments, "q3");
    let q3_hov = hovered_seg == Some("q3");
    if q3_hov {
        painter.circle_stroke(q3_pos, 5.0, Stroke::new(1.4, Color32::from_rgb(95, 195, 255)));
    }
    painter.circle_filled(q3_pos, 2.8, seg_color(q3, q3_hov));

    let deg_br = segment_brightness(segments, "deg");
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
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("7-сегментный дисплей (45 сегментов) (?)")
                .small()
                .color(Color32::from_rgb(165, 175, 190)),
        )
        .on_hover_text(
            "45-сегментный индикатор часов и символов (/sys/class/leds/<prefix>:white:clock-*):\n\
             • Разряды 1..4 (28 сегментов): a1..g1, a2..g2, a3..g3, a4..g4\n\
             • Надстрочные и подстрочные маркеры разрядов (8 сегментов): p4..p7 (верх), q4..q7 (низ)\n\
             • Центральное двоеточие (2 сегмента): p3 (верхняя точка), q3 (нижняя точка)\n\
             • Левый знаковый блок (6 сегментов): h1 (clockH0), h2 (clockH1), m1 (clockMINUS), m2, l1 (clockH2), l2 (clockH3)\n\
             • Символ градуса (1 сегмент): deg (clockDEGREE)",
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
        Vec2::new(ui.available_width().max(240.0), 76.0),
        Sense::click_and_drag(),
    );
    let hover_pos = cresp
        .hover_pos()
        .or_else(|| cresp.interact_pointer_pos());
    let hovered_seg = if cresp.hovered() || cresp.dragged() {
        hover_pos.and_then(|ptr| hit_test_clock_segment(crect, ptr))
    } else {
        None
    };
    if hovered_seg.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let cp = ui.painter_at(crect);
    draw_segment_clock(&cp, crect, &active_segs, hovered_seg.as_deref());

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
                let default_batt = if idx < 3 {
                    [215, 232, 255]
                } else {
                    [0, 0, 0]
                };
                let rgb = st.ring_pixels.get(idx).copied().unwrap_or(default_batt);
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

    let lmb_down = ui.input(|i| i.pointer.primary_down());
    let rmb_down = ui.input(|i| i.pointer.secondary_down());
    let lmb_clicked = mresp.clicked();
    let rmb_clicked = mresp.secondary_clicked();
    if (lmb_down || rmb_down || lmb_clicked || rmb_clicked)
        && (mresp.hovered() || mresp.dragged() || lmb_clicked || rmb_clicked)
    {
        if let Some(p) = ui
            .input(|i| i.pointer.interact_pos())
            .or_else(|| mresp.interact_pointer_pos())
        {
            if let Some(idx) = hit_test_matrix_pixel(mrect, cell, p) {
                let draw_mode = (lmb_down || lmb_clicked) && !(rmb_down || rmb_clicked);
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
