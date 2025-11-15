# parsing/typescript/audit.rs Review

## TL;DR

- 目的: TypeScript/TSXコードをTree-sitterで走査し、独自パーサが実際に処理しているASTノード・抽出されたシンボル種別とのカバレッジを可視化するレポートを生成する。
- 主要公開API: `TypeScriptParserAudit::audit_file`, `TypeScriptParserAudit::audit_code`, `TypeScriptParserAudit::generate_report`, 型`TypeScriptParserAudit`, エラー型`AuditError`。
- コアロジック: Tree-sitterでAST走査→ノード種別収集→自前`TypeScriptParser`による解析→抽出シンボル種別収集→両者比較レポート化。
- 複雑箇所: レポート生成で「実装済/未実装/ファイルに存在しない」判定の分岐。AST走査は再帰だが単純。
- 重大リスク: `audit_file`が任意パス読み込み（パストラバーサル/権限）、大規模ファイルでのAST全走査コスト、`NodeTracker`未使用インポート、`FileId(1)`固定による衝突の可能性。
- Rust安全性: unsafeなし、`Result`でエラー伝搬、`HashMap/HashSet`使用による所有権安全。並行性は扱っていない。
- パフォーマンス: 時間・空間ともに入力サイズとASTノード数に対して線形。大規模コードでのメモリ負荷に注意。

## Overview & Purpose

このモジュールは、TypeScript（TSX含む）コードに対するパーサの実装カバレッジを監査するための補助ツールです。具体的には:

- Tree-sitterを用いてファイル内に現れるASTノード種別を列挙。
- 独自の`TypeScriptParser`が実際に「扱った（処理した）」ノード種別を収集。
- この差分から、実装済み・ギャップ（Grammar上は存在するが未対応）・サンプル上未出のノードをレポート化。

用途:
- シンボル抽出の漏れを定量把握。
- 実装優先度の整理（ギャップの可視化）。
- テストサンプル拡充の指針作成（未出ノードの検出）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | AuditError | pub | 監査処理における各種失敗の定義（ファイル読み込み/言語設定/パース失敗/パーサ生成失敗） | Low |
| Struct | TypeScriptParserAudit | pub | カバレッジ結果（grammar_nodes / implemented_nodes / extracted_symbol_kinds）と監査処理APIを提供 | Med |
| Function | TypeScriptParserAudit::audit_file | pub | ファイルパスからコード読込→監査 | Low |
| Function | TypeScriptParserAudit::audit_code | pub | コード文字列から監査（Tree-sitter走査＋独自パーサ実行） | Med |
| Function | TypeScriptParserAudit::generate_report | pub | カバレッジレポートの文字列生成 | Med |
| Function | discover_nodes | private | ASTを再帰走査してノード種別とkind_idを収集 | Low |
| Module | tests | private | 簡易ユニットテスト（インターフェース/クラスの検出確認） | Low |

### Dependencies & Interactions

- 内部依存:
  - `TypeScriptParserAudit::audit_code` → `discover_nodes`（AST再帰走査）
  - `TypeScriptParserAudit::audit_code` → `TypeScriptParser::new`, `TypeScriptParser::parse`, `TypeScriptParser::get_handled_nodes`（独自パーサとの連携）
  - `TypeScriptParserAudit::generate_report` → `format_utc_timestamp`（レポートのタイムスタンプ生成）
- 外部依存（抜粋）:

| 依存 | 目的 | 備考 |
|-----|------|------|
| tree_sitter::{Node, Parser} | TSXのAST生成と走査 | 言語設定にTSXを使用 |
| tree_sitter_typescript::LANGUAGE_TSX | 言語定義（TSX） | TypeScript + JSX対応 |
| thiserror::Error | エラー型導出 | `AuditError`の派生 |
| crate::types::{FileId, SymbolCounter} | パーサ呼び出しで使用 | `FileId(1)`固定を渡す |
| crate::io::format::format_utc_timestamp | タイムスタンプ書式 | レポートヘッダに使用 |
| super::TypeScriptParser | 独自パーサ | ノード追跡・シンボル抽出 |

- 被依存推定:
  - CLI/開発者ツールから監査を回す機能。
  - CIでのカバレッジチェックレポート生成。
  - ドキュメント/READMEなどで現状対応範囲の可視化。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| AuditError | enum AuditError | 監査処理の失敗理由の表現 | - | - |
