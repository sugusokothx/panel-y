---
project: Panel_y
doc_type: 作図設定仕様
target: Rust移植 Phase 0
created: "2026-04-27"
updated: "2026-04-27"
status: 完了
source_proto: "code/proto_3_1b"
---

# Rust移植 Phase 0 作図設定仕様

---

## 1. 目的

Rust版は、現行 `proto_3_1b` が保存する `.pyc.json` を読み込み互換対象とする。
本書では現行フォーマットを固定し、Rust版での互換方針を定義する。

---

## 2. 現行フォーマット

現行保存処理は `code/proto_3_1b/app.py` の `save_plotconfig()`。

トップレベル構造:

```json
{
  "version": 1,
  "data_path": "/path/to/data.parquet",
  "rows": [],
  "ch_styles": {},
  "theme": "dark",
  "scroll_zoom": false
}
```

---

## 3. トップレベル項目

| キー | 型 | 必須 | 現行デフォルト | 説明 |
|------|----|------|----------------|------|
| `version` | number | 必須 | `1` | 作図設定フォーマットのバージョン |
| `data_path` | string | 必須 | `""` | 読み込むParquetファイルのパス |
| `rows` | array | 必須 | `[]` | 行構成 |
| `ch_styles` | object | 必須 | `{}` | チャンネル別の色・線幅 |
| `theme` | string | 任意 | `"dark"` | `"dark"` または `"light"` |
| `scroll_zoom` | boolean | 任意 | `false` | PlotlyのscrollZoom相当 |

Rust版読み込み方針:

- `version` が未指定の場合は `1` とみなしてもよい
- `data_path` が空、存在しない、Parquetでない場合は明示エラー
- 未知キーは無視する
- 型不正は安全側にフォールバックし、復元できない項目だけ捨てる

---

## 4. `rows` 項目

各行の構造:

```json
{
  "channels": ["sine_50Hz", "noise"],
  "ymin": null,
  "ymax": null,
  "step_channels": ["pwm_1kHz"]
}
```

| キー | 型 | 必須 | 説明 |
|------|----|------|------|
| `channels` | string array | 必須 | この行に重ね表示するチャンネル |
| `ymin` | number/null | 任意 | Y軸下限 |
| `ymax` | number/null | 任意 | Y軸上限 |
| `step_channels` | string array | 任意 | 階段表示するチャンネル |

Rust版読み込み方針:

- Parquetに存在しないチャンネルは無視する
- `channels` が空になった行は表示対象から除外する
- `step_channels` に存在するが `channels` にないチャンネルは無視する
- `ymin > ymax` の場合は当該範囲指定を無効化する
- `ymin` だけ、`ymax` だけの指定も許容する

---

## 5. `ch_styles` 項目

構造:

```json
{
  "sine_50Hz": {
    "color": "#1f77b4",
    "width": 1
  }
}
```

| キー | 型 | 必須 | 説明 |
|------|----|------|------|
| `color` | string | 任意 | `#RRGGBB` 形式を基本とする |
| `width` | number | 任意 | 線幅。現行UIでは0.5から5、step 0.5 |

Rust版読み込み方針:

- 不正な色はデフォルト色へフォールバックする
- 不正な線幅は `1` へフォールバックする
- Parquetに存在しないチャンネルのスタイルは保持しても無視してもよい
- 保存時は使用中チャンネルだけ保存する方針でよい

---

## 6. テーマ

現行値:

- `dark`
- `light`

Rust版方針:

- `dark` / `light` を読み込み互換対象にする
- 不明な値は `dark` にフォールバックする
- Rust版で追加テーマを持つ場合も、保存時は互換フィールドを維持する

---

## 7. scroll_zoom

現行の意味:

- `false`: スクロールズーム無効。UI表示は `スケール固定`
- `true`: スクロールズーム有効。UI表示は `スケール解除`

Rust版方針:

- 入力デバイス操作設計が変わっても、設定値は読み込む
- Rust版UIで同じ名前を使うかはPhase 2で決める

---

## 8. サンプル

```json
{
  "version": 1,
  "data_path": "/Users/user/panel-y/code/proto_3_1b/data/test_100k.parquet",
  "rows": [
    {
      "channels": ["sine_50Hz", "chirp_1_500Hz"],
      "ymin": -12,
      "ymax": 12,
      "step_channels": []
    },
    {
      "channels": ["pwm_1kHz"],
      "ymin": -1,
      "ymax": 6,
      "step_channels": ["pwm_1kHz"]
    }
  ],
  "ch_styles": {
    "sine_50Hz": {"color": "#1f77b4", "width": 1},
    "chirp_1_500Hz": {"color": "#ff7f0e", "width": 1},
    "pwm_1kHz": {"color": "#2ca02c", "width": 1.5}
  },
  "theme": "dark",
  "scroll_zoom": false
}
```

---

## 9. Rust版保存フォーマット

Phase 4までは現行互換を優先する。

Rust版固有項目が必要になった場合:

- `version: 2` とする
- `version: 1` の読み込み互換は維持する
- 追加項目はトップレベルに置くか、`rust` 名前空間を作る
- 互換に不要な描画キャッシュやLOD情報は保存しない

将来候補:

- 拡張子を `.panely.json` に変更
- `.pyc.json` は読み込み互換専用として扱う

---

## 10. 互換性テスト項目

- 現行Dash版で保存した設定をRust版で読み込める
- Rust版で保存した設定をRust版で再ロードできる
- 存在しないチャンネルを含む設定で落ちない
- 空行を含む設定で落ちない
- `data_path` が存在しない場合に明示エラーを出す
- テーマ不正値で落ちない
- 色不正値で落ちない
- 線幅不正値で落ちない
