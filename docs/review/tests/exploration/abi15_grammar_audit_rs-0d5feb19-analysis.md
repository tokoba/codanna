# exploration\abi15_grammar_audit.rs Review

## TL;DR

- 目的: 多言語（PHP/Go/Python/Rust/TypeScript/C/C++/GDScript/C#/Kotlin）のTree-sitter文法ノードを例示コードから探索し、Parser実装（codannaの各ParserAudit）との対応を比較し、レポートを自動生成するテスト群。
- 主要API（内部テスト関数）: comprehensive_<lang>_analysis、generate_<lang>_node_discovery、generate_<lang>_tree_structure、discover_nodes_with_ids、generate_tree_structure、collect_node_statistics。
- コアロジック: AST全走査でノード種別とkind_id収集→カテゴリ別整形→Audit結果と比較→複数ファイルに出力。
- 複雑箇所: 多言語に対する共通パターンの重複実装、ファイルI/Oの並列テストによる競合、文法JSON未整備時の挙動（GDScriptのみ特別扱い）。
- 重大リスク: テストのデフォルト並列実行で同一出力ファイルに同時書き込み、grammar JSONや出力ディレクトリ欠如時のpanic（expect/unwrap）によるテスト失敗、coverageで分母0時のNaN/inf表示。
- Rust安全性: unsafe未使用・所有権/借用は問題なし。Node.byte_rangeの文字境界非整合に対してOptionを安全に扱うが、parse(None)のNoneに対するunwrapが一部に存在。
- 改善提案: 共通処理の抽象化（ジェネリック/トレイト）、テストの直列化/一時ディレクトリ化、失敗時のResult返却とエラー分類、観測性向上（構造化ログ/メトリクス）。

## Overview & Purpose

このファイルは「Grammar Audit and Node Discovery Test」という目的で、Tree-sitterの各言語の文法ノード（named nodes）を以下の三視点から分析し、成果物をレポートとして出力するためのテストスイートです。

- Grammar JSON analysis: node-types.json（Tree-sitter生成物）から全namedノードを取得。
- Node discovery: 例示コード（examples/<lang>/comprehensive.*）をTree-sitterでパースし、実際に出現したノード種別とkind_idを列挙。
- Parser audit: codannaの各言語ParserAudit（例: PhpParserAudit）で、実装済みノードや抽出できたシンボル種別などを取得し、前二者と比較レポート。

成果物は contributing/parsers/<lang>/ 以下に複数ファイル（AUDIT_REPORT.md、GRAMMAR_ANALYSIS.md、node_discovery.txt、TREE_STRUCT.md）として保存されます。テストは cargo test abi15_grammar_audit -- --nocapture で実行することを想定しています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | tests | private | 全テスト関数と補助関数群の定義 | Med |
| Function | comprehensive_<lang>_analysis | private (test) | 文法JSONの読込/比較、ParserAudit実行、分析レポート生成/書出し | Med |
| Function | generate_<lang>_node_discovery | private (test/helper) | 例示コードからAST走査し、ノード種別→kind_idのマッピング文字列生成 | Med |
| Function | generate_<lang>_tree_structure | private (test) | ASTのツリー構造とノード統計（件数/最大深さ）をMarkdownで出力 | Med |
| Function | discover_nodes_with_ids | private (helper) | Node全走査で registry(HashMap<String,u16>) と found(HashSet<String>) を埋める | Low |
| Function | generate_tree_structure | private (helper) | ASTノードの階層出力（depth制限、フィールド名表示、プレビュー） | Med |
| Function | collect_node_statistics | private (helper) | ノード種別ごとの出現件数と最大深さを計測 | Low |
| Module | abi15_exploration_common | private (submodule) | print_node_treeユーティリティ（外部表示用） | 不明 |

### Dependencies & Interactions

- 内部依存:
  - comprehensive_* → generate_*_node_discovery（間接的に）→ discover_nodes_with_ids, print_node_tree
  - generate_*_tree_structure → generate_tree_structure, collect_node_statistics
  - ほぼ全関数で std::fs, HashMap/HashSet, tree_sitter::{Language, Parser, Node} 使用
- 外部依存（主要）:

| クレート/モジュール | 用途 |
|---------------------|------|
| tree_sitter_*（各言語） | 言語定義（Language）、AST（Node）、パーサ（Parser） |
| codanna::parsing::<lang>::audit::* | ParserAudit（audit_file, generate_report, sets） |
| serde_json::Value | node-types.jsonの解析 |
| std::fs | ファイル読書/書出し |
| std::collections::{HashMap, HashSet} | ノード登録/集合演算 |

- 被依存推定:
  - 本モジュール自体はテスト用途。成果物（出力Markdown/テキスト）はドキュメンテーション/品質監査の下流工程で参照される可能性。

## API Surface (Public/Exported) and Data Contracts

公開APIはありません（テストモジュール内のprivate関数のみ）。以下は主要な内部テストAPIの一覧です。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| comprehensive_php_analysis | fn comprehensive_php_analysis() | PHPのGrammar/Audit比較分析と各種レポート出力 | O(G + E + I log I) | O(G + E + I) |
| comprehensive_go_analysis | fn comprehensive_go_analysis() | Go版 | O(G + E + I log I) | O(G + E + I) |
| comprehensive_python_analysis | fn comprehensive_python_analysis() | Python版 | O(G + E + I log I) | O(G + E + I) |
| comprehensive_rust_analysis | fn comprehensive_rust_analysis() | Rust版 | O(G + E + I log I) | O(G + E + I) |
| comprehensive_typescript_analysis | fn comprehensive_typescript_analysis() | TS版 | O(G + E + I log I) | O(G + E + I) |
| comprehensive_c_analysis | fn comprehensive_c_analysis() | C版 | O(G + E + I log I) | O(G + E + I) |
| comprehensive_cpp_analysis | fn comprehensive_cpp_analysis() | C++版 | O(G + E + I log I) | O(G + E + I) |
| comprehensive_gdscript_analysis | fn comprehensive_gdscript_analysis() | GDScript版（grammar警告分岐あり） | O(G + E + I log I) | O(G + E + I) |
| comprehensive_csharp_analysis | fn comprehensive_csharp_analysis() | C#版 | O(G + E + I log I) | O(G + E + I) |
| comprehensive_kotlin_analysis | fn comprehensive_kotlin_analysis() | Kotlin版 | O(G + E + I log I) | O(G + E + I) |
| generate_<lang>_node_discovery | fn generate_<lang>_node_discovery() -> String | ASTからkind→idマッピングをカテゴリ整理して文字列生成 | O(N + K log K) | O(K) |
| generate_<lang>_tree_structure | fn generate_<lang>_tree_structure() | ASTツリー描画と統計のMarkdown出力 | O(N) | O(U) |
| discover_nodes_with_ids | fn discover_nodes_with_ids(node: Node, registry: &mut HashMap<String, u16>, found: &mut HashSet<String>) | AST全走査でレジストリ/集合を埋める | O(N) | O(K) |
| generate_tree_structure | fn generate_tree_structure(out: &mut String, node: Node, code: &str, depth: usize, field_name: Option<&str>) | 階層出力（深さ制限、フィールド名、プレビュー有） | O(N) | O(1) 追加 |
| collect_node_statistics | fn collect_node_statistics(node: Node, stats: &mut HashMap<String, (usize, usize)>) | 件数と最大深さ測定 | O(N) | O(U) |

- ここで G = Grammarノード数（JSON named）、E = 例示コードで発見されたノード種別数、I = 実装済みノード数（ParserAudit）、N = ASTノード総数、K = ユニークなノード種別数、U = ユニーク種別数。

以下、主要APIの詳細。

1) comprehensive_<lang>_analysis（各言語共通パターン）
- 目的と責務
  - grammar JSONのnamedノード集合を抽出
  - ParserAudit::audit_fileで例示コードを監査（implemented_nodes, extracted_symbol_kinds, grammar_nodes）
  - 例示で出現したノード、実装済みノード、grammarにあるが例示に無いノードを比較
  - AUDIT_REPORT.md, GRAMMAR_ANALYSIS.md, node_discovery.txt を保存（node_discoveryは generate_<lang>_node_discovery で生成）
