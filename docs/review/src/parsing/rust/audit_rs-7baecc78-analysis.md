# parsing\rust\audit.rs Review

## TL;DR

- 目的: Tree-sitterで抽出したASTノードと、独自のRustParserが「実際に処理したノード」を突き合わせ、シンボル抽出のギャップを可視化するレポートを生成する。
- 公開API: RustParserAudit::audit_file, audit_code, generate_report とデータ契約 AuditError, RustParserAudit。
- 複雑箇所: audit_code での二重解析（tree-sitterとRustParser）およびノード名の同一性前提（木構造のノード名とRustParser側のハンドリング名が一致している必要）。
- 重大リスク: ファイル単位のカバレッジであり「文法全体」ではないため、レポートの「ギャップ」や「未検出」はサンプル依存。RustParserの get_handled_nodes() と tree-sitterノード名の整合性が保証されていない可能性。
- エラー設計: thiserrorによるAuditErrorでI/O, 言語設定, 解析失敗, パーサ作成失敗を明示化。panicはなし。
- 並行性: 非同期・並行処理はなし。スレッド安全性に関わる構造は未使用。
- セキュリティ: 外部入力はファイルパスのみ。コマンド／SQLインジェクションはなし。ログに秘密情報は出力しないが、ファイル読み込みパスのバリデーションは利用側責務。

## Overview & Purpose

本モジュールは、Rustコード片を対象に以下を行う監査機能を提供する。

- tree-sitter（Rust言語）でファイル／文字列を解析し、AST上に現れるノード種類を収集（grammar_nodes）。
- 独自の RustParser で同じコードを解析し、「実際に処理したノード」および「抽出されたシンボル種別」を収集（implemented_nodes, extracted_symbol_kinds）。
- 重要ノード群（function_item, struct_item, enum_item など）について、実装／ギャップ／未発見を判定し、カバレッジレポートをMarkdown文字列として生成。

用途は、シンボル抽出の網羅性を高めるための「監査レポート」。設計上、文法全体ではなく「対象コード片で出現したノード」のみを対象に比較するため、網羅性評価はサンプル依存となる。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | AuditError | pub | 監査処理における失敗要因の分類（I/O, 言語設定, 解析失敗, パーサ作成失敗） | Low |
| Struct | RustParserAudit | pub | ノード／シンボルカバレッジの収集とレポート生成 | Med |
| Method | RustParserAudit::audit_file(&str) -> Result<Self, AuditError> | pub | ファイルを読み、audit_codeで監査を実行 | Low |
| Method | RustParserAudit::audit_code(&str) -> Result<Self, AuditError> | pub | 文字列コードに対する監査（tree-sitterとRustParserの二重解析） | Med |
| Method | RustParserAudit::generate_report(&self) -> String | pub | 監査結果をMarkdownレポートに整形 | Med |
| Fn | discover_nodes(Node, &mut HashMap<String, u16>) | private | ASTをDFSしてノード種類とIDを収集 | Low |
| Mod | tests | private | 簡単なユニットテスト（構造体・メソッドの抽出確認） | Low |

### Dependencies & Interactions

- 内部依存
  - audit_file → audit_code を呼び出す。
  - audit_code → tree_sitter::Parser による解析 → discover_nodes でノード収集 → RustParser::new()/parse()/get_handled_nodes() → SymbolCounter → 集計結果に整形。
  - generate_report → format_utc_timestamp(), self.grammar_nodes/self.implemented_nodes/self.extracted_symbol_kinds を使用してテーブル生成。

- 外部依存（クレート・モジュール）
  | 依存名 | 用途 | 備考 |
  |--------|------|------|
  | tree_sitter | AST解析（汎用） | Node/Parser使用 |
  | tree_sitter_rust | Rust言語のtree-sitter言語定義 | set_languageに渡す |
  | thiserror | エラー型定義（派生） | AuditError |
  | std::collections::{HashMap, HashSet} | 集計構造 | ノード種別と重複排除 |
  | std::fs::read_to_string | ファイル入力 | audit_fileで使用 |
  | crate::io::format::format_utc_timestamp | タイムスタンプ生成 | レポートに記載 |
  | super::RustParser | 独自パーサ | 監査対象（詳細不明） |
  | crate::types::{FileId, SymbolCounter} | パーサ連携 | 詳細不明（このチャンクには現れない） |
  | crate::parsing::NodeTracker | 未使用 | インポートのみ、参照なし |

