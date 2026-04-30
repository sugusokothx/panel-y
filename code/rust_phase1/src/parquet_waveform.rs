use arrow_array::{
    Array, Float32Array, Float64Array, Int8Array, Int16Array, Int32Array, Int64Array, UInt8Array,
    UInt16Array, UInt32Array, UInt64Array,
};
use arrow_schema::DataType;
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::parquet_schema::{self, ColumnInfo};

const BATCH_SIZE: usize = 64 * 1024;

macro_rules! append_primitive {
    ($array:expr, $output:expr, $array_type:ty, $convert:expr) => {{
        let primitive = $array
            .as_any()
            .downcast_ref::<$array_type>()
            .ok_or_else(|| "Arrow array downcast failed".to_owned())?;

        for index in 0..primitive.len() {
            if primitive.is_null(index) {
                $output.push(f64::NAN);
            } else {
                $output.push($convert(primitive.value(index)));
            }
        }

        Ok(())
    }};
}

macro_rules! append_primitive_as_f32 {
    ($array:expr, $output:expr, $array_type:ty, $convert:expr) => {{
        let primitive = $array
            .as_any()
            .downcast_ref::<$array_type>()
            .ok_or_else(|| "Arrow array downcast failed".to_owned())?;

        for index in 0..primitive.len() {
            if primitive.is_null(index) {
                $output.push(f32::NAN);
            } else {
                $output.push($convert(primitive.value(index)));
            }
        }

        Ok(())
    }};
}

#[derive(Clone, Debug)]
pub struct WaveformData {
    pub path: PathBuf,
    pub channel_name: String,
    pub channel_path: String,
    pub time: Vec<f64>,
    pub values: Vec<f32>,
    pub projected_column_indices: [usize; 2],
    pub elapsed: Duration,
}

#[derive(Clone, Debug)]
pub struct TimeData {
    pub path: PathBuf,
    pub column_name: String,
    pub column_path: String,
    pub time: Vec<f64>,
    pub projected_column_index: usize,
    pub elapsed: Duration,
}

#[derive(Clone, Debug)]
pub struct ChannelData {
    pub path: PathBuf,
    pub channel_name: String,
    pub channel_path: String,
    pub values: Vec<f32>,
    pub projected_column_index: usize,
    pub elapsed: Duration,
}

#[derive(Clone, Debug)]
pub struct MinMaxEnvelope {
    pub buckets: Vec<EnvelopeBucket>,
    pub source_sample_count: usize,
    pub requested_bucket_count: usize,
    pub bucket_size: usize,
    pub time_range: Option<(f64, f64)>,
    pub value_range: Option<(f64, f64)>,
    pub elapsed: Duration,
}

