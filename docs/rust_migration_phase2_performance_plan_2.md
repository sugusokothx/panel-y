---
project: Panel_y
doc_type: Phase 2性能改善計画 その2
target: Rust移植 Phase 2
created: "2026-05-02"
updated: "2026-05-02"
status: 一部実装済み
source: "docs/rust_migration_phase2_performance_plan.md"
implementation: "code/rust_phase1"
---

# Rust移植 Phase 2 性能改善計画 その2

---

## 1. 目的

Phase 2性能改善の主戦略を、操作中previewから「イベントを隠さない描画最適化」へ切り替える。

想定する利用方法:

- 波形上のイベントを目印にして拡大対象を探す
- pan/zoomしながら、スパイク、エッジ、段差、異常値へ近づいていく
- 操作中と停止後で見え方が変わると、目印を失いやすい

この用途では、操作中だけ描画品質を落とすpreviewは相性が悪い。以後は、通常描画そのものを軽くし、pan/zoom中も停止後も同じ情報が見える方針を基本にする。

---

## 2. 基本判断

採る方針:

- 単純な等間隔間引きは採らない
- 描画点数削減は、情報を保存する方式だけ採る
- Lineはpixel-aware min/max envelopeを基本にする
- Step / digital系はedge / change-point保持を基本にする
- 操作中previewは主戦略から外し、残す場合もdebug / emergency toggle扱いにする

採らない方針:

- 2点に1点だけ描く
- N sampleごとに代表値だけ描く
- 操作中だけbucket数を大きく下げる
- Step信号を常時min/max envelopeへ倒す

理由:

- 等間隔間引きは短いspikeやedgeを消す可能性がある
- previewは操作中と停止後で目印の見え方が変わる
- 探索用途では、多少のカクつきより「イベントが消える/変形する」ほうが問題になりやすい

---

## 3. 表示品質の要件

### 3.1 Line

要件:

- 1 pixel相当の範囲に複数sampleがある場合、min/maxを残す
- spike、外れ値、急峻なピークを消さない
- envelopeのbucket数を減らす場合も、bucket内のmin/maxは保持する
- hover値は描画用envelopeではなくraw dataから取る

注意:

- min/max envelopeはspikeの存在を残せるが、bucket内の正確な時刻までは表現できない
- 詳細な時刻確認はzoom後またはhover / cursor機能でraw dataを見る

### 3.2 Step / Digital

要件:

- 低密度から中密度ではchange-pointを保持する
- 表示範囲の左端では、直前sampleの状態を持ち込む
- 高密度でedgeがpixel密度を超える場合は、edgeを等間隔に間引かない
- 高密度時はdense表示、edge density表示、またはStep専用LODを検討する

注意:

- Stepを単純なmin/max envelopeで描くとedge位置が曖昧になる
- PWMのような高密度信号は、広域表示では「個々のedge」ではなく「密度/占有帯域」を見せるほうが正確な場合がある

---

## 4. 現状のボトルネック整理

現行実装:

- `visible_row_traces()` がframe内で表示用traceを生成する
- Lineは表示rangeごとにmin/max envelopeを生成する
- Stepは `raw step` -> `change-point step` -> `envelope fallback`
- envelope cacheはrange完全一致に近く、pan/zoomで効きにくい

問題:

- wide range x multi channelでcold生成が重い
- pan時に少しrangeが動くだけでも再生成が走る
- bucket数を下げても、Line envelopeの可視sample走査コストは残る
- previewは見た目の一貫性を崩すため、探索用途と相性が悪い

---

## 5. 対策方針

### 5.1 previewの扱いを変更する

方針:

- previewを性能改善の主戦略から外す
- 既定OFF、またはdebug / emergency用toggleへ降格する
- medium級、小さい表示範囲、イベント探索中は通常品質を維持する

完了条件:

- medium級データのpan/zoomでpreviewが発動しない
- preview OFFでも既存の基本操作が成立する
- UI上でpreview状態がユーザーの意図せず混ざらない

### 5.2 overscan range cache

狙い:

- pan中にrangeが少し動くたびにenvelopeを再生成しない
- 表示範囲より広いrangeのenvelopeを持ち、表示rangeがその内側なら再利用する

設計:

- view rangeの左右にoverscanを付けたcache rangeを作る
- pan中にview rangeがcache range内ならcacheを再利用する
- cache bucketは時間幅またはsample幅を固定し、panでbucket境界が大きく動かないようにする
- 描画時はview range外のbucketをclipする

注意:

- Y auto rangeはoverscan範囲ではなく、実際のview range相当から計算する
- overscan由来の外れ値でY軸が引っ張られないようにする
- cache memory上限を持つ

完了条件:

- large全9chのpanでenvelope再生成回数が減る
- pan中と停止後で見た目が切り替わらない
- Y auto rangeが表示範囲外の値に引っ張られない

### 5.3 bucket境界共有 / 一括生成

狙い:

- 同じtime/index rangeをchannelごとに重複計算しない
- multi channel表示でLine envelope生成の固定費を下げる

設計:

- view rangeからsample index rangeを一度だけ求める
- bucket境界を一度だけ作る
- 各Line channelは同じbucket境界でmin/maxを取る
- Rowやchannelが違っても、同じx range / bucket条件なら境界を共有する

完了条件:

- medium / largeのmulti channel visible trace生成時間が下がる
- 表示点数とY rangeの意味が現行と一致する

### 5.4 Line用lazy tile / LOD

狙い:

- 10M級の広域Line表示で、毎frame raw samplesを広く走査しない
- min/maxを保持したまま描画点数と走査量を下げる

設計:

- channelごとに固定sample幅のmin/max tileをlazy buildする
- 最初はcoarse levelを1段または数段だけ作る
- 表示rangeとpixel幅から、raw scanかtile scanかを選ぶ
- tileは描画専用cacheであり、解析用raw dataとは分離する