- アルゴリズム（代表: Rust版）
  1. fs::read_to_stringで node-types.json 読込 → serde_json::from_str → Value::Arrayから named==true の type を HashSet に格納
  2. RustParserAudit::audit_file("examples/rust/comprehensive.rs") → Okならaudit、Errなら空のAuditにフォールバック
  3. audit.grammar_nodes のkeysを例示ノード集合に
  4. audit.generate_report() を AUDIT_REPORT.md に書出し
  5. 比較集合 in_grammar_only, in_example_not_handled, handled_well を生成/ソート
  6. GRAMMAR_ANALYSIS.md に統計と各集合の一覧を出力
  7. generate_rust_node_discovery() を node_discovery.txt に書出し
  8. printlnでサマリ出力（coverage計算あり）
- 引数
  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | なし | - | - | テスト関数。固定パスから読込/出力 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | なし | ファイルへ副作用。標準出力へ情報表示 |
- 使用例（該当テスト内部呼び出し）
  ```rust
  // comprehensive_rust_analysis（行番号不明）
  let grammar_json = fs::read_to_string("contributing/parsers/rust/node-types.json")
      .expect("Failed to read Rust grammar file");
  let audit = match RustParserAudit::audit_file("examples/rust/comprehensive.rs") {
      Ok(audit) => audit,
      Err(e) => {
          println!("Warning: Failed to audit Rust file: {e}");
          RustParserAudit { /* ... */ }
      }
  };
  fs::write("contributing/parsers/rust/AUDIT_REPORT.md", audit.generate_report())?;
  ```
- エッジケース
  - grammar JSONが存在しない/構造が異なる
  - audit_fileが失敗（パース不可/ファイルなし）
  - 例示でのノード数が0（coverage除算）
  - 出力ディレクトリ未作成（fs::writeの失敗）

2) generate_<lang>_node_discovery
- 目的と責務
  - 指定言語の例示コードをTree-sitterでパースし、ノード種別→kind_idを収集、カテゴリに沿ってわかりやすく整形したテキストを返す
- アルゴリズム（代表: PHP版）
  1. Languageを取得しABIバージョンを記録
  2. Parserに言語設定、例示コード読込（失敗時フォールバックの最小コード）
  3. parser.parse → root_node
  4. DEBUG_TREE環境変数がセットならprint_node_treeでASTダンプ
  5. discover_nodes_with_idsで HashMap<kind, id>, HashSet<found> を埋める
  6. カテゴリ定義に従って「✓/○/✗」で出力整形し、未分類ノードも列挙
- 引数/戻り値
  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | なし | - | - | 固定ファイルを対象とし、整形済み文字列を返す |
  | 戻り値 | String | - | 整形済み出力（呼び出し側でfs::write） |
- 使用例
  ```rust
  // generate_php_node_discovery（行番号不明）
  let mut parser = Parser::new();
  let language: Language = tree_sitter_php::LANGUAGE_PHP.into();
  parser.set_language(&language).unwrap();
  let code = fs::read_to_string("examples/php/comprehensive.php")
            .unwrap_or_else(|_| "<?php\nclass Example {}\n".to_string());
  let tree = parser.parse(&code, None).unwrap();
  let root = tree.root_node();
  discover_nodes_with_ids(root, &mut node_registry, &mut found_in_file);
  ```
- エッジケース
  - parseがNone（unwrapでpanic）
  - 例示コード読込失敗時のフォールバックコードが文法カバレッジ不足
  - カテゴリ定義に存在しないノード名（✗として表示）

3) discover_nodes_with_ids（短いので全引用）

```rust
fn discover_nodes_with_ids(
    node: Node,
    registry: &mut HashMap<String, u16>,
    found_in_file: &mut HashSet<String>,
) {
    let node_kind = node.kind();
    registry.insert(node_kind.to_string(), node.kind_id());
    found_in_file.insert(node_kind.to_string());

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_nodes_with_ids(child, registry, found_in_file);
    }
}
```

- 目的と責務: AST全走査して kind→kind_id を登録し、出現集合に追加
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | node | Node | 起点ノード |
  | registry | &mut HashMap<String, u16> | kind→id レジストリ |
  | found_in_file | &mut HashSet<String> | 実際に出現したノード種別集合 |
