# parsing\cpp\audit.rs Review

## TL;DR

- 目的: **C++コードのASTノードカバレッジ**を監査し、Tree-sitterが検出したノード・自前パーサがハンドルするノード・抽出されたシンボル種別を比較・可視化
- 公開API: **CppParserAudit::audit_file**, **audit_code**, **coverage_percentage**, **generate_report**、および**AuditError**/**CppParserAudit**データ構造
- コアロジック: **Tree-sitterによるAST走査**（discover_nodes）＋**自前CppParserの実行**で実際のハンドリングと抽出を集計
- 重要な複雑点: **外部依存の連携**（tree-sitterと自前パーサ）、レポート生成の**キーノード判定**と**差分出力**
- 重大リスク: **FileId::new(1).unwrap()のpanic可能性**、レポートの**非決定順序**（HashMap）、**キーノードのハードコード**による誤検出・誤判定
- Rust安全性: **unsafe不使用**、所有権・借用は関数スコープに閉じた安全設計。ただし**再帰（discover_nodes）**の深さ依存でスタック過負荷の可能性は*理論上*あり
- 並行性: **同期・非同期なし**。tree-sitter Parserは本関数内ローカル利用で安全だが、**スレッド間共有は非推奨**

## Overview & Purpose

このモジュールは、C++パーサの実装品質を高めるための**監査（Audit）機能**を提供します。具体的には、Tree-sitterで得られる**ASTノードの種類**（grammar_nodes）、実際に自前パーサ（CppParser）が**ハンドルしたノード**（implemented_nodes）、そして**抽出されたシンボル種別**（extracted_symbol_kinds）を収集・比較し、**カバレッジレポート**（generate_report）として出力します。

目的は以下の通りです：
- 現在のパーサが**対応できていないノード**を可視化（ギャップの発見）
- **重要ノード（キーノード）**に対する実装状況の指標化
- **テスト例の不足**（not found）を検知し、サンプル拡充を促す

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | AuditError | pub | 監査処理のエラー型を集約 | Low |
| Struct | CppParserAudit | pub | 監査結果（grammar_nodes, implemented_nodes, extracted_symbol_kinds）を保持 | Low |
| Fn | CppParserAudit::audit_file | pub | ファイル読み込み→監査実行 | Low |
| Fn | CppParserAudit::audit_code | pub | Tree-sitterでAST全走査→CppParser実行→結果集約 | Med |
| Fn | CppParserAudit::coverage_percentage | pub | 実装率の算出（implemented/grammar） | Low |
| Fn | CppParserAudit::generate_report | pub | カバレッジレポート文字列の生成 | Med |
| Fn | discover_nodes | private | ASTノード種別の再帰収集 | Low |

### Dependencies & Interactions

- 内部依存
  - **audit_code → discover_nodes**（AST全走査）
  - **audit_code → CppParser**（自前パーサでシンボル抽出・ハンドル済みノード取得）
  - **generate_report → format_utc_timestamp**（日時の付与）

- 外部依存（表）

| 依存 | 種別 | 用途 |
|------|------|------|
| tree_sitter::Parser | クレート | C++コードをASTへパース |
| tree_sitter_cpp::LANGUAGE | クレート | C++言語定義（Tree-sitter） |
| thiserror::Error | クレート | エラー型の派生（表示/変換） |
| std::fs::read_to_string | 標準 | ファイル読み込み |
| std::collections::{HashMap, HashSet} | 標準 | 集計構造 |
| crate::types::{FileId, SymbolCounter} | 自前 | パース用のファイルID、シンボル集計用 |
| crate::io::format::format_utc_timestamp | 自前 | レポートにUTCタイムスタンプ付与 |
| super::CppParser | 自前 | C++シンボル抽出ロジック（パーサ本体） |

- 被依存推定
  - **CLI/ツールの監査コマンド**や**CIの品質ゲート**で利用
  - **開発者が差分を確認**し、パーサ実装を拡充するための補助

※ インポートされている**NodeTracker**はこのファイル内で未使用です（このチャンクに現れない利用箇所）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| CppParserAudit::audit_file | `pub fn audit_file(file_path: &str) -> Result<Self, AuditError>` | ファイルから読み込み、監査を実行 | O(n) | O(u + s) |
| CppParserAudit::audit_code | `pub fn audit_code(code: &str) -> Result<Self, AuditError>` | 文字列コードに対してAST走査＋自前パーサ実行 | O(n) | O(u + s) |
| CppParserAudit::coverage_percentage | `pub fn coverage_percentage(&self) -> f64` | 実装率（%）の算出 | O(1) | O(1) |
| CppParserAudit::generate_report | `pub fn generate_report(&self) -> String` | カバレッジレポート文字列の生成 | O(k + u + i + s) | O(report) |
| 型: AuditError | `pub enum AuditError` | 監査処理に関するエラー表現 | - | - |
| 型: CppParserAudit | `pub struct CppParserAudit { pub grammar_nodes: HashMap<String, u16>, pub implemented_nodes: HashSet<String>, pub extracted_symbol_kinds: HashSet<String> }` | 監査結果のデータ契約 | - | - |

凡例: n=コード長/ASTノード数、u=ユニークASTノード種類数、i=実装ノード種類数、s=シンボル種類数、k=キーノード数

### CppParserAudit::audit_file

1) 目的と責務
- ファイルパスからコードを読み込み、**audit_code**に委譲して監査結果を作成。

2) アルゴリズム
- read_to_stringでコード読込
- audit_code(code)を呼び出し
- 結果の返却

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| file_path | &str | 監査対象のC++ソースファイルパス |

4) 戻り値

| 型 | 説明 |
|----|------|
| Result<CppParserAudit, AuditError> | 監査結果またはエラー |

5) 使用例
```rust
use parsing::cpp::audit::CppParserAudit;

let audit = CppParserAudit::audit_file("examples/comprehensive.cpp")?;
println!("Coverage: {:.2}%", audit.coverage_percentage());
println!("{}", audit.generate_report());
```

6) エッジケース
- ファイルが存在しない／読み取り不可（AuditError::FileRead）
- 空ファイル（AST空→coverage 0%）

### CppParserAudit::audit_code

1) 目的と責務
- **Tree-sitter**でASTを構築し、**discover_nodes**でノード種類を収集。
- **CppParser**でコードを解析して、**ハンドルされたノード**と**抽出シンボル種別**を収集。
- 監査結果を構築。

2) アルゴリズム（ステップ）
- Parser::new → set_language(tree_sitter_cpp::LANGUAGE)
- parser.parse(code) → Tree生成
- tree.root_node()から**discover_nodes**で全ノード種類をHashMapに収集
- CppParser::new → parse(code, FileId::new(1).unwrap(), &mut SymbolCounter)
- SymbolのkindをDebug表現でHashSetへ（extracted_symbol_kinds）
- CppParser::get_handled_nodes() → 名前をHashSetへ（implemented_nodes）
- CppParserAudit構築

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| code | &str | 監査対象のC++コード文字列 |

4) 戻り値

| 型 | 説明 |
|----|------|
| Result<CppParserAudit, AuditError> | 監査結果またはエラー |

5) 使用例
```rust
let code = r#"
namespace N {
  struct S { int x; };
  int f(int a) { return a + 1; }
}
"#;

let audit = CppParserAudit::audit_code(code)?;
assert!(audit.grammar_nodes.contains_key("namespace_definition"));
println!("{:?}", audit.implemented_nodes);
```

6) エッジケース
- set_language失敗（AuditError::LanguageSetup）
- parse失敗（AuditError::ParseFailure）
- CppParser::new失敗（AuditError::ParserCreation）
- FileId::new(1).unwrap()でpanicの可能性（回避推奨）
- きわめて深いAST構造で再帰が深くなる可能性

### CppParserAudit::coverage_percentage

1) 目的と責務
- grammar_nodes（ユニーク種類数）に対してimplemented_nodes（ユニーク種類数）の比率を計算。

2) アルゴリズム
- grammar_nodesが空なら0.0
- len(implemented)/len(grammar)をf64で返却

3) 引数
- なし（&selfのみ）

4) 戻り値

| 型 | 説明 |
|----|------|
| f64 | パーサがハンドルしたノード種類の割合（%） |

5) 使用例
```rust
let pct = audit.coverage_percentage();
println!("Coverage: {:.2}%", pct);
```

6) エッジケース
- grammar_nodesが空（0%）

### CppParserAudit::generate_report

1) 目的と責務
- 監査結果の**人間可読なレポート**をMarkdown形式で生成。

2) アルゴリズム
- ヘッダ、Summary（counts）
- Coverage Table（キーノードの実装状況）
- Legend（記号の意味）
- Recommended Actions（gap/not foundの対応方針）

3) 引数
- なし（&selfのみ）

4) 戻り値

| 型 | 説明 |
|----|------|
| String | レポート本文（Markdown） |

5) 使用例
```rust
let report = audit.generate_report();
println!("{}", report);
```

6) エッジケース
- grammar_nodesが空→Summaryは0、Coverage Tableはキーノードのほぼ「not found」
- implemented_nodesやextracted_symbol_kindsが空→適切に0表示

### Data Contracts（構造体フィールド）

- CppParserAudit
  - **grammar_nodes: HashMap<String, u16>**
    - キー＝Tree-sitterノード種別名、値＝kind_id（u16）
    - ユニーク種類のみ保持（頻度は不保持）
  - **implemented_nodes: HashSet<String>**
    - 自前パーサがハンドルするノード種別名（動的トラッキング）
  - **extracted_symbol_kinds: HashSet<String>**
    - 抽出されたシンボルのkind（Debug文字列）

- AuditError
  - **FileRead(std::io::Error)**
  - **LanguageSetup(String)**
  - **ParseFailure**
  - **ParserCreation(String)**

## Walkthrough & Data Flow

- audit_file
  - ファイル読み取り → audit_codeへ委譲
- audit_code
  - Tree-sitterのParser初期化・言語設定 → AST構築
  - discover_nodesで**AST全走査**・**ノード種類収集**
  - CppParser.new → parse → **シンボル抽出**
  - CppParser.get_handled_nodes → **実装済みノード収集**
  - 結果をCppParserAuditにまとめる
- generate_report
  - SummaryとCoverage Table、推奨アクションを組み立て

```mermaid
sequenceDiagram
  autonumber
  participant U as 呼び出し元
  participant A as CppParserAudit
  participant TS as tree_sitter::Parser
  participant CP as CppParser
  participant SC as SymbolCounter

  U->>A: audit_file(file_path)
  A->>A: read_to_string(file_path)
  A->>A: audit_code(code)
  A->>TS: Parser::new(); set_language(LANGUAGE)
  TS-->>A: parse(code) -> Tree
  A->>A: discover_nodes(Tree.root)（再帰）
  A->>CP: CppParser::new()
  A->>SC: SymbolCounter::new()
  A->>CP: parse(code, FileId(1), &mut SC)
  CP-->>A: symbols(Vec)
  A->>CP: get_handled_nodes()
  CP-->>A: handled(Vec)
  A-->>U: CppParserAudit{grammar_nodes, implemented_nodes, extracted_symbol_kinds}

  U->>A: generate_report()
  A-->>U: String (Markdown Report)
```
上記の図は`audit_code`関数の主要ステップを示します（このチャンクには行番号が含まれていないため関数名のみで参照）。

該当コード抜粋（discover_nodesは完全引用）：
```rust
fn discover_nodes(node: tree_sitter::Node, registry: &mut HashMap<String, u16>) {
    registry.insert(node.kind().to_string(), node.kind_id());

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_nodes(child, registry);
    }
}
```

`generate_report`のキーノード定義部（主要箇所のみ抜粋）：
```rust
// Key nodes we care about for symbol extraction
let key_nodes = vec![
    "translation_unit",
    "function_definition",
    "class_specifier",
    "struct_specifier",
    "union_specifier",
    "enum_specifier",
    "namespace_definition",
    "template_declaration",
    "template_function",
    "template_type",
    "function_declarator",
    "init_declarator",
    "parameter_declaration",
    "field_declaration",
    "type_definition",
    "alias_declaration",
    "access_specifier",
    "base_class_clause",
    "destructor_name",
    "operator_name",
    "field_initializer_list",
    "lambda_expression",
    "using_declaration",
    "call_expression",
    "field_expression",
    "qualified_identifier",
];
/* ... 省略 ... */
```

## Complexity & Performance

- audit_file: O(n) 時間（ファイルサイズ n）、O(1) 追加メモリ（読み込み文字列）
- audit_code: 
  - Tree-sitterのparseが**O(n)**（入力コード長 n）
  - discover_nodesの再帰走査が**O(N)**（ASTノード総数 N、一般に n に線形）
  - CppParser.parseは実装次第だが一般に**O(n)**程度（ここでは外部）
  - 合計は**O(n)**時間、スペースは**O(u + s)**（ユニークノード・シンボル種類）
- coverage_percentage: O(1)
- generate_report: O(k + u + i + s)（k=キーノード数、u/i/sは各集合サイズ）

ボトルネック・スケール限界:
- **Tree-sitterのパース**が主なコスト。大規模ファイルでは時間・メモリ負荷増。
- **再帰走査（discover_nodes）**はAST深さに依存。極端な深さでスタック使用が増加。
- **レポート生成**はHashMap/HashSet由来で順序非決定のため、出力の安定性に欠ける（比較やスナップショットテストに不向き）。

I/O・ネットワーク・DB:
- **I/O**はaudit_fileのみ（read_to_string）。ネットワーク・DBアクセスはなし。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト:
- メモリ安全性: **unsafe未使用**。所有権・借用は関数スコープで完結。再帰の深さによる**理論上のスタック過負荷**に注意。
- インジェクション: 解析のみで**コマンド/SQL/パスインジェクションなし**。file_pathは読み取り用途。
- 認証・認可: 該当なし。
- 秘密情報: **ハードコード秘密情報なし**。レポート出力に機密は原則含まれないが、**シンボル種別**はプロジェクト内部情報を含む可能性があるので外部公開には注意。
- 並行性: **スレッド競合なし**。Parserはローカル利用で安全。共有する設計は避ける。

既知/潜在バグ:
- **panic**の可能性: `FileId::new(1).unwrap()` が失敗時にpanic。例外処理に統一するべき（AuditErrorへ変換）。
- **レポート順序の非決定性**: HashMap/HashSetを直接列挙しており、**安定順序ではない**ため、テキスト比較テストや人間の目視比較が不安定。
- **キーノードのハードコード**: 実際のTree-sitter C++文法と**名称の差異**がある場合、誤って「not found」や「gap」判定となる。
- **頻度情報の欠落**: grammar_nodesは**種類のみ**を保持し、発生回数を集計しないため、「重要ノードの出現頻度」の視点が欠ける。

エッジケース詳細表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ファイルが存在しない | "no_such.cpp" | AuditError::FileRead を返す | `audit_file`で? | OK |
| 言語設定失敗 | 不正LANGUAGE | AuditError::LanguageSetup を返す | `set_language`→map_err | OK |
| パース失敗 | 破損/巨大入力 | AuditError::ParseFailure を返す | `parse(...).ok_or(...)` | OK |
| CppParser生成失敗 | 構築エラー | AuditError::ParserCreation を返す | `CppParser::new().map_err` | OK |
| FileId生成失敗 | FileId::new(1)=None | エラーへ変換して返す | `unwrap()` | 問題（panic） |
| 空コード | "" | coverage=0%、レポートは0件表示 | 現行で0% | OK |
| 非常に深いAST | 極端なテンプレートネスト | 再帰でスタック過負荷を避ける | 再帰のみ | リスク |
| キーノード未出現 | テスト例が不足 | "not found" として表示し推奨アクション提示 | generate_report | OK |

Rust特有の観点（詳細チェック）:
- 所有権: `audit_code`内の変数（`parser`, `tree`）は関数スコープで所有され、**NodeはTreeに紐づく**ため生存期間中のみ使用（安全）。
- 借用: `discover_nodes`は`&mut HashMap`を可変借用し、**同期なし**の単一スレッドで安全に更新。
- ライフタイム: Tree-sitterの`Node`は**Treeのライフタイムに束縛**される設計だが、本関数スコープ内で完結しており**明示的ライフタイム指定不要**。
- unsafe境界: **unsafeなし**。
- Send/Sync: グローバル共有なし。`Parser`は**スレッド間共有しない**設計が望ましい（本コードはローカル変数でOK）。
- await境界/非同期: 非同期処理なし。
- エラー設計: `thiserror`で表現を統一し、`From<std::io::Error>`を実装（FileRead）。一方で**unwrapの混入**は設計上好ましくない。

## Design & Architecture Suggestions

- **panic回避**: `FileId::new(1).unwrap()`を**Result**に変換して`AuditError`へ伝播する（例: `AuditError::ParserCreation("invalid FileId".into())`など、専用Variant追加も検討）。
- **順序安定化**: レポート生成時は`BTreeMap`/`BTreeSet`へ一時変換して**安定した並び**で表記。CI差分が読みやすくなる。
- **キーノードの外部設定化**: `key_nodes`を**設定ファイル**または**関数引数**で渡せるようにし、プロジェクトやTree-sitterバージョン差異に適応。
- **頻度収集**: `grammar_nodes`を`HashMap<String, (u16, count)>`のように拡張し、**出現回数**を表示することで優先度判断に資する。
- **発見/ギャップの明確化**: レポートに「実装されているがファイルに出現しなかったノード」や「ファイルに存在するが未実装」などを**別セクション**で粒度を上げる。
- **API拡張**: `audit_code_with_file_id(code, file_id)`のようなAPIを追加し、呼び出し元でFileIdを管理できる構成に。
- **パイプライン化**: 複数ファイルを束ねた**プロジェクト単位の監査**APIを用意して総合カバレッジを測定。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト（audit_codeの正常系）
```rust
#[test]
fn audit_code_collects_core_nodes() {
    let code = r#"
    namespace N {
        struct S { int x; };
        int f(int a) { return a + 1; }
    }"#;
    let audit = CppParserAudit::audit_code(code).expect("audit should succeed");
    assert!(audit.grammar_nodes.contains_key("namespace_definition"));
    assert!(audit.grammar_nodes.contains_key("struct_specifier"));
    assert!(audit.extracted_symbol_kinds.len() >= 1);
}
```

- エラー系テスト（audit_fileのFileRead）
```rust
#[test]
fn audit_file_missing_returns_error() {
    let err = CppParserAudit::audit_file("no_such_dir/no_such_file.cpp").unwrap_err();
    match err {
        AuditError::FileRead(_) => {},
        _ => panic!("expected FileRead"),
    }
}
```

- パース失敗テスト（疑似的に不正コード）
```rust
#[test]
fn audit_code_parse_failure() {
    // Tree-sitterは多くの不正でも木を返すことがあるが、極端なケースを想定
    let code = "\u{0}"; // NULなど
    let res = CppParserAudit::audit_code(code);
    // 実際にParseFailureになるかはTree-sitter依存（このチャンクでは不明）
    // ここではエラーでも成功でも受容するが、エラー時はParseFailureであることを確認
    if let Err(e) = res {
        match e {
            AuditError::ParseFailure | AuditError::LanguageSetup(_) => {},
            _ => panic!("unexpected error: {:?}", e),
        }
    }
}
```

- レポート安定性テスト（キーの存在チェック）
```rust
#[test]
fn report_contains_summary_and_table() {
    let code = "int f(){return 0;}";
    let audit = CppParserAudit::audit_code(code).unwrap();
    let report = audit.generate_report();
    assert!(report.contains("# C++ Parser Coverage Report"));
    assert!(report.contains("## Summary"));
    assert!(report.contains("## Coverage Table"));
    assert!(report.contains("| Node Type | ID | Status |"));
}
```

- クリティカル修正テスト（unwrap排除後を想定）
```rust
#[test]
fn file_id_creation_error_propagates() {
    // ここでは実装変更後を想定（このチャンクには現れない）
    // FileId生成に失敗した場合、AuditErrorの新Variant等で返ることを確認
    // 不明: 具体的なFileId制約はこのチャンクには現れない
}
```

- インテグレーションテスト
  - **実際のCppParser**と**comprehensive.cpp**サンプルを用いて、**gap/not found**がレポートに出ることと、**coverage%**がしきい値を超えることを検証（ここではコード割愛）。

## Refactoring Plan & Best Practices

- **unwrap排除**: `FileId::new(1).unwrap()` → `FileId::new(1).ok_or(AuditError::ParserCreation("invalid FileId".into()))?`
- **順序の安定化**: レポート生成時に`BTreeMap`/`BTreeSet`へ変換して`sorted`出力
- **NodeTrackerの未使用削除**: インポートのみで未使用なので削除して警告抑止
- **キー定義の集中管理**: `key_nodes`を定数/設定として別モジュールへ抽出
- **レポートの拡張**: 発生回数・未実装率・推奨優先度等を加える
- **文字列生成の効率化**: `String::with_capacity`で容量予約、`write!`マクロの活用で軽微なパフォーマンス改善

## Observability (Logging, Metrics, Tracing)

- **tracing**の導入
  - `audit_code`開始/終了、Tree-sitterパースの成功/失敗、CppParserの実行結果（件数）を`tracing::info!`/`tracing::error!`で記録
  - `generate_report`時にカバレッジ値・gap/not found件数をログ
- **メトリクス**
  - `grammar_nodes.len()`、`implemented_nodes.len()`、`extracted_symbol_kinds.len()`をカウンタとして出力
  - カバレッジ%をゲージとして記録
- **トレーシング**
  - `audit_code`全体をspanで囲み、外部からの呼び出しコンテキストと紐づける
- **ログの機密配慮**
  - シンボル名やパスなどの出力は**デバッグレベル**に限定し、デフォルトは集計値のみ

## Risks & Unknowns

- **CppParserの内部動作**はこのチャンクには現れないため不明（ハンドル済みノードの収集方法、返却Symbolの仕様、`get_handled_nodes`の構造）
- **FileId::new**の仕様（なぜ1が有効なのか、制約）は不明
- **Tree-sitter C++のノード名**の完全性（`template_function`, `template_type`などの名称が正しいか）は*バージョン依存*で、キーノードの一部は**存在しない可能性**あり
- **SymbolCounter**の役割と副作用はこのチャンクには現れない
- **極端なAST深さ**の現場再現性は低いものの、再帰利用に伴う**スタック過負荷**は理論上の懸念

以上により、本モジュールは**監査用途として有効**ながら、いくつかの**堅牢性（panic回避・順序安定）と拡張性（キー管理・頻度集計）**の改善余地があります。