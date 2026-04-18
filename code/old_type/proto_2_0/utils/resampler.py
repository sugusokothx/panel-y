"""等時間間隔チェック・補間リサンプリングモジュール。"""

import numpy as np
import pandas as pd

UNIFORMITY_THRESHOLD = 1e-6  # 相対誤差 1ppm


def check_uniform(time: pd.Series) -> bool:
    """時間系列が等間隔かどうか判定する。"""
    diffs = time.diff().dropna()
    mean = diffs.mean()
    if mean == 0:
        return False
    return (diffs.max() - diffs.min()) / mean < UNIFORMITY_THRESHOLD


def resample_uniform(df: pd.DataFrame) -> pd.DataFrame:
    """非等間隔データを線形補間で等間隔にリサンプリングする。"""
    time = df["time"]
    n = len(time)
    new_time = np.linspace(time.iloc[0], time.iloc[-1], n)

    result = {"time": new_time}
    channels = [col for col in df.columns if col != "time"]
    for ch in channels:
        result[ch] = np.interp(new_time, time.values, df[ch].values)

    return pd.DataFrame(result)
