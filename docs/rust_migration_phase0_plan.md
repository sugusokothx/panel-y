---
project: Panel_y
doc_type: 詳細計画書
target: Rust移植 Phase 0
created: "2026-04-27"
updated: "2026-04-27"
status: 完了
source_proto: "code/proto_3_1b"
parent_plan: "docs/rust_migration_plan.md"
---

# Rust移植 Phase 0 詳細計画 — `proto_3_1b` 仕様固定

---

## 1. 目的

Phase 0 では、Rust版の実装を始める前に `proto_3_1b` を仕様リファレンスとして固定する。

この段階ではRustコードを書かない。目的は、後続Phaseで「何を再現するか」「何を性能比較するか」
「どの結果を正とするか」を明確にすることである。

---

## 2. 完了条件

Phase 0 は、以下が揃った時点で完了とする。

- `proto_3_1b` の機能チェックリストがある
- 主要UI操作の仕様が文章またはスクリーンショットで記録されている
- `.pyc.json` 作図設定の保存項目と互換方針が整理されている
- 小・中・大の代表データセットが定義されている
- Dash/Plotly版の性能ベースライン測定項目が定義されている
- Cursor/統計/FFT の数値比較用リファレンスが定義されている
- Rust版Phase 1で実装する範囲と、後回しにする範囲が決まっている

---

## 3. Phase 0で作る成果物

| 成果物 | 保存先案 | 内容 |
|--------|----------|------|
| 機能チェックリスト | `docs/rust_migration_phase0_feature_checklist.md` | Rust版で再現する機能の一覧 |
| UI操作仕様 | `docs/rust_migration_phase0_ui_spec.md` | 現行UIの画面構成・操作・期待動作 |
| 作図設定仕様 | `docs/rust_migration_phase0_plotconfig_spec.md` | `.pyc.json` の項目、互換方針、サンプル |
| データセット定義 | `docs/rust_migration_phase0_dataset_plan.md` | 小・中・大データの用途、作成/保管方針 |
| 性能測定計画 | `docs/rust_migration_phase0_benchmark_plan.md` | 測定項目、手順、判定基準 |
| 数値リファレンス計画 | `docs/rust_migration_phase0_analysis_reference.md` | 統計/FFTの比較対象と許容誤差 |
| Phase 1範囲定義 | `docs/rust_migration_phase1_scope.md` | Rust縦切りプロトタイプの実装範囲 |

大量データやスクリーンショットはGit管理対象にしない可能性があるため、文書側には保存場所と再生成手順を記録する。

---

## 4. 作業分解

### 0-1. 仕様固定対象の確認

対象:

- [code/proto_3_1b/app.py](/Users/sugusokothx/life/factory/panel-y/code/proto_3_1b/app.py)
- [code/proto_3_1b/assets/style.css](/Users/sugusokothx/life/factory/panel-y/code/proto_3_1b/assets/style.css)
- [code/proto_3_1b/assets/resize_sync.js](/Users/sugusokothx/life/factory/panel-y/code/proto_3_1b/assets/resize_sync.js)
- [code/proto_3_1b/utils/converter.py](/Users/sugusokothx/life/factory/panel-y/code/proto_3_1b/utils/converter.py)
- [docs/proto_3_1b_app_intro_slides.md](/Users/sugusokothx/life/factory/panel-y/docs/proto_3_1b_app_intro_slides.md)
- [docs/wave_viewer_roadmap.md](/Users/sugusokothx/life/factory/panel-y/docs/wave_viewer_roadmap.md)

確認内容:

- `proto_3_1b` を機能開発完了版として扱う
- `proto_3_1` との差分を確認し、Rust版の正は `proto_3_1b` と明記する
- 既知バグや未反映差分を洗い出す

注意:

- `proto_3_1` 側にexe化済み `multiconverter.py` があり、`proto_3_1b` 側は旧CLI形である。
  Rust版ビューア本体の仕様固定では `proto_3_1b` を優先し、変換CLIは別扱いにする。

成果物:

- `docs/rust_migration_phase0_feature_checklist.md`

---

### 0-2. 機能チェックリスト作成

カテゴリ:

