# panel-y — 波形ビューワアプリ

**コンセプト**: 計測データ（.mat / .csv / .parquet）を読み込み、波形をインタラクティブに可視化・分析するデスクトップWebアプリ。会社のパワーエレクトロニクス・モータ制御データの解析ツール。

## ディレクトリ構成

```
factory/panel-y/
├── CLAUDE.md              ← このファイル
├── docs/                  ← 設計書・企画書・バグレポート
│   ├── 企画書.md
│   ├── wave_viewer_roadmap.md
│   ├── proto_*_設計書.md
│   └── bug_report_*.md
├── journal/               ← 開発日誌（YYYY-MM-DD.md）← 秘書課はここを読む
└── code/                  ← 実装
    ├── requirements.txt
    ├── old_type/          ← 初期プロトタイプ（アーカイブ）
    │   ├── Proto_0_0/     ← 旧TypeScript実装
    │   ├── proto_0_1/ ～ proto_0_4/  ← Python初期版
    ├── proto_1/           ← マルチフォーマット対応
    ├── proto_2_0/         ← Min-Max envelope実装
    ├── proto_3_0/         ← データ間引き + ズーム連動
    └── proto_3_1/         ← 現行最新（FFT・テーマ対応）
```

## 現在の状態

- **最新**: `proto_3_1/` — FFTスペクトル解析（Cursor A-B区間）実装済み
- **次の着手候補**:
  - 信号処理（LPF フィルタ）
  - カーソル間FFT の改善
  - コマンドライン対応（ファイルパス引数）

## 開発の進め方

```bash
cd code/proto_3_1
source ../.venv/bin/activate  # または proto内の venv
python app.py
```

## 秘書課との連携

- **開発指示は秘書課からしない** — 開発はここで完結
- **進捗共有**: `journal/YYYY-MM-DD.md` に記録 → 秘書課が日報に転記
- **トレードとは無関係** — 会社業務のツール開発

## 行動規範

### 「焦ったら止まれ」ルール

コンテキストが枯渇してきたら品質を落として続行せず、「ここで一度区切りましょう」と中断を宣言すること。
