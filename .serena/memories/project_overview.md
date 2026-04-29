# Panel-y — プロジェクト概要

## 目的
計測データ（.mat / .csv / .parquet）を読み込み、波形をインタラクティブに可視化・分析するデスクトップWebアプリ。
パワーエレクトロニクス・モータ制御データの解析ツール（会社業務用）。

## Tech Stack
- **言語**: Python 3
- **Webフレームワーク**: Dash (>= 2.16.0) + Plotly (>= 5.20.0)
- **データ処理**: pandas (>= 2.0.0), numpy (>= 1.26.0), scipy (>= 1.12.0)
- **ファイル形式**: .mat (scipy/h5py), .csv, .parquet (pyarrow)
- **フロントエンド**: CSS変数によるDark/Lightテーマ切替、JavaScriptリサイズ同期

## アーキテクチャ (Proto 3.1 — 現行最新)
- 各行は独立した `go.Figure()`（make_subplotsは不使用）
- `hover-x-store` / `xaxis-range-store` を介して全行同期
- pattern-matching callback（ALL）で動的行数に対応
- Min-Max envelope による描画最適化（閾値超過時に自動適用）
- ズーム連動で envelope ↔ 生データを自動切り替え

## ディレクトリ構成
```
code/proto_3_1/          ← 現行最新プロトタイプ
├── app.py               ← メインDashアプリ（UIレイアウト＋コールバック）
├── multiconverter.py    ← .mat一括Parquet変換CLI
├── generate_test_data.py← テストデータ生成
├── requirements.txt     ← 依存パッケージ
├── assets/              ← CSS (style.css) + JS (resize_sync.js)
├── data/                ← データファイル置き場
└── utils/               ← ユーティリティモジュール
    ├── reader_mat.py    ← .mat読み込み (scipy/h5py)
    ├── reader_csv.py    ← .csv読み込み
    ├── converter.py     ← Parquet変換
    ├── resampler.py     ← リサンプリング（等間隔化）
    ├── time_normalizer.py ← 時間列検出・正規化
    └── column_filter.py ← 数値列フィルタリング
```

以前のプロトタイプ (proto_0_x ~ proto_3_0) は `code/old_type/` およびそれぞれのディレクトリに保存。
