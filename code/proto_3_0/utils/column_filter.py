"""非数値カラムを除外するモジュール。"""

import pandas as pd


def filter_numeric_columns(
    df: pd.DataFrame, time_col: str = "time"
) -> pd.DataFrame:
    """数値型カラムのみを残す。time カラムは常に保持する。"""
    cols_to_keep = [time_col]
    removed = []
    for col in df.columns:
        if col == time_col:
            continue
        if pd.api.types.is_numeric_dtype(df[col]):
            cols_to_keep.append(col)
        else:
            removed.append(col)
    if removed:
        print(f"[警告] 非数値カラムを除外しました: {removed}")
    channels = [c for c in cols_to_keep if c != time_col]
    if not channels:
        raise ValueError("波形チャンネルが0本です（全て非数値 or 時間のみ）")
    return df[cols_to_keep]