- データ読み込み
- 行レイアウト
- 波形描画
- チャンネル表示設定
- X/Y軸操作
- ホバー同期
- 計測モード
- 区間統計
- FFT
- 作図設定保存/ロード
- テーマ/ヘッダー/ショートカット
- 前処理・変換ツール

各機能に以下を付ける。

| 項目 | 説明 |
|------|------|
| 現行状態 | `proto_3_1b` で動作済み / 要確認 / 既知リスク |
| Rust版優先度 | Phase 1 / Phase 2 / Phase 3 / Phase 4 / 後回し |
| 再現度 | 完全再現 / 仕様変更可 / 廃止候補 |
| 確認方法 | 手動操作、数値比較、スクリーンショットなど |

成果物:

- `docs/rust_migration_phase0_feature_checklist.md`

---

### 0-3. UI操作仕様の記録

記録する画面・操作:

- 初期画面
- Parquetファイル読み込み
- パス候補表示
- 行追加・削除
- 複数チャンネル重ね表示
- step表示設定
- 色・線幅設定
- Y軸範囲設定
- 更新ボタン / Ctrl+Enter
- スクロールズーム固定切替
- テーマ切替
- ヘッダーピン固定
- ホバー値表示
- X軸ズーム・パン同期
- 計測モードON/OFF
- Cursor A/B配置
- 区間統計表示
- FFT実行
- 作図設定保存
- 作図設定ロード

記録方法:

- 操作仕様はMarkdownで記述する
- 必要に応じてスクリーンショットを `docs/rust_migration_phase0_assets/` に置く
- 大きな画像をGit管理しない場合は、再取得手順だけを記録する

成果物:

- `docs/rust_migration_phase0_ui_spec.md`

---

### 0-4. 作図設定 `.pyc.json` 仕様整理

整理する項目:

- `version`
- `data_path`
- `rows`
- `rows[].channels`
- `rows[].ymin`
- `rows[].ymax`
- `rows[].step_channels`
- `ch_styles`
- `ch_styles.{channel}.color`
- `ch_styles.{channel}.width`
- `theme`
- `scroll_zoom`

決めること:

- Rust版は現行 `.pyc.json` を読み込み互換対象にする
- Rust版固有項目が必要な場合は `version: 2` とする
- 拡張子は当面 `.pyc.json` 互換を維持する
- 将来の正式拡張子候補として `.panely.json` を検討する

成果物:

- `docs/rust_migration_phase0_plotconfig_spec.md`
- サンプル設定ファイルのパス一覧

---

### 0-5. 代表データセット定義

データセットの種類:

| 種別 | 目安 | 用途 |
|------|------|------|
| 小 | 100k点 | 開発中の高速確認、CI候補 |
| 中 | 数百万点 | 実務に近い操作感確認 |
| 大 | 90M点級 | 性能限界・メモリ・連続ズーム確認 |

既存候補:

- [code/proto_3_1b/data/test_100k.parquet](/Users/sugusokothx/life/factory/panel-y/code/proto_3_1b/data/test_100k.parquet)
- [code/proto_3_1b/generate_test_data.py](/Users/sugusokothx/life/factory/panel-y/code/proto_3_1b/generate_test_data.py)

決めること:

- 中・大データを実機ログにするか、合成データにするか
- 大容量データはGit管理しない
- 生成スクリプトまたは取得場所を文書化する
- 各データのチャンネル数、サンプリング周波数、記録長を記録する

成果物:

- `docs/rust_migration_phase0_dataset_plan.md`

---

### 0-6. Dash/Plotly版の性能ベースライン計画

測定項目:

- アプリ起動時間
- Parquet読み込み時間
- 初回表示までの時間
- メモリ使用量
- 1回ズーム後の応答時間
- 連続ズーム後の応答性
- ズームリセット時間
- FFT実行時間
- 作図設定ロード時間

測定対象:

- 小データ
- 中データ
- 大データ

記録方法:

- 測定日時
- OS / CPU / RAM
- Pythonバージョン
- Dash / Plotly / pandas / numpy バージョン
- データセット情報
- 操作手順
- 結果

成果物:

- `docs/rust_migration_phase0_benchmark_plan.md`
- 後続で `docs/rust_migration_phase0_benchmark_results.md` を作成

---

### 0-7. 解析結果の数値リファレンス定義

対象:

- Cursor A/B の値
- Δt
- 周波数換算
- Mean
- Max / Min
- P-P
- RMS
- Slope
- FFT周波数ピーク
- FFT振幅
- dB表示

決めること:

- 比較に使うデータセット
- Cursor A/B の固定時刻
- FFT対象チャンネル
- 窓関数
- 周波数レンジ
- 許容誤差

成果物:

- `docs/rust_migration_phase0_analysis_reference.md`

---

### 0-8. Phase 1スコープ定義

Phase 1で実装するもの:

- Rustアプリ起動
- Parquet読み込み
- チャンネル一覧取得
- 1ch選択
- 1行波形表示
- pan / zoom
- min/max envelope
- 90M点級データでの基本性能確認

Phase 1で実装しないもの:

- 複数行UI
- 複数チャンネル重ね表示
- Cursor
- FFT
- 作図設定保存/ロード
- テーマ切替
- 前処理CLI再実装

成果物:

- `docs/rust_migration_phase1_scope.md`

---

## 5. 推奨順序

1. 機能チェックリストを作る
2. UI操作仕様を作る
3. 作図設定仕様を作る
4. データセット定義を作る
5. 性能測定計画を作る
6. 解析リファレンス計画を作る
7. Phase 1スコープを確定する

この順序にする理由:

- 先に機能とUIを固定しないと、後続の性能測定やRust版スコープがぶれる
- 作図設定は互換性に関わるため、早い段階で固定する必要がある
- 性能測定は対象データが決まってから設計する方が再現性が高い

---

## 6. Phase 0の非目標

Phase 0では以下を行わない。

- Rust実装
- GUIライブラリの最終決定
- 大規模なPython版改修
- `proto_3_1b` の機能追加
- 大容量データのGit追加
- 自動テスト基盤の本格整備

ただし、仕様固定に必要な軽微な調査や測定補助スクリプトの作成は、必要になった時点で別タスクとして扱う。

---

## 7. 判断ゲート

Phase 0完了時に以下を判断する。

| 判断 | 条件 |
|------|------|
| Phase 1開始 | 再現対象、データセット、性能測定項目、Phase 1スコープが明確 |
| Phase 0延長 | 現行仕様に未確認部分が多い、または大データ基準が未定 |
| 移植方針再検討 | Rust版で再現すべきUI/描画要件がRust GUI候補と明らかに合わない |

---

## 8. 最初に着手する1ファイル

最初に作るべき成果物は `docs/rust_migration_phase0_feature_checklist.md`。

理由:

- Rust版の実装範囲を決める土台になる
- UI仕様、作図設定、性能測定、Phase 1スコープのすべてがこの一覧に依存する
- `proto_3_1b` の「完成」の中身を明文化できる

---

## 9. 実行結果

Phase 0の成果物として以下を作成した。

| 成果物 | パス |
|--------|------|
| 機能チェックリスト | `docs/rust_migration_phase0_feature_checklist.md` |
| UI操作仕様 | `docs/rust_migration_phase0_ui_spec.md` |
| 作図設定仕様 | `docs/rust_migration_phase0_plotconfig_spec.md` |
| データセット定義 | `docs/rust_migration_phase0_dataset_plan.md` |
| 性能測定計画 | `docs/rust_migration_phase0_benchmark_plan.md` |
| 数値リファレンス | `docs/rust_migration_phase0_analysis_reference.md` |
| Phase 1範囲定義 | `docs/rust_migration_phase1_scope.md` |

補足:

- 小データとして `test_100k.parquet` を採用した
- 中データ仕様として `panely_medium_2m_8ch` を定義した
- 大データ仕様として `panely_large_10s_1mhz_9ch` を定義した
- 区間統計とFFTの固定リファレンス値を計算して記録した
- Dash/Plotly版の実測結果は未取得で、測定計画のみ作成した
- 中・大データの実ファイルは未生成で、Phase 1前に実機ログまたは合成データとして用意する