impl MinMaxEnvelope {
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    pub fn draw_point_count(&self) -> usize {
        self.buckets.len() * 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvelopeBucket {
    pub time: f64,
    pub min: f32,
    pub max: f32,
}

impl WaveformData {
    pub fn sample_count(&self) -> usize {
        self.time.len()
    }

    pub fn memory_bytes(&self) -> usize {
        self.time.len() * std::mem::size_of::<f64>()
            + self.values.len() * std::mem::size_of::<f32>()
    }

    pub fn time_range(&self) -> Option<(f64, f64)> {
        Some((*self.time.first()?, *self.time.last()?))
    }

    pub fn min_max_envelope(&self, requested_bucket_count: usize) -> MinMaxEnvelope {
        min_max_envelope(&self.time, &self.values, requested_bucket_count)
    }

    pub fn min_max_envelope_for_range(
        &self,
        time_range: (f64, f64),
        requested_bucket_count: usize,
    ) -> MinMaxEnvelope {
        min_max_envelope_for_range(&self.time, &self.values, time_range, requested_bucket_count)
    }
}

impl TimeData {
    pub fn sample_count(&self) -> usize {
        self.time.len()
    }

    pub fn memory_bytes(&self) -> usize {
        self.time.len() * std::mem::size_of::<f64>()
    }

    pub fn time_range(&self) -> Option<(f64, f64)> {
        Some((*self.time.first()?, *self.time.last()?))
    }
}

impl ChannelData {
    pub fn sample_count(&self) -> usize {
        self.values.len()
    }

    pub fn memory_bytes(&self) -> usize {
        self.values.len() * std::mem::size_of::<f32>()
    }

    pub fn min_max_envelope_for_range(
        &self,
        time: &[f64],
        time_range: (f64, f64),
        requested_bucket_count: usize,
    ) -> MinMaxEnvelope {
        min_max_envelope_for_range(time, &self.values, time_range, requested_bucket_count)
    }
}

pub fn min_max_envelope(
    time: &[f64],
    values: &[f32],
    requested_bucket_count: usize,
) -> MinMaxEnvelope {
    min_max_envelope_for_indices(
        time,
        values,
        0,
        time.len().min(values.len()),
        requested_bucket_count,
        time_range(time),
    )
}

pub fn min_max_envelope_for_range(
    time: &[f64],
    values: &[f32],
    time_range: (f64, f64),
    requested_bucket_count: usize,
) -> MinMaxEnvelope {
    let Some((range_start, range_end)) = normalized_time_range(time_range) else {
        return empty_envelope(requested_bucket_count, None, Instant::now());
    };

    let sample_count = time.len().min(values.len());
    let start = time.partition_point(|time| *time < range_start);
    let end = time.partition_point(|time| *time <= range_end);

    min_max_envelope_for_indices(
        time,
        values,
        start.min(sample_count),
        end.min(sample_count),
        requested_bucket_count,
        Some((range_start, range_end)),
    )
}

fn min_max_envelope_for_indices(
    time: &[f64],
    values: &[f32],
    start_index: usize,
    end_index: usize,
    requested_bucket_count: usize,
    time_range: Option<(f64, f64)>,
) -> MinMaxEnvelope {
    let started = Instant::now();
    let sample_count = time.len().min(values.len());
    let start_index = start_index.min(sample_count);
    let end_index = end_index.min(sample_count).max(start_index);
    let source_sample_count = end_index - start_index;
    let target_bucket_count = requested_bucket_count
        .max(1)
        .min(source_sample_count.max(1));
    let bucket_size = source_sample_count.div_ceil(target_bucket_count).max(1);
    let mut buckets = Vec::with_capacity(target_bucket_count);
    let mut value_range = None;

    let mut start = start_index;
    while start < end_index {
        let end = (start + bucket_size).min(end_index);
        if let Some(bucket) = envelope_bucket(time, values, start, end) {
            value_range = update_range(value_range, f64::from(bucket.min));
            value_range = update_range(value_range, f64::from(bucket.max));
            buckets.push(bucket);
        }
        start = end;
    }

    MinMaxEnvelope {
        buckets,
        source_sample_count,
        requested_bucket_count,
        bucket_size,
        time_range,
        value_range,
        elapsed: started.elapsed(),
    }
}

fn envelope_bucket(
    time_values: &[f64],
    channel_values: &[f32],
    start: usize,
    end: usize,
) -> Option<EnvelopeBucket> {
    if start >= end {
        return None;
    }

    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for value in &channel_values[start..end] {
        if value.is_finite() {
            min = min.min(*value);
            max = max.max(*value);
        }
    }

    if !min.is_finite() || !max.is_finite() {
        return None;
    }

    let time_start = *time_values.get(start)?;
    let time_end = *time_values.get(end - 1)?;
    let time = (time_start + time_end) * 0.5;
    if !time.is_finite() {
        return None;
    }

    Some(EnvelopeBucket { time, min, max })
}

pub fn time_range(time: &[f64]) -> Option<(f64, f64)> {
    Some((*time.first()?, *time.last()?))
}

fn normalized_time_range((start, end): (f64, f64)) -> Option<(f64, f64)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }

    match start.partial_cmp(&end)? {
        std::cmp::Ordering::Less => Some((start, end)),
        std::cmp::Ordering::Greater => Some((end, start)),
        std::cmp::Ordering::Equal => None,
    }
}

fn empty_envelope(
    requested_bucket_count: usize,
    time_range: Option<(f64, f64)>,
    started: Instant,
) -> MinMaxEnvelope {
    MinMaxEnvelope {
        buckets: Vec::new(),
        source_sample_count: 0,
        requested_bucket_count,
        bucket_size: 1,
        time_range,
        value_range: None,
        elapsed: started.elapsed(),
    }
}

