use eframe::egui;

mod parquet_schema;
mod parquet_waveform;

const FULL_RANGE_ENVELOPE_BUCKETS: usize = 4_096;

fn main() -> eframe::Result {
    if let Some(command) = cli_command_arg() {
        let result = match command {
            CliCommand::Schema { path } => parquet_schema::read_schema_summary(path)
                .map(|summary| summary.to_report()),
            CliCommand::LoadChannel { path, channel } => {
                parquet_waveform::read_selected_channel(path, &channel).map(|data| {
                    let envelope = data.min_max_envelope(FULL_RANGE_ENVELOPE_BUCKETS);
                    let time_range = format_range(envelope.time_range);
                    let value_range = format_range(envelope.value_range);
                    format!(
                        "file: {}\nchannel: {}\nchannel path: {}\nsamples: {}\ntime range: {time_range}\nvalue range: {value_range}\nprojected columns: {:?}\nread time: {:.3}s\nenvelope: {} buckets requested, {} buckets built, bucket size {}, draw points {}, {:.3}s\nmemory: {:.1} MiB",
                        data.path.display(),
                        data.channel_name,
                        data.channel_path,
                        data.sample_count(),
                        data.projected_column_indices,
                        data.elapsed.as_secs_f64(),
                        envelope.requested_bucket_count,
                        envelope.bucket_count(),
                        envelope.bucket_size,
                        envelope.draw_point_count(),
                        envelope.elapsed.as_secs_f64(),
                        bytes_to_mib(data.memory_bytes()),
                    )
                })
            }
        };

        match result {
            Ok(report) => println!("{report}"),
            Err(error) => {
                eprintln!("read failed: {error}");
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

enum CliCommand {
    Schema { path: String },
    LoadChannel { path: String, channel: String },
}

fn cli_command_arg() -> Option<CliCommand> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--schema") => args.next().map(|path| CliCommand::Schema { path }),
        Some("--load-channel") => {
            let path = args.next()?;
            let channel = args.next()?;
            Some(CliCommand::LoadChannel { path, channel })
        }
        _ => None,
    }
}

#[derive(Debug)]
struct PanelYApp {
    parquet_path: String,
    selected_channel: String,
    status: String,
    schema: Option<parquet_schema::SchemaSummary>,
    waveform: Option<parquet_waveform::WaveformData>,
    envelope: Option<parquet_waveform::MinMaxEnvelope>,
}

impl Default for PanelYApp {
    fn default() -> Self {
        Self {
            parquet_path: default_parquet_path(),
            selected_channel: String::new(),
            status: "Ready".to_owned(),
            schema: None,
            waveform: None,
            envelope: None,
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
                self.waveform = None;
                self.envelope = None;
            }
            Err(error) => {
                self.status = format!("Schema load failed: {error}");
                self.schema = None;
                self.waveform = None;
                self.envelope = None;
            }
        }
    }

    fn load_selected_channel(&mut self) {
        if self.selected_channel.is_empty() {
            self.status = "Select a channel before loading waveform data".to_owned();
            return;
        }

        match parquet_waveform::read_selected_channel(&self.parquet_path, &self.selected_channel) {
            Ok(waveform) => {
                let envelope = waveform.min_max_envelope(FULL_RANGE_ENVELOPE_BUCKETS);
                self.status = format!(
                    "Loaded waveform: {} samples, {}, {:.1} MiB, read {:.3}s, envelope {}/{} buckets {:.3}s",
                    waveform.sample_count(),
                    waveform.channel_name,
                    bytes_to_mib(waveform.memory_bytes()),
                    waveform.elapsed.as_secs_f64(),
                    envelope.bucket_count(),
                    envelope.requested_bucket_count,
                    envelope.elapsed.as_secs_f64()
                );
                self.envelope = Some(envelope);
                self.waveform = Some(waveform);
            }
            Err(error) => {
                self.status = format!("Waveform load failed: {error}");
                self.waveform = None;
                self.envelope = None;
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

                let can_load_waveform = self.schema.as_ref().is_some_and(|schema| {
                    schema.time_column.is_some() && !schema.channels.is_empty()
                }) && !self.selected_channel.is_empty();
                if ui
                    .add_enabled(
                        can_load_waveform,
                        egui::Button::new("Load Selected Channel"),
                    )
                    .clicked()
                {
                    self.load_selected_channel();
                }

                ui.add_space(16.0);
                if draw_schema_controls(ui, &mut self.selected_channel, self.schema.as_ref()) {
                    self.waveform = None;
                    self.envelope = None;
                    self.status = format!("Selected channel: {}", self.selected_channel);
                }
            });

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            let available = ui.available_size();
            let (rect, _response) = ui.allocate_exact_size(available, egui::Sense::drag());
            let painter = ui.painter_at(rect);
            let plot_rect = plot_area_rect(rect);

            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            draw_plot_frame(&painter, plot_rect, ui.visuals());

            if let (Some(waveform), Some(envelope)) =
                (self.waveform.as_ref(), self.envelope.as_ref())
            {
                draw_waveform_envelope(&painter, plot_rect, ui.visuals(), waveform, envelope);
            } else {
                draw_plot_placeholder(
                    &painter,
                    plot_rect,
                    self.schema.as_ref(),
                    &self.selected_channel,
                );
            }
        });
    }
}

