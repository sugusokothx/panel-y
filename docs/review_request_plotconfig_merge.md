# レビュー依頼書: 作図ファイル機能の proto_3_1b への統合

- **日付**: 2026-04-13
- **ブランチ**: `proto_3_1b`
- **対象ファイル**: `code/proto_3_1b/app.py` （+392行 / -9行）
- **レビュー観点**: 機能統合の正当性、バグ混入の有無、3_1b固有処理との整合性

---

## 1. 背景・目的

proto_3_1 と proto_3_1b は同じコードベースから分岐した兄弟ブランチ。

| ブランチ | 追加機能 |
|----------|----------|
| proto_3_1 | 作図ファイル保存・ロード（.pyc.json）+ バグ修正2件 |
| proto_3_1b | 大容量データ対応（minmax vectorize, searchsorted, float32, df_overview） |

両方の機能を proto_3_1b に統合する。proto_3_1 のコードを 3_1b に折り込む形。

---

## 2. 変更一覧

### 2.1 新規追加（proto_3_1 から移植）

| # | 変更 | 行数(概算) | 説明 |
|---|------|-----------|------|
| A1 | `import json` | 1行 | 作図ファイル読み書きに必要 |
| A2 | `list_config_suggestions()` | 40行 | .pyc.json ファイルのパス補完候補を生成 |
| A3 | レイアウト: 作図ファイルUI | 25行 | config-path-input, 💾保存ボタン, 📂ロードボタン, config-status |
| A4 | `suggest_config_files()` CB | 15行 | config-path-input の入力に応じてファイル候補を表示 |
| A5 | `on_config_suggestion_click()` CB | 10行 | 候補クリック → config-path-input を更新 |
| A6 | `save_plotconfig()` CB | 55行 | 行構成・スタイル・テーマ等をJSON保存 |
| A7 | `load_plotconfig()` CB | 160行 | JSONから一括復元（データ読込 + UI + 波形描画） |

### 2.2 既存コード修正

| # | 変更 | 説明 |
|---|------|------|
| B1 | `make_ch_settings()` シグネチャ変更 | `ch_styles: dict \| None = None` 引数を追加。保存済みの色・太さを反映 |
| B2 | `load_file()` の Output 追加 | `config-path-input` にデフォルト作図パス（`{stem}.pyc.json`）を返す。戻り値数 7→8 |
| B3 | `manage_rows()` 誤発火防止ガード | 削除ボタンの `n_clicks` 存在チェックを追加 |
| B4 | `update_waveform_rows()` スタイル引渡し修正 | `make_ch_settings(used_chs)` → `make_ch_settings(used_chs, ch_styles)` |

### 2.3 3_1b 固有処理との整合（移植時の適応箇所）

`load_plotconfig()` 内のデータ読み込み処理を proto_3_1b 仕様に合わせた:

| 項目 | proto_3_1（移植元） | proto_3_1b（統合後） |
|------|-------------------|---------------------|
| グローバル変数 | `df`, `channels` | `df`, `df_overview`, `time_arr_cache`, `channels` |
| 型キャスト | なし | float32 ダウンキャスト |
| time軸 | 毎回 `.values` | `time_arr_cache` にキャッシュ |
| 非単調time | 未対応 | `np.argsort` でソート補正 |
| ダウンサンプル | なし | `OVERVIEW_POINTS` 間引き → `df_overview` |
| ステータス表示 | 基本情報のみ | メモリ使用量(MB)を追加表示 |
| エラー時のクリーンアップ | `df`, `channels` のみリセット | `df_overview`, `time_arr_cache` もリセット |

---

## 3. レビューで特に確認してほしい点

### 3.1 `load_plotconfig()` の整合性（最重要）

- 3_1b の `load_file()` と同等のデータ初期化処理を `load_plotconfig()` にも複製している
- 重複コードだが、2つのコールバックは異なる Input/Output を持つため共通化が難しい
- **確認**: float32 キャスト、time_arr_cache、df_overview の初期化漏れがないか

### 3.2 `manage_rows()` の誤発火防止

- `load_plotconfig` が `rows-container` を更新すると、パターンマッチ callback の `row-delete` Input が変化して `manage_rows` が誤発火する既知バグへの対策
- **確認**: `not any(n for n in (delete_clicks or []) if n)` のガード条件で十分か。正常な削除操作を阻害しないか

### 3.3 `make_ch_settings()` の後方互換

- 引数追加は `ch_styles=None` のデフォルト値で後方互換を維持
- `load_file()` からの呼び出しは従来通り `make_ch_settings(initial_channels)` （スタイルなし）
- `update_waveform_rows()` と `load_plotconfig()` からは `ch_styles` 付きで呼び出し
- **確認**: 引数なし呼び出し時に `saved.get()` が空dict を返すので問題ないはずだが確認

