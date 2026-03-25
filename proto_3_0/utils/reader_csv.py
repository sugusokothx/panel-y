"""CSV ファイル読み込みモジュール。"""

from pathlib import Path
import pandas as pd


def read_csv(path: Path) -> pd.DataFrame:
    """CSV ファイルを読み込んで DataFrame を返す。"""
    return pd.read_csv(path)
