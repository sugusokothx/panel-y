use eframe::egui;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod parquet_schema;
mod parquet_waveform;

const FULL_RANGE_ENVELOPE_BUCKETS: usize = 4_096;
const MIN_VISIBLE_ENVELOPE_BUCKETS: usize = 128;
const MAX_VISIBLE_ENVELOPE_BUCKETS: usize = 8_192;
const WHEEL_ZOOM_SENSITIVITY: f64 = 0.0015;
const BENCH_VISIBLE_ENVELOPE_BUCKETS: usize = 1_200;
const BENCH_RANGE_RUNS: usize = 24;
const BENCH_HOVER_RUNS: usize = 1_000;
const STRESS_RANGE_RUNS: usize = 1_000;
const STRESS_REPORT_BLOCKS: usize = 5;
const MIN_WAVEFORM_ROW_HEIGHT: f32 = 180.0;
const WAVEFORM_ROW_GAP: f32 = 10.0;
const MAX_EXACT_STEP_SAMPLES: usize = 12_000;
const MAX_STEP_CHANGE_POINTS: usize = 12_000;
const INTERACTION_PREVIEW_SETTLE: Duration = Duration::from_millis(140);
const PREVIEW_MIN_DATASET_SAMPLES: usize = 8_000_000;
const PREVIEW_MIN_VISIBLE_LINE_SAMPLE_WORK: usize = 32_000_000;
const PREVIEW_MIN_VISIBLE_STEP_SAMPLES: usize = 1_000_000;
const PREVIEW_BUCKET_DIVISOR: usize = 4;
const MAX_PREVIEW_ENVELOPE_BUCKETS: usize = 512;
const ENVELOPE_OVERSCAN_RATIO: f64 = 0.5;
const ENVELOPE_CONTEXT_SPAN_TOLERANCE: f64 = 0.05;
const MAX_CACHED_ENVELOPE_BUCKETS: usize = MAX_VISIBLE_ENVELOPE_BUCKETS * 3;
const LINE_TILE_SAMPLE_WIDTH: usize = 256;
const LINE_TILE_MIN_SOURCE_SAMPLES: usize = 500_000;
const LINE_TILE_MIN_BUCKET_SIZE: usize = LINE_TILE_SAMPLE_WIDTH * 2;
const DEFAULT_TRACE_LINE_WIDTH: f32 = 1.25;
const MIN_TRACE_LINE_WIDTH: f32 = 0.5;
const MAX_TRACE_LINE_WIDTH: f32 = 6.0;
const MAX_HOVER_READOUT_CHANNELS: usize = 4;
const MAX_RECENT_PARQUET_PATHS: usize = 8;

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
            CliCommand::BenchPhase2 { path, channels } => benchmark_phase2_view(path, channels),
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
    BenchPhase2 {
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
        Some("--bench-phase2") => {
            let path = args.next()?;
            let channels = args.collect();
            Some(CliCommand::BenchPhase2 { path, channels })
        }
        _ => None,
    }
}

struct PanelYApp {
    dataset: DatasetState,
    view: ViewState,
    load: LoadState,
    interaction: InteractionState,
    perf: PerfStats,
    session_preset: Option<SessionPreset>,
    load_result_tx: mpsc::Sender<LoadJobResult>,
    load_result_rx: mpsc::Receiver<LoadJobResult>,
    next_load_job_id: u64,
}

#[derive(Debug)]
struct DatasetState {
    parquet_path: String,
    recent_parquet_paths: Vec<String>,
    schema: Option<parquet_schema::SchemaSummary>,
    shared_time: Option<parquet_waveform::TimeData>,
    loaded_channels: ChannelStore,
}

#[derive(Debug)]
struct ViewState {
    selected_channel: String,
    channel_filter: String,
    channel_state_filter: ChannelStateFilter,
    channel_sort: ChannelSort,
    selected_row_id: Option<u64>,
    next_row_id: u64,
    x_range: Option<(f64, f64)>,
    hover_x: Option<f64>,
    large_preview_enabled: bool,
    rows: Vec<PlotRow>,
}

#[derive(Debug)]
struct LoadState {
    status: String,
    pending_jobs: usize,
    progress: Option<String>,
    error: Option<String>,
    active_jobs: BTreeMap<u64, ActiveLoadJob>,
}

#[derive(Clone, Debug)]
struct InteractionState {
    last_range_change: Option<Instant>,
}

#[derive(Clone, Debug)]
struct PerfStats {
    show: bool,
    frame_ms: f64,
    interaction_ms: f64,
    visible_ms: f64,
    draw_ms: f64,
    rows: usize,
    channels: usize,
    draw_points: usize,
    requested_buckets: usize,
    effective_buckets: usize,
    preview: bool,
    envelope_cache_hits: usize,
    envelope_cache_misses: usize,
    envelope_context_hits: usize,
    envelope_context_misses: usize,
    line_tile_hits: usize,
    line_tile_builds: usize,
    step_edge_hits: usize,
    step_edge_builds: usize,
}

#[derive(Clone, Debug)]
struct ActiveLoadJob {
    channel_path: String,
    channel_name: String,
    target_row_id: u64,
    target_row_label: String,
    time_was_cached: bool,
    channel_was_cached: bool,
    started: std::time::Instant,
}

#[derive(Clone, Debug)]
struct SessionPreset {
    parquet_path: String,
    selected_channel: String,
    selected_row_index: Option<usize>,
    x_range: Option<(f64, f64)>,
    large_preview_enabled: bool,
    rows: Vec<SessionPresetRow>,
}

#[derive(Clone, Debug, PartialEq)]
struct SessionPresetRow {
    y_range: RowYRange,
    channels: Vec<SessionPresetChannel>,
}