- 戻り値: なし（副作用）
- エッジケース
  - 非常に深い木→再帰が深くなる（他所でdepth制限はgenerate_tree_structureのみ）

4) generate_tree_structure（重要部分抜粋）

```rust
fn generate_tree_structure(
    output: &mut String,
    node: Node,
    code: &str,
    depth: usize,
    field_name: Option<&str>,
) {
    if depth > 50 {
        output.push_str(&format!(
            "{:indent$}... (truncated at depth 50)\n",
            "",
            indent = depth * 2
        ));
        return;
    }

    let node_text = code.get(node.byte_range()).unwrap_or("<invalid>");
    let display_text = node_text.lines().next().unwrap_or("")
        .chars().take(80).collect::<String>();
    let field_prefix = if let Some(fname) = field_name {
        format!("{fname}: ")
    } else { String::new() };

    output.push_str(&format!(
        "{:indent$}{}{} [{}]",
        "", field_prefix, node.kind(), node.kind_id(), indent = depth * 2
    ));
    if node.child_count() == 0 || display_text.len() <= 40 {
        output.push_str(&format!(" = '{}'", display_text.replace('\n', "\\n")));
    }
    output.push('\n');

    let mut cursor = node.walk();
    for (i, child) in node.children(&mut cursor).enumerate() {
        let child_field = node.field_name_for_child(i as u32);
        generate_tree_structure(output, child, code, depth + 1, child_field);
    }
}
```

- 目的と責務: 可読なASTツリー出力（フィールド名、kind/kind_id、短いテキストプレビュー）
- 特徴: 深さ>50で打ち切り、コード文字列スライスはOption安全化（unwrap_or("<invalid>")）
- エッジケース
  - Unicodeの文字境界不一致で .get が None → "<invalid>" 埋め
  - 非常に深いAST → 50で打ち切り

5) collect_node_statistics（重要部分抜粋）

```rust
fn collect_node_statistics(node: Node, stats: &mut HashMap<String, (usize, usize)>) {
    fn collect_recursive(
        node: Node,
        stats: &mut HashMap<String, (usize, usize)>,
        depth: usize,
    ) {
        let node_kind = node.kind().to_string();
        let entry = stats.entry(node_kind).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = entry.1.max(depth);

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_recursive(child, stats, depth + 1);
        }
    }
    collect_recursive(node, stats, 0);
}
```

- 目的: ノード種別ごとの出現数・最大深さ測定
- エッジケース: 大規模ASTでメモリ上のstatsが大きくなる

## Walkthrough & Data Flow

代表的なフロー（Rust版 comprehensive_rust_analysis）:

```mermaid
flowchart TD
    A[Start] --> B[Read node-types.json (fs::read_to_string)]
    B --> C{Parse JSON (serde_json)}
    C -- Ok(Array) --> D[Collect named types into HashSet all_grammar_nodes]
    C -- Err/Unexpected --> D2[panic (expect) or structure mismatch]
    D --> E[Run RustParserAudit::audit_file(example)]
    E -- Ok --> F[audit: implemented_nodes, grammar_nodes, extracted_symbol_kinds]
    E -- Err --> F2[Fallback empty audit; println Warning]
    F --> G[example_nodes := keys(audit.grammar_nodes)]
    G --> H[Write AUDIT_REPORT.md]
    H --> I[Build analysis sets: in_grammar_only, in_example_not_handled, handled_well]
    I --> J[Compose GRAMMAR_ANALYSIS.md sections (Stats/Handled/Gaps/Missing/SymbolKinds)]
    J --> K[Write GRAMMAR_ANALYSIS.md]
    K --> L[Generate node_discovery via generate_rust_node_discovery()]
    L --> M[Write node_discovery.txt]
    M --> N[println summary & coverage]
    N --> O[End]
```

上記の図は`comprehensive_rust_analysis`関数の主要分岐を示す（行番号不明）。

Node Discoveryの共通フロー（generate_<lang>_node_discovery）:
- Language取得→ABI出力→Parserセット→例示コード読込（フォールバック有）→parse→root取得
- DEBUG_TREEでprint_node_tree（ユーティリティ、行番号不明）
- discover_nodes_with_idsでレジストリ作成
- 事前定義カテゴリに照らして ✓（例示で確認）/○（grammarのみ）/✗（未発見） を並べ、未分類も列挙
- 文字列を呼び出し側でファイル出力