pub fn read_selected_channel(
    path: impl AsRef<Path>,
    selected_channel: &str,
) -> Result<WaveformData, String> {
    let path = path.as_ref();
    let summary = parquet_schema::read_schema_summary(path)?;
    let time_column = summary
        .time_column
        .as_ref()
        .ok_or_else(|| "time column not found".to_owned())?;
    let channel = find_channel(&summary.channels, selected_channel)?;

    let started = Instant::now();
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(file).map_err(|error| error.to_string())?;
    let projection =
        ProjectionMask::leaves(builder.parquet_schema(), [time_column.index, channel.index]);
    let projected_column_indices = [time_column.index, channel.index];
    let (time_position, channel_position) = projected_positions(time_column.index, channel.index)?;
    let mut reader = builder
        .with_projection(projection)
        .with_batch_size(BATCH_SIZE)
        .build()
        .map_err(|error| error.to_string())?;

    let capacity = usize::try_from(summary.row_count).unwrap_or(0);
    let mut time = Vec::with_capacity(capacity);
    let mut values = Vec::with_capacity(capacity);

    for batch in &mut reader {
        let batch = batch.map_err(|error| error.to_string())?;
        append_numeric_as_f64(
            batch.column(time_position).as_ref(),
            &mut time,
            time_column.display_name(),
            NullPolicy::Reject,
        )?;
        append_numeric_as_f32(
            batch.column(channel_position).as_ref(),
            &mut values,
            channel.display_name(),
            NullPolicy::NaN,
        )?;
    }

    if time.len() != values.len() {
        return Err(format!(
            "time/value length mismatch: {} vs {}",
            time.len(),
            values.len()
        ));
    }

    Ok(WaveformData {
        path: path.to_path_buf(),
        channel_name: channel.display_name().to_owned(),
        channel_path: channel.path.clone(),
        time,
        values,
        projected_column_indices,
        elapsed: started.elapsed(),
    })
}

pub fn read_time_column(
    path: impl AsRef<Path>,
    summary: &parquet_schema::SchemaSummary,
) -> Result<TimeData, String> {
    let path = path.as_ref();
    let time_column = summary
        .time_column
        .as_ref()
        .ok_or_else(|| "time column not found".to_owned())?;

    let started = Instant::now();
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(file).map_err(|error| error.to_string())?;
    let projection = ProjectionMask::leaves(builder.parquet_schema(), [time_column.index]);
    let mut reader = builder
        .with_projection(projection)
        .with_batch_size(BATCH_SIZE)
        .build()
        .map_err(|error| error.to_string())?;

    let capacity = usize::try_from(summary.row_count).unwrap_or(0);
    let mut time = Vec::with_capacity(capacity);

    for batch in &mut reader {
        let batch = batch.map_err(|error| error.to_string())?;
        append_numeric_as_f64(
            batch.column(0).as_ref(),
            &mut time,
            time_column.display_name(),
            NullPolicy::Reject,
        )?;
    }

    Ok(TimeData {
        path: path.to_path_buf(),
        column_name: time_column.display_name().to_owned(),
        column_path: time_column.path.clone(),
        time,
        projected_column_index: time_column.index,
        elapsed: started.elapsed(),
    })
}

pub fn read_channel_values(
    path: impl AsRef<Path>,
    summary: &parquet_schema::SchemaSummary,
    selected_channel: &str,
) -> Result<ChannelData, String> {
    let path = path.as_ref();
    let channel = find_channel(&summary.channels, selected_channel)?;

    let started = Instant::now();
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(file).map_err(|error| error.to_string())?;
    let projection = ProjectionMask::leaves(builder.parquet_schema(), [channel.index]);
    let mut reader = builder
        .with_projection(projection)
        .with_batch_size(BATCH_SIZE)
        .build()
        .map_err(|error| error.to_string())?;

    let capacity = usize::try_from(summary.row_count).unwrap_or(0);
    let mut values = Vec::with_capacity(capacity);

    for batch in &mut reader {
        let batch = batch.map_err(|error| error.to_string())?;
        append_numeric_as_f32(
            batch.column(0).as_ref(),
            &mut values,
            channel.display_name(),
            NullPolicy::NaN,
        )?;
    }

    Ok(ChannelData {
        path: path.to_path_buf(),
        channel_name: channel.display_name().to_owned(),
        channel_path: channel.path.clone(),
        values,
        projected_column_index: channel.index,
        elapsed: started.elapsed(),
    })
}

fn find_channel<'a>(
    channels: &'a [ColumnInfo],
    selected_channel: &str,
) -> Result<&'a ColumnInfo, String> {
    channels
        .iter()
        .find(|channel| {
            channel.path == selected_channel
                || channel.name == selected_channel
                || channel.display_name() == selected_channel
        })
        .ok_or_else(|| format!("selected channel not found: {selected_channel}"))
}

