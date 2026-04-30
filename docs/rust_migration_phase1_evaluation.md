---
project: Panel_y
doc_type: Phase 1評価メモ
target: Rust移植 Phase 1
created: "2026-04-30"
updated: "2026-04-30"
status: Phase 2移行可
source_proto: "code/proto_3_1b"
implementation: "code/rust_phase1"
---

# Rust移植 Phase 1 評価メモ

---

## 1. 結論

Rust Phase 1 の縦切りプロトタイプは、Phase 2へ進める。

採用を継続する方針:

- GUI: `eframe` + `egui`
- Parquet読み込み: Apache Arrow Rust `parquet` crate
- データ保持: `time: f64`, `value: f32` の列指向配列
- 描画用データ: 表示範囲に応じた min/max envelope
- 描画: 当面は `egui::Painter` によるカスタム描画

Phase 1で確認した範囲では、読み込み後の pan/zoom は表示幅オーダーの処理に収まっており、Dash/Plotly版より明確に軽い。

注意点として、当初の文書にある「90M点級」は実ファイルではまだ未測定である。今回の大データ測定は `panely_large_10s_1mhz_9ch.parquet` の 10,000,000 行で行った。Phase 1は「Phase 2移行可」と判断するが、90M点級または実機ログ相当データの補足測定はPhase 2早期の必須確認として残す。

---

## 2. 合格条件に対する結果

| 条件 | 結果 | 根拠 |
|------|------|------|
| `test_100k.parquet` を読み込んで1ch表示できる | 合格 | schema読込、チャンネル選択、1ch表示まで実装済み |
| 大データを読み込んでもアプリが落ちない | 合格 | 10,000,000行データでGUI読込・表示を確認 |
| 大データでpan/zoomが操作不能にならない | 合格 | GUI手動確認で pan/zoom/reset が実用上ストレスなし |
| 表示処理が全点数ではなく表示幅に概ね比例している | 合格 | visible envelope は 1200 buckets 要求、描画点は約2400点 |
| 連続ズームでメモリが増え続けない | 合格 | 1000回の表示範囲 envelope 再抽出でRSSは130.0 MiBのまま |
| Dash/Plotly版より大データの操作感が明確に軽い | 合格 | Rust visible envelope 最大0.0065 s、Dash backendズーム相当最大0.111 s |
| 90M点級データでフリーズしない | 未完了 | 10M行では確認済み。90M点級はPhase 2早期に補足測定する |

---

## 3. 測定結果の要点

### 10M行データ

対象:

- file: `panely_large_10s_1mhz_9ch.parquet`
- channel: `iu`
- rows: 10,000,000
- projected columns: `[0, 1]`

結果:

| 項目 | 結果 |
|------|------|
| array memory | 114.4 MiB |
| Parquet read | 0.278 s |
| full envelope | 0.013 s |
| visible envelope 24 runs | 0.059 s |
| visible envelope avg | 0.0025 s |
| visible envelope max | 0.0065 s |
| stress 1000 runs avg | 0.0032 s |
| stress 1000 runs max | 0.0090 s |
| RSS after load | 130.0 MiB |
| RSS after stress | 130.0 MiB |

GUIでの体感:

- `Load Selected Channel` から波形表示まで約0.5 s
- 表示後の pan/zoom/reset は良好
- 初回表示0.5 s程度は許容範囲

### Dash/Plotly版との比較

同等の大データに対する Dash/Python backend 測定:

| 項目 | Dash/Python backend | Rust Phase 1 |
|------|---------------------|--------------|
| Parquet read | 0.406 s | 0.278 s |
| 初回表示相当 | 0.349 s | GUI体感 約0.5 s |
| zoom相当 max | 0.111 s | visible envelope max 0.0065 s |
| 連続zoom max | 0.150 s | stress max 0.0090 s |
| peak/RSS | 1405.6 MiB peak | 130.0 MiB RSS |

