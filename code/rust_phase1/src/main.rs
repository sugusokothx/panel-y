use eframe::egui;

mod parquet_schema;
mod parquet_waveform;

const FULL_RANGE_ENVELOPE_BUCKETS: usize = 4_096;
const MIN_VISIBLE_ENVELOPE_BUCKETS: usize = 128;
const MAX_VISIBLE_ENVELOPE_BUCKETS: usize = 8_192;
const WHEEL_ZOOM_SENSITIVITY: f64 = 0.0015;
const BENCH_VISIBLE_ENVELOPE_BUCKETS: usize = 1_200;
const BENCH_RANGE_RUNS: usize = 24;
const STRESS_RANGE_RUNS: usize = 1_000;
const STRESS_REPORT_BLOCKS: usize = 5;

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
            CliCommand::BenchChannel { path, channel } => benchmark_channel(path, &channel),
            CliCommand::StressChannel {
                path,
                channel,
                runs,
            } => stress_channel(path, &channel, runs),
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
    Schema {
        path: String,
    },
    LoadChannel {
        path: String,
        channel: String,
    },
    BenchChannel {
        path: String,
        channel: String,
    },
    StressChannel {
        path: String,
        channel: String,
        runs: usize,
    },
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
        Some("--bench-channel") => {
            let path = args.next()?;
            let channel = args.next()?;
            Some(CliCommand::BenchChannel { path, channel })
        }
        Some("--stress-channel") => {
            let path = args.next()?;
            let channel = args.next()?;
            let runs = args
                .next()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(STRESS_RANGE_RUNS);
            Some(CliCommand::StressChannel {
                path,
                channel,
                runs,
            })
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
    view_range: Option<(f64, f64)>,
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
            view_range: None,
        }
    }
}

fn default_parquet_path() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proto_3_1b/data/test_100k.parquet")
        .display()
        .to_string()
}

fn benchmark_channel(path: String, channel: &str) -> Result<String, String> {
    let data = parquet_waveform::read_selected_channel(&path, channel)?;
    let full_range = data
        .time_range()
        .ok_or_else(|| "loaded waveform has no time range".to_owned())?;
    let full_envelope = data.min_max_envelope(FULL_RANGE_ENVELOPE_BUCKETS);
    let ranges = benchmark_ranges(full_range, BENCH_RANGE_RUNS);
    let mut range_results = Vec::with_capacity(ranges.len());
    let started = std::time::Instant::now();

    for (index, range) in ranges.into_iter().enumerate() {
        let envelope = data.min_max_envelope_for_range(range, BENCH_VISIBLE_ENVELOPE_BUCKETS);
        range_results.push(BenchmarkRangeResult {
            index: index + 1,
            range,
            source_sample_count: envelope.source_sample_count,
            bucket_count: envelope.bucket_count(),
            bucket_size: envelope.bucket_size,
            draw_point_count: envelope.draw_point_count(),
            elapsed_sec: envelope.elapsed.as_secs_f64(),
        });
    }

    let range_total_sec = started.elapsed().as_secs_f64();
    let range_max_sec = range_results
        .iter()
        .map(|result| result.elapsed_sec)
        .fold(0.0, f64::max);
    let range_avg_sec = if range_results.is_empty() {
        0.0
    } else {
        range_results
            .iter()
            .map(|result| result.elapsed_sec)
            .sum::<f64>()
            / range_results.len() as f64
    };

    let mut report = String::new();
    report.push_str(&format!("file: {}\n", data.path.display()));
    report.push_str(&format!("channel: {}\n", data.channel_name));
    report.push_str(&format!("channel path: {}\n", data.channel_path));
    report.push_str(&format!("samples: {}\n", data.sample_count()));
    report.push_str(&format!(
        "time range: {}\n",
        format_range(data.time_range())
    ));
    report.push_str(&format!(
        "projected columns: {:?}\n",
        data.projected_column_indices
    ));
    report.push_str(&format!("read time: {:.3}s\n", data.elapsed.as_secs_f64()));
    report.push_str(&format!(
        "array memory: {:.1} MiB\n",
        bytes_to_mib(data.memory_bytes())
    ));
    report.push_str(&format!(
        "full envelope: {} buckets requested, {} buckets built, bucket size {}, draw points {}, {:.3}s\n",
        full_envelope.requested_bucket_count,
        full_envelope.bucket_count(),
        full_envelope.bucket_size,
        full_envelope.draw_point_count(),
        full_envelope.elapsed.as_secs_f64()
    ));
    report.push_str(&format!(
        "visible envelope benchmark: {} runs, {} buckets requested, total {:.3}s, avg {:.4}s, max {:.4}s\n",
        range_results.len(),
        BENCH_VISIBLE_ENVELOPE_BUCKETS,
        range_total_sec,
        range_avg_sec,
        range_max_sec
    ));
    report.push_str("runs:\n");
    for result in &range_results {
        report.push_str(&format!(
            "  #{:02} {:.6}..{:.6}s samples={} buckets={} bucket={} draw_points={} {:.4}s\n",
            result.index,
            result.range.0,
            result.range.1,
            result.source_sample_count,
            result.bucket_count,
            result.bucket_size,
            result.draw_point_count,
            result.elapsed_sec
        ));
    }

    Ok(report)
}

