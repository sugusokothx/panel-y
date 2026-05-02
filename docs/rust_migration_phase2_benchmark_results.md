---
project: Panel_y
doc_type: Phase 2性能測定結果
target: Rust移植 Phase 2
created: "2026-04-30"
updated: "2026-05-02"
status: P2-12測定完了
implementation: "code/rust_phase1"
---

# Rust移植 Phase 2 性能測定結果

---

## 1. 目的

Phase 2で追加した複数行・複数チャンネル・line / step・hover同期・非同期読み込み後の状態を前提に、表示データ生成とメモリ使用量を定量確認する。

この測定はGUI描画そのものではなく、GUIが各frameで使う `visible_row_traces()` 相当の表示データ生成をCLIから実行する。

測定に含むもの:

- time列とchannel列の個別読み込み
- 共有time + channel raw cacheのメモリ
- 複数row / 複数channelの表示データ生成
- line channelのrange-aware min/max envelope生成
- step channelのraw step / change-point step / envelope fallback判定
- hover位置の値探索

測定に含まないもの:

- `egui::Painter` による実際のshape描画
- OSイベント処理
- GUIでの非同期worker起動オーバーヘッド

---

## 2. 測定方法

追加したCLI:

```bash
cd code/rust_phase1
cargo run --release -- --bench-phase2 ../proto_3_1b/data/panely_medium_2m_8ch.parquet
cargo run --release -- --bench-phase2 ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet
cargo run --release -- --bench-phase2 ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet iu iv iw gate_pwm
```

`--bench-phase2` のレイアウト:

- 指定channelがなければ全channelを対象にする
- 3 channelごとに1 rowへ配置する
- channel名に `pwm` / `gate` / `step` を含むものはStep、それ以外はLineとして測る
- 表示範囲は24種類
- envelope要求bucket数は1200
- hover測定は1000位置

---

## 3. 対象データ

| dataset | rows | channels | 総channel点数 | メモ |
|---|---:|---:|---:|---|
| `panely_medium_2m_8ch.parquet` | 2,000,000 | 8 | 16,000,000 | 100 kHz, 約20 s |
| `panely_large_10s_1mhz_9ch.parquet` | 10,000,000 | 9 | 90,000,000 | 1 MHz, 約10 s |
| `ripple_validation_2_4m.parquet` | 2,400,000 | 5 | 12,000,000 | 1 MHz, 約2.4 s, ripple envelope検証 |

---

## 4. 結果サマリ

| case | layout | read time列 | channel read合計 | raw cache | RSS after load | RSS after bench | visible avg | visible max | hover |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| medium 全8ch | 3 rows / 8 ch | 0.040 s | 0.085 s | 76.3 MiB | 91.0 MiB | 91.6 MiB | 0.0080 s | 0.0221 s | 0.264 us/lookup |
| large 全9ch | 3 rows / 9 ch | 0.215 s | 0.941 s | 419.6 MiB | 434.9 MiB | 435.4 MiB | 0.0313 s | 0.0925 s | 0.439 us/lookup |
| large 代表4ch | 2 rows / 4 ch | 0.153 s | 0.236 s | 228.9 MiB | 244.1 MiB | 244.4 MiB | 0.0144 s | 0.0382 s | 0.504 us/lookup |

RSSは表示データ生成・hover測定後もほぼ増えなかった。envelope cache追加分は大データ全9chでも約0.5 MiBだった。

---

## 5. 表示データ生成

| case | ranges | buckets | avg | max | max draw points | max raw step samples |
|---|---:|---:|---:|---:|---:|---:|
| medium 全8ch | 24 | 1200 | 0.0080 s | 0.0221 s | 30,396 | 8,010 |
| large 全9ch | 24 | 1200 | 0.0313 s | 0.0925 s | 27,058 | 4,001 |
| large 代表4ch | 24 | 1200 | 0.0144 s | 0.0382 s | 15,148 | 4,001 |

大データ全9chでは、5,000,000 samples相当の広い表示範囲で最大92.5 ms、2,000,000 samples相当で約32-33 msだった。GUI確認で見えた高速ズーム時の軽いカクつきは、このcoldな複数ch envelope生成時間で説明できる。

一方、500,000 samples相当では全9chでも約8.6-9.0 ms、100,000 samples相当では約1.5-1.9 msまで下がる。

---

## 6. Hover

| case | positions | lookups | hits | total | avg |
|---|---:|---:|---:|---:|---:|
| medium 全8ch | 1000 | 8000 | 8000 | 0.0021 s | 0.264 us/lookup |
| large 全9ch | 1000 | 9000 | 9000 | 0.0040 s | 0.439 us/lookup |
| large 代表4ch | 1000 | 4000 | 4000 | 0.0020 s | 0.504 us/lookup |

hover値探索は十分軽く、現時点の主要ボトルネックではない。

---

## 7. 判断

Phase 2の定量ベンチとして、10M rows / 9ch、総channel点数90M点の全channel raw cacheと複数row表示データ生成を確認した。

判断:

