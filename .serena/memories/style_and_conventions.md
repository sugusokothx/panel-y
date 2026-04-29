# コードスタイル・規約

## Python
- 関数ベースの構成（クラスは使用していない）
- 型ヒント: 関数の戻り値に使用あり（例: `def main() -> int:`）
- docstring: モジュールレベルで日本語の説明あり
- 変数名・関数名: snake_case
- 定数: UPPER_SNAKE_CASE（例: `GRAPH_HEIGHT`, `DECIMATE_THRESHOLD`）
- コメント: 日本語で記載

## Dashアプリ固有
- コールバックはデコレータ方式（`@app.callback`）
- pattern-matching callback（`ALL`）で動的UI対応
- Storeコンポーネントで状態管理（hover-x-store, xaxis-range-store）

## プロジェクト規約
- 開発日誌は `journal/YYYY-MM-DD.md` に記録
- 設計書・バグレポートは `docs/` に配置
- 新バージョンは `code/proto_X_Y/` ディレクトリで開発
