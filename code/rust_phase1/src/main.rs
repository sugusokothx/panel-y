use eframe::egui;
use std::collections::BTreeMap;

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
const MIN_WAVEFORM_ROW_HEIGHT: f32 = 180.0;
const WAVEFORM_ROW_GAP: f32 = 10.0;
const MAX_EXACT_STEP_SAMPLES: usize = 12_000;
const MAX_STEP_CHANGE_POINTS: usize = 12_000;
const DEFAULT_TRACE_LINE_WIDTH: f32 = 1.25;
const MIN_TRACE_LINE_WIDTH: f32 = 0.5;
const MAX_TRACE_LINE_WIDTH: f32 = 6.0;

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
            CliCommand::BenchMultiChannel { path, channels } => {
                benchmark_multi_channel(path, channels)
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
            .with_title("Panel_y Rust Phase 2")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Panel_y Rust Phase 2",
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
    BenchMultiChannel {
        path: String,
        channels: Vec<String>,
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
        Some("--bench-multi-channel") => {
            let path = args.next()?;
            let channels = args.collect();
            Some(CliCommand::BenchMultiChannel { path, channels })
        }
        _ => None,
    }
}

#[derive(Debug)]
struct PanelYApp {
    dataset: DatasetState,
    view: ViewState,
    load: LoadState,
}

#[derive(Debug)]
struct DatasetState {
    parquet_path: String,
    schema: Option<parquet_schema::SchemaSummary>,
    shared_time: Option<parquet_waveform::TimeData>,
    loaded_channels: ChannelStore,
}

#[derive(Debug)]
struct ViewState {
    selected_channel: String,
    selected_row_id: Option<u64>,
    next_row_id: u64,
    x_range: Option<(f64, f64)>,
    rows: Vec<PlotRow>,
}

#[derive(Debug)]
struct LoadState {
    status: String,
    pending_jobs: usize,
    progress: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct PlotRow {
    id: u64,
    channels: Vec<RowChannel>,
}

#[derive(Clone, Debug, PartialEq)]
struct RowChannel {
    channel_path: String,
    color_index: usize,
    draw_mode: DrawMode,
    visible: bool,
    color_override: Option<egui::Color32>,
    line_width: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DrawMode {
    Line,
    Step,
}

#[derive(Debug, Default)]
struct ChannelStore {
    raw_by_channel: BTreeMap<String, parquet_waveform::ChannelData>,
    envelope_cache: BTreeMap<EnvelopeKey, parquet_waveform::MinMaxEnvelope>,
    envelope_context: Option<EnvelopeContext>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EnvelopeContext {
    range_start_bits: u64,
    range_end_bits: u64,
    requested_bucket_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct EnvelopeKey {
    channel_path: String,
    range_start_bits: u64,
    range_end_bits: u64,
    requested_bucket_count: usize,
}

#[derive(Clone, Debug)]
struct VisibleTrace {
    channel_name: String,
    channel_path: String,
    sample_count: usize,
    color: egui::Color32,
    line_width: f32,
    draw_mode: DrawMode,
    data: VisibleTraceData,
}

#[derive(Clone, Debug)]
enum VisibleTraceData {
    Envelope(parquet_waveform::MinMaxEnvelope),
    RawStep(RawStepTrace),
}

#[derive(Clone, Debug)]
struct RawStepTrace {
    samples: Vec<StepSample>,
    source_sample_count: usize,
    time_range: Option<(f64, f64)>,
    value_range: Option<(f64, f64)>,
    kind: StepTraceKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepTraceKind {
    RawSamples,
    ChangePoints,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StepSample {
    time: f64,
    value: f32,
}

#[derive(Debug)]
struct VisibleRowTrace {
    row_id: u64,
    row_index: usize,
    row_channel_count: usize,
    traces: Vec<VisibleTrace>,
}

impl Default for PanelYApp {
    fn default() -> Self {
        Self {
            dataset: DatasetState::default(),
            view: ViewState::default(),
            load: LoadState::default(),
        }
    }
}

impl Default for DatasetState {
    fn default() -> Self {
        Self {
            parquet_path: default_parquet_path(),
            schema: None,
            shared_time: None,
            loaded_channels: ChannelStore::default(),
        }
    }
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            selected_channel: String::new(),
            selected_row_id: Some(0),
            next_row_id: 1,
            x_range: None,
            rows: vec![PlotRow {
                id: 0,
                channels: Vec::new(),
            }],
        }
    }
}

impl Default for LoadState {
    fn default() -> Self {
        Self {
            status: "Ready".to_owned(),
            pending_jobs: 0,
            progress: None,
            error: None,
        }
    }
}

impl DrawMode {
    const ALL: [Self; 2] = [Self::Line, Self::Step];

    fn as_str(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Step => "Step",
        }
    }
}

impl RowChannel {
    fn new(channel_path: &str, color_index: usize) -> Self {
        Self {
            channel_path: channel_path.to_owned(),
            color_index,
            draw_mode: DrawMode::Line,
            visible: true,
            color_override: None,
            line_width: DEFAULT_TRACE_LINE_WIDTH,
        }
    }

    fn color(&self, dark_mode: bool) -> egui::Color32 {
        self.color_override
            .unwrap_or_else(|| channel_color(self.color_index, dark_mode))
    }
}

fn default_parquet_path() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proto_3_1b/data/test_100k.parquet")
        .display()
        .to_string()
}

impl ViewState {
    fn reset_for_schema(&mut self, schema: &parquet_schema::SchemaSummary) {
        if !schema
            .channels
            .iter()
            .any(|channel| channel.path == self.selected_channel)
        {
            self.selected_channel = schema
                .channels
                .first()
                .map(|channel| channel.path.clone())
                .unwrap_or_default();
        }

        self.x_range = None;
        self.selected_row_id = Some(0);
        self.next_row_id = 1;
        self.rows = vec![PlotRow {
            id: 0,
            channels: Vec::new(),
        }];
    }

    fn reset_empty(&mut self) {
        self.selected_channel.clear();
        self.x_range = None;
        self.selected_row_id = Some(0);
        self.next_row_id = 1;
        self.rows = vec![PlotRow {
            id: 0,
            channels: Vec::new(),
        }];
    }

    fn ensure_row_state(&mut self) {
        if self.rows.is_empty() {
            self.rows.push(PlotRow {
                id: 0,
                channels: Vec::new(),
            });
        }

        let next_row_id = self.rows.iter().map(|row| row.id).max().unwrap_or(0) + 1;
        self.next_row_id = self.next_row_id.max(next_row_id);

        let selected_row_exists = self
            .selected_row_id
            .is_some_and(|selected_id| self.rows.iter().any(|row| row.id == selected_id));
        if !selected_row_exists {
            self.selected_row_id = self.rows.first().map(|row| row.id);
        }
    }

    fn add_row(&mut self) -> u64 {
        self.ensure_row_state();
        let id = self.next_row_id;
        self.next_row_id += 1;
        self.rows.push(PlotRow {
            id,
            channels: Vec::new(),
        });
        self.selected_row_id = Some(id);
        id
    }