#[derive(Clone, Debug, PartialEq)]
struct SessionPresetChannel {
    channel_path: String,
    color_index: usize,
    draw_mode: DrawMode,
    visible: bool,
    color_override: Option<egui::Color32>,
    line_width: f32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SessionPresetApplyReport {
    restored_rows: usize,
    restored_channels: usize,
    missing_channels: Vec<String>,
}

#[derive(Debug)]
struct LoadJobResult {
    job_id: u64,
    parquet_path: String,
    channel_path: String,
    result: Result<LoadedChannelData, String>,
}

#[derive(Debug)]
struct LoadedChannelData {
    time: Option<parquet_waveform::TimeData>,
    channel: Option<parquet_waveform::ChannelData>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SchemaControlsResponse {
    selection_changed: bool,
    load_channel: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CacheControlsResponse {
    action: Option<CacheControlAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CacheControlAction {
    AddToSelectedRow(String),
    UnloadChannel(String),
    UnloadAllChannels,
    ClearDrawCaches,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParquetPathValidationError {
    Empty,
    Missing { path: String },
    NotAccessible { path: String, error: String },
    NotFile { path: String },
    WrongExtension { path: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChannelLoadState {
    Loaded,
    Loading,
    Unloaded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChannelStateFilter {
    All,
    Loaded,
    Loading,
    Unloaded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChannelSort {
    SchemaOrder,
    Name,
    LoadedFirst,
}

#[derive(Clone, Debug)]
struct PlotRow {
    id: u64,
    y_range: RowYRange,
    channels: Vec<RowChannel>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RowYRange {
    mode: YRangeMode,
    min: f64,
    max: f64,
    last_auto: Option<(f64, f64)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum YRangeMode {
    Auto,
    Manual,
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
    line_tile_cache: BTreeMap<String, parquet_waveform::LineTileCache>,
    step_edge_cache: BTreeMap<String, StepEdgeCache>,
    envelope_cache: BTreeMap<EnvelopeKey, parquet_waveform::MinMaxEnvelope>,
    envelope_context: Option<EnvelopeContext>,
    envelope_plan: Option<parquet_waveform::MinMaxEnvelopePlan>,
    last_envelope_stats: EnvelopeCacheStats,
    step_fallback_hints: BTreeMap<String, StepFallbackHint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StepFallbackHint {
    min_span: f64,
    min_source_sample_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EnvelopeContext {
    cache_range_start_bits: u64,
    cache_range_end_bits: u64,
    view_span_bits: u64,
    requested_view_bucket_count: usize,
    cache_bucket_count: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EnvelopeCacheStats {
    context_hits: usize,
    context_misses: usize,
    hits: usize,
    misses: usize,
    clipped: usize,
    tile_hits: usize,
    tile_builds: usize,
    tile_buckets: usize,
    step_edge_hits: usize,
    step_edge_builds: usize,
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
    hover_value: Option<f32>,
    data: VisibleTraceData,
}

#[derive(Clone, Debug)]
enum VisibleTraceData {
    Envelope(parquet_waveform::MinMaxEnvelope),
    DenseStepEnvelope(parquet_waveform::MinMaxEnvelope),
    RawStep(RawStepTrace),
}

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
enum StepTraceBuildResult {
    Trace(RawStepTrace),
    TooManyChangePoints { source_sample_count: usize },
    Empty,
}

#[derive(Clone, Debug, PartialEq)]
struct StepEdgeCache {
    sample_count: usize,
    edges: Vec<StepEdge>,
    elapsed: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StepEdge {
    index: usize,
    time: f64,
    value: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StepSample {
    time: f64,
    value: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VisibleSampleRange {
    range_start: f64,
    range_end: f64,
    start: usize,
    end: usize,
    context_start: usize,
    source_sample_count: usize,
    draw_sample_count: usize,
}

#[derive(Debug)]
struct VisibleRowTrace {
    row_id: u64,
    row_index: usize,
    row_channel_count: usize,
    loading_channel_count: usize,
    unloaded_channel_count: usize,
    y_range: RowYRange,
    traces: Vec<VisibleTrace>,
}

impl Default for PanelYApp {
    fn default() -> Self {
        let (load_result_tx, load_result_rx) = mpsc::channel();
        Self {
            dataset: DatasetState::default(),
            view: ViewState::default(),
            load: LoadState::default(),
            interaction: InteractionState::default(),
            perf: PerfStats::default(),
            session_preset: None,
            load_result_tx,
            load_result_rx,
            next_load_job_id: 1,
        }
    }
}

impl Default for DatasetState {
    fn default() -> Self {
        let parquet_path = default_parquet_path();
        Self {
            parquet_path: parquet_path.clone(),
            recent_parquet_paths: vec![parquet_path],
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
            channel_filter: String::new(),
            channel_state_filter: ChannelStateFilter::All,
            channel_sort: ChannelSort::SchemaOrder,
            selected_row_id: Some(0),
            next_row_id: 1,
            x_range: None,
            hover_x: None,
            large_preview_enabled: false,
            rows: vec![PlotRow::new(0)],
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
            active_jobs: BTreeMap::new(),
        }
    }
}

impl Default for InteractionState {
    fn default() -> Self {
        Self {
            last_range_change: None,
        }
    }
}

impl Default for PerfStats {
    fn default() -> Self {
        Self {
            show: true,
            frame_ms: 0.0,
            interaction_ms: 0.0,
            visible_ms: 0.0,
            draw_ms: 0.0,
            rows: 0,
            channels: 0,
            draw_points: 0,
            requested_buckets: 0,
            effective_buckets: 0,
            preview: false,
            envelope_cache_hits: 0,
            envelope_cache_misses: 0,
            envelope_context_hits: 0,
            envelope_context_misses: 0,
            line_tile_hits: 0,
            line_tile_builds: 0,
            step_edge_hits: 0,
            step_edge_builds: 0,
        }
    }
}

impl LoadState {
    fn is_busy(&self) -> bool {
        !self.active_jobs.is_empty()
    }

    fn is_channel_loading(&self, channel_path: &str) -> bool {
        self.active_jobs
            .values()
            .any(|job| job.channel_path == channel_path)
    }

    fn loading_channel_paths(&self) -> BTreeSet<String> {
        self.active_jobs
            .values()
            .map(|job| job.channel_path.clone())
            .collect()
    }

    fn refresh_progress(&mut self) {
        self.pending_jobs = self.active_jobs.len();
        self.progress = if self.active_jobs.is_empty() {
            None
        } else {
            let labels = self
                .active_jobs
                .values()
                .map(|job| format!("{} -> {}", job.channel_name, job.target_row_label))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!("loading: {labels}"))
        };
    }
}

impl InteractionState {
    fn mark_range_changed(&mut self) {
        self.last_range_change = Some(Instant::now());
    }

    fn preview_active(&self) -> bool {
        self.last_range_change
            .is_some_and(|changed_at| changed_at.elapsed() < INTERACTION_PREVIEW_SETTLE)
    }
}

impl PerfStats {
    fn update(
        &mut self,
        timing: FrameTiming,
        visible_rows: &[VisibleRowTrace],
        requested_buckets: usize,
        effective_buckets: usize,
        preview: bool,
        envelope_stats: EnvelopeCacheStats,
    ) {
        self.frame_ms = duration_ms(timing.frame);
        self.interaction_ms = duration_ms(timing.interaction);
        self.visible_ms = duration_ms(timing.visible);
        self.draw_ms = duration_ms(timing.draw);
        self.rows = visible_rows.len();
        self.channels = visible_rows
            .iter()
            .map(|row| row.traces.len())
            .sum::<usize>();
        self.draw_points = visible_rows
            .iter()
            .flat_map(|row| row.traces.iter())
            .map(trace_draw_point_count)
            .sum::<usize>();
        self.requested_buckets = requested_buckets;
        self.effective_buckets = effective_buckets;
        self.preview = preview;
        self.envelope_cache_hits = envelope_stats.hits;
        self.envelope_cache_misses = envelope_stats.misses;
        self.envelope_context_hits = envelope_stats.context_hits;
        self.envelope_context_misses = envelope_stats.context_misses;
        self.line_tile_hits = envelope_stats.tile_hits;
        self.line_tile_builds = envelope_stats.tile_builds;
        self.step_edge_hits = envelope_stats.step_edge_hits;
        self.step_edge_builds = envelope_stats.step_edge_builds;
    }

    fn summary(&self) -> String {
        let mode = if self.preview { "preview" } else { "full" };
        format!(
            "{mode} frame {:.1}ms visible {:.1}ms draw {:.1}ms input {:.1}ms rows {} ch {} points {} buckets {}/{} env h/m {}/{} ctx {}/{} tile h/b {}/{} edge h/b {}/{}",
            self.frame_ms,
            self.visible_ms,
            self.draw_ms,
            self.interaction_ms,
            self.rows,
            self.channels,
            self.draw_points,
            self.effective_buckets,
            self.requested_buckets,
            self.envelope_cache_hits,
            self.envelope_cache_misses,
            self.envelope_context_hits,
            self.envelope_context_misses,
            self.line_tile_hits,
            self.line_tile_builds,
            self.step_edge_hits,
            self.step_edge_builds
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct FrameTiming {
    frame: Duration,
    interaction: Duration,
    visible: Duration,
    draw: Duration,
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

impl YRangeMode {
    const ALL: [Self; 2] = [Self::Auto, Self::Manual];

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Manual => "Manual",
        }
    }
}

impl Default for RowYRange {
    fn default() -> Self {
        Self {
            mode: YRangeMode::Auto,
            min: -1.0,
            max: 1.0,
            last_auto: None,
        }
    }
}

impl RowYRange {
    fn set_last_auto(&mut self, range: (f64, f64)) {
        if normalized_y_range(range.0, range.1).is_some() {
            self.last_auto = Some(range);
        }
    }

    fn manual_seed_range(&self) -> (f64, f64) {
        self.last_auto.unwrap_or_else(|| {
            let default = Self::default();
            (default.min, default.max)
        })
    }

    fn set_manual_from_last_auto(&mut self) {
        let (min, max) = self.manual_seed_range();
        self.mode = YRangeMode::Manual;
        self.min = min;
        self.max = max;
    }
}

impl PlotRow {
    fn new(id: u64) -> Self {
        Self {
            id,
            y_range: RowYRange::default(),
            channels: Vec::new(),
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
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proto_3_1b/data/test_100k.parquet")
        .display()
        .to_string()
}

impl ParquetPathValidationError {
    fn status(&self) -> &'static str {
        match self {
            Self::Empty => "Parquet path is empty",
            Self::Missing { .. }
            | Self::NotAccessible { .. }
            | Self::NotFile { .. }
            | Self::WrongExtension { .. } => "Invalid Parquet path",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Empty => "Enter a .parquet file path or choose one with Browse.".to_owned(),
            Self::Missing { path } => format!("File does not exist:\n{path}"),
            Self::NotAccessible { path, error } => {
                format!("Cannot check this path:\n{path}\n\nDetails: {error}")
            }
            Self::NotFile { path } => format!("Path is not a file:\n{path}"),
            Self::WrongExtension { path } => {
                format!("Expected a .parquet file, but got:\n{path}")
            }
        }
    }
}

fn validate_parquet_path(path: &str) -> Result<PathBuf, ParquetPathValidationError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(ParquetPathValidationError::Empty);
    }

    let path = PathBuf::from(trimmed);
    let display_path = path.display().to_string();
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => {
            return Err(ParquetPathValidationError::Missing { path: display_path });
        }
        Err(error) => {
            return Err(ParquetPathValidationError::NotAccessible {
                path: display_path,
                error: error.to_string(),
            });
        }
    }

    if !path.is_file() {
        return Err(ParquetPathValidationError::NotFile { path: display_path });
    }

    let has_parquet_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("parquet"));
    if !has_parquet_extension {
        return Err(ParquetPathValidationError::WrongExtension { path: display_path });
    }

    Ok(path)
}

fn format_schema_load_error(path: &Path, error: &str) -> String {
    format!(
        "Could not load the Parquet schema.\nPath:\n{}\n\nDetails: {error}\n\nThe file exists and has a .parquet extension, but the metadata could not be read.",
        path.display()
    )
}

fn remember_recent_parquet_path(recent_paths: &mut Vec<String>, path: impl Into<String>) {
    let path = path.into();
    let path = path.trim();
    if path.is_empty() {
        return;
    }

    let path = path.to_owned();
    recent_paths.retain(|current| current != &path);
    recent_paths.insert(0, path);
    recent_paths.truncate(MAX_RECENT_PARQUET_PATHS);
}

fn recent_path_label(path: &str) -> String {
    let path_ref = Path::new(path);
    let file_name = path_ref
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    let parent_name = path_ref
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str());

    match parent_name {
        Some(parent_name) if !parent_name.is_empty() => format!("{file_name} ({parent_name})"),
        _ => file_name.to_owned(),
    }
}

fn parquet_dialog_directory(path: &str) -> Option<PathBuf> {
    let path = Path::new(path.trim());
    if path.is_dir() {
        return Some(path.to_path_buf());
    }

    path.parent()
        .filter(|parent| parent.exists())
        .map(Path::to_path_buf)
}

impl ChannelLoadState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Loading => "loading",
            Self::Unloaded => "unloaded",
        }
    }

    fn action_label(self) -> &'static str {
        match self {
            Self::Loaded => "Add",
            Self::Loading => "Loading",
            Self::Unloaded => "Load",
        }
    }

    fn can_load_or_add(self) -> bool {
        !matches!(self, Self::Loading)
    }
}

impl ChannelStateFilter {
    const ALL: [Self; 4] = [Self::All, Self::Loaded, Self::Loading, Self::Unloaded];

    fn as_str(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Loaded => "Loaded",
            Self::Loading => "Loading",
            Self::Unloaded => "Unloaded",
        }
    }

    fn matches(self, load_state: ChannelLoadState) -> bool {
        match self {
            Self::All => true,
            Self::Loaded => load_state == ChannelLoadState::Loaded,
            Self::Loading => load_state == ChannelLoadState::Loading,
            Self::Unloaded => load_state == ChannelLoadState::Unloaded,
        }
    }
}

impl ChannelSort {
    const ALL: [Self; 3] = [Self::SchemaOrder, Self::Name, Self::LoadedFirst];

    fn as_str(self) -> &'static str {
        match self {
            Self::SchemaOrder => "Schema order",
            Self::Name => "Name",
            Self::LoadedFirst => "Loaded first",
        }
    }
}

fn channel_load_state(
    channel_path: &str,
    loaded_channels: &ChannelStore,
    loading_channel_paths: &BTreeSet<String>,
) -> ChannelLoadState {
    if loaded_channels.has_channel(channel_path) {
        ChannelLoadState::Loaded
    } else if loading_channel_paths.contains(channel_path) {
        ChannelLoadState::Loading
    } else {
        ChannelLoadState::Unloaded
    }
}

fn channel_list_entries<'a>(
    channels: &'a [parquet_schema::ColumnInfo],
    text_filter: &str,
    state_filter: ChannelStateFilter,
    sort: ChannelSort,
    loaded_channels: &ChannelStore,
    loading_channel_paths: &BTreeSet<String>,
) -> Vec<(&'a parquet_schema::ColumnInfo, ChannelLoadState)> {
    let mut entries = channels
        .iter()
        .filter_map(|channel| {
            let load_state =
                channel_load_state(&channel.path, loaded_channels, loading_channel_paths);
            (channel_matches_filter(channel, text_filter) && state_filter.matches(load_state))
                .then_some((channel, load_state))
        })
        .collect::<Vec<_>>();

    match sort {
        ChannelSort::SchemaOrder => {}
        ChannelSort::Name => {
            entries.sort_by(|(left, _), (right, _)| {
                left.display_name()
                    .to_ascii_lowercase()
                    .cmp(&right.display_name().to_ascii_lowercase())
                    .then(left.index.cmp(&right.index))
            });
        }
        ChannelSort::LoadedFirst => {
            entries.sort_by(|(left, left_state), (right, right_state)| {
                channel_loaded_first_rank(*left_state)
                    .cmp(&channel_loaded_first_rank(*right_state))
                    .then(left.index.cmp(&right.index))
            });
        }
    }

    entries
}

fn channel_loaded_first_rank(load_state: ChannelLoadState) -> usize {
    match load_state {
        ChannelLoadState::Loaded => 0,
        ChannelLoadState::Loading => 1,
        ChannelLoadState::Unloaded => 2,
    }
}

fn channel_matches_filter(channel: &parquet_schema::ColumnInfo, filter: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }

    let filter = filter.to_ascii_lowercase();
    channel.name.to_ascii_lowercase().contains(&filter)
        || channel.path.to_ascii_lowercase().contains(&filter)
        || channel
            .display_name()
            .to_ascii_lowercase()
            .contains(&filter)
}

fn estimated_sample_rate_hz(sample_count: usize, time_range: Option<(f64, f64)>) -> Option<f64> {
    let (start, end) = time_range?;
    let span = (end - start).abs();
    if sample_count < 2 || !span.is_finite() || span <= 0.0 {
        return None;
    }

    Some((sample_count - 1) as f64 / span)
}

fn format_frequency_hz(frequency_hz: f64) -> String {
    let frequency_hz = frequency_hz.abs();
    if !frequency_hz.is_finite() {
        return "-".to_owned();
    }

    if frequency_hz >= 1_000_000.0 {
        format!("{:.3} MHz", frequency_hz / 1_000_000.0)
    } else if frequency_hz >= 1_000.0 {
        format!("{:.3} kHz", frequency_hz / 1_000.0)
    } else {
        format!("{frequency_hz:.3} Hz")
    }
}

impl ViewState {
    fn reset_for_schema(&mut self, schema: &parquet_schema::SchemaSummary) {
        self.channel_filter.clear();
        self.channel_state_filter = ChannelStateFilter::All;
        self.channel_sort = ChannelSort::SchemaOrder;
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
        self.hover_x = None;
        self.selected_row_id = Some(0);
        self.next_row_id = 1;
        self.rows = vec![PlotRow::new(0)];
    }

    fn reset_empty(&mut self) {
        self.selected_channel.clear();
        self.channel_filter.clear();
        self.channel_state_filter = ChannelStateFilter::All;
        self.channel_sort = ChannelSort::SchemaOrder;
        self.x_range = None;
        self.hover_x = None;
        self.selected_row_id = Some(0);
        self.next_row_id = 1;
        self.rows = vec![PlotRow::new(0)];
    }

    fn ensure_row_state(&mut self) {
        if self.rows.is_empty() {
            self.rows.push(PlotRow::new(0));
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
        self.rows.push(PlotRow::new(id));
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

    fn row_display_name(&self, row_id: u64) -> String {
        self.rows
            .iter()
            .position(|row| row.id == row_id)
            .map(|index| format!("Row {}", index + 1))
            .unwrap_or_else(|| "Row -".to_owned())
    }

    fn selected_row_id_or_first(&mut self) -> Option<u64> {
        self.ensure_row_state();
        self.selected_row_id
            .or_else(|| self.rows.first().map(|row| row.id))
    }

    fn add_channel_to_row(&mut self, row_id: u64, channel_path: &str) -> Option<(bool, u64)> {
        self.ensure_row_state();
        let row = self.rows.iter_mut().find(|row| row.id == row_id)?;
        if row
            .channels
            .iter()
            .any(|channel| channel.channel_path == channel_path)
        {
            return Some((false, row.id));
        }

        row.channels
            .push(RowChannel::new(channel_path, row.channels.len()));
        Some((true, row.id))
    }

    #[cfg(test)]
    fn add_channel_to_selected_row(&mut self, channel_path: &str) -> (bool, u64) {
        let row_id = self.selected_row_id_or_first().unwrap_or(0);
        self.add_channel_to_row(row_id, channel_path)
            .unwrap_or((false, row_id))
    }

    fn has_visible_channels(&self) -> bool {
        self.rows
            .iter()
            .any(|row| row.channels.iter().any(|channel| channel.visible))
    }
}

impl SessionPreset {
    fn capture(parquet_path: &str, view: &ViewState) -> Self {
        Self {
            parquet_path: parquet_path.trim().to_owned(),
            selected_channel: view.selected_channel.clone(),
            selected_row_index: view
                .selected_row_id
                .and_then(|row_id| view.rows.iter().position(|row| row.id == row_id)),
            x_range: view.x_range,
            large_preview_enabled: view.large_preview_enabled,
            rows: view
                .rows
                .iter()
                .map(|row| SessionPresetRow {
                    y_range: row.y_range,
                    channels: row
                        .channels
                        .iter()
                        .map(SessionPresetChannel::from)
                        .collect(),
                })
                .collect(),
        }
    }

    fn channel_count(&self) -> usize {
        self.rows.iter().map(|row| row.channels.len()).sum()
    }

    fn summary(&self) -> String {
        format!(
            "{}: {} row(s), {} channel(s)",
            recent_path_label(&self.parquet_path),
            self.rows.len().max(1),
            self.channel_count()
        )
    }

    fn apply_to_view(
        &self,
        view: &mut ViewState,
        schema: &parquet_schema::SchemaSummary,
    ) -> SessionPresetApplyReport {
        let valid_channels = schema
            .channels
            .iter()
            .map(|channel| channel.path.as_str())
            .collect::<BTreeSet<_>>();
        let mut missing_channels = BTreeSet::new();
        let mut restored_channels = 0usize;

        let mut rows = self
            .rows
            .iter()
            .enumerate()
            .map(|(row_index, preset_row)| {
                let channels = preset_row
                    .channels
                    .iter()
                    .filter_map(|channel| {
                        if valid_channels.contains(channel.channel_path.as_str()) {
                            restored_channels += 1;
                            Some(RowChannel::from(channel))
                        } else {
                            missing_channels.insert(channel.channel_path.clone());
                            None
                        }
                    })
                    .collect();

                PlotRow {
                    id: row_index as u64,
                    y_range: preset_row.y_range,
                    channels,
                }
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            rows.push(PlotRow::new(0));
        }

        view.rows = rows;
        view.next_row_id = view.rows.len() as u64;
        view.selected_row_id = self
            .selected_row_index
            .and_then(|row_index| view.rows.get(row_index).map(|row| row.id))
            .or_else(|| view.rows.first().map(|row| row.id));
        view.x_range = self.x_range;
        view.hover_x = None;
        view.large_preview_enabled = self.large_preview_enabled;
        view.selected_channel = if valid_channels.contains(self.selected_channel.as_str()) {
            self.selected_channel.clone()
        } else {
            view.rows
                .iter()
                .flat_map(|row| row.channels.iter())
                .map(|channel| channel.channel_path.clone())
                .next()
                .or_else(|| schema.channels.first().map(|channel| channel.path.clone()))
                .unwrap_or_default()
        };
        view.ensure_row_state();

        SessionPresetApplyReport {
            restored_rows: view.rows.len(),
            restored_channels,
            missing_channels: missing_channels.into_iter().collect(),
        }
    }
}

impl From<&RowChannel> for SessionPresetChannel {
    fn from(channel: &RowChannel) -> Self {
        Self {
            channel_path: channel.channel_path.clone(),
            color_index: channel.color_index,
            draw_mode: channel.draw_mode,
            visible: channel.visible,
            color_override: channel.color_override,
            line_width: channel.line_width,
        }
    }
}

impl From<&SessionPresetChannel> for RowChannel {
    fn from(channel: &SessionPresetChannel) -> Self {
        Self {
            channel_path: channel.channel_path.clone(),
            color_index: channel.color_index,
            draw_mode: channel.draw_mode,
            visible: channel.visible,
            color_override: channel.color_override,
            line_width: channel
                .line_width
                .clamp(MIN_TRACE_LINE_WIDTH, MAX_TRACE_LINE_WIDTH),
        }
    }
}

fn row_missing_channel_counts(
    row: &PlotRow,
    loaded_channels: Option<&ChannelStore>,
    loading_channel_paths: &BTreeSet<String>,
) -> (usize, usize) {
    row.channels.iter().filter(|channel| channel.visible).fold(
        (0usize, 0usize),
        |(loading, unloaded), channel| {
            if loaded_channels.is_some_and(|store| store.has_channel(&channel.channel_path)) {
                (loading, unloaded)
            } else if loading_channel_paths.contains(&channel.channel_path) {
                (loading + 1, unloaded)
            } else {
                (loading, unloaded + 1)
            }
        },
    )
}

impl ChannelStore {
    fn clear_all(&mut self) {
        self.raw_by_channel.clear();
        self.line_tile_cache.clear();
        self.step_edge_cache.clear();
        self.clear_envelope_cache();
        self.step_fallback_hints.clear();
    }

    fn unload_channel(&mut self, channel_path: &str) -> bool {
        let removed_raw = self.raw_by_channel.remove(channel_path).is_some();
        let removed_line_tiles = self.line_tile_cache.remove(channel_path).is_some();
        let removed_step_edges = self.step_edge_cache.remove(channel_path).is_some();
        let removed_step_hint = self.step_fallback_hints.remove(channel_path).is_some();
        if removed_raw || removed_line_tiles || removed_step_edges || removed_step_hint {
            self.clear_envelope_cache();
        }

        removed_raw
    }

    fn clear_draw_caches(&mut self) -> bool {
        let had_draw_cache = self.has_draw_caches();
        self.line_tile_cache.clear();
        self.step_edge_cache.clear();
        self.step_fallback_hints.clear();
        self.clear_envelope_cache();
        self.last_envelope_stats = EnvelopeCacheStats::default();
        had_draw_cache
    }

    fn clear_envelope_cache(&mut self) {
        self.envelope_cache.clear();
        self.envelope_context = None;
        self.envelope_plan = None;
    }

    fn has_draw_caches(&self) -> bool {
        !self.envelope_cache.is_empty()
            || self.envelope_context.is_some()
            || self.envelope_plan.is_some()
            || !self.line_tile_cache.is_empty()
            || !self.step_edge_cache.is_empty()
            || !self.step_fallback_hints.is_empty()
    }

    fn has_channel(&self, channel_path: &str) -> bool {
        self.raw_by_channel.contains_key(channel_path)
    }

    fn channel(&self, channel_path: &str) -> Option<&parquet_waveform::ChannelData> {
        self.raw_by_channel.get(channel_path)
    }

    fn insert_channel(&mut self, channel: parquet_waveform::ChannelData) {
        self.step_fallback_hints.remove(&channel.channel_path);
        self.line_tile_cache.remove(&channel.channel_path);
        self.step_edge_cache.remove(&channel.channel_path);
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

    fn line_tile_memory_bytes(&self) -> usize {
        self.line_tile_cache
            .values()
            .map(parquet_waveform::LineTileCache::memory_bytes)
            .sum()
    }

    fn line_tile_build_seconds(&self) -> f64 {
        self.line_tile_cache
            .values()
            .map(|cache| cache.elapsed.as_secs_f64())
            .sum()
    }

    fn step_edge_memory_bytes(&self) -> usize {
        self.step_edge_cache
            .values()
            .map(StepEdgeCache::memory_bytes)
            .sum()
    }

    fn step_edge_build_seconds(&self) -> f64 {
        self.step_edge_cache
            .values()
            .map(|cache| cache.elapsed.as_secs_f64())
            .sum()
    }

    fn begin_envelope_frame(&mut self) {
        self.last_envelope_stats = EnvelopeCacheStats::default();
    }

    fn ensure_line_tile_cache(&mut self, channel_path: &str) -> Option<bool> {
        let was_cached = self.line_tile_cache.contains_key(channel_path);
        if !was_cached {
            let channel = self.raw_by_channel.get(channel_path)?;
            let tile_cache =
                parquet_waveform::build_line_tile_cache(&channel.values, LINE_TILE_SAMPLE_WIDTH);
            self.line_tile_cache
                .insert(channel_path.to_owned(), tile_cache);
            self.last_envelope_stats.tile_builds += 1;
        } else {
            self.last_envelope_stats.tile_hits += 1;
        }

        Some(!was_cached)
    }

    fn ensure_step_edge_cache(&mut self, channel_path: &str, time: &[f64]) -> Option<bool> {
        let was_cached = self.step_edge_cache.contains_key(channel_path);
        if !was_cached {
            let channel = self.raw_by_channel.get(channel_path)?;
            let edge_cache = build_step_edge_cache(time, &channel.values);
            self.step_edge_cache
                .insert(channel_path.to_owned(), edge_cache);
            self.last_envelope_stats.step_edge_builds += 1;
        } else {
            self.last_envelope_stats.step_edge_hits += 1;
        }

        Some(!was_cached)
    }

    fn cached_change_point_step_trace_result(
        &mut self,
        channel_path: &str,
        time: &[f64],
        time_range: (f64, f64),
        max_change_points: usize,
    ) -> Option<StepTraceBuildResult> {
        self.ensure_step_edge_cache(channel_path, time)?;
        self.step_edge_cache.get(channel_path).map(|cache| {
            build_change_point_step_trace_from_cache(time, cache, time_range, max_change_points)
        })
    }

    fn prepare_envelope_context(
        &mut self,
        time: &[f64],
        value_count: usize,
        view_range: (f64, f64),
        full_range: (f64, f64),
        requested_bucket_count: usize,
    ) {
        let Some(context) =
            EnvelopeContext::for_view(view_range, full_range, requested_bucket_count)
        else {
            self.clear_envelope_cache();
            self.last_envelope_stats.context_misses += 1;
            return;
        };

        if self
            .envelope_context
            .is_some_and(|current| current.reuses_for_view(view_range, requested_bucket_count))
        {
            self.last_envelope_stats.context_hits += 1;
            return;
        }

        self.envelope_cache.clear();
        self.envelope_plan = Some(parquet_waveform::min_max_envelope_plan_for_range(
            time,
            value_count,
            context.cache_range(),
            context.cache_bucket_count,
        ));
        self.envelope_context = Some(context);
        self.last_envelope_stats.context_misses += 1;
    }

    fn ensure_envelope(
        &mut self,
        channel_path: &str,
        time: &[f64],
        view_range: (f64, f64),
        requested_bucket_count: usize,
        allow_line_tile_lod: bool,
    ) -> Option<(parquet_waveform::MinMaxEnvelope, bool)> {
        let context = self.envelope_context?;
        let cache_range = context.cache_range();
        let key = EnvelopeKey::new(channel_path, cache_range, context.cache_bucket_count);
        let was_cached = self.envelope_cache.contains_key(&key);
        if !was_cached {
            let envelope = {
                if allow_line_tile_lod
                    && self
                        .envelope_plan
                        .as_ref()
                        .is_some_and(should_use_line_tile_lod)
                {
                    let plan = self.envelope_plan.clone()?;
                    let _tile_was_built = self.ensure_line_tile_cache(channel_path)?;
                    let channel = self.raw_by_channel.get(channel_path)?;
                    let tile_cache = self.line_tile_cache.get(channel_path)?;
                    let envelope = parquet_waveform::min_max_envelope_for_plan_with_tiles(
                        time,
                        &channel.values,
                        &plan,
                        tile_cache,
                    );
                    self.last_envelope_stats.tile_buckets += envelope.bucket_count();
                    envelope
                } else if let Some(plan) = self.envelope_plan.as_ref() {
                    let channel = self.raw_by_channel.get(channel_path)?;
                    parquet_waveform::min_max_envelope_for_plan(&channel.values, plan)
                } else {
                    let channel = self.raw_by_channel.get(channel_path)?;
                    channel.min_max_envelope_for_range(
                        time,
                        cache_range,
                        context.cache_bucket_count,
                    )
                }
            };
            self.envelope_cache.insert(key.clone(), envelope);
            self.last_envelope_stats.misses += 1;
        } else {
            self.last_envelope_stats.hits += 1;
        }

        let cached = self.envelope_cache.get(&key)?.clone();
        if context.cache_range_equals(view_range)
            && context.cache_bucket_count == requested_bucket_count
        {
            return Some((cached, !was_cached));
        }

        let channel = self.raw_by_channel.get(channel_path)?;
        self.last_envelope_stats.clipped += 1;
        Some((
            parquet_waveform::clip_min_max_envelope_to_range(
                &cached,
                time,
                &channel.values,
                view_range,
                requested_bucket_count,
            ),
            !was_cached,
        ))
    }

    fn should_skip_step_change_points(
        &self,
        channel_path: &str,
        time_range: (f64, f64),
        source_sample_count: usize,
    ) -> bool {
        let Some(hint) = self.step_fallback_hints.get(channel_path) else {
            return false;
        };
        let span = (time_range.1 - time_range.0).abs();
        span.is_finite()
            && span >= hint.min_span * 0.95
            && source_sample_count >= hint.min_source_sample_count.saturating_mul(9) / 10
    }

    fn record_step_change_point_fallback(
        &mut self,
        channel_path: &str,
        time_range: (f64, f64),
        source_sample_count: usize,
    ) {
        let span = (time_range.1 - time_range.0).abs();
        if !span.is_finite() || span <= 0.0 || source_sample_count == 0 {
            return;
        }

        self.step_fallback_hints
            .entry(channel_path.to_owned())
            .and_modify(|hint| {
                hint.min_span = hint.min_span.min(span);
                hint.min_source_sample_count =
                    hint.min_source_sample_count.min(source_sample_count);
            })
            .or_insert(StepFallbackHint {
                min_span: span,
                min_source_sample_count: source_sample_count,
            });
    }
}

impl EnvelopeContext {
    fn for_view(
        view_range: (f64, f64),
        full_range: (f64, f64),
        requested_bucket_count: usize,
    ) -> Option<Self> {
        let (view_start, view_end) = normalized_range(view_range)?;
        let (cache_start, cache_end) = overscan_cache_range((view_start, view_end), full_range)?;
        let view_span = view_end - view_start;
        if !view_span.is_finite() || view_span <= 0.0 {
            return None;
        }

        Some(Self {
            cache_range_start_bits: cache_start.to_bits(),
            cache_range_end_bits: cache_end.to_bits(),
            view_span_bits: view_span.to_bits(),
            requested_view_bucket_count: requested_bucket_count,
            cache_bucket_count: cache_bucket_count_for_view(
                (view_start, view_end),
                (cache_start, cache_end),
                requested_bucket_count,
            ),
        })
    }

    fn cache_range(self) -> (f64, f64) {
        (
            f64::from_bits(self.cache_range_start_bits),
            f64::from_bits(self.cache_range_end_bits),
        )
    }

    fn view_span(self) -> f64 {
        f64::from_bits(self.view_span_bits)
    }

    fn reuses_for_view(self, view_range: (f64, f64), requested_bucket_count: usize) -> bool {
        if self.requested_view_bucket_count != requested_bucket_count {
            return false;
        }

        let Some((view_start, view_end)) = normalized_range(view_range) else {
            return false;
        };
        let view_span = view_end - view_start;
        if !similar_span(self.view_span(), view_span, ENVELOPE_CONTEXT_SPAN_TOLERANCE) {
            return false;
        }

        let (cache_start, cache_end) = self.cache_range();
        view_start >= cache_start && view_end <= cache_end
    }

    fn cache_range_equals(self, view_range: (f64, f64)) -> bool {
        let Some((view_start, view_end)) = normalized_range(view_range) else {
            return false;
        };
        self.cache_range_start_bits == view_start.to_bits()
            && self.cache_range_end_bits == view_end.to_bits()
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

impl StepEdgeCache {
    fn edge_count(&self) -> usize {
        self.edges.len()
    }

    fn memory_bytes(&self) -> usize {
        self.edges.len() * std::mem::size_of::<StepEdge>()
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

    store.begin_envelope_frame();
    store.prepare_envelope_context(
        &time.time,
        time.sample_count(),
        full_range,
        full_range,
        BENCH_VISIBLE_ENVELOPE_BUCKETS,
    );
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
            true,
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

fn benchmark_phase2_view(path: String, channels: Vec<String>) -> Result<String, String> {
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
    let rss_after_all_channels = process_rss_mib();

    let rows = phase2_benchmark_rows(&selected_channels);
    let (line_channel_count, step_channel_count) = phase2_benchmark_mode_counts(&rows);
    let mut app = PanelYApp::default();
    app.dataset.parquet_path = path;
    app.dataset.schema = Some(schema);
    app.dataset.shared_time = Some(time);
    app.dataset.loaded_channels = store;
    app.view.rows = rows;
    app.view.selected_row_id = app.view.rows.first().map(|row| row.id);
    app.view.next_row_id = app.view.rows.iter().map(|row| row.id).max().unwrap_or(0) + 1;
    app.view.x_range = Some(full_range);

    let ranges = benchmark_ranges(full_range, BENCH_RANGE_RUNS);
    let mut visible_results = Vec::with_capacity(ranges.len());
    for (index, range) in ranges.into_iter().enumerate() {
        app.view.x_range = Some(range);
        app.view.hover_x = Some((range.0 + range.1) * 0.5);
        let started = std::time::Instant::now();
        let visible_rows = app.visible_row_traces(BENCH_VISIBLE_ENVELOPE_BUCKETS, true, false);
        let elapsed_sec = started.elapsed().as_secs_f64();
        let envelope_stats = app.dataset.loaded_channels.last_envelope_stats;

        let mut visible_channel_count = 0usize;
        let mut envelope_trace_count = 0usize;
        let mut dense_step_trace_count = 0usize;
        let mut raw_step_trace_count = 0usize;
        let mut edge_step_trace_count = 0usize;
        let mut raw_step_sample_count = 0usize;
        let mut draw_point_count = 0usize;
        let mut source_sample_count = 0usize;
        let mut hover_value_count = 0usize;

        for row in &visible_rows {
            for trace in &row.traces {
                visible_channel_count += 1;
                draw_point_count += trace_draw_point_count(trace);
                source_sample_count = source_sample_count.max(trace_source_sample_count(trace));
                if trace.hover_value.is_some() {
                    hover_value_count += 1;
                }

                match &trace.data {
                    VisibleTraceData::Envelope(_) => {
                        envelope_trace_count += 1;
                    }
                    VisibleTraceData::DenseStepEnvelope(_) => {
                        dense_step_trace_count += 1;
                    }
                    VisibleTraceData::RawStep(raw_step) => {
                        raw_step_trace_count += 1;
                        raw_step_sample_count += raw_step.samples.len();
                        if raw_step.kind == StepTraceKind::ChangePoints {
                            edge_step_trace_count += 1;
                        }
                    }
                }
            }
        }

        visible_results.push(Phase2VisibleRunResult {
            index: index + 1,
            range,
            elapsed_sec,
            row_count: visible_rows.len(),
            visible_channel_count,
            envelope_trace_count,
            dense_step_trace_count,
            raw_step_trace_count,
            edge_step_trace_count,
            raw_step_sample_count,
            draw_point_count,
            source_sample_count,
            hover_value_count,
            envelope_stats,
        });
    }
    let pan_cache_result = benchmark_phase2_pan_cache(&mut app, full_range);
    let rss_after_visible_benchmark = process_rss_mib();

    let hover_result = benchmark_phase2_hover(&app, full_range);
    let rss_after_hover_benchmark = process_rss_mib();

    let raw_memory_bytes = app
        .dataset
        .shared_time
        .as_ref()
        .map_or(0, parquet_waveform::TimeData::memory_bytes)
        + app.dataset.loaded_channels.raw_memory_bytes();
    let channel_memory_bytes = channel_results
        .iter()
        .map(|result| result.memory_bytes)
        .sum::<usize>();
    let visible_total_sec = visible_results
        .iter()
        .map(|result| result.elapsed_sec)
        .sum::<f64>();
    let visible_avg_sec = if visible_results.is_empty() {
        0.0
    } else {
        visible_total_sec / visible_results.len() as f64
    };
    let visible_max_sec = visible_results
        .iter()
        .map(|result| result.elapsed_sec)
        .fold(0.0, f64::max);
    let max_draw_points = visible_results
        .iter()
        .map(|result| result.draw_point_count)
        .max()
        .unwrap_or(0);
    let max_raw_step_samples = visible_results
        .iter()
        .map(|result| result.raw_step_sample_count)
        .max()
        .unwrap_or(0);
    let total_envelope_hits = visible_results
        .iter()
        .map(|result| result.envelope_stats.hits)
        .sum::<usize>();
    let total_envelope_misses = visible_results
        .iter()
        .map(|result| result.envelope_stats.misses)
        .sum::<usize>();
    let total_context_hits = visible_results
        .iter()
        .map(|result| result.envelope_stats.context_hits)
        .sum::<usize>();
    let total_context_misses = visible_results
        .iter()
        .map(|result| result.envelope_stats.context_misses)
        .sum::<usize>();
    let total_tile_hits = visible_results
        .iter()
        .map(|result| result.envelope_stats.tile_hits)
        .sum::<usize>();
    let total_tile_builds = visible_results
        .iter()
        .map(|result| result.envelope_stats.tile_builds)
        .sum::<usize>();
    let total_step_edge_hits = visible_results
        .iter()
        .map(|result| result.envelope_stats.step_edge_hits)
        .sum::<usize>();
    let total_step_edge_builds = visible_results
        .iter()
        .map(|result| result.envelope_stats.step_edge_builds)
        .sum::<usize>();
    let step_edge_count = app
        .dataset
        .loaded_channels
        .step_edge_cache
        .values()
        .map(StepEdgeCache::edge_count)
        .sum::<usize>();

    let schema = app.dataset.schema.as_ref().expect("schema is set");
    let shared_time = app.dataset.shared_time.as_ref().expect("time is set");

    let mut report = String::new();
    report.push_str(&format!("file: {}\n", schema.path.display()));
    report.push_str(&format!("rows: {}\n", schema.row_count));
    report.push_str(&format!("channels selected: {}\n", selected_channels.len()));
    report.push_str(&format!("channel list: {}\n", selected_channels.join(", ")));
    report.push_str(&format!(
        "layout: {} rows, line channels {}, step channels {}\n",
        app.view.rows.len(),
        line_channel_count,
        step_channel_count
    ));
    report.push_str(&format!(
        "time: {} samples, {:.1} MiB, read {:.3}s\n",
        shared_time.sample_count(),
        bytes_to_mib(shared_time.memory_bytes()),
        shared_time.elapsed.as_secs_f64()
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
        "visible trace benchmark: {} ranges, {} buckets requested, total {:.3}s, avg {:.4}s, max {:.4}s, max draw points {}, max raw step samples {}\n",
        visible_results.len(),
        BENCH_VISIBLE_ENVELOPE_BUCKETS,
        visible_total_sec,
        visible_avg_sec,
        visible_max_sec,
        max_draw_points,
        max_raw_step_samples
    ));
    report.push_str(&format!(
        "envelope cache: hits {}, misses {}, context hits {}, context misses {}, line tile hits {}, builds {}, tile memory {:.1} MiB, step edge hits {}, builds {}, edges {}, edge memory {:.1} MiB\n",
        total_envelope_hits,
        total_envelope_misses,
        total_context_hits,
        total_context_misses,
        total_tile_hits,
        total_tile_builds,
        bytes_to_mib(app.dataset.loaded_channels.line_tile_memory_bytes()),
        total_step_edge_hits,
        total_step_edge_builds,
        step_edge_count,
        bytes_to_mib(app.dataset.loaded_channels.step_edge_memory_bytes())
    ));
    report.push_str(&format!(
        "pan cache benchmark: {} ranges, total {:.3}s, avg {:.4}s, max {:.4}s, cache hits {}, misses {}, context hits {}, context misses {}, tile hits {}, builds {}, step edge hits {}, builds {}\n",
        pan_cache_result.ranges,
        pan_cache_result.elapsed_sec,
        pan_cache_result.avg_visible_sec(),
        pan_cache_result.max_elapsed_sec,
        pan_cache_result.envelope_hits,
        pan_cache_result.envelope_misses,
        pan_cache_result.context_hits,
        pan_cache_result.context_misses,
        pan_cache_result.tile_hits,
        pan_cache_result.tile_builds,
        pan_cache_result.step_edge_hits,
        pan_cache_result.step_edge_builds
    ));
    report.push_str(&format!(
        "hover benchmark: {} positions, {} lookups, {} hits, total {:.4}s, avg {:.3} us/lookup\n",
        hover_result.positions,
        hover_result.lookups,
        hover_result.hits,
        hover_result.elapsed_sec,
        hover_result.avg_lookup_us()
    ));
    report.push_str(&format!(
        "rss: before_schema={}, after_schema={}, after_time={}, after_all_channels={}, after_visible_benchmark={}, after_hover_benchmark={}\n",
        format_optional_mib(rss_before_schema),
        format_optional_mib(rss_after_schema),
        format_optional_mib(rss_after_time),
        format_optional_mib(rss_after_all_channels),
        format_optional_mib(rss_after_visible_benchmark),
        format_optional_mib(rss_after_hover_benchmark)
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
    report.push_str("visible runs:\n");
    for result in &visible_results {
        report.push_str(&format!(
            "  #{:02} {:.6}..{:.6}s {:.4}s rows={} ch={} samples={} env={} dense={} step={} edge={} step_samples={} draw_points={} hover_values={} cache_h/m={}/{} ctx={}/{} tile_h/b={}/{} edge_h/b={}/{}\n",
            result.index,
            result.range.0,
            result.range.1,
            result.elapsed_sec,
            result.row_count,
            result.visible_channel_count,
            result.source_sample_count,
            result.envelope_trace_count,
            result.dense_step_trace_count,
            result.raw_step_trace_count,
            result.edge_step_trace_count,
            result.raw_step_sample_count,
            result.draw_point_count,
            result.hover_value_count,
            result.envelope_stats.hits,
            result.envelope_stats.misses,
            result.envelope_stats.context_hits,
            result.envelope_stats.context_misses,
            result.envelope_stats.tile_hits,
            result.envelope_stats.tile_builds,
            result.envelope_stats.step_edge_hits,
            result.envelope_stats.step_edge_builds
        ));
    }

    Ok(report)
}

fn phase2_benchmark_rows(selected_channels: &[String]) -> Vec<PlotRow> {
    if selected_channels.is_empty() {
        return vec![PlotRow::new(0)];
    }

    selected_channels
        .chunks(3)
        .enumerate()
        .map(|(row_index, channels)| {
            let mut row = PlotRow::new(row_index as u64);
            row.channels = channels
                .iter()
                .enumerate()
                .map(|(channel_index, channel_path)| {
                    let mut row_channel = RowChannel::new(channel_path, channel_index);
                    row_channel.draw_mode = phase2_benchmark_draw_mode(channel_path);
                    row_channel
                })
                .collect();
            row
        })
        .collect()
}

fn phase2_benchmark_draw_mode(channel_path: &str) -> DrawMode {
    let lower = channel_path.to_ascii_lowercase();
    if lower.contains("pwm") || lower.contains("gate") || lower.contains("step") {
        DrawMode::Step
    } else {
        DrawMode::Line
    }
}

fn phase2_benchmark_mode_counts(rows: &[PlotRow]) -> (usize, usize) {
    rows.iter().flat_map(|row| row.channels.iter()).fold(
        (0usize, 0usize),
        |(line, step), channel| match channel.draw_mode {
            DrawMode::Line => (line + 1, step),
            DrawMode::Step => (line, step + 1),
        },
    )
}

fn benchmark_phase2_pan_cache(
    app: &mut PanelYApp,
    full_range: (f64, f64),
) -> Phase2PanCacheBenchmarkResult {
    let ranges = benchmark_pan_ranges(full_range, BENCH_RANGE_RUNS);
    if ranges.is_empty() {
        return Phase2PanCacheBenchmarkResult::default();
    }

    app.dataset.loaded_channels.clear_envelope_cache();
    let mut result = Phase2PanCacheBenchmarkResult {
        ranges: ranges.len(),
        ..Phase2PanCacheBenchmarkResult::default()
    };

    for range in ranges {
        app.view.x_range = Some(range);
        app.view.hover_x = Some((range.0 + range.1) * 0.5);
        let started = std::time::Instant::now();
        let _visible_rows = app.visible_row_traces(BENCH_VISIBLE_ENVELOPE_BUCKETS, true, false);
        let elapsed_sec = started.elapsed().as_secs_f64();
        let envelope_stats = app.dataset.loaded_channels.last_envelope_stats;

        result.elapsed_sec += elapsed_sec;
        result.max_elapsed_sec = result.max_elapsed_sec.max(elapsed_sec);
        result.envelope_hits += envelope_stats.hits;
        result.envelope_misses += envelope_stats.misses;
        result.context_hits += envelope_stats.context_hits;
        result.context_misses += envelope_stats.context_misses;
        result.tile_hits += envelope_stats.tile_hits;
        result.tile_builds += envelope_stats.tile_builds;
        result.step_edge_hits += envelope_stats.step_edge_hits;
        result.step_edge_builds += envelope_stats.step_edge_builds;
    }

    result
}

fn benchmark_phase2_hover(app: &PanelYApp, full_range: (f64, f64)) -> Phase2HoverBenchmarkResult {
    let Some(shared_time) = app.dataset.shared_time.as_ref() else {
        return Phase2HoverBenchmarkResult::empty();
    };
    let Some((range_start, range_end)) = normalized_range(full_range) else {
        return Phase2HoverBenchmarkResult::empty();
    };
    let span = range_end - range_start;
    if span <= 0.0 {
        return Phase2HoverBenchmarkResult::empty();
    }

    let started = std::time::Instant::now();
    let mut lookups = 0usize;
    let mut hits = 0usize;
    for index in 0..BENCH_HOVER_RUNS {
        let ratio = if BENCH_HOVER_RUNS <= 1 {
            0.5
        } else {
            index as f64 / (BENCH_HOVER_RUNS - 1) as f64
        };
        let target_time = range_start + span * ratio;
        for row in &app.view.rows {
            for row_channel in row.channels.iter().filter(|channel| channel.visible) {
                let Some(channel) = app
                    .dataset
                    .loaded_channels
                    .channel(&row_channel.channel_path)
                else {
                    continue;
                };
                lookups += 1;
                if hover_value_at_time(
                    &shared_time.time,
                    &channel.values,
                    target_time,
                    row_channel.draw_mode,
                )
                .is_some()
                {
                    hits += 1;
                }
            }
        }
    }

    Phase2HoverBenchmarkResult {
        positions: BENCH_HOVER_RUNS,
        lookups,
        hits,
        elapsed_sec: started.elapsed().as_secs_f64(),
    }
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

#[derive(Debug)]
struct Phase2VisibleRunResult {
    index: usize,
    range: (f64, f64),
    elapsed_sec: f64,
    row_count: usize,
    visible_channel_count: usize,
    envelope_trace_count: usize,
    dense_step_trace_count: usize,
    raw_step_trace_count: usize,
    edge_step_trace_count: usize,
    raw_step_sample_count: usize,
    draw_point_count: usize,
    source_sample_count: usize,
    hover_value_count: usize,
    envelope_stats: EnvelopeCacheStats,
}

#[derive(Debug, Default)]
struct Phase2PanCacheBenchmarkResult {
    ranges: usize,
    elapsed_sec: f64,
    max_elapsed_sec: f64,
    envelope_hits: usize,
    envelope_misses: usize,
    context_hits: usize,
    context_misses: usize,
    tile_hits: usize,
    tile_builds: usize,
    step_edge_hits: usize,
    step_edge_builds: usize,
}

impl Phase2PanCacheBenchmarkResult {
    fn avg_visible_sec(&self) -> f64 {
        if self.ranges == 0 {
            0.0
        } else {
            self.elapsed_sec / self.ranges as f64
        }
    }
}

#[derive(Debug)]
struct Phase2HoverBenchmarkResult {
    positions: usize,
    lookups: usize,
    hits: usize,
    elapsed_sec: f64,
}

impl Phase2HoverBenchmarkResult {
    fn empty() -> Self {
        Self {
            positions: 0,
            lookups: 0,
            hits: 0,
            elapsed_sec: 0.0,
        }
    }

    fn avg_lookup_us(&self) -> f64 {
        if self.lookups == 0 {
            0.0
        } else {
            self.elapsed_sec * 1_000_000.0 / self.lookups as f64
        }
    }
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

fn benchmark_pan_ranges(full_range: (f64, f64), run_count: usize) -> Vec<(f64, f64)> {
    let Some((start, end)) = normalized_range(full_range) else {
        return Vec::new();
    };
    let span = end - start;
    if span <= 0.0 || run_count == 0 {
        return Vec::new();
    }

    let window = (span * 0.20).max(span * 1.0e-9).min(span);
    if window >= span || run_count == 1 {
        return vec![(start, start + window)];
    }

    let available = span - window;
    let requested_total_shift = window * 0.10 * run_count.saturating_sub(1) as f64;
    let total_shift = requested_total_shift.min(available);
    let step = total_shift / run_count.saturating_sub(1) as f64;
    let first_left = start + (available - total_shift) * 0.5;

    (0..run_count)
        .map(|index| {
            let left = (first_left + step * index as f64).min(end - window);
            (left, left + window)
        })
        .collect()
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

fn load_channel_data(
    path: &str,
    summary: &parquet_schema::SchemaSummary,
    channel_path: &str,
    needs_time: bool,
    needs_channel: bool,
) -> Result<LoadedChannelData, String> {
    let time = if needs_time {
        Some(parquet_waveform::read_time_column(path, summary)?)
    } else {
        None
    };
    let channel = if needs_channel {
        Some(parquet_waveform::read_channel_values(
            path,
            summary,
            channel_path,
        )?)
    } else {
        None
    };

    if let (Some(time), Some(channel)) = (&time, &channel)
        && time.sample_count() != channel.sample_count()
    {
        return Err(format!(
            "time/value length mismatch: {} vs {}",
            time.sample_count(),
            channel.sample_count()
        ));
    }

    Ok(LoadedChannelData { time, channel })
}

fn spawn_load_channel_job(
    tx: mpsc::Sender<LoadJobResult>,
    ctx: egui::Context,
    job_id: u64,
    path: String,
    summary: parquet_schema::SchemaSummary,
    channel_path: String,
    needs_time: bool,
    needs_channel: bool,
) {
    std::thread::spawn(move || {
        let result = load_channel_data(&path, &summary, &channel_path, needs_time, needs_channel);
        let _ = tx.send(LoadJobResult {
            job_id,
            parquet_path: path,
            channel_path,
            result,
        });
        ctx.request_repaint();
    });
}

impl PanelYApp {
    fn select_parquet_path(&mut self, path: String) {
        if self.load.is_busy() {
            self.load.status = "Wait for active load jobs before changing Parquet path".to_owned();
            return;
        }

        let path = path.trim().to_owned();
        if path.is_empty() {
            return;
        }

        if self.dataset.parquet_path != path {
            self.dataset.parquet_path = path.clone();
            self.reset_dataset_after_path_change();
        }
        remember_recent_parquet_path(&mut self.dataset.recent_parquet_paths, path);
        self.load.error = None;
        self.load.status = "Selected Parquet path. Load schema to continue.".to_owned();
    }

    fn reset_dataset_after_path_change(&mut self) {
        self.dataset.schema = None;
        self.dataset.shared_time = None;
        self.dataset.loaded_channels.clear_all();
        self.view.reset_empty();
        self.load.progress = None;
        self.load.error = None;
        self.load.status = "Path changed. Load schema to continue.".to_owned();
    }

    fn clear_dataset_after_schema_failure(&mut self) {
        self.dataset.schema = None;
        self.dataset.shared_time = None;
        self.dataset.loaded_channels.clear_all();
        self.view.reset_empty();
        self.load.progress = None;
    }

    fn save_session_preset(&mut self) {
        self.view.ensure_row_state();
        let preset = SessionPreset::capture(&self.dataset.parquet_path, &self.view);
        let status = format!("Saved session preset: {}", preset.summary());
        self.session_preset = Some(preset);
        self.load.error = None;
        self.load.status = status;
    }

    fn apply_session_preset(&mut self) {
        if self.load.is_busy() {
            self.load.status =
                "Wait for active load jobs before applying session preset".to_owned();
            return;
        }

        let Some(preset) = self.session_preset.clone() else {
            self.load.status = "No session preset saved".to_owned();
            return;
        };

        let mut schema_loaded_for_preset = false;
        if self.dataset.parquet_path.trim() != preset.parquet_path || self.dataset.schema.is_none()
        {
            self.dataset.parquet_path = preset.parquet_path.clone();
            self.reset_dataset_after_path_change();
            self.load_schema();
            schema_loaded_for_preset = true;
        }

        let Some(schema) = self.dataset.schema.as_ref() else {
            if self.load.error.is_none() {
                self.load.status =
                    "Load schema before applying the saved session preset".to_owned();
            }
            return;
        };

        let report = preset.apply_to_view(&mut self.view, schema);
        self.dataset.loaded_channels.clear_envelope_cache();
        remember_recent_parquet_path(
            &mut self.dataset.recent_parquet_paths,
            preset.parquet_path.clone(),
        );

        let schema_note = if schema_loaded_for_preset {
            "schema loaded, "
        } else {
            ""
        };
        let missing_note = if report.missing_channels.is_empty() {
            String::new()
        } else {
            format!(
                ", skipped {} missing channel(s)",
                report.missing_channels.len()
            )
        };
        self.load.status = format!(
            "Applied session preset (not .pyc.json): {schema_note}{} row(s), {} channel(s){missing_note}",
            report.restored_rows, report.restored_channels
        );
        self.load.error = if report.missing_channels.is_empty() {
            None
        } else {
            Some(format!(
                "Session preset skipped channel(s) not found in the current schema:\n{}",
                report.missing_channels.join("\n")
            ))
        };
    }

    fn clear_session_preset(&mut self) {
        self.session_preset = None;
        self.load.error = None;
        self.load.status = "Cleared session preset".to_owned();
    }

    fn add_cached_channel_to_selected_row(&mut self, channel_path: &str) {
        let Some(channel) = self.dataset.loaded_channels.channel(channel_path) else {
            self.load.status = format!("Channel is not cached: {channel_path}");
            return;
        };
        let channel_name = channel.channel_name.clone();
        let Some(target_row_id) = self.view.selected_row_id_or_first() else {
            self.load.status = "Select a row before adding cached channel".to_owned();
            return;
        };
        let target_row_label = self.view.row_display_name(target_row_id);
        let (row_added, row_id) = self
            .view
            .add_channel_to_row(target_row_id, channel_path)
            .unwrap_or((false, target_row_id));
        self.view.selected_channel = channel_path.to_owned();
        self.dataset.loaded_channels.clear_envelope_cache();
        self.load.error = None;
        self.load.status = if row_added {
            format!("Added cached channel: {channel_name} -> {target_row_label} (id {row_id})")
        } else {
            format!(
                "Cached channel already exists in {target_row_label}: {channel_name} (id {row_id})"
            )
        };
    }

    fn unload_cached_channel(&mut self, channel_path: &str) {
        if self.load.is_channel_loading(channel_path) {
            self.load.status = format!("Wait for active load before unloading: {channel_path}");
            return;
        }

        let channel_name = self
            .dataset
            .loaded_channels
            .channel(channel_path)
            .map(|channel| channel.channel_name.clone())
            .unwrap_or_else(|| channel_path.to_owned());
        if self.dataset.loaded_channels.unload_channel(channel_path) {
            self.load.error = None;
            self.load.status = format!(
                "Unloaded cached channel data: {channel_name}; row references remain as unloaded"
            );
        } else {
            self.load.status = format!("Channel is not cached: {channel_path}");
        }
    }

    fn unload_all_cached_channels(&mut self) {
        if self.load.is_busy() {
            self.load.status = "Wait for active load jobs before unloading all channels".to_owned();
            return;
        }

        let channel_count = self.dataset.loaded_channels.raw_by_channel.len();
        let raw_memory = self.dataset.loaded_channels.raw_memory_bytes();
        self.dataset.loaded_channels.clear_all();
        self.load.error = None;
        self.load.status = format!(
            "Unloaded all cached channel data: {channel_count} channel(s), {:.1} MiB; row references remain as unloaded",
            bytes_to_mib(raw_memory)
        );
    }

    fn clear_draw_caches(&mut self) {
        let envelope_count = self.dataset.loaded_channels.envelope_cache.len();
        let line_tile_count = self.dataset.loaded_channels.line_tile_cache.len();
        let step_edge_count = self.dataset.loaded_channels.step_edge_cache.len();
        let hint_count = self.dataset.loaded_channels.step_fallback_hints.len();
        if self.dataset.loaded_channels.clear_draw_caches() {
            self.load.error = None;
            self.load.status = format!(
                "Cleared draw caches: {envelope_count} envelope(s), {line_tile_count} line tile cache(s), {step_edge_count} step edge cache(s), {hint_count} hint(s)"
            );
        } else {
            self.load.status = "No draw caches to clear".to_owned();
        }
    }

    fn handle_cache_action(&mut self, action: CacheControlAction) {
        match action {
            CacheControlAction::AddToSelectedRow(channel_path) => {
                self.add_cached_channel_to_selected_row(&channel_path);
            }
            CacheControlAction::UnloadChannel(channel_path) => {
                self.unload_cached_channel(&channel_path);
            }
            CacheControlAction::UnloadAllChannels => {
                self.unload_all_cached_channels();
            }
            CacheControlAction::ClearDrawCaches => {
                self.clear_draw_caches();
            }
        }
    }

    fn browse_parquet_path(&mut self) {
        if self.load.is_busy() {
            self.load.status = "Wait for active load jobs before browsing".to_owned();
            return;
        }

        let mut dialog = rfd::FileDialog::new()
            .set_title("Open Parquet file")
            .add_filter("Parquet", &["parquet"]);
        if let Some(directory) = parquet_dialog_directory(&self.dataset.parquet_path) {
            dialog = dialog.set_directory(directory);
        }

        if let Some(path) = dialog.pick_file() {
            self.select_parquet_path(path.display().to_string());
        }
    }

    fn load_schema(&mut self) {
        if self.load.is_busy() {
            self.load.status = "Wait for active load jobs before loading schema".to_owned();
            return;
        }

        let parquet_path = match validate_parquet_path(&self.dataset.parquet_path) {
            Ok(path) => path,
            Err(error) => {
                self.clear_dataset_after_schema_failure();
                self.load.status = error.status().to_owned();
                self.load.error = Some(error.message());
                return;
            }
        };

        match parquet_schema::read_schema_summary(&parquet_path) {
            Ok(summary) => {
                self.dataset.parquet_path = parquet_path.display().to_string();
                remember_recent_parquet_path(
                    &mut self.dataset.recent_parquet_paths,
                    self.dataset.parquet_path.clone(),
                );
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
                self.clear_dataset_after_schema_failure();
                self.load.status = "Schema load failed".to_owned();
                self.load.error = Some(format_schema_load_error(&parquet_path, &error));
            }
        }
    }

    fn load_selected_channel(&mut self, ctx: &egui::Context) {
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

        let Some(target_row_id) = self.view.selected_row_id_or_first() else {
            self.load.status = "Select a row before loading waveform data".to_owned();
            return;
        };
        let path = self.dataset.parquet_path.clone();
        let selected_channel = self.view.selected_channel.clone();
        let channel_name = summary
            .channels
            .iter()
            .find(|channel| channel.path == selected_channel)
            .map(|channel| channel.display_name().to_owned())
            .unwrap_or_else(|| selected_channel.clone());
        let target_row_label = self.view.row_display_name(target_row_id);
        let (row_added, row_id) = self
            .view
            .add_channel_to_row(target_row_id, &selected_channel)
            .unwrap_or((false, target_row_id));
        let time_was_cached = self.dataset.shared_time.is_some();
        let channel_was_cached = self.dataset.loaded_channels.has_channel(&selected_channel);

        if time_was_cached && channel_was_cached {
            self.finish_cached_channel_add(
                &selected_channel,
                &target_row_label,
                row_added,
                row_id,
                true,
            );
            return;
        }

        if self.load.is_channel_loading(&selected_channel) {
            let row_note = if row_added {
                format!("queued display in {target_row_label}")
            } else {
                format!("already queued in {target_row_label}")
            };
            self.load.status = format!("Already loading: {channel_name}, {row_note}");
            return;
        }

        let needs_time = !time_was_cached;
        let needs_channel = !channel_was_cached;
        let job_id = self.next_load_job_id;
        self.next_load_job_id = self.next_load_job_id.saturating_add(1);
        self.load.active_jobs.insert(
            job_id,
            ActiveLoadJob {
                channel_path: selected_channel.clone(),
                channel_name: channel_name.clone(),
                target_row_id,
                target_row_label: target_row_label.clone(),
                time_was_cached,
                channel_was_cached,
                started: std::time::Instant::now(),
            },
        );
        self.load.error = None;
        self.load.status = format!("Loading: {channel_name} -> {target_row_label}");
        self.load.refresh_progress();

        spawn_load_channel_job(
            self.load_result_tx.clone(),
            ctx.clone(),
            job_id,
            path,
            summary,
            selected_channel,
            needs_time,
            needs_channel,
        );
    }

    fn finish_cached_channel_add(
        &mut self,
        channel_path: &str,
        target_row_label: &str,
        row_added: bool,
        row_id: u64,
        time_was_cached: bool,
    ) {
        let Some(channel) = self.dataset.loaded_channels.channel(channel_path) else {
            self.load.status = format!("Loaded channel is missing from cache: {channel_path}");
            return;
        };
        let channel_name = channel.channel_name.clone();
        let channel_sample_count = channel.sample_count();
        let channel_memory = channel.memory_bytes();
        if self.view.x_range.is_none() {
            self.view.x_range = self
                .dataset
                .shared_time
                .as_ref()
                .and_then(parquet_waveform::TimeData::time_range);
        }
        self.load.error = None;
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
            "reused cached: {channel_name} ({channel_sample_count} samples, {:.1} MiB), {row_note} (id {row_id}); {time_note}; cache {} ch, total {:.1} MiB",
            bytes_to_mib(channel_memory),
            self.dataset.loaded_channels.raw_by_channel.len(),
            bytes_to_mib(total_memory)
        );
    }

    fn drain_load_results(&mut self) {
        while let Ok(result) = self.load_result_rx.try_recv() {
            self.apply_load_result(result);
        }
    }

    fn apply_load_result(&mut self, result: LoadJobResult) {
        let Some(job) = self.load.active_jobs.remove(&result.job_id) else {
            self.load.refresh_progress();
            return;
        };
        self.load.refresh_progress();

        if result.parquet_path != self.dataset.parquet_path {
            self.load.status = format!(
                "Ignored stale load result for {} from {}",
                job.channel_name, result.parquet_path
            );
            return;
        }
        if result.channel_path != job.channel_path {
            self.load.status = format!(
                "Ignored mismatched load result: expected {}, got {}",
                job.channel_path, result.channel_path
            );
            return;
        }

        match result.result {
            Ok(loaded) => self.apply_loaded_channel(job, loaded),
            Err(error) => {
                self.load.error = Some(error.clone());
                self.load.status = format!("Waveform load failed: {}: {error}", job.channel_name);
            }
        }
    }

    fn apply_loaded_channel(&mut self, job: ActiveLoadJob, loaded: LoadedChannelData) {
        if let Some(time) = loaded.time {
            if let Some(existing_time) = &self.dataset.shared_time
                && existing_time.sample_count() != time.sample_count()
            {
                self.load.status = format!(
                    "Time load failed: sample count changed: {} vs {}",
                    existing_time.sample_count(),
                    time.sample_count()
                );
                self.load.error = Some(self.load.status.clone());
                return;
            }
            self.dataset.shared_time = Some(time);
        }

        let Some(shared_time) = self.dataset.shared_time.as_ref() else {
            self.load.status = "Time data is not available".to_owned();
            self.load.error = Some(self.load.status.clone());
            return;
        };
        let time_sample_count = shared_time.sample_count();
        let time_read_sec = shared_time.elapsed.as_secs_f64();

        if let Some(channel) = loaded.channel {
            if channel.sample_count() != time_sample_count {
                self.load.status = format!(
                    "Waveform load failed: time/value length mismatch: {} vs {}",
                    time_sample_count,
                    channel.sample_count()
                );
                self.load.error = Some(self.load.status.clone());
                return;
            }
            if !self
                .dataset
                .loaded_channels
                .has_channel(&channel.channel_path)
            {
                self.dataset.loaded_channels.insert_channel(channel);
            }
        }

        let Some(channel) = self.dataset.loaded_channels.channel(&job.channel_path) else {
            self.load.status =
                format!("Loaded channel is missing from cache: {}", job.channel_path);
            self.load.error = Some(self.load.status.clone());
            return;
        };

        let channel_name = channel.channel_name.clone();
        let channel_read_sec = channel.elapsed.as_secs_f64();
        let channel_sample_count = channel.sample_count();
        let channel_memory = channel.memory_bytes();
        let (row_added, row_id) = self
            .view
            .add_channel_to_row(job.target_row_id, &job.channel_path)
            .unwrap_or((false, job.target_row_id));
        if self.view.x_range.is_none() {
            self.view.x_range = shared_time.time_range();
        }

        self.load.error = None;
        let cache_note = if job.channel_was_cached {
            "reused cached"
        } else {
            "loaded"
        };
        let row_note = if row_added {
            format!("added to {}", job.target_row_label)
        } else {
            format!("already in {}", job.target_row_label)
        };
        let time_note = if job.time_was_cached {
            "time cached".to_owned()
        } else {
            format!("time loaded {:.3}s", time_read_sec)
        };
        let total_memory = self
            .dataset
            .shared_time
            .as_ref()
            .map_or(0, parquet_waveform::TimeData::memory_bytes)
            + self.dataset.loaded_channels.raw_memory_bytes();
        let elapsed_sec = job.started.elapsed().as_secs_f64();
        self.load.status = format!(
            "{cache_note}: {channel_name} ({channel_sample_count} samples, {:.1} MiB, read {:.3}s), {row_note} (id {row_id}); {time_note}; cache {} ch, total {:.1} MiB; background {:.3}s",
            bytes_to_mib(channel_memory),
            channel_read_sec,
            self.dataset.loaded_channels.raw_by_channel.len(),
            bytes_to_mib(total_memory),
            elapsed_sec
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

    fn should_use_interaction_preview(&self) -> bool {
        if !self.view.large_preview_enabled || !self.interaction.preview_active() {
            return false;
        }

        let Some(shared_time) = self.dataset.shared_time.as_ref() else {
            return false;
        };
        let Some(full_range) = shared_time.time_range() else {
            return false;
        };
        let min_span = min_view_span(full_range, shared_time.sample_count());
        let view_range = clamp_view_range(
            self.view.x_range.unwrap_or(full_range),
            full_range,
            min_span,
        );
        let Some(sample_range) =
            visible_sample_range(&shared_time.time, shared_time.sample_count(), view_range)
        else {
            return false;
        };
        let (line_channels, step_channels) = self.visible_loaded_draw_mode_counts();

        preview_needed_for_workload(
            shared_time.sample_count(),
            sample_range.source_sample_count,
            line_channels,
            step_channels,
        )
    }

    fn visible_loaded_draw_mode_counts(&self) -> (usize, usize) {
        self.view
            .rows
            .iter()
            .flat_map(|row| row.channels.iter())
            .filter(|channel| {
                channel.visible
                    && self
                        .dataset
                        .loaded_channels
                        .has_channel(&channel.channel_path)
            })
            .fold((0usize, 0usize), |(line, step), channel| {
                match channel.draw_mode {
                    DrawMode::Line => (line + 1, step),
                    DrawMode::Step => (line, step + 1),
                }
            })
    }

    fn visible_row_traces(
        &mut self,
        requested_bucket_count: usize,
        dark_mode: bool,
        preview: bool,
    ) -> Vec<VisibleRowTrace> {
        let rows = self.view.rows.clone();
        let loading_channel_paths = self.load.loading_channel_paths();
        self.dataset.loaded_channels.begin_envelope_frame();
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
                    loading_channel_count: row_missing_channel_counts(
                        &row,
                        None,
                        &loading_channel_paths,
                    )
                    .0,
                    unloaded_channel_count: row_missing_channel_counts(
                        &row,
                        None,
                        &loading_channel_paths,
                    )
                    .1,
                    y_range: row.y_range,
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
                    loading_channel_count: row_missing_channel_counts(
                        &row,
                        Some(&self.dataset.loaded_channels),
                        &loading_channel_paths,
                    )
                    .0,
                    unloaded_channel_count: row_missing_channel_counts(
                        &row,
                        Some(&self.dataset.loaded_channels),
                        &loading_channel_paths,
                    )
                    .1,
                    y_range: row.y_range,
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
        let mut latest_auto_y_ranges = Vec::with_capacity(rows.len());
        let mut built_count = 0usize;
        let mut exact_step_count = 0usize;
        let mut edge_step_count = 0usize;
        let mut dense_step_count = 0usize;
        let hover_x = self.view.hover_x;
        {
            let dataset = &mut self.dataset;
            let Some(shared_time) = dataset.shared_time.as_ref() else {
                return visible_rows;
            };
            let time_values = &shared_time.time;
            let loaded_channels = &mut dataset.loaded_channels;
            loaded_channels.prepare_envelope_context(
                time_values,
                shared_time.sample_count(),
                view_range,
                full_range,
                requested_bucket_count,
            );

            for (row_index, row) in rows.into_iter().enumerate() {
                let mut traces = Vec::with_capacity(row.channels.len());
                let row_channel_count = row.channels.len();
                let mut loading_channel_count = 0usize;
                let mut unloaded_channel_count = 0usize;
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
                        if loading_channel_paths.contains(&row_channel.channel_path) {
                            loading_channel_count += 1;
                        } else {
                            unloaded_channel_count += 1;
                        }
                        continue;
                    };

                    let data = match row_channel.draw_mode {
                        DrawMode::Line => {
                            let Some((envelope, was_built)) = loaded_channels.ensure_envelope(
                                &channel_path,
                                time_values,
                                view_range,
                                requested_bucket_count,
                                true,
                            ) else {
                                continue;
                            };
                            if was_built {
                                built_count += 1;
                            }
                            VisibleTraceData::Envelope(envelope)
                        }
                        DrawMode::Step => {
                            let sample_range =
                                visible_sample_range(time_values, sample_count, view_range);
                            let use_preview_envelope = preview
                                && sample_range.is_some_and(|range| {
                                    range.draw_sample_count > MAX_EXACT_STEP_SAMPLES
                                });
                            let use_hint_fallback = sample_range.is_some_and(|range| {
                                loaded_channels.should_skip_step_change_points(
                                    &channel_path,
                                    view_range,
                                    range.source_sample_count,
                                )
                            });

                            if use_preview_envelope || use_hint_fallback {
                                dense_step_count += 1;
                                let Some((envelope, was_built)) = loaded_channels.ensure_envelope(
                                    &channel_path,
                                    time_values,
                                    view_range,
                                    requested_bucket_count,
                                    false,
                                ) else {
                                    continue;
                                };
                                if was_built {
                                    built_count += 1;
                                }
                                VisibleTraceData::DenseStepEnvelope(envelope)
                            } else if let Some(raw_step) =
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
                            } else if let Some(step_result) = loaded_channels
                                .cached_change_point_step_trace_result(
                                    &channel_path,
                                    time_values,
                                    view_range,
                                    MAX_STEP_CHANGE_POINTS,
                                )
                            {
                                match step_result {
                                    StepTraceBuildResult::Trace(edge_step) => {
                                        edge_step_count += 1;
                                        VisibleTraceData::RawStep(edge_step)
                                    }
                                    StepTraceBuildResult::TooManyChangePoints {
                                        source_sample_count,
                                    } => {
                                        loaded_channels.record_step_change_point_fallback(
                                            &channel_path,
                                            view_range,
                                            source_sample_count,
                                        );
                                        dense_step_count += 1;
                                        let Some((envelope, was_built)) = loaded_channels
                                            .ensure_envelope(
                                                &channel_path,
                                                time_values,
                                                view_range,
                                                requested_bucket_count,
                                                false,
                                            )
                                        else {
                                            continue;
                                        };
                                        if was_built {
                                            built_count += 1;
                                        }
                                        VisibleTraceData::DenseStepEnvelope(envelope)
                                    }
                                    StepTraceBuildResult::Empty => {
                                        dense_step_count += 1;
                                        let Some((envelope, was_built)) = loaded_channels
                                            .ensure_envelope(
                                                &channel_path,
                                                time_values,
                                                view_range,
                                                requested_bucket_count,
                                                false,
                                            )
                                        else {
                                            continue;
                                        };
                                        if was_built {
                                            built_count += 1;
                                        }
                                        VisibleTraceData::DenseStepEnvelope(envelope)
                                    }
                                }
                            } else {
                                continue;
                            }
                        }
                    };

                    let hover_value = hover_x.and_then(|hover_x| {
                        loaded_channels.channel(&channel_path).and_then(|channel| {
                            hover_value_at_time(
                                time_values,
                                &channel.values,
                                hover_x,
                                row_channel.draw_mode,
                            )
                        })
                    });

                    traces.push(VisibleTrace {
                        channel_name,
                        channel_path,
                        sample_count,
                        color: row_channel.color(dark_mode),
                        line_width: row_channel
                            .line_width
                            .clamp(MIN_TRACE_LINE_WIDTH, MAX_TRACE_LINE_WIDTH),
                        draw_mode: row_channel.draw_mode,
                        hover_value,
                        data,
                    });
                }

                visible_rows.push(VisibleRowTrace {
                    row_id: row.id,
                    row_index,
                    row_channel_count,
                    loading_channel_count,
                    unloaded_channel_count,
                    y_range: row.y_range,
                    traces,
                });

                if let Some((min, max)) = visible_rows
                    .last()
                    .and_then(|row| combined_trace_value_range(&row.traces))
                {
                    latest_auto_y_ranges.push((row.id, Some(padded_range(min, max))));
                } else {
                    latest_auto_y_ranges.push((row.id, None));
                }
            }
        }

        for (row_id, auto_y_range) in latest_auto_y_ranges {
            let Some(auto_y_range) = auto_y_range else {
                continue;
            };
            if let Some(row) = self.view.rows.iter_mut().find(|row| row.id == row_id) {
                row.y_range.set_last_auto(auto_y_range);
            }
        }

        if built_count > 0 || exact_step_count > 0 || edge_step_count > 0 || dense_step_count > 0 {
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
            let envelope_stats = self.dataset.loaded_channels.last_envelope_stats;
            let quality = if preview { "preview" } else { "full" };
            self.load.status = format!(
                "{quality} view {:.6}..{:.6}s: {} rows, {} ch, {} visible samples, built {} envelope(s), raw step {}, edge step {}, dense step {}, env h/m {}/{}, ctx {}/{}, clips {}, tile h/b {}/{}, edge h/b {}/{}, cache {}, edge cache {}, hints {}",
                view_range.0,
                view_range.1,
                visible_rows.len(),
                visible_channel_count,
                source_sample_count,
                built_count,
                exact_step_count,
                edge_step_count,
                dense_step_count,
                envelope_stats.hits,
                envelope_stats.misses,
                envelope_stats.context_hits,
                envelope_stats.context_misses,
                envelope_stats.clipped,
                envelope_stats.tile_hits,
                envelope_stats.tile_builds,
                envelope_stats.step_edge_hits,
                envelope_stats.step_edge_builds,
                self.dataset.loaded_channels.envelope_cache.len(),
                self.dataset.loaded_channels.step_edge_cache.len(),
                self.dataset.loaded_channels.step_fallback_hints.len()
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
            self.view.hover_x = None;
            return;
        };
        let Some(full_range) = shared_time.time_range() else {
            self.view.hover_x = None;
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
        let (pointer_pos, zoom_delta, scroll_delta, zoom_modifier_pressed) = ui.input(|input| {
            (
                input.pointer.latest_pos(),
                input.zoom_delta(),
                input.smooth_scroll_delta(),
                input.modifiers.matches_any(egui::Modifiers::COMMAND),
            )
        });
        let Some((pointer_pos, plot_rect)) = pointer_pos.and_then(|pointer_pos| {
            plot_rects
                .iter()
                .copied()
                .find(|plot_rect| plot_rect.contains(pointer_pos))
                .map(|plot_rect| (pointer_pos, plot_rect))
        }) else {
            if self.view.hover_x.take().is_some() {
                ui.ctx().request_repaint();
            }
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
                ui.input_mut(|input| {
                    input.smooth_scroll_delta.x = 0.0;
                });
            }

            let zoom_factor = x_zoom_factor(zoom_delta, scroll_delta.y, zoom_modifier_pressed);
            if zoom_delta == 1.0 && zoom_modifier_pressed && scroll_delta.y != 0.0 {
                ui.input_mut(|input| {
                    input.smooth_scroll_delta.y = 0.0;
                });
            }

            if zoom_factor.is_finite() && (zoom_factor - 1.0).abs() > f64::EPSILON {
                let anchor_ratio = ((pointer_pos.x - plot_rect.left()) / plot_rect.width()) as f64;
                next_range =
                    zoom_view_range(next_range, full_range, anchor_ratio, zoom_factor, min_span);
                changed = true;
            }
        }

        if changed && next_range != current_range {
            self.view.x_range = Some(next_range);
            self.interaction.mark_range_changed();
            ui.ctx().request_repaint_after(INTERACTION_PREVIEW_SETTLE);
        }

        let next_hover_x = time_at_plot_x(pointer_pos.x, plot_rect, next_range);
        if self.view.hover_x != next_hover_x {
            self.view.hover_x = next_hover_x;
            ui.ctx().request_repaint();
        }
    }
}

impl eframe::App for PanelYApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let frame_started = Instant::now();
        self.drain_load_results();
        let load_active = self.load.is_busy();
        let loading_channel_paths = self.load.loading_channel_paths();

        egui::Panel::top("top_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Panel_y Rust Phase 2");
                ui.separator();
                ui.label(&self.load.status);
                if self.load.pending_jobs > 0 {
                    ui.separator();
                    ui.add(egui::Spinner::new());
                    ui.label(format!("jobs: {}", self.load.pending_jobs));
                }
                if let Some(progress) = &self.load.progress {
                    ui.separator();
                    ui.label(progress);
                }
                if self.perf.show {
                    ui.separator();
                    ui.monospace(self.perf.summary());
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
                        ui.horizontal(|ui| {
                            let browse_width = 72.0;
                            let text_width = (ui.available_width()
                                - browse_width
                                - ui.spacing().item_spacing.x)
                                .max(80.0);
                            let path_before_edit = self.dataset.parquet_path.clone();
                            let path_response = ui.add_enabled(
                                !load_active,
                                egui::TextEdit::singleline(&mut self.dataset.parquet_path)
                                    .desired_width(text_width),
                            );
                            if path_response.changed()
                                && self.dataset.parquet_path != path_before_edit
                            {
                                self.reset_dataset_after_path_change();
                            }

                            if ui
                                .add_enabled(
                                    !load_active,
                                    egui::Button::new("Browse").min_size(egui::vec2(
                                        browse_width,
                                        ui.spacing().interact_size.y,
                                    )),
                                )
                                .clicked()
                            {
                                self.browse_parquet_path();
                            }
                        });

                        let parquet_path_validation =
                            validate_parquet_path(&self.dataset.parquet_path);
                        match &parquet_path_validation {
                            Ok(path) => {
                                ui.label(format!("Ready: {}", path.display()));
                            }
                            Err(error) => {
                                ui.colored_label(ui.visuals().warn_fg_color, error.message());
                            }
                        }

                        if !self.dataset.recent_parquet_paths.is_empty() {
                            let mut selected_recent_path = None;
                            ui.horizontal(|ui| {
                                ui.label("Recent");
                                egui::ComboBox::from_id_salt("recent_parquet_paths")
                                    .selected_text("Choose...")
                                    .show_ui(ui, |ui| {
                                        for path in &self.dataset.recent_parquet_paths {
                                            let response = ui.selectable_label(
                                                self.dataset.parquet_path == *path,
                                                recent_path_label(path),
                                            );
                                            if response.clicked() {
                                                selected_recent_path = Some(path.clone());
                                            }
                                            response.on_hover_text(path);
                                        }
                                    });
                            });
                            if let Some(path) = selected_recent_path {
                                self.select_parquet_path(path);
                            }
                        }

                        ui.add_space(12.0);
                        if ui
                            .add_enabled(
                                !load_active && parquet_path_validation.is_ok(),
                                egui::Button::new("Load Schema"),
                            )
                            .clicked()
                        {
                            self.load_schema();
                        }

                        if let Some(error) = self.load.error.clone() {
                            let mut clear_error = false;
                            ui.add_space(8.0);
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.colored_label(ui.visuals().warn_fg_color, "Load issue");
                                    if ui.button("Clear").clicked() {
                                        clear_error = true;
                                    }
                                });
                                ui.add(egui::Label::new(error).wrap());
                            });
                            if clear_error {
                                self.load.error = None;
                                if !load_active {
                                    self.load.status = "Ready".to_owned();
                                }
                            }
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
                            self.load_selected_channel(ui.ctx());
                        }

                        ui.add_space(16.0);
                        let schema_response = draw_schema_controls(
                            ui,
                            &mut self.view,
                            &self.dataset,
                            &loading_channel_paths,
                        );
                        if schema_response.selection_changed {
                            self.load.status =
                                format!("Selected channel: {}", self.view.selected_channel);
                        }
                        if let Some(channel_path) = schema_response.load_channel {
                            self.view.selected_channel = channel_path;
                            self.load_selected_channel(ui.ctx());
                        }

                        if self.dataset.schema.is_some() || self.session_preset.is_some() {
                            ui.add_space(16.0);
                            ui.heading("Session preset");
                            ui.label("Rust session only; not .pyc.json.");

                            let mut save_preset = false;
                            let mut apply_preset = false;
                            let mut clear_preset = false;
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(
                                        !load_active && self.dataset.schema.is_some(),
                                        egui::Button::new("Save"),
                                    )
                                    .clicked()
                                {
                                    save_preset = true;
                                }
                                if ui
                                    .add_enabled(
                                        !load_active && self.session_preset.is_some(),
                                        egui::Button::new("Apply"),
                                    )
                                    .clicked()
                                {
                                    apply_preset = true;
                                }
                                if ui
                                    .add_enabled(
                                        self.session_preset.is_some(),
                                        egui::Button::new("Clear"),
                                    )
                                    .clicked()
                                {
                                    clear_preset = true;
                                }
                            });

                            if let Some(preset) = &self.session_preset {
                                ui.monospace(preset.summary());
                            } else {
                                ui.label("No preset saved");
                            }

                            if save_preset {
                                self.save_session_preset();
                            } else if apply_preset {
                                self.apply_session_preset();
                            } else if clear_preset {
                                self.clear_session_preset();
                            }
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
                            ui.checkbox(&mut self.perf.show, "Frame timing");
                            ui.checkbox(&mut self.view.large_preview_enabled, "Emergency preview")
                                .on_hover_text(
                                    "Debug option: temporarily reduces quality only while interacting with very large visible ranges",
                                );
                        }

                        let cache_response = draw_channel_cache_controls(
                            ui,
                            &self.dataset,
                            &self.view,
                            &loading_channel_paths,
                            load_active,
                        );
                        if let Some(action) = cache_response.action {
                            self.handle_cache_action(action);
                        }
                        if draw_row_controls(
                            ui,
                            &mut self.view,
                            self.dataset.schema.as_ref(),
                            &loading_channel_paths,
                        ) {
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
                .scroll_source(
                    egui::containers::scroll_area::ScrollSource::SCROLL_BAR
                        | egui::containers::scroll_area::ScrollSource::MOUSE_WHEEL,
                )
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

                    let interaction_started = Instant::now();
                    self.handle_plot_interaction(ui, &response, &plot_rects);
                    let interaction_elapsed = interaction_started.elapsed();

                    let preview = self.should_use_interaction_preview();
                    let effective_buckets = if preview {
                        preview_envelope_bucket_count(requested_buckets)
                    } else {
                        requested_buckets
                    };
                    let visible_started = Instant::now();
                    let visible_rows =
                        self.visible_row_traces(effective_buckets, ui.visuals().dark_mode, preview);
                    let visible_elapsed = visible_started.elapsed();

                    let draw_started = Instant::now();
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
                                    row.y_range,
                                );
                            }
                            Some(row) if row.loading_channel_count > 0 => {
                                draw_status_label(&painter, plot_rect, "Loading waveform data...");
                            }
                            Some(row) if row.unloaded_channel_count > 0 => {
                                draw_status_label(
                                    &painter,
                                    plot_rect,
                                    "Waveform data is not loaded",
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

                        draw_hover_line(
                            &painter,
                            plot_rect,
                            self.view.hover_x,
                            self.view.x_range,
                            ui.visuals(),
                        );
                        if let Some(row) = visible_row {
                            draw_hover_readout(
                                &painter,
                                plot_rect,
                                self.view.hover_x,
                                ui.visuals(),
                                &row.traces,
                            );
                        }
                    }
                    let draw_elapsed = draw_started.elapsed();
                    let envelope_stats = self.dataset.loaded_channels.last_envelope_stats;
                    self.perf.update(
                        FrameTiming {
                            frame: frame_started.elapsed(),
                            interaction: interaction_elapsed,
                            visible: visible_elapsed,
                            draw: draw_elapsed,
                        },
                        &visible_rows,
                        requested_buckets,
                        effective_buckets,
                        preview,
                        envelope_stats,
                    );
                });
        });
    }
}

fn draw_schema_controls(
    ui: &mut egui::Ui,
    view: &mut ViewState,
    dataset: &DatasetState,
    loading_channel_paths: &BTreeSet<String>,
) -> SchemaControlsResponse {
    ui.heading("Schema");

    let schema = dataset.schema.as_ref();
    let Some(schema) = schema else {
        ui.label("No schema loaded");
        return SchemaControlsResponse::default();
    };

    let mut response = SchemaControlsResponse::default();

    draw_schema_summary(ui, dataset, schema);

    ui.add_space(12.0);
    ui.heading("Channels");
    draw_target_row_selector(ui, view);

    draw_channel_browser_controls(ui, view);

    if schema.channels.is_empty() {
        ui.label("No numeric channels found");
    } else {
        let channel_filter = view.channel_filter.clone();
        let state_filter = view.channel_state_filter;
        let sort = view.channel_sort;
        let target_row_label = view.selected_row_display_name();
        let entries = channel_list_entries(
            &schema.channels,
            &channel_filter,
            state_filter,
            sort,
            &dataset.loaded_channels,
            loading_channel_paths,
        );
        ui.label(format!(
            "{} / {} channels",
            entries.len(),
            schema.channels.len()
        ));

        egui::ScrollArea::vertical()
            .id_salt("channel_list")
            .max_height(320.0)
            .show(ui, |ui| {
                if entries.is_empty() {
                    ui.label("No channels match the filter");
                }

                for (channel, load_state) in entries {
                    draw_channel_list_row(
                        ui,
                        view,
                        channel,
                        load_state,
                        &target_row_label,
                        &mut response,
                    );
                }
            });
    }

    draw_selected_channel_details(ui, view, dataset, schema, loading_channel_paths);
    draw_raw_columns(ui, schema);

    response
}

fn draw_channel_browser_controls(ui: &mut egui::Ui, view: &mut ViewState) {
    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.add(
            egui::TextEdit::singleline(&mut view.channel_filter)
                .hint_text("channel name or path")
                .desired_width(ui.available_width().max(80.0)),
        );
    });

    ui.horizontal_wrapped(|ui| {
        ui.label("State");
        egui::ComboBox::from_id_salt("channel_state_filter")
            .selected_text(view.channel_state_filter.as_str())
            .show_ui(ui, |ui| {
                for state_filter in ChannelStateFilter::ALL {
                    ui.selectable_value(
                        &mut view.channel_state_filter,
                        state_filter,
                        state_filter.as_str(),
                    );
                }
            });

        ui.label("Sort");
        egui::ComboBox::from_id_salt("channel_sort")
            .selected_text(view.channel_sort.as_str())
            .show_ui(ui, |ui| {
                for sort in ChannelSort::ALL {
                    ui.selectable_value(&mut view.channel_sort, sort, sort.as_str());
                }
            });
    });
}

fn draw_channel_list_row(
    ui: &mut egui::Ui,
    view: &mut ViewState,
    channel: &parquet_schema::ColumnInfo,
    load_state: ChannelLoadState,
    target_row_label: &str,
    response: &mut SchemaControlsResponse,
) {
    let selected = view.selected_channel == channel.path;
    ui.push_id(("channel", channel.path.clone()), |ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    load_state.can_load_or_add(),
                    egui::Button::new(load_state.action_label())
                        .min_size(egui::vec2(52.0, ui.spacing().interact_size.y)),
                )
                .on_hover_text(format!(
                    "{} {} to {}",
                    load_state.action_label(),
                    channel.display_name(),
                    target_row_label
                ))
                .clicked()
            {
                view.selected_channel = channel.path.clone();
                response.selection_changed = true;
                response.load_channel = Some(channel.path.clone());
            }

            ui.monospace(format!("{:<8}", load_state.as_str()));
            ui.monospace(format!("#{:02}", channel.index));

            if ui
                .selectable_label(selected, compact_channel_label(channel.display_name(), 24))
                .on_hover_text(&channel.path)
                .clicked()
            {
                view.selected_channel = channel.path.clone();
                response.selection_changed = true;
            }
        });
    });
}

fn draw_selected_channel_details(
    ui: &mut egui::Ui,
    view: &ViewState,
    dataset: &DatasetState,
    schema: &parquet_schema::SchemaSummary,
    loading_channel_paths: &BTreeSet<String>,
) {
    if view.selected_channel.is_empty() {
        return;
    }

    ui.add_space(8.0);
    let Some(channel) = schema
        .channels
        .iter()
        .find(|channel| channel.path == view.selected_channel)
    else {
        ui.label(format!("Selected: {}", view.selected_channel));
        return;
    };

    let load_state = channel_load_state(
        &channel.path,
        &dataset.loaded_channels,
        loading_channel_paths,
    );
    let rows = channel_row_membership(view, &channel.path);
    let rows = if rows.is_empty() {
        "none".to_owned()
    } else {
        rows.join(", ")
    };
    let logical = channel.logical_type.as_deref().unwrap_or("-");
    let loaded_note = dataset
        .loaded_channels
        .channel(&channel.path)
        .map(|loaded| {
            format!(
                "{} samples, {:.1} MiB, read {:.3}s",
                loaded.sample_count(),
                bytes_to_mib(loaded.memory_bytes()),
                loaded.elapsed.as_secs_f64()
            )
        })
        .unwrap_or_else(|| "not loaded".to_owned());

    ui.group(|ui| {
        ui.label("Selected channel");
        ui.monospace(channel.display_name());
        ui.label(format!("State: {}", load_state.as_str()));
        ui.label(format!("Rows: {rows}"));
        ui.label(format!("Index: #{}", channel.index));
        ui.label(format!("Type: {} / {}", channel.physical_type, logical));
        ui.label(format!("Cache: {loaded_note}"));
        ui.monospace(format!("Path: {}", channel.path));
    });
}

fn channel_row_membership(view: &ViewState, channel_path: &str) -> Vec<String> {
    view.rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.channels
                .iter()
                .any(|channel| channel.channel_path == channel_path)
        })
        .map(|(index, _)| format!("Row {}", index + 1))
        .collect()
}

