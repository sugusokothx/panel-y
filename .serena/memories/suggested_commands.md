# 開発で使うコマンド一覧

## 環境セットアップ
```bash
cd code/proto_3_1
source ../.venv/bin/activate   # 共有venv
pip install -r requirements.txt
```

## アプリ起動
```bash
python app.py
# → http://localhost:8050
```

## CLIツール
```bash
# .matファイル一括Parquet変換
python multiconverter.py --input ./input --output ./output
python multiconverter.py --input ./input --output ./output --force  # 上書き

# テストデータ生成
python generate_test_data.py
```

## ユーティリティ（システムコマンド）
```bash
git status / git log / git diff   # バージョン管理
ls / find / grep                  # ファイル探索（macOS/Darwin）
```

## 備考
- テストフレームワーク: 未導入（テストスイートなし）
- Linter/Formatter: 明示的な設定なし（プロジェクト内に.flake8, pyproject.toml等は未確認）