    fn remove_row(&mut self, row_id: u64) -> bool {
        self.ensure_row_state();
        if self.rows.len() <= 1 {
            return false;
        }

        let Some(remove_index) = self.rows.iter().position(|row| row.id == row_id) else {
            return false;
        };

        self.rows.remove(remove_index);
        if self.selected_row_id == Some(row_id) {
            let next_index = remove_index.min(self.rows.len().saturating_sub(1));
            self.selected_row_id = self.rows.get(next_index).map(|row| row.id);
        }
        self.ensure_row_state();
        true
    }

    fn selected_row_index(&self) -> Option<usize> {
        let selected_row_id = self.selected_row_id?;
        self.rows.iter().position(|row| row.id == selected_row_id)
    }

    fn selected_row_display_name(&self) -> String {
        self.selected_row_index()
            .map(|index| format!("Row {}", index + 1))
            .unwrap_or_else(|| "Row -".to_owned())
    }

    fn add_channel_to_selected_row(&mut self, channel_path: &str) -> (bool, u64) {
        self.ensure_row_state();
        let row_index = self.selected_row_index().unwrap_or(0);
        let row = &mut self.rows[row_index];
        if row
            .channels
            .iter()
            .any(|channel| channel.channel_path == channel_path)
        {
            return (false, row.id);
        }

        row.channels
            .push(RowChannel::new(channel_path, row.channels.len()));
        (true, row.id)
    }

    fn has_visible_channels(&self) -> bool {
        self.rows
            .iter()
            .any(|row| row.channels.iter().any(|channel| channel.visible))
    }
}

impl ChannelStore {
    fn clear_all(&mut self) {
        self.raw_by_channel.clear();
        self.clear_envelope_cache();
    }

    fn clear_envelope_cache(&mut self) {
        self.envelope_cache.clear();
        self.envelope_context = None;
    }

    fn has_channel(&self, channel_path: &str) -> bool {
        self.raw_by_channel.contains_key(channel_path)
    }

    fn channel(&self, channel_path: &str) -> Option<&parquet_waveform::ChannelData> {
        self.raw_by_channel.get(channel_path)
    }

    fn insert_channel(&mut self, channel: parquet_waveform::ChannelData) {
        self.raw_by_channel
            .insert(channel.channel_path.clone(), channel);
        self.clear_envelope_cache();
    }

    fn raw_memory_bytes(&self) -> usize {
        self.raw_by_channel
            .values()
            .map(parquet_waveform::ChannelData::memory_bytes)
            .sum()
    }

    fn prepare_envelope_context(&mut self, time_range: (f64, f64), requested_bucket_count: usize) {
        let context = EnvelopeContext::new(time_range, requested_bucket_count);
        if self.envelope_context != Some(context) {
            self.envelope_cache.clear();
            self.envelope_context = Some(context);
        }
    }

