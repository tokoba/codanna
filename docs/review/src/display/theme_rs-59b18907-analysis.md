# theme.rs Review

## TL;DR

- 目的: 端末出力の色・スタイルを一元管理するためのグローバルなテーマと整形ヘルパーを提供
- 主要公開API: THEME、Theme::default、success_with_icon、error_with_icon、warning_with_icon、apply、should_disable_colors
- 中核ロジック: NO_COLORや端末判定に基づくカラー出力の有効/無効化、およびStyleによる条件付き整形
- 複雑箇所: 端末判定と環境変数の組み合わせによるカラーフラグの決定（出力先/環境の変化に追従）
- 重大リスク: Unicodeアイコンの互換性、環境変数のグローバル性によるテスト並列実行時の不安定さ、2つのカラーライブラリの併用による一貫性
- 性能: 文字列整形はO(n)（n=テキスト長）。NO_COLOR/端末判定はO(1)だが毎回評価されるため軽微なオーバーヘッドあり
- 安全性: unsafe無し、グローバル初期化はLazyLockでスレッド安全。インジェクションや権限関連は該当なし

## Overview & Purpose

このファイルは、ターミナル出力のための色・スタイル設定を提供します。アプリ全体で統一された外観を実現するため、グローバルに共有されるテーマ THEME と、成功/エラー/警告メッセージを装飾するヘルパーを公開します。端末が色をサポートしない場合や NO_COLOR が設定されている場合は、自動的に非カラー出力へフォールバックします。（根拠: THEME, Theme 構造体, should_disable_colors 関数; 行番号:不明）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Static | THEME: LazyLock<Theme> | pub | グローバルなテーマの遅延初期化と共有 | Low |
| Struct | Theme | pub | 用途別のStyle（success/error等）の束ね役 | Low |
| Impl(Default) | Theme::default() | pub | デフォルトの配色・装飾の定義 | Low |
| Fn (impl Theme) | success_with_icon(&self, &str) -> String | pub | ✓ アイコン付き成功メッセージ整形 | Low |
| Fn (impl Theme) | error_with_icon(&self, &str) -> String | pub | ✗ アイコン付きエラーメッセージ整形 | Low |
| Fn (impl Theme) | warning_with_icon(&self, &str) -> String | pub | ⚠ アイコン付き警告メッセージ整形 | Low |
| Fn (impl Theme) | should_disable_colors() -> bool | pub | 色出力の無効化条件を判定（NO_COLOR/端末） | Low |
| Fn (impl Theme) | apply<T: Display>(&self, &Style, T) -> String | pub | 条件付きでStyleを適用し文字列化 | Low |

### Dependencies & Interactions

- 内部依存
  - 各アイコン付き整形関数は should_disable_colors() を呼び出して分岐（行番号:不明）
  - apply は引数の Style と Display を利用し、should_disable_colors() によって適用有無を切り替え（行番号:不明）
  - THEME は LazyLock により初回アクセス時に Theme::default() で初期化（行番号:不明）

- 外部依存

| クレート/モジュール | 具体項目 | 用途/備考 |
|---------------------|----------|-----------|
| console | Style | ANSIスタイル表現と適用 |
| owo_colors | OwoColorize | アイコン（✓, ✗, ⚠）の色付け |
| is_terminal | IsTerminal | stdout が端末かどうかの判定 |
| std::sync | LazyLock | スレッド安全な遅延初期化 |
| std::env | var | NO_COLOR 環境変数の確認 |

- 被依存推定
  - CLIの出力層（進捗、結果、警告表示）
  - ログ整形（人間向けのカラーログ）
  - ファイルパスやコード片を強調するUI補助

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| THEME | pub static THEME: LazyLock<Theme> | グローバルテーマ共有 | O(1) 初回のみ初期化 | O(1) |
| Theme::default | fn default() -> Theme | デフォルト配色/装飾の構築 | O(1) | O(1) |
| Theme::success_with_icon | fn success_with_icon(&self, text: &str) -> String | ✓ + successスタイルで整形 | O(n) | O(n) |
| Theme::error_with_icon | fn error_with_icon(&self, text: &str) -> String | ✗ + errorスタイルで整形 | O(n) | O(n) |
| Theme::warning_with_icon | fn warning_with_icon(&self, text: &str) -> String | ⚠ + warningスタイルで整形 | O(n) | O(n) |
| Theme::should_disable_colors | fn should_disable_colors() -> bool | 色を無効にすべきか判定 | O(1) | O(1) |
| Theme::apply | fn apply<T: Display>(&self, style: &Style, text: T) -> String | 条件付きで style を適用 | O(n) | O(n) |

各APIの詳細:

