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
| 小データ手動測定 | 一部完了 | B-01〜B-06、B-08〜B-11を記録済み。B-07は要確認 |
| 中データ準備 | 未着手 | `panely_medium_2m_8ch.parquet` 相当 |
| 大データ準備 | 未着手 | `panely_large_10s_1mhz_9ch.parquet` 相当、または実機ログ |
| 中・大データ測定 | 未着手 | データ作成後に実施 |

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
| B-07 | ズームリセット時間 | 測定不可 | 測定不可 | 測定不可 | N/A | ダブルクリック、Reset axes、AutoscaleでX軸が全体表示へ戻らず。要確認 |
| B-08 | ホバー応答 | 1.599 s | 1.592 s | 1.592 s | 1.599 s | マウス移動開始から値ラベル更新まで。自動操作の移動時間を含む |
| B-09 | Cursor配置応答 | 1.237 s | 1.241 s | 1.240 s | 1.241 s | Cursor A/Bクリック開始から統計パネル表示まで。Bクリック後は約0.62 s |
| B-10 | FFT実行時間 | 0.627 s | 0.633 s | 0.631 s | 0.633 s | FFTボタンからFFTグラフ表示まで。対象はデフォルト先頭ch |
| B-11 | 作図設定ロード時間 | 0.632 s | 0.632 s | 0.624 s | 0.632 s | sampleデータ表示状態から `.pyc.json` ロードでtest_100kの4グラフ復元まで |

### 4.5 体感メモ

- 小データでは読み込み、初回描画、1回ズームとも体感上の引っかかりはほぼない。
- 連続ズーム10回は3試行とも完走し、UIフリーズは確認されなかった。
- B-05/B-06は1行目グラフ上を横方向にドラッグし、最終行のX軸目盛り変更を待ち条件にした。
- B-07は以下を試したが、X軸目盛りがズーム状態から全体表示へ戻らなかった。
  - グラフ上のダブルクリック
  - Plotly modebarの `Reset axes`
  - Plotly modebarの `Autoscale`
  そのため秒数ではなく「測定不可」として記録する。操作方法の問題か、現行Dash版のズーム同期仕様による問題かは別途確認する。
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

## 5. 次に測ること

1. B-07のズームリセット操作を手動でも確認し、現行Dash版の既知リスクにするか判断する
2. 中データを生成または配置する
3. 大データを生成または配置する
4. 中・大データで同じ測定表を追加する
