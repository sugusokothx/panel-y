"""Backend benchmark for Phase 1 synthetic datasets.

This script calls the Dash app functions directly. It measures data loading,
figure generation, Plotly JSON serialization, zoom redraws, hover lookup,
cursor statistics, and FFT. Browser rendering time is intentionally not
included because this benchmark is meant to be repeatable from the terminal.
"""

from __future__ import annotations

import argparse
import gc
import json
import resource
import time
from pathlib import Path
from typing import Any

from dash import no_update

import app as panel_app


DATA_DIR = Path(__file__).parent / "data"
DATASETS = {
    "medium": DATA_DIR / "panely_medium_2m_8ch.parquet",
    "large": DATA_DIR / "panely_large_10s_1mhz_9ch.parquet",
}


def _rss_mib() -> float:
    # macOS reports ru_maxrss in bytes.
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024 / 1024


def _elapsed(fn):
    start = time.perf_counter()
    value = fn()
    return time.perf_counter() - start, value


def _row_groups() -> list[list[str]]:
    return [[ch] for ch in panel_app.channels[:8]]


def _style_state(row_groups: list[list[str]]) -> tuple[list[str], list[float], list[dict[str, str]]]:
    used = [group[0] for group in row_groups]
    colors = [panel_app.TRACE_COLORS[i % len(panel_app.TRACE_COLORS)] for i in range(len(used))]
    widths = [1.0 for _ in used]
    color_ids = [{"ch": ch} for ch in used]
    return colors, widths, color_ids


def _render_figures(row_groups: list[list[str]], x_range: list[float] | None = None) -> int:
    total_json_bytes = 0
    for i, group in enumerate(row_groups):
        fig = panel_app.make_row_fig(
            group,
            show_xaxis=(i == len(row_groups) - 1),
            x_range=x_range,
            theme=panel_app.DEFAULT_THEME,
        )
        total_json_bytes += len(fig.to_json())
    return total_json_bytes


def _measure_dataset(path: Path) -> dict[str, Any]:
    result: dict[str, Any] = {"dataset": path.name, "path": str(path)}

    load_sec, load_result = _elapsed(lambda: panel_app.load_file(1, str(path)))
    result["load_sec"] = load_sec
    result["rss_after_load_mib"] = _rss_mib()
    result["load_status"] = str(load_result[1])
    result["rows"] = len(panel_app.df)
    result["channels"] = list(panel_app.channels)

    row_groups = _row_groups()
    colors, widths, color_ids = _style_state(row_groups)
    none_rows = [None] * len(row_groups)
    empty_steps = [[] for _ in row_groups]

    # Measure the callback-like row construction separately from figure JSON
    # serialization. The JSON stage approximates Dash response payload work.
    update_sec, _ = _elapsed(
        lambda: panel_app.update_waveform_rows(
            1,
            row_groups,
            none_rows,
            none_rows,
            empty_steps,
            colors,
            widths,
            color_ids,
            None,
            False,
        )
    )
    initial_fig_sec, initial_json_bytes = _elapsed(lambda: _render_figures(row_groups, None))
    result["initial_update_sec"] = update_sec
    result["initial_fig_json_sec"] = initial_fig_sec
    result["initial_json_bytes"] = initial_json_bytes
    result["rss_after_initial_mib"] = _rss_mib()

    t0 = float(panel_app.df["time"].iloc[0])
    t1 = float(panel_app.df["time"].iloc[-1])
    span = t1 - t0
    zoom_range = [t0 + 0.2 * span, t0 + 0.4 * span]

    zoom_sec, zoom_json_bytes = _elapsed(lambda: _render_figures(row_groups, zoom_range))
    result["zoom_sec"] = zoom_sec
    result["zoom_json_bytes"] = zoom_json_bytes
    result["rss_after_zoom_mib"] = _rss_mib()

    zoom_runs = []
    total_start = time.perf_counter()
    for i in range(10):
        left = t0 + (0.05 + i * 0.02) * span
        right = left + 0.1 * span
        sec, _ = _elapsed(lambda left=left, right=right: _render_figures(row_groups, [left, right]))
        zoom_runs.append(sec)
    result["continuous_zoom_total_sec"] = time.perf_counter() - total_start
    result["continuous_zoom_max_sec"] = max(zoom_runs)
    result["continuous_zoom_runs"] = zoom_runs
    result["rss_after_continuous_zoom_mib"] = _rss_mib()

    reset_sec, reset_json_bytes = _elapsed(lambda: _render_figures(row_groups, None))
    result["reset_sec"] = reset_sec
    result["reset_json_bytes"] = reset_json_bytes

    hover_x = t0 + 0.33 * span
    val_ids = [{"row": i} for i in range(len(row_groups))]
    hover_sec, _ = _elapsed(lambda: panel_app.update_values(hover_x, val_ids, row_groups))
    result["hover_sec"] = hover_sec

    cursor_a = t0 + 0.10 * span
    cursor_b = t0 + 0.20 * span
    stats_sec, _ = _elapsed(lambda: panel_app.update_delta_panel(cursor_a, cursor_b, row_groups))
    result["cursor_stats_sec"] = stats_sec

    fft_sec, fft_result = _elapsed(
        lambda: panel_app.compute_fft(
            1,
            cursor_a,
            cursor_b,
            [row_groups[0][0]],
            "hanning",
            "amplitude",
            None,
            None,
            panel_app.DEFAULT_THEME,
        )
    )
    fft_json_bytes = 0
    if fft_result[0] is not no_update:
        fft_json_bytes = len(fft_result[0].to_json())
    result["fft_sec"] = fft_sec
    result["fft_json_bytes"] = fft_json_bytes
    result["rss_after_analysis_mib"] = _rss_mib()

    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("dataset", choices=sorted(DATASETS))
    parser.add_argument("--run", type=int, default=1)
    args = parser.parse_args()

    path = DATASETS[args.dataset]
    if not path.exists():
        raise FileNotFoundError(path)

    result = _measure_dataset(path)
    result["run"] = args.run
    print(json.dumps(result, ensure_ascii=False, indent=2))
    gc.collect()


if __name__ == "__main__":
    main()
