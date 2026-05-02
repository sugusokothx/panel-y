---
project: Panel_y
doc_type: Phase 2.5 UI polish計画
target: Rust移植 Phase 2.5
created: "2026-05-02"
updated: "2026-05-02"
status: 計画中
implementation: "code/rust_phase1"
---

# Rust移植 Phase 2.5 実地検証UI polish計画

---

## 1. 目的

Phase 3の解析機能へ入る前に、実地検証を効率よく回すためのUIを整える。

Phase 2で表示性能と基本ビューア機能は一区切りになった。次に必要なのは、Cursor/統計/FFTを増やすことではなく、実データを開き、必要なchannelを選び、表示状態を作り、pan/zoomで確認する一連の操作を速く・迷わず・失敗から戻りやすくすることである。

Phase 2.5では、実地経験を積むための操作利便性を優先する。

---

## 2. 基本判断

優先するもの:

- データロード関係のUI充実
- wheel操作分離
- box zoom
- 実地検証時の状態把握
- 失敗時の復帰しやすさ

Phase 2.5ではやらないもの:

- Cursor A/B
- 区間統計
- FFT
- `.pyc.json` 完全互換の保存/ロード
- Line tile / Step edge cacheのbackground build

理由:

- 解析機能を積む前に、実ログを何度も開いて検証できる導線が必要
- 操作が重いと、性能・表示品質・UIの実地フィードバックを集めにくい
- wheelとbox zoomは、Cursor操作や範囲解析を載せる前の基本操作として固めておきたい

---

## 3. 優先タスク

### P2.5-01 データロードUIの整理

狙い:

- 実ログを開いてから表示開始するまでの手数を減らす
- どのファイル、time列、channel、cache状態を見ているか分かりやすくする
- ロード失敗時に何を直せばよいか分かるようにする

候補:

- Parquet pathの存在確認と拡張子チェック
- schema load後に、rows / time range / 推定sample rate / numeric channel数を見やすく表示
- channel検索/filter
- channelのloaded / loading / unloaded状態表示
- 複数channelのまとめてload
- load target rowの明確化
- load errorの固定表示とclear
- 直近pathのsession内履歴

完了条件:

- path入力、schema load、channel選択、load/addの流れが実地検証で迷わない
- channel数が多い実ログでも目的channelを探しやすい
- loading中・loaded済み・未ロードの違いがUI上で分かる

### P2.5-02 wheel操作分離

現状:

- 波形上のwheel / pinchがX zoomとして動く
- 行数が多い場合、波形側の縦移動とX zoomが競合しやすい

方針:

- 通常wheelは縦スクロールに使う
- `Ctrl + wheel` またはtrackpad pinchをX zoomに使う
- horizontal scrollはX panに使う
- drag pan、double click resetは維持する

完了条件:

- 行数が多いときに、意図せずX zoomせず縦スクロールできる
- X zoomは明示操作として実行できる
- 既存のpan/zoom/reset操作を壊さない

### P2.5-03 box zoom

狙い:

- 実ログ中のイベントを見つけたあと、対象範囲へ一気に寄れるようにする
- Phase 3のCursor/区間解析に入る前に、範囲指定の操作感を固める

方針:

- 最初はX範囲zoomを優先する
- primary drag panは維持する
- `Alt/Option + drag` またはsecondary dragでbox zoomを起動する
- drag中はrubber bandを描画する
- release時に選択範囲を全row共通のX rangeへ反映する
- Y range変更はPhase 2.5では必須にしない

完了条件:

- 見つけたイベントをドラッグ範囲で素早く拡大できる
- pan操作と誤爆しない
- 複数rowでも同じX rangeへ同期してzoomする

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
P2.5-01 データロードUIの整理
  -> P2.5-02 wheel操作分離
  -> P2.5-03 box zoom
  -> P2.5-04 ロード済みcache操作
```

最優先はデータロードUI。wheel操作分離とbox zoomは、実地検証で波形をたどるための操作改善として続けて入れる。

---

## 5. Phase 3への移行条件

- 実ログを開く、schemaを確認する、複数channelをloadする操作が実用上のストレスにならない
- wheel操作で縦スクロールとX zoomを使い分けられる
- box zoomでイベント周辺へ素早く寄れる
- 代表データでロード、pan、zoom、row追加、channel追加を一通りGUI確認済み

これを満たしたら、Phase 3 Cursor A/Bへ進む。