    fn ensure_envelope(
        &mut self,
        channel_path: &str,
        time: &[f64],
        time_range: (f64, f64),
        requested_bucket_count: usize,
    ) -> Option<(parquet_waveform::MinMaxEnvelope, bool)> {
        let key = EnvelopeKey::new(channel_path, time_range, requested_bucket_count);
        let was_cached = self.envelope_cache.contains_key(&key);
        if !was_cached {
            let envelope = {
                let channel = self.raw_by_channel.get(channel_path)?;
                channel.min_max_envelope_for_range(time, time_range, requested_bucket_count)
            };
            self.envelope_cache.insert(key.clone(), envelope);
        }

        self.envelope_cache
            .get(&key)
            .cloned()
            .map(|envelope| (envelope, !was_cached))
    }
}

impl EnvelopeContext {
    fn new(time_range: (f64, f64), requested_bucket_count: usize) -> Self {
        Self {
            range_start_bits: time_range.0.to_bits(),
            range_end_bits: time_range.1.to_bits(),
            requested_bucket_count,
        }
    }
}

impl EnvelopeKey {
    fn new(channel_path: &str, time_range: (f64, f64), requested_bucket_count: usize) -> Self {
        Self {
            channel_path: channel_path.to_owned(),
            range_start_bits: time_range.0.to_bits(),
            range_end_bits: time_range.1.to_bits(),
            requested_bucket_count,
        }
    }
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

fn benchmark_multi_channel(path: String, channels: Vec<String>) -> Result<String, String> {
    let rss_before_schema = process_rss_mib();
    let schema = parquet_schema::read_schema_summary(&path)?;
    let rss_after_schema = process_rss_mib();
    if schema.time_column.is_none() {
        return Err("time column not found".to_owned());
    }

    let selected_channels = if channels.is_empty() {
        schema
            .channels
            .iter()
            .map(|channel| channel.path.clone())
            .collect::<Vec<_>>()
    } else {
        channels
    };
    if selected_channels.is_empty() {
        return Err("no channels selected".to_owned());
    }

    let time = parquet_waveform::read_time_column(&path, &schema)?;
    let rss_after_time = process_rss_mib();
    let full_range = time
        .time_range()
        .ok_or_else(|| "loaded time column has no time range".to_owned())?;
    let mut store = ChannelStore::default();
    let mut channel_results = Vec::with_capacity(selected_channels.len());

    for channel_name in &selected_channels {
        let channel = parquet_waveform::read_channel_values(&path, &schema, channel_name)?;
        if channel.sample_count() != time.sample_count() {
            return Err(format!(
                "time/value length mismatch for {}: {} vs {}",
                channel.channel_path,
                time.sample_count(),
                channel.sample_count()
            ));
        }

        let result = MultiChannelLoadResult {
            channel_name: channel.channel_name.clone(),
            channel_path: channel.channel_path.clone(),
            sample_count: channel.sample_count(),
            read_sec: channel.elapsed.as_secs_f64(),
            memory_bytes: channel.memory_bytes(),
            rss_after_load: process_rss_mib(),
        };
        store.insert_channel(channel);
        channel_results.push(result);
    }

    store.prepare_envelope_context(full_range, BENCH_VISIBLE_ENVELOPE_BUCKETS);
    let mut envelope_results = Vec::with_capacity(selected_channels.len());
    for channel_path in channel_results
        .iter()
        .map(|result| result.channel_path.as_str())
    {
        let Some((envelope, _was_built)) = store.ensure_envelope(
            channel_path,
            &time.time,
            full_range,
            BENCH_VISIBLE_ENVELOPE_BUCKETS,
        ) else {
            return Err(format!(
                "loaded channel is missing from cache: {channel_path}"
            ));
        };
        envelope_results.push(MultiChannelEnvelopeResult {
            channel_path: channel_path.to_owned(),
            source_sample_count: envelope.source_sample_count,
            bucket_count: envelope.bucket_count(),
            bucket_size: envelope.bucket_size,
            draw_point_count: envelope.draw_point_count(),
            elapsed_sec: envelope.elapsed.as_secs_f64(),
        });
    }
    let rss_after_envelopes = process_rss_mib();

    let raw_memory_bytes = time.memory_bytes() + store.raw_memory_bytes();
    let channel_memory_bytes = channel_results
        .iter()
        .map(|result| result.memory_bytes)
        .sum::<usize>();
    let envelope_total_sec = envelope_results
        .iter()
        .map(|result| result.elapsed_sec)
        .sum::<f64>();
    let draw_point_total = envelope_results
        .iter()
        .map(|result| result.draw_point_count)
        .sum::<usize>();

    let mut report = String::new();
    report.push_str(&format!("file: {}\n", schema.path.display()));
    report.push_str(&format!("rows: {}\n", schema.row_count));
    report.push_str(&format!("channels selected: {}\n", selected_channels.len()));
    report.push_str(&format!("channel list: {}\n", selected_channels.join(", ")));
    report.push_str(&format!(
        "time: {} samples, {:.1} MiB, read {:.3}s\n",
        time.sample_count(),
        bytes_to_mib(time.memory_bytes()),
        time.elapsed.as_secs_f64()
    ));
    report.push_str(&format!(
        "channel arrays: {:.1} MiB\n",
        bytes_to_mib(channel_memory_bytes)
    ));
    report.push_str(&format!(
        "raw cache memory: {:.1} MiB\n",
        bytes_to_mib(raw_memory_bytes)
    ));
    report.push_str(&format!(
        "envelopes: {} channels, {} buckets requested, draw points {}, total {:.3}s\n",
        envelope_results.len(),
        BENCH_VISIBLE_ENVELOPE_BUCKETS,
        draw_point_total,
        envelope_total_sec
    ));
    report.push_str(&format!(
        "rss: before_schema={}, after_schema={}, after_time={}, after_all_channels={}, after_envelopes={}\n",
        format_optional_mib(rss_before_schema),
        format_optional_mib(rss_after_schema),
        format_optional_mib(rss_after_time),
        format_optional_mib(channel_results.last().and_then(|result| result.rss_after_load)),
        format_optional_mib(rss_after_envelopes)
    ));
    report.push_str("channel loads:\n");
    for (index, result) in channel_results.iter().enumerate() {
        report.push_str(&format!(
            "  #{:02} {} ({}) samples={} array={:.1} MiB read={:.3}s rss={}\n",
            index + 1,
            result.channel_name,
            result.channel_path,
            result.sample_count,
            bytes_to_mib(result.memory_bytes),
            result.read_sec,
            format_optional_mib(result.rss_after_load)
        ));
    }
    report.push_str("envelope builds:\n");
    for (index, result) in envelope_results.iter().enumerate() {
        report.push_str(&format!(
            "  #{:02} {} samples={} buckets={} bucket={} draw_points={} {:.4}s\n",
            index + 1,
            result.channel_path,
            result.source_sample_count,
            result.bucket_count,
            result.bucket_size,
            result.draw_point_count,
            result.elapsed_sec
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

#[derive(Debug)]
struct MultiChannelLoadResult {
    channel_name: String,
    channel_path: String,
    sample_count: usize,
    read_sec: f64,
    memory_bytes: usize,
    rss_after_load: Option<f64>,
}

#[derive(Debug)]
struct MultiChannelEnvelopeResult {
    channel_path: String,
    source_sample_count: usize,
    bucket_count: usize,
    bucket_size: usize,
    draw_point_count: usize,
    elapsed_sec: f64,
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
        match parquet_schema::read_schema_summary(&self.dataset.parquet_path) {
            Ok(summary) => {
                let time_status = if summary.time_column.is_some() {
                    "time column detected"
                } else {
                    "time column missing"
                };
                self.view.reset_for_schema(&summary);
                self.dataset.shared_time = None;
                self.dataset.loaded_channels.clear_all();
                self.load.error = None;
                self.load.progress = None;
                self.load.status = format!(
                    "Loaded schema: {} rows, {} channels, {time_status}",
                    summary.row_count,
                    summary.channels.len()
                );
                self.dataset.schema = Some(summary);
            }
            Err(error) => {
                self.load.status = format!("Schema load failed: {error}");
                self.load.error = Some(error);
                self.dataset.schema = None;
                self.dataset.shared_time = None;
                self.dataset.loaded_channels.clear_all();
                self.view.reset_empty();
            }
        }
    }

    fn load_selected_channel(&mut self) {
        if self.view.selected_channel.is_empty() {
            self.load.status = "Select a channel before loading waveform data".to_owned();
            return;
        }

        let Some(summary) = self.dataset.schema.clone() else {
            self.load.status = "Load schema before loading waveform data".to_owned();
            return;
        };
        if summary.time_column.is_none() {
            self.load.status = "Time column is required before loading waveform data".to_owned();
            return;
        }

        self.load.pending_jobs = 1;
        let path = self.dataset.parquet_path.clone();
        let selected_channel = self.view.selected_channel.clone();
        let time_was_cached = self.dataset.shared_time.is_some();

        if !time_was_cached {
            match parquet_waveform::read_time_column(&path, &summary) {
                Ok(time) => {
                    self.dataset.shared_time = Some(time);
                }
                Err(error) => {
                    self.load.pending_jobs = 0;
                    self.load.error = Some(error.clone());
                    self.load.status = format!("Time load failed: {error}");
                    return;
                }
            }
        }

        let Some(shared_time) = self.dataset.shared_time.as_ref() else {
            self.load.pending_jobs = 0;
            self.load.status = "Time data is not available".to_owned();
            return;
        };
        let time_sample_count = shared_time.sample_count();
        let time_read_sec = shared_time.elapsed.as_secs_f64();

        let channel_was_cached = self.dataset.loaded_channels.has_channel(&selected_channel);
        let channel_path = if channel_was_cached {
            selected_channel.clone()
        } else {
            match parquet_waveform::read_channel_values(&path, &summary, &selected_channel) {
                Ok(channel) => {
                    if channel.sample_count() != time_sample_count {
                        self.load.pending_jobs = 0;
                        self.load.status = format!(
                            "Waveform load failed: time/value length mismatch: {} vs {}",
                            time_sample_count,
                            channel.sample_count()
                        );
                        return;
                    }
                    let channel_path = channel.channel_path.clone();
                    self.dataset.loaded_channels.insert_channel(channel);
                    channel_path
                }
                Err(error) => {
                    self.load.pending_jobs = 0;
                    self.load.error = Some(error.clone());
                    self.load.status = format!("Waveform load failed: {error}");
                    return;
                }
            }
        };

        let Some(channel) = self.dataset.loaded_channels.channel(&channel_path) else {
            self.load.pending_jobs = 0;
            self.load.status = format!("Loaded channel is missing from cache: {channel_path}");
            return;
        };

        let channel_name = channel.channel_name.clone();
        let channel_read_sec = channel.elapsed.as_secs_f64();
        let channel_sample_count = channel.sample_count();
        let channel_memory = channel.memory_bytes();
        let target_row_label = self.view.selected_row_display_name();
        let (row_added, row_id) = self.view.add_channel_to_selected_row(&channel_path);
        if self.view.x_range.is_none() {
            self.view.x_range = self
                .dataset
                .shared_time
                .as_ref()
                .and_then(parquet_waveform::TimeData::time_range);
        }

        self.load.pending_jobs = 0;
        self.load.error = None;
        let cache_note = if channel_was_cached {
            "reused cached"
        } else {
            "loaded"
        };
        let row_note = if row_added {
            format!("added to {target_row_label}")
        } else {
            format!("already in {target_row_label}")
        };
        let time_note = if time_was_cached {
            "time cached"
        } else {
            "time loaded"
        };
        let total_memory = self
            .dataset
            .shared_time
            .as_ref()
            .map_or(0, parquet_waveform::TimeData::memory_bytes)
            + self.dataset.loaded_channels.raw_memory_bytes();
        self.load.status = format!(
            "{cache_note}: {channel_name} ({channel_sample_count} samples, {:.1} MiB, read {:.3}s), {row_note} (id {row_id}); {time_note} {:.3}s; cache {} ch, total {:.1} MiB",
            bytes_to_mib(channel_memory),
            channel_read_sec,
            time_read_sec,
            self.dataset.loaded_channels.raw_by_channel.len(),
            bytes_to_mib(total_memory)
        );
    }

    fn reset_view_range(&mut self) {
        self.view.x_range = self
            .dataset
            .shared_time
            .as_ref()
            .and_then(parquet_waveform::TimeData::time_range);
        self.dataset.loaded_channels.clear_envelope_cache();
    }

    fn visible_row_traces(
        &mut self,
        requested_bucket_count: usize,
        dark_mode: bool,
    ) -> Vec<VisibleRowTrace> {
        let rows = self.view.rows.clone();
        if rows.is_empty() {
            return Vec::new();
        }

        let Some(shared_time) = self.dataset.shared_time.as_ref() else {
            return rows
                .into_iter()
                .enumerate()
                .map(|(row_index, row)| VisibleRowTrace {
                    row_id: row.id,
                    row_index,
                    row_channel_count: row.channels.len(),
                    traces: Vec::new(),
                })
                .collect();
        };
        let Some(full_range) = shared_time.time_range() else {
            self.view.x_range = None;
            self.dataset.loaded_channels.clear_envelope_cache();
            return rows
                .into_iter()
                .enumerate()
                .map(|(row_index, row)| VisibleRowTrace {
                    row_id: row.id,
                    row_index,
                    row_channel_count: row.channels.len(),
                    traces: Vec::new(),
                })
                .collect();
        };

        let min_span = min_view_span(full_range, shared_time.sample_count());
        let view_range = clamp_view_range(
            self.view.x_range.unwrap_or(full_range),
            full_range,
            min_span,
        );
        if self.view.x_range != Some(view_range) {
            self.view.x_range = Some(view_range);
            self.dataset.loaded_channels.clear_envelope_cache();
        }

        let mut visible_rows = Vec::with_capacity(rows.len());
        let mut built_count = 0usize;
        let mut exact_step_count = 0usize;
        let mut edge_step_count = 0usize;
        {
            let dataset = &mut self.dataset;
            let Some(shared_time) = dataset.shared_time.as_ref() else {
                return visible_rows;
            };
            let time_values = &shared_time.time;
            let loaded_channels = &mut dataset.loaded_channels;
            loaded_channels.prepare_envelope_context(view_range, requested_bucket_count);

            for (row_index, row) in rows.into_iter().enumerate() {
                let mut traces = Vec::with_capacity(row.channels.len());
                let row_channel_count = row.channels.len();
                for row_channel in row.channels {
                    if !row_channel.visible {
                        continue;
                    }

                    let Some((channel_name, channel_path, sample_count)) = loaded_channels
                        .channel(&row_channel.channel_path)
                        .map(|channel| {
                            (
                                channel.channel_name.clone(),
                                channel.channel_path.clone(),
                                channel.sample_count(),
                            )
                        })
                    else {
                        continue;
                    };

                    let data = match row_channel.draw_mode {
                        DrawMode::Line => {
                            let Some((envelope, was_built)) = loaded_channels.ensure_envelope(
                                &channel_path,
                                time_values,
                                view_range,
                                requested_bucket_count,
                            ) else {
                                continue;
                            };
                            if was_built {
                                built_count += 1;
                            }
                            VisibleTraceData::Envelope(envelope)
                        }
                        DrawMode::Step => {
                            if let Some(raw_step) =
                                loaded_channels.channel(&channel_path).and_then(|channel| {
                                    build_raw_step_trace(
                                        time_values,
                                        &channel.values,
                                        view_range,
                                        MAX_EXACT_STEP_SAMPLES,
                                    )
                                })
                            {
                                exact_step_count += 1;
                                VisibleTraceData::RawStep(raw_step)
                            } else if let Some(edge_step) =
                                loaded_channels.channel(&channel_path).and_then(|channel| {
                                    build_change_point_step_trace(
                                        time_values,
                                        &channel.values,
                                        view_range,
                                        MAX_STEP_CHANGE_POINTS,
                                    )
                                })
                            {
                                edge_step_count += 1;
                                VisibleTraceData::RawStep(edge_step)
                            } else {
                                let Some((envelope, was_built)) = loaded_channels.ensure_envelope(
                                    &channel_path,
                                    time_values,
                                    view_range,
                                    requested_bucket_count,
                                ) else {
                                    continue;
                                };
                                if was_built {
                                    built_count += 1;
                                }
                                VisibleTraceData::Envelope(envelope)
                            }
                        }
                    };

                    traces.push(VisibleTrace {
                        channel_name,
                        channel_path,
                        sample_count,
                        color: row_channel.color(dark_mode),
                        line_width: row_channel
                            .line_width
                            .clamp(MIN_TRACE_LINE_WIDTH, MAX_TRACE_LINE_WIDTH),
                        draw_mode: row_channel.draw_mode,
                        data,
                    });
                }

                visible_rows.push(VisibleRowTrace {
                    row_id: row.id,
                    row_index,
                    row_channel_count,
                    traces,
                });
            }
        }

        if built_count > 0 || exact_step_count > 0 || edge_step_count > 0 {
            let visible_channel_count = visible_rows
                .iter()
                .map(|row| row.traces.len())
                .sum::<usize>();
            let source_sample_count = visible_rows
                .iter()
                .flat_map(|row| row.traces.iter())
                .next()
                .map(trace_source_sample_count)
                .unwrap_or_default();
            self.load.status = format!(
                "View {:.6}..{:.6}s: {} rows, {} ch, {} visible samples, built {} envelope(s), raw step {}, edge step {}, cache {}",
                view_range.0,
                view_range.1,
                visible_rows.len(),
                visible_channel_count,
                source_sample_count,
                built_count,
                exact_step_count,
                edge_step_count,
                self.dataset.loaded_channels.envelope_cache.len()
            );
        }

        visible_rows
    }

    fn handle_plot_interaction(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        plot_rects: &[egui::Rect],
    ) {
        let Some(shared_time) = self.dataset.shared_time.as_ref() else {
            return;
        };
        let Some(full_range) = shared_time.time_range() else {
            return;
        };

        let min_span = min_view_span(full_range, shared_time.sample_count());
        let mut next_range = clamp_view_range(
            self.view.x_range.unwrap_or(full_range),
            full_range,
            min_span,
        );
        let current_range = next_range;
        let mut changed = false;
        let (pointer_pos, zoom_delta, scroll_delta) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.zoom_delta(),
                input.smooth_scroll_delta(),
            )
        });
        let Some((pointer_pos, plot_rect)) = pointer_pos.and_then(|pointer_pos| {
            plot_rects
                .iter()
                .copied()
                .find(|plot_rect| plot_rect.contains(pointer_pos))
                .map(|plot_rect| (pointer_pos, plot_rect))
        }) else {
            return;
        };

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

        if plot_rect.width() > 1.0 {
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
            self.view.x_range = Some(next_range);
            self.dataset.loaded_channels.clear_envelope_cache();
        }
    }
}

impl eframe::App for PanelYApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Panel_y Rust Phase 2");
                ui.separator();
                ui.label(&self.load.status);
                if self.load.pending_jobs > 0 {
                    ui.separator();
                    ui.label(format!("jobs: {}", self.load.pending_jobs));
                }
            });
        });

        egui::Panel::left("controls")
            .resizable(false)
            .default_size(320.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("controls_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.heading("Dataset");
                        ui.label("Parquet path");
                        ui.text_edit_singleline(&mut self.dataset.parquet_path);

                        ui.add_space(12.0);
                        if ui.button("Load Schema").clicked() {
                            self.load_schema();
                        }

                        let can_load_waveform =
                            self.dataset.schema.as_ref().is_some_and(|schema| {
                                schema.time_column.is_some() && !schema.channels.is_empty()
                            }) && !self.view.selected_channel.is_empty();
                        let add_channel_label =
                            format!("Load / Add to {}", self.view.selected_row_display_name());
                        if ui
                            .add_enabled(can_load_waveform, egui::Button::new(add_channel_label))
                            .clicked()
                        {
                            self.load_selected_channel();
                        }

                        ui.add_space(16.0);
                        if draw_schema_controls(
                            ui,
                            &mut self.view.selected_channel,
                            self.dataset.schema.as_ref(),
                        ) {
                            self.load.status =
                                format!("Selected channel: {}", self.view.selected_channel);
                        }

                        if self.dataset.shared_time.is_some() || self.view.has_visible_channels() {
                            ui.add_space(16.0);
                            ui.heading("View");
                            if ui.button("Reset X Range").clicked() {
                                self.reset_view_range();
                            }
                            if let Some((start, end)) = self.view.x_range {
                                ui.monospace(format!("x: {start:.6} .. {end:.6} s"));
                            }
                        }

                        draw_channel_cache_controls(ui, &self.dataset);
                        if draw_row_controls(ui, &mut self.view, self.dataset.schema.as_ref()) {
                            self.dataset.loaded_channels.clear_envelope_cache();
                        }
                    });
            });

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            let viewport_size = ui.available_size();
            egui::ScrollArea::vertical()
                .id_salt("waveform_scroll")
                .auto_shrink([false, false])
                .scroll_bar_visibility(
                    egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                )
                .scroll_source(egui::containers::scroll_area::ScrollSource::SCROLL_BAR)
                .show(ui, |ui| {
                    let row_count = self.view.rows.len().max(1);
                    let content_height = waveform_content_height(viewport_size.y, row_count);
                    let content_size =
                        egui::vec2(viewport_size.x.max(1.0), content_height.max(1.0));
                    let (rect, response) =
                        ui.allocate_exact_size(content_size, egui::Sense::click_and_drag());
                    let painter = ui.painter_at(rect);
                    let row_rects = row_outer_rects(rect, row_count);
                    let plot_rects = row_rects
                        .iter()
                        .copied()
                        .map(plot_area_rect)
                        .collect::<Vec<_>>();
                    let requested_buckets = plot_rects
                        .first()
                        .copied()
                        .map(visible_envelope_bucket_count)
                        .unwrap_or(MIN_VISIBLE_ENVELOPE_BUCKETS);

                    self.handle_plot_interaction(ui, &response, &plot_rects);
                    let visible_rows =
                        self.visible_row_traces(requested_buckets, ui.visuals().dark_mode);

                    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
                    for (index, row_rect) in row_rects.iter().copied().enumerate() {
                        let Some(plot_rect) = plot_rects.get(index).copied() else {
                            continue;
                        };
                        let visible_row = visible_rows.get(index);
                        let row_id = visible_row.map(|row| row.row_id).unwrap_or(index as u64);
                        let row_index = visible_row.map(|row| row.row_index).unwrap_or(index);
                        let selected = self.view.selected_row_id == Some(row_id);

                        draw_row_marker(&painter, row_rect, row_index, selected, ui.visuals());
                        draw_plot_frame(&painter, plot_rect, ui.visuals());

                        match visible_row {
                            Some(row) if !row.traces.is_empty() => {
                                draw_waveform_traces(
                                    &painter,
                                    plot_rect,
                                    ui.visuals(),
                                    &row.traces,
                                );
                            }
                            Some(row) if row.row_channel_count > 0 => {
                                draw_status_label(&painter, plot_rect, "All channels are hidden");
                            }
                            _ => {
                                draw_row_placeholder(
                                    &painter,
                                    plot_rect,
                                    self.dataset.schema.as_ref(),
                                    &self.view.selected_channel,
                                    row_index,
                                );
                            }
                        }
                    }
                });
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