fn projected_positions(time_index: usize, channel_index: usize) -> Result<(usize, usize), String> {
    match time_index.cmp(&channel_index) {
        std::cmp::Ordering::Less => Ok((0, 1)),
        std::cmp::Ordering::Greater => Ok((1, 0)),
        std::cmp::Ordering::Equal => Err("time and channel columns must differ".to_owned()),
    }
}

#[derive(Clone, Copy)]
enum NullPolicy {
    Reject,
    NaN,
}

fn append_numeric_as_f64(
    array: &dyn Array,
    output: &mut Vec<f64>,
    column_name: &str,
    null_policy: NullPolicy,
) -> Result<(), String> {
    if array.null_count() > 0 && matches!(null_policy, NullPolicy::Reject) {
        return Err(format!("{column_name} contains null values"));
    }

    match array.data_type() {
        DataType::Float64 => append_primitive!(array, output, Float64Array, |value| value),
        DataType::Float32 => append_primitive!(array, output, Float32Array, f64::from),
        DataType::Int64 => append_primitive!(array, output, Int64Array, |value| value as f64),
        DataType::Int32 => append_primitive!(array, output, Int32Array, f64::from),
        DataType::Int16 => append_primitive!(array, output, Int16Array, f64::from),
        DataType::Int8 => append_primitive!(array, output, Int8Array, f64::from),
        DataType::UInt64 => append_primitive!(array, output, UInt64Array, |value| value as f64),
        DataType::UInt32 => append_primitive!(array, output, UInt32Array, f64::from),
        DataType::UInt16 => append_primitive!(array, output, UInt16Array, f64::from),
        DataType::UInt8 => append_primitive!(array, output, UInt8Array, f64::from),
        other => Err(format!(
            "{column_name} is not a supported numeric type: {other}"
        )),
    }
}

fn append_numeric_as_f32(
    array: &dyn Array,
    output: &mut Vec<f32>,
    column_name: &str,
    null_policy: NullPolicy,
) -> Result<(), String> {
    if array.null_count() > 0 && matches!(null_policy, NullPolicy::Reject) {
        return Err(format!("{column_name} contains null values"));
    }

    match array.data_type() {
        DataType::Float64 => {
            append_primitive_as_f32!(array, output, Float64Array, |value| { value as f32 })
        }
        DataType::Float32 => append_primitive_as_f32!(array, output, Float32Array, |value| value),
        DataType::Int64 => {
            append_primitive_as_f32!(array, output, Int64Array, |value| { value as f32 })
        }
        DataType::Int32 => {
            append_primitive_as_f32!(array, output, Int32Array, |value| { value as f32 })
        }
        DataType::Int16 => {
            append_primitive_as_f32!(array, output, Int16Array, |value| { value as f32 })
        }
        DataType::Int8 => {
            append_primitive_as_f32!(array, output, Int8Array, |value| value as f32)
        }
        DataType::UInt64 => {
            append_primitive_as_f32!(array, output, UInt64Array, |value| { value as f32 })
        }
        DataType::UInt32 => {
            append_primitive_as_f32!(array, output, UInt32Array, |value| { value as f32 })
        }
        DataType::UInt16 => {
            append_primitive_as_f32!(array, output, UInt16Array, |value| { value as f32 })
        }
        DataType::UInt8 => {
            append_primitive_as_f32!(array, output, UInt8Array, |value| value as f32)
        }
        other => Err(format!(
            "{column_name} is not a supported numeric type: {other}"
        )),
    }
}

