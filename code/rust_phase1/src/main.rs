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

#[derive(Clone, Debug, Eq, PartialEq)]
struct RowChannel {
    channel_path: String,
    color_index: usize,
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
struct VisibleEnvelope {
    channel_name: String,
    channel_path: String,
    sample_count: usize,
    color: egui::Color32,
    envelope: parquet_waveform::MinMaxEnvelope,
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
        self.rows = vec![PlotRow {
            id: 0,
            channels: Vec::new(),
        }];
    }

    fn reset_empty(&mut self) {
        self.selected_channel.clear();
        self.x_range = None;
        self.rows = vec![PlotRow {
            id: 0,
            channels: Vec::new(),
        }];
    }

    fn add_channel_to_first_row(&mut self, channel_path: &str) -> bool {
        if self.rows.is_empty() {
            self.rows.push(PlotRow {
                id: 0,
                channels: Vec::new(),
            });
        }

        let row = &mut self.rows[0];
        if row
            .channels
            .iter()
            .any(|channel| channel.channel_path == channel_path)
        {
            return false;
        }

        row.channels.push(RowChannel {
            channel_path: channel_path.to_owned(),
            color_index: row.channels.len(),
        });
        true
    }

    fn visible_channels(&self) -> Vec<RowChannel> {
        self.rows
            .iter()
            .flat_map(|row| row.channels.iter().cloned())
            .collect()
    }

