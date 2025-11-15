# audit.rs Review

## TL;DR

- 目的: tree-sitter-gdscriptで得られるAST全体と、社内GDScriptパーサが実際に扱っているノード種別・抽出シンボルを比較し、抽出対象のギャップを可視化する。
- 公開API: `GdscriptParserAudit::audit_file`, `GdscriptParserAudit::audit_code`, `GdscriptParserAudit::generate_report`, 型`GdscriptParserAudit`, 例外型`AuditError`。
- コアロジック: `audit_code`でtree-sitterを用いた全ノード走査と自前パーサによるシンボル抽出を二段で実行し、結果を差分集計。
- 複雑箇所: AST全走査（再帰）と自前パーサの抽出結果のマージ、レポート生成時のギャップ判定。
- 重大リスク: `FileId::new(1).unwrap`によるパニック可能性、`symbol.kind`の`Debug`文字列依存による不安定なデータ契約、カバレッジ指標が「発見有無のみ」で頻度がわからない。
- Rust安全性/エラー/並行性: `unsafe`未使用、エラーは`thiserror`で明示、並行性なし。I/O/パースエラーは戻り値で伝搬するが`unwrap`が一点的リスク。
- テスト: 基本的なハッピーパスの単体テストあり。エラーパス・大規模入力・欠落ノードのテスト追加推奨。

## Overview & Purpose

このモジュールは、GDScriptのASTをtree-sitterで構築し、プロダクション用の`GdscriptParser`がどのノードを扱い、どのシンボル種別を抽出できているかを監査するためのユーティリティを提供する。目的は以下の通り。

- grammar（tree-sitter-gdscript）のノード種別と、実際に抽出対象として実装済みのノード種別の差を可視化。
- 抽出結果（シンボル種別）の把握と、IDE機能に必要なシンボル抽出のギャップの特定。
- CIやドキュメント向けにMarkdownレポートを生成して、カバレッジ指標を共有可能にする。

主要フローは「コード文字列のパース→ASTノード列挙→自前パーサで抽出→レポート生成」である。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | AuditError | pub | 監査処理におけるI/O/パース/初期化エラーの分類 | Low |
| Struct | GdscriptParserAudit | pub | 監査結果（grammarノード、実装済みノード、抽出シンボル種別）を保持 | Med |
| Impl Method | GdscriptParserAudit::audit_file | pub | ファイルパスからコードを読み込み監査を実行 | Med |
| Impl Method | GdscriptParserAudit::audit_code | pub | コード文字列から直接監査を実行 | Med |
| Impl Method | GdscriptParserAudit::generate_report | pub | カバレッジのMarkdownレポート生成 | Low |
| Function | discover_nodes | private | ASTを再帰走査し、ノード種別とkind_idを収集 | Low |
| Module | tests | private | 単体テスト（簡易ケース） | Low |

### Dependencies & Interactions

- 内部依存
  - `GdscriptParserAudit::audit_code` → `discover_nodes`（AST全走査）
  - `GdscriptParserAudit::audit_code` → `GdscriptParser::new`, `GdscriptParser::parse`, `GdscriptParser::get_handled_nodes`（プロダクションパーサ呼び出し）
  - `GdscriptParserAudit::generate_report` → `format_utc_timestamp`（レポートのタイムスタンプ）
- 外部依存（主要）
  | クレート/モジュール | 用途 |
  |--------------------|------|
  | thiserror::Error | エラー型定義（`AuditError`） |
  | tree_sitter::{Node, Parser} | AST構築とノード走査 |
  | tree_sitter_gdscript::LANGUAGE | GDScript言語定義の設定 |
  | std::collections::{HashMap, HashSet} | 集計データ構造 |
  | crate::types::{FileId, SymbolCounter} | パーサ用のファイルID、シンボルカウンタ |
  | crate::io::format::format_utc_timestamp | レポートの生成日時 |
  | super::GdscriptParser | プロダクションのGDScriptパーサ |
- 被依存推定
  - CIジョブやドキュメント生成タスクでのカバレッジ報告。
  - パーサ開発時の回帰チェックスクリプト。
  - IDE機能（シンボル抽出）の対応範囲確認用ツール。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| AuditError | enum AuditError | 監査時の各種失敗種別 | - | - |
