---
project: Panel_y
doc_type: Phase 2.5 UI polish計画
target: Rust移植 Phase 2.5
created: "2026-05-02"
updated: "2026-05-02"
status: 低リスクロードUI優先
implementation: "code/rust_phase1"
---

# Rust移植 Phase 2.5 実地検証UI polish計画

---

## 1. 目的

Phase 3の解析機能へ入る前に、実地検証を効率よく回すためのUIを整える。

Phase 2で表示性能と基本ビューア機能は一区切りになった。次に必要なのは、Cursor/統計/FFTを増やすことではなく、実データを開き、必要なchannelを選び、表示状態を作り、pan/zoomで確認する一連の操作を速く・迷わず・失敗から戻りやすくすることである。

Phase 2.5では、実地経験を積むための操作利便性を優先する。最初の実装対象は、低リスクに分類できるデータロードUI改善に絞る。

---

## 2. 基本判断

優先するもの:

- データロード関係のUI充実
- 実地検証時の状態把握
- 失敗時の復帰しやすさ
- 軽量なsession preset

Phase 2.5ではやらないもの:

- Cursor A/B
- 区間統計
- FFT
- `.pyc.json` 完全互換の保存/ロード
- Line tile / Step edge cacheのbackground build
- 正式な作図設定仕様

後回しにするもの:

- wheel操作分離
- box zoom

理由:

- 解析機能を積む前に、実ログを何度も開いて検証できる導線が必要
- 操作が重いと、性能・表示品質・UIの実地フィードバックを集めにくい
- parquet path直打ち、channel選択とload操作の距離、ロード状態の分かりにくさは実地検証の回転数に直接効く
- wheelとbox zoomは有用だが、データロードUIの整理後に扱う

---

## 3. 優先タスク

### P2.5-01 Parquet選択UI

狙い:

- parquet path直打ちを避け、実ログを開く入口を軽くする
- 直近に使ったファイルへ戻りやすくする
- path不備をschema load前に見つけやすくする

候補:

- File picker / Browse button
- 直近pathのsession内履歴
- Parquet pathの存在確認と拡張子チェック
- load errorの固定表示とclear

完了条件:

- file pickerまたはrecent pathからParquetを選べる
- 存在しないpath、拡張子違い、schema load失敗が分かりやすく表示される
- path選択からschema loadまでの導線が直感的である

### P2.5-02 Schema / channel一覧UI

狙い:

- schema load後に、データの概要とchannel候補を素早く把握できる
- channel選択とload操作の距離を縮める
- channel数が多い実ログでも目的channelを探しやすくする

候補:

- rows / time range / 推定sample rate / numeric channel数の要約表示
- channel検索/filter
- channelごとの `loaded` / `loading` / `unloaded` 状態表示
- channel一覧から直接 `Load` / `Add to row`
- target row選択をchannel一覧の近くに置く
- 複数channel選択と `Load checked`

完了条件:

- channel一覧内で選択、状態確認、load/addが完結する
- loaded済みchannelを再利用して別rowへ追加しやすい
- loading中・loaded済み・未ロードの違いがUI上で分かる

### P2.5-03 低リスク作図補助: session preset

狙い:

- 実地検証中によく使う表示状態へ戻れるようにする
- 正式な作図設定互換へ踏み込まず、便利さだけ先に得る
- `.pyc.json` 互換と混同しない

方針:

- 名前は `View preset` または `Session preset` とする
- 保存対象はRust版内部の軽量状態に限定する
- `.pyc.json` 完全互換は名乗らない
- schema不一致時は復元可能なchannelだけ扱い、不足分は警告する

候補保存対象:

- parquet path
- row構成
- channel path
- Line / Step
- visible
- color override
- line width
- Y Auto / Manual range
- X range

完了条件:

- 現在の表示状態をsession presetとして保存できる
- 同じschemaで主要な表示状態を復元できる
- `.pyc.json` 互換や正式作図設定とは別物としてUI上でも分かる

### P2.5-04 ロード済みcache操作

狙い:

- 実地検証でchannelを入れ替えながらメモリ状態を把握できるようにする

候補:

- loaded channel listを見やすくする
- channel単位のunload
- 全channel unload
- cache memoryの表示を残す
- loaded済みchannelを別rowへ追加しやすくする

完了条件:

- 試行錯誤中に不要channelを消して状態を整理できる
- cache再利用とunloadの違いが分かる

---

## 4. 推奨実施順

```text
P2.5-01 Parquet選択UI
  -> P2.5-02 Schema / channel一覧UI
  -> P2.5-03 低リスク作図補助: session preset
  -> P2.5-04 ロード済みcache操作
  -> P2.5-later wheel操作分離
  -> P2.5-later box zoom
```

最優先はデータロードUI。wheel操作分離とbox zoomは、実地検証で波形をたどるために有用だが、今回の最初の実装対象からは外し、ロードUI整理後に扱う。

---

## 5. Phase 3への移行条件

- 実ログを開く、schemaを確認する、複数channelをloadする操作が実用上のストレスにならない
- channel一覧からload/addでき、loaded / loading / unloaded状態が分かる
- 低リスクなsession presetで、実地検証中の表示状態へ戻れる
- 代表データでロード、pan、zoom、row追加、channel追加を一通りGUI確認済み

これを満たしたら、Phase 3 Cursor A/Bへ進む。