メモリ見積もり:

- 10M samplesを256 samples/tileにすると約39,063 tiles
- min/maxをf32 x 2で持つと約0.3 MiB/channel/level
- 9ch x 複数levelでもraw cacheに対して小さい見込み

注意:

- 不規則timeの場合はtile indexからtime範囲を引けるようにする
- tile構築はUI threadを止めない
- channel reload時にtile cacheを破棄する

完了条件:

- large全9chの広域Line表示でvisible生成時間が安定して下がる
- spike / outlierが消えない
- RSS増加が許容範囲に収まる

### 5.5 Step用edge cache / dense LOD

狙い:

- Stepの広範囲表示で毎回change-point走査をしない
- edgeを単純間引きせず、探索に必要な状態変化を維持する

設計:

- Step channelごとにchange-point index/time/value listをlazy buildする
- 表示rangeではedge listをbinary searchして抽出する
- edge数がpixel密度を大きく超える場合はdense表示へ切り替える
- dense表示は「edgeを消して滑らかに見せる」のではなく、「この範囲は高密度に変化している」ことを示す

候補:

- high/low occupancy band
- per-pixel edge density
- min/max ribbon + dense marker

完了条件:

- `step_validation_100k.parquet` の1sample進み/遅れ確認を維持する
- gate_pwmの広範囲表示で無駄なchange-point走査が減る
- dense表示でもedgeが少ない範囲では正確なStep表示へ戻る

実装結果:

- `ChannelStore` にStep channelごとのedge cacheを追加した
- cacheは初回だけ `index/time/value` のchange-point listをlazy buildする
- 表示rangeではedge listをbinary searchして、左端直前の状態とrange内edgeだけを抽出する
- edge数が `MAX_STEP_CHANGE_POINTS` を超える場合はStep専用dense envelopeへfallbackする
- dense envelopeはLine tile LODを使わず、Step fallbackとしてmin/max ribbonを描く
- frame timing / benchにStep edge cache hit/buildとedge memoryを表示するようにした

### 5.6 channel並列化

位置づけ:

- cache / tile / boundary共有後の追加策
- cold生成が残る場合に検証する

注意:

- メモリ帯域が上限になりやすい
- frameごとにthread生成しない
- cache更新と借用設計が複雑になる

完了条件:

- large全9chのcold生成maxが明確に下がる
- small / mediumでoverheadが増えない

---

## 6. 推奨実施順

```text
previewを既定OFF / debug扱いへ変更
  -> frame timingにcache hit/missを追加
  -> overscan range cache
  -> bucket境界共有 / 一括生成
  -> Line lazy tile / LOD
  -> Step edge cache / dense LOD
  -> 必要ならchannel並列化
```

最初にやるべきこと:

1. previewを主経路から外す
2. overscan range cacheでpan中の再生成を減らす
3. bucket境界共有でmulti channelの重複計算を減らす

tile / LODは効果が大きいが設計面の影響も大きいため、overscan cacheとbucket境界共有の効果を測ってから入る。

---

## 7. 合格条件

品質:

- pan/zoom中と停止後で波形の目印が変わらない
- spike / edge / step / outlierが単純間引きで消えない
- hover値はraw data基準を維持する

性能:

- medium全8chはpreviewなしで滑らかに操作できる
- large全9chの広域pan/zoomでframe詰まりが明確に減る
- large全9chのvisible maxがP2-12の92.5 msから明確に低下する

メモリ:

- tile / cache増加分がraw cacheに対して許容範囲
- cache上限と破棄条件がある

回帰:

- `step_validation_100k.parquet` のStep確認を維持する
- `spike_event` またはspike系channelでspikeが消えない
- Y auto rangeがoverscan範囲に引っ張られない

---

## 8. 検証計画

CLI:

```text
cargo check
cargo test
cargo run --release -- --bench-phase2 ../proto_3_1b/data/panely_medium_2m_8ch.parquet
cargo run --release -- --bench-phase2 ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet
cargo run --release -- --bench-phase2 ../proto_3_1b/data/panely_large_10s_1mhz_9ch.parquet iu iv iw gate_pwm
```

GUI:

- medium全8chでpan/zoom時にpreviewなしで滑らかなこと
- large全9chで広域pan/zoom時のframe timingを確認すること
- spike / edgeを目印にzoomしても、操作中と停止後で目印が消えないこと

追加したい計測:

- envelope cache hit/miss
- overscan hit/miss
- tile build time
- tile hit/miss
- visible trace生成内訳: index search / bucket boundary / channel minmax / step edge抽出

---

## 9. リスク

overscan range cache:

- cache range由来の値がY auto rangeに混ざると見た目が崩れる
- bucket境界の扱いが雑だとpanで波形が揺れて見える

Line tile / LOD:

- min/maxはspikeの存在を残せるが、bucket内の正確な時刻は表現できない
- tile構築をUI threadで行うと読み込み時に固まる

Step dense LOD:

- dense表示の見た目を誤ると、digital信号がanalog信号のように見える
- edge密度表示と正確なedge表示の切替条件が必要

並列化:

- メモリ帯域律速で効果が限定される可能性がある
- cache更新の複雑化で不整合を生む可能性がある

---

## 10. 結論

今後の主戦略はpreviewではなく、以下に切り替える。

```text
Line: min/maxを保持したままcache / tileで軽くする
Step: edgeを保持し、dense時だけ専用表示へ落とす
pan: overscan cacheで再生成を避ける
multi channel: bucket境界共有で重複計算を減らす
```

単純な描画点間引きは採らない。描画点数を減らす場合は、min/max、edge、densityなど、探索に必要な情報を保存する形だけを採用する。