fn draw_raw_columns(ui: &mut egui::Ui, schema: &parquet_schema::SchemaSummary) {
    ui.add_space(12.0);
    egui::CollapsingHeader::new("Raw columns")
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(240.0)
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
        });
}

fn draw_schema_summary(
    ui: &mut egui::Ui,
    dataset: &DatasetState,
    schema: &parquet_schema::SchemaSummary,
) {
    ui.label(format!("Rows: {}", schema.row_count));
    ui.label(format!("Row groups: {}", schema.row_group_count));
    ui.label(format!("Columns: {}", schema.column_count));
    ui.label(format!("Numeric channels: {}", schema.channels.len()));

    match &schema.time_column {
        Some(time_column) => {
            ui.label(format!("Time column: {}", time_column.display_name()));
        }
        None => {
            ui.colored_label(ui.visuals().warn_fg_color, "Time column: not found");
        }
    }

    if let Some(time) = &dataset.shared_time {
        ui.label(format!("Time range: {} s", format_range(time.time_range())));
        let sample_rate = estimated_sample_rate_hz(time.sample_count(), time.time_range())
            .map(format_frequency_hz)
            .unwrap_or_else(|| "-".to_owned());
        ui.label(format!("Estimated sample rate: {sample_rate}"));
    } else {
        ui.label("Time range: not loaded");
        ui.label("Estimated sample rate: -");
    }
}

