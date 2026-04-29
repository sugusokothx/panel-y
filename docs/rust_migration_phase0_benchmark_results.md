---
project: Panel_y
doc_type: 性能測定結果
target: Rust移植 Phase 0
created: "2026-04-29"
updated: "2026-04-29"
status: 測定中
source_proto: "code/proto_3_1b"
plan: "docs/rust_migration_phase0_benchmark_plan.md"
---

# Rust移植 Phase 0 性能測定結果

---

## 1. 目的

Dash/Plotly版 `proto_3_1b` の性能ベースラインを記録し、Rust版Phase 1以降の比較基準にする。

測定計画は `docs/rust_migration_phase0_benchmark_plan.md` に従う。

---

## 2. 測定方針

- 同じデータ・同じ操作を原則3回測定する
- 平均値だけでなく、最悪値と体感メモを残す
- Python/Dashプロセスとブラウザプロセスのメモリは分けて記録する
- ブラウザ操作はばらつきがあるため、秒数とあわせて固まり・遅延・表示崩れを記録する
- Cursor/統計/FFTはRust Phase 3以降向けの参考値として残す

---

## 3. 測定状態

| 項目 | 状態 | メモ |
|------|------|------|
| 結果ファイル作成 | 完了 | 2026-04-29 |
| 小データ環境確認 | 完了 | `test_100k.parquet` を読み込み確認済み |
| 小データ手動測定 | 完了 | B-01〜B-11を記録済み。B-07は手動再確認で成功 |
| 中データ準備 | 完了 | `panely_medium_2m_8ch.parquet` を合成データとして生成済み |
| 大データ準備 | 完了 | `panely_large_10s_1mhz_9ch.parquet` を合成データとして生成済み |
| 中・大データ測定 | 完了（backend） | Dash/Python backend測定を追加済み。ブラウザ描画込みは必要時に補足 |

---

## 4. 測定 2026-04-29: 小データ

### 4.1 環境

記録時刻: 2026-04-29 10:17 JST

| 項目 | 値 |
|------|----|
| OS | macOS 26.4.1 build 25E253 |
| Machine | MacBook Pro MacBookPro18,3 |
| CPU | Apple M1 Pro, 10 cores |
| RAM | 16 GB |
| Python | 3.13.7 |
| Dash | 4.1.0 |
| Plotly | 6.7.0 |
| pandas | 3.0.2 |
| numpy | 2.4.4 |
| pyarrow | 23.0.1 |
| Browser | Codex in-app browser（version未記録） |

### 4.2 対象アプリ

| 項目 | 値 |
|------|----|
| app | `code/proto_3_1b/app.py` |
| 起動コマンド | `cd code/proto_3_1b && source ../.venv/bin/activate && python app.py` |
| URL | `http://localhost:8050` |

### 4.3 対象データ

| 項目 | 値 |
|------|----|
| file | `code/proto_3_1b/data/test_100k.parquet` |
| rows | 100,000 |
| columns | `time`, `sine_50Hz`, `pwm_1kHz`, `noise`, `chirp_1_500Hz` |
| channels | 4 |
| time range | 0.0 s to 0.99999 s |
| size | 3,707,182 bytes |

Parquet読み込み時に `pyarrow` のCPU情報取得警告が出るが、データ読み込み自体は成功した。

作図設定ロード測定では、一時ファイル `/tmp/panely_benchmark_test_100k.pyc.json` を使用した。

### 4.4 結果