1) THEME: LazyLock<Theme>
- 目的と責務
  - アプリ全体で共有されるテーマの単一インスタンスを提供（行番号:不明）
- アルゴリズム
  - 初回アクセス時に Theme::default() を呼び出して初期化。以降は同一インスタンスを返す
- 引数: なし
- 戻り値: グローバルな Theme 参照（静的ライフタイム）
- 使用例
  ```rust
  use crate::display::theme::THEME;

  fn print_success() {
      println!("{}", THEME.success_with_icon("Completed"));
  }
  ```
- エッジケース
  - 並列初期化: LazyLockにより一度だけ初期化、データ競合なし

2) Theme::default() -> Theme
- 目的と責務
  - success/error/warning/info/header/emphasis/dim/path/number/code それぞれのデフォルト Style を決定（行番号:不明）
- アルゴリズム
  - console::Style::new() から色/装飾（bright/bold/dim等）を設定して構築
- 引数: なし
- 戻り値: Theme
- 使用例
  ```rust
  use crate::display::theme::Theme;

  let theme = Theme::default();
  let s = theme.apply(&theme.header, "Title");
  ```
- エッジケース
  - 端末が色非対応でも Theme 自体は常に構築可能

3) Theme::success_with_icon(&self, text: &str) -> String
- 目的と責務
  - 成功メッセージを ✓ アイコン付きで整形（行番号:不明）
- アルゴリズム
  - should_disable_colors() の結果で分岐
    - true: "✓ {text}" を返す
    - false: "✓" を緑色に、text を success スタイルで装飾して連結
- 引数

  | 名前 | 型 | 意味 |
  |------|----|------|
  | text | &str | 表示する本文 |

- 戻り値

  | 型 | 意味 |
  |----|------|
  | String | 整形済みメッセージ |

- 使用例
  ```rust
  use crate::display::theme::THEME;

  println!("{}", THEME.success_with_icon("OK"));
  ```
- エッジケース
  - NO_COLOR/非端末時は無色でアイコンのみ（ASCII互換でない端末では見た目崩れの可能性）

4) Theme::error_with_icon(&self, text: &str) -> String
- 目的と責務
  - エラーメッセージを ✗ アイコン付きで整形（行番号:不明）
- アルゴリズム
  - should_disable_colors() で無色/有色を分岐
- 引数/戻り値/例: success_with_icon と同様
- エッジケース
  - Unicode ✗ に非対応な環境での表示崩れの可能性

5) Theme::warning_with_icon(&self, text: &str) -> String
- 目的と責務
  - 警告メッセージを ⚠ アイコン付きで整形（行番号:不明）
- アルゴリズム
  - should_disable_colors() で無色/有色を分岐
- 引数/戻り値/例: success_with_icon と同様
- エッジケース
  - Unicode ⚠ に非対応な環境での表示崩れの可能性

6) Theme::should_disable_colors() -> bool
- 目的と責務
  - 環境変数 NO_COLOR の存在、または stdout が端末でない場合に色出力を無効にする（行番号:不明）
- アルゴリズム
  - std::env::var("NO_COLOR").is_ok() || !std::io::stdout().is_terminal()
- 引数: なし
- 戻り値

  | 型 | 意味 |
  |----|------|
  | bool | true: 色無効, false: 色有効 |

- 使用例
  ```rust
  use crate::display::theme::Theme;

  if Theme::should_disable_colors() {
      eprintln!("Colors disabled");
  }
  ```
- エッジケース
  - 実行中に NO_COLOR が変更されると出力の一貫性が崩れる可能性（後述）

7) Theme::apply<T: Display>(&self, style: &Style, text: T) -> String
- 目的と責務
  - 任意の Display を Style で装飾（条件付き）して String 化（行番号:不明）
- アルゴリズム
  - should_disable_colors() が true の場合は text.to_string()
  - false の場合は style.apply_to(text).to_string()
- 引数

  | 名前 | 型 | 意味 |
  |------|----|------|
  | style | &console::Style | 適用するスタイル |
  | text | T: Display | 表示可能な任意の値 |

- 戻り値

  | 型 | 意味 |
  |----|------|
  | String | 整形済み文字列 |

- 使用例
  ```rust
  use crate::display::theme::THEME;

  let n = 42;
  println!("{}", THEME.apply(&THEME.number, n));
  ```
- エッジケース
  - text が巨大な場合、メモリアロケーションが増える（O(n)）

データコントラクト:
- Unicodeの使用: ✓, ✗, ⚠ のUnicode記号を出力
- スタイル適用可否: should_disable_colors() による二値判定
- 返却型: すべて所有Stringを返す（出力時の所有権は呼び出し側）

## Walkthrough & Data Flow