| TypeScriptParserAudit | struct TypeScriptParserAudit | 監査結果のコンテナ | - | - |
| audit_file | fn audit_file(file_path: &str) -> Result<Self, AuditError> | ファイル読み込みから監査実行 | O(n + m) | O(u + s) |
| audit_code | fn audit_code(code: &str) -> Result<Self, AuditError> | コード文字列から監査実行 | O(n + m) | O(u + s) |
| generate_report | fn generate_report(&self) -> String | カバレッジレポートの生成 | O(k + g) | O(r) |

注:
- n: コード長、m: ASTノード数、u: 一意ノード種別数、s: 抽出シンボル数、k: キー対象ノード数（固定配列）、g: ギャップ/未出ノード数、r: レポート文字列長。

### Data Contracts

- AuditError
  - FileRead(std::io::Error)
  - LanguageSetup(String)
  - ParseFailure
  - ParserCreation(String)
- TypeScriptParserAudit
  - grammar_nodes: HashMap<String, u16>
    - キー: Tree-sitterの`Node::kind()`名
    - 値: `Node::kind_id()`（u16）
    - 備考: 同一kindが複数出現しても最後に挿入されたidで上書き。頻度ではなく種別集合の収集。
  - implemented_nodes: HashSet<String>
    - 独自パーサが動的に追跡した「扱ったノード種別」名の集合。
  - extracted_symbol_kinds: HashSet<String>
    - 抽出されたシンボルの`kind`の`Debug`文字列表現集合（例: "Interface", "Class"）。

### 各APIの詳細

1) audit_file
- 目的と責務
  - ファイルパスからUTF-8文字列として読み込み、`audit_code`に委譲。
- アルゴリズム（ステップ）
  1. `std::fs::read_to_string(file_path)`でコード取得。
  2. 失敗時は`AuditError::FileRead`でErr。
  3. 成功時は`audit_code(&code)`を呼ぶ。
- 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| file_path | &str | はい | 読み込むTypeScript/TSXファイルへのパス |

- 戻り値

| 型 | 説明 |
|----|------|
| Result<TypeScriptParserAudit, AuditError> | 監査結果かエラー |

- 使用例
```rust
use parsing::typescript::audit::TypeScriptParserAudit;

let audit = TypeScriptParserAudit::audit_file("examples/comprehensive.ts")
    .expect("audit failed");
println!("{}", audit.generate_report());
```
- エッジケース
  - 空ファイル/非UTF-8ファイル
  - 存在しないパス/権限不足
  - 読み込み成功でも解析失敗（`audit_code`側）

2) audit_code
- 目的と責務
  - 渡されたコード文字列をTSX言語でTree-sitterパースし、ASTノード種別を収集。独自`TypeScriptParser`でシンボル抽出・処理済ノード収集を行い、両者をまとめる。
- アルゴリズム（ステップ）
  1. `Parser::new()`でパーサ生成。
  2. `set_language(&LANGUAGE_TSX)`で言語設定（失敗時`LanguageSetup`）。
  3. `parser.parse(code, None)`でAST生成（Noneならインクリメンタルなし）。失敗時`ParseFailure`。
  4. `discover_nodes(tree.root_node(), &mut grammar_nodes)`でAST全走査し種別とidを格納。
  5. `TypeScriptParser::new()`で独自パーサ生成（失敗時`ParserCreation`）。
  6. `FileId(1)`と`SymbolCounter::new()`を用意し、`ts_parser.parse(code, file_id, &mut symbol_counter)`でシンボル抽出。
  7. 各シンボルの`kind`を`format!("{:?}", symbol.kind)`で文字列化し`extracted_symbol_kinds`へ。
  8. `ts_parser.get_handled_nodes()`から動的追跡済みノード名を`implemented_nodes`へ。
  9. `TypeScriptParserAudit`を構築して返却。
- 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| code | &str | はい | TypeScript/TSXコード（UTF-8） |

- 戻り値

| 型 | 説明 |
|----|------|
| Result<TypeScriptParserAudit, AuditError> | 監査結果かエラー |

- 使用例
```rust
use parsing::typescript::audit::TypeScriptParserAudit;

let code = r#"export interface I { x: number } class C implements I { x = 1 }"#;
let audit = TypeScriptParserAudit::audit_code(code)?;
assert!(audit.extracted_symbol_kinds.contains("Interface"));
assert!(audit.extracted_symbol_kinds.contains("Class"));
# Ok::<(), Box<dyn std::error::Error>>(())
```
- エッジケース
  - 空文字列/コメントのみ
  - TSX要素（JSX）を含む
  - 構文エラーを含む（Tree-sitterはエラーノードを生成可能）

