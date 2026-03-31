"""MATLAB .mat ファイル読み込みモジュール。"""

import logging
from pathlib import Path
import warnings
import numpy as np
import pandas as pd

logger = logging.getLogger(__name__)


def read_mat(
    path: Path,
    *,
    allow_matrix_columns: bool = False,
    length_mismatch: str = "raise",
) -> pd.DataFrame:
    """MATLAB .mat ファイルを読み込んで DataFrame を返す。
    v7.2以前（scipy）→ v7.3以降（h5py）の順に試行する。
    """
    e_scipy = None
    e_h5 = None

    # v7.2 以前
    try:
        import scipy.io
        mat = scipy.io.loadmat(str(path))
        return _mat_dict_to_df(
            mat,
            allow_matrix_columns=allow_matrix_columns,
            length_mismatch=length_mismatch,
        )
    except Exception as e:
        e_scipy = e

    # v7.3 以降（HDF5）
    try:
        import h5py
        with h5py.File(str(path), "r") as f:
            return _h5_to_df(
                f,
                allow_matrix_columns=allow_matrix_columns,
                length_mismatch=length_mismatch,
            )
    except Exception as e:
        e_h5 = e

    raise ValueError(
        f"MATLAB ファイルの読み込みに失敗しました。\n"
        f"  scipy: {e_scipy}\n"
        f"  h5py:  {e_h5}"
    )


def _mat_dict_to_df(
    mat: dict,
    *,
    allow_matrix_columns: bool = False,
    length_mismatch: str = "raise",
) -> pd.DataFrame:
    """scipy.io.loadmat の結果を DataFrame に変換する。"""
    data = {}
    logs = []
    for key, val in mat.items():
        if key.startswith("_"):
            logs.append(f"{key}: 除外 (メタデータ)")
            continue
        extracted, detail = _extract_numeric_columns(
            key, val, allow_matrix_columns=allow_matrix_columns
        )
        logs.append(detail)
        data.update(extracted)
    return _build_dataframe(data, logs, length_mismatch=length_mismatch)


def _h5_to_df(
    f,
    *,
    allow_matrix_columns: bool = False,
    length_mismatch: str = "raise",
) -> pd.DataFrame:
    """h5py.File オブジェクトを DataFrame に変換する。"""
    data = {}
    logs = []
    for key in f.keys():
        val = np.array(f[key])
        extracted, detail = _extract_numeric_columns(
            key, val, allow_matrix_columns=allow_matrix_columns
        )
        logs.append(detail)
        data.update(extracted)
    return _build_dataframe(data, logs, length_mismatch=length_mismatch)


def _extract_numeric_columns(
    key: str,
    val: np.ndarray,
    *,
    allow_matrix_columns: bool,
) -> tuple[dict[str, np.ndarray], str]:
    """配列を検証し、DataFrame 列候補を抽出する。"""
    if not isinstance(val, np.ndarray):
        return {}, f"{key}: 除外 (ndarray 以外: {type(val).__name__})"
    if not np.issubdtype(val.dtype, np.number):
        return {}, f"{key}: 除外 (非数値dtype: {val.dtype}, shape={val.shape})"
    if val.ndim == 0:
        return {key: np.array([val.item()])}, f"{key}: 採用 (scalar -> shape=(1,))"
    if val.ndim == 1:
        return {key: val}, f"{key}: 採用 (1D, shape={val.shape})"
    if val.ndim == 2 and 1 in val.shape:
        return {key: val.reshape(-1)}, f"{key}: 採用 (ベクトル2D, shape={val.shape} -> ({val.size},))"
    if val.ndim == 2 and allow_matrix_columns:
        columns = {f"{key}_{i}": val[:, i] for i in range(val.shape[1])}
        return columns, f"{key}: 採用 (2D行列を列展開, shape={val.shape} -> {len(columns)}列)"
    if val.ndim == 2:
        return {}, f"{key}: 除外 (2D行列; allow_matrix_columns=False, shape={val.shape})"
    return {}, f"{key}: 除外 (3次元以上, shape={val.shape})"


def _build_dataframe(
    data: dict[str, np.ndarray],
    logs: list[str],
    *,
    length_mismatch: str,
) -> pd.DataFrame:
    """列長整合性を検証し DataFrame を構築する。"""
    for line in logs:
        logger.info("MAT reader: %s", line)

    if not data:
        raise ValueError("有効な数値変数が見つかりませんでした")

    lengths = {k: len(v) for k, v in data.items()}
    unique_lengths = sorted(set(lengths.values()))
    if len(unique_lengths) > 1:
        if length_mismatch == "raise":
            raise ValueError(f"列長が一致しません: {lengths}")
        if length_mismatch == "warn_truncate":
            min_len = min(unique_lengths)
            warnings.warn(
                f"列長不一致を検出したため最短長 {min_len} に切り詰めます: {lengths}",
                RuntimeWarning,
                stacklevel=2,
            )
            data = {k: v[:min_len] for k, v in data.items()}
        else:
            raise ValueError(
                "length_mismatch は 'raise' または 'warn_truncate' を指定してください"
            )

    df = pd.DataFrame(data)
    logger.info("MAT reader: 最終DataFrame shape=%s, columns=%s", df.shape, list(df.columns))
    return df
