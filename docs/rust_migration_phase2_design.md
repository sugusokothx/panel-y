---
project: Panel_y
doc_type: Phase 2設計方針
target: Rust移植 Phase 2
created: "2026-04-30"
updated: "2026-04-30"
status: P2-12測定完了
source_proto: "code/proto_3_1b"
base_implementation: "code/rust_phase1"
---

# Rust移植 Phase 2 設計方針

---

## 1. 目的

Phase 2では、Rust Phase 1で確認した「大容量Parquet読み込み + 表示範囲 envelope + pan/zoom」を土台にして、波形ビューアとしての日常操作をRust版に移す。

Phase 2で目指す状態:

- 1行複数チャンネル重ね表示ができる
- 複数行表示ができる
- 全行でX軸 pan/zoom が同期する
- line / step 表示を使い分けられる
- チャンネル別の色・線幅を設定できる
- 行ごとのY軸範囲を指定できる
- hover位置とチャンネル値を確認できる
- 読み込み中にUIが固まらない

Cursor、統計、FFT、作図設定保存/ロードはPhase 3以降を基本とする。ただし、Phase 2の状態モデルはそれらを後から載せられる形にする。

---

## 2. 基本方針

Phase 1の技術方針を継続する。

| 項目 | Phase 2方針 |
|------|-------------|
| GUI | `eframe` + `egui` を継続 |
| Parquet | `parquet` crate のprojection読み込みを継続 |
| raw data | loaded channelごとに `Arc<[f32]>` 相当で共有 |
| time | dataset単位で共有し、チャンネルごとに重複保持しない |
| 描画用データ | range-aware min/max envelope を基本にする |
| tile pyramid | 初期実装では入れない。必要性が出た時点で追加 |
| 解析 | Phase 3以降もraw data参照を前提にする |
| UI処理 | 重い読み込みと抽出はUIスレッドから分離する |

---

## 3. 状態モデル

Phase 2では、UI状態とデータ状態を分ける。

想定する主要構造:

```text
AppState
  DatasetState
    path
    schema
    time_column
    channel_metadata[]
    shared_time
    loaded_channels: ChannelStore

  ViewState
    x_range
    rows: PlotRow[]
    hover_x
    theme
    interaction_mode

  LoadState
    pending_jobs
    progress
    error
```

```text
ChannelStore
  raw_by_channel: channel_name -> ChannelData
  envelope_cache: EnvelopeKey -> MinMaxEnvelope
```

```text
PlotRow
  id
  channels: RowChannel[]
  y_range
  height
```

```text
RowChannel
  channel_name
  draw_mode: line | step
  color
  line_width
  visible
```

重要な分離:

- `DatasetState` はファイルとraw dataを持つ
- `ViewState` は現在の表示状態だけを持つ
- `PlotRow` は表示構成だけを持ち、raw dataを所有しない
- 同じチャンネルを複数行に出しても、raw dataは1つだけ保持する

---

## 4. データ読み込み方針

### schema読込

Phase 1と同じく、最初はParquet metadataだけを読む。

schema読込で行うこと:

- row count取得
- row group count取得
- `time` 列検出
- 数値チャンネル検出
- physical/logical type記録

### チャンネル読み込み

Phase 2では、表示に使うチャンネルだけをon demandで読む。

基本ルール:

- rowにチャンネルを追加した時点で未ロードなら読み込む
- 既にロード済みのチャンネルは再利用する
- `time` はdataset単位で共有する
- 全チャンネル一括読み込みは初期実装では避ける

### 非同期読み込み

Phase 1では同期読み込みでも成立したが、Phase 2では複数ch読み込み時の体感を守るため非同期化する。

UI方針:

- 読み込み中は対象チャンネルをloading表示にする
- schema変更、ファイル変更、該当チャンネル削除など危険な操作は一時的に無効化する
- 読み込み完了後にenvelope cacheを無効化して再描画する
- エラーは画面上で確認できる形に残す

---

## 5. 描画方針

### X軸

`x_range` は `ViewState` に1つだけ持ち、全行で共有する。

操作:

- drag: pan
- wheel/pinch: cursor位置基準 zoom
- horizontal scroll: pan
- double click / reset: 全体表示

### Y軸

Y軸は行ごとに持つ。

初期実装:

- auto range
- manual min/max

後続で検討:

- row内の表示チャンネルだけでauto range
- outlierを含む場合のrobust auto range
- Y range reset

### line表示

line表示は、Phase 1のrange-aware min/max envelopeを使う。

抽出ルール:

- 表示範囲内のsample indexを `partition_point` 相当で求める
- 表示幅に応じてbucket数を決める
- 各bucketのmin/maxを描画する
- visible sample数が十分少ない場合はraw line描画に切り替えてもよい

### step表示

