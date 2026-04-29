"""Generate synthetic Phase 1 benchmark datasets.

The files are intentionally written in chunks so the large 90M-point dataset
can be created on a 16 GB machine without keeping the full table in memory.
Generated Parquet files are ignored by git via the repository .gitignore.
"""

from __future__ import annotations

import argparse
import hashlib
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import pyarrow as pa
import pyarrow.parquet as pq


DATA_DIR = Path(__file__).parent / "data"


@dataclass(frozen=True)
class DatasetSpec:
    name: str
    filename: str
    rows: int
    fs: float
    channels: tuple[str, ...]
    row_group_size: int


MEDIUM_SPEC = DatasetSpec(
    name="medium",
    filename="panely_medium_2m_8ch.parquet",
    rows=2_000_000,
    fs=100_000.0,
    channels=(
        "sine_50Hz",
        "sine_1kHz",
        "pwm_1kHz",
        "pwm_10kHz",
        "noise",
        "chirp_1_500Hz",
        "step_signal",
        "spike_signal",
    ),
    row_group_size=200_000,
)


LARGE_SPEC = DatasetSpec(
    name="large",
    filename="panely_large_10s_1mhz_9ch.parquet",
    rows=10_000_000,
    fs=1_000_000.0,
    channels=(
        "iu",
        "iv",
        "iw",
        "vu",
        "vv",
        "vw",
        "torque",
        "gate_pwm",
        "spike_event",
    ),
    row_group_size=250_000,
)


def _medium_chunk(start: int, count: int, spec: DatasetSpec) -> dict[str, np.ndarray]:
    idx = start + np.arange(count, dtype=np.float64)
    t = idx / spec.fs
    duration = spec.rows / spec.fs

    phase_chirp = 2 * np.pi * (1.0 * t + 0.5 * ((500.0 - 1.0) / duration) * t * t)
    step_levels = np.array([-2.0, 0.0, 3.0, 1.0], dtype=np.float32)
    step_idx = np.floor(t / 0.5).astype(np.int64) % len(step_levels)

    rng = np.random.default_rng(42 + start // spec.row_group_size)
    spike = np.zeros(count, dtype=np.float32)
    global_idx = start + np.arange(count)
    spike_mask = (global_idx % 13_700) < 6
    spike[spike_mask] = 8.0

    return {
        "time": t.astype(np.float64, copy=False),
        "sine_50Hz": (10.0 * np.sin(2 * np.pi * 50.0 * t)).astype(np.float32),
        "sine_1kHz": (4.0 * np.sin(2 * np.pi * 1_000.0 * t)).astype(np.float32),
        "pwm_1kHz": (((np.mod(t * 1_000.0, 1.0) < 0.3).astype(np.float32)) * 5.0),
        "pwm_10kHz": (((np.mod(t * 10_000.0, 1.0) < 0.45).astype(np.float32)) * 5.0),
        "noise": rng.normal(0.0, 1.0, count).astype(np.float32),
        "chirp_1_500Hz": (5.0 * np.sin(phase_chirp)).astype(np.float32),
        "step_signal": step_levels[step_idx],
        "spike_signal": spike,
    }


def _large_chunk(start: int, count: int, spec: DatasetSpec) -> dict[str, np.ndarray]:
    idx = start + np.arange(count, dtype=np.float64)
    t = idx / spec.fs

    angle = 2 * np.pi * 50.0 * t
    ripple = 1.2 * np.sin(2 * np.pi * 1_200.0 * t)
    carrier = np.mod(t * 20_000.0, 1.0)
    duty = 0.5 + 0.35 * np.sin(2 * np.pi * 50.0 * t)

    global_idx = start + np.arange(count)
    spike = np.zeros(count, dtype=np.float32)
    spike_mask = (global_idx % 97_531) < 10
    spike[spike_mask] = 1.0

    return {
        "time": t.astype(np.float64, copy=False),
        "iu": (20.0 * np.sin(angle) + ripple).astype(np.float32),
        "iv": (20.0 * np.sin(angle - 2 * np.pi / 3) + 0.8 * ripple).astype(np.float32),
        "iw": (20.0 * np.sin(angle + 2 * np.pi / 3) + 1.1 * ripple).astype(np.float32),
        "vu": (320.0 * np.sin(angle) + 18.0 * np.sin(2 * np.pi * 5_000.0 * t)).astype(np.float32),
        "vv": (320.0 * np.sin(angle - 2 * np.pi / 3) + 15.0 * np.sin(2 * np.pi * 5_000.0 * t)).astype(np.float32),
        "vw": (320.0 * np.sin(angle + 2 * np.pi / 3) + 16.0 * np.sin(2 * np.pi * 5_000.0 * t)).astype(np.float32),
        "torque": (5.0 + 1.5 * np.sin(2 * np.pi * 5.0 * t) + 0.2 * np.sin(2 * np.pi * 250.0 * t)).astype(np.float32),
        "gate_pwm": ((carrier < duty).astype(np.float32) * 5.0),
        "spike_event": spike,
    }


def _schema(spec: DatasetSpec) -> pa.Schema:
    return pa.schema(
        [pa.field("time", pa.float64())]
        + [pa.field(ch, pa.float32()) for ch in spec.channels]
    )


def _write_dataset(spec: DatasetSpec, force: bool) -> Path:
    DATA_DIR.mkdir(exist_ok=True)
    out = DATA_DIR / spec.filename
    if out.exists() and not force:
        raise FileExistsError(f"{out} already exists; pass --force to overwrite")

    chunk_fn = _medium_chunk if spec.name == "medium" else _large_chunk
    writer = pq.ParquetWriter(
        out,
        _schema(spec),
        compression="zstd",
        compression_level=3,
        use_dictionary=False,
    )
    try:
        for start in range(0, spec.rows, spec.row_group_size):
            count = min(spec.row_group_size, spec.rows - start)
            arrays = chunk_fn(start, count, spec)
            table = pa.table(arrays, schema=_schema(spec))
            writer.write_table(table, row_group_size=count)
            print(f"{spec.name}: wrote {start + count:,}/{spec.rows:,} rows")
    finally:
        writer.close()
    return out


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def _inspect(path: Path) -> dict[str, object]:
    pf = pq.ParquetFile(path)
    schema_names = pf.schema_arrow.names
    metadata = pf.metadata
    return {
        "path": str(path),
        "rows": metadata.num_rows,
        "columns": schema_names,
        "size_bytes": path.stat().st_size,
        "sha256": _sha256(path),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "target",
        choices=("medium", "large", "all"),
        help="Dataset to generate",
    )
    parser.add_argument("--force", action="store_true", help="Overwrite existing output")
    args = parser.parse_args()

    specs = []
    if args.target in ("medium", "all"):
        specs.append(MEDIUM_SPEC)
    if args.target in ("large", "all"):
        specs.append(LARGE_SPEC)

    for spec in specs:
        out = _write_dataset(spec, force=args.force)
        info = _inspect(out)
        print(
            f"{spec.name}: done path={info['path']} rows={info['rows']:,} "
            f"columns={len(info['columns'])} size={info['size_bytes']:,} "
            f"sha256={info['sha256']}"
        )


if __name__ == "__main__":
    main()