fn draw_target_row_selector(ui: &mut egui::Ui, view: &mut ViewState) {
    view.ensure_row_state();
    let mut selected_row_id = view.selected_row_id;

    ui.horizontal(|ui| {
        ui.label("Target row");
        egui::ComboBox::from_id_salt("channel_list_target_row")
            .selected_text(view.selected_row_display_name())
            .show_ui(ui, |ui| {
                for (row_index, row) in view.rows.iter().enumerate() {
                    ui.selectable_value(
                        &mut selected_row_id,
                        Some(row.id),
                        format!("Row {}", row_index + 1),
                    );
                }
            });
    });

    view.selected_row_id = selected_row_id;
}

fn draw_channel_cache_controls(
    ui: &mut egui::Ui,
    dataset: &DatasetState,
    view: &ViewState,
    loading_channel_paths: &BTreeSet<String>,
    load_active: bool,
) -> CacheControlsResponse {
    ui.add_space(16.0);
    ui.heading("Cache");
    let mut response = CacheControlsResponse::default();

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

    if !dataset.loaded_channels.line_tile_cache.is_empty() {
        ui.label(format!(
            "Line tiles: {} cached ({:.1} MiB, build {:.3}s)",
            dataset.loaded_channels.line_tile_cache.len(),
            bytes_to_mib(dataset.loaded_channels.line_tile_memory_bytes()),
            dataset.loaded_channels.line_tile_build_seconds()
        ));
    }
    if !dataset.loaded_channels.step_edge_cache.is_empty() {
        let edge_count = dataset
            .loaded_channels
            .step_edge_cache
            .values()
            .map(StepEdgeCache::edge_count)
            .sum::<usize>();
        ui.label(format!(
            "Step edges: {} cached, {} edges ({:.1} MiB, build {:.3}s)",
            dataset.loaded_channels.step_edge_cache.len(),
            edge_count,
            bytes_to_mib(dataset.loaded_channels.step_edge_memory_bytes()),
            dataset.loaded_channels.step_edge_build_seconds()
        ));
    }

    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !load_active && !dataset.loaded_channels.raw_by_channel.is_empty(),
                egui::Button::new("Unload all channels"),
            )
            .on_hover_text("Drop cached channel data while keeping rows and time data")
            .clicked()
        {
            response.action = Some(CacheControlAction::UnloadAllChannels);
        }
        if ui
            .add_enabled(
                dataset.loaded_channels.has_draw_caches(),
                egui::Button::new("Clear draw caches"),
            )
            .on_hover_text("Drop envelope, line tile, and step edge caches while keeping raw data")
            .clicked()
        {
            response.action = Some(CacheControlAction::ClearDrawCaches);
        }
    });

    if dataset.loaded_channels.raw_by_channel.is_empty() {
        ui.label("No cached channels");
        return response;
    }

    let target_row_label = view.selected_row_display_name();
    egui::ScrollArea::vertical()
        .id_salt("loaded_channel_cache_list")
        .max_height(220.0)
        .show(ui, |ui| {
            for channel in dataset.loaded_channels.raw_by_channel.values() {
                let file_name = channel
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("-");
                let channel_loading = loading_channel_paths.contains(&channel.channel_path);
                ui.horizontal_wrapped(|ui| {
                    ui.monospace(format!(
                        "#{} {}",
                        channel.projected_column_index, channel.channel_name
                    ));
                    ui.label(format!(
                        "{} samples, {:.1} MiB, file {}",
                        channel.sample_count(),
                        bytes_to_mib(channel.memory_bytes()),
                        file_name
                    ));
                    if ui
                        .small_button(format!("Add to {target_row_label}"))
                        .on_hover_text("Reuse cached data in the selected row")
                        .clicked()
                    {
                        response.action = Some(CacheControlAction::AddToSelectedRow(
                            channel.channel_path.clone(),
                        ));
                    }
                    if ui
                        .add_enabled(
                            !channel_loading,
                            egui::Button::new("Unload")
                                .small()
                                .wrap_mode(egui::TextWrapMode::Extend),
                        )
                        .on_hover_text("Drop this channel's cached data; row references stay")
                        .clicked()
                    {
                        response.action = Some(CacheControlAction::UnloadChannel(
                            channel.channel_path.clone(),
                        ));
                    }
                });
            }
        });

    response
}