3) generate_report
- 目的と責務
  - 監査結果を人間向けカバレッジレポートとして整形（Markdown風テーブル）。
- アルゴリズム（ステップ）
  1. 見出し/タイムスタンプを出力。
  2. 概要（ノード種別数/実装済ノード数/シンボル種別数）。
  3. キー対象ノード配列（固定）で各種別のステータス判定:
     - grammar_nodesに存在 かつ implemented_nodesに存在 → implemented
     - grammar_nodesに存在 かつ implemented_nodesに不在 → gap
     - grammar_nodesに不在 → not found
  4. 凡例と推奨アクション（ギャップ/未出の列挙）。
- 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | &TypeScriptParserAudit | はい | 監査結果 |

- 戻り値

| 型 | 説明 |
|----|------|
| String | レポート文字列 |

- 使用例
```rust
let report = audit.generate_report();
println!("{report}");
```
- エッジケース
  - キー対象ノードがすべて未出（"not found"のみ）
  - implemented_nodesが空
  - extracted_symbol_kindsが空

## Walkthrough & Data Flow

- 入力（audit_file）
  - file_path受領→ファイルをUTF-8で読み込み→文字列を`audit_code`へ。
- 入力（audit_code）
  - code文字列→Tree-sitter(Parser)に投入→AST生成（TSX）。
  - `discover_nodes`がAST再帰走査し`grammar_nodes`にkind→kind_idを登録。
  - 独自`TypeScriptParser`を生成し、`parse`に`code`, `FileId(1)`, `SymbolCounter`を渡す。
  - 返却された`symbols`から`kind`をデバッグ文字列化して`extracted_symbol_kinds`に追加。
  - `get_handled_nodes()`からノード追跡情報を取り出し、`implemented_nodes`に反映。
- 出力（generate_report）
  - `grammar_nodes`と`implemented_nodes`をキー対象ノードの配列で参照し、結果をMarkdownテーブルに整形。
  - ギャップと未出のリストを示し、推奨アクションを出力。

注: 重要な主張に対する行番号は、このチャンクには行番号が含まれないため不明。

## Complexity & Performance

- audit_file
  - 時間: O(n)（ファイル読み込み）＋`audit_code`のコスト
  - 空間: O(n)（コード文字列）＋`audit_code`の空間
- audit_code
  - 時間: O(n + m) 目安。Tree-sitterパースは概ね入力長に線形、`discover_nodes`はASTノード数mに線形。独自パーサの時間は実装依存だが典型的に線形。
  - 空間: O(u + s) 目安。`grammar_nodes`は一意ノード種別数u（Grammar定義上の上限近辺で飽和）。`extracted_symbol_kinds`はシンボル種別数s（実質小さい集合）。
- generate_report
  - 時間: O(k + g)（キー対象ノード数kは固定、ギャップ/未出数gに比例）
  - 空間: O(r)（出力文字列長）

ボトルネック:
- 大規模ファイルでのAST全走査（再帰）によるCPU時間増加。
- `symbols`生成の独自パーサ側のコスト。
- `grammar_nodes`が頻度情報を持たないため、優先度判断に追加計算が必要。

スケール限界:
- 超大規模モノレポで多数ファイルを逐次解析すると、総時間は線形に増加。並列化には各ファイルで独立パーサを生成する必要。

I/O/ネットワーク/DB:
- ファイルI/Oのみ。ネットワーク/DBアクセスはなし。

## Edge Cases, Bugs, and Security

### エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | ParseFailureでErr | `audit_code` | 実装済 |
| コメントのみ | "// a" | AST生成成功。ノード種別は限定的 | `audit_code` | 実装済 |
| 構文エラー | "class {" | ASTは生成されうる（エラーノード含む）。走査継続 | `audit_code` | 実装済 |
| 非UTF-8ファイル | バイナリ | FileReadでErr | `audit_file` | 実装済 |
| ファイル未存在 | "nope.ts" | FileReadでErr | `audit_file` | 実装済 |
| 権限不足 | "/root/secret.ts" | FileReadでErr | `audit_file` | 実装済 |
| TSX要素含有 | "<div/>" | TSX言語設定でパース成功 | `audit_code` | 実装済 |
| 巨大ファイル | 数MB〜 | 時間/メモリが増加するが動作 | 全体 | 注意 |
| implemented_nodesが空 | 特定設定 | gap/not foundが増える | `generate_report` | 実装済 |
| Symbol無し | 宣言無し | extracted_symbol_kindsが空 | `audit_code` | 実装済 |

