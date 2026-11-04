use super::{gnome_action_row, gnome_preferences_group, LED_PATTERNS};
use crate::gui::state::GuiAppState;
use eframe::egui::{self, Color32, RichText};
use protocol::HostCommand;

fn board_sensor_capabilities(board_slug: &str) -> (bool, bool) {
    let desc = protocol::find_board_descriptor(board_slug);
    let is_sn3236 = desc.map(|d| d.ring_driver == "i2c_sn3236").unwrap_or(false);
    let has_ultrasound = desc.map(|d| d.has_ultrasound).unwrap_or(false);
    (is_sn3236, has_ultrasound)
}

fn set_tof_distance(app: &mut GuiAppState, mm: u32) {
    app.tof_wave_reset_at = None;
    app.tof_slider = mm;
    app.tof_restore_distance = mm;
    app.send_command(HostCommand::SetSensor {
        doa_angle: None,
        mic_rms: None,
        als_lux: None,
        temp_c: None,
        tof_mm: Some(mm),
        swipe: None,
        swipe_direction: None,
    });
}

pub fn render_tof_and_sensors_card(ui: &mut egui::Ui, app: &mut GuiAppState) {
    app.ensure_board_led_patterns_loaded();
    let board_slug = app.active_board_slug();
    let (is_sn3236, has_ultrasound) = board_sensor_capabilities(&board_slug);
    let (card_title, card_subtitle) = if is_sn3236 {
        (
            "2. Лазерный дальномер ToF (VL53L0X) и датчики среды",
            "Оптический датчик жестов руки VL53L0X (Яндекс Станция Мини 1), освещённость ALS и температура SoC",
        )
    } else if has_ultrasound {
        (
            "2. Ультразвуковой тракт (38.4 кГц), освещённость ALS и датчики",
            "Дистанция акустического отражения для pcm.ultrasound (96 кГц / 38.4 кГц), датчик света light-sensor-top и температура SoC",
        )
    } else {
        (
            "2. Датчики среды (ALS, температура SoC) и световое кольцо",
            "Датчик внешней освещённости ltr308als01 (IIO), температура SoC и управление Световым кольцом (LED Ring)",
        )
    };

    gnome_preferences_group(ui, card_title, card_subtitle, |ui| {
        if is_sn3236 {
            gnome_action_row(
                ui,
                "Импульс датчика ToF (STM VL53L0X)",
                "Кратковременное приближение ладони (140 мм на 0.9 с с возвратом к прежней дистанции)",
                false,
                |ui| {
                    let wave_btn = egui::Button::new(
                        RichText::new("Взмах руки (Импульс 140 мм)")
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(53, 132, 228));
                    if ui.add(wave_btn).clicked() {
                        app.trigger_tof_wave();
                    }
                    if ui.button("Поднести ладонь (120 мм)").clicked() {
                        set_tof_distance(app, 120);
                    }
                    if ui.button("Убрать руку (8190 мм)").clicked() {
                        set_tof_distance(app, 8190);
                    }
                },
            );

            gnome_action_row(
                ui,
                "Точная дистанция ToF (VL53L0X)",
                "Ручная установка расстояния от ладони до крышки (40..8190 мм) для регулировки громкости жестом",
                false,
                |ui| {
                    if ui
                        .add(egui::Slider::new(&mut app.tof_slider, 40..=8190).suffix(" мм"))
                        .changed()
                    {
                        set_tof_distance(app, app.tof_slider);
                    }
                },
            );
        } else if has_ultrasound {
            gnome_action_row(
                ui,
                "Акустическое отражение ультразвука (pcm.ultrasound, 38.4 кГц)",
                "Дистанция до препятствия (40..8190 мм), определяющая амплитуду отражённого эха на усилителе ACM8625S (2-002c)",
                false,
                |ui| {
                    if ui
                        .add(egui::Slider::new(&mut app.tof_slider, 40..=8190).suffix(" мм"))
                        .changed()
                    {
                        set_tof_distance(app, app.tof_slider);
                    }
                    if ui.button("Ближнее эхо (180 мм)").clicked() {
                        set_tof_distance(app, 180);
                    }
                    if ui.button("Свободное поле (8190 мм)").clicked() {
                        set_tof_distance(app, 8190);
                    }
                },
            );
        }

        gnome_action_row(
            ui,
            "Громкость, датчики среды и паттерн LED",
            "Громкость (0..10), освещённость ALS (лк), температура SoC (C) и принудительный .led паттерн",
            true,
            |ui| {
                if ui
                    .add(egui::Slider::new(&mut app.volume_slider, 0..=10).text("Громк"))
                    .changed()
                {
                    app.send_command(HostCommand::SetVolume {
                        volume_level: app.volume_slider,
                    });
                }
                if ui
                    .add(egui::Slider::new(&mut app.als_slider, 0..=10000).text("лк"))
                    .changed()
                {
                    app.send_command(HostCommand::SetSensor {
                        doa_angle: None,
                        mic_rms: None,
                        als_lux: Some(app.als_slider),
                        temp_c: None,
                        tof_mm: None,
                        swipe: None,
                        swipe_direction: None,
                    });
                }
                if ui
                    .add(
                        egui::Slider::new(&mut app.temp_slider, 20.0..=85.0)
                            .text("C")
                            .fixed_decimals(1),
                    )
                    .changed()
                {
                    app.send_command(HostCommand::SetSensor {
                        doa_angle: None,
                        mic_rms: None,
                        als_lux: None,
                        temp_c: Some(app.temp_slider),
                        tof_mm: None,
                        swipe: None,
                        swipe_direction: None,
                    });
                }

                let preset_selected_text = if !app.selected_firmware_led_pattern.is_empty() {
                    "Пресет: — (выбран .led)".to_string()
                } else {
                    let lbl = LED_PATTERNS
                        .iter()
                        .find(|&&(val, _)| val == app.selected_led_pattern)
                        .map(|&(_, l)| l)
                        .unwrap_or("Авто (по состоянию Quasar)");
                    format!("Пресет: {}", lbl)
                };

                egui::ComboBox::from_id_salt("gnome_led_preset_combo")
                    .selected_text(preset_selected_text)
                    .width(210.0)
                    .show_ui(ui, |ui| {
                        for &(val, label) in LED_PATTERNS {
                            let is_sel = app.selected_firmware_led_pattern.is_empty()
                                && app.selected_led_pattern == val;
                            if ui.selectable_label(is_sel, label).clicked() {
                                app.apply_preset_led_pattern(val);
                            }
                        }
                    });

                let fw_selected_text = if app.selected_firmware_led_pattern.is_empty() {
                    if app.firmware_led_patterns.is_empty() {
                        "Из прошивки (.led): нет образа".to_string()
                    } else {
                        format!(
                            "Из прошивки (.led): {} файлов",
                            app.firmware_led_patterns.len()
                        )
                    }
                } else {
                    app.firmware_led_patterns
                        .iter()
                        .find(|p| p.id == app.selected_firmware_led_pattern)
                        .map(|p| p.label.clone())
                        .unwrap_or_else(|| app.selected_firmware_led_pattern.clone())
                };

                egui::ComboBox::from_id_salt("gnome_firmware_led_combo")
                    .selected_text(fw_selected_text)
                    .width(265.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                app.selected_firmware_led_pattern.is_empty(),
                                "— Не выбран (использовать Пресет) —",
                            )
                            .clicked()
                        {
                            app.apply_preset_led_pattern("");
                        }
                        if !app.firmware_led_patterns.is_empty() {
                            ui.separator();
                        }
                        for info in app.firmware_led_patterns.clone() {
                            let is_sel = app.selected_firmware_led_pattern == info.id;
                            let [r, g, b] = info.primary_rgb;
                            let swatch = RichText::new("●")
                                .color(Color32::from_rgb(r, g, b))
                                .strong();
                            ui.horizontal(|ui| {
                                ui.label(swatch);
                                if ui.selectable_label(is_sel, &info.label).clicked() {
                                    app.apply_firmware_led_pattern(&info.id);
                                }
                            });
                        }
                    });
            },
        );
    });
}