fn draw_row_controls(
    ui: &mut egui::Ui,
    view: &mut ViewState,
    schema: Option<&parquet_schema::SchemaSummary>,
    loading_channel_paths: &BTreeSet<String>,
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
        let row_has_loading_channel = row
            .channels
            .iter()
            .any(|channel| loading_channel_paths.contains(&channel.channel_path));
        ui.horizontal(|ui| {
            let row_label = format!("Row {}", row_index + 1);
            if ui
                .selectable_label(selected_row_id == Some(row.id), row_label)
                .clicked()
            {
                selected_row_id = Some(row.id);
            }
            if ui
                .add_enabled(
                    can_delete_row && !row_has_loading_channel,
                    egui::Button::new("Delete"),
                )
                .clicked()
            {
                remove_row_id = Some(row.id);
            }
        });

        if draw_y_range_controls(ui, row.id, &mut row.y_range) {
            changed = true;
        }

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
                        let channel_loading = loading_channel_paths.contains(&channel.channel_path);
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

                        let label = if channel_loading {
                            format!(
                                "{} (loading)",
                                channel_display_name(schema, &channel.channel_path)
                            )
                        } else {
                            channel_display_name(schema, &channel.channel_path)
                        };
                        ui.label(label);
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

                        if ui
                            .add_enabled(!channel_loading, egui::Button::new("Remove"))
                            .clicked()
                        {
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

fn draw_y_range_controls(ui: &mut egui::Ui, row_id: u64, y_range: &mut RowYRange) -> bool {
    let mut changed = false;

    ui.push_id(("row_y_range", row_id), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Y");
            let mut selected_mode = y_range.mode;
            egui::ComboBox::from_id_salt("mode")
                .selected_text(selected_mode.as_str())
                .show_ui(ui, |ui| {
                    for mode in YRangeMode::ALL {
                        if ui
                            .selectable_value(&mut selected_mode, mode, mode.as_str())
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            if selected_mode != y_range.mode {
                match selected_mode {
                    YRangeMode::Auto => y_range.mode = YRangeMode::Auto,
                    YRangeMode::Manual => y_range.set_manual_from_last_auto(),
                }
                changed = true;
            }

            if y_range.mode == YRangeMode::Manual {
                let mut min = y_range.min;
                if ui
                    .add(egui::DragValue::new(&mut min).speed(0.1).prefix("min "))
                    .changed()
                {
                    y_range.min = min;
                    changed = true;
                }

                let mut max = y_range.max;
                if ui
                    .add(egui::DragValue::new(&mut max).speed(0.1).prefix("max "))
                    .changed()
                {
                    y_range.max = max;
                    changed = true;
                }

                if ui.small_button("Reset").clicked() {
                    y_range.set_manual_from_last_auto();
                    changed = true;
                }
            }
        });
    });

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
    y_range: RowYRange,
) {
    let Some(first) = visible_traces.first() else {
        draw_status_label(painter, rect, "No channel loaded");
        return;
    };
    let Some((time_min, time_max)) = trace_time_range(first) else {
        draw_status_label(painter, rect, "No time range available");
        return;
    };

    let Some(auto_value_range) = combined_trace_value_range(visible_traces) else {
        draw_status_label(painter, rect, "No finite values available");
        return;
    };

    let time_span = time_max - time_min;
    if !time_span.is_finite() || time_span <= 0.0 {
        draw_status_label(painter, rect, "Invalid time range");
        return;
    }

    let Some((value_min, value_max)) = display_value_range(auto_value_range, y_range) else {
        draw_status_label(painter, rect, "Invalid value range");
        return;
    };
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

    let trace_painter = painter.with_clip_rect(rect);
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
            VisibleTraceData::Envelope(envelope)
            | VisibleTraceData::DenseStepEnvelope(envelope) => {
                let mut upper = Vec::with_capacity(envelope.buckets.len());
                let mut lower = Vec::with_capacity(envelope.buckets.len());
                for bucket in &envelope.buckets {
                    let min_point = to_screen(bucket.time, f64::from(bucket.min));
                    let max_point = to_screen(bucket.time, f64::from(bucket.max));
                    trace_painter.line_segment([min_point, max_point], vertical_stroke);
                    lower.push(min_point);
                    upper.push(max_point);
                }

                if upper.len() >= 2 {
                    trace_painter.line(upper, line_stroke);
                    trace_painter.line(lower, line_stroke);
                }
            }
            VisibleTraceData::RawStep(raw_step) => {
                draw_raw_step_trace(
                    &trace_painter,
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
        VisibleTraceData::Envelope(envelope) | VisibleTraceData::DenseStepEnvelope(envelope) => {
            envelope.time_range
        }
        VisibleTraceData::RawStep(raw_step) => raw_step.time_range,
    }
}

fn trace_value_range(trace: &VisibleTrace) -> Option<(f64, f64)> {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) | VisibleTraceData::DenseStepEnvelope(envelope) => {
            envelope.value_range
        }
        VisibleTraceData::RawStep(raw_step) => raw_step.value_range,
    }
}

fn combined_trace_value_range(visible_traces: &[VisibleTrace]) -> Option<(f64, f64)> {
    let mut combined_value_range = None;
    for visible in visible_traces {
        if let Some((min, max)) = trace_value_range(visible) {
            combined_value_range = extend_range(combined_value_range, min);
            combined_value_range = extend_range(combined_value_range, max);
        }
    }

    combined_value_range
}

fn display_value_range(auto_value_range: (f64, f64), y_range: RowYRange) -> Option<(f64, f64)> {
    match y_range.mode {
        YRangeMode::Auto => Some(padded_range(auto_value_range.0, auto_value_range.1)),
        YRangeMode::Manual => normalized_y_range(y_range.min, y_range.max),
    }
}

fn normalized_y_range(min: f64, max: f64) -> Option<(f64, f64)> {
    if !min.is_finite() || !max.is_finite() {
        return None;
    }

    if min == max {
        return Some(padded_range(min, max));
    }

    Some((min.min(max), min.max(max)))
}

fn trace_source_sample_count(trace: &VisibleTrace) -> usize {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) | VisibleTraceData::DenseStepEnvelope(envelope) => {
            envelope.source_sample_count
        }
        VisibleTraceData::RawStep(raw_step) => raw_step.source_sample_count,
    }
}