- 被依存推定
  - CLIや開発ツールから「監査レポート出力」機能として呼ばれる可能性はあるが、具体箇所は不明（このチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| RustParserAudit::audit_file | fn audit_file(file_path: &str) -> Result<Self, AuditError> | ファイルパスからコードを読み込み監査 | O(L + N + P) | O(U + S) |
| RustParserAudit::audit_code | fn audit_code(code: &str) -> Result<Self, AuditError> | 文字列コードを監査（ノード発見＋シンボル抽出） | O(N + P) | O(U + S) |
| RustParserAudit::generate_report | fn generate_report(&self) -> String | カバレッジレポートMarkdown生成 | O(K) | O(K) |

- 記法補足
  - L: ファイルサイズ（バイト数）
  - N: ASTノード数
  - P: 独自RustParserの解析コスト（対象コード依存）
  - U: ユニークなノード種類数
  - S: 抽出シンボル種類数
  - K: レポート対象キーノード数

### 各APIの詳細

1) RustParserAudit::audit_file

- 目的と責務
  - ファイル読み込み（UTF-8テキスト前提）と audit_code 呼び出し。
- アルゴリズム
  1. read_to_string(file_path) でコードを読み込む（失敗は AuditError::FileRead）。
  2. audit_code(&code) を呼び出し、監査結果を返す。
- 引数

  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | file_path | &str | はい | 読み込むファイルのパス |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | Result<RustParserAudit, AuditError> | 成功時は監査結果、失敗時はAuditError |

- 使用例
  ```rust
  use crate::parsing::rust::audit::RustParserAudit;

  fn run_audit_on_file(path: &str) -> String {
      let audit = RustParserAudit::audit_file(path).expect("audit failed");
      audit.generate_report()
  }
  ```
- エッジケース
  - パスが存在しない、権限なし、非UTF-8: AuditError::FileRead。
  - 行番号根拠: 不明（このチャンクには行番号メタデータがない）

- 関連コード抜粋
  ```rust
  pub fn audit_file(file_path: &str) -> Result<Self, AuditError> {
      let code = std::fs::read_to_string(file_path)?;
      Self::audit_code(&code)
  }
  ```

2) RustParserAudit::audit_code

- 目的と責務
  - 入力コードを tree-sitter で解析し、ASTノードを収集。
  - 同コードを独自 RustParser で解析して「処理済みノード」と「抽出シンボル種別」を収集。
  - 監査用の集約構造体（Self）を返却。
- アルゴリズム
  1. Parser::new() でtree-sitterパーサを作成。
  2. tree_sitter_rust::LANGUAGE をセット（失敗は AuditError::LanguageSetup）。
  3. parser.parse(code, None) → Tree を得る（NoneはAuditError::ParseFailure）。
  4. discover_nodes(root_node, &mut grammar_nodes) でASTのDFS収集。
  5. RustParser::new()（失敗は AuditError::ParserCreation）。
  6. FileId(1) を用意し、SymbolCounter::new() を生成。
  7. rust_parser.parse(code, file_id, &mut symbol_counter) → symbols を得る。
  8. symbols の kind を Debug表示文字列化し HashSet に格納（extracted_symbol_kinds）。
  9. rust_parser.get_handled_nodes() の名前集合を HashSet に格納（implemented_nodes）。
  10. RustParserAudit { grammar_nodes, implemented_nodes, extracted_symbol_kinds } を返す。
- 引数

  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | code | &str | はい | 監査対象のRustコード |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | Result<RustParserAudit, AuditError> | 成功時は監査結果、失敗時はAuditError |

- 使用例
  ```rust
  use crate::parsing::rust::audit::RustParserAudit;

  fn audit_snippet(code: &str) {
      let audit = RustParserAudit::audit_code(code).unwrap();
      assert!(audit.extracted_symbol_kinds.contains("Struct"));
      println!("{}", audit.generate_report());
  }
  ```
- エッジケース
  - 空文字列: 解析は成功する可能性が高いが、シンボルは0件。ParseFailureになる条件は稀（tree-sitterがNoneを返すケースは限定的）。
  - 大規模コード: パースとDFS収集が線形に増加。
  - ノード名不一致: RustParserの handled_nodes の名前が tree-sitter の kind 名と一致しないと「ギャップ」と誤判定。
  - 行番号根拠: 不明（このチャンクには行番号メタデータがない）