- Phase 2の基本ビューア機能は、Rust版だけで日常的な波形確認ができる水準に到達している
- 大データ全9chでもRSSは約435 MiBで、16 GB級の開発機では許容範囲
- hover同期は性能リスクではない
- 高速pan/zoom中に毎回5M samples x 多chのcold envelopeを作るケースでは、16 ms/frameを超えるため軽いカクつきが出うる

次の最適化候補:

- pan/zoom中の表示範囲に対するenvelope再利用または近傍range cache
- tile / LOD pyramid
- step channelの広範囲fallback判定の早期化
- `egui::Painter` shape生成時間のGUI側計測

対策計画は [rust_migration_phase2_performance_plan.md](rust_migration_phase2_performance_plan.md) に整理した。

Phase 2の完了ゲートであるP2-12は測定完了扱いにする。上記の最適化はPhase 2 optionalまたはPhase 3前の性能改善タスクとして扱う。

---

## 8. 追加測定: ripple envelope検証

2026-05-02に、Line tile / min-max envelopeで細かいリップルが消えず、帯として残るかを見るための専用データを追加した。

生成:

```bash
cd code
.venv/bin/python proto_3_1b/generate_ripple_validation_data.py --force
```

出力:

- `code/proto_3_1b/data/ripple_validation_2_4m.parquet`
- 2,400,000 rows / 5 channels
- 39,063,837 bytes
- sha256: `0125ad1bf06cee22a4281d0b86134f87b043a170f63181d17f77140b1289cc39`

channels:

- `ripple_low_amp_80kHz`
- `ripple_am_60kHz`
- `ripple_noise_70kHz`
- `ripple_bucket_near_2048samples`
- `ripple_on_sine_80kHz`

Rust schema / load確認:

- schema: 2,400,000 rows / 10 row groups / 5 numeric channels
- `ripple_low_amp_80kHz`
  - samples: 2,400,000
  - time range: `0.000000 .. 2.399999`
  - value range: `-0.024951 .. 0.024951`
  - full envelope: 4,096 buckets / bucket size 586 / 8,192 draw points

Phase 2 bench:

```bash
cd code/rust_phase1
cargo run --release -- --bench-phase2 ../proto_3_1b/data/ripple_validation_2_4m.parquet ripple_low_amp_80kHz ripple_am_60kHz ripple_noise_70kHz ripple_bucket_near_2048samples ripple_on_sine_80kHz
```

結果:

| case | layout | raw cache | RSS after bench | visible avg | visible max | tile hits/builds | hover |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripple 5ch | 2 rows / 5 line ch | 64.1 MiB | 79.5 MiB | 0.0040 s | 0.0250 s | 25 / 5 | 0.306 us/lookup |

判断:

- 5chすべてLineとして測定され、Step経路は混ざっていない
- 初回広域rangeで5ch分のLine tileがbuildされ、その後の広域rangeでtile hitが出る
- 低振幅80 kHzリップルはfull envelopeで約±0.025の帯として値域が保持される
- bench上の表示データ生成は十分軽く、リップル検証データは今後のGUI目視確認用として使える

---

## 9. 追加測定: Step edge cache / dense LOD

2026-05-02に、Step channel向けのedge cacheとdense fallbackを追加した。

実装内容:

- Step channelごとに `index/time/value` のchange-point listをlazy buildする
- 表示rangeではedge listをbinary searchして、左端直前の状態とrange内edgeだけを抽出する
- edge数が多すぎる場合はStep専用dense envelopeへfallbackする
- Step dense envelopeではLine tile LODを使わない
- frame timing / benchにStep edge cache hit/build、edge数、edge memoryを追加する

確認:

```bash
cd code/rust_phase1
cargo run --release -- --bench-phase2 ../proto_3_1b/data/step_validation_100k.parquet
cargo run --release -- --bench-phase2 ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet gate_pwm
cargo run --release -- --bench-phase2 ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet
```

結果:

| case | layout | visible avg | visible max | edge hits/builds | edge count | edge memory | dense / exact behavior |
|---|---:|---:|---:|---:|---:|---:|---|
| `step_validation_100k.parquet` | 1 row / 3 step ch | 0.0001 s | 0.0005 s | 33 / 3 | - | 0.1 MiB | 1sample進み/遅れ経路を維持 |
| `gate_pwm` only | 1 row / 1 step ch | 0.0064 s | 0.0309 s | 8 / 1 | 400,000 | 9.2 MiB | 広域はdense、100k samples級はedge-step |
| large 全9ch | 3 rows / 8 line + 1 step | 0.0194 s | 0.1857 s | 8 / 1 | 400,000 | 9.2 MiB | Line tile + Step dense/edgeの混在OK |

判断:

- `step_validation_100k.parquet` の1sample進み/遅れ確認は維持された
- `gate_pwm` は初回に400,000 edgesのcacheをbuildし、以後はedge listからrange抽出できる
- 広範囲ではdense envelopeへfallbackし、edgeが少ない100k samples級では正確なedge-stepへ戻る
- large全9chでは初回rangeにLine tile buildとStep edge cache buildが乗るためmaxは大きめだが、pan cacheはavg 0.0079 s / max 0.0135 sで維持される