| GdscriptParserAudit | struct GdscriptParserAudit { grammar_nodes: HashMap<String, u16>, implemented_nodes: HashSet<String>, extracted_symbol_kinds: HashSet<String> } | 監査結果保持 | - | O(U + V + W) |
| audit_file | pub fn audit_file(path: &str) -> Result<Self, AuditError> | ファイルパスから監査 | O(N + P) | O(U + V + W) |
| audit_code | pub fn audit_code(code: &str) -> Result<Self, AuditError> | コード文字列から監査 | O(N + P) | O(U + V + W) |
| generate_report | pub fn generate_report(&self) -> String | Markdownレポート生成 | O(K + U) | O(K + U) |

- 記号:
  - N: ASTノード数（tree-sitterパース後のノード総数）
  - P: プロダクションパーサの処理量（コード長・構造に依存）
  - U: 発見されたgrammarノード種別数
  - V: 実装済みノード種別数
  - W: 抽出シンボル種別数
  - K: キーとなるノード配列の要素数（固定）

### 各APIの詳細

1) GdscriptParserAudit::audit_file
- 目的と責務
  - ファイルを読み込み、中身を`audit_code`に渡して監査を実施する。
- アルゴリズム（ステップ）
  1. `std::fs::read_to_string(path)`でコードを読み込む。
  2. `audit_code(&code)`を呼び出す。
  3. 結果を返す。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | path | &str | 読み込むGDScriptファイルのパス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Result<GdscriptParserAudit, AuditError> | 成功時は監査結果、失敗時はエラー |
- 使用例
  ```rust
  use parsing::gdscript::audit::GdscriptParserAudit;

  let audit = GdscriptParserAudit::audit_file("examples/player.gd")
      .expect("audit should succeed");
  println!("{}", audit.generate_report());
  ```
- エッジケース
  - ファイルが存在しない/権限がない場合は`AuditError::FileRead`。
  - テキストでないバイナリや巨大ファイルでも読み込みは試みるが、`audit_code`でのパース失敗があり得る。

2) GdscriptParserAudit::audit_code
- 目的と責務
  - コード文字列をtree-sitterでパースして全ノード種別を列挙し、その後プロダクションパーサで抽出シンボルと実装済みノード種別を収集する。
- アルゴリズム（ステップ）
  1. `Parser::new()`でtree-sitterパーサ生成。
  2. `set_language(&tree_sitter_gdscript::LANGUAGE.into())`で言語設定（失敗時`AuditError::LanguageSetup`）。
  3. `parser.parse(code, None)`でパース（`None`は旧ツリー未使用）。失敗時`AuditError::ParseFailure`。
  4. `discover_nodes(tree.root_node(), &mut grammar_nodes)`で再帰的にノード種別（`kind()`）と`kind_id()`を収集。
  5. `GdscriptParser::new()`で自前パーサ生成（失敗時`AuditError::ParserCreation`）。
  6. `SymbolCounter::new()`と`FileId::new(1).unwrap`を用意して`gd_parser.parse(code, file_id, &mut counter)`を実行。
  7. 得られた`symbols`から`symbol.kind`の`Debug`文字列を`extracted_symbol_kinds`に格納。
  8. `gd_parser.get_handled_nodes()`から各`handled.name`を`implemented_nodes`へ格納。
  9. `GdscriptParserAudit`を構築して返す。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | code | &str | 監査対象のGDScriptソースコード文字列 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Result<GdscriptParserAudit, AuditError> | 成功時は監査結果、失敗時はエラー |
- 使用例
  ```rust
  let code = r#"
  class_name Player
  func move():
      pass
  "#;

  let audit = GdscriptParserAudit::audit_code(code)?;
  assert!(audit.grammar_nodes.contains_key("function_definition"));
  ```