step表示はmin/maxだけではエッジ位置が曖昧になる可能性がある。

初期実装の方針:

- gate/pwm系チャンネルはstepとして描画できるようにする
- 表示範囲内の値変化点を優先して残す
- 高密度で変化点が多すぎる場合は、bucket内の先頭値、末尾値、min/max、変化点代表を保持する

step表示はPhase 2のリスク項目として扱い、line表示を先に安定させる。

---

## 6. envelope / LOD方針

Phase 2初期は、full tile pyramidを作らない。

採用する基本方式:

- raw dataはloaded channel単位で保持する
- 表示時に `x_range` とpixel幅から必要なenvelopeを作る
- 同一channel、同一range、同一bucket数なら行間でenvelopeを共有する
- pan/zoomでrangeが変わった場合は、必要に応じて再抽出する

cache方針:

- まずは「現在表示中のenvelope」を保持する程度に留める
- pan中の過去rangeを大量に蓄積しない
- channel切替、width変更、data reloadでcacheを破棄する

tile pyramidを追加する条件:

- 複数ch/複数行でpan/zoomが重くなる
- 90M点級でrange-aware envelope再抽出が体感上遅い
- step表示のエッジ抽出が重い
- 同じrangeを何度も再抽出するケースが増える

---

## 7. UI構成

Phase 2の基本画面:

```text
left panel
  file path
  schema / load controls
  channel list
  row controls
  style controls

central panel
  waveform rows
  shared x axis
  hover line
  value readout

status area
  load progress
  sample count
  memory / timing summary
  errors
```

UI実装の優先:

1. ファイルとschemaの表示
2. row追加・削除
3. rowごとのチャンネル選択
4. style編集
5. hover値表示
6. theme切替

---

## 8. 実装順

推奨順:

| ID | 作業 | 完了条件 |
|----|------|----------|
| P2-01 | 状態モデルを整理する | data/view/load stateが分離されている |
| P2-02 | 複数channelをロードできるようにする | loaded channelをcacheして再利用できる |
| P2-03 | 1行複数channel表示 | 同じrowに複数波形を重ねられる |
| P2-04 | 複数行表示 | row追加・削除ができる |
| P2-05 | X軸同期 | pan/zoom/resetが全行に効く |
| P2-06 | line / step表示 | line安定後、stepのエッジ保持を確認する |
| P2-07 | style設定 | color、line width、visibleを変更できる |
| P2-08 | Y軸範囲指定 | rowごとにauto/manualを使える |
| P2-09 | hover同期 | 全行で同じX位置の値を表示できる |
| P2-10 | 非同期読み込み | 複数ch読み込み中もUIが固まらない |
| P2-11 | theme切替 | light/darkを切り替えられる |
| P2-12 | 性能再測定 | 中・大・90M点級で測定結果を残す |

進捗メモ:

- 2026-04-30: P2-01完了。`PanelYApp` を `DatasetState` / `ViewState` / `LoadState` に分離した
- 2026-04-30: P2-02完了。dataset単位の共有timeとchannel単位のraw cache、表示range単位のenvelope cacheを追加した
- 2026-04-30: P2-03一部完了。1行目へ複数channelを追加し、重ね表示できるようにした。行追加・削除とstyle編集は未実装
- 2026-04-30: P2-04実装。row追加・削除、選択rowへのchannel追加、rowごとの描画rect分割に対応した
- 2026-04-30: P2-10実装・GUI確認。channel/time読み込みをworker thread化し、完了時だけUI stateへ反映するようにした
- 2026-04-30: P2-05の実装土台を確認。`ViewState::x_range` は1つだけ保持し、pan/zoom/resetは全row共通rangeへ反映する。複数行GUIの手動操作確認は次回継続
- 2026-04-30: P2-04/P2-05手動確認完了。複数行にわたるDelete/Remove、pan/zoom/resetの全row同期は軽快で問題なし
- 2026-04-30: 行数増加時の見切れ対策として、左ペインを縦スクロール化し、波形ペインもrow最小高を維持して縦スクロールできるようにした
- 2026-04-30: UI改善候補として、波形エリアのX zoomを `Ctrl + ホイール`、波形側の縦移動をホイールのみへ分離する案を記録した。性能検証段階では現仕様のまま進める
- 2026-04-30: P2-06a実装。`RowChannel` に `Line` / `Step` の表示モードを追加し、row内channelごとに切替UIを追加した。Stepは低密度表示範囲ではraw sampleから正確な階段描画を行い、高密度では既存min/max envelopeへfallbackする
- 2026-04-30: P2-06a GUI確認。`pwm_1kHz` / `noise` で拡大状態のLine/Step切替はOK。エッジ位置検証用に `step_validation_100k.parquet` 生成スクリプトを追加した
- 2026-04-30: `step_validation_100k.parquet` の `pwm_1kHz` / `pwm_1kHz_delay_1sample` / `pwm_1kHz_advance_1sample` でLine/Step表示を確認し、1サンプル進み/遅れの相対関係が正確であることを確認した
- 2026-04-30: P2-06b実装。高密度Stepでも値の変化点数が `MAX_STEP_CHANGE_POINTS` 以下なら change-point Step として正確なエッジ時刻を保持し、変化点が多すぎる場合だけmin/max envelopeへfallbackするようにした。`gate_pwm` / `pwm_10kHz` では中域ズームで変化点保持が必要なことを確認した
- 2026-04-30: P2-06b GUI確認。`gate_pwm` のStep表示で、change-point Step対象範囲とmin/max envelope fallback範囲のどちらも見え方に違和感がないことを確認した
- 2026-04-30: P2-07実装。row内channelごとにvisible、color override、line widthを編集できるようにし、hidden channelは描画・Y auto range・表示ステータス集計から外れるようにした
- 2026-04-30: P2-07 GUI確認。visible ON/OFF、line width即時反映、custom color / resetが期待通り動作することを確認した
- 2026-04-30: P2-09実装。hover X位置を `ViewState` で共有し、全rowへの縦ライン表示とvisible channelの値readoutを追加した。Lineはnearest sample、Stepはsample-and-hold値を表示する
- 2026-04-30: P2-09 GUI確認完了。`test_100k.parquet` / `panely_large_10s_1mhz_9ch.parquet` で複数row hover、Line/Step値表示、hidden channel除外、plot外hover clear、左右readout位置切替、更新レスポンスが期待通り動作することを確認した
- 2026-04-30: P2-08実装。`PlotRow` にAuto/ManualのY軸範囲設定を追加し、rowごとのmin/max入力とmanual range描画に対応した。Manual時はplot rectでtraceをclipする
- 2026-04-30: P2-08 GUI確認完了。Manual min/max反映、初期値/Reset時の `-1..1` 復帰、Manual設定中にchannelをhiddenにしてもplot min/maxがmanual設定を保持することを確認した
- 2026-04-30: P2-08改善。Manual切替時とManual Reset時のmin/maxを固定 `-1..1` ではなく直近のAuto表示Y範囲から初期化するように変更した。Auto範囲が未取得の場合だけ `-1..1` にfallbackする
- 2026-04-30: P2-08改善GUI確認完了。Manual切替時の初期値とReset時に直近Auto範囲へ戻る動きが問題ないことを確認した
- 2026-04-30: P2-12測定完了。`--bench-phase2` を追加し、中データ全8ch、大データ全9ch、大データ代表4chで複数row表示データ生成・hover・RSSを測定した。大データ全9chではvisible trace生成 avg 31.3 ms / max 92.5 ms、RSS after bench 435.4 MiB。結果は [rust_migration_phase2_benchmark_results.md](rust_migration_phase2_benchmark_results.md) に記録した

---

## 9. Phase 2完了条件

Phase 2は以下を満たしたら完了とする。

- Rust版だけで日常的な波形確認ができる
- 1行複数チャンネル重ね表示ができる
- 複数行表示ができる
- X軸 pan/zoom が全行で同期する
- line / step、色、線幅、Y軸範囲を最低限操作できる
- hover位置のチャンネル値が確認できる
- 複数ch表示でも描画処理が表示幅オーダーに収まる
- 中・大データで操作不能にならない
- 90M点級または実機ログ相当データの測定方針が確定している

---

## 10. リスク

| リスク | 対応 |
|--------|------|
| 複数chロードでメモリが増えすぎる | on demand load、unload、channel cache上限を入れる |
| step表示でエッジが見えなくなる | min/maxとは別に変化点保持を入れる |
| `egui::Painter` が複数行で重い | 描画点数とshape生成時間を測り、必要なら `wgpu` custom rendererへ移る |
| 非同期読み込みで状態が複雑化する | data/view/load stateを分け、job完了時だけUI stateへ反映する |
| 作図設定の互換が後から難しくなる | Phase 2の状態モデルをそのまま設定保存へ写せる構造にする |

---

## 11. 参照

- [Rust移植 Phase 1 評価メモ](rust_migration_phase1_evaluation.md)
- [Rust移植 Phase 1 ベンチマーク結果](rust_migration_phase1_benchmark_results.md)
- [Rust移植 Phase 2 性能測定結果](rust_migration_phase2_benchmark_results.md)
- [Rust移植 Phase 0 UI仕様](rust_migration_phase0_ui_spec.md)
- [Rust移植 Phase 0 作図設定仕様](rust_migration_phase0_plotconfig_spec.md)
- [Rust移植計画書](rust_migration_plan.md)
