use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Panel_y Rust Phase 1")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Panel_y Rust Phase 1",
        options,
        Box::new(|_cc| Ok(Box::new(PanelYApp::default()))),
    )
}

#[derive(Debug)]
struct PanelYApp {
    parquet_path: String,
    selected_channel: String,
    status: String,
}

impl Default for PanelYApp {
    fn default() -> Self {
        Self {
            parquet_path: "../proto_3_1b/data/test_100k.parquet".to_owned(),
            selected_channel: "sine_50Hz".to_owned(),
            status: "Ready".to_owned(),
        }
    }
}

impl eframe::App for PanelYApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Panel_y Rust Phase 1");
                ui.separator();
                ui.label(&self.status);
            });
        });

        egui::Panel::left("controls")
            .resizable(false)
            .default_size(320.0)
            .show_inside(ui, |ui| {
                ui.heading("Dataset");
                ui.label("Parquet path");
                ui.text_edit_singleline(&mut self.parquet_path);

                ui.add_space(12.0);
                ui.heading("Channel");
                ui.text_edit_singleline(&mut self.selected_channel);

                ui.add_space(12.0);
                if ui.button("Load").clicked() {
                    self.status = "Parquet loader is not implemented yet".to_owned();
                }
            });

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            let available = ui.available_size();
            let (rect, _response) = ui.allocate_exact_size(available, egui::Sense::drag());
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            draw_plot_frame(&painter, rect, ui.visuals());
        });
    }
}

fn draw_plot_frame(painter: &egui::Painter, rect: egui::Rect, visuals: &egui::Visuals) {
    let stroke = egui::Stroke::new(1.0, visuals.widgets.noninteractive.fg_stroke.color);
    let grid_stroke = egui::Stroke::new(1.0, visuals.faint_bg_color);

    for i in 1..10 {
        let t = i as f32 / 10.0;
        let x = egui::lerp(rect.left()..=rect.right(), t);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            grid_stroke,
        );

        let y = egui::lerp(rect.top()..=rect.bottom(), t);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            grid_stroke,
        );
    }

    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
}
