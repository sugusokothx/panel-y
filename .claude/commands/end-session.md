---
description: セッション終了の定型処理 (memory 更新 + journal 作成 + commit + push 確認)
---

# セッション終了処理

このセッションを区切りとして記録する。以下の 4 ステップを順に実行する。

> 標準仕様: `life/.claude/rules/factory-journal.md`
> 出典: factory 共通雛形 (`life/templates/factory/.claude/commands/end-session.md`) を panel-y 用に配備

## 前提確認

- 現在の `git status` を確認し、**未コミットの本作業 (journal/memory 以外の変更) があれば先にそれをコミット**してから journal ステップに進む
- 未コミットの本作業がない場合はそのまま進む

## ステップ 1: project memory の更新 (該当時)

**※ panel-y は現時点で project memory 無し（省略）**

(参考: 将来 memory を運用する場合は `~/.claude/projects/-Users-sugusokothx-life-factory-panel-y/memory/project_status.md` を対象とする想定。導入時に本節を有効化する)

- 既存の「次ステップ」セクション末尾に、本セッションの進展を追記する
- 追記内容には以下を含める:
  - 本セッションで完了した主要タスク (コミットハッシュ付き)
  - 確定した重要な設計判断・規律
  - 次セッションの起点 (次にやるべきこと、前提知識、注意点)
- 既存の記述と重複する場合は新規追記ではなく既存節を更新
- 「## 注意点」など他セクションも、今日の作業で古くなった部分があれば更新

## ステップ 2: journal の作成

対象ファイル: `journal/YYYY-MM-DD.md` (今日の日付)

- **既存ファイルがあれば追記**、無ければ新規作成
- フォーマット標準: `life/.claude/rules/factory-journal.md` 参照

```markdown
# YYYY-MM-DD 開発日誌

## 本日のテーマ
(1〜2 段落でセッションの主目的)

## 進め方の合意
(意思決定の経緯、選んだ案 vs 却下した案)

## 成果物
(コミット / 新ファイル / 設計書など)

## 今日の重要発見
(技術的気づき、設計判断、運用ルール)

## レビュー (該当時のみ)
(Gemini/GPT/ユーザー指摘 + 反映内容)

## 明日以降
(次セッションの着手点)

## 所感
(セッションの振り返り、メタな気づき)
```

セクションは内容に応じて取捨選択。

## ステップ 3: journal を 1 commit

- `journal/YYYY-MM-DD.md` のみを add
- コミットメッセージは `docs(journal): YYYY-MM-DD 開発日誌 — <主テーマ>` 形式
- 本文に本日の主要トピックを箇条書き
- 既存運用に従い `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` を付ける

## ステップ 4: push 確認

- **自動 push しない**。ユーザに「push しますか?」と必ず確認する
- ローカルに何コミット先行しているかを示す (`git status` 出力 or `git log origin/main..HEAD --oneline`)
- ユーザが OK と答えてから `git push origin main` を実行

## 終了報告

push 完了 (or push 見送り) 後、以下を 1 メッセージで報告:
- 本セッションのコミット一覧 (短いハッシュ + 一行要約)
- 次セッションの起点 (memory に書いた内容と整合)
- 必要なら長期 TODO のリマインダ提案 (`/schedule` の自然な提案、強制ではない)