fn draw_channel_cache_controls(ui: &mut egui::Ui, dataset: &DatasetState) {
    ui.add_space(16.0);
    ui.heading("Cache");

    if let Some(time) = &dataset.shared_time {
        let file_name = time
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("-");
        ui.label(format!(
            "Time: {} / {} ({} samples, {:.1} MiB, col #{}, file {}, {:.3}s)",
            time.column_name,
            time.column_path,
            time.sample_count(),
            bytes_to_mib(time.memory_bytes()),
            time.projected_column_index,
            file_name,
            time.elapsed.as_secs_f64()
        ));
    } else {
        ui.label("Time: not loaded");
    }

    ui.label(format!(
        "Channels: {} cached ({:.1} MiB)",
        dataset.loaded_channels.raw_by_channel.len(),
        bytes_to_mib(dataset.loaded_channels.raw_memory_bytes())
    ));
    for channel in dataset.loaded_channels.raw_by_channel.values() {
        let file_name = channel
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("-");
        ui.monospace(format!(
            "  #{} {} ({} samples, file {})",
            channel.projected_column_index,
            channel.channel_name,
            channel.sample_count(),
            file_name
        ));
    }
}

fn draw_row_controls(
    ui: &mut egui::Ui,
    view: &mut ViewState,
    schema: Option<&parquet_schema::SchemaSummary>,
) -> bool {
    ui.add_space(16.0);
    ui.heading("Rows");
    view.ensure_row_state();

    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.button("Add Row").clicked() {
            view.add_row();
            changed = true;
        }
    });

    ui.label(format!("Load target: {}", view.selected_row_display_name()));

    let can_delete_row = view.rows.len() > 1;
    let mut selected_row_id = view.selected_row_id;
    let mut remove_row_id = None;

    for (row_index, row) in view.rows.iter_mut().enumerate() {
        ui.separator();
        ui.horizontal(|ui| {
            let row_label = format!("Row {}", row_index + 1);
            if ui
                .selectable_label(selected_row_id == Some(row.id), row_label)
                .clicked()
            {
                selected_row_id = Some(row.id);
            }
            if ui
                .add_enabled(can_delete_row, egui::Button::new("Delete"))
                .clicked()
            {
                remove_row_id = Some(row.id);
            }
        });

        if row.channels.is_empty() {
            ui.label("No channels in row");
            continue;
        }

        let mut remove_channel = None;
        for channel in &mut row.channels {
            ui.push_id(
                ("channel_style", row.id, channel.channel_path.clone()),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .checkbox(&mut channel.visible, "")
                            .on_hover_text("Visible")
                            .changed()
                        {
                            changed = true;
                        }

                        let mut color = channel.color(ui.visuals().dark_mode);
                        if egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut color,
                            egui::color_picker::Alpha::Opaque,
                        )
                        .changed()
                        {
                            channel.color_override = Some(color);
                            changed = true;
                        }

                        ui.label(channel_display_name(schema, &channel.channel_path));
                        egui::ComboBox::from_id_salt("draw_mode")
                            .selected_text(channel.draw_mode.as_str())
                            .show_ui(ui, |ui| {
                                for draw_mode in DrawMode::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut channel.draw_mode,
                                            draw_mode,
                                            draw_mode.as_str(),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });

                        let mut line_width = channel.line_width;
                        if ui
                            .add(
                                egui::DragValue::new(&mut line_width)
                                    .speed(0.1)
                                    .range(MIN_TRACE_LINE_WIDTH..=MAX_TRACE_LINE_WIDTH)
                                    .prefix("w "),
                            )
                            .on_hover_text("Line width")
                            .changed()
                        {
                            channel.line_width =
                                line_width.clamp(MIN_TRACE_LINE_WIDTH, MAX_TRACE_LINE_WIDTH);
                            changed = true;
                        }

                        if channel.color_override.is_some()
                            && ui.small_button("Reset color").clicked()
                        {
                            channel.color_override = None;
                            changed = true;
                        }

                        if ui.small_button("Remove").clicked() {
                            remove_channel = Some(channel.channel_path.clone());
                        }
                    });
                },
            );
        }

        if let Some(channel_path) = remove_channel {
            row.channels
                .retain(|channel| channel.channel_path != channel_path);
            changed = true;
        }
    }

    view.selected_row_id = selected_row_id;
    if let Some(row_id) = remove_row_id
        && view.remove_row(row_id)
    {
        changed = true;
    }

    changed
}