fn draw_schema_controls(
    ui: &mut egui::Ui,
    selected_channel: &mut String,
    schema: Option<&parquet_schema::SchemaSummary>,
) -> bool {
    ui.heading("Schema");

    let Some(schema) = schema else {
        ui.label("No schema loaded");
        return false;
    };

    let mut selection_changed = false;

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
                    if ui
                        .selectable_value(
                            selected_channel,
                            channel.path.clone(),
                            channel.display_name(),
                        )
                        .changed()
                    {
                        selection_changed = true;
                    }
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

    selection_changed
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

fn plot_area_rect(rect: egui::Rect) -> egui::Rect {
    let left = if rect.width() > 180.0 { 58.0 } else { 12.0 };
    let bottom = if rect.height() > 120.0 { 36.0 } else { 12.0 };
    let top = 20.0;
    let right = 18.0;

    egui::Rect::from_min_max(
        egui::pos2(rect.left() + left, rect.top() + top),
        egui::pos2(rect.right() - right, rect.bottom() - bottom),
    )
}

fn draw_waveform_envelope(
    painter: &egui::Painter,
    rect: egui::Rect,
    visuals: &egui::Visuals,
    waveform: &parquet_waveform::WaveformData,
    envelope: &parquet_waveform::MinMaxEnvelope,
) {
    let Some((time_min, time_max)) = envelope.time_range else {
        draw_status_label(painter, rect, "No time range available");
        return;
    };
    let Some((value_min, value_max)) = envelope.value_range else {
        draw_status_label(painter, rect, "No finite values available");
        return;
    };

    let time_span = time_max - time_min;
    if !time_span.is_finite() || time_span <= 0.0 {
        draw_status_label(painter, rect, "Invalid time range");
        return;
    }

    let (value_min, value_max) = padded_range(value_min, value_max);
    let value_span = value_max - value_min;
    if !value_span.is_finite() || value_span <= 0.0 {
        draw_status_label(painter, rect, "Invalid value range");
        return;
    }

    let color = if visuals.dark_mode {
        egui::Color32::from_rgb(80, 190, 255)
    } else {
        egui::Color32::from_rgb(0, 94, 155)
    };
    let vertical_stroke = egui::Stroke::new(1.0, color.linear_multiply(0.45));
    let line_stroke = egui::Stroke::new(1.25, color);

    let to_screen = |time: f64, value: f64| -> egui::Pos2 {
        let x_t = ((time - time_min) / time_span) as f32;
        let y_t = ((value - value_min) / value_span) as f32;
        egui::pos2(
            egui::lerp(rect.left()..=rect.right(), x_t),
            egui::lerp(rect.bottom()..=rect.top(), y_t),
        )
    };

    let mut upper = Vec::with_capacity(envelope.buckets.len());
    let mut lower = Vec::with_capacity(envelope.buckets.len());
    for bucket in &envelope.buckets {
        let min_point = to_screen(bucket.time, f64::from(bucket.min));
        let max_point = to_screen(bucket.time, f64::from(bucket.max));
        painter.line_segment([min_point, max_point], vertical_stroke);
        lower.push(min_point);
        upper.push(max_point);
    }

    if upper.len() >= 2 {
        painter.line(upper, line_stroke);
        painter.line(lower, line_stroke);
    }

    draw_axis_labels(
        painter,
        rect,
        waveform,
        envelope,
        (time_min, time_max),
        (value_min, value_max),
        visuals,
    );
}

fn draw_axis_labels(
    painter: &egui::Painter,
    rect: egui::Rect,
    waveform: &parquet_waveform::WaveformData,
    envelope: &parquet_waveform::MinMaxEnvelope,
    time_range: (f64, f64),
    value_range: (f64, f64),
    visuals: &egui::Visuals,
) {
    let text_color = visuals.text_color();
    let weak_color = visuals.weak_text_color();
    let font = egui::FontId::monospace(12.0);

    painter.text(
        rect.left_top() + egui::vec2(0.0, -16.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{}  samples={}  envelope={}/{} buckets  bucket={}  draw_points={}",
            waveform.channel_path,
            envelope.source_sample_count,
            envelope.bucket_count(),
            envelope.requested_bucket_count,
            envelope.bucket_size,
            envelope.draw_point_count()
        ),
        font.clone(),
        text_color,
    );
    painter.text(
        rect.left_bottom() + egui::vec2(0.0, 18.0),
        egui::Align2::LEFT_TOP,
        format!("{:.6}", time_range.0),
        font.clone(),
        weak_color,
    );
    painter.text(
        rect.right_bottom() + egui::vec2(0.0, 18.0),
        egui::Align2::RIGHT_TOP,
        format!("{:.6} s", time_range.1),
        font.clone(),
        weak_color,
    );
    painter.text(
        rect.left_top() + egui::vec2(-8.0, 0.0),
        egui::Align2::RIGHT_TOP,
        format!("{:.3}", value_range.1),
        font.clone(),
        weak_color,
    );
    painter.text(
        rect.left_bottom() + egui::vec2(-8.0, 0.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{:.3}", value_range.0),
        font,
        weak_color,
    );
}

fn draw_plot_placeholder(
    painter: &egui::Painter,
    rect: egui::Rect,
    schema: Option<&parquet_schema::SchemaSummary>,
    selected_channel: &str,
) {
    let label = match schema {
        Some(schema) if schema.time_column.is_some() && !selected_channel.is_empty() => {
            format!("Schema loaded. Ready to read time + {selected_channel}.")
        }
        Some(_) => "Schema loaded. Selectable waveform data is not ready yet.".to_owned(),
        None => "Load a Parquet schema to detect time and channel columns.".to_owned(),
    };

    draw_status_label(painter, rect, &label);
}

fn draw_status_label(painter: &egui::Painter, rect: egui::Rect, label: &str) {
    painter.text(
        rect.left_top() + egui::vec2(16.0, 16.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::monospace(14.0),
        egui::Color32::GRAY,
    );
}

fn format_range(range: Option<(f64, f64)>) -> String {
    match range {
        Some((start, end)) => format!("{start:.6} .. {end:.6}"),
        None => "-".to_owned(),
    }
}

fn bytes_to_mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn padded_range(min: f64, max: f64) -> (f64, f64) {
    if min == max {
        let pad = min.abs().max(1.0) * 0.05;
        return (min - pad, max + pad);
    }

    let pad = (max - min).abs() * 0.05;
    (min - pad, max + pad)
}