Tree Structure生成（generate_<lang>_tree_structure）:
- parse→generate_tree_structureでツリー描写（深さ制限）→collect_node_statisticsで統計→Markdown形成→ファイル保存

## Complexity & Performance

- AST走査系（discover_nodes_with_ids, generate_tree_structure, collect_node_statistics）:
  - 時間: O(N)（N=ノード総数）
  - 空間: O(K)～O(U)（ユニーク種別数）
- 比較分析系（comprehensive_*_analysis）:
  - JSON処理: O(G)（Grammarノード数）
  - 集合演算とソート: O(E) 収集 + O(E log E) ソート（E=例示ノード種別数）
  - 出力形成: 文字列構築量はノード種類に比例
- ボトルネック:
  - 大規模例示コードのAST走査・文字列構築（I/Oではfs::writeの回数）
  - 多言語同時テスト時のファイルI/O競合
- スケール限界:
  - 非常に大きなASTでは、出力Markdownサイズが巨大化
  - 深さ制限（50）で出力は打ち切られるが、統計は全走査
- 実運用負荷要因:
  - ディスクI/O（複数ファイル出力）
  - JSONパース（node-types.jsonが大きい場合の負荷）

## Edge Cases, Bugs, and Security

セキュリティチェックリストに沿った評価。

- メモリ安全性
  - Buffer overflow / Use-after-free: Rust安全抽象を使用。unsafeなし。該当なし。
  - Integer overflow: kind_idはu16。演算なし。該当なし。
- インジェクション
  - SQL/Command/Path traversal: 固定パスのみ使用。ユーザ入力なし。該当なし。
- 認証・認可
  - 該当なし。
- 秘密情報
  - Hard-coded secrets: なし。ログ漏えい: 例示コード断片を出力するがローカルの例示ファイルのみ。問題軽微。
- 並行性
  - Race condition: cargo testの並列実行で同一ファイルへ同時writeの可能性（node_discovery.txtなど）。排他なし。
  - Deadlock: なし。

詳細なエッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| grammar JSON欠如 | contributing/parsers/python/node-types.json 不在 | 警告/スキップ | 多言語: expectでpanic。GDScriptのみ警告に落とす分岐あり | 問題あり（GDScript以外） |
| 例示コード欠如 | examples/go/comprehensive.go 不在 | フォールバックコードで最小パース成功 | 多言語でunwrap_or_elseでフォールバックを用意 | 一部改善済 |
| parseがNone | Parser::parseがNone返却 | 優雅にErr処理 | 多言語でunwrap使用（panic） | 問題あり |
| 出力ディレクトリ不存在 | contributing/parsers/php/ ディレクトリ不在 | 自動作成 or エラー | GDScriptのみcreate_dir_all。他はfs::writeでpanic | 問題あり |
| coverage分母0 | 例示ノード数0 | 0%など安全表記 | 浮動小数除算0→NaN/infを表示 | 問題あり |
| 並列テストの書き込み衝突 | 同一node_discovery.txtに複数テスト同時書込み | 直列化 or 排他 | 排他なし。上書き順不定 | 問題あり |
| 非ASCII文字境界 | node.byte_rangeが文字境界不一致 | 安全にフォールバック | unwrap_or("<invalid>")で安全 | OK |
| 巨大AST | 極端に深い/大きい | 出力打切/統計は継続 | depth>50で打切り | OK |
| カテゴリ未整合 | カテゴリに存在しない実ノード/誤名 | UNCATEGORIZEDに回す | 実装済（未分類出力） | OK |
| 環境変数DEBUG_TREE | セット時に大量stdout | ノイズ許容 | print_node_treeを実行 | 想定通り |

## Design & Architecture Suggestions

- 共通処理の抽象化
  - 各言語の comprehensive_*_analysis と generate_*_node_discovery はほぼ同型。ジェネリック/トレイト（例: LanguageAdapter）で共通化し、言語固有のパス・カテゴリだけ差し替え可能に。
  - カテゴリ定義は静的テーブル（BTreeMap）化し、未定義は自動的に未分類へ。