    fn has_visible_channels(&self) -> bool {
        self.rows.iter().any(|row| !row.channels.is_empty())
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
        let row_added = self.view.add_channel_to_first_row(&channel_path);
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
            "added to row"
        } else {
            "already in row"
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
            "{cache_note}: {channel_name} ({channel_sample_count} samples, {:.1} MiB, read {:.3}s), {row_note}; {time_note} {:.3}s; cache {} ch, total {:.1} MiB",
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

    fn visible_envelopes(
        &mut self,
        requested_bucket_count: usize,
        dark_mode: bool,
    ) -> Vec<VisibleEnvelope> {
        let visible_channels = self.view.visible_channels();
        if visible_channels.is_empty() {
            return Vec::new();
        }

        let Some(shared_time) = self.dataset.shared_time.as_ref() else {
            return Vec::new();
        };
        let Some(full_range) = shared_time.time_range() else {
            self.view.x_range = None;
            self.dataset.loaded_channels.clear_envelope_cache();
            return Vec::new();
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

        let mut envelopes = Vec::with_capacity(visible_channels.len());
        let mut built_count = 0usize;
        {
            let dataset = &mut self.dataset;
            let Some(shared_time) = dataset.shared_time.as_ref() else {
                return envelopes;
            };
            let time_values = &shared_time.time;
            let loaded_channels = &mut dataset.loaded_channels;
            loaded_channels.prepare_envelope_context(view_range, requested_bucket_count);

            for row_channel in visible_channels {
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
                envelopes.push(VisibleEnvelope {
                    channel_name,
                    channel_path,
                    sample_count,
                    color: channel_color(row_channel.color_index, dark_mode),
                    envelope,
                });
            }
        }

        if built_count > 0 {
            let source_sample_count = envelopes
                .first()
                .map(|visible| visible.envelope.source_sample_count)
                .unwrap_or_default();
            self.load.status = format!(
                "View {:.6}..{:.6}s: {} ch, {} visible samples, built {} envelope(s), cache {}",
                view_range.0,
                view_range.1,
                envelopes.len(),
                source_sample_count,
                built_count,
                self.dataset.loaded_channels.envelope_cache.len()
            );
        }

        envelopes
    }

    fn handle_plot_interaction(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        plot_rect: egui::Rect,
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
                ui.heading("Dataset");
                ui.label("Parquet path");
                ui.text_edit_singleline(&mut self.dataset.parquet_path);

                ui.add_space(12.0);
                if ui.button("Load Schema").clicked() {
                    self.load_schema();
                }

                let can_load_waveform = self.dataset.schema.as_ref().is_some_and(|schema| {
                    schema.time_column.is_some() && !schema.channels.is_empty()
                }) && !self.view.selected_channel.is_empty();
                if ui
                    .add_enabled(can_load_waveform, egui::Button::new("Load / Add Channel"))
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
                    self.load.status = format!("Selected channel: {}", self.view.selected_channel);
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

        egui::CentralPanel::default_margins().show_inside(ui, |ui| {
            let available = ui.available_size();
            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let plot_rect = plot_area_rect(rect);
            let requested_buckets = visible_envelope_bucket_count(plot_rect);

            self.handle_plot_interaction(ui, &response, plot_rect);
            let visible_envelopes =
                self.visible_envelopes(requested_buckets, ui.visuals().dark_mode);

            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            draw_plot_frame(&painter, plot_rect, ui.visuals());

            if visible_envelopes.is_empty() {
                draw_plot_placeholder(
                    &painter,
                    plot_rect,
                    self.dataset.schema.as_ref(),
                    &self.view.selected_channel,
                );
            } else {
                draw_waveform_envelopes(&painter, plot_rect, ui.visuals(), &visible_envelopes);
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

    let mut changed = false;
    for row in &mut view.rows {
        ui.label(format!("Row {}", row.id + 1));
        if row.channels.is_empty() {
            ui.label("No channels in row");
            continue;
        }

        let mut remove_channel = None;
        for channel in &row.channels {
            ui.horizontal(|ui| {
                let color = channel_color(channel.color_index, ui.visuals().dark_mode);
                ui.colored_label(color, format!("ch {}", channel.color_index + 1));
                ui.label(channel_display_name(schema, &channel.channel_path));
                if ui.small_button("Remove").clicked() {
                    remove_channel = Some(channel.channel_path.clone());
                }
            });
        }

        if let Some(channel_path) = remove_channel {
            row.channels
                .retain(|channel| channel.channel_path != channel_path);
            changed = true;
        }
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
    let left = if rect.width() > 180.0 { 58.0 } else { 12.0 };
    let bottom = if rect.height() > 120.0 { 36.0 } else { 12.0 };
    let top = 20.0;
    let right = 18.0;

    egui::Rect::from_min_max(
        egui::pos2(rect.left() + left, rect.top() + top),
        egui::pos2(rect.right() - right, rect.bottom() - bottom),
    )
}

fn draw_waveform_envelopes(
    painter: &egui::Painter,
    rect: egui::Rect,
    visuals: &egui::Visuals,
    visible_envelopes: &[VisibleEnvelope],
) {
    let Some(first) = visible_envelopes.first() else {
        draw_status_label(painter, rect, "No channel loaded");
        return;
    };
    let Some((time_min, time_max)) = first.envelope.time_range else {
        draw_status_label(painter, rect, "No time range available");
        return;
    };

    let mut combined_value_range = None;
    for visible in visible_envelopes {
        if let Some((min, max)) = visible.envelope.value_range {
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

    for visible in visible_envelopes {
        let vertical_stroke = egui::Stroke::new(1.0, visible.color.linear_multiply(0.45));
        let line_stroke = egui::Stroke::new(1.25, visible.color);
        let mut upper = Vec::with_capacity(visible.envelope.buckets.len());
        let mut lower = Vec::with_capacity(visible.envelope.buckets.len());
        for bucket in &visible.envelope.buckets {
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

    draw_axis_labels(
        painter,
        rect,
        visible_envelopes,
        (time_min, time_max),
        (value_min, value_max),
        visuals,
    );
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
    visible_envelopes: &[VisibleEnvelope],
    time_range: (f64, f64),
    value_range: (f64, f64),
    visuals: &egui::Visuals,
) {
    let text_color = visuals.text_color();
    let weak_color = visuals.weak_text_color();
    let font = egui::FontId::monospace(12.0);
    let channel_label = visible_envelopes
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
    let channel_label = if visible_envelopes.len() > 3 {
        format!("{channel_label}, ...")
    } else {
        channel_label
    };
    let source_sample_count = visible_envelopes
        .first()
        .map(|visible| visible.envelope.source_sample_count)
        .unwrap_or_default();
    let raw_sample_count = visible_envelopes
        .first()
        .map(|visible| visible.sample_count)
        .unwrap_or_default();
    let bucket_count: usize = visible_envelopes
        .iter()
        .map(|visible| visible.envelope.bucket_count())
        .sum();
    let draw_point_count: usize = visible_envelopes
        .iter()
        .map(|visible| visible.envelope.draw_point_count())
        .sum();

    painter.text(
        rect.left_top() + egui::vec2(0.0, -16.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{}  ch={}  visible_samples={}  raw_samples={}  buckets={}  draw_points={}",
            channel_label,
            visible_envelopes.len(),
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

fn extend_range(range: Option<(f64, f64)>, value: f64) -> Option<(f64, f64)> {
    if !value.is_finite() {
        return range;
    }

    match range {
        Some((min, max)) => Some((min.min(value), max.max(value))),
        None => Some((value, value)),
    }
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