- エッジケース
  - 空文字列: tree-sitterはパース木を返しうるが、抽出できるノード/シンボルはゼロに近い。
  - 非GDScript構文: `ParseFailure`となる可能性（tree-sitterが木を返せないケース）。
  - 非常に深い/大きなコード: 再帰走査のコスト増、スタック枯渇リスクは低いが注意。
  - `FileId::new(1).unwrap`がパニックする可能性（`audit_code`内部の一点的危険箇所）。
  - `symbol.kind`の`Debug`出力への依存により、文字列表現が将来変更されると後方互換性が壊れる可能性。

3) GdscriptParserAudit::generate_report
- 目的と責務
  - 監査結果から人間可読なMarkdownレポートを生成する。
- アルゴリズム（ステップ）
  1. 見出しと生成日時を付与。
  2. ノード総数・実装済みノード総数・抽出シンボル種別数を要約。
  3. 固定配列`key_nodes`（代表的ノード種別）に対し、発見/未発見/実装済み判定を行いテーブル化。
  4. 凡例と推奨対応（ギャップ/未サンプル）を出力。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | self | &GdscriptParserAudit | 監査結果 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | String | Markdown形式のレポート文字列 |
- 使用例
  ```rust
  let report = audit.generate_report();
  println!("{}", report);
  ```
- エッジケース
  - `key_nodes`に含まれないノードはテーブルに出ない（総数には含まれる）。
  - 文字列連結のため非常に大きな`grammar_nodes`でも生成可能だが出力サイズ増。

データ契約上の注意点
- `GdscriptParserAudit.grammar_nodes`: ノード種別名→kind_id（u16）を保持。出現回数は保持しない。
- `extracted_symbol_kinds`: `symbol.kind`の`Debug`文字列表現を保持。安定APIではない可能性あり（将来的な`Debug`出力変更に弱い）。
- `implemented_nodes`: `gd_parser.get_handled_nodes()`の`name`を保持。ここも文字列契約。

## Walkthrough & Data Flow

- 入力
  - `audit_file(path)`の場合: ファイルパスを受け取り、ファイル内容を文字列化。
  - `audit_code(code)`の場合: 文字列を直接受け取る。
- 処理フロー
  1. tree-sitterで`code`をパースして`Tree`を生成。
  2. `discover_nodes(root, &mut grammar_nodes)`で再帰的に`Node.kind()`と`Node.kind_id()`を収集。
     ```rust
     fn discover_nodes(node: Node, registry: &mut HashMap<String, u16>) {
         registry.insert(node.kind().to_string(), node.kind_id());
         let mut cursor = node.walk();
         for child in node.children(&mut cursor) {
             discover_nodes(child, registry);
         }
     }
     ```
  3. `GdscriptParser::new()`で自前パーサを用意し、`SymbolCounter`と`FileId`を渡して`parse`を呼ぶ。
  4. `symbols`から`symbol.kind`を`Debug`で`String`化し`extracted_symbol_kinds`へ。
  5. `gd_parser.get_handled_nodes()`の`name`を`implemented_nodes`へ。
  6. 以上を`GdscriptParserAudit`にまとめて返却。
- 出力
  - `GdscriptParserAudit`インスタンス（集計済みセット/マップ）
  - `generate_report()`で人間可読なMarkdownレポート

根拠（関数名:行番号）
- `audit_code`でのtree-sitterセットアップと`ParseFailure`発生箇所（`parser.set_language`→`AuditError::LanguageSetup`、`parser.parse(..).ok_or(AuditError::ParseFailure)?`）。関数`audit_code`内。
- `audit_code`での`FileId::new(1).unwrap`（パニック可能箇所）。関数`audit_code`内。
- `discover_nodes`での再帰走査と`HashMap`登録。関数`discover_nodes`内。
- `generate_report`での`key_nodes`差分判定。関数`generate_report`内。

（行番号はこのファイルのチャンク内の該当関数ブロックに存在し、コード断片は上記に引用）

## Complexity & Performance

- 時間計算量
  - `discover_nodes`: ASTノード数Nに対してO(N)。
  - tree-sitterのパース: 入力コード長Mに対して概ね線形〜線形ith（tree-sitterの特性に依存）。
  - 自前パーサ: コード構造に依存（仮にO(P)）。
  - `generate_report`: 固定配列`key_nodes`と`grammar_nodes`→O(K + U)。
