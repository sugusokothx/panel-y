# Panel_y Rust Phase 1

Rust migration vertical prototype for `code/proto_3_1b`.

Phase 1 focuses on one risk path:

1. Start a native Rust desktop app.
2. Read a Parquet file.
3. Detect the `time` column and channel columns.
4. Display one selected channel.
5. Keep pan/zoom responsive by drawing a min/max envelope instead of all samples.

## Run

```bash
cargo run
```

Schema detection can also be checked without opening the GUI:

```bash
cargo run -- --schema ../proto_3_1b/data/test_100k.parquet
```

To verify the Phase 1 narrow read path without opening the GUI:

```bash
cargo run -- --load-channel ../proto_3_1b/data/test_100k.parquet sine_50Hz
```

To benchmark load time plus repeated visible-range envelope extraction:

```bash
cargo run --release -- --bench-channel ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet iu
```

To stress repeated visible-range extraction and sample RSS:

```bash
cargo run --release -- --stress-channel ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet iu 1000
```

## Reference Data

Use the Phase 0 datasets under:

```text
../proto_3_1b/data/
```

The first target is:

```text
../proto_3_1b/data/test_100k.parquet
```

The large-data target is:

```text
../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet
```

## Current State

- `eframe` app shell is in place.
- The central waveform drawing surface is ready.
- Parquet schema loading is in place.
- The `time` column and numeric channel columns are detected.
- The selected channel read path loads only `time` plus that one channel (`time: f64`, value: `f32`).
- Full-range min/max envelope drawing is in place for the selected channel.
- X-axis pan/zoom is in place for the selected channel.
- The visible X range is re-extracted into a min/max envelope sized to the current plot width.