fn stress_channel(path: String, channel: &str, runs: usize) -> Result<String, String> {
    let runs = runs.max(1);
    let rss_before_load = process_rss_mib();
    let data = parquet_waveform::read_selected_channel(&path, channel)?;
    let rss_after_load = process_rss_mib();
    let full_range = data
        .time_range()
        .ok_or_else(|| "loaded waveform has no time range".to_owned())?;
    let full_envelope = data.min_max_envelope(FULL_RANGE_ENVELOPE_BUCKETS);
    let rss_after_full_envelope = process_rss_mib();
    let ranges = benchmark_ranges(full_range, BENCH_RANGE_RUNS);
    if ranges.is_empty() {
        return Err("no benchmark ranges could be built".to_owned());
    }

    let mut block_results = Vec::with_capacity(STRESS_REPORT_BLOCKS);
    let block_size = runs.div_ceil(STRESS_REPORT_BLOCKS);
    let mut total_elapsed_sec = 0.0;
    let mut max_elapsed_sec: f64 = 0.0;
    let mut draw_points_total = 0usize;
    let mut samples_total = 0usize;

    let started = std::time::Instant::now();
    for index in 0..runs {
        let range = ranges[index % ranges.len()];
        let envelope = data.min_max_envelope_for_range(range, BENCH_VISIBLE_ENVELOPE_BUCKETS);
        let elapsed_sec = envelope.elapsed.as_secs_f64();
        total_elapsed_sec += elapsed_sec;
        max_elapsed_sec = max_elapsed_sec.max(elapsed_sec);
        draw_points_total += envelope.draw_point_count();
        samples_total += envelope.source_sample_count;

        let completed = index + 1;
        if completed % block_size == 0 || completed == runs {
            block_results.push(StressBlockResult {
                completed_runs: completed,
                elapsed_wall_sec: started.elapsed().as_secs_f64(),
                rss_mib: process_rss_mib(),
            });
        }
    }

    let wall_sec = started.elapsed().as_secs_f64();
    let avg_elapsed_sec = total_elapsed_sec / runs as f64;
    let rss_after_stress = process_rss_mib();

    let mut report = String::new();
    report.push_str(&format!("file: {}\n", data.path.display()));
    report.push_str(&format!("channel: {}\n", data.channel_name));
    report.push_str(&format!("samples: {}\n", data.sample_count()));
    report.push_str(&format!(
        "time range: {}\n",
        format_range(data.time_range())
    ));
    report.push_str(&format!("read time: {:.3}s\n", data.elapsed.as_secs_f64()));
    report.push_str(&format!(
        "array memory: {:.1} MiB\n",
        bytes_to_mib(data.memory_bytes())
    ));
    report.push_str(&format!(
        "full envelope: {} buckets requested, {} buckets built, bucket size {}, draw points {}, {:.3}s\n",
        full_envelope.requested_bucket_count,
        full_envelope.bucket_count(),
        full_envelope.bucket_size,
        full_envelope.draw_point_count(),
        full_envelope.elapsed.as_secs_f64()
    ));
    report.push_str(&format!(
        "stress visible envelope: {} runs, {} buckets requested, wall {:.3}s, measured total {:.3}s, avg {:.4}s, max {:.4}s\n",
        runs,
        BENCH_VISIBLE_ENVELOPE_BUCKETS,
        wall_sec,
        total_elapsed_sec,
        avg_elapsed_sec,
        max_elapsed_sec
    ));
    report.push_str(&format!(
        "stress totals: samples={}, draw_points={}\n",
        samples_total, draw_points_total
    ));
    report.push_str(&format!(
        "rss: before_load={}, after_load={}, after_full_envelope={}, after_stress={}\n",
        format_optional_mib(rss_before_load),
        format_optional_mib(rss_after_load),
        format_optional_mib(rss_after_full_envelope),
        format_optional_mib(rss_after_stress)
    ));
    report.push_str("rss blocks:\n");
    for block in &block_results {
        report.push_str(&format!(
            "  runs={} wall={:.3}s rss={}\n",
            block.completed_runs,
            block.elapsed_wall_sec,
            format_optional_mib(block.rss_mib)
        ));
    }

    Ok(report)
}