fn trace_bucket_count(trace: &VisibleTrace) -> usize {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) | VisibleTraceData::DenseStepEnvelope(envelope) => {
            envelope.bucket_count()
        }
        VisibleTraceData::RawStep(_) => 0,
    }
}

fn trace_draw_point_count(trace: &VisibleTrace) -> usize {
    match &trace.data {
        VisibleTraceData::Envelope(envelope) | VisibleTraceData::DenseStepEnvelope(envelope) => {
            envelope.draw_point_count()
        }
        VisibleTraceData::RawStep(raw_step) => raw_step.samples.len().saturating_mul(2),
    }
}

fn trace_step_kind(trace: &VisibleTrace) -> Option<StepTraceKind> {
    match &trace.data {
        VisibleTraceData::RawStep(raw_step) => Some(raw_step.kind),
        VisibleTraceData::Envelope(_) | VisibleTraceData::DenseStepEnvelope(_) => None,
    }
}

fn visible_envelope_bucket_count(rect: egui::Rect) -> usize {
    (rect.width().round() as usize)
        .clamp(MIN_VISIBLE_ENVELOPE_BUCKETS, MAX_VISIBLE_ENVELOPE_BUCKETS)
}

fn preview_envelope_bucket_count(requested_bucket_count: usize) -> usize {
    (requested_bucket_count / PREVIEW_BUCKET_DIVISOR)
        .max(MIN_VISIBLE_ENVELOPE_BUCKETS)
        .min(MAX_PREVIEW_ENVELOPE_BUCKETS)
}

fn preview_needed_for_workload(
    dataset_sample_count: usize,
    visible_source_sample_count: usize,
    line_channel_count: usize,
    step_channel_count: usize,
) -> bool {
    if dataset_sample_count < PREVIEW_MIN_DATASET_SAMPLES {
        return false;
    }

    let line_sample_work = visible_source_sample_count.saturating_mul(line_channel_count);
    line_sample_work >= PREVIEW_MIN_VISIBLE_LINE_SAMPLE_WORK
        || (step_channel_count > 0
            && visible_source_sample_count >= PREVIEW_MIN_VISIBLE_STEP_SAMPLES)
}