fn channel_display_name(
    schema: Option<&parquet_schema::SchemaSummary>,
    channel_path: &str,
) -> String {
    schema
        .and_then(|schema| {
            schema
                .channels
                .iter()
                .find(|channel| channel.path == channel_path)
        })
        .map(|channel| channel.display_name().to_owned())
        .unwrap_or_else(|| channel_path.to_owned())
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
    let left = if rect.width() > 180.0 {
        58.0_f32
    } else {
        12.0_f32
    }
    .min(rect.width() * 0.4);
    let bottom = if rect.height() > 120.0 {
        36.0_f32
    } else {
        12.0_f32
    }
    .min(rect.height() * 0.3);
    let top = 20.0_f32.min(rect.height() * 0.3);
    let right = 18.0_f32.min(rect.width() * 0.2);

    egui::Rect::from_min_max(
        egui::pos2(rect.left() + left, rect.top() + top),
        egui::pos2(rect.right() - right, rect.bottom() - bottom),
    )
}

fn waveform_content_height(viewport_height: f32, row_count: usize) -> f32 {
    let row_count = row_count.max(1);
    let total_gap = if row_count > 1 {
        WAVEFORM_ROW_GAP * row_count.saturating_sub(1) as f32
    } else {
        0.0
    };
    let minimum_height = MIN_WAVEFORM_ROW_HEIGHT * row_count as f32 + total_gap;

    viewport_height.max(minimum_height)
}