初回表示は測定条件が完全には一致しない。Dash側はbackendのみでブラウザ描画を含まず、Rust側はGUI体感値である。比較上の主な判断材料は、読み込み後の pan/zoom 処理時間とメモリ使用量とする。

---

## 4. 失敗条件に該当しない理由

| 失敗条件 | 判断 |
|----------|------|
| GUI/描画ライブラリの制約でpan/zoomが実装困難 | 該当しない。drag pan、wheel/pinch zoom、横scroll pan、double click resetを実装・確認済み |
| 大データでRust版も全点処理に近くなる | 該当しない。描画点数は元sample数ではなくbucket数に比例している |
| LOD/tile cacheの実装コストが過大 | 現時点では該当しない。full tile pyramidなしでもrange-aware envelopeでPhase 2へ進める |
| Parquet読み込みのライブラリ制約が大きい | 該当しない。projectionで `time` と選択chのみ読めている |
| C++/Qtなど他案の方が明確に短期実用化しやすい | 現時点では該当しない。Rust/eguiで主要リスクを短期間に検証できた |

---

## 5. 技術判断

### Rust移植は継続

10M行データで、読み込み後の表示範囲 envelope 再抽出は最大0.0065 sだった。連続操作相当の1000回再抽出でもRSSは増え続けなかった。

この結果から、Rust版は大容量波形ビューアとして継続する価値がある。

### `egui::Painter` は継続

Phase 1の描画点数は表示幅オーダーに抑えられており、`egui::Painter` が直ちにボトルネックになる兆候はない。

Phase 2では複数行・複数チャンネルで描画点数が増えるため、次の条件に該当したら `wgpu` custom renderer を再検討する。

- 1フレームあたりの描画点数が増え、pan/zoom中に体感遅延が出る
- 複数行表示でCPU側のshape生成が支配的になる
- step表示のエッジ保持で描画点数が増えすぎる

### 本格的なtile pyramidはまだ入れない

Phase 1では、表示範囲ごとに raw array から min/max envelope を再抽出するだけで十分軽かった。

Phase 2の初期実装では、以下を基本方針にする。

- loaded channelごとにraw dataを保持する
- 表示範囲と表示幅に応じてrange-aware envelopeを作る
- 同一channelが複数行に出る場合はenvelopeを共有する
- tile pyramidは、複数ch/複数行または90M点級測定で必要性が出てから追加する

---

## 6. Phase 2へ渡す方針

Phase 2は、`code/rust_phase1` を土台にして基本ビューア機能を増やす。

優先順:

1. 複数チャンネルを同時に読み込めるデータ保持へ拡張する
2. 1行複数チャンネル重ね表示を実装する
3. 複数行表示を実装する
4. X軸 pan/zoom を全行で同期する
5. line / step 表示を分ける
6. Y軸範囲指定、色、線幅を移植する
7. hover同期とチャンネル値表示を移植する
8. 非同期読み込み、progress表示、読み込み中UI制御を入れる
9. 中・大・90M点級で再測定する

設計原則:

- 描画はLODまたはenvelopeを使い、表示ピクセル数オーダーに抑える
- 解析機能はPhase 3以降でも、生データを参照する前提を崩さない
- UI状態、データ保持、描画用抽出、解析を分離する
- 複数行でも同じチャンネルのraw dataとenvelopeを重複保持しない

---

## 7. 残課題

- 90M点級または実機ログ相当データでの測定
- チャンネルを複数同時保持した場合のメモリ上限確認
- step表示でのエッジ保持方式
- 非同期読み込み中のUI設計
- Phase 2以降の作図設定v2の状態モデル

---

## 8. 参照

- [Rust移植 Phase 1 ベンチマーク結果](rust_migration_phase1_benchmark_results.md)
- [Rust移植 Phase 1 範囲定義](rust_migration_phase1_scope.md)
- [Rust移植 Phase 1 技術選定](rust_migration_phase1_tech_selection.md)
- [Rust移植 Phase 0 性能測定結果](rust_migration_phase0_benchmark_results.md)