#[derive(Debug)]
struct BenchmarkRangeResult {
    index: usize,
    range: (f64, f64),
    source_sample_count: usize,
    bucket_count: usize,
    bucket_size: usize,
    draw_point_count: usize,
    elapsed_sec: f64,
}

#[derive(Debug)]
struct StressBlockResult {
    completed_runs: usize,
    elapsed_wall_sec: f64,
    rss_mib: Option<f64>,
}

fn benchmark_ranges(full_range: (f64, f64), run_count: usize) -> Vec<(f64, f64)> {
    let Some((start, end)) = normalized_range(full_range) else {
        return Vec::new();
    };
    let span = end - start;
    if span <= 0.0 || run_count == 0 {
        return Vec::new();
    }

    let mut ranges = Vec::with_capacity(run_count);
    for index in 0..run_count {
        let phase = index as f64 / run_count.max(1) as f64;
        let window_ratio = match index % 4 {
            0 => 0.50,
            1 => 0.20,
            2 => 0.05,
            _ => 0.01,
        };
        let window = (span * window_ratio).max(span * 1.0e-9);
        let available = (span - window).max(0.0);
        let left = start + available * phase;
        ranges.push((left, left + window));
    }

    ranges
}

fn process_rss_mib() -> Option<f64> {
    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let rss_kib = text.trim().parse::<f64>().ok()?;
    Some(rss_kib / 1024.0)
}