fn row_outer_rects(rect: egui::Rect, row_count: usize) -> Vec<egui::Rect> {
    if row_count == 0 || rect.height() <= 0.0 || rect.width() <= 0.0 {
        return Vec::new();
    }

    let gap = if row_count > 1 {
        WAVEFORM_ROW_GAP.min(rect.height() / row_count as f32 * 0.2)
    } else {
        0.0
    };
    let total_gap = gap * row_count.saturating_sub(1) as f32;
    let row_height = ((rect.height() - total_gap).max(1.0) / row_count as f32).max(1.0);
    let mut row_rects = Vec::with_capacity(row_count);
    let mut top = rect.top();

    for index in 0..row_count {
        let bottom = if index + 1 == row_count {
            rect.bottom()
        } else {
            (top + row_height).min(rect.bottom())
        };
        row_rects.push(egui::Rect::from_min_max(
            egui::pos2(rect.left(), top),
            egui::pos2(rect.right(), bottom),
        ));
        top = bottom + gap;
    }

    row_rects
}

fn draw_waveform_traces(
    painter: &egui::Painter,
    rect: egui::Rect,
    visuals: &egui::Visuals,
    visible_traces: &[VisibleTrace],
) {
    let Some(first) = visible_traces.first() else {
        draw_status_label(painter, rect, "No channel loaded");
        return;
    };
    let Some((time_min, time_max)) = trace_time_range(first) else {
        draw_status_label(painter, rect, "No time range available");
        return;
    };

    let mut combined_value_range = None;
    for visible in visible_traces {
        if let Some((min, max)) = trace_value_range(visible) {
            combined_value_range = extend_range(combined_value_range, min);
            combined_value_range = extend_range(combined_value_range, max);
        }
    }
    let Some((value_min, value_max)) = combined_value_range else {
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

    let to_screen = |time: f64, value: f64| -> egui::Pos2 {
        let x_t = ((time - time_min) / time_span) as f32;
        let y_t = ((value - value_min) / value_span) as f32;
        egui::pos2(
            egui::lerp(rect.left()..=rect.right(), x_t),
            egui::lerp(rect.bottom()..=rect.top(), y_t),
        )
    };

    for visible in visible_traces {
        let line_width = visible
            .line_width
            .clamp(MIN_TRACE_LINE_WIDTH, MAX_TRACE_LINE_WIDTH);
        let vertical_stroke = egui::Stroke::new(
            (line_width * 0.8).max(1.0),
            visible.color.linear_multiply(0.45),
        );
        let line_stroke = egui::Stroke::new(line_width, visible.color);

        match &visible.data {
            VisibleTraceData::Envelope(envelope) => {
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
            }
            VisibleTraceData::RawStep(raw_step) => {
                draw_raw_step_trace(
                    painter,
                    raw_step,
                    line_stroke,
                    (time_min, time_max),
                    to_screen,
                );
            }
        }
    }

    draw_axis_labels(
        painter,
        rect,
        visible_traces,
        (time_min, time_max),
        (value_min, value_max),
        visuals,
    );
}

fn draw_raw_step_trace(
    painter: &egui::Painter,
    raw_step: &RawStepTrace,
    stroke: egui::Stroke,
    time_range: (f64, f64),
    to_screen: impl Fn(f64, f64) -> egui::Pos2,
) {
    let Some(first) = raw_step.samples.first().copied() else {
        return;
    };

    let mut points = Vec::with_capacity(raw_step.samples.len().saturating_mul(2).max(2));
    points.push(to_screen(
        first.time.max(time_range.0),
        f64::from(first.value),
    ));

    let mut previous = first;
    for current in raw_step.samples.iter().skip(1).copied() {
        points.push(to_screen(current.time, f64::from(previous.value)));
        points.push(to_screen(current.time, f64::from(current.value)));
        previous = current;
    }

    if time_range.1 > previous.time {
        points.push(to_screen(time_range.1, f64::from(previous.value)));
    }

    if points.len() >= 2 {
        painter.line(points, stroke);
    }
}

fn trace_time_range(trace: &VisibleTrace) -> Option<(f64, f64)> {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) => envelope.time_range,
        VisibleTraceData::RawStep(raw_step) => raw_step.time_range,
    }
}

fn trace_value_range(trace: &VisibleTrace) -> Option<(f64, f64)> {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) => envelope.value_range,
        VisibleTraceData::RawStep(raw_step) => raw_step.value_range,
    }
}

