---
project: Panel_y
doc_type: 問題点連絡書
target: Proto #3.1
created: "2026-04-02"
status: 調査済み・修正着手中（→ proto_3_1b）
---

# 問題点連絡書 — 大容量データ読み込み時のハングアップ

---

## 発生状況

### テスト環境
- データ: 電流・電圧波形 9ch、サンプリング周波数 1MHz、記録長 10秒
- 総データ点数: **9ch × 10,000,000点 = 90,000,000点**
- ブラウザ: Chrome

### 症状
- ファイルコンバート・読み込み: 成功
- 初期表示: 成功
- スケール拡大（1回）: 成功
- **拡大縮小を繰り返す → ハングアップ**
- Chromeタブのメモリ使用量: **3GB超**

### 参考（正常動作したケース）
- 100ch、サンプリング500μs（2kHz）、記録長5秒 → 軽量に動作
  - 総点数: 100ch × 10,000点 = 1,000,000点（90M点の1/90）

---

## 原因分析

### 原因1: `minmax_envelope` がPythonループ（主因）

```python
# app.py: minmax_envelope()
for i in range(n_buckets):    # n_buckets = 2500 のPythonループ
    seg = data[i0:i1]
    d_min[i] = seg.min()
    d_max[i] = seg.max()
```

ズームのたびに9行分 × 2500回のPythonループが実行される。
1回のズームイベントで**合計22,500回のPythonループ**が走り、数秒〜数十秒かかる。
ズーム操作を連続して行うと未処理コールバックが積み上がりハングアップに至る。

### 原因2: 全データへのbooleanマスクを毎回生成

```python
# app.py: make_row_fig()
mask = (time_arr >= x_range[0]) & (time_arr <= x_range[1])
```

90M要素のbooleanアレイをズームのたびに生成。
メモリアロケーションが繰り返され、GC負荷とメモリ膨張を引き起こす。

### 原因3: DataFrameをfloat64でフル展開

90M行 × 10col × 8byte ≈ **720MB**（rawデータのみ）
pandas内部のコピーやplotly JSON化で **3〜4倍** に膨張 → 3GB超に到達。

---

## 修正方針（proto_3_1b ブランチ）

| # | 対策 | 効果 | 対象箇所 |
|---|------|------|----------|
| 1 | `minmax_envelope` をnumpy vectorize化（reshape→max/min） | 速度 ~100x | `minmax_envelope()` |
| 2 | boolean mask → `np.searchsorted` によるインデックス範囲取得 | メモリ削減・高速化 | `make_row_fig()` |
| 3 | 読み込み時にfloat32へダウンキャスト | メモリ半減（720MB→360MB） | `load_file()` |
| 4 | 全体表示用の事前ダウンサンプリングキャッシュ | 初期表示・縮小操作の高速化 | `load_file()` / `make_row_fig()` |

---

## 教訓

- **1MHz × 10秒 × 9ch = 90M点** はPlotly/Dashの通常想定を超える規模
- Min-Maxエンベロープ自体の設計は正しいが、実装（Pythonループ）が大規模データに耐えられなかった
- numpyのvectorized操作（reshape + nanmax/nanmin）で同等の計算を桁違いに高速化できる
- データ型の選択（float64 vs float32）がメモリフットプリントに直結する

---

## 関連

- 実装ブランチ: `proto_3_1b`
- 前バージョン: `code/proto_3_1/app.py`