fn format_optional_mib(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1} MiB"))
        .unwrap_or_else(|| "n/a".to_owned())
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
                self.view_range = None;
            }
            Err(error) => {
                self.status = format!("Schema load failed: {error}");
                self.schema = None;
                self.waveform = None;
                self.envelope = None;
                self.view_range = None;
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
                let view_range = waveform.time_range();
                let envelope = view_range.map_or_else(
                    || waveform.min_max_envelope(FULL_RANGE_ENVELOPE_BUCKETS),
                    |range| waveform.min_max_envelope_for_range(range, FULL_RANGE_ENVELOPE_BUCKETS),
                );
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
                self.view_range = view_range;
                self.waveform = Some(waveform);
            }
            Err(error) => {
                self.status = format!("Waveform load failed: {error}");
                self.waveform = None;
                self.envelope = None;
                self.view_range = None;
            }
        }
    }

    fn reset_view_range(&mut self) {
        self.view_range = self
            .waveform
            .as_ref()
            .and_then(parquet_waveform::WaveformData::time_range);
        self.envelope = None;
    }

    fn ensure_visible_envelope(&mut self, requested_bucket_count: usize) {
        let Some(waveform) = self.waveform.as_ref() else {
            return;
        };
        let Some(full_range) = waveform.time_range() else {
            self.envelope = None;
            self.view_range = None;
            return;
        };

        let min_span = min_view_span(full_range, waveform.sample_count());
        let view_range =
            clamp_view_range(self.view_range.unwrap_or(full_range), full_range, min_span);
        if self.view_range != Some(view_range) {
            self.view_range = Some(view_range);
        }

        let envelope_is_stale = match self.envelope.as_ref() {
            Some(envelope) => {
                envelope.time_range != Some(view_range)
                    || envelope.requested_bucket_count != requested_bucket_count
            }
            None => true,
        };

        if envelope_is_stale {
            let envelope = waveform.min_max_envelope_for_range(view_range, requested_bucket_count);
            self.status = format!(
                "View {:.6}..{:.6}s: {} visible samples, envelope {}/{} buckets, bucket {}, {:.3}s",
                view_range.0,
                view_range.1,
                envelope.source_sample_count,
                envelope.bucket_count(),
                envelope.requested_bucket_count,
                envelope.bucket_size,
                envelope.elapsed.as_secs_f64()
            );
            self.envelope = Some(envelope);
        }
    }

    fn handle_plot_interaction(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        plot_rect: egui::Rect,
    ) {
        let Some(waveform) = self.waveform.as_ref() else {
            return;
        };
        let Some(full_range) = waveform.time_range() else {
            return;
        };

        let min_span = min_view_span(full_range, waveform.sample_count());
        let mut next_range =
            clamp_view_range(self.view_range.unwrap_or(full_range), full_range, min_span);
        let current_range = next_range;
        let mut changed = false;

        if response.double_clicked() {
            next_range = full_range;
            changed = true;
        }

        if response.dragged_by(egui::PointerButton::Primary) && plot_rect.width() > 1.0 {
            let delta_x = f64::from(response.drag_delta().x);
            if delta_x != 0.0 {
                let span = next_range.1 - next_range.0;
                let shift = -delta_x / f64::from(plot_rect.width()) * span;
                next_range = pan_view_range(next_range, full_range, shift, min_span);
                changed = true;
            }
        }

        let (pointer_pos, zoom_delta, scroll_delta) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.zoom_delta(),
                input.smooth_scroll_delta(),
            )
        });

        if let Some(pointer_pos) = pointer_pos
            && plot_rect.contains(pointer_pos)
            && plot_rect.width() > 1.0
        {
            if scroll_delta.x != 0.0 {
                let span = next_range.1 - next_range.0;
                let shift = -f64::from(scroll_delta.x) / f64::from(plot_rect.width()) * span;
                next_range = pan_view_range(next_range, full_range, shift, min_span);
                changed = true;
            }

            let zoom_factor = if zoom_delta != 1.0 {
                1.0 / f64::from(zoom_delta)
            } else if scroll_delta.y != 0.0 {
                (-f64::from(scroll_delta.y) * WHEEL_ZOOM_SENSITIVITY).exp()
            } else {
                1.0
            };

            if zoom_factor.is_finite() && (zoom_factor - 1.0).abs() > f64::EPSILON {
                let anchor_ratio = ((pointer_pos.x - plot_rect.left()) / plot_rect.width()) as f64;
                next_range =
                    zoom_view_range(next_range, full_range, anchor_ratio, zoom_factor, min_span);
                changed = true;
            }
        }

        if changed && next_range != current_range {
            self.view_range = Some(next_range);
            self.envelope = None;
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
                    self.view_range = None;
                    self.status = format!("Selected channel: {}", self.selected_channel);
                }

                if self.waveform.is_some() {
                    ui.add_space(16.0);
                    ui.heading("View");
                    if ui.button("Reset X Range").clicked() {
                        self.reset_view_range();
                    }
                    if let Some((start, end)) = self.view_range {
                        ui.monospace(format!("x: {start:.6} .. {end:.6} s"));
                    }
                }
            });

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            let available = ui.available_size();
            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let plot_rect = plot_area_rect(rect);
            let requested_buckets = visible_envelope_bucket_count(plot_rect);

            self.handle_plot_interaction(ui, &response, plot_rect);
            self.ensure_visible_envelope(requested_buckets);

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

