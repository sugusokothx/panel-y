---
project: Panel_y
doc_type: Phase 2性能改善検討メモ
target: Rust移植 Phase 2
created: "2026-04-30"
updated: "2026-04-30"
status: 計画
source: "docs/rust_migration_phase2_benchmark_results.md"
implementation: "code/rust_phase1"
---

# Rust移植 Phase 2 性能改善検討メモ

---

## 1. 目的

P2-12定量ベンチで見えた「大データ複数channel表示時の高速pan/zoomの軽いカクつき」について、対策候補の見通し、リスク、代替案、実施順を整理する。

対象は主に以下:

- range cache
- tile / LOD
- Step広範囲fallback判定の早期化
- その他の代替案

---

## 2. 現状認識

P2-12の結果:

| case | layout | visible avg | visible max | RSS after bench | hover |
|---|---:|---:|---:|---:|---:|
| medium 全8ch | 3 rows / 8 ch | 8.0 ms | 22.1 ms | 91.6 MiB | 0.264 us/lookup |
| large 全9ch | 3 rows / 9 ch | 31.3 ms | 92.5 ms | 435.4 MiB | 0.439 us/lookup |
| large 代表4ch | 2 rows / 4 ch | 14.4 ms | 38.2 ms | 244.4 MiB | 0.504 us/lookup |

判断:

- hover値探索は十分軽く、現時点の主要ボトルネックではない
- RSSは10M rows x 9chの全channel raw cacheでも約435 MiBで、当面の主リスクではない
- 問題は広い表示範囲で複数channelの表示データをcold生成する経路
- 大データ全9chでは、5M samples相当の広い範囲で最大92.5 msになり、16 ms/frameを超える
- GUI確認で見えた高速zoom時の軽いカクつきは、このvisible trace生成時間で説明できる

現行実装の特徴:

- `visible_row_traces()` がframe内で表示データを生成する
- Lineは表示範囲ごとにrange-aware min/max envelopeを作る
- Stepは `raw step` -> `change-point step` -> `envelope fallback` の順で試す
- envelope cacheはrange完全一致が前提
- pan/zoomでx rangeが変わるとcacheをclearするため、連続pan/zoomでは現行cacheが効きにくい

---

## 3. 対策候補の評価

| 対策 | 期待効果 | リスク | 優先度 | メモ |
|---|---|---|---:|---|
| GUI frame timing計測 | 実ボトルネック特定 | 低 | 1 | `visible_row_traces()` 以外に `egui::Painter` shape生成・描画が詰まっていないか確認する |
| 操作中preview / debounce | UI体感改善 | 低〜中 | 2 | drag/zoom中は低bucketまたは前回表示を使い、停止後に精密再生成する |
| Step広範囲fallback早期化 | Step channelの無駄走査削減 | 低 | 3 | 効果は限定的。全9ch広範囲の主因はLine envelope側の可能性が高い |
| channelごとのenvelope並列化 | 多channel広範囲で効果 | 中 | 4 | メモリ帯域が上限。実装はLODより軽い |
| range cache改善 | pan/zoom戻り・近傍rangeで効果 | 中 | 5 | exact LRUだけでは連続操作に弱い。overscan cacheが本命 |
| tile / LOD | 広範囲Line表示で大きな効果 | 高 | 6 | メモリ、構築時間、Stepエッジ保持、cache invalidationが設計リスク |
| bucket境界共有・一括生成 | 多channelで中程度の効果 | 中 | 追加候補 | 同一time/index範囲をchannelごとに重複計算しない |

---

## 4. 各案の見通しとリスク

### 4.1 GUI frame timing計測

目的:

- `visible_row_traces()` 生成時間
- draw shape生成時間
- frame全体時間
- 操作中と停止後の差

をGUI側で測る。

見通し:

- 低コストで、以後の対策選定の前提になる
- CLIベンチでは見えないPainter側コストを確認できる

リスク:

- ほぼなし
- 計測表示自体がUIを邪魔しないよう、status表示またはdebug toggle扱いにする

完了条件:

- large全9chで、pan/zoom時のframe内訳が確認できる
- `visible_row_traces()` とPainter側のどちらが支配的か判断できる

### 4.2 操作中preview / debounce

案:

- drag/zoom中はbucket数を下げる
- または前回の表示データを保持してUIを止めず、操作停止後に通常bucketで再生成する
- wheel/dragイベントが連続している間は重い再生成を間引く

見通し:

- ユーザー体感には最も早く効く可能性が高い
- LODなしでも「操作中に固まらない」状態を作れる

リスク:

- 操作中だけ波形が粗く見える
- 停止後の精密表示への切替タイミングを誤るとちらつく
- Cursor/解析系をPhase 3で載せる場合、表示中previewと解析用raw dataを混同しない設計が必要

完了条件:

- large全9chで高速pan/zoom中もUI操作が詰まらない
- 停止後に通常品質の表示へ戻る
- preview中であることを必要ならstatusに出せる

### 4.3 Step広範囲fallback判定の早期化

現状:

- Stepはraw samplesが多い場合、change-point抽出を試す
- change-pointが多すぎる場合にenvelope fallbackする
- 広範囲で高周波PWMを表示すると、fallback前に一定量を走査する

案:

- 表示範囲のsample数が大きく、かつ過去に同channelがfallbackしたspan以上なら早期にenvelopeへ回す
- channelごとに「step change密度」の簡易メタ情報を保持する
- 低密度・中域zoomでは現行のedge保持を維持する

