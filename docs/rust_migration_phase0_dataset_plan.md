---
project: Panel_y
doc_type: データセット定義
target: Rust移植 Phase 0
created: "2026-04-27"
updated: "2026-04-27"
status: 完了
source_proto: "code/proto_3_1b"
---

# Rust移植 Phase 0 データセット定義

---

## 1. 目的

Rust版の性能・描画・解析を比較するため、代表データセットを小・中・大に分けて定義する。
大容量データはGit管理せず、保存場所または生成手順だけを記録する。

---

## 2. 既存データ

### sample_waveform.parquet

| 項目 | 値 |
|------|----|
| パス | `code/proto_3_1b/data/sample_waveform.parquet` |
| 行数 | 1,000 |
| カラム | `time`, `voltage_u`, `current_u` |
| time範囲 | 0.0 s から 0.0999 s |
| Ts | 0.0001 s |
| 用途 | 最小動作確認 |

### test_100k.parquet

| 項目 | 値 |
|------|----|
| パス | `code/proto_3_1b/data/test_100k.parquet` |
| 行数 | 100,000 |
| カラム | `time`, `sine_50Hz`, `pwm_1kHz`, `noise`, `chirp_1_500Hz` |
| time範囲 | 0.0 s から 0.99999 s |
| Ts | 0.00001 s |
| Fs | 100 kHz |
| 用途 | 開発中の標準確認、解析リファレンス |

生成元:

- `code/proto_3_1b/generate_test_data.py`

---

## 3. データセット区分

| 区分 | 目安 | 候補 | 用途 | Git管理 |
|------|------|------|------|---------|
| 小 | 1kから100k点 | `sample_waveform.parquet`, `test_100k.parquet` | 開発中の高速確認 | 原則しない。既存は作業用 |
| 中 | 数百万点 | 合成データまたは実機ログ | 実務に近い操作感確認 | しない |
| 大 | 90M点級 | 実機ログまたは専用合成データ | 性能限界確認 | しない |

---

## 4. 小データ

採用:

- `code/proto_3_1b/data/test_100k.parquet`

理由:

- チャンネル種類が揃っている
- アナログ波形、PWM、ノイズ、チャープを含む
- FFTと統計の比較に使いやすい
- 生成スクリプトがある

確認項目:

- Parquet読み込み
- チャンネル一覧取得
- 1ch表示
- 複数ch重ね表示
- step表示
- Cursor/統計
- FFT

---

## 5. 中データ

採用定義:

| 項目 | 値 |
|------|----|
| 論理名 | `panely_medium_2m_8ch` |
| 推奨ファイル名 | `panely_medium_2m_8ch.parquet` |
| 行数 | 2,000,000 |
| チャンネル数 | 8 |
| time範囲 | 0.0 s から約 19.99999 s |
| Ts | 0.00001 s |
| Fs | 100 kHz |
| 用途 | 実務に近い操作感、LOD、pan/zoom、メモリ確認 |

信号構成:

- `sine_50Hz`
- `sine_1kHz`
- `pwm_1kHz`
- `pwm_10kHz`
- `noise`
- `chirp_1_500Hz`
- `step_signal`
- `spike_signal`

作成方針:

- Phase 1前に合成データとして生成する
- 実機ログが使える場合でも、この合成仕様は比較用として残す
- `test_100k.parquet` の生成スクリプトを拡張して作る

保存方針:

- `code/proto_3_1b/data/` に置いてもGitには追加しない
- 文書にはファイル名、生成手順、ハッシュを記録する

生成済みファイル:

| 項目 | 値 |
|------|----|
| 生成日 | 2026-04-29 |
| パス | `code/proto_3_1b/data/panely_medium_2m_8ch.parquet` |
| 生成コマンド | `cd code/proto_3_1b && ../.venv/bin/python generate_phase1_datasets.py medium` |
| 行数 | 2,000,000 |
| カラム数 | 9 (`time` + 8 ch) |
| row groups | 10 |
| ファイルサイズ | 18,426,601 bytes |
| sha256 | `8a3bf8d753df196ae121ecba705070b9a1f23779602beec8bc99719ba96fee71` |

---

## 6. 大データ

採用定義:

| 項目 | 値 |
|------|----|
| 論理名 | `panely_large_10s_1mhz_9ch` |
| 推奨ファイル名 | `panely_large_10s_1mhz_9ch.parquet` |
| 行数 | 10,000,000 |
| チャンネル数 | 9 |
| 総波形点数 | 90,000,000 |
| time範囲 | 0.0 s から約 9.999999 s |
| Ts | 0.000001 s |
| Fs | 1 MHz |
| 用途 | 性能限界、メモリ、連続ズーム、LOD/tile cache検証 |

信号構成:

- `iu`
- `iv`
- `iw`
- `vu`
- `vv`
- `vw`
- `torque`
- `gate_pwm`
- `spike_event`

用途:

- Rust移植の主目的である性能限界確認
- 連続ズーム時のUI応答性確認
- メモリ使用量確認
- LOD/tile cache設計の妥当性確認

保存方針:

- Git管理しない
- ローカルまたは社内共有ストレージに置く
- パス、データ仕様、取得日だけ文書化する
- 機密データの場合は匿名化または合成データに置き換える

必要メタ情報:

- ファイル名
- 行数
- チャンネル数
- time範囲
- Ts/Fs
- ファイルサイズ
- データ由来
- 使用可否

作成方針:

- 実機ログが利用可能なら実機ログを優先する
- 実機ログを使えない場合は同じ行数・チャンネル数の合成データを生成する
- 合成データの場合も、PWM、スパイク、チャープ、ノイズを含める

生成済みファイル:

| 項目 | 値 |
|------|----|
| 生成日 | 2026-04-29 |
| パス | `code/proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet` |
| 生成コマンド | `cd code/proto_3_1b && ../.venv/bin/python generate_phase1_datasets.py large` |
| 行数 | 10,000,000 |
| カラム数 | 10 (`time` + 9 ch) |
| row groups | 40 |
| ファイルサイズ | 292,253,458 bytes |
| sha256 | `1a092f5e6da3d158c4156b5cc338600f5dbe61ff0059e0a135c72a4274b6fc2e` |

---

## 7. Phase 1で必須のデータ

Phase 1開始時点で最低限必要なもの:

- 小: `test_100k.parquet`
- 大: `panely_large_10s_1mhz_9ch.parquet`、または同等仕様の実機ログ

中データ `panely_medium_2m_8ch.parquet` はPhase 1中に追加でもよい。

---

## 8. 再生成方針

小データ:

```bash
cd code/proto_3_1b
source ../.venv/bin/activate
python generate_test_data.py
```

中・大データ:

- `code/proto_3_1b/generate_phase1_datasets.py` を使う
- 出力ファイルは `.gitignore` 対象にする

```bash
cd code/proto_3_1b
../.venv/bin/python generate_phase1_datasets.py medium
../.venv/bin/python generate_phase1_datasets.py large
```

---

## 9. 未決定事項

- 実機ログを使う場合の機密扱い