- 関連コード抜粋（重要部分のみ）
  ```rust
  pub fn audit_code(code: &str) -> Result<Self, AuditError> {
      let mut parser = Parser::new();
      let language = tree_sitter_rust::LANGUAGE.into();
      parser
          .set_language(&language)
          .map_err(|e| AuditError::LanguageSetup(e.to_string()))?;

      let tree = parser.parse(code, None).ok_or(AuditError::ParseFailure)?;
      let mut grammar_nodes = HashMap::new();
      discover_nodes(tree.root_node(), &mut grammar_nodes);

      let mut rust_parser =
          RustParser::new().map_err(|e| AuditError::ParserCreation(e.to_string()))?;
      let file_id = FileId(1);
      let mut symbol_counter = crate::types::SymbolCounter::new();
      let symbols = rust_parser.parse(code, file_id, &mut symbol_counter);

      let mut extracted_symbol_kinds = HashSet::new();
      for symbol in &symbols {
          extracted_symbol_kinds.insert(format!("{:?}", symbol.kind));
      }

      let implemented_nodes: HashSet<String> = rust_parser
          .get_handled_nodes()
          .iter()
          .map(|handled_node| handled_node.name.clone())
          .collect();

      Ok(Self {
          grammar_nodes,
          implemented_nodes,
          extracted_symbol_kinds,
      })
  }
  ```

3) RustParserAudit::generate_report

- 目的と責務
  - 収集済みのカバレッジ情報から、Markdown形式のレポートを組み立てる。
- アルゴリズム
  1. 見出しとタイムスタンプを付与。
  2. 要約（ノード総数、処理済みノード数、シンボル種別数）を記載。
  3. キーとなるノード名のリストに対して、grammar_nodesとimplemented_nodesからステータス判定を行い表にまとめる。
  4. 凡例（implemented/gap/not found）を追記。
  5. 推奨アクション（ギャップ優先、未検出はサンプル追加）を追記。
- 引数

  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &RustParserAudit | はい | 監査結果 |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | String | Markdownドキュメント |

- 使用例
  ```rust
  let audit = RustParserAudit::audit_code("pub fn f() {}").unwrap();
  let md = audit.generate_report();
  println!("{md}");
  ```

- エッジケース
  - キーリストとノード名の不整合がある場合、「❌ not found」や「⚠️ gap」が増える。
  - タイムスタンプ取得に失敗する可能性は低い（format_utc_timestampの実装詳細はこのチャンクには現れない）。
  - 行番号根拠: 不明（このチャンクには行番号メタデータがない）

### Data Contracts

- AuditError（thiserror）
  - バリアント
    - FileRead(std::io::Error)
    - LanguageSetup(String)
    - ParseFailure
    - ParserCreation(String)
  - 特徴
    - Display実装はthiserror導出により整備。エラー伝播は ? および map_err を通じて行われる。

- RustParserAudit
  - フィールド
    - grammar_nodes: HashMap<String, u16>
    - implemented_nodes: HashSet<String>
    - extracted_symbol_kinds: HashSet<String>
  - 意味
    - grammar_nodes: 「対象コードに現れた」ASTノード種別名→kind_id。文法全体の網羅ではない点に注意。
    - implemented_nodes: 独自RustParserが動的に記録した「処理済みノード名」集合。
    - extracted_symbol_kinds: RustParserのシンボル抽出結果のkind（Debug文字列）。

## Walkthrough & Data Flow

- audit_file のデータフロー
  1. 入力: file_path（&str）
  2. read_to_string → code（String）
  3. audit_code(code) を呼び出し
  4. 出力: RustParserAudit or AuditError::FileRead

- audit_code のデータフロー
  1. 入力: code（&str）
  2. tree-sitterを初期化 → set_language(language)
  3. parse(code, None) → Tree or None
  4. root_node から discover_nodes をDFSで呼び出し → grammar_nodes（ユニーク種類をHashMapに登録）
  5. RustParser::new() → rust_parser
  6. FileId(1) と SymbolCounter::new() を準備
  7. rust_parser.parse(code, file_id, &mut symbol_counter) → symbols（Vec<Symbol>を想定、詳細はこのチャンクには現れない）
  8. symbols.kind の Debug文字列を HashSet に格納 → extracted_symbol_kinds
  9. rust_parser.get_handled_nodes() の name を HashSet に格納 → implemented_nodes
  10. RustParserAudit を構築して返却

- generate_report のデータフロー
  1. 入力: RustParserAudit
  2. タイムスタンプ取得 → ヘッダ生成
  3. 集合サイズの要約
  4. key_nodes リストで grammar_nodes と implemented_nodes を突き合わせ
  5. gaps/missing の分類と推奨アクション生成
  6. 出力: Markdown文字列

