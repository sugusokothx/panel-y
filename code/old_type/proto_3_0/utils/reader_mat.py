"""MATLAB .mat ファイル読み込みモジュール。"""

from pathlib import Path
import numpy as np
import pandas as pd


def read_mat(path: Path) -> pd.DataFrame:
    """MATLAB .mat ファイルを読み込んで DataFrame を返す。
    v7.2以前（scipy）→ v7.3以降（h5py）の順に試行する。
    """
    e_scipy = None
    e_h5 = None

    # v7.2 以前
    try:
        import scipy.io
        mat = scipy.io.loadmat(str(path))
        return _mat_dict_to_df(mat)
    except Exception as e:
        e_scipy = e

    # v7.3 以降（HDF5）
    try:
        import h5py
        with h5py.File(str(path), "r") as f:
            return _h5_to_df(f)
    except Exception as e:
        e_h5 = e

    raise ValueError(
        f"MATLAB ファイルの読み込みに失敗しました。\n"
        f"  scipy: {e_scipy}\n"
        f"  h5py:  {e_h5}"
    )


def _mat_dict_to_df(mat: dict) -> pd.DataFrame:
    """scipy.io.loadmat の結果を DataFrame に変換する。"""
    data = {}
    for key, val in mat.items():
        if key.startswith("_"):
            continue
        if isinstance(val, np.ndarray) and val.ndim <= 2:
            data[key] = val.flatten()
    if not data:
        raise ValueError("有効な数値変数が見つかりませんでした")
    min_len = min(len(v) for v in data.values())
    return pd.DataFrame({k: v[:min_len] for k, v in data.items()})


def _h5_to_df(f) -> pd.DataFrame:
    """h5py.File オブジェクトを DataFrame に変換する。"""
    data = {}
    for key in f.keys():
        val = f[key]
        if hasattr(val, "shape") and len(val.shape) <= 2:
            data[key] = np.array(val).flatten()
    if not data:
        raise ValueError("有効な数値変数が見つかりませんでした")
    min_len = min(len(v) for v in data.values())
    return pd.DataFrame({k: v[:min_len] for k, v in data.items()})