### 3.4 戻り値のタプル長

- `load_file`: 7→8要素（`config-path-input` 追加）
- `load_plotconfig`: 17要素
- **確認**: 全エラーパスで `no_update` の個数が Output 数と一致しているか

---

## 4. 未変更（意図的にスキップ）

- `assets/style.css`: 作図ファイルUIに必要なスタイルは既存クラス（`input-path`, `btn`, `btn-accent`, `status-text`, `suggestion-item`）で対応済み
- `utils/` 以下: 変更なし
- `generate_test_data.py`, `multiconverter.py`: 変更なし

---

## 5. テスト計画

| # | テスト | 手順 |
|---|--------|------|
| T1 | 通常のデータ読み込み | Parquet読込 → config-path-input にデフォルトパスが入ることを確認 |
| T2 | 作図保存 | 行構成・スタイル変更後に💾 → .pyc.json が正しく生成されること |
| T3 | 作図ロード | 📂 → データ + 行構成 + スタイル + テーマが一括復元されること |
| T4 | ロード後の行削除 | ロード直後に行1が消えないこと（誤発火防止の確認） |
| T5 | ロード後のスタイル変更 | 太さを変更 → 更新ボタン → 値が戻らないこと |
| T6 | 大容量データでの作図ロード | 数百万点のデータで作図ロード → df_overview が生成され、ハングしないこと |
| T7 | 作図ファイルパス補完 | config-path-input に入力 → .pyc.json候補が表示されること |




---

レビュー依頼書に沿って、code/proto_3_1b/app.py を静的レビューしました。
（実行・テストは行っていません）

指摘事項
1) update_waveform_rows() の戻り値数が Output 定義と不一致
update_waveform_rows() は Output を3つ定義していますが、if not row_groups or df is None: の分岐で2要素しか返していません（app.py の 1228-1232 行付近）。
この分岐に入ると Dash 側で callback 例外（戻り値長不一致）が発生します。

お勧めのタスク
update_waveform_rows の全分岐で3要素を返すよう修正

`code/proto_3_1b/app.py` の `update_waveform_rows` を修正する。

1. `@app.callback` の Output 定義（`waveform-container`, `row-groups-store`, `ch-settings-container`）が3つであることを確認する。
2. `if not row_groups or df is None:` 分岐の return を3要素に揃える。\
   例: `(空状態メッセージDiv, [], no_update)` もしくは `(空状態メッセージDiv, [], [])`。
3. 同関数内の他分岐も含め、すべての return のタプル長が3で統一されていることを確認する。
4. 同様のミス再発防止のため、コメントで「Output数=3」の意図を明記する。



2) load_plotconfig() のデータ読み込みに例外処理がなく、UIエラーに直結する
load_plotconfig() では pd.read_parquet(data_path) を try/except で保護していません（837行付近）。
data_path が壊れたファイル・権限不足・ディレクトリ誤指定（data_path が空で Path(".") になるケース含む）だと callback がそのまま落ちます。

お勧めのタスク
load_plotconfig の read_parquet と data_path 妥当性チェックを強化

`code/proto_3_1b/app.py` の `load_plotconfig` で、`data_path` 検証と読み込み失敗時の復帰を追加する。

1. `data_path_str` 取得直後に空文字チェックを追加し、未設定なら `config-status` に明示的エラーを返す。
2. `data_path.exists()` だけでなく `is_file()` と `.suffix == ".parquet"` も確認する。
3. `pd.read_parquet(data_path)` を try/except で囲み、失敗時は 17 Output 長を維持したエラー return を返す。
4. 失敗時にグローバル状態（`df`, `df_overview`, `time_arr_cache`, `channels`）を安全側へリセットするか、既存状態維持方針を明文化して一貫させる。



3) load_file() / load_plotconfig() のサンプリング周波数計算が1点データで落ちる
両関数とも ts = df["time"].iloc[1] - df["time"].iloc[0] を前提にしており、行数が1以下だと IndexError になります（load_plotconfig 868行、load_file 1045行付近）。
また ts == 0 でも 1/ts が不正になります。

お勧めのタスク
time列の最小行数とゼロ刻みを考慮したステータス表示に修正

`code/proto_3_1b/app.py` の `load_file` と `load_plotconfig` で周波数表示計算を堅牢化する。

1. `len(df) >= 2` を確認してから `ts` を計算する。
2. `len(df) < 2` または `ts == 0` の場合は Hz 表示を `"N/A"` にする等のフォールバックを入れる。
3. 既存の `load_status` 文字列フォーマットを共通化し、片方だけ修正漏れが起きないよう同一ロジックに寄せる。
4. 必要なら `time` 列の存在チェックも追加し、欠落時は明示エラーを返す。
