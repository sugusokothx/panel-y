---
project: Panel_y
doc_type: 性能測定計画
target: Rust移植 Phase 0
created: "2026-04-27"
updated: "2026-04-27"
status: 完了
source_proto: "code/proto_3_1b"
---

# Rust移植 Phase 0 性能測定計画

---

## 1. 目的

Dash/Plotly版 `proto_3_1b` の性能ベースラインを定義し、Rust版Phase 1以降の比較基準にする。

本書では測定項目と手順を定義する。実測結果は別途
`docs/rust_migration_phase0_benchmark_results.md` に記録する。

---

## 2. 測定対象

| 対象 | パス |
|------|------|
| Dash/Plotly版 | `code/proto_3_1b/app.py` |
| Python環境 | `code/.venv` |
| 小データ | `code/proto_3_1b/data/test_100k.parquet` |
| 中データ | 未定 |
| 大データ | 未定 |

---

## 3. 環境記録

測定時に必ず記録する。

- 測定日時
- OS
- CPU
- RAM
- Pythonバージョン
- Dashバージョン
- Plotlyバージョン
- pandasバージョン
- numpyバージョン
- pyarrowバージョン
- ブラウザ名/バージョン
- データファイル名
- データ行数
- チャンネル数
- ファイルサイズ

確認コマンド例:

```bash
cd /Users/sugusokothx/life/factory/panel-y
code/.venv/bin/python --version
code/.venv/bin/python -c "import dash, plotly, pandas, numpy, pyarrow; print(dash.__version__, plotly.__version__, pandas.__version__, numpy.__version__, pyarrow.__version__)"
```

---

## 4. 測定項目

| ID | 項目 | 説明 | 対象データ |
|----|------|------|------------|
| B-01 | アプリ起動時間 | `python app.py` からHTTP応答可能まで | 小/中/大共通 |
| B-02 | Parquet読み込み時間 | 読み込みボタン押下からステータス表示まで | 小/中/大 |
| B-03 | 初回波形表示時間 | 更新押下から波形表示完了まで | 小/中/大 |
| B-04 | メモリ使用量 | 読み込み後と操作後のPython/ブラウザメモリ | 小/中/大 |
| B-05 | 1回ズーム応答 | ズーム操作から全行反映まで | 小/中/大 |
| B-06 | 連続ズーム耐性 | 10回連続ズーム後の固まり・遅延 | 中/大 |
| B-07 | ズームリセット時間 | 全体表示復帰まで | 中/大 |
| B-08 | ホバー応答 | スパイクラインと値更新の体感遅延 | 小/中/大 |
| B-09 | Cursor配置応答 | A/Bクリックから統計表示まで | 小/中/大 |
| B-10 | FFT実行時間 | FFTボタン押下からスペクトル表示まで | 小/中 |
| B-11 | 作図設定ロード時間 | `.pyc.json` ロードから復元完了まで | 小/中 |

---

## 5. 手動測定手順

### 共通準備

```bash
cd /Users/sugusokothx/life/factory/panel-y/code/proto_3_1b
source ../.venv/bin/activate
python app.py
```

ブラウザで `http://localhost:8050` を開く。

### B-02/B-03 読み込み・初回表示

1. Parquetパスを入力する
2. `読み込み` を押す
3. ステータス表示までの時間を記録する
4. `更新` を押す
5. 波形が表示されるまでの時間を記録する

### B-05/B-06 ズーム

1. 波形を表示する
2. 横方向にズームする
3. 全行に反映されるまでの体感時間を記録する
4. 10回連続でズーム・パンする
5. フリーズ、遅延、メモリ増加を記録する

### B-09 Cursor/統計

1. 計測モードをONにする
2. Cursor Aを配置する
3. Cursor Bを配置する
4. 統計表が表示されるまでの時間を記録する

### B-10 FFT

1. Cursor A/Bを配置する
2. FFT対象チャンネルを選ぶ
3. 窓関数を選ぶ
4. `FFT` を押す
5. スペクトル表示までの時間を記録する

---

## 6. 記録フォーマット

```markdown
## 測定 YYYY-MM-DD

### 環境

- OS:
- CPU:
- RAM:
- Python:
- Dash:
- Plotly:
- pandas:
- numpy:
- pyarrow:
- Browser:

### データ

- file:
- rows:
- channels:
- size:

### 結果

| ID | 結果 | メモ |
|----|------|------|
| B-01 | | |
| B-02 | | |
| B-03 | | |
| B-04 | | |
| B-05 | | |
| B-06 | | |
| B-07 | | |
| B-08 | | |
| B-09 | | |
| B-10 | | |
| B-11 | | |
```

---

## 7. Rust版Phase 1の合格目安

Phase 1では以下を満たすことを目標にする。

- 大データで読み込み後にUIが操作不能にならない
- pan/zoomが表示ピクセル数オーダーの処理で完結する
- 連続ズームでメモリが増え続けない
- Dash/Plotly版よりズーム応答が明確に軽い
- 大データで初回表示までの時間が許容範囲に収まる

具体的な秒数・メモリ上限は、Dash/Plotly版の実測結果取得後に決める。

---

## 8. 注意

ブラウザ描画の手動測定はばらつきが出る。
同じデータ・同じ手順で3回測定し、平均ではなく「最悪値」と「体感」を残す。
