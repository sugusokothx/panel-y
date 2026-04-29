use eframe::egui;

mod parquet_schema;

fn main() -> eframe::Result {
    if let Some(path) = schema_report_arg() {
        match parquet_schema::read_schema_summary(path) {
            Ok(summary) => {
                println!("{}", summary.to_report());
            }
            Err(error) => {
                eprintln!("schema read failed: {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }

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

fn schema_report_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--schema") => args.next(),
        _ => None,
    }
}

#[derive(Debug)]
struct PanelYApp {
    parquet_path: String,
    selected_channel: String,
    status: String,
    schema: Option<parquet_schema::SchemaSummary>,
}

impl Default for PanelYApp {
    fn default() -> Self {
        Self {
            parquet_path: default_parquet_path(),
            selected_channel: String::new(),
            status: "Ready".to_owned(),
            schema: None,
        }
    }
}

fn default_parquet_path() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proto_3_1b/data/test_100k.parquet")
        .display()
        .to_string()
}

impl PanelYApp {
    fn load_schema(&mut self) {
        match parquet_schema::read_schema_summary(&self.parquet_path) {
            Ok(summary) => {
                if !summary
                    .channels
                    .iter()
                    .any(|channel| channel.path == self.selected_channel)
                {
                    self.selected_channel = summary
                        .channels
                        .first()
                        .map(|channel| channel.path.clone())
                        .unwrap_or_default();
                }

                let time_status = if summary.time_column.is_some() {
                    "time column detected"
                } else {
                    "time column missing"
                };
                self.status = format!(
                    "Loaded schema: {} rows, {} channels, {time_status}",
                    summary.row_count,
                    summary.channels.len()
                );
                self.schema = Some(summary);
            }
            Err(error) => {
                self.status = format!("Schema load failed: {error}");
                self.schema = None;
            }
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
                if ui.button("Load Schema").clicked() {
                    self.load_schema();
                }

                ui.add_space(16.0);
                draw_schema_controls(ui, &mut self.selected_channel, self.schema.as_ref());
            });

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            let available = ui.available_size();
            let (rect, _response) = ui.allocate_exact_size(available, egui::Sense::drag());
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            draw_plot_frame(&painter, rect, ui.visuals());
            draw_plot_placeholder(&painter, rect, self.schema.as_ref(), &self.selected_channel);
        });
    }
}

fn draw_schema_controls(
    ui: &mut egui::Ui,
    selected_channel: &mut String,
    schema: Option<&parquet_schema::SchemaSummary>,
) {
    ui.heading("Schema");

    let Some(schema) = schema else {
        ui.label("No schema loaded");
        return;
    };

    ui.label(format!("Rows: {}", schema.row_count));
    ui.label(format!("Row groups: {}", schema.row_group_count));
    ui.label(format!("Columns: {}", schema.column_count));

    match &schema.time_column {
        Some(time_column) => {
            ui.label(format!("Time: {}", time_column.display_name()));
        }
        None => {
            ui.colored_label(ui.visuals().warn_fg_color, "Time: not found");
        }
    }

    ui.add_space(12.0);
    ui.heading("Channel");
    if schema.channels.is_empty() {
        ui.label("No numeric channels found");
    } else {
        let selected_text = schema
            .channels
            .iter()
            .find(|channel| channel.path == *selected_channel)
            .map(|channel| channel.display_name())
            .unwrap_or_else(|| selected_channel.as_str());

        egui::ComboBox::from_id_salt("selected_channel")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for channel in &schema.channels {
                    ui.selectable_value(
                        selected_channel,
                        channel.path.clone(),
                        channel.display_name(),
                    );
                }
            });
    }

    ui.add_space(12.0);
    ui.heading("Columns");
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .show(ui, |ui| {
            for column in &schema.columns {
                let role = column.role.as_str();
                let logical = column.logical_type.as_deref().unwrap_or("-");
                let numeric = if column.is_numeric { "num" } else { "-" };
                ui.monospace(format!(
                    "#{:02} {:<7} {:<3} {:<8} {} ({}, {})",
                    column.index,
                    role,
                    numeric,
                    column.physical_type,
                    column.display_name(),
                    logical,
                    column.converted_type
                ));
            }
        });
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

fn draw_plot_placeholder(
    painter: &egui::Painter,
    rect: egui::Rect,
    schema: Option<&parquet_schema::SchemaSummary>,
    selected_channel: &str,
) {
    let label = match schema {
        Some(schema) if schema.time_column.is_some() && !selected_channel.is_empty() => {
            format!("Schema loaded. Next step: read time + {selected_channel}.")
        }
        Some(_) => "Schema loaded. Selectable waveform data is not ready yet.".to_owned(),
        None => "Load a Parquet schema to detect time and channel columns.".to_owned(),
    };

    painter.text(
        rect.left_top() + egui::vec2(16.0, 16.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::monospace(14.0),
        egui::Color32::GRAY,
    );
}
