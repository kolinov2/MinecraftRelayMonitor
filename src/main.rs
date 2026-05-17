
use eframe::egui;
use serde_json::Value;
use std::process::Command;
use std::time::{Duration, Instant};

fn ping(ip: &str) -> u32 {
    let out = Command::new("ping")
        .args(["-n", "1", ip])
        .output()
        .unwrap();

    let text = String::from_utf8_lossy(&out.stdout);

    for line in text.lines() {
        if let Some(pos) = line.find("time=") {
            let sub = &line[pos + 5..];
            let num: String = sub.chars().take_while(|c| c.is_numeric()).collect();
            return num.parse().unwrap_or(999);
        }
    }

    999
}

fn geo(ip: &str) -> Option<Value> {
    let url = format!(
        "http://ip-api.com/json/{}?fields=as,org,country,regionName,city",
        ip
    );

    reqwest::blocking::get(url).ok()?.json().ok()
}

fn is_microsoft(v: &Value) -> bool {
    let org = v["org"].as_str().unwrap_or("");
    org.contains("Microsoft") || org.contains("Azure")
}

fn find_relay() -> Option<(String, Value)> {
    let out = Command::new("netstat")
        .args(["-ano"])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&out.stdout);

    for line in text.lines() {
        if !line.contains("ESTABLISHED") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let remote = parts[2];
        let ip = remote.split(':').next()?;

        if ip == "127.0.0.1" {
            continue;
        }

        if remote.contains("443") {
            if let Some(info) = geo(ip) {
                if is_microsoft(&info) {
                    return Some((ip.to_string(), info));
                }
            }
        }
    }

    None
}

fn color(ms: u32) -> egui::Color32 {
    if ms < 60 {
        egui::Color32::GREEN
    } else if ms < 120 {
        egui::Color32::YELLOW
    } else {
        egui::Color32::RED
    }
}

struct App {
    ip: Option<String>,
    geo: Option<Value>,
    ping: u32,
    history: Vec<u32>,
    last_update: Instant,
    start_time: Instant,
}

impl Default for App {
    fn default() -> Self {
        Self {
            ip: None,
            geo: None,
            ping: 0,
            history: vec![],
            last_update: Instant::now(),
            start_time: Instant::now(),
        }
    }
}

impl App {
    fn update_ping(&mut self) {
        if let Some(ip) = &self.ip {
            let ms = ping(ip);
            self.ping = ms;

            self.history.push(ms);
            if self.history.len() > 60 {
                self.history.remove(0);
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.ip.is_none() {
            if let Some((ip, geo)) = find_relay() {
                self.ip = Some(ip);
                self.geo = Some(geo);
            }
        }

        if self.last_update.elapsed() > Duration::from_secs(1) {
            self.update_ping();
            self.last_update = Instant::now();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Minecraft Relay Monitor");
            ui.separator();

            if let Some(ip) = &self.ip {
                ui.label(format!("IP: {}", ip));
                ui.colored_label(color(self.ping), format!("Ping: {} ms", self.ping));
            } else if self.start_time.elapsed().as_secs() > 60 {
                ui.colored_label(egui::Color32::LIGHT_RED, "Error: The program cannot find the relay address.");
            } else {
                ui.label("Searching relay...");
            }

            ui.separator();

            if let Some(g) = &self.geo {
                ui.label("ISP: Microsoft Azure Cloud");
                ui.label(format!("City: {}", g["city"]));
                ui.label(format!("Country: {}", g["country"]));
            } else if self.start_time.elapsed().as_secs() > 60 && self.ip.is_none() {
                ui.colored_label(egui::Color32::GRAY, "No ISP data available.");
            } else {
                ui.label("ISP: Waiting for connection...");
            }

            ui.separator();
            ui.add_space(5.0);

            // graph
            let mut min_ping_val = 45.0;
            let mut max_ping_val = 75.0;

            if !self.history.is_empty() {
                let &real_min = self.history.iter().min().unwrap_or(&50);
                let &real_max = self.history.iter().max().unwrap_or(&70);

                min_ping_val = (real_min as f64 - 5.0).max(0.0);
                max_ping_val = real_max as f64 + 5.0;

                if max_ping_val <= min_ping_val {
                    max_ping_val = min_ping_val + 10.0;
                }
            }

            let desired_height = 150.0;

            ui.horizontal(|ui| {
                let (left_rect, _) = ui.allocate_exact_size(egui::vec2(65.0, desired_height), egui::Sense::hover());
                let left_painter = ui.painter_at(left_rect);

                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width() - 5.0, desired_height),
                    egui::Sense::hover()
                );
                let painter = ui.painter_at(rect);

                let map_y = |ping_val: f64| -> f32 {
                    let pct = (ping_val - min_ping_val) / (max_ping_val - min_ping_val);
                    rect.bottom() - (pct as f32 * rect.height())
                };


                let galley = left_painter.layout_no_wrap(
                    "Ping (ms)".to_string(),
                    egui::FontId::proportional(11.0),
                    ui.visuals().text_color(),
                );

                let text_pos_x = left_rect.left() + 12.0 - galley.size().y / 2.0;
                let text_pos_y = left_rect.center().y + galley.size().x / 2.0;

                let mut text_shape = egui::epaint::TextShape::new(
                    egui::pos2(text_pos_x, text_pos_y),
                    galley,
                    ui.visuals().text_color(),
                );
                text_shape.angle = -std::f32::consts::FRAC_PI_2;
                left_painter.add(text_shape);

                let grid_steps = [
                    min_ping_val + 5.0,
                    (max_ping_val + min_ping_val) / 2.0,
                    max_ping_val - 5.0
                ];

                for grid_val in grid_steps {
                    let y_pos = map_y(grid_val);

                    let label_text = format!("{:.0} —", grid_val);
                    left_painter.text(
                        egui::pos2(left_rect.right() - 2.0, y_pos),
                        egui::Align2::RIGHT_CENTER,
                        label_text,
                        egui::FontId::proportional(10.0),
                        ui.visuals().text_color(),
                    );
                }

                painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(60));
                let stroke_grid = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(15));
                painter.rect_stroke(rect, 0.0, stroke_grid);

                for grid_val in grid_steps {
                    let y_pos = map_y(grid_val);
                    painter.line_segment(
                        [egui::pos2(rect.left(), y_pos), egui::pos2(rect.right(), y_pos)],
                        stroke_grid,
                    );
                }

                if self.history.len() > 1 {
                    let max_points = 60.0;
                    let step_x = rect.width() / (max_points - 1.0);

                    let mut line_points = Vec::new();
                    for (i, &ping_val) in self.history.iter().enumerate() {
                        let x = rect.left() + (i as f32 * step_x);
                        let y = map_y(ping_val as f64);
                        line_points.push(egui::pos2(x, y));
                    }

                    let stroke_line = egui::Stroke::new(1.5, egui::Color32::from_rgb(235, 78, 78));
                    for pair in line_points.windows(2) {
                        painter.line_segment([pair[0], pair[1]], stroke_line);
                    }
                }
            });
        });

        ctx.request_repaint();
    }
}

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([620.0, 330.0])
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "Relay Monitor",
        options,
        Box::new(|_| Box::new(App::default())),
    );
}