fn should_use_line_tile_lod(plan: &parquet_waveform::MinMaxEnvelopePlan) -> bool {
    plan.source_sample_count >= LINE_TILE_MIN_SOURCE_SAMPLES
        && plan.bucket_size >= LINE_TILE_MIN_BUCKET_SIZE
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
    let dense_step_count = visible_traces
        .iter()
        .filter(|visible| matches!(&visible.data, VisibleTraceData::DenseStepEnvelope(_)))
        .count();

    painter.text(
        rect.left_top() + egui::vec2(0.0, -16.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{}  ch={}  step={}  edge={}  dense={}  visible_samples={}  raw_samples={}  buckets={}  draw_points={}",
            channel_label,
            visible_traces.len(),
            step_count,
            edge_step_count,
            dense_step_count,
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

fn draw_hover_line(
    painter: &egui::Painter,
    rect: egui::Rect,
    hover_x: Option<f64>,
    time_range: Option<(f64, f64)>,
    visuals: &egui::Visuals,
) {
    let Some(hover_x) = hover_x else {
        return;
    };
    let Some(time_range) = time_range else {
        return;
    };
    let Some(x) = plot_x_for_time(hover_x, rect, time_range) else {
        return;
    };

    let stroke = egui::Stroke::new(1.25, visuals.selection.stroke.color);
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        stroke,
    );
}

fn draw_hover_readout(
    painter: &egui::Painter,
    rect: egui::Rect,
    hover_x: Option<f64>,
    visuals: &egui::Visuals,
    visible_traces: &[VisibleTrace],
) {
    let Some(hover_x) = hover_x else {
        return;
    };
    let Some(hover_line_x) = visible_traces
        .first()
        .and_then(trace_time_range)
        .and_then(|time_range| plot_x_for_time(hover_x, rect, time_range))
    else {
        return;
    };

    let values = visible_traces
        .iter()
        .filter_map(|trace| trace.hover_value.map(|value| (trace, value)))
        .take(MAX_HOVER_READOUT_CHANNELS)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return;
    }

    let extra_count = visible_traces
        .iter()
        .filter(|trace| trace.hover_value.is_some())
        .count()
        .saturating_sub(values.len());
    let mut lines = Vec::with_capacity(values.len() + 2);
    lines.push(format!("x {:.6} s", hover_x));
    for (trace, value) in &values {
        lines.push(format!(
            "{}  {}",
            compact_channel_label(trace_label(trace), 24),
            format_hover_value(*value)
        ));
    }
    if extra_count > 0 {
        lines.push(format!("+{extra_count} more"));
    }

    let font = egui::FontId::monospace(11.0);
    let line_height = 15.0;
    let max_panel_width = (rect.width() - 8.0).max(64.0).min(340.0);
    let panel_width = lines
        .iter()
        .map(|line| line.chars().count() as f32)
        .fold(0.0, f32::max)
        .mul_add(7.2, 18.0)
        .clamp(64.0, max_panel_width);
    let panel_height = lines.len() as f32 * line_height + 12.0;
    let panel_left = if hover_line_x < rect.center().x {
        rect.right() - panel_width - 8.0
    } else {
        rect.left() + 8.0
    }
    .max(rect.left() + 4.0)
    .min(rect.right() - panel_width - 4.0);
    let panel_top = rect.top() + 8.0;
    let panel_rect = egui::Rect::from_min_size(
        egui::pos2(panel_left, panel_top),
        egui::vec2(panel_width, panel_height),
    );
    let fill = if visuals.dark_mode {
        egui::Color32::from_black_alpha(190)
    } else {
        egui::Color32::from_white_alpha(220)
    };
    painter.rect_filled(panel_rect, 3.0, fill);
    painter.rect_stroke(
        panel_rect,
        3.0,
        egui::Stroke::new(1.0, visuals.widgets.noninteractive.fg_stroke.color),
        egui::StrokeKind::Inside,
    );

    let text_color = visuals.text_color();
    let mut y = panel_rect.top() + 6.0;
    painter.text(
        egui::pos2(panel_rect.left() + 8.0, y),
        egui::Align2::LEFT_TOP,
        &lines[0],
        font.clone(),
        text_color,
    );
    y += line_height;

    for ((trace, _), line) in values.iter().zip(lines.iter().skip(1)) {
        let swatch_y = y + line_height * 0.5;
        painter.line_segment(
            [
                egui::pos2(panel_rect.left() + 8.0, swatch_y),
                egui::pos2(panel_rect.left() + 18.0, swatch_y),
            ],
            egui::Stroke::new(2.0, trace.color),
        );
        painter.text(
            egui::pos2(panel_rect.left() + 24.0, y),
            egui::Align2::LEFT_TOP,
            line,
            font.clone(),
            text_color,
        );
        y += line_height;
    }

    if extra_count > 0
        && let Some(extra_line) = lines.last()
    {
        painter.text(
            egui::pos2(panel_rect.left() + 24.0, y),
            egui::Align2::LEFT_TOP,
            extra_line,
            font,
            visuals.weak_text_color(),
        );
    }
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

fn hover_value_at_time(
    time: &[f64],
    values: &[f32],
    target_time: f64,
    draw_mode: DrawMode,
) -> Option<f32> {
    match draw_mode {
        DrawMode::Line => nearest_sample_value(time, values, target_time),
        DrawMode::Step => step_sample_value_at_time(time, values, target_time),
    }
}

fn nearest_sample_value(time: &[f64], values: &[f32], target_time: f64) -> Option<f32> {
    if !target_time.is_finite() {
        return None;
    }

    let sample_count = time.len().min(values.len());
    if sample_count == 0 {
        return None;
    }

    let insertion_index =
        time[..sample_count].partition_point(|sample_time| *sample_time < target_time);
    let candidates = [
        insertion_index.checked_sub(1),
        (insertion_index < sample_count).then_some(insertion_index),
    ];
    let mut best = None;

    for index in candidates.into_iter().flatten() {
        let sample_time = time[index];
        let value = values[index];
        if !sample_time.is_finite() || !value.is_finite() {
            continue;
        }

        let distance = (sample_time - target_time).abs();
        match best {
            Some((best_distance, _)) if best_distance <= distance => {}
            _ => best = Some((distance, value)),
        }
    }

    best.map(|(_, value)| value)
}

fn step_sample_value_at_time(time: &[f64], values: &[f32], target_time: f64) -> Option<f32> {
    if !target_time.is_finite() {
        return None;
    }

    let sample_count = time.len().min(values.len());
    if sample_count == 0 {
        return None;
    }

    let index = time[..sample_count]
        .partition_point(|sample_time| *sample_time <= target_time)
        .saturating_sub(1)
        .min(sample_count - 1);
    let sample_time = time[index];
    let value = values[index];
    (sample_time.is_finite() && value.is_finite()).then_some(value)
}

fn time_at_plot_x(x: f32, rect: egui::Rect, time_range: (f64, f64)) -> Option<f64> {
    let (start, end) = normalized_range(time_range)?;
    if rect.width() <= 1.0 {
        return None;
    }

    let ratio = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
    Some(start + (end - start) * ratio)
}

fn plot_x_for_time(time: f64, rect: egui::Rect, time_range: (f64, f64)) -> Option<f32> {
    let (start, end) = normalized_range(time_range)?;
    if !time.is_finite() || rect.width() <= 1.0 {
        return None;
    }

    let ratio = ((time - start) / (end - start)) as f32;
    if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
        return None;
    }

    Some(egui::lerp(rect.left()..=rect.right(), ratio))
}

fn trace_label(trace: &VisibleTrace) -> &str {
    if trace.channel_name.is_empty() {
        trace.channel_path.as_str()
    } else {
        trace.channel_name.as_str()
    }
}

fn compact_channel_label(label: &str, max_chars: usize) -> String {
    let char_count = label.chars().count();
    if char_count <= max_chars {
        return label.to_owned();
    }

    if max_chars <= 3 {
        return label.chars().take(max_chars).collect();
    }

    let mut compact = label.chars().take(max_chars - 3).collect::<String>();
    compact.push_str("...");
    compact
}

fn format_hover_value(value: f32) -> String {
    let value = f64::from(value);
    let abs = value.abs();
    if value == 0.0 || (1.0e-3..1.0e5).contains(&abs) {
        format!("{value:.6}")
    } else {
        format!("{value:.6e}")
    }
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

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
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

fn visible_sample_range(
    time: &[f64],
    value_count: usize,
    time_range: (f64, f64),
) -> Option<VisibleSampleRange> {
    let (range_start, range_end) = normalized_range(time_range)?;
    let sample_count = time.len().min(value_count);
    if sample_count == 0 {
        return None;
    }

    let time = &time[..sample_count];
    let start = time.partition_point(|time| *time < range_start);
    let end = time.partition_point(|time| *time <= range_end);
    let context_start = start.saturating_sub(usize::from(start > 0));
    Some(VisibleSampleRange {
        range_start,
        range_end,
        start,
        end,
        context_start,
        source_sample_count: end.saturating_sub(start),
        draw_sample_count: end.saturating_sub(context_start),
    })
}

fn build_raw_step_trace(
    time: &[f64],
    values: &[f32],
    time_range: (f64, f64),
    max_samples: usize,
) -> Option<RawStepTrace> {
    let sample_range = visible_sample_range(time, values.len(), time_range)?;
    if sample_range.draw_sample_count == 0 || sample_range.draw_sample_count > max_samples {
        return None;
    }

    let mut samples = Vec::with_capacity(sample_range.draw_sample_count);
    let mut value_range = None;

    if sample_range.context_start < sample_range.start {
        let value = values[sample_range.context_start];
        if value.is_finite() {
            samples.push(StepSample {
                time: sample_range.range_start,
                value,
            });
            value_range = extend_range(value_range, f64::from(value));
        }
    }

    for index in sample_range.start..sample_range.end {
        let sample_time = time[index];
        let value = values[index];
        if !sample_time.is_finite() || !value.is_finite() {
            continue;
        }

        samples.push(StepSample {
            time: sample_time.clamp(sample_range.range_start, sample_range.range_end),
            value,
        });
        value_range = extend_range(value_range, f64::from(value));
    }

    if samples.is_empty() || value_range.is_none() {
        return None;
    }

    Some(RawStepTrace {
        samples,
        source_sample_count: sample_range.source_sample_count,
        time_range: Some((sample_range.range_start, sample_range.range_end)),
        value_range,
        kind: StepTraceKind::RawSamples,
    })
}

#[cfg(test)]
fn build_change_point_step_trace(
    time: &[f64],
    values: &[f32],
    time_range: (f64, f64),
    max_change_points: usize,
) -> Option<RawStepTrace> {
    match build_change_point_step_trace_result(time, values, time_range, max_change_points) {
        StepTraceBuildResult::Trace(trace) => Some(trace),
        StepTraceBuildResult::TooManyChangePoints { .. } | StepTraceBuildResult::Empty => None,
    }
}

#[cfg(test)]
fn build_change_point_step_trace_result(
    time: &[f64],
    values: &[f32],
    time_range: (f64, f64),
    max_change_points: usize,
) -> StepTraceBuildResult {
    let Some(sample_range) = visible_sample_range(time, values.len(), time_range) else {
        return StepTraceBuildResult::Empty;
    };
    if sample_range.draw_sample_count == 0 {
        return StepTraceBuildResult::Empty;
    }

    let mut samples = Vec::new();
    let mut value_range = None;
    let mut previous_value = None;
    let mut change_points = 0usize;

    if sample_range.context_start < sample_range.start {
        let value = values[sample_range.context_start];
        if value.is_finite() {
            samples.push(StepSample {
                time: sample_range.range_start,
                value,
            });
            value_range = extend_range(value_range, f64::from(value));
            previous_value = Some(value);
        }
    }

    for index in sample_range.start..sample_range.end {
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
                    return StepTraceBuildResult::TooManyChangePoints {
                        source_sample_count: sample_range.source_sample_count,
                    };
                }
                samples.push(StepSample {
                    time: sample_time.clamp(sample_range.range_start, sample_range.range_end),
                    value,
                });
                previous_value = Some(value);
                change_points += 1;
            }
            None => {
                samples.push(StepSample {
                    time: sample_time.clamp(sample_range.range_start, sample_range.range_end),
                    value,
                });
                previous_value = Some(value);
            }
        }
    }

    if samples.is_empty() || value_range.is_none() {
        return StepTraceBuildResult::Empty;
    }

    StepTraceBuildResult::Trace(RawStepTrace {
        samples,
        source_sample_count: sample_range.source_sample_count,
        time_range: Some((sample_range.range_start, sample_range.range_end)),
        value_range,
        kind: StepTraceKind::ChangePoints,
    })
}

fn build_step_edge_cache(time: &[f64], values: &[f32]) -> StepEdgeCache {
    let started = Instant::now();
    let sample_count = time.len().min(values.len());
    let mut edges = Vec::new();
    let mut previous_value = None;

    for index in 0..sample_count {
        let sample_time = time[index];
        let value = values[index];
        if !sample_time.is_finite() || !value.is_finite() {
            continue;
        }

        match previous_value {
            Some(previous) if previous == value => {}
            _ => {
                edges.push(StepEdge {
                    index,
                    time: sample_time,
                    value,
                });
                previous_value = Some(value);
            }
        }
    }

    StepEdgeCache {
        sample_count,
        edges,
        elapsed: started.elapsed(),
    }
}