見通し:

- Step channelの広範囲表示では効く
- 実装は比較的小さい

リスク:

- 早期fallbackしすぎると、見えるはずのエッジを失う
- channel特性の推定を誤ると、低密度Stepの良さが崩れる

完了条件:

- `gate_pwm` の広範囲表示で不要なchange-point走査が減る
- `step_validation_100k.parquet` の1sample進み/遅れ確認は維持する

### 4.4 channelごとのenvelope並列化

案:

- 複数Line channelのenvelope生成をchannel単位で並列化する
- まずは表示データ生成の中だけを対象にする
- 必要なら `rayon` 導入を検討するが、標準threadでも実験は可能

見通し:

- large全9chのようにchannel数が多いケースで効く可能性がある
- tile / LODより設計変更が小さい

リスク:

- メモリ帯域が上限になり、CPU core数ほどは伸びない
- frameごとにthreadを作ると逆に重い
- shared cache更新と借用設計が複雑になる

完了条件:

- large全9chの広範囲visible maxが明確に下がる
- frameごとのalloc/thread overheadが増えない

### 4.5 range cache改善

現状:

- envelope cacheはrange完全一致が前提
- pan/zoomでrangeが変わるとcache clearされる
- exact LRUだけでは連続pan/zoomの新規rangeに弱い

案:

- exact cacheではなく、表示範囲より少し広いoverscan rangeをcacheする
- pan中に表示rangeがoverscan range内なら再抽出しない
- zoom段階ごとにbucket密度を揃え、近いrangeを同じcacheに乗せる

見通し:

- panには効きやすい
- zoomには効きにくいが、段階化すれば一部効く
- 実装難度はLODより低い

リスク:

- bucket境界と表示範囲が一致しないため、見た目が少し粗くなる可能性
- cache key設計が雑だとメモリが増える
- Y auto rangeがoverscan範囲由来になると表示範囲外の値に引っ張られる可能性

完了条件:

- pan時のvisible trace再生成回数が減る
- Y auto rangeは表示範囲の意味を維持する
- cache memory上限を持つ

### 4.6 tile / LOD

案:

- full tile pyramidではなく、まずは粗い固定sample幅のmin/max tileをlazy buildする
- line表示を対象にする
- Stepは別扱いにし、edge保持が必要なzoom範囲ではraw/change-pointを優先する

見通し:

- 広範囲Line表示の根本対策になる
- 大データ全9chのmax 92.5 msには最も効く可能性が高い

リスク:

- 実装範囲が大きい
- tile構築時間とメモリをどう制限するかが必要
- channel追加、file reload、style変更、range変更時のcache invalidationが複雑
- Step信号にmin/max tileだけを使うとエッジ位置が曖昧になる

完了条件:

- large全9chの広範囲表示で16 ms/frame相当を狙える
- tile memoryがraw cacheに対して許容範囲
- Step表示のエッジ保持仕様を壊さない

---

## 5. 推奨実施順

### Phase A: 計測と低リスク対策

1. GUI frame timing計測を入れる
2. 操作中preview / debounceを入れる
3. Step広範囲fallback判定を早期化する

狙い:

- ユーザー体感のカクつきを先に減らす
- LODに進む前に本当の支配要因を確認する

合格目安:

- large全9chで高速pan/zoom中にUIが詰まらない
- 停止後に通常品質で再描画される
- hover更新は現在同等の軽さを維持する

### Phase B: 中規模最適化

4. channelごとのenvelope並列化を検証する
5. overscan range cacheを検証する

狙い:

- wide range x multi channelのcold生成時間を下げる
- pan中の無駄な再生成を減らす

合格目安:

- large全9chのvisible maxが明確に低下する
- RSS増加が制御される
- キャッシュ由来の表示不整合がない

### Phase C: LOD導入

6. coarse tile / LODをlazy buildで導入する
7. 必要ならtile階層を増やす

狙い:

- 90M点級の広範囲表示を安定して軽くする
- Phase 3以降の解析機能とは切り離し、描画専用最適化として扱う

合格目安:

- large全9chの広範囲表示が安定して軽い
- tile構築中でもUIが固まらない
- raw data参照の解析方針を崩さない

---

## 6. すぐには採らない案

### exact LRU cacheのみ

戻る/リセットには効くが、連続pan/zoomで毎回新しいrangeになるため効果が薄い。range cacheをやるならoverscanまたはrange段階化とセットにする。

### full tile pyramidを最初から作る

効果は大きいが、構築時間・メモリ・Step表示仕様のリスクが大きい。まずはlazyなcoarse tileで検証する。

### Stepを常にenvelope表示に倒す

速くなるが、Phase 2で確認したedge保持の価値を壊す。広範囲fallbackはよいが、低密度・中域zoomではraw/change-point Stepを維持する。

---

## 7. 現時点の結論

最初に着手すべきはtile / LODではなく、GUI frame timing計測と操作中preview / debounceである。

理由:

- CLIベンチだけではPainter側の実コストが未確認
- ユーザー体感の問題は「計算が遅い」だけでなく「UI threadを止める」ことにもある
- preview / debounceはLODより小さく、早く効果を確認できる
- tile / LODは根本対策候補だが、Phase A/Bで不足が明確になってから入る方が安全

推奨順:

```text
GUI frame timing
  -> 操作中preview / debounce
  -> Step広範囲fallback早期化
  -> channel並列化 / overscan range cache
  -> coarse tile / LOD
```