fn update_range(range: Option<(f64, f64)>, value: f64) -> Option<(f64, f64)> {
    if !value.is_finite() {
        return range;
    }

    match range {
        Some((min, max)) => Some((min.min(value), max.max(value))),
        None => Some((value, value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_time_and_selected_channel_if_dataset_exists() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../proto_3_1b/data/test_100k.parquet");

        if !path.exists() {
            return;
        }

        let data = read_selected_channel(&path, "sine_50Hz").expect("read selected channel");

        assert_eq!(data.sample_count(), 100_000);
        assert_eq!(data.values.len(), 100_000);
        assert_eq!(data.projected_column_indices, [0, 1]);
        assert_eq!(data.channel_name, "sine_50Hz");
        assert!(data.time_range().is_some_and(|(start, end)| start < end));
        assert!(data.min_max_envelope(512).value_range.is_some());
    }

    #[test]
    fn reads_non_first_channel_with_same_time_projection_if_dataset_exists() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../proto_3_1b/data/test_100k.parquet");

        if !path.exists() {
            return;
        }

        let data = read_selected_channel(&path, "chirp_1_500Hz").expect("read selected channel");

        assert_eq!(data.sample_count(), 100_000);
        assert_eq!(data.projected_column_indices, [0, 4]);
        assert_eq!(data.channel_name, "chirp_1_500Hz");
    }

    #[test]
    fn reads_shared_time_and_channel_values_if_dataset_exists() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../proto_3_1b/data/test_100k.parquet");

        if !path.exists() {
            return;
        }

        let summary = parquet_schema::read_schema_summary(&path).expect("read schema");
        let time = read_time_column(&path, &summary).expect("read time column");
        let channel =
            read_channel_values(&path, &summary, "chirp_1_500Hz").expect("read channel values");

        assert_eq!(time.sample_count(), 100_000);
        assert_eq!(channel.sample_count(), time.sample_count());
        assert_eq!(time.projected_column_index, 0);
        assert_eq!(channel.projected_column_index, 4);
        assert_eq!(channel.channel_name, "chirp_1_500Hz");
        assert!(
            channel
                .min_max_envelope_for_range(&time.time, time.time_range().unwrap(), 512)
                .value_range
                .is_some()
        );
    }

    #[test]
    fn builds_full_range_min_max_envelope() {
        let data = WaveformData {
            path: PathBuf::new(),
            channel_name: "ch".to_owned(),
            channel_path: "ch".to_owned(),
            time: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            values: vec![1.0, 3.0, -1.0, 4.0, 2.0, 5.0, 0.0],
            projected_column_indices: [0, 1],
            elapsed: Duration::ZERO,
        };

        let envelope = data.min_max_envelope(3);

        assert_eq!(envelope.source_sample_count, 7);
        assert_eq!(envelope.bucket_size, 3);
        assert_eq!(envelope.bucket_count(), 3);
        assert_eq!(envelope.value_range, Some((-1.0, 5.0)));
        assert_eq!(
            envelope.buckets,
            vec![
                EnvelopeBucket {
                    time: 1.0,
                    min: -1.0,
                    max: 3.0,
                },
                EnvelopeBucket {
                    time: 4.0,
                    min: 2.0,
                    max: 5.0,
                },
                EnvelopeBucket {
                    time: 6.0,
                    min: 0.0,
                    max: 0.0,
                },
            ]
        );
    }

    #[test]
    fn builds_time_range_min_max_envelope() {
        let data = WaveformData {
            path: PathBuf::new(),
            channel_name: "ch".to_owned(),
            channel_path: "ch".to_owned(),
            time: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            values: vec![1.0, 3.0, -1.0, 4.0, 2.0, 5.0, 0.0],
            projected_column_indices: [0, 1],
            elapsed: Duration::ZERO,
        };

        let envelope = data.min_max_envelope_for_range((1.5, 4.5), 2);

        assert_eq!(envelope.time_range, Some((1.5, 4.5)));
        assert_eq!(envelope.source_sample_count, 3);
        assert_eq!(envelope.bucket_size, 2);
        assert_eq!(envelope.bucket_count(), 2);
        assert_eq!(envelope.value_range, Some((-1.0, 4.0)));
        assert_eq!(
            envelope.buckets,
            vec![
                EnvelopeBucket {
                    time: 2.5,
                    min: -1.0,
                    max: 4.0,
                },
                EnvelopeBucket {
                    time: 4.0,
                    min: 2.0,
                    max: 2.0,
                },
            ]
        );
    }

    #[test]
    fn accepts_reversed_time_range_for_envelope() {
        let data = WaveformData {
            path: PathBuf::new(),
            channel_name: "ch".to_owned(),
            channel_path: "ch".to_owned(),
            time: vec![0.0, 1.0, 2.0],
            values: vec![1.0, 2.0, 3.0],
            projected_column_indices: [0, 1],
            elapsed: Duration::ZERO,
        };

        let envelope = data.min_max_envelope_for_range((2.0, 0.0), 8);

        assert_eq!(envelope.time_range, Some((0.0, 2.0)));
        assert_eq!(envelope.source_sample_count, 3);
        assert_eq!(envelope.bucket_count(), 3);
    }
}