fn build_change_point_step_trace_from_cache(
    time: &[f64],
    edge_cache: &StepEdgeCache,
    time_range: (f64, f64),
    max_change_points: usize,
) -> StepTraceBuildResult {
    let Some(sample_range) = visible_sample_range(time, edge_cache.sample_count, time_range) else {
        return StepTraceBuildResult::Empty;
    };
    if sample_range.draw_sample_count == 0 || edge_cache.edges.is_empty() {
        return StepTraceBuildResult::Empty;
    }

    let edge_start = edge_cache
        .edges
        .partition_point(|edge| edge.time < sample_range.range_start);
    let edge_end = edge_cache
        .edges
        .partition_point(|edge| edge.time <= sample_range.range_end);
    let visible_edge_count = edge_end.saturating_sub(edge_start);
    let has_context = edge_start > 0;
    let change_points = if has_context {
        visible_edge_count
    } else {
        visible_edge_count.saturating_sub(1)
    };

    if change_points > max_change_points {
        return StepTraceBuildResult::TooManyChangePoints {
            source_sample_count: sample_range.source_sample_count,
        };
    }

    let mut samples = Vec::with_capacity(visible_edge_count + usize::from(has_context));
    let mut value_range = None;

    if has_context {
        let edge = edge_cache.edges[edge_start - 1];
        samples.push(StepSample {
            time: sample_range.range_start,
            value: edge.value,
        });
        value_range = extend_range(value_range, f64::from(edge.value));
    }

    for edge in &edge_cache.edges[edge_start..edge_end] {
        samples.push(StepSample {
            time: edge
                .time
                .clamp(sample_range.range_start, sample_range.range_end),
            value: edge.value,
        });
        value_range = extend_range(value_range, f64::from(edge.value));
    }

    if samples.is_empty() || value_range.is_none() {
        return StepTraceBuildResult::Empty;
    }

    StepTraceBuildResult::Trace(RawStepTrace {
        samples,
        source_sample_count: sample_range.source_sample_count,
        time_range: Some((sample_range.range_start, sample_range.range_end)),
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

fn x_zoom_factor(zoom_delta: f32, scroll_delta_y: f32, zoom_modifier_pressed: bool) -> f64 {
    if zoom_delta != 1.0 {
        1.0 / f64::from(zoom_delta)
    } else if zoom_modifier_pressed && scroll_delta_y != 0.0 {
        (-f64::from(scroll_delta_y) * WHEEL_ZOOM_SENSITIVITY).exp()
    } else {
        1.0
    }
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

fn overscan_cache_range(view_range: (f64, f64), full_range: (f64, f64)) -> Option<(f64, f64)> {
    let (view_start, view_end) = normalized_range(view_range)?;
    let (full_start, full_end) = normalized_range(full_range)?;
    let view_span = view_end - view_start;
    if view_span <= 0.0 {
        return None;
    }

    let overscan = view_span * ENVELOPE_OVERSCAN_RATIO;
    let start = (view_start - overscan).max(full_start);
    let end = (view_end + overscan).min(full_end);
    normalized_range((start, end))
}

fn cache_bucket_count_for_view(
    view_range: (f64, f64),
    cache_range: (f64, f64),
    requested_bucket_count: usize,
) -> usize {
    let Some((view_start, view_end)) = normalized_range(view_range) else {
        return requested_bucket_count.max(1);
    };
    let Some((cache_start, cache_end)) = normalized_range(cache_range) else {
        return requested_bucket_count.max(1);
    };
    let view_span = view_end - view_start;
    let cache_span = cache_end - cache_start;
    if view_span <= 0.0 || cache_span <= 0.0 {
        return requested_bucket_count.max(1);
    }

    let scale = (cache_span / view_span).max(1.0);
    (((requested_bucket_count.max(1) as f64) * scale).ceil() as usize)
        .clamp(requested_bucket_count.max(1), MAX_CACHED_ENVELOPE_BUCKETS)
}

fn similar_span(left: f64, right: f64, tolerance: f64) -> bool {
    if !left.is_finite() || !right.is_finite() || left <= 0.0 || right <= 0.0 {
        return false;
    }

    let ratio = left.min(right) / left.max(right);
    ratio >= 1.0 - tolerance
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

    fn test_channel_info(index: usize, name: &str) -> parquet_schema::ColumnInfo {
        parquet_schema::ColumnInfo {
            index,
            name: name.to_owned(),
            path: name.to_owned(),
            physical_type: "FLOAT".to_owned(),
            logical_type: None,
            converted_type: "NONE".to_owned(),
            is_numeric: true,
            role: parquet_schema::ColumnRole::Channel,
        }
    }

    fn test_schema(channels: &[&str]) -> parquet_schema::SchemaSummary {
        let channel_infos = channels
            .iter()
            .enumerate()
            .map(|(index, name)| test_channel_info(index + 1, name))
            .collect::<Vec<_>>();
        let time_column = parquet_schema::ColumnInfo {
            index: 0,
            name: "time".to_owned(),
            path: "time".to_owned(),
            physical_type: "DOUBLE".to_owned(),
            logical_type: None,
            converted_type: "NONE".to_owned(),
            is_numeric: true,
            role: parquet_schema::ColumnRole::Time,
        };
        let mut columns = vec![time_column.clone()];
        columns.extend(channel_infos.clone());

        parquet_schema::SchemaSummary {
            path: PathBuf::from("/tmp/test.parquet"),
            row_count: 4,
            row_group_count: 1,
            column_count: columns.len(),
            created_by: None,
            time_column: Some(time_column),
            channels: channel_infos,
            columns,
        }
    }

    fn test_channel_data(channel_path: &str) -> parquet_waveform::ChannelData {
        parquet_waveform::ChannelData {
            path: PathBuf::new(),
            channel_name: channel_path.to_owned(),
            channel_path: channel_path.to_owned(),
            values: vec![0.0, 1.0, 0.0],
            projected_column_index: 1,
            elapsed: Duration::ZERO,
        }
    }

    fn empty_line_tile_cache() -> parquet_waveform::LineTileCache {
        parquet_waveform::LineTileCache {
            tile_size: 2,
            sample_count: 3,
            tiles: Vec::new(),
            elapsed: Duration::ZERO,
        }
    }

    fn empty_min_max_envelope() -> parquet_waveform::MinMaxEnvelope {
        parquet_waveform::MinMaxEnvelope {
            buckets: Vec::new(),
            source_sample_count: 0,
            requested_bucket_count: 10,
            bucket_size: 0,
            time_range: None,
            value_range: None,
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn adds_channels_to_the_selected_row() {
        let mut view = ViewState::default();

        let (added_first, first_row_id) = view.add_channel_to_selected_row("sine_50Hz");
        assert!(added_first);
        assert_eq!(first_row_id, 0);
        assert_eq!(view.rows[0].channels.len(), 1);
        assert_eq!(view.rows[0].y_range.mode, YRangeMode::Auto);
        assert_eq!(view.rows[0].channels[0].draw_mode, DrawMode::Line);
        assert!(view.rows[0].channels[0].visible);
        assert_eq!(
            view.rows[0].channels[0].line_width,
            DEFAULT_TRACE_LINE_WIDTH
        );
        assert_eq!(view.rows[0].channels[0].color_override, None);

        let second_row_id = view.add_row();
        assert_eq!(view.rows[1].y_range, RowYRange::default());
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
    fn session_preset_restores_lightweight_view_state() {
        let schema = test_schema(&["sine_50Hz", "pwm_1kHz"]);
        let mut view = ViewState {
            selected_channel: "pwm_1kHz".to_owned(),
            x_range: Some((1.25, 2.5)),
            large_preview_enabled: true,
            ..ViewState::default()
        };
        let (added_first, _) = view.add_channel_to_selected_row("sine_50Hz");
        assert!(added_first);
        view.rows[0].y_range = RowYRange {
            mode: YRangeMode::Manual,
            min: -2.0,
            max: 3.0,
            last_auto: Some((-1.0, 1.0)),
        };
        view.rows[0].channels[0].visible = false;
        view.rows[0].channels[0].draw_mode = DrawMode::Step;
        view.rows[0].channels[0].color_override = Some(egui::Color32::from_rgb(10, 20, 30));
        view.rows[0].channels[0].line_width = 2.5;
        let second_row_id = view.add_row();
        let (added_second, _) = view.add_channel_to_selected_row("pwm_1kHz");
        assert!(added_second);

        let preset = SessionPreset::capture("/tmp/test.parquet", &view);
        let mut restored = ViewState::default();
        let report = preset.apply_to_view(&mut restored, &schema);

        assert_eq!(report.restored_rows, 2);
        assert_eq!(report.restored_channels, 2);
        assert!(report.missing_channels.is_empty());
        assert_eq!(restored.x_range, Some((1.25, 2.5)));
        assert!(restored.large_preview_enabled);
        assert_eq!(restored.selected_channel, "pwm_1kHz");
        assert_eq!(restored.selected_row_id, Some(second_row_id));
        assert_eq!(restored.rows[0].y_range.mode, YRangeMode::Manual);
        assert_eq!(
            (restored.rows[0].y_range.min, restored.rows[0].y_range.max),
            (-2.0, 3.0)
        );
        assert!(!restored.rows[0].channels[0].visible);
        assert_eq!(restored.rows[0].channels[0].draw_mode, DrawMode::Step);
        assert_eq!(
            restored.rows[0].channels[0].color_override,
            Some(egui::Color32::from_rgb(10, 20, 30))
        );
        assert_eq!(restored.rows[0].channels[0].line_width, 2.5);
    }

    #[test]
    fn session_preset_skips_channels_missing_from_schema() {
        let original_schema = test_schema(&["sine_50Hz", "missing_later"]);
        let target_schema = test_schema(&["sine_50Hz"]);
        let mut view = ViewState::default();
        let (added_first, _) = view.add_channel_to_selected_row("sine_50Hz");
        assert!(added_first);
        let (added_second, _) = view.add_channel_to_selected_row("missing_later");
        assert!(added_second);

        let preset = SessionPreset::capture("/tmp/test.parquet", &view);
        let mut restored = ViewState::default();
        let report = preset.apply_to_view(&mut restored, &target_schema);

        assert_eq!(original_schema.channels.len(), 2);
        assert_eq!(report.restored_rows, 1);
        assert_eq!(report.restored_channels, 1);
        assert_eq!(report.missing_channels, vec!["missing_later".to_owned()]);
        assert_eq!(restored.rows[0].channels.len(), 1);
        assert_eq!(restored.rows[0].channels[0].channel_path, "sine_50Hz");
    }

    #[test]
    fn converts_plot_x_to_time_with_clamping() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 0.0), egui::vec2(100.0, 50.0));

        assert_eq!(time_at_plot_x(10.0, rect, (2.0, 12.0)), Some(2.0));
        assert_eq!(time_at_plot_x(60.0, rect, (2.0, 12.0)), Some(7.0));
        assert_eq!(time_at_plot_x(120.0, rect, (2.0, 12.0)), Some(12.0));
    }

    #[test]
    fn line_hover_uses_nearest_sample_value() {
        let time = [0.0, 1.0, 2.0, 3.0];
        let values = [0.0, 10.0, 20.0, 30.0];

        assert_eq!(
            hover_value_at_time(&time, &values, 1.6, DrawMode::Line),
            Some(20.0)
        );
        assert_eq!(
            hover_value_at_time(&time, &values, 1.5, DrawMode::Line),
            Some(10.0)
        );
    }

    #[test]
    fn step_hover_uses_held_sample_value() {
        let time = [0.0, 1.0, 2.0, 3.0];
        let values = [0.0, 10.0, 20.0, 30.0];

        assert_eq!(
            hover_value_at_time(&time, &values, 1.9, DrawMode::Step),
            Some(10.0)
        );
        assert_eq!(
            hover_value_at_time(&time, &values, 2.0, DrawMode::Step),
            Some(20.0)
        );
    }

    #[test]
    fn auto_y_range_uses_padded_trace_range() {
        let range = display_value_range((1.0, 3.0), RowYRange::default()).expect("auto y range");

        assert!((range.0 - 0.9).abs() < 1.0e-12);
        assert!((range.1 - 3.1).abs() < 1.0e-12);
    }

    #[test]
    fn manual_y_range_uses_row_values_without_padding() {
        let range = display_value_range(
            (-10.0, 10.0),
            RowYRange {
                mode: YRangeMode::Manual,
                min: -2.5,
                max: 7.5,
                ..RowYRange::default()
            },
        )
        .expect("manual y range");

        assert_eq!(range, (-2.5, 7.5));
    }

    #[test]
    fn manual_y_range_accepts_reversed_inputs() {
        let range = display_value_range(
            (-10.0, 10.0),
            RowYRange {
                mode: YRangeMode::Manual,
                min: 5.0,
                max: -1.0,
                ..RowYRange::default()
            },
        )
        .expect("manual y range");

        assert_eq!(range, (-1.0, 5.0));
    }

    #[test]
    fn manual_y_range_seeds_from_last_auto_range() {
        let mut y_range = RowYRange::default();
        y_range.set_last_auto((-2.0, 4.0));

        y_range.set_manual_from_last_auto();

        assert_eq!(y_range.mode, YRangeMode::Manual);
        assert_eq!((y_range.min, y_range.max), (-2.0, 4.0));
    }

    #[test]
    fn manual_y_range_seed_falls_back_to_default_without_auto_range() {
        let mut y_range = RowYRange::default();

        y_range.set_manual_from_last_auto();

        assert_eq!(y_range.mode, YRangeMode::Manual);
        assert_eq!((y_range.min, y_range.max), (-1.0, 1.0));
    }

    #[test]
    fn manual_y_range_reset_uses_latest_auto_range() {
        let mut y_range = RowYRange::default();
        y_range.set_last_auto((-2.0, 4.0));
        y_range.set_manual_from_last_auto();
        y_range.min = -10.0;
        y_range.max = 10.0;
        y_range.set_last_auto((-0.5, 0.5));

        y_range.set_manual_from_last_auto();

        assert_eq!(y_range.mode, YRangeMode::Manual);
        assert_eq!((y_range.min, y_range.max), (-0.5, 0.5));
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

    #[test]
    fn change_point_step_trace_reports_dense_fallback() {
        let time = [0.0, 1.0, 2.0, 3.0, 4.0];
        let values = [0.0, 1.0, 0.0, 1.0, 0.0];

        let result = build_change_point_step_trace_result(&time, &values, (0.0, 4.0), 2);

        assert_eq!(
            result,
            StepTraceBuildResult::TooManyChangePoints {
                source_sample_count: 5
            }
        );
    }

    #[test]
    fn step_edge_cache_trace_preserves_edges_by_range_lookup() {
        let time = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let values = [0.0, 0.0, 0.0, 5.0, 5.0, 0.0, 0.0];
        let cache = build_step_edge_cache(&time, &values);

        let result = build_change_point_step_trace_from_cache(&time, &cache, (0.0, 6.0), 3);
        let StepTraceBuildResult::Trace(trace) = result else {
            panic!("expected cached edge trace");
        };

        assert_eq!(cache.edge_count(), 3);
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
    fn step_edge_cache_trace_carries_previous_state_at_range_start() {
        let time = [0.0, 1.0, 2.0, 3.0, 4.0];
        let values = [0.0, 5.0, 5.0, 0.0, 0.0];
        let cache = build_step_edge_cache(&time, &values);

        let result = build_change_point_step_trace_from_cache(&time, &cache, (2.5, 4.0), 3);
        let StepTraceBuildResult::Trace(trace) = result else {
            panic!("expected cached edge trace");
        };

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
    fn step_edge_cache_trace_reports_dense_fallback() {
        let time = [0.0, 1.0, 2.0, 3.0, 4.0];
        let values = [0.0, 1.0, 0.0, 1.0, 0.0];
        let cache = build_step_edge_cache(&time, &values);

        let result = build_change_point_step_trace_from_cache(&time, &cache, (0.0, 4.0), 2);

        assert_eq!(
            result,
            StepTraceBuildResult::TooManyChangePoints {
                source_sample_count: 5
            }
        );
    }

    #[test]
    fn channel_store_reuses_step_edge_cache() {
        let time = [0.0, 1.0, 2.0, 3.0, 4.0];
        let mut channel = test_channel_data("gate_pwm");
        channel.values = vec![0.0, 1.0, 0.0, 1.0, 0.0];
        let mut store = ChannelStore::default();
        store.insert_channel(channel);

        let _ = store.cached_change_point_step_trace_result("gate_pwm", &time, (0.0, 4.0), 10);
        let first_stats = store.last_envelope_stats;
        let _ = store.cached_change_point_step_trace_result("gate_pwm", &time, (1.0, 3.0), 10);
        let second_stats = store.last_envelope_stats;

        assert_eq!(store.step_edge_cache.len(), 1);
        assert_eq!(first_stats.step_edge_builds, 1);
        assert_eq!(first_stats.step_edge_hits, 0);
        assert_eq!(second_stats.step_edge_builds, 1);
        assert_eq!(second_stats.step_edge_hits, 1);
    }

    #[test]
    fn channel_store_unloads_one_channel_and_its_draw_caches() {
        let mut store = ChannelStore::default();
        store.insert_channel(test_channel_data("gate_pwm"));
        store.insert_channel(test_channel_data("sine_50Hz"));
        store
            .line_tile_cache
            .insert("gate_pwm".to_owned(), empty_line_tile_cache());
        store.step_edge_cache.insert(
            "gate_pwm".to_owned(),
            StepEdgeCache {
                sample_count: 3,
                edges: Vec::new(),
                elapsed: Duration::ZERO,
            },
        );
        store.step_fallback_hints.insert(
            "gate_pwm".to_owned(),
            StepFallbackHint {
                min_span: 1.0,
                min_source_sample_count: 3,
            },
        );
        store.envelope_cache.insert(
            EnvelopeKey::new("gate_pwm", (0.0, 1.0), 10),
            empty_min_max_envelope(),
        );

        assert!(store.unload_channel("gate_pwm"));

        assert!(!store.has_channel("gate_pwm"));
        assert!(store.has_channel("sine_50Hz"));
        assert!(store.line_tile_cache.is_empty());
        assert!(store.step_edge_cache.is_empty());
        assert!(store.step_fallback_hints.is_empty());
        assert!(store.envelope_cache.is_empty());
    }

    #[test]
    fn channel_store_clear_draw_caches_keeps_raw_channel_data() {
        let mut store = ChannelStore::default();
        store.insert_channel(test_channel_data("gate_pwm"));
        store
            .line_tile_cache
            .insert("gate_pwm".to_owned(), empty_line_tile_cache());
        store.step_edge_cache.insert(
            "gate_pwm".to_owned(),
            StepEdgeCache {
                sample_count: 3,
                edges: Vec::new(),
                elapsed: Duration::ZERO,
            },
        );
        store.step_fallback_hints.insert(
            "gate_pwm".to_owned(),
            StepFallbackHint {
                min_span: 1.0,
                min_source_sample_count: 3,
            },
        );
        store.envelope_cache.insert(
            EnvelopeKey::new("gate_pwm", (0.0, 1.0), 10),
            empty_min_max_envelope(),
        );

        assert!(store.clear_draw_caches());

        assert!(store.has_channel("gate_pwm"));
        assert!(store.line_tile_cache.is_empty());
        assert!(store.step_edge_cache.is_empty());
        assert!(store.step_fallback_hints.is_empty());
        assert!(store.envelope_cache.is_empty());
        assert!(!store.clear_draw_caches());
    }

    #[test]
    fn unloading_cached_channel_keeps_row_reference() {
        let mut app = PanelYApp::default();
        app.dataset
            .loaded_channels
            .insert_channel(test_channel_data("gate_pwm"));
        let (added, _) = app.view.add_channel_to_selected_row("gate_pwm");
        assert!(added);

        app.unload_cached_channel("gate_pwm");

        assert!(!app.dataset.loaded_channels.has_channel("gate_pwm"));
        assert!(
            app.view.rows[0]
                .channels
                .iter()
                .any(|channel| channel.channel_path == "gate_pwm")
        );
        assert!(app.load.status.contains("row references remain"));
    }

    #[test]
    fn step_fallback_hint_applies_to_same_or_wider_ranges() {
        let mut store = ChannelStore::default();

        store.record_step_change_point_fallback("gate_pwm", (10.0, 20.0), 100_000);

        assert!(store.should_skip_step_change_points("gate_pwm", (10.0, 20.0), 95_000));
        assert!(store.should_skip_step_change_points("gate_pwm", (10.0, 30.0), 150_000));
        assert!(!store.should_skip_step_change_points("gate_pwm", (12.0, 18.0), 60_000));
        assert!(!store.should_skip_step_change_points("other", (10.0, 30.0), 150_000));
    }

    #[test]
    fn preview_bucket_count_is_capped_and_reduced() {
        assert_eq!(
            preview_envelope_bucket_count(100),
            MIN_VISIBLE_ENVELOPE_BUCKETS
        );
        assert_eq!(preview_envelope_bucket_count(1_200), 300);
        assert_eq!(
            preview_envelope_bucket_count(8_192),
            MAX_PREVIEW_ENVELOPE_BUCKETS
        );
    }

    #[test]
    fn preview_workload_gate_excludes_medium_class_data() {
        assert!(!preview_needed_for_workload(2_000_000, 1_000_000, 8, 3));
    }

    #[test]
    fn preview_workload_gate_excludes_small_large_data_windows() {
        assert!(!preview_needed_for_workload(10_000_000, 100_000, 8, 1));
    }

    #[test]
    fn preview_workload_gate_allows_large_wide_ranges() {
        assert!(preview_needed_for_workload(10_000_000, 5_000_000, 8, 1));
    }

    #[test]
    fn preview_is_disabled_by_default() {
        assert!(!ViewState::default().large_preview_enabled);
    }

    #[test]
    fn channel_filter_matches_name_or_path_case_insensitively() {
        let channel = parquet_schema::ColumnInfo {
            index: 3,
            name: "GatePWM".to_owned(),
            path: "motor/gate_pwm".to_owned(),
            physical_type: "FLOAT".to_owned(),
            logical_type: None,
            converted_type: "NONE".to_owned(),
            is_numeric: true,
            role: parquet_schema::ColumnRole::Channel,
        };

        assert!(channel_matches_filter(&channel, ""));
        assert!(channel_matches_filter(&channel, "gate"));
        assert!(channel_matches_filter(&channel, "MOTOR/"));
        assert!(!channel_matches_filter(&channel, "current"));
    }

    #[test]
    fn channel_load_state_prioritizes_loaded_over_loading() {
        let mut store = ChannelStore::default();
        store.insert_channel(parquet_waveform::ChannelData {
            path: PathBuf::new(),
            channel_name: "iu".to_owned(),
            channel_path: "iu".to_owned(),
            values: vec![1.0, 2.0],
            projected_column_index: 1,
            elapsed: Duration::ZERO,
        });
        let mut loading = BTreeSet::new();
        loading.insert("iu".to_owned());
        loading.insert("iv".to_owned());

        assert_eq!(
            channel_load_state("iu", &store, &loading),
            ChannelLoadState::Loaded
        );
        assert_eq!(
            channel_load_state("iv", &store, &loading),
            ChannelLoadState::Loading
        );
        assert_eq!(
            channel_load_state("iw", &store, &loading),
            ChannelLoadState::Unloaded
        );
    }

    #[test]
    fn channel_state_filter_matches_expected_load_states() {
        assert!(ChannelStateFilter::All.matches(ChannelLoadState::Loaded));
        assert!(ChannelStateFilter::All.matches(ChannelLoadState::Loading));
        assert!(ChannelStateFilter::Loaded.matches(ChannelLoadState::Loaded));
        assert!(!ChannelStateFilter::Loaded.matches(ChannelLoadState::Unloaded));
        assert!(ChannelStateFilter::Unloaded.matches(ChannelLoadState::Unloaded));
        assert!(!ChannelStateFilter::Unloaded.matches(ChannelLoadState::Loading));
    }

    #[test]
    fn channel_list_entries_filter_and_sort_channels() {
        let channels = vec![
            test_channel_info(0, "zeta"),
            test_channel_info(1, "alpha"),
            test_channel_info(2, "beta"),
        ];
        let mut store = ChannelStore::default();
        store.insert_channel(parquet_waveform::ChannelData {
            path: PathBuf::new(),
            channel_name: "beta".to_owned(),
            channel_path: "beta".to_owned(),
            values: vec![1.0, 2.0],
            projected_column_index: 2,
            elapsed: Duration::ZERO,
        });
        let mut loading = BTreeSet::new();
        loading.insert("zeta".to_owned());

        let by_name = channel_list_entries(
            &channels,
            "",
            ChannelStateFilter::All,
            ChannelSort::Name,
            &store,
            &loading,
        );
        assert_eq!(
            by_name
                .iter()
                .map(|(channel, _)| channel.path.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "beta", "zeta"]
        );

        let loaded_first = channel_list_entries(
            &channels,
            "",
            ChannelStateFilter::All,
            ChannelSort::LoadedFirst,
            &store,
            &loading,
        );
        assert_eq!(
            loaded_first
                .iter()
                .map(|(channel, state)| (channel.path.as_str(), *state))
                .collect::<Vec<_>>(),
            vec![
                ("beta", ChannelLoadState::Loaded),
                ("zeta", ChannelLoadState::Loading),
                ("alpha", ChannelLoadState::Unloaded),
            ]
        );

        let unloaded = channel_list_entries(
            &channels,
            "a",
            ChannelStateFilter::Unloaded,
            ChannelSort::SchemaOrder,
            &store,
            &loading,
        );
        assert_eq!(
            unloaded
                .iter()
                .map(|(channel, _)| channel.path.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha"]
        );
    }

    #[test]
    fn channel_row_membership_reports_rows_that_contain_channel() {
        let mut view = ViewState::default();
        view.add_channel_to_selected_row("iu");
        view.add_row();
        view.add_channel_to_selected_row("iv");
        view.add_row();
        view.add_channel_to_selected_row("iu");

        assert_eq!(channel_row_membership(&view, "iu"), vec!["Row 1", "Row 3"]);
        assert_eq!(channel_row_membership(&view, "iw"), Vec::<String>::new());
    }

    #[test]
    fn estimated_sample_rate_uses_sample_intervals() {
        assert_eq!(
            estimated_sample_rate_hz(1001, Some((0.0, 1.0))),
            Some(1000.0)
        );
        assert_eq!(estimated_sample_rate_hz(1, Some((0.0, 1.0))), None);
        assert_eq!(estimated_sample_rate_hz(100, Some((1.0, 1.0))), None);
    }

    #[test]
    fn frequency_formatter_scales_units() {
        assert_eq!(format_frequency_hz(500.0), "500.000 Hz");
        assert_eq!(format_frequency_hz(12_500.0), "12.500 kHz");
        assert_eq!(format_frequency_hz(2_500_000.0), "2.500 MHz");
    }

    #[test]
    fn x_zoom_factor_ignores_plain_vertical_wheel() {
        assert_eq!(x_zoom_factor(1.0, 120.0, false), 1.0);
    }

    #[test]
    fn x_zoom_factor_uses_modifier_wheel_or_pinch() {
        let modifier_zoom = x_zoom_factor(1.0, 120.0, true);
        let pinch_zoom = x_zoom_factor(2.0, 0.0, false);

        assert!(modifier_zoom < 1.0);
        assert_eq!(pinch_zoom, 0.5);
    }

    #[test]
    fn parquet_path_validation_accepts_existing_parquet_file() {
        let path =
            std::env::temp_dir().join(format!("panel_y_validation_{}.PARQUET", std::process::id()));
        std::fs::write(&path, b"placeholder").expect("write temp parquet path");

        let result = validate_parquet_path(&path.display().to_string());

        let _ = std::fs::remove_file(&path);
        assert_eq!(result, Ok(path));
    }

    #[test]
    fn parquet_path_validation_rejects_missing_file() {
        let path = std::env::temp_dir().join(format!(
            "panel_y_validation_missing_{}.parquet",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let error = validate_parquet_path(&path.display().to_string()).expect_err("missing path");

        assert!(matches!(error, ParquetPathValidationError::Missing { .. }));
    }

    #[test]
    fn parquet_path_validation_rejects_directory() {
        let error =
            validate_parquet_path(&std::env::temp_dir().display().to_string()).expect_err("dir");

        assert!(matches!(error, ParquetPathValidationError::NotFile { .. }));
    }

    #[test]
    fn parquet_path_validation_rejects_wrong_extension() {
        let path =
            std::env::temp_dir().join(format!("panel_y_validation_{}.csv", std::process::id()));
        std::fs::write(&path, b"placeholder").expect("write temp csv path");

        let error = validate_parquet_path(&path.display().to_string()).expect_err("wrong ext");

        let _ = std::fs::remove_file(&path);
        assert!(matches!(
            error,
            ParquetPathValidationError::WrongExtension { .. }
        ));
    }

    #[test]
    fn recent_parquet_paths_are_deduped_and_limited() {
        let mut recent_paths = Vec::new();
        for index in 0..(MAX_RECENT_PARQUET_PATHS + 2) {
            remember_recent_parquet_path(&mut recent_paths, format!("/tmp/{index}.parquet"));
        }

        assert_eq!(recent_paths.len(), MAX_RECENT_PARQUET_PATHS);
        assert_eq!(recent_paths[0], "/tmp/9.parquet");
        assert!(!recent_paths.iter().any(|path| path == "/tmp/0.parquet"));

        remember_recent_parquet_path(&mut recent_paths, "/tmp/5.parquet");

        assert_eq!(recent_paths.len(), MAX_RECENT_PARQUET_PATHS);
        assert_eq!(recent_paths[0], "/tmp/5.parquet");
        assert_eq!(
            recent_paths
                .iter()
                .filter(|path| path.as_str() == "/tmp/5.parquet")
                .count(),
            1
        );
    }

    #[test]
    fn overscan_envelope_cache_reuses_same_span_pan() {
        let time = (0..100).map(|value| value as f64).collect::<Vec<_>>();
        let values = (0..100).map(|value| value as f32).collect::<Vec<_>>();
        let mut store = ChannelStore::default();
        store.insert_channel(parquet_waveform::ChannelData {
            path: std::path::PathBuf::new(),
            channel_name: "ch".to_owned(),
            channel_path: "ch".to_owned(),
            values,
            projected_column_index: 1,
            elapsed: Duration::ZERO,
        });

        store.begin_envelope_frame();
        store.prepare_envelope_context(&time, time.len(), (10.0, 30.0), (0.0, 99.0), 10);
        let first = store
            .ensure_envelope("ch", &time, (10.0, 30.0), 10, true)
            .expect("first envelope");
        assert!(first.1);
        assert_eq!(first.0.time_range, Some((10.0, 30.0)));
        assert_eq!(store.last_envelope_stats.context_misses, 1);
        assert_eq!(store.last_envelope_stats.misses, 1);

        store.begin_envelope_frame();
        store.prepare_envelope_context(&time, time.len(), (15.0, 35.0), (0.0, 99.0), 10);
        let second = store
            .ensure_envelope("ch", &time, (15.0, 35.0), 10, true)
            .expect("second envelope");
        assert!(!second.1);
        assert_eq!(second.0.time_range, Some((15.0, 35.0)));
        assert_eq!(store.last_envelope_stats.context_hits, 1);
        assert_eq!(store.last_envelope_stats.hits, 1);
    }

    #[test]
    fn overscan_envelope_context_rejects_zoom_span_change() {
        let context = EnvelopeContext::for_view((10.0, 30.0), (0.0, 100.0), 100).expect("context");

        assert!(context.reuses_for_view((15.0, 35.0), 100));
        assert!(!context.reuses_for_view((15.0, 25.0), 100));
    }

    #[test]
    fn line_tile_lod_only_applies_to_wide_line_buckets() {
        let wide_plan = parquet_waveform::MinMaxEnvelopePlan {
            bucket_spans: Vec::new(),
            source_sample_count: LINE_TILE_MIN_SOURCE_SAMPLES,
            requested_bucket_count: 100,
            bucket_size: LINE_TILE_MIN_BUCKET_SIZE,
            time_range: Some((0.0, 1.0)),
        };
        let narrow_plan = parquet_waveform::MinMaxEnvelopePlan {
            bucket_size: LINE_TILE_MIN_BUCKET_SIZE - 1,
            ..wide_plan.clone()
        };
        let small_plan = parquet_waveform::MinMaxEnvelopePlan {
            source_sample_count: LINE_TILE_MIN_SOURCE_SAMPLES - 1,
            ..wide_plan.clone()
        };

        assert!(should_use_line_tile_lod(&wide_plan));
        assert!(!should_use_line_tile_lod(&narrow_plan));
        assert!(!should_use_line_tile_lod(&small_plan));
    }
}