| ID | 項目 | Run 1 | Run 2 | Run 3 | 最悪値 | メモ |
|----|------|-------|-------|-------|--------|------|
| B-01 | アプリ起動時間 | 1.368 s | 1.321 s | 1.324 s | 1.368 s | `python app.py` からHTTP 200応答まで |
| B-02 | Parquet読み込み時間 | 0.622 s | 0.627 s | 0.623 s | 0.627 s | 読み込みボタンからステータス表示まで |
| B-03 | 初回波形表示時間 | 0.729 s | 0.625 s | 0.624 s | 0.729 s | 更新ボタンから4グラフ表示まで |
| B-04 | メモリ使用量 | 読込後 241.0 MiB | 連続ズーム後 252.9 MiB | 3回目後 253.5 MiB | 253.5 MiB | Dash親子プロセスRSS合算。ブラウザ単体メモリは未分離 |
| B-05 | 1回ズーム応答 | 0.463 s | 0.465 s | 0.459 s | 0.465 s | 1行目グラフを横ドラッグし、X軸目盛り更新まで |
| B-06 | 連続ズーム耐性 | 10回 / 4.104 s / max 0.465 s | 10回 / 4.105 s / max 0.468 s | 10回 / 4.093 s / max 0.462 s | max 0.468 s | 10回連続ズーム完走。フリーズなし |
| B-07 | ズームリセット時間 | 0.641 s（ダブルクリック） | 0.643 s（Reset axes） | 0.644 s（Autoscale） | 0.644 s | 手動再確認では全操作でX軸が全体表示へ復帰 |
| B-08 | ホバー応答 | 1.599 s | 1.592 s | 1.592 s | 1.599 s | マウス移動開始から値ラベル更新まで。自動操作の移動時間を含む |
| B-09 | Cursor配置応答 | 1.237 s | 1.241 s | 1.240 s | 1.241 s | Cursor A/Bクリック開始から統計パネル表示まで。Bクリック後は約0.62 s |
| B-10 | FFT実行時間 | 0.627 s | 0.633 s | 0.631 s | 0.633 s | FFTボタンからFFTグラフ表示まで。対象はデフォルト先頭ch |
| B-11 | 作図設定ロード時間 | 0.632 s | 0.632 s | 0.624 s | 0.632 s | sampleデータ表示状態から `.pyc.json` ロードでtest_100kの4グラフ復元まで |

### 4.5 体感メモ

- 小データでは読み込み、初回描画、1回ズームとも体感上の引っかかりはほぼない。
- 連続ズーム10回は3試行とも完走し、UIフリーズは確認されなかった。
- B-05/B-06は1行目グラフ上を横方向にドラッグし、最終行のX軸目盛り変更を待ち条件にした。
- B-07は手動再確認で以下すべてが全体表示へ戻ることを確認した。
  - グラフ上のダブルクリック
  - Plotly modebarの `Reset axes`
  - Plotly modebarの `Autoscale`
  再確認では、ズーム後の最終行X軸目盛りが `0.2, 0.3, 0.4` になった状態から、全体表示時の `0, 0.2, 0.4, 0.6, 0.8` に戻ることを判定条件にした。
  前回の「測定不可」は、Dash初期化直後または自動操作タイミングによる不安定さの可能性が高く、現時点では現行Dash版の既知リスクにはしない。
- B-08は自動ブラウザ操作のマウス移動時間を含むため、純粋なDash callback時間ではない。
- B-09はA/B両方のクリック操作を含む。2クリック目から統計パネル表示までは約0.62 sだった。
- B-10はCursor A/B設定済み、FFT対象チャンネルはデフォルトの `sine_50Hz`。
- B-11はリロード直後のクリックが自動操作上不安定だったため、`sample_waveform.parquet` 表示状態から `/tmp/panely_benchmark_test_100k.pyc.json` をロードする手順で測った。
- B-04のDash側RSS内訳:
  - 読み込み後: parent 73.5 MiB + child 167.5 MiB = 241.0 MiB
  - 連続ズーム後: parent 73.5 MiB + child 179.4 MiB = 252.9 MiB
  - 3回目連続ズーム後: parent 73.5 MiB + child 180.0 MiB = 253.5 MiB
- `app.run(debug=True)` のため、Dashはreloader親子2プロセスで起動している。
- Codex in-app browser側のメモリはブラウザ単体プロセスとして分離できなかったため、今回のB-04はDash側RSSを基準値として残す。

---

## 5. 測定 2026-04-29: 中・大データ backend

### 5.1 測定方法

ブラウザAPIが利用できない状態だったため、中・大データは `code/proto_3_1b/benchmark_phase1_datasets.py` で
Dashアプリの関数を直接呼び出して測定した。

含むもの:

- Parquet読み込みとfloat32ダウンキャスト
- `df_overview` 作成
- 波形Figure生成
- Plotly JSON化
- ズーム範囲変更時のFigure再生成
- ホバー値検索
- Cursor A/B区間統計
- FFT計算とFFT Figure JSON化

含まないもの:

- ブラウザ描画時間
- Dash HTTP通信
- ブラウザ側メモリ

そのため、この節の値は小データのブラウザ操作測定とは直接同一条件ではなく、Dash/Python backend側の比較値として扱う。

実行コマンド:

```bash
cd code/proto_3_1b
../.venv/bin/python benchmark_phase1_datasets.py medium --run 1
../.venv/bin/python benchmark_phase1_datasets.py medium --run 2
../.venv/bin/python benchmark_phase1_datasets.py medium --run 3
../.venv/bin/python benchmark_phase1_datasets.py large --run 1
../.venv/bin/python benchmark_phase1_datasets.py large --run 2
../.venv/bin/python benchmark_phase1_datasets.py large --run 3
```

### 5.2 対象データ

| 区分 | file | rows | columns | size | メモ |
|------|------|------|---------|------|------|
| 中 | `panely_medium_2m_8ch.parquet` | 2,000,000 | `time` + 8 ch | 18,426,601 bytes | 100 kHz, 約20 s |
| 大 | `panely_large_10s_1mhz_9ch.parquet` | 10,000,000 | `time` + 9 ch | 292,253,458 bytes | 1 MHz, 約10 s |

### 5.3 結果

| ID | 項目 | 中 Run 1 | 中 Run 2 | 中 Run 3 | 中 最悪値 | 大 Run 1 | 大 Run 2 | 大 Run 3 | 大 最悪値 | メモ |
|----|------|----------|----------|----------|-----------|----------|----------|----------|-----------|------|
| B-02 | Parquet読み込み | 0.188 s | 0.085 s | 0.153 s | 0.188 s | 0.406 s | 0.223 s | 0.232 s | 0.406 s | `load_file()` |
| B-03 | 初回波形表示相当 | 0.319 s | 0.267 s | 0.332 s | 0.332 s | 0.349 s | 0.336 s | 0.288 s | 0.349 s | 行更新 + Figure生成 + JSON化 |
| B-04 | backendピークRSS | 372.0 MiB | 374.3 MiB | 376.1 MiB | 376.1 MiB | 1222.8 MiB | 1405.6 MiB | 1404.3 MiB | 1405.6 MiB | 解析後までのプロセスpeak |
| B-05 | 1回ズーム応答相当 | 0.102 s | 0.098 s | 0.108 s | 0.108 s | 0.111 s | 0.110 s | 0.110 s | 0.111 s | 表示範囲20%〜40% |
| B-06 | 連続ズーム耐性 | 10回 / 1.045 s / max 0.131 s | 10回 / 1.034 s / max 0.127 s | 10回 / 1.111 s / max 0.139 s | total 1.111 s / max 0.139 s | 10回 / 1.127 s / max 0.150 s | 10回 / 1.124 s / max 0.142 s | 10回 / 1.122 s / max 0.141 s | total 1.127 s / max 0.150 s | backend上はフリーズなし |
| B-07 | ズームリセット相当 | 0.099 s | 0.099 s | 0.106 s | 0.106 s | 0.108 s | 0.107 s | 0.118 s | 0.118 s | 全体表示Figure再生成 |
| B-08 | ホバー値更新相当 | 0.010 s | 0.006 s | 0.008 s | 0.010 s | 0.025 s | 0.029 s | 0.032 s | 0.032 s | `update_values()` |
| B-09 | Cursor統計相当 | 0.013 s | 0.012 s | 0.015 s | 0.015 s | 0.054 s | 0.054 s | 0.054 s | 0.054 s | 10%〜20%区間 |
| B-10 | FFT実行相当 | 0.046 s | 0.041 s | 0.062 s | 0.062 s | 0.189 s | 0.163 s | 0.163 s | 0.189 s | 先頭ch、10%〜20%区間 |
| B-11 | 作図設定ロード | 未測定 | 未測定 | 未測定 | N/A | 未測定 | 未測定 | 未測定 | N/A | Phase 1対象外のため今回省略 |

### 5.4 体感・設計メモ

- backendだけを見る限り、2M行・10M行ともMin-Max envelopeによりズーム再描画は表示幅オーダーに抑えられている。
- 大データのbackendピークRSSは最大約1.4 GiBだった。Rust Phase 1では同等データでメモリ使用量を主要比較項目にする。
- 大データのFFTは1秒区間、約1,000,000サンプルで最悪0.189 sだった。Rust Phase 1ではFFTは対象外だが、Phase 3で比較する参考値として残す。
- `pyarrow` のCPU情報取得警告は中・大データ測定でも出たが、読み込みと計算は成功した。

---

## 6. 次に測ること

1. 必要ならブラウザ操作込みで中・大データの表示確認を補足する
2. Rust Phase 1プロトタイプで同じ中・大データを読み込み、読み込み・初回表示・pan/zoom・メモリを比較する
