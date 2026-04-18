"""Panel_y マルチファイル変換 CLI。

指定フォルダ直下の .mat ファイルを一括で Parquet に変換する。

使い方:
    python multiconverter.py --input ./input --output ./output
    python multiconverter.py --input ./input --output ./output --force
"""

import argparse
import sys
from pathlib import Path

from utils.converter import convert_to_parquet


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Panel_y マルチファイル変換 — フォルダ内の .mat を一括 Parquet 変換"
    )
    parser.add_argument("--input", required=True, help="入力フォルダのパス")
    parser.add_argument("--output", required=True, help="出力フォルダのパス")
    parser.add_argument(
        "--force", action="store_true", help="既存の .parquet ファイルを上書きする"
    )
    args = parser.parse_args()

    input_dir = Path(args.input)
    output_dir = Path(args.output)

    # [1] 入力フォルダをスキャン（直下のみ）
    if not input_dir.is_dir():
        print(f"[エラー] 入力フォルダが存在しません: {input_dir}")
        return 1

    mat_files = sorted(input_dir.glob("*.mat"))
    if not mat_files:
        print(f"[警告] .mat ファイルが見つかりませんでした: {input_dir}")
        return 0

    print(f"[スキャン完了] {len(mat_files)} 件の .mat ファイルを検出")

    # [2] 出力フォルダを作成
    output_dir.mkdir(parents=True, exist_ok=True)

    # [3] ループ処理
    results = {"success": [], "skipped": [], "failed": []}

    for mat_file in mat_files:
        out_file = output_dir / (mat_file.stem + ".parquet")

        # スキップ判定
        if out_file.exists() and not args.force:
            print(f"[スキップ] {mat_file.name} → {out_file.name} (既存)")
            results["skipped"].append(mat_file.name)
            continue

        print(f"\n--- {mat_file.name} ---")
        try:
            convert_to_parquet(mat_file, out_file)
            results["success"].append(mat_file.name)
        except Exception as e:
            print(f"[失敗] {mat_file.name}: {e}")
            results["failed"].append((mat_file.name, str(e)))

    # [4] サマリー表示
    print("\n" + "=" * 40)
    print("変換サマリー")
    print("=" * 40)
    print(f"  成功  : {len(results['success'])} 件")
    print(f"  スキップ: {len(results['skipped'])} 件")
    print(f"  失敗  : {len(results['failed'])} 件")

    if results["failed"]:
        print("\n失敗ファイル一覧:")
        for name, err in results["failed"]:
            print(f"  - {name}: {err}")

    # [5] 失敗があれば exit code 1
    return 1 if results["failed"] else 0


if __name__ == "__main__":
    sys.exit(main())