- グローバル初期化
  1. 最初に THEME にアクセスした時、LazyLock が Theme::default() を実行して Theme を構築（行番号:不明）
  2. 以降 THEME は同じインスタンスを参照

- メッセージ整形フロー（例: success_with_icon）
  1. 呼び出し側が THEME.success_with_icon("Done") を呼ぶ
  2. should_disable_colors() が NO_COLOR または stdout 非端末をチェック
  3. true なら "✓ Done"、false なら "✓" を緑色、"Done" を success スタイルで装飾し結合
  4. 結果の String を返却

- 任意スタイル適用（apply）
  1. 呼び出し側が THEME.apply(&THEME.header, "Title") を呼ぶ
  2. should_disable_colors() の結果に応じて装飾の有無を切り替え
  3. String を返却

このチャンクは単純な直線処理で分岐は2つ程度のため、Mermaid図の基準には該当しません。

## Complexity & Performance

- 時間計算量
  - success_with_icon / error_with_icon / warning_with_icon / apply: O(n)（n=text の表示長）。整形と連結による
  - should_disable_colors: O(1)（環境変数チェックと端末判定）
  - LazyLock 初期化: O(1)

- 空間計算量
  - 各整形関数/適用: O(n)（新規 String の割当と内容コピー）

- ボトルネック・スケール限界
  - 短いユーザメッセージに最適。巨大な出力を大量に生成する場合、毎回 String を確保するため GC のないRustでもヒープ負荷が増える
  - should_disable_colors を毎回評価しているため、ごくわずかな定数オーバーヘッドがある（通常は無視可能）

- 実運用負荷要因
  - I/O（println!等）側が主ボトルネックであり、このモジュールの整形コストは相対的に小さい

## Edge Cases, Bugs, and Security

- メモリ安全性
  - unsafe 未使用。全て借用/所有の範囲内で完結し、バッファオーバーフローやUAFの懸念なし（行番号:不明）
  - 文字列生成は標準の String/format! を用い安全

- インジェクション
  - 本モジュールは文字列整形のみで外部コマンド/SQL/ファイルアクセスを行わないため該当なし

- 認証・認可
  - 該当なし

- 秘密情報
  - ハードコードされた秘密情報なし
  - ログ漏えいは呼び出し側の text に依存。ここでは無加工で表示するため、機密情報を渡さない設計/運用が必要

- 並行性
  - THEME は LazyLock により初期化時の競合が防がれる
  - 環境変数はプロセスグローバルなので、テスト並列実行時に NO_COLOR の設定・解除が競合すると出力の一貫性が乱れる可能性

- 2つのカラーライブラリの併用
  - アイコン色付けに owo_colors、本文スタイルに console を併用。ANSIコードの競合は通常発生しないが、保守の一貫性はやや低下

- Unicodeアイコンの互換性
  - 古いWindowsコンソールや特殊環境で ✓/✗/⚠ が表示できない可能性。ASCII代替を用意していない

エッジケース詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| NO_COLOR 設定 | NO_COLOR=1 | 全出力を無色にする | should_disable_colors | OK |
| 非端末出力 | stdoutをファイルにリダイレクト | 全出力を無色にする | should_disable_colors | OK |
| NO_COLOR 変更中 | 実行中にNO_COLORを切替 | 出力が途中から変化する可能性 | should_disable_colorsが毎回評価 | 要注意 |
| Unicode非対応端末 | レガシー端末 | アイコンが豆腐/崩れ | フォールバック無し | 改善余地 |
| 非ASCII本文 | "日本語✓" | そのまま表示（色は条件次第） | 文字列整形のみ | OK |
| 巨大な本文 | 数十MBのtext | 整形/割当コスト増 | O(n)整形 | 要注意 |
| 改行含む本文 | "line1\nline2" | 改行維持で整形 | 文字列連結 | OK |
| 並列テスト | 複数スレッドでNO_COLOR操作 | テストが不安定 | 環境変数はグローバル | 要対策 |

## Design & Architecture Suggestions

- カラー判定のキャッシュ化
  - 起動時/初回判定結果をキャッシュし、都度の環境変数/端末判定を避けるオプションを追加（例: FORCE_COLOR/NO_COLOR優先順位の明示的サポート）

- アイコンのASCIIフォールバック
  - Unicode不可環境に備え、"OK"/"ERR"/"WARN" や "+"/"x"/"!" への切替設定を提供

- ライブラリの統一
  - console と owo_colors の併用をどちらかに統一し、保守性を向上（例: すべて console::Style に寄せる）

- 構成可能な Theme
  - Builderパターンまたは設定読込で色/装飾を外部から上書き可能に
  - アイコン有無（on/off）や明度（bright/bold）を選択可能に

