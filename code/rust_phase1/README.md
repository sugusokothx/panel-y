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
- Parquet loading and LOD drawing are not implemented yet.