### 既知/潜在バグ

- NodeTrackerの未使用インポート
  - `use crate::parsing::NodeTracker;`がこのファイル内で未使用。警告の原因。除去推奨。
- `grammar_nodes`が頻度を記録しない
  - 現状は一意種別集合のみ。優先度付けには出現頻度も欲しい場合がある。
- `FileId(1)`の固定
  - 同一IDを常に渡しており、独自パーサ側がIDを意味的に利用する場合は衝突・混同の恐れ。監査用でもコメントを追加するかAPIでID可変にする。
- `extracted_symbol_kinds`が`Debug`表現
  - `format!("{:?}", symbol.kind)`は表示目的には十分だが、安定した識別子が必要なら`Display`/専用文字列への変換が望ましい。

### セキュリティチェックリスト

- メモリ安全性
  - Buffer overflow: 標準ライブラリ利用のみ。unsafe未使用。問題なし。
  - Use-after-free: 所有権/借用に従っており問題なし。
  - Integer overflow: `u16`の`kind_id`格納のみで計算なし。問題なし。
- インジェクション
  - SQL/Command/Path traversal: `audit_file`は任意パスを読むため、上位層で許可ディレクトリの制限/検証が必要。コマンド実行なし、SQLなし。
- 認証・認可
  - 本モジュールでは未実施。上位層で必要に応じて制御。
- 秘密情報
  - レポートはコード本文を含まず、抽出種別と統計のみ。ログ漏洩の懸念は低い。
- 並行性
  - 非同期/並列なし。共有可変状態なし。データ競合の懸念は低い。

## Design & Architecture Suggestions

- TSX/TSの選択切り替え
  - 現状TSX固定。引数で`TS`/`TSX`を選べるようにし、JSXを含まない場合のオーバーヘッドを避ける。
- 頻度情報の収集
  - `grammar_nodes: HashMap<String, (u16, u32)>`などに変更して出現回数も記録することで優先度判断が容易になる。
- 柔軟なキー対象ノード
  - レポートのキー対象ノード配列を外部設定可能にし、プロジェクト要件に応じて監査対象を拡張/縮小。
- 安定したシンボル種別表現
  - `Debug`ではなく、明示的な`Display`や専用`&'static str`へのマッピングを使用。
- `FileId`の扱いをAPI化
  - 監査呼び出し側で`FileId`を渡せるようにし、IDの衝突懸念を緩和。
- 複数ファイル監査
  - 新API（例: `audit_files<I: IntoIterator<Item=&str>>`）でまとめて監査し、総合レポートを生成。並列化は各ファイルごとに独立パーサを生成する設計。
- エクスポートの明確化
  - 未使用インポート削除。公開フィールドのままで良いか、アクセサ/ビルダでの管理に切替を検討。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト
  - 正常系: インターフェース/クラス/メソッドの検出（既存テスト）。
  - 空入力: `audit_code("")`が`ParseFailure`を返すかを検証。
  - TSX要素: JSX含むコードで`jsx_element`/`jsx_self_closing_element`の検出有無を確認。
  - ギャップ判定: `grammar_nodes`に存在するが`implemented_nodes`にないノード例を用意し、レポートが「gap」を示すことを確認。
  - 未出判定: キー対象に含まれるがファイル内にないノードについて「not found」を確認。
- 例: 追加テストコード
```rust
#[test]
fn test_audit_empty_code() {
    let code = "";
    let res = TypeScriptParserAudit::audit_code(code);
    assert!(res.is_err());
    if let Err(e) = res {
        match e {
            AuditError::ParseFailure => {},
            _ => panic!("Unexpected error: {e:?}"),
        }
    }
}

#[test]
fn test_audit_tsx_elements() {
    let code = r#"
        const App = () => <div id="root"><span/></div>;
        export default App;
    "#;
    let audit = TypeScriptParserAudit::audit_code(code).unwrap();
    // TSX使用のため、grammar_nodesにJSX系が入る可能性を確認
    assert!(audit.grammar_nodes.contains_key("jsx_element"));
    assert!(audit.grammar_nodes.contains_key("jsx_self_closing_element"));
}
```
- 統合テスト
  - 実ファイルを用い、`audit_file`→`generate_report`の一連動作を検証。
  - CIでレポートをアーティファクト化し、閾値（例: 実装済み率）を満たさない場合に警告。