- discover_nodes のデータフロー
  1. 入力: Node, &mut HashMap<String, u16>
  2. registry.insert(node.kind().to_string(), node.kind_id()) で種別名→IDを登録
  3. 子ノードに対して再帰（DFS）
  4. 出力: registry 更新（種類ベース、カウントは保持しない）

関連コード（discover_nodesは短いので全体引用）:
```rust
fn discover_nodes(node: Node, registry: &mut HashMap<String, u16>) {
    registry.insert(node.kind().to_string(), node.kind_id());

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_nodes(child, registry);
    }
}
```

注記: 行番号の根拠は不明（このチャンクには行番号メタデータがない）ため、関数名での参照に留めている。

## Complexity & Performance

- audit_file
  - 時間: O(L + N + P)
    - L: ファイル読み込み
    - N: ASTノード数に比例したDFS
    - P: 独自RustParserの解析コスト
  - 空間: O(U + S)
    - U: ユニークなノード種類数（HashMap/HashSet）
    - S: 抽出シンボル種類数（HashSet）

- audit_code
  - 時間: O(N + P)
    - tree-sitterの解析は入力サイズに概ね線形、DFS収集もノード数に線形。
  - 空間: O(U + S)

- generate_report
  - 時間: O(K)
    - Kはキーとなるノード数（固定のベクタ長）
  - 空間: O(K)

- ボトルネックとスケール限界
  - 巨大ファイルでは tree-sitter解析とDFSが支配的。
  - 独自RustParserのコスト P は実装依存（このチャンクには詳細がない）。
  - grammar_nodes は種類ベースであり、頻度（件数）を持たない点が分析の粒度を制限。

- 実運用負荷要因
  - I/O: read_to_stringによる一括読み込み。
  - CPU: AST構築とDFS、独自解析。
  - メモリ: ユニーク種類とシンボル種類の集合保持。

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | 解析は成功しうるがシンボル0、レポート生成OK | audit_code | OK |
| 非UTF-8ファイル | バイナリ | FileReadエラー | audit_file | OK |
| パスなし | "not_exists.rs" | FileReadエラー | audit_file | OK |
| 言語設定失敗 | 不正LANGUAGE | LanguageSetup | audit_code | OK |
| 解析失敗 | parser.parseがNone | ParseFailure | audit_code | OK |
| パーサ作成失敗 | RustParser::new失敗 | ParserCreation | audit_code | OK |
| 非対応ノード | "macro_rules"など | gap/not found判定 | generate_report | OK（サンプル依存） |
| 大規模コード | 数万行 | 処理時間増加 | 全体 | 注意 |

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 見当たらない。標準ライブラリと安全なAPIのみ。unsafe未使用。
  - 所有権/借用/ライフタイム: Node/Parserはtree-sitterの安全APIを通じて使用。NodeはTreeのライフタイムに依存するが、関数内で完結しており安全（audit_code, discover_nodes）。

- インジェクション
  - SQL/Command/Path traversal: コードはファイル読み込みのみ。外部コマンド実行やSQLアクセスなし。Path traversalは利用側が与えるパスに依存（本モジュールでは無防備だが一般的な読み取り用途で問題は小さい）。

- 認証・認可
  - 該当なし。このモジュールはローカル解析のみ。

- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: 監査レポートにコード内容は含まず、統計のみ。安全。

- 並行性
  - Race condition / Deadlock: なし。同期原語未使用、グローバル可変状態なし。
  - Send/Sync: 明示的な境界指定なし（このチャンクには現れない）。

- 既知/潜在バグ
  - ノード名の整合性問題: RustParser::get_handled_nodes() の name と tree-sitter の kind 名が一致しない場合、誤った「gap」判定が出る。
  - FileId の固定値: audit_codeで FileId(1) を固定しているため、並行実行や複数ファイル監査では衝突や不適切な識別のリスク。設計上の簡略化と見られる。
  - NodeTracker の未使用インポート: メンテナンス上のノイズ。将来的に意図の不一致を招く可能性。

## Design & Architecture Suggestions

- ノード名の正規化
  - RustParser 側の handled_nodes 名と tree-sitter の kind 名のマッピング表を導入して正規化。誤判定削減に有効。

- FileId の外部注入
  - audit_code に FileId を引数で渡せるようにする、または内部でユニーク採番する。複数ファイル監査の安全性向上。

- カバレッジの構造化
  - generate_report とは別に構造化データ（例: CoverageResult）を返すAPIを追加し、比率や一覧を機械可読に。UI/CI連携が容易に。

- キーリストの設定化
  - key_nodes を外部設定や引数で渡せるようにする。利用プロジェクトの関心に応じた柔軟性。

