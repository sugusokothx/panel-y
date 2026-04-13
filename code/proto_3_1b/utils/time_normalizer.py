"""時間カラムの検出・単位判定・正規化を行うモジュール。"""

import pandas as pd

TIME_COL_CANDIDATES = ["time", "t", "Time", "TIME", "timestamp", "elapsed", "ECUTIME"]


def detect_time_col(df: pd.DataFrame, hint: str | None = None) -> str:
    """時間カラムを検出して返す。hint があればそれを優先する。"""
    if hint:
        if hint in df.columns:
            return hint
        raise ValueError(
            f"指定された時間カラム '{hint}' が見つかりません。"
            f"利用可能: {list(df.columns)}"
        )
    for candidate in TIME_COL_CANDIDATES:
        if candidate in df.columns:
            return candidate
    raise ValueError(
        f"時間カラムを自動検出できませんでした。"
        f"カラム一覧: {list(df.columns)}  "
        f"--time-col で明示指定してください。"
    )


def detect_time_unit(series: pd.Series, hint: str | None = None) -> str:
    """時間単位をヒューリスティックに推定する。hint があればそれを返す。"""
    if hint:
        return hint
    max_val = series.max()
    if max_val < 1_000:
        return "s"
    elif max_val < 1_000_000:
        return "ms"
    else:
        return "us"


def normalize_time(
    df: pd.DataFrame, time_col: str, time_unit: str
) -> pd.DataFrame:
    """時間カラムを 'time'（秒単位）に正規化する。"""
    df = df.copy()
    divisor = {"s": 1, "ms": 1_000, "us": 1_000_000}[time_unit]
    if divisor != 1:
        df[time_col] = df[time_col] / divisor
    if time_col != "time":
        df = df.rename(columns={time_col: "time"})
    return df