fn visible_envelope_bucket_count(rect: egui::Rect) -> usize {
    (rect.width().round() as usize)
        .clamp(MIN_VISIBLE_ENVELOPE_BUCKETS, MAX_VISIBLE_ENVELOPE_BUCKETS)
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

fn min_view_span(full_range: (f64, f64), sample_count: usize) -> f64 {
    let full_span = full_range.1 - full_range.0;
    if !full_span.is_finite() || full_span <= 0.0 {
        return f64::EPSILON;
    }

    let sample_span = if sample_count > 1 {
        full_span / (sample_count - 1) as f64
    } else {
        full_span
    };

    sample_span.max(full_span * 1.0e-9).max(f64::EPSILON)
}

fn clamp_view_range(range: (f64, f64), full_range: (f64, f64), min_span: f64) -> (f64, f64) {
    let Some((full_start, full_end)) = normalized_range(full_range) else {
        return full_range;
    };
    let full_span = full_end - full_start;
    if full_span <= 0.0 {
        return (full_start, full_end);
    }

    let (start, end) = normalized_range(range).unwrap_or((full_start, full_end));
    let span = (end - start).clamp(min_span.min(full_span), full_span);
    let center = ((start + end) * 0.5).clamp(full_start, full_end);
    range_with_span_around(center, span, (full_start, full_end))
}

fn pan_view_range(
    range: (f64, f64),
    full_range: (f64, f64),
    shift: f64,
    min_span: f64,
) -> (f64, f64) {
    let (start, end) = clamp_view_range(range, full_range, min_span);
    let span = end - start;
    range_with_start(start + shift, span, full_range)
}

fn zoom_view_range(
    range: (f64, f64),
    full_range: (f64, f64),
    anchor_ratio: f64,
    zoom_factor: f64,
    min_span: f64,
) -> (f64, f64) {
    let Some((full_start, full_end)) = normalized_range(full_range) else {
        return full_range;
    };
    let full_span = full_end - full_start;
    if full_span <= 0.0 {
        return (full_start, full_end);
    }

    let (start, end) = clamp_view_range(range, (full_start, full_end), min_span);
    let span = end - start;
    let anchor_ratio = anchor_ratio.clamp(0.0, 1.0);
    let anchor_time = start + span * anchor_ratio;
    let next_span = (span * zoom_factor).clamp(min_span.min(full_span), full_span);
    let next_start = anchor_time - next_span * anchor_ratio;

    range_with_start(next_start, next_span, (full_start, full_end))
}

fn range_with_span_around(center: f64, span: f64, full_range: (f64, f64)) -> (f64, f64) {
    range_with_start(center - span * 0.5, span, full_range)
}

fn range_with_start(start: f64, span: f64, full_range: (f64, f64)) -> (f64, f64) {
    let Some((full_start, full_end)) = normalized_range(full_range) else {
        return full_range;
    };
    let full_span = full_end - full_start;
    if span >= full_span {
        return (full_start, full_end);
    }

    let start = start.clamp(full_start, full_end - span);
    (start, start + span)
}

fn normalized_range((start, end): (f64, f64)) -> Option<(f64, f64)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }

    match start.partial_cmp(&end)? {
        std::cmp::Ordering::Less => Some((start, end)),
        std::cmp::Ordering::Greater => Some((end, start)),
        std::cmp::Ordering::Equal => None,
    }
}
