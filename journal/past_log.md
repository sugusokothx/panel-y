# panel-y 過去開発ログ

秘書課ジャーナル（`.secretary/journal/`）から抽出。2026-03-27 転記。

---
2026-03-10

- 波形解析ビューアを作成するプロジェクトを立ち上げる（過去メモにある）

---
2026-03-15

- プロジェクト立ち上げ、企画書・設計書作成（`/projects/panel_y`）
- Proto #1 実用ビューア 実装完了 + コミット（`d08a23b`）
  - 前処理ユーティリティ（CSV/MAT→Parquet変換）
  - Dashアプリ（ファイル選択+補完、chドロップダウン、計測モード+カーソル差分）
  - 動作確認OK

---
2026-03-16

- Panel_y 設計ドキュメント整備
  - proto_1_設計書.md: 実装に合わせて全面更新
  - 企画書.md: ステータス・ロードマップ・未決定事項を更新
  - preprocessor_design.md: status→実装済み、multiconverter追記
- Proto #1b（Panel + Bokeh版）設計企画書を新規作成
  - Dash版との比較、Bokeh固有の実装方針整理
  - Phase分割の実装計画策定
- Serenaオンボーディング実施

---
2026-03-22

- panel-y を /newspace → /life/factory/panel-y に移動、.gitignore・Serena設定更新
- panel-y プロジェクトのオンボーディング実施（Serenaメモリ・Exploreエージェント）
- TIL: Serenaのアクティブプロジェクトはworking directoryで決まる

---
2026-03-24

- proto_0_3 を feature/proto-0.3 ブランチにコミット
- Proto #0.4（ドロップダウンUI方式）新規作成・動作確認OK
- 波形表示機能ロードマップ作成（`wave_viewer_roadmap.md`）
- Proto #0.3, #0.4 の設計書作成
- Proto #2.0 完成（#1 + #0.4 統合）
  - 行ごとドロップダウンで複数チャンネル重ね表示
  - make_subplots方式を試行→ホバー連動不可のため独立Graph方式に方針転換
  - スケールロック（デフォルトOFF、解除時はX軸のみズーム）追加
  - Y軸範囲の数値指定（行ごと）追加
  - Y軸目盛りSI接頭辞表記
  - 更新キーボードショートカット（Ctrl+Enter）追加
  - X軸ズーム全行同期（ctx.triggered_id分岐廃止→常時全適用方式で解決）
- 共通venv化（panel-y直下に.venv、各proto個別venv削除）
- Proto #2.0 設計書・バグレポート作成
- feature/proto-2.0 ブランチにコミット・push
- TIL: Plotlyの`make_subplots`はホバー連動・スパイクライン縦断ができない根本制約がある。独立Graph+Store同期が唯一の実用解法

---
2026-03-25

- feature/proto-2.0 → main へマージ・push完了
- Phase 1-2（line/step表示切り替え）実装・動作確認OK
- Phase 1-3（線の色・太さ選択）実装・動作確認OK
- UI改善（Label列拡大+リサイズ、Value表示改善、legend非表示、ホバー全行修正）
- カーソル区間解析機能（Mean/Max/Min/P-P/RMS/Slope + Hz表示）実装
- Proto #3.0 — Min-Max envelopeデータ間引き + ズーム連動 実装・push完了
- TIL: Min-Max envelope（バケットごとにmin/max保持）はオシロ的描画に最適。ピーク・スパイクを欠落させずに100k→2500点に圧縮できる
- 申し送り: Proto #3.0 を会社データで実地テスト → 間引き閾値の調整

---
2026-03-26

- Proto #3.1 デザイン改善（Dark/Lightテーマ、ch設定2行カード化、行定義折りたたみ）
- ズーム同期バグ修正（2行目以降のズームが他行に反映されない問題）
- ラベル列幅のヘッダー一括調整（ResizeObserver）
- Y軸タイトル重複表示の削除
- ラベル列幅同期バグ修正（個別inline style → CSS変数方式 + flex-shrink:0）
- ヘッダーバーのスティッキー固定 + 📌ピン留めトグル追加
- Proto #3.1 コミット&プッシュ（`31b0f54`）
- Serena: lifeリポ全体+panel-yのオンボーディング完了（メモリ5件作成）
- TIL: ResizeObserverで個別要素のinline style.widthを設定する方式は、React再レンダリングでDOMが再生成されるとリセットされる。CSSカスタムプロパティをコンテナに設定する方式のほうが堅牢
- 申し送り: Proto #3.1 を会社データで実地テスト → 間引き閾値の調整、次の着手候補 → 信号処理(LPF) or カーソル間FFT or コマンドライン
