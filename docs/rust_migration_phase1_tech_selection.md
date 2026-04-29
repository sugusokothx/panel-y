---
project: Panel_y
doc_type: 技術選定
target: Rust移植 Phase 1
created: "2026-04-29"
updated: "2026-04-29"
status: 仮決定
source_proto: "code/proto_3_1b"
scope: "docs/rust_migration_phase1_scope.md"
---

# Rust移植 Phase 1 技術選定 — 検証用仮採用セット

---

## 1. 目的

Phase 1では、完成版の技術スタックを確定しない。
大容量Parquetを読み、1チャンネル波形を軽く表示し、pan/zoomできるかを最短で検証するため、
実装を始めるための仮採用セットを決める。

この文書の判断は、Phase 1の測定結果で見直す。

---

## 2. 仮採用セット

| 項目 | 仮採用 | 理由 |
|------|--------|------|
| アプリ/GUI | `eframe` + `egui` | Rustネイティブで早くUIを組める。Phase 1の1画面プロトタイプに向く |
| GUIレンダラ | `eframe` の `wgpu` backendを優先 | 将来のGPUカスタム描画に寄せるため。動作不安定なら `glow` backendへ一時退避 |
| Parquet読み込み | Apache Arrow Rust `parquet` crate | Parquet/Arrowの公式Rust実装。必要列だけ読む検証に向く |
| Parquet代替 | `polars` lazy `scan_parquet` | 直接Arrow APIで詰まった場合のフォールバック。依存と抽象化は重くなる |
| データ保持 | `time: Arc<[f64]>`, `value: Arc<[f32]>` | time精度を維持し、波形値は現行方針に合わせてfloat32化する |
| チャンネル保持 | Phase 1では選択中1chのみfull resolution保持 | 90M点級ファイルでのメモリ上限を先に確認する |
| 範囲検索 | `slice::partition_point` 相当 | `time` 単調配列から表示範囲のindexを高速に得る |
| LOD | 選択chごとのmin/max tile pyramid | 全体表示や広域ズームでも描画点数を画面幅オーダーへ制限する |
| 描画 | `egui` のカスタム描画領域 + LOD済みline/envelope | `egui_plot`任せにせず、描画へ渡す点数を明示制御する |
| pan/zoom | 自前の `ViewRange` 状態で管理 | Plot widget内部状態に依存せず、後続の複数行同期へ拡張しやすくする |
| 非同期処理 | UI thread + worker thread | Parquet読み込みとLOD構築でUIを固めない |
| 計測 | `std::time::Instant` + OSメモリ計測 | Phase 1では実装を軽く保ち、読み込み/LOD/描画の時間を分けて記録する |

---

## 3. 採用しないもの

| 候補 | Phase 1で採用しない理由 |
|------|--------------------------|
| Tauri + Plotly.js | 現行の主ボトルネックであるブラウザ/Plotly描画パイプラインを残す |
| Dash継続 | Phase 1の目的がDash/Plotly限界の回避であるため |
| `egui_plot` 主描画 | 便利だが、90M点級での点数制御とLOD切替をブラックボックス化したくない |
| 直接 `winit` + `wgpu` | 描画制御は強いが、Phase 1のUI実装コストが上がる |
| Iced / Slint | 一般UIとしては候補だが、波形描画と低レベル描画制御の検証を優先する |
| 最初から全ch読み込み | Phase 1の主リスクは1ch縦切りで検証できる。メモリを先に悪化させない |
| 最初から作図設定互換 | Phase 1範囲外。`.pyc.json`互換はPhase 2以降で扱う |

---

## 4. アーキテクチャ仮案

```
eframe app
  │
  ├─ UI state
  │   ├─ parquet path
  │   ├─ selected channel
  │   └─ ViewRange { x_min, x_max }
  │
  ├─ Loader worker
  │   ├─ Parquet schema read
  │   ├─ time列検証
  │   ├─ channel列検出
  │   └─ time + selected channel の列読み込み
  │
  ├─ WaveStore
  │   ├─ time: Arc<[f64]>
  │   ├─ value: Arc<[f32]>
  │   └─ metadata
  │
  ├─ LodStore
  │   ├─ raw threshold
  │   ├─ min/max base tiles
  │   └─ min/max pyramid levels
  │
  └─ WaveformView
      ├─ index range extraction
      ├─ raw line or envelope point generation
      └─ egui Painter drawing
```

---

## 5. Parquet読み込み方針

Phase 1では、まずApache Arrow Rustの `parquet` crateで実装する。

読み込みの順序:

1. ファイルパスの拡張子を確認する
2. schemaを読み、`time` 列を探す
3. `time` 以外の数値列をチャンネル候補として列挙する
4. `time` と選択中1chだけを読み込む
5. `time` を `f64`、波形値を `f32` の連続配列へ変換する
6. `time` が非単調なら、time/valueを安定ソートする
7. 読み込み時間、変換時間、ピークメモリを記録する

確認ポイント:

- 必要列だけのprojectionができるか
- Arrow chunkから連続配列へ変換するときのコピー回数
- 10M rows x 9ch級ファイルで、選択chだけ読めるか
- 圧縮方式による読み込み時間差

フォールバック:

- 直接APIでprojectionや型変換の実装コストが大きい場合は、`polars` lazy `scan_parquet` + `select([time, channel])` を試す
- それでもメモリが大きい場合は、row group単位の読み込みか、time列の圧縮保持を検討する

---

## 6. データ保持方針

Phase 1の基本形:

```rust
struct WaveStore {
    time: Arc<[f64]>,
    value: Arc<[f32]>,
    channel_name: String,
    sample_count: usize,
    x_min: f64,
    x_max: f64,
    y_min: f32,
    y_max: f32,
}
```

方針:

- Phase 1ではDataFrameを保持しない
- 読み込み後は描画に必要な連続配列だけを保持する
- `time` は解析互換を考えて `f64` を維持する
- 波形値は現行Dash版と同様に `f32` を基本にする
- 全チャンネル同時保持はPhase 2以降で検討する

見直し条件:

- time配列だけでメモリが重すぎる場合は、等間隔データを `start + dt + len` として保持する
- 非等間隔データが多い場合は、`Arc<[f64]>` を継続する
- `f32` 化でピーク値や表示に問題が出る場合は、値配列の `f64` 保持を比較する

---

## 7. LOD / tile cache方針

Phase 1では、選択中1chに対してmin/max tile pyramidを作る。

基本ルール:

- `time` は単調増加配列として扱う
- 表示範囲 `[x_min, x_max]` をindex範囲へ変換する
- samples per pixel が小さい場合はraw lineを描く
- samples per pixel が大きい場合はmin/max envelopeを描く
- envelopeはピークを落とさないことを優先する

仮パラメータ:

| 項目 | 初期値 | メモ |
|------|--------|------|
| raw描画上限 | `4 samples / pixel` 以下 | まずは保守的に設定 |
| base tile size | `256 samples` | 構築コストとピーク保持のバランスを見る |
| pyramid fanout | `4` | level数と選択コストを抑える |
| 描画点数目標 | `2〜4 * viewport_width_px` | vertical envelopeを含めても画面幅オーダーにする |

Tileの保持候補:

```rust
struct MinMaxTile {
    start_index: u32,
    end_index: u32,
    min_value: f32,
    max_value: f32,
    min_index: u32,
    max_index: u32,
}
```

注意:

- 10M rows級なら `u32` indexで足りるが、将来の巨大データでは `usize` へ切り替える余地を残す
- Phase 1では選択chごとにLODを作り直してよい
- チャンネル切替時のキャッシュ共有はPhase 2以降で扱う

---

## 8. 描画方針

Phase 1では、`egui` の中央領域に独自の波形ビューを作る。

描画の責務:

- plot rectを決める
- `ViewRange` から表示index範囲を得る
- raw lineまたはmin/max envelopeの描画点を作る
- `egui::Painter` へ画面幅オーダーの点だけ渡す
- wheel/drag操作を `ViewRange` に反映する

`egui_plot` は主描画に使わない。
ただし、初期の座標変換や操作感確認で短時間使うだけなら許容する。

見直し条件:

- LOD済み点だけでも `egui::Painter` が遅い場合は、`wgpu` custom rendererへ進む
- `eframe` のbackend差で描画が不安定な場合は、`glow` backendも比較する
- pan/zoom操作の自由度が足りない場合は、直接 `winit` + `wgpu` を再検討する

---

## 9. Phase 1実装順

1. `code/rust_phase1` にRustプロジェクトを作る
2. `eframe` で空ウィンドウを表示する
3. 固定パスのParquet schemaを表示する
4. `time` と1chを読み込む
5. `WaveStore` に変換する
6. 全体範囲をrawまたはmin/max envelopeで表示する
7. wheel zoomとdrag panを追加する
8. LOD/tile cacheを入れる
9. チャンネル選択UIを追加する
10. 小/大データで読み込み、初回表示、pan/zoom、メモリを測る

---

## 10. 合格条件

この仮採用セットは、以下を満たしたらPhase 2へ進める候補とする。

- `test_100k.parquet` を読み込んで1ch表示できる
- `panely_large_10s_1mhz_9ch.parquet` 相当を読み込んでも落ちない
- pan/zoom中にUIが操作不能にならない
- 表示処理が全点数ではなく画面幅オーダーに収まる
- 連続ズームでメモリが増え続けない
- Dash/Plotly版より大データの操作感が明確に軽い

---

## 11. 見直し条件

以下の場合は、この仮採用セットを見直す。

- `eframe` / `egui` で必要なpan/zoom操作が自然に作れない
- LOD済み点だけを渡しても描画が重い
- Parquet読み込みのprojectionが期待通りに効かない
- `time: f64` + `value: f32` の1ch保持でもメモリが厳しい
- LOD構築時間が長く、初回表示を大きく遅らせる
- 実装量がPhase 1の縦切り検証として重すぎる

見直し先:

| 問題 | 次の候補 |
|------|----------|
| GUI/描画が重い | 直接 `winit` + `wgpu` |
| `egui::Painter` が重い | `wgpu` custom renderer |
| Parquet直接APIが重い | `polars` lazy |
| time配列メモリが重い | 等間隔time圧縮 |
| LOD構築が重い | lazy tile build / row group単位LOD |

---

## 12. 参照

- [Rust移植 Phase 1 範囲定義](rust_migration_phase1_scope.md)
- [Rust移植計画書](rust_migration_plan.md)
- [Rust移植 Phase 0 性能測定結果](rust_migration_phase0_benchmark_results.md)
- [eframe docs.rs](https://docs.rs/eframe/latest/eframe/)
- [egui docs.rs](https://docs.rs/egui/latest/egui/)
- [Apache Arrow Rust parquet docs.rs](https://docs.rs/parquet/latest/parquet/)
- [Polars docs.rs](https://docs.rs/polars/latest/polars/)
- [wgpu docs.rs](https://docs.rs/wgpu/latest/wgpu/)
