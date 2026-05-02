"""Generate a Parquet dataset for ripple envelope validation.

The channels are designed to exercise min/max envelope and line-tile LOD
without relying on simple downsampling:

- low-amplitude high-frequency ripple
- amplitude-modulated ripple
- ripple mixed with deterministic noise
- ripple whose period is close to the Phase 2 bench bucket width
- small high-frequency ripple riding on a low-frequency sine wave
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
DEFAULT_ROWS = 2_400_000
DEFAULT_FS = 1_000_000.0
DEFAULT_CHUNK_SIZE = 240_000
NOISE_SEED = 20260502


@dataclass(frozen=True)
class RippleSpec:
    rows: int
    fs: float
    chunk_size: int


CHANNELS = (
    "ripple_low_amp_80kHz",
    "ripple_am_60kHz",
    "ripple_noise_70kHz",
    "ripple_bucket_near_2048samples",
    "ripple_on_sine_80kHz",
)


def _schema() -> pa.Schema:
    return pa.schema(
        [pa.field("time", pa.float64())]
        + [pa.field(channel, pa.float32()) for channel in CHANNELS]
    )


def _chunk(start: int, count: int, spec: RippleSpec) -> dict[str, np.ndarray]:
    idx = start + np.arange(count, dtype=np.float64)
    t = idx / spec.fs
    rng = np.random.default_rng(NOISE_SEED + start)

    ripple_low_amp = 0.025 * np.sin(2 * np.pi * 80_000.0 * t)
    am = 0.09 * (1.0 + 0.75 * np.sin(2 * np.pi * 5.0 * t))
    ripple_am = am * np.sin(2 * np.pi * 60_000.0 * t)
    ripple_noise = (
        0.08 * np.sin(2 * np.pi * 70_000.0 * t)
        + rng.normal(0.0, 0.018, count)
    )
    bucket_near_freq = spec.fs / 2_048.0
    ripple_bucket_near = 0.12 * np.sin(2 * np.pi * bucket_near_freq * t)
    ripple_on_sine = (
        0.40 * np.sin(2 * np.pi * 50.0 * t)
        + 0.025 * np.sin(2 * np.pi * 80_000.0 * t)
    )

    return {
        "time": t.astype(np.float64, copy=False),
        "ripple_low_amp_80kHz": ripple_low_amp.astype(np.float32),
        "ripple_am_60kHz": ripple_am.astype(np.float32),
        "ripple_noise_70kHz": ripple_noise.astype(np.float32),
        "ripple_bucket_near_2048samples": ripple_bucket_near.astype(np.float32),
        "ripple_on_sine_80kHz": ripple_on_sine.astype(np.float32),
    }


def _write_dataset(spec: RippleSpec, force: bool) -> Path:
    DATA_DIR.mkdir(exist_ok=True)
    out = DATA_DIR / "ripple_validation_2_4m.parquet"
    if out.exists() and not force:
        raise FileExistsError(f"{out} already exists; pass --force to overwrite")

    writer = pq.ParquetWriter(
        out,
        _schema(),
        compression="zstd",
        compression_level=3,
        use_dictionary=False,
    )
    try:
        for start in range(0, spec.rows, spec.chunk_size):
            count = min(spec.chunk_size, spec.rows - start)
            table = pa.table(_chunk(start, count, spec), schema=_schema())
            writer.write_table(table, row_group_size=count)
            print(f"wrote {start + count:,}/{spec.rows:,} rows")
    finally:
        writer.close()

    return out


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--force", action="store_true", help="Overwrite existing output")
    parser.add_argument("--rows", type=int, default=DEFAULT_ROWS)
    parser.add_argument("--fs", type=float, default=DEFAULT_FS)
    parser.add_argument("--chunk-size", type=int, default=DEFAULT_CHUNK_SIZE)
    args = parser.parse_args()

    if args.rows <= 0:
        raise ValueError("--rows must be positive")
    if args.fs <= 0.0:
        raise ValueError("--fs must be positive")
    if args.chunk_size <= 0:
        raise ValueError("--chunk-size must be positive")

    spec = RippleSpec(rows=args.rows, fs=args.fs, chunk_size=args.chunk_size)
    out = _write_dataset(spec, force=args.force)
    metadata = pq.ParquetFile(out).metadata
    print(
        f"generated: {out} ({metadata.num_rows:,} rows, {len(CHANNELS)} channels, "
        f"{out.stat().st_size:,} bytes, sha256={_sha256(out)})"
    )
    print("channels:")
    for channel in CHANNELS:
        print(f"  - {channel}")


if __name__ == "__main__":
    main()