- 空間計算量
  - `grammar_nodes`: ノード種別数Uに対してO(U)。
  - `implemented_nodes`: 種別数Vに対してO(V)。
  - `extracted_symbol_kinds`: 種別数Wに対してO(W)。
- ボトルネック
  - 非常に大きなGDScriptファイルに対してAST全走査の再帰コスト。
  - レポート文字列の連結（大量のノードでも耐えるがサイズ増）。
- スケール限界
  - 再帰実装のため極端に深いASTでのスタック消費。一般的なGDScriptでは現実的な問題になりにくい。
  - シンボル抽出側（自前パーサ）の計算量・メモリ使用量が主な支配要因になりうる。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - `unsafe`未使用。標準`HashMap/HashSet`と所有権/借用は健全。
  - 再帰`discover_nodes`はRustのスタック管理下で安全。
  - `Node.kind_id()`を`u16`で保持しており整数オーバーフローの懸念なし。
- インジェクション
  - SQL/Command/Path traversalは実装に存在しない。ファイル読み込みのみ。
- 認証・認可
  - 対象外。
- 秘密情報
  - ハードコード秘密情報なし。ログ出力も本ファイルでは無し。
- 並行性
  - 並行処理なし。`tree_sitter::Parser`や内部状態はローカルに生成・使用。

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | パースは失敗または空AST。監査結果は空ベースで生成 | `audit_code`の`parser.parse(..)` | OK（`ParseFailure`でErrの可能性） |
| 不正ファイルパス | "not_exists.gd" | `AuditError::FileRead` | `audit_file`で`read_to_string` | OK |
| 非GDScript構文 | "{" | `AuditError::ParseFailure` | `audit_code` | OK |
| 巨大ファイル | 数MB〜数十MB | 時間・メモリ増大だが動作継続 | 再帰走査＋文字列生成 | OK（性能注意） |
| `FileId::new(1).unwrap`失敗 | FileIdが不正 | パニックせずエラーにすべき | `audit_code` | 問題（unwrapによるpanic可能性） |
| `symbol.kind`表現の変化 | Debug出力が変更 | 既存レポート/テストが壊れる | `audit_code` | 潜在リスク |
| 出現回数が不明 | 同一kind多数 | カバレッジの頻度が見えない | `discover_nodes` | 仕様（改善余地） |

重要な主張の根拠
- `audit_code`内で`FileId::new(1).unwrap`を直接呼ぶため、`FileId::new`が`Option`/`Result`を返している場合に失敗時panicする可能性がある（関数`audit_code`の中段）。
- `symbol.kind`を`format!("{:?}", symbol.kind)`で文字列化しているため、`Debug`出力への依存がある（`audit_code`後段のループ）。

## Design & Architecture Suggestions

- `FileId::new(1).unwrap`の排除
  - 監査APIに`file_id`を引数追加、または`FileId::new(1)`の失敗を`AuditError`に変換して戻り値で返す。
- `symbol.kind`の型安全化
  - `HashSet<SymbolKind>`のように列挙型（型）を保持し、表示時にフォーマッタで文字列化。`Debug`文字列への依存を排除。
- ノード出現頻度の収集
  - `grammar_nodes: HashMap<String, u16>`ではkind_idのみ。別途`HashMap<String, (kind_id, count)>`や新構造体で頻度も蓄積すると、カバレッジの重み付けが可能。
- `key_nodes`の外部設定化
  - 構成ファイルや引数で拡張可能にし、言語仕様変更に追随しやすくする。
- レポートの詳細化
  - 実装済みノードと未実装ノードの一覧に加え、抽出シンボル種別ごとの発生件数・サンプル位置なども含める。
- エラー設計の一貫性
  - `LanguageSetup`や`ParserCreation`を含む初期化失敗に、原因型（元エラー）を`source`としてラップする改善（現在はString化）。

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト
  - `test_audit_simple_gdscript`: 基本的なクラス・メソッド・関数定義を含むコードで監査し、ノード発見とシンボル抽出、レポート生成のハッピーパスを確認。