## Refactoring Plan & Best Practices

- 未使用インポート削除
  - `use crate::parsing::NodeTracker;`を削除。
- `grammar_nodes`の構造拡張
  - `HashMap<String, NodeStats>`（idと回数）に拡張し、回数カウントを`discover_nodes`内で増加。
```rust
struct NodeStats { id: u16, count: u32 }

fn discover_nodes(node: Node, registry: &mut HashMap<String, NodeStats>) {
    let key = node.kind().to_string();
    let id = node.kind_id();
    registry.entry(key)
        .and_modify(|s| s.count += 1)
        .or_insert(NodeStats { id, count: 1 });

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_nodes(child, registry);
    }
}
```
- レポートの抽象化
  - レンダラ（Markdown/JSON）を差し替え可能にし、機械可読な形式（JSON）出力を併設。
- エラー詳細の拡充
  - `LanguageSetup(String)`に言語名やヒントを含める。
- API整備
  - `audit_code_with_lang(code, LangChoice)`でTS/TSX選択を可能に。
- 安全なファイル読み込み
  - 上位層で許可ディレクトリのチェック、パス正規化を行うユーティリティを導入。
- 安定識別子でのシンボル種別管理
  - 列挙型や定数文字列へ移行し、デバッグ表現依存を回避。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 監査開始/終了、Tree-sitterパース時間、独自パーサ時間、検出されたノード種別数とギャップ数を`info`レベルで記録。
  - 失敗時の詳細（ファイルパス、言語設定エラー内容）を`error`レベルで記録。
- メトリクス
  - カウンタ: 解析ファイル数、ギャップ数、未出数。
  - ヒストグラム: パース時間、レポート生成時間、ASTノード数。
- トレーシング
  - `audit_code`の主要ステップ（言語設定、パース、AST走査、独自パーサ解析、レポート整形）にspanを付与。
- 出力制御
  - レポートに「実装済み率」を追加（例: implemented_nodes ∩ grammar_nodes / grammar_nodes の割合）。

このチャンクのコードにはロギング/メトリクス/トレーシングは実装されていないため、上記は提案。

## Risks & Unknowns

- Unknowns
  - `TypeScriptParser::get_handled_nodes`の戻り型詳細やノード追跡の仕組みはこのチャンクには現れない。
  - `SymbolCounter`の役割と`parse`との関係（重複制御/統計等）は不明。
  - `TypeScriptParser::parse`が返す`symbols`の型詳細は不明（`kind`フィールドの存在のみ推測可能）。
- Risks
  - 任意パス読み込みによるセキュリティリスク（上位層での対策が必要）。
  - 大規模コードでのメモリ/時間増加（AST全走査）。
  - `FileId(1)`固定に起因する意味衝突。
  - TSX固定言語設定によりTSのみのプロジェクトで不要な複雑性が入りうる。

以上により、この監査モジュールはシンプルかつ有用だが、観測性・設定柔軟性・頻度情報の導入でさらに実用度が高まる。Rustの安全性観点では問題が少なく、エラー設計も`thiserror`で明確。並行性の導入は未対応だが、各解析を独立タスク化する設計で容易に拡張可能。

---

Rust特有の観点（詳細チェックリスト）

- メモリ安全性
  - 所有権: 文字列`code`は借用で渡され、`symbols`や各`HashMap/HashSet`は関数内で所有→返却時に構造体へムーブ。（行番号不明）
  - 借用: `set_language(&language)`など参照借用のみ。可変借用は`parser`や`cursor`に限定。
  - ライフタイム: 明示的パラメータ不要。返却するのは所有データ。
- unsafe境界
  - unsafeブロックなし。（行番号不明）
- 並行性・非同期
  - Send/Sync: この構造体は`HashMap/HashSet/String`のみで構成され、通常はSend/Syncに問題なし（外部`TypeScriptParser`は実行中のみ）。
  - データ競合: 共有可変状態なし。単一スレッド前提。
  - await境界/キャンセル: 非同期未使用。
- エラー設計
  - Result vs Option: 失敗は`Result`で詳細化、パース不可は`AuditError::ParseFailure`。`Option`はTree-sitterのインターフェースに由来し`None`→`Err`へ変換。
  - panic箇所: `unwrap`/`expect`はテスト以外では使用せず、実コードは全て`Result`で扱う。
  - エラー変換: `std::io::Error`は`From`で`FileRead`に変換（`#[from]`）。その他は手動で`String`へ整形。