- ノード頻度の収集
  - grammar_nodes を「種類→ID」に加え「種類→出現回数」の収集も行い、優先度付けの参考に。

- シンボル種別の型化
  - extracted_symbol_kinds を Debug文字列ではなく、明示的な列挙や型へ（変換失敗時の扱いも定義）。将来的な破壊的変更耐性を向上。

- エラー詳細の強化
  - LanguageSetup/ParserCreation の Stringではなく、専用エラー型や原因列挙で表現。デバッグ容易性向上。

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト
  - test_audit_simple_rust: struct/impl/methodの検知とStruct/Methodシンボル抽出を確認。

- 追加推奨ユニットテスト
  1) ファイル読み込み失敗
  ```rust
  #[test]
  fn test_audit_file_read_error() {
      let err = RustParserAudit::audit_file("nonexistent.rs").unwrap_err();
      match err {
          AuditError::FileRead(_) => {},
          _ => panic!("expected FileRead"),
      }
  }
  ```
  2) 空コードの取り扱い
  ```rust
  #[test]
  fn test_audit_empty_code() {
      let audit = RustParserAudit::audit_code("").unwrap();
      assert!(audit.extracted_symbol_kinds.is_empty());
      let report = audit.generate_report();
      assert!(report.contains("Coverage Table"));
  }
  ```
  3) 言語設定失敗（疑似）
  - tree_sitter_rust::LANGUAGE の代替を注入できないため、直接は困難。Parser抽象化でモック可能にするとテスト容易。
  4) ギャップ判定の検証
  ```rust
  #[test]
  fn test_gap_detection() {
      let code = "pub struct S;"; // struct_item のみ
      let audit = RustParserAudit::audit_code(code).unwrap();
      let report = audit.generate_report();
      // 他のキーは not found または gap
      assert!(report.contains("struct_item"));
      assert!(report.contains("Coverage Table"));
  }
  ```
  5) 大規模入力の健全性
  - ベンチマーク/パフォーマンステストでタイムアウトしないことを確認（criterion等、別クレートが必要）。

- 統合テスト
  - 複数ファイル監査シナリオ（現状のFileId固定により要改善）。
  - RustParser のシンボル抽出種別増加/減少への追従（extracted_symbol_kindsの整合性）。

## Refactoring Plan & Best Practices

- API拡張
  - audit_code に FileId を引数化または内部でユニーク生成。
  - key_nodes を引数や設定で受け取り可能に。

- コード健全化
  - 未使用インポート NodeTracker の削除。
  - grammar_nodes を BTreeMap にしてレポートで安定したソートを提供（可読性向上）。

- 型の明確化
  - extracted_symbol_kinds は Debug文字列依存を排し、SymbolKind（列挙型など）を使う。必要なら ToString 実装で出力。

- エラーハンドリングの充実
  - ParserCreation/LanguageSetup の String ではなく、原因列挙（例: UnsupportedVersion, InvalidGrammar）を定義。

- テストの強化
  - 例示コードの充実（macro_rules, use_declaration 等）による「not found」削減。
  - モック可能な設計（Parser抽象化）で失敗パスの網羅。

## Observability (Logging, Metrics, Tracing)

- 現状
  - ログ・メトリクス・トレースはなし。レポートにタイムスタンプのみ。

- 推奨
  - ログ: key_nodes のステータス集計、gap/missing の件数をINFOで記録。
  - メトリクス: カバレッジ比率（implemented / present）、シンボル抽出件数。
  - トレース: audit_code の各ステップ（tree-sitter解析、RustParser解析、DFS収集）の所要時間を計測し、性能ボトルネック特定に活用。

## Risks & Unknowns

- RustParser の詳細不明
  - parse の戻り型、SymbolCounter の意味、get_handled_nodes の名寄せロジックは不明（このチャンクには現れない）。整合性前提が崩れるとレポート品質に影響。

- ノード名の差異
  - tree-sitterのノード名は言語定義に依存。key_nodes リストが最新の定義とずれている可能性（例: macro_rules の実ノード名差）。「not found」誤判定を招く。

- 単一ファイル性
  - 文法全体ではなく単一コード片に限定したカバレッジ推定。包括的な評価にはサンプル拡充が不可欠。

- FileId 固定値
  - 将来的な複数ファイル監査や並列処理での衝突可能性。現状は単体監査前提と見られる。

- タイムスタンプ生成の外部依存
  - format_utc_timestamp の挙動（失敗可能性、フォーマット）は不明（このチャンクには現れない）。レポート整形への影響は軽微。