- 追加推奨テスト
  - エラーパス
    - ファイル読み込み失敗（`audit_file`→`AuditError::FileRead`）。
    - 言語設定失敗をシミュレート（難しいがモック化/DIができるなら）。
    - パース失敗（無効構文で`AuditError::ParseFailure`）。
    - `FileId::new(..)`失敗を想定するテスト（パニック防止の設計変更後）。
  - カバレッジ検証
    - `key_nodes`に含まれる複数ノードを網羅するフィクスチャで、`gap`と`implemented`の両方が出ることを確認。
    - 未サンプルノードが`not found`としてレポートされること。
  - 大規模入力
    - 数千行のGDScriptで性能退行がないことを確認（ベンチに近いが簡易テストでも有用）。
- 使用例（ユニットテスト相当）
  ```rust
  #[test]
  fn audit_handles_empty_code() {
      let audit = GdscriptParserAudit::audit_code("").unwrap();
      assert!(audit.extracted_symbol_kinds.is_empty());
      let report = audit.generate_report();
      assert!(report.contains("Coverage Table"));
  }

  #[test]
  fn audit_reports_gaps() {
      let code = r#"
      class_name Player
      func foo(): pass
      "#;
      let audit = GdscriptParserAudit::audit_code(code).unwrap();
      let report = audit.generate_report();
      // 任意のkey_nodesがgap/not foundになることを確認（環境に応じて文字列判定）
      assert!(report.contains("gap") || report.contains("not found"));
  }
  ```

## Refactoring Plan & Best Practices

- 例外安全
  - `FileId::new(1).unwrap`をやめ、`?`で`AuditError`へ変換する（必要に応じて新Variantを追加）。
- データ構造改善
  - `grammar_nodes`を`HashMap<String, GrammarStats { kind_id: u16, count: u32 }>`などに変更。
  - `extracted_symbol_kinds`は型安全な列挙型に。
- 責務分離
  - レポート生成を専用モジュールへ分離し、表示ロジックと収集ロジックを独立。
- 拡張性
  - `key_nodes`を外部設定（環境変数/設定ファイル/引数）にし、CIで異なるプロファイルを用意。
- 可読性
  - `generate_report`でテーブル生成を小関数化（ヘッダ生成、行生成、凡例生成）。
- ベンチ/プロファイル
  - 大規模ファイルでの再帰走査の性能測定を追加し、必要ならイテレータベースに変更。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 監査開始/終了、パース成功/失敗、ノード総数、抽出シンボル総数を`tracing`で`info!`/`warn!`出力。
- メトリクス
  - 監査対象ごとの
    - ASTノード総数（`grammar_nodes.len()`）
    - 実装済みノード数（`implemented_nodes.len()`）
    - 抽出シンボル種別数（`extracted_symbol_kinds.len()`）
  - 収集し、Prometheus等にエクスポート可能な形で計測。
- トレーシング
  - `audit_code`に`#[instrument]`（code長やファイル名タグ）を付与し、失敗時のスパンで原因追跡可能に。
- レポーティング
  - CIで`generate_report`結果をアーティファクトとして保存し、履歴を可視化。

## Risks & Unknowns

- `FileId::new(1).unwrap`の前提
  - `FileId::new`の仕様が不明（このチャンクには現れない）。`unwrap`はパニックの可能性があり、監査ツールとしては不適切。
- `symbol.kind`の`Debug`文字列
  - 実際の`SymbolKind`型の定義や`Debug`出力の安定性は不明（このチャンクには現れない）。データ契約として脆弱。
- `GdscriptParser::get_handled_nodes`の戻り値仕様
  - `handled.name`が何に準拠しているか（grammarのkind名との整合性など）は不明（このチャンクには現れない）。
- `LanguageParser`トレイトの関与
  - `use crate::parsing::parser::LanguageParser;`があるがこのモジュール内で使用されていない。設計上の役割は不明（このチャンクには現れない）。
- tree-sitterバージョン差異
  - `kind_id`や`kind()`の安定性はtree-sitterの言語定義に依存し、将来変更時の影響範囲は不明。