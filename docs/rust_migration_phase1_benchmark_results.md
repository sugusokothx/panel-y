# Rust移植 Phase 1 ベンチマーク結果

## 対象

`code/rust_phase1` の縦切りプロトタイプで、1チャンネル表示時の Parquet 読み込み、全体 envelope、表示範囲 envelope 再抽出を測定した。

GUI操作は以下を手動確認済み。

- ドラッグ: pan
- ホイール/ピンチ: cursor 位置基準 zoom
- 横スクロール: pan
- ダブルクリックまたは Reset X Range: 全体表示

## 測定方法

Rust release build の CLI ベンチを使用した。

```bash
cd code/rust_phase1
cargo run --release -- --bench-channel ../proto_3_1b/data/panely_medium_2m_8ch.parquet sine_50Hz
cargo run --release -- --bench-channel ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet iu
cargo run --release -- --stress-channel ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet iu 1000
```

`--bench-channel` は次を測る。

- Parquet から `time` と選択チャンネルだけを読み込む時間
- 全体範囲の min/max envelope 抽出時間
- pan/zoom 相当の 24 種類の表示範囲で min/max envelope を再抽出する時間

表示範囲 envelope は GUI の表示幅相当として 1200 buckets を要求した。

`--stress-channel` は同じ 1200 buckets の表示範囲 envelope 再抽出を指定回数繰り返し、RSS をブロックごとに記録する。

## 結果

| dataset | channel | rows | array memory | read | full envelope | visible envelope 24 runs | avg | max |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `panely_medium_2m_8ch.parquet` | `sine_50Hz` | 2,000,000 | 22.9 MiB | 0.039 s | 0.003 s | 0.012 s | 0.0005 s | 0.0013 s |
| `panely_large_10s_1mhz_9ch.parquet` | `iu` | 10,000,000 | 114.4 MiB | 0.278 s | 0.013 s | 0.059 s | 0.0025 s | 0.0065 s |

## stress 結果

`panely_large_10s_1mhz_9ch.parquet` / `iu` で表示範囲 envelope を 1000 回再抽出した。

| runs | wall | measured total | avg | max | RSS before load | RSS after load | RSS after full envelope | RSS after stress |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1000 | 3.179 s | 3.160 s | 0.0032 s | 0.0090 s | 9.0 MiB | 130.0 MiB | 130.0 MiB | 130.0 MiB |

RSS block samples:

| completed runs | wall | RSS |
|---:|---:|---:|
| 200 | 0.635 s | 130.0 MiB |
| 400 | 1.271 s | 130.0 MiB |
| 600 | 1.903 s | 130.0 MiB |
| 800 | 2.540 s | 130.0 MiB |
| 1000 | 3.175 s | 130.0 MiB |

連続再抽出中に RSS は増加しなかった。

## GUI 主観評価

`panely_large_10s_1mhz_9ch.parquet` を GUI で読み込み、`Load Selected Channel` から波形が表示されるまでの体感は約 0.5 s だった。これまでの小さいデータより初回表示までの時間は明確に長い。

一方で、波形表示後の操作感は良好だった。

- pan は問題なく追従した
- 拡大操作は問題なく動いた
- X range reset は問題なく動いた
- 波形観察の操作としては実用上ストレスがない

初回表示 0.5 s 程度は、CLI 測定の読み込み時間 0.278 s に GUI 側の初回 envelope 抽出・描画更新が加わったものとして、おおむね妥当な範囲と判断する。

## large 詳細

`panely_large_10s_1mhz_9ch.parquet` / `iu`

- samples: 10,000,000
- time range: `0.000000 .. 9.999999`
- projected columns: `[0, 1]`
- full envelope: 4096 buckets requested, 4096 buckets built, bucket size 2442, draw points 8192
- visible envelope: 1200 buckets requested
- visible envelope draw points: 約 2400 点

表示範囲ごとの再抽出は、5,000,000 samples の広い範囲でも最大 0.0065 s だった。100,000 samples 程度の狭い範囲では 0.0001-0.0002 s 程度。

## 評価

P1-08 と P1-09 は完了。

- X軸 pan/zoom は GUI 手動確認済み
- 表示範囲に応じた min/max envelope 再抽出は CLI 測定済み
- 描画点数は表示範囲の元 sample 数ではなく、要求 bucket 数に比例している
- 10M 行データでは、読み込み後の pan/zoom 相当処理は十分軽い
- 1000 回の表示範囲再抽出で RSS は増え続けなかった
- GUI 上の large 実操作でも、初回表示後の波形観察操作感は良好だった

Phase 1 の主要なリスク確認は完了。次は Phase 1 評価として、継続判断と Phase 2 へ渡す実装方針を整理する。