- 遅延文字列化の提供
  - 文字列確保を避け、Displayを実装するラッパ型を返すAPI（必要時にANSI適用）を追加しパフォーマンス改善余地を作る

## Testing Strategy (Unit/Integration) with Examples

- テストの基本方針
  - NO_COLOR による分岐はシリアル実行（serial_test クレート等）で環境変数競合を回避
  - 端末判定はモックが困難なため、NO_COLOR を使うテスト中心に
  - 文字列の含有チェック（ANSIコード非含有/含有）で期待を検証

- 単体テスト例（NO_COLORで無色を検証）
  ```rust
  use std::env;
  use crate::display::theme::{THEME, Theme};

  // serial_test::serial アトリビュートを推奨
  #[test]
  fn success_with_icon_no_color() {
      env::set_var("NO_COLOR", "1");
      let out = THEME.success_with_icon("Done");
      // 無色の想定: そのまま "✓ Done"
      assert_eq!(out, "✓ Done");
      env::remove_var("NO_COLOR");
  }

  #[test]
  fn apply_no_color_returns_plain_text() {
      env::set_var("NO_COLOR", "1");
      let theme = Theme::default();
      let out = theme.apply(&theme.header, "Title");
      assert_eq!(out, "Title");
      env::remove_var("NO_COLOR");
  }
  ```

- 有色ケースの一例（環境依存のため注意）
  - テスト環境で stdout が端末でない場合が多い。CI ではスキップや条件分岐を推奨
  ```rust
  #[test]
  fn warning_with_icon_colored_contains_ansi_when_terminal() {
      // 条件付きでのみ実行（簡易例）
      if !Theme::should_disable_colors() {
          let s = THEME.warning_with_icon("Be careful");
          // ANSIシーケンスが含まれる可能性を簡易検査
          assert!(s.contains("\u{1b}["));
      }
  }
  ```

- 並列実行の注意
  - 環境変数に依存するテストは必ず直列化
  - テスト終了時に必ず NO_COLOR を復元/削除

## Refactoring Plan & Best Practices

- 判定フラグの集中管理
  - Theme 内に color_enabled: LazyLock<bool> 等を導入し、should_disable_colors をキャッシュ（オプトインで再評価可能に）

- API 拡張
  - icons_on/off, ascii_icons オプション追加
  - apply_non_allocating 的な遅延整形（Display実装のラッパ）を提供して大量出力時の性能向上

- 依存ライブラリの単一化
  - console か owo_colors のどちらかに統一し、学習コスト/バイナリサイズ/ANSI実装差異を低減

- 構成ファイル/環境変数サポート拡充
  - NO_COLOR, FORCE_COLOR, COLORTERM などの一般的慣習を明示サポートし、優先順位をドキュメント化

- ユニット/ドキュメントテストの追加
  - 代表ケース（NO_COLORあり/なし、apply の基本動作）のテスト強化
  - README/ドキュメントに使用例を増やす

## Observability (Logging, Metrics, Tracing)

- 本モジュールは表示整形の責務のみであり、ログ/メトリクス/トレース出力は不要
- ただし開発時のデバッグ用途として:
  - 初回に should_disable_colors の結果を一度ログ出力（trace/debug）する設計は有用
  - 実運用では冗長になり得るため標準では無効化

## Risks & Unknowns

- 環境依存
  - Unicode アイコンの表示可否が環境に依存
  - 端末判定は stdout のみ対象で、他ストリーム（stderr）への出力方針は未定義（このチャンクには現れない）

- 並行性と環境変数
  - 実行時に NO_COLOR を切り替えると、出力の一貫性が失われる可能性
  - テスト並列実行時の競合

- ライブラリ混在
  - console と owo_colors の併用起因のメンテ難度（API差異、挙動差）

- 不明点
  - このテーマがどの範囲のUIで使われるか、色のアクセシビリティ要件（コントラスト比など）の非機能要件
  - Windows古環境対応方針（ConPTY以前）やリモート端末（tmux/screen）での挙動ポリシー

【Rust特有の観点】

- 所有権/借用/ライフタイム
  - すべて &self 参照で不変借用。String を新規生成して返すため所有権の衝突なし（行番号:不明）
  - 明示的ライフタイムは不要

- unsafe 境界
  - unsafe ブロックは存在しない（行番号:不明）

- 並行性・非同期
  - THEME は読み取り専用で実質的に Send + Sync 相当と推定（Style の内部実装に依存）。LazyLock により初期化の原子性は保証
  - 非同期/await 境界は登場しない

- エラー設計
  - Result/Option は使用せず、失敗し得る操作がないため妥当
  - panic を誘発する unwrap/expect なし（行番号:不明）