fn trace_source_sample_count(trace: &VisibleTrace) -> usize {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) => envelope.source_sample_count,
        VisibleTraceData::RawStep(raw_step) => raw_step.source_sample_count,
    }
}

fn trace_bucket_count(trace: &VisibleTrace) -> usize {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) => envelope.bucket_count(),
        VisibleTraceData::RawStep(_) => 0,
    }
}

fn trace_draw_point_count(trace: &VisibleTrace) -> usize {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) => envelope.draw_point_count(),
        VisibleTraceData::RawStep(raw_step) => raw_step.samples.len().saturating_mul(2),
    }
}

fn trace_step_kind(trace: &VisibleTrace) -> Option<StepTraceKind> {
    match &trace.data {
        VisibleTraceData::RawStep(raw_step) => Some(raw_step.kind),
        VisibleTraceData::Envelope(_) => None,
    }
}

fn visible_envelope_bucket_count(rect: egui::Rect) -> usize {
    (rect.width().round() as usize)
        .clamp(MIN_VISIBLE_ENVELOPE_BUCKETS, MAX_VISIBLE_ENVELOPE_BUCKETS)
}

fn channel_color(index: usize, dark_mode: bool) -> egui::Color32 {
    const DARK_COLORS: [(u8, u8, u8); 8] = [
        (80, 190, 255),
        (255, 176, 70),
        (120, 220, 120),
        (255, 110, 150),
        (190, 150, 255),
        (95, 220, 210),
        (240, 220, 90),
        (210, 210, 220),
    ];
    const LIGHT_COLORS: [(u8, u8, u8); 8] = [
        (0, 94, 155),
        (190, 95, 0),
        (32, 130, 60),
        (185, 40, 80),
        (110, 80, 190),
        (0, 130, 130),
        (150, 125, 0),
        (80, 80, 90),
    ];

    let colors = if dark_mode { DARK_COLORS } else { LIGHT_COLORS };
    let (r, g, b) = colors[index % colors.len()];
    egui::Color32::from_rgb(r, g, b)
}