- エラー設計の統一
  - fs::read_to_stringのexpectを廃し、Resultを返却→テストではassert/skipを選択可能に。
  - parse(None)のunwrapを避け、None時はErrにしてフォールバックコード再試行/スキップ。
- 出力の競合対策
  - cargo testの並列性を考慮し、serial_testや一時ディレクトリ（tempfile）使用、またはファイル名にテストID/スレッドID付加。
  - もしくは1つの「総合」テストに集約し、順序制御。
- 観測性
  - printlnではなくlog/tracing採用（レベル/フィールド構造化、生成時間計測）。
- コマンド化
  - テストではなく、cargo runまたは独立バイナリで生成を行い、失敗を明示的なExitコードで返却。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト（ヘルパー関数）
  - discover_nodes_with_idsの最小ASTでの収集検証
  - generate_tree_structureのdepth制限・プレビューの挙動検証（疑似ノードで）
- インテグレーションテスト
  - 各言語について一時ディレクトリへ出力し、ファイル存在と基本的なセクションの包含をスナップショット比較。
  - grammar JSONが不在のケースでの挙動（GDScriptとその他の差異）を確認。
- 例
  ```rust
  // 疑似テスト例: discover_nodes_with_idsのユニットテスト（擬似ノード生成は適宜）
  #[test]
  fn test_discover_nodes_basic() {
      // 準備: 既知の小さなコード/言語でパース
      let mut parser = Parser::new();
      let language: Language = tree_sitter_go::LANGUAGE.into();
      parser.set_language(&language).unwrap();
      let code = "package main\nfunc main() {}";
      let tree = parser.parse(code, None).unwrap();
      let root = tree.root_node();

      let mut registry = HashMap::new();
      let mut found = HashSet::new();
      discover_nodes_with_ids(root, &mut registry, &mut found);

      assert!(registry.contains_key("source_file"));
      assert!(found.contains("function_declaration"));
  }
  ```
- 並列性テスト
  - 複数スレッドで同一パスに出力すると競合することを確認し、直列化設定が必要であることを実証。

## Refactoring Plan & Best Practices

- フェーズ1（安全化）
  - unwrap/expectの排除、Result伝搬。GDScript以外もgrammar欠如時に警告に移行。
  - parse(None)のNoneハンドリング追加。
  - coverage除算で分母ゼロなら「0.0%」/「N/A」表示。
  - 全言語で出力ディレクトリのcreate_dir_allを統一実施。
- フェーズ2（抽象化）
  - LanguageAdapterトレイトを定義（grammar_path, example_path, get_language(), ParserAuditの関連型、カテゴリ定義取得）。
  - 汎用関数 run_comprehensive_analysis<T: LanguageAdapter>() と generate_node_discovery<T>() に集約。
- フェーズ3（並列制御/出力設計）
  - serial_testで直列実行、または出力先を一時ディレクトリ化して競合回避。
  - 出力ファイル名にタイムスタンプ/テストIDを付与。
- ベストプラクティス
  - 文字列構築の大量操作はString::with_capacityで最適化、ソートはBTreeSet活用で不要なVec化削減。
  - カテゴリ定義・ノード名は定数群にまとめ、誤記防止。

## Observability (Logging, Metrics, Tracing)

- 現状: printlnによるコンソール出力のみ。*構造化ログ無し*、*メトリクス無し*、*トレース無し*。
- 提案:
  - tracingで各ステップ（grammar読込、audit、AST走査、ファイル書出し）の開始/終了・所要時間を計測。
  - ノード種別数・coverage・書出し成功/失敗カウンタをメトリクス（prometheusなど）に記録可能に。
  - DEBUG_TREEはログレベル（debug）に統一。

## Risks & Unknowns

- abi15_exploration_common::print_node_treeの詳細はこのチャンクに現れない（不明）。
- codanna::parsing::<lang>::audit::* の内部仕様（audit_fileの失敗条件、generate_reportのフォーマット）は不明。
- 各言語カテゴリの網羅性・正確性は文法定義と照合が必要（このチャンクでは検証不可）。
- cargo testの並列挙動に依存するファイル競合は、環境により再現性が異なる可能性。