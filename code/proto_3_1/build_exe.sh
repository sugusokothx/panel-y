#!/bin/bash
# multiconverter を単一 exe にビルドする
# 使い方: bash build_exe.sh

set -e

cd "$(dirname "$0")"

pip install pyinstaller -q

pyinstaller \
    --onefile \
    --name multiconverter \
    --clean \
    multiconverter.py

echo ""
echo "ビルド完了: dist/multiconverter"
echo "使い方: .mat ファイルと同じフォルダに置いて実行"