fn draw_axis_labels(
    painter: &egui::Painter,
    rect: egui::Rect,
    visible_traces: &[VisibleTrace],
    time_range: (f64, f64),
    value_range: (f64, f64),
    visuals: &egui::Visuals,
) {
    let text_color = visuals.text_color();
    let weak_color = visuals.weak_text_color();
    let font = egui::FontId::monospace(12.0);
    let channel_label = visible_traces
        .iter()
        .take(3)
        .map(|visible| {
            if visible.channel_name.is_empty() {
                visible.channel_path.as_str()
            } else {
                visible.channel_name.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let channel_label = if visible_traces.len() > 3 {
        format!("{channel_label}, ...")
    } else {
        channel_label
    };
    let source_sample_count = visible_traces
        .first()
        .map(trace_source_sample_count)
        .unwrap_or_default();
    let raw_sample_count = visible_traces
        .first()
        .map(|visible| visible.sample_count)
        .unwrap_or_default();
    let bucket_count: usize = visible_traces.iter().map(trace_bucket_count).sum();
    let draw_point_count: usize = visible_traces.iter().map(trace_draw_point_count).sum();
    let step_count = visible_traces
        .iter()
        .filter(|visible| visible.draw_mode == DrawMode::Step)
        .count();
    let edge_step_count = visible_traces
        .iter()
        .filter(|visible| trace_step_kind(visible) == Some(StepTraceKind::ChangePoints))
        .count();

    painter.text(
        rect.left_top() + egui::vec2(0.0, -16.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{}  ch={}  step={}  edge={}  visible_samples={}  raw_samples={}  buckets={}  draw_points={}",
            channel_label,
            visible_traces.len(),
            step_count,
            edge_step_count,
            source_sample_count,
            raw_sample_count,
            bucket_count,
            draw_point_count
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

fn draw_row_marker(
    painter: &egui::Painter,
    rect: egui::Rect,
    row_index: usize,
    selected: bool,
    visuals: &egui::Visuals,
) {
    let color = if selected {
        visuals.selection.stroke.color
    } else {
        visuals.weak_text_color()
    };
    painter.text(
        rect.right_top() + egui::vec2(-4.0, 4.0),
        egui::Align2::RIGHT_TOP,
        format!("Row {}", row_index + 1),
        egui::FontId::monospace(12.0),
        color,
    );
}

fn draw_row_placeholder(
    painter: &egui::Painter,
    rect: egui::Rect,
    schema: Option<&parquet_schema::SchemaSummary>,
    selected_channel: &str,
    row_index: usize,
) {
    let label = match schema {
        Some(schema) if schema.time_column.is_some() && !selected_channel.is_empty() => {
            format!(
                "Row {} is empty. Load time + {selected_channel} into the selected row.",
                row_index + 1
            )
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

fn extend_range(range: Option<(f64, f64)>, value: f64) -> Option<(f64, f64)> {
    if !value.is_finite() {
        return range;
    }

    match range {
        Some((min, max)) => Some((min.min(value), max.max(value))),
        None => Some((value, value)),
    }
}

fn build_raw_step_trace(
    time: &[f64],
    values: &[f32],
    time_range: (f64, f64),
    max_samples: usize,
) -> Option<RawStepTrace> {
    let (range_start, range_end) = normalized_range(time_range)?;
    let sample_count = time.len().min(values.len());
    if sample_count == 0 {
        return None;
    }

    let start = time
        .partition_point(|time| *time < range_start)
        .min(sample_count);
    let end = time
        .partition_point(|time| *time <= range_end)
        .min(sample_count);
    let context_start = start.saturating_sub(usize::from(start > 0));
    let source_sample_count = end.saturating_sub(start);
    let draw_sample_count = end.saturating_sub(context_start);
    if draw_sample_count == 0 || draw_sample_count > max_samples {
        return None;
    }

    let mut samples = Vec::with_capacity(draw_sample_count);
    let mut value_range = None;

    if context_start < start {
        let value = values[context_start];
        if value.is_finite() {
            samples.push(StepSample {
                time: range_start,
                value,
            });
            value_range = extend_range(value_range, f64::from(value));
        }
    }

    for index in start..end {
        let sample_time = time[index];
        let value = values[index];
        if !sample_time.is_finite() || !value.is_finite() {
            continue;
        }

        samples.push(StepSample {
            time: sample_time.clamp(range_start, range_end),
            value,
        });
        value_range = extend_range(value_range, f64::from(value));
    }

    if samples.is_empty() || value_range.is_none() {
        return None;
    }

    Some(RawStepTrace {
        samples,
        source_sample_count,
        time_range: Some((range_start, range_end)),
        value_range,
        kind: StepTraceKind::RawSamples,
    })
}

fn build_change_point_step_trace(
    time: &[f64],
    values: &[f32],
    time_range: (f64, f64),
    max_change_points: usize,
) -> Option<RawStepTrace> {
    let (range_start, range_end) = normalized_range(time_range)?;
    let sample_count = time.len().min(values.len());
    if sample_count == 0 {
        return None;
    }

    let start = time
        .partition_point(|time| *time < range_start)
        .min(sample_count);
    let end = time
        .partition_point(|time| *time <= range_end)
        .min(sample_count);
    let context_start = start.saturating_sub(usize::from(start > 0));
    let source_sample_count = end.saturating_sub(start);
    if end.saturating_sub(context_start) == 0 {
        return None;
    }

    let mut samples = Vec::new();
    let mut value_range = None;
    let mut previous_value = None;
    let mut change_points = 0usize;

    if context_start < start {
        let value = values[context_start];
        if value.is_finite() {
            samples.push(StepSample {
                time: range_start,
                value,
            });
            value_range = extend_range(value_range, f64::from(value));
            previous_value = Some(value);
        }
    }

    for index in start..end {
        let sample_time = time[index];
        let value = values[index];
        if !sample_time.is_finite() || !value.is_finite() {
            continue;
        }

        value_range = extend_range(value_range, f64::from(value));
        match previous_value {
            Some(previous) if previous == value => {}
            Some(_) => {
                if change_points >= max_change_points {
                    return None;
                }
                samples.push(StepSample {
                    time: sample_time.clamp(range_start, range_end),
                    value,
                });
                previous_value = Some(value);
                change_points += 1;
            }
            None => {
                samples.push(StepSample {
                    time: sample_time.clamp(range_start, range_end),
                    value,
                });
                previous_value = Some(value);
            }
        }
    }

    if samples.is_empty() || value_range.is_none() {
        return None;
    }

    Some(RawStepTrace {
        samples,
        source_sample_count,
        time_range: Some((range_start, range_end)),
        value_range,
        kind: StepTraceKind::ChangePoints,
    })
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

#[cfg(test)]
mod app_tests {
    use super::*;

    #[test]
    fn adds_channels_to_the_selected_row() {
        let mut view = ViewState::default();

        let (added_first, first_row_id) = view.add_channel_to_selected_row("sine_50Hz");
        assert!(added_first);
        assert_eq!(first_row_id, 0);
        assert_eq!(view.rows[0].channels.len(), 1);
        assert_eq!(view.rows[0].channels[0].draw_mode, DrawMode::Line);
        assert!(view.rows[0].channels[0].visible);
        assert_eq!(
            view.rows[0].channels[0].line_width,
            DEFAULT_TRACE_LINE_WIDTH
        );
        assert_eq!(view.rows[0].channels[0].color_override, None);

        let second_row_id = view.add_row();
        let (added_second, target_row_id) = view.add_channel_to_selected_row("pwm_1kHz");
        assert!(added_second);
        assert_eq!(target_row_id, second_row_id);
        assert!(
            view.rows[0]
                .channels
                .iter()
                .any(|ch| ch.channel_path == "sine_50Hz")
        );
        assert!(
            view.rows[1]
                .channels
                .iter()
                .any(|ch| ch.channel_path == "pwm_1kHz")
        );
    }

    #[test]
    fn rejects_duplicate_channels_within_a_row() {
        let mut view = ViewState::default();

        let (added_first, _) = view.add_channel_to_selected_row("sine_50Hz");
        let (added_duplicate, _) = view.add_channel_to_selected_row("sine_50Hz");

        assert!(added_first);
        assert!(!added_duplicate);
        assert_eq!(view.rows[0].channels.len(), 1);
    }

    #[test]
    fn hidden_channels_are_not_counted_as_visible() {
        let mut view = ViewState::default();
        let (added, _) = view.add_channel_to_selected_row("sine_50Hz");
        assert!(added);
        assert!(view.has_visible_channels());

        view.rows[0].channels[0].visible = false;

        assert!(!view.has_visible_channels());
    }

    #[test]
    fn deleting_selected_row_moves_selection_to_an_existing_row() {
        let mut view = ViewState::default();
        view.add_row();
        view.add_row();
        let selected_before_delete = view.selected_row_id.expect("selected row");

        assert!(view.remove_row(selected_before_delete));
        assert_eq!(view.rows.len(), 2);
        assert!(
            view.selected_row_id
                .is_some_and(|row_id| view.rows.iter().any(|row| row.id == row_id))
        );
    }

    #[test]
    fn row_plot_rects_remain_valid_for_many_rows() {
        let content_height = waveform_content_height(160.0, 8);
        assert!(content_height >= MIN_WAVEFORM_ROW_HEIGHT * 8.0);

        let outer =
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(640.0, content_height));
        let rows = row_outer_rects(outer, 8);

        assert_eq!(rows.len(), 8);
        for row in rows {
            let plot = plot_area_rect(row);
            assert!(plot.width() > 0.0);
            assert!(plot.height() > 0.0);
            assert!(outer.contains_rect(row));
        }
    }

    #[test]
    fn builds_raw_step_trace_with_previous_state_at_range_start() {
        let time = [0.0, 1.0, 2.0, 3.0];
        let values = [0.0, 1.0, 0.0, 1.0];

        let trace = build_raw_step_trace(&time, &values, (1.5, 2.5), 10).expect("raw step trace");

        assert_eq!(trace.source_sample_count, 1);
        assert_eq!(
            trace.samples,
            vec![
                StepSample {
                    time: 1.5,
                    value: 1.0,
                },
                StepSample {
                    time: 2.0,
                    value: 0.0,
                },
            ]
        );
        assert_eq!(trace.value_range, Some((0.0, 1.0)));
    }

    #[test]
    fn raw_step_trace_respects_sample_limit() {
        let time = [0.0, 1.0, 2.0, 3.0];
        let values = [0.0, 1.0, 0.0, 1.0];

        let trace = build_raw_step_trace(&time, &values, (0.0, 3.0), 2);

        assert!(trace.is_none());
    }

    #[test]
    fn change_point_step_trace_preserves_edges_when_sample_count_is_high() {
        let time = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let values = [0.0, 0.0, 0.0, 5.0, 5.0, 0.0, 0.0];

        let trace = build_change_point_step_trace(&time, &values, (0.0, 6.0), 3)
            .expect("change-point step trace");

        assert_eq!(trace.kind, StepTraceKind::ChangePoints);
        assert_eq!(trace.source_sample_count, 7);
        assert_eq!(
            trace.samples,
            vec![
                StepSample {
                    time: 0.0,
                    value: 0.0,
                },
                StepSample {
                    time: 3.0,
                    value: 5.0,
                },
                StepSample {
                    time: 5.0,
                    value: 0.0,
                },
            ]
        );
        assert_eq!(trace.value_range, Some((0.0, 5.0)));
    }

    #[test]
    fn change_point_step_trace_carries_previous_state_at_range_start() {
        let time = [0.0, 1.0, 2.0, 3.0, 4.0];
        let values = [0.0, 5.0, 5.0, 0.0, 0.0];

        let trace = build_change_point_step_trace(&time, &values, (2.5, 4.0), 3)
            .expect("change-point step trace");

        assert_eq!(
            trace.samples,
            vec![
                StepSample {
                    time: 2.5,
                    value: 5.0,
                },
                StepSample {
                    time: 3.0,
                    value: 0.0,
                },
            ]
        );
        assert_eq!(trace.value_range, Some((0.0, 5.0)));
    }

    #[test]
    fn change_point_step_trace_respects_change_limit() {
        let time = [0.0, 1.0, 2.0, 3.0, 4.0];
        let values = [0.0, 1.0, 0.0, 1.0, 0.0];

        let trace = build_change_point_step_trace(&time, &values, (0.0, 4.0), 2);

        assert!(trace.is_none());
    }
}
