# 実装計画書: TypeScript波形アナライザー

## プロジェクト概要

Pythonで解析した電力変換器（インバータ等）の波形データ（JSON形式）を読み込み、
ブラウザ上でインタラクティブに可視化・解析するツール。

---

## 技術スタック

| 項目 | 内容 |
|------|------|
| 言語 | TypeScript |
| ビルドツール | Vite (Vanilla TypeScript テンプレート) |
| グラフライブラリ | Plotly.js (`plotly.js-dist-min`) |
| データソース | `waveform.json` (Pythonスクリプトで生成) |

---

## 実装ステップ

### Step 1: プロジェクト初期化

- `npm create vite@latest . -- --template vanilla-ts` でViteプロジェクトを生成
- 依存パッケージのインストール:
  - `npm install plotly.js-dist-min`
  - `npm install --save-dev @types/plotly.js-dist-min`

### Step 2: 型定義ファイルの作成 (`src/types.ts`)

```ts
export interface WaveformData {
  time: number[];
  voltage: number[];
  current: number[];
}
```

- `time`, `voltage`, `current` の3フィールドを持つインターフェースを定義
- 後続のバリデーション・描画処理で型安全性を確保

### Step 3: データロードとバリデーション (`src/main.ts` 内)

- `fetch('./waveform.json')` でJSONを非同期取得
- 取得データが `WaveformData` の形式に合致するか簡易チェック:
  - 各フィールドが存在し `Array.isArray` で数値配列であることを確認
  - 不正データの場合はエラーメッセージをUIに表示して処理中断

### Step 4: 可視化ロジックの実装 (`src/main.ts` 内)

Plotly.js を用いて以下の仕様で描画:

- **レイアウト:** 上下2段のサブプロット
  - 上段: 電圧波形（Voltage [V]）
  - 下段: 電流波形（Current [A]）
- **X軸リンク:** `xaxis2.matches = 'x'` で時間軸を同期（片方を拡大すると両方が連動）
- **ホバー設定:**
  - `hovermode: 'x unified'` で全チャンネルの値を同時表示
  - `spikedistance: -1` / `xaxis.showspikes: true` でスパイク線を有効化

### Step 5: Pythonサンプルスクリプトの作成 (`generate_sample.py`)

テスト用波形データを生成して `waveform.json` に出力:

- **電圧波形:** 50Hz 正弦波（振幅 200V）
- **電流波形:** 50Hz 正弦波（振幅 10A）+ 高周波ノイズ（ランダム成分）
- サンプリング: 0〜0.04秒、1000点
- 出力形式: `{ "time": [...], "voltage": [...], "current": [...] }`

---

## 成果物一覧

| ファイル | 説明 |
|----------|------|
| `index.html` | アプリのエントリーポイントHTML |
| `src/main.ts` | データロード・バリデーション・Plotly描画ロジック |
| `src/types.ts` | `WaveformData` インターフェース定義 |
| `generate_sample.py` | テスト用JSON波形データ生成スクリプト |
| `waveform.json` | Pythonスクリプトで生成されるデータファイル |

---

## 起動手順

```bash
# 1. サンプルデータを生成
python generate_sample.py

# 2. 開発サーバーを起動
npm run dev
```

ブラウザで `http://localhost:5173` にアクセスして波形を確認。

---

## 実装上の注意点

- `waveform.json` は `public/` ディレクトリに配置し、`fetch` で取得できるようにする
- Plotly.js の型定義は `plotly.js-dist-min` 向けの `@types/plotly.js-dist-min` を使用する
- X軸リンクにより拡大・縮小・パン操作が電圧・電流グラフで完全同期される
