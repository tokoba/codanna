# parsing\php\parser.rs Review

## TL;DR

- 本ファイルは **tree-sitter-php (ABI-14)** を用いた **PHPコードの構文木解析**と、関数/クラス/定数/グローバル変数などの**シンボル抽出**を行うコアモジュール
- 公開APIは **PhpParser::new**, **PhpParser::parse**, および `LanguageParser`/`NodeTracker` の **トレイトメソッド群**（呼び出し追跡・型使用・定義抽出・import解析など）
- コアロジックは `extract_symbols_from_node` の **大規模分岐**で種類別に処理関数へ委譲し、**ParserContext** でスコープ/親コンテキストを管理
- 重大リスク: **find_method_calls がメソッド呼び出しを抽出しない不具合**、**UTF-8境界での文字列スライスが panic する可能性**、**implements/extends の誤認識の可能性**
- セキュリティ/安全性: 外部入力の実行はなしだが、**文字列スライス境界**・**Naiveなドキュメントコメント検出**・**不完全な import/define 解析**は耐性不足
- 推奨改善: メソッド呼び出し解析の実装追加、`process_constant` のコンテキスト設定、`extract_doc_comment` の頑健化、`extract_variable_types` の対応拡充、文字抽出の **utf8_text** 利用

## Overview & Purpose

このモジュールは、PHPソースコードから以下を抽出するための **静的解析パーサ**です。

- シンボル抽出: **関数**, **メソッド**, **クラス**, **インターフェース**, **トレイト**, **フィールド**, **定数**, **グローバル変数**
- 関係抽出: **関数呼び出し**, **型使用（引数/戻り値/プロパティ）**, **型内メソッド定義**, **import/use/require/include**
- 補助情報: **シグネチャ**, **ドキュメントコメント**, **スコープコンテキスト**, **位置情報 Range**

ASTは **tree-sitter-php 0.23.4 (ABI-14)** を利用して構築します。スコープ/親コンテキストは `ParserContext` を介して管理し、再帰解析の安全性は `check_recursion_depth` によって担保されます。抽出結果は `Symbol` のベクタとして返されます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | PhpParser | pub | Tree-sitterパーサを保持し、PHPコードからシンボルと関係を抽出 | High |
| Enum | PhpParseError | pub | PHPパーサ固有のエラー種別（初期化エラー、構文エラー、型注釈エラー等） | Low |
| Trait Impl | impl LanguageParser for PhpParser | crate公開（トレイト経由） | 解析系の高レベルAPI（find_calls, find_uses, find_imports等）を提供 | Med |
| Trait Impl | impl NodeTracker for PhpParser | crate公開（トレイト経由） | 解析済みノード種別のトラッキング | Low |
| 関数群 | process_* / extract_* | private | ノード種別ごとのシンボル/関係抽出の具体実装 | High |

### Dependencies & Interactions

- 内部依存
  - `ParserContext`（スコープ管理: enter_scope/exit_scope, current_class/function の設定・復元）
  - `NodeTrackingState`（解析済みノードの記録）
  - `SymbolCounter`（ID採番）
  - `check_recursion_depth`（再帰安全性）
  - `Symbol`, `Range`, `SymbolKind`（抽出成果物のデータ構造）
- 外部依存（推奨表）

| クレート/モジュール | 用途 |
|--------------------|------|
| tree_sitter | AST生成・ノード走査 |
| tree_sitter_php | PHP言語定義（LANGUAGE_PHP） |
| thiserror | エラー型定義（派生） |
| std::any::Any | トレイトオブジェクト変換（as_any） |

- 被依存推定
  - コードベースインテリジェンスの上位コンポーネント（言語ごとのパーサ管理）、シンボルインデクサ、参照解析器、ナビゲーション/検索機能

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PhpParser::new | `pub fn new() -> Result<Self, PhpParseError>` | PHPパーサの初期化 | O(1) | O(1) |
| PhpParser::parse | `pub fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | ソースからシンボル抽出 | O(n) | O(m) |
| LanguageParser::parse | `fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | トレイト経由の委譲 | O(n) | O(m) |
| LanguageParser::find_calls | `fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 関数呼び出し抽出（関数/メソッド内の関数呼び出しのみ） | O(n) | O(k) |
| LanguageParser::find_method_calls | `fn find_method_calls(&mut self, code: &str) -> Vec<MethodCall>` | メソッド呼び出し抽出（現状: 関数呼び出しを流用するバグあり） | O(n) | O(k) |
| LanguageParser::find_implementations | `fn find_implementations<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | クラスが実装するインターフェース関係抽出 | O(n) | O(k) |
| LanguageParser::find_uses | `fn find_uses<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 型使用（パラメータ型・戻り値型・プロパティ型）抽出 | O(n) | O(k) |
| LanguageParser::find_defines | `fn find_defines<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | クラス/インターフェース/トレイト内メソッド定義抽出 | O(n) | O(k) |
| LanguageParser::find_imports | `fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import>` | use/require/include の解析 | O(n) | O(k) |
| LanguageParser::language | `fn language(&self) -> Language` | 言語種別の返却（Php） | O(1) | O(1) |
| LanguageParser::find_variable_types | `fn find_variable_types<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | パラメータの型注釈抽出 | O(n) | O(k) |
| LanguageParser::extract_doc_comment | `fn extract_doc_comment(&self, node: &Node, code: &str) -> Option<String>` | 直前コメントからドキュメント抽出 | O(L) | O(L) |
| LanguageParser::as_any | `fn as_any(&self) -> &dyn Any` | トレイトオブジェクト化 | O(1) | O(1) |
| NodeTracker::get_handled_nodes | `fn get_handled_nodes(&self) -> &HashSet<HandledNode>` | 解析済ノード種別の参照 | O(1) | O(s) |
| NodeTracker::register_handled_node | `fn register_handled_node(&mut self, node_kind: &str, node_id: u16)` | 解析済ノード種別の登録 | O(1) | O(1) |

各APIの詳細:

1) PhpParser::new
- 目的と責務: tree-sitter の Parser に `LANGUAGE_PHP` を設定し、`ParserContext` と `NodeTrackingState` を初期化する
- アルゴリズム:
  - Parser::new を作成
  - set_language に失敗したら `PhpParseError::ParserInitFailed` を返す
- 引数:

| 名前 | 型 | 意味 |
|------|----|------|
| なし | - | - |

- 戻り値:

| 型 | 意味 |
|----|------|
| Result<PhpParser, PhpParseError> | パーサインスタンスまたは初期化失敗 |

- 使用例:
```rust
let mut parser = PhpParser::new().expect("PHP parser init failed");
```
- エッジケース:
  - tree-sitter-php がリンク不整合だと `ParserInitFailed`（詳細はエラーメッセージに含まれる）

2) PhpParser::parse
- 目的と責務: PHPソースからシンボルを抽出
- アルゴリズム:
  - `ParserContext::new()`でコンテキストリセット
  - `self.parser.parse(code, None)`でAST生成
  - ルートノードから `extract_symbols_from_node` で深さ優先再帰
- 引数:

| 名前 | 型 | 意味 |
|------|----|------|
| code | &str | PHPソース |
| file_id | FileId | ファイル識別子 |
| symbol_counter | &mut SymbolCounter | シンボルID採番 |

- 戻り値:

| 型 | 意味 |
|----|------|
| Vec<Symbol> | 抽出されたシンボル一覧 |

- 使用例:
```rust
let mut counter = SymbolCounter::new();
let symbols = parser.parse("<?php function f(){}", FileId(1), &mut counter);
```
- エッジケース:
  - AST生成が `None` の場合は空Vecを返す（構文エラーの詳細は返らない）

3) LanguageParser::find_calls
- 目的と責務: 関数/メソッド内からの「関数呼び出し」だけを抽出（メンバメソッド呼び出し `$obj->method()` は対象外）
- アルゴリズム:
  - ASTを構築し、`extract_calls_from_node` を再帰適用
  - `function_call_expression` のみを収集し、現在の関数/メソッド名をコンテキストとして格納
- 引数/戻り値:

| 引数 | 型 | 意味 |
|------|----|------|
| code | &str | PHPソース |

| 戻り値型 | 意味 |
|---------|------|
| Vec<(&str, &str, Range)> | (呼び出し元関数名, 呼び出し先関数名, 位置) |

- 使用例:
```rust
let calls = parser.find_calls("<?php function a(){ b(); }");
```
- エッジケース:
  - ネストされた呼び出し（引数内の関数呼び出し）も検出

4) LanguageParser::find_method_calls
- 目的と責務: メソッド呼び出し抽出（設計意図）
- 現状の実装: `find_calls` の結果を `MethodCall` にマッピングしているため、**メソッド呼び出しは収集されないバグ**（"member_call_expression" を取り扱うロジックなし）
- 使用例:
```rust
let mcalls = parser.find_method_calls("<?php $x->m();");
// 現状は空になる可能性が高い（バグ）
```
- エッジケース:
  - `$this->method()` や `ClassName::staticMethod()` を抽出しない

5) LanguageParser::find_implementations
- 目的: クラスが実装するインターフェース関係を抽出
- アルゴリズム:
  - `class_declaration` の `base_clause` を走査し、`name` をインターフェース名として追加
  - ⚠️ 実際の grammar では `implements` と `extends` の表現差異があり、**誤認の可能性**
- エッジケース:
  - 複数インターフェースの実装、名前空間付き名

6) LanguageParser::find_uses
- 目的: 型使用（パラメータ型、戻り値型、プロパティ型）の抽出
- アルゴリズム:
  - `typed_property_declaration` | `parameter_declaration` の `type` を収集
  - `function_definition` / `method_declaration` の `return_type` を収集
- エッジケース:
  - ユニオン型 (`type_list`)、nullable型（`?T`）の文字列収集は可能だが正規化はしない

7) LanguageParser::find_defines
- 目的: 型（クラス/インターフェース/トレイト）内のメソッド定義一覧を抽出
- アルゴリズム:
  - `declaration_list` 内の `method_declaration` を列挙

8) LanguageParser::find_imports
- 目的: `use` 宣言、`require/include` の文字列引数を抽出
- アルゴリズム:
  - `namespace_use_declaration` 内の `namespace_use_clause` を走査し、`qualified_name` と `namespace_aliasing_clause` を抽出
  - `require_expression` 等の第2子にある `string` をパスとして抽出
- エッジケース:
  - 非文字列（変数・式）引数の `require/include` は抽出されない

9) LanguageParser::language
- 目的: 言語種別を返す（Php）

10) LanguageParser::find_variable_types
- 目的: パラメータの変数名と型注釈を抽出
- アルゴリズム:
  - `simple_parameter` の子から `type_list|named_type|primitive_type` と `variable_name` を抽出
  - ⚠️ grammarにより `parameter_declaration` を使うことがあり、**抽出漏れの可能性**
- 使用例:
```rust
let vt = parser.find_variable_types("<?php function f(int $x): void {}");
```

11) LanguageParser::extract_doc_comment
- 目的: ノード直前の `comment` が `/**` または `//` で始まる場合に docstring として抽出
- エッジケース:
  - 直前にホワイトスペースや他ノードがあると検出できない

12) NodeTracker メソッド
- 目的: どのノード種別を解析したか（kind, kind_id）を記録し、後から参照可能にする

データ契約（このモジュールから見える利用フィールド）
- Symbol: `name`, `kind`, `file_id`, `range`, `signature`（任意）, `doc_comment`（任意）, `scope_context`（任意）
- Range: `start_line`, `start_column`, `end_line`, `end_column`
- Import: `path`, `alias`, `is_glob`, `file_id`, `is_type_only`

## Walkthrough & Data Flow

- 入力（&str ソース）→ `Parser::parse` → AST（root）→ `extract_symbols_from_node`（深さ優先）
- 各ノード種別を `match node.kind()` で分岐。該当する `process_*` 関数へ委譲し `Symbol` を生成
- スコープ管理: 関数/メソッド/クラス/インターフェース/トレイトに突入すると `enter_scope` で現在スコープを設定、子ノード解析後 `exit_scope`、その後**保存していた親コンテキストを復元**
- 関係抽出（calls/uses/defines/imports/...）は `LanguageParser` 実装の各 `find_*` で別途ASTを再帰走査

Mermaid（分岐が多いため使用）

```mermaid
flowchart TD
  A[extract_symbols_from_node] --> B{node.kind}
  B -->|function_definition| C[process_function + enter Function scope]
  C --> D[process_children]
  D --> E[exit_scope + restore context]
  B -->|method_declaration| F[process_method + enter Function scope]
  F --> D
  B -->|class_declaration| G[process_class + enter Class scope]
  G --> D
  B -->|interface_declaration| H[process_interface + enter Class scope]
  H --> D
  B -->|trait_declaration| I[process_trait + enter Class scope]
  I --> D
  B -->|property_declaration| J[process_property]
  B -->|const_declaration| K[process_children for const_element]
  B -->|const_element| L{is_global_scope?}
  L -->|Yes| M[emit Constant (global)]
  L -->|No| N[skip; class_const handled elsewhere]
  B -->|class_const_declaration| O[process_constant]
  B -->|expression_statement| P{child.kind}
  P -->|function_call_expression & define() & global| Q[process_define]
  P -->|assignment_expression & global| R[process_global_assignment]
  P -->|other| S[process_children]
  B -->|default| T[register + process_children]
```

上記の図は `extract_symbols_from_node` 関数における主要分岐を示す（行番号: 不明）。

## Complexity & Performance

- 時間計算量
  - 解析系（parse, find_*）: O(n)（n=ASTノード数）。tree-sitterパースも概ね線形。
- 空間計算量
  - 結果蓄積に応じて O(m)（m=抽出シンボル/関係数）
- ボトルネック
  - 再帰走査による全ノード探索。大規模ファイルでは深い再帰がコストに。
  - `&code[node.byte_range()]` による文字列スライスの頻発（UTF-8境界の検証がない）
- スケール限界/運用要因
  - 大ファイル・大量ファイル解析時は AST 構築メモリと結果ベクタが支配的
  - I/O/ネットワーク/DBは未関与（CPU/メモリ主体）
- メモ
  - `Vec::with_capacity` を各find_*で使用（32/16など）により再割当の抑制
  - 再帰深さは `check_recursion_depth` でガード（詳細はこのチャンクには現れない）

## Edge Cases, Bugs, and Security

セキュリティチェックリストに沿った評価:

- メモリ安全性
  - **文字列スライスのUTF-8境界**: `&code[node.byte_range()]` は**バイト境界依存**であり、境界が文字境界でない場合は Rust の `str` スライスで panic し得る（例: マルチバイト文字途中で切断）。tree-sitter は通常トークン境界で返すが、保証に依存。改善案: `Node::utf8_text` を利用して `Result<&str, Utf8Error>` を扱う。
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（Rustの安全機構、FFIはtree-sitterに依存）
- インジェクション
  - SQL/Command/Path traversal: 本モジュールは解析のみで実行はしないため直接的な脅威は低い
  - `include/require` の抽出は文字列リテラルのみ対応。式は未解析（安全面ではプラス）
- 認証・認可
  - 該当なし
- 秘密情報
  - Hard-coded secrets: 該当なし
  - Log leakage: テストの `debug_parse` が eprintln を使用（テスト限定）
- 並行性
  - `&mut self` を要求するため**同一インスタンスの並行使用は不可**。`tree_sitter::Parser` の Send/Sync 特性はこのチャンクでは不明。並列解析にはインスタンスをスレッドごとに分けるのが安全。
  - Race/Deadlock: 共有状態なし（`ParserContext` はインスタンス内に閉じている）
- エラー設計
  - `PhpParseError` は `new()` のみで利用。構文エラー時の詳細報告（`SyntaxError`）は未使用
  - 多数の `Option` による分岐で `None` 時はスキップ。`unwrap/expect` は使用していない

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| メソッド呼び出し抽出 | `$x->m();` | find_method_callsで抽出 | find_callsを流用しておりmember_call非対応 | Bug |
| define名が変数 | `define($n, 42);` | 定数未抽出（仕様決定次第） | 第1引数が文字列のみ処理 | Known Limitation |
| コメントに空白介在 | `/**a*/\n\nfunction f(){}` | docコメント抽出 | 直前の兄弟がcommentのみを対象 | Bug/Fragile |
| implements/extends識別 | `class C extends B implements I {}` | Iのみを実装として抽出 | `base_clause`のnameをすべて追加 | Risk |
| パラメータ型ノード差異 | `function f(int $x){}` | 変数型抽出 | `simple_parameter`のみ処理 | Bug/Incomplete |
| マルチ定数宣言 | `const A=1,B=2;` | 両方抽出 | `const_declaration`→子の`const_element`で対応 | OK |
| 文字列境界 | ソースに多バイト文字 | 安全な文字列抽出 | `&str`スライスで境界不一致panic可能性 | Risk |
| クラス定数のコンテキスト | `class C { const X=1; }` | scope_context付与 | `process_constant`で未設定 | Bug |

根拠（関数名:行番号）：行番号はこのチャンクでは不明のため、関数名のみ記載。

## Design & Architecture Suggestions

- **find_method_callsの修正**: `member_call_expression` および `scoped_call_expression`（静的呼び出し）を対象にした `extract_method_calls_from_node` を追加し、`find_method_calls` で使用する
- **文字列抽出の安全化**: `Node::utf8_text(code.as_bytes())` の利用に切り替え、UTF-8エラーをハンドリング。失敗時はフォールバックやサニタイズ
- **implements/extendsの厳密化**: grammarに従って `implements_list`（または相当ノード）を明確に識別。`base_clause`がextendsを含む場合は除外
- **docコメント抽出の頑健化**: 直前兄弟が `comment` でない場合でも、前方に連続する `comment` をスキップ可能なホワイトスペース/属性ノードを越えて探索（ただし誤付与防止のヒューリスティック必要）
- **型抽出の拡充**: `parameter_declaration` と `simple_parameter` の差異を吸収。ユニオン/nullable/ジェネリクス風表記の正規化（必要に応じて）
- **コンテキスト一貫性**: `process_constant` にも `scope_context` を付与し、クラス定数の親クラス情報を保持
- **構文エラー報告**: AST生成失敗時に `PhpParseError::SyntaxError` を返却可能なAPI（別メソッド）を設けるか、エラーイベントを発行
- **import解析強化**: `use function`, `use const`, グループuse、`as`の抽出ロジック（インデックス前提を避けフィールド名で判定）

## Testing Strategy (Unit/Integration) with Examples

優先すべきテスト観点と例:

- メソッド呼び出し抽出（現状バグの再現と修正確認）
```php
<?php
class C { function a() { $this->m( helper() ); } }
```
```rust
let calls = parser.find_calls(code);
assert!(calls.iter().all(|(_, name, _)| *name != "m")); // 関数呼び出しのみ
let mcalls = parser.find_method_calls(code);
assert!(mcalls.iter().any(|c| c.target_name == "m")); // 修正後に期待
```

- docコメント抽出の空白介在
```php
<?php
/** Doc */
/**

*/ // 空行など介在
function f() {}
```
```rust
let symbols = parser.parse(code, FileId(1), &mut SymbolCounter::new());
let f = symbols.into_iter().find(|s| s.name.as_ref() == "f").unwrap();
assert!(f.doc_comment.is_some(), "空白介在でもdocを検出");
```

- implements/extends 識別
```php
<?php
class C extends B implements I, J {}
```
```rust
let impls = parser.find_implementations(code);
assert!(impls.iter().any(|(_, iface, _)| *iface == "I"));
assert!(impls.iter().any(|(_, iface, _)| *iface == "J"));
// extends B は実装として含まないこと
assert!(!impls.iter().any(|(_, iface, _)| *iface == "B"));
```

- 変数型抽出（parameter_declaration/simple_parameter 両対応）
```php
<?php
function f(int $x, ?string $y) {}
```
```rust
let vts = parser.find_variable_types(code);
assert!(vts.iter().any(|(var, typ, _)| *var == "x" && *typ == "int"));
assert!(vts.iter().any(|(var, typ, _)| *var == "y" && *typ == "?string"));
```

- クラス定数のスコープコンテキスト
```php
<?php
class C { const X = 1; }
```
```rust
let symbols = parser.parse(code, FileId(1), &mut SymbolCounter::new());
let x = symbols.into_iter().find(|s| s.name.as_ref() == "X").unwrap();
assert!(x.scope_context.is_some());
```

- UTF-8境界耐性（改善後）
```php
<?php
function ｆ() {} // 全角
```
```rust
// utf8_textに切り替え後、panicなく抽出できることを確認
```

## Refactoring Plan & Best Practices

- 段階的改善
  1. **find_method_calls 実装**追加とテスト整備（最優先）
  2. **文字列抽出の安全化**（utf8_text活用）と影響範囲の回帰テスト
  3. **process_constant の scope_context 付与**（一貫性）
  4. **implements/extends 識別の修正**（grammar準拠）
  5. **docコメント抽出の拡張**（頑健化）
  6. **variable_types のノード対応拡充**
- ベストプラクティス
  - AST子取得は**フィールド名**優先（`child_by_field_name`）、インデックス直接参照は最小化
  - 再帰は**早期ガード**（`check_recursion_depth`）と**明確なスコープenter/exit**でコンテキスト漏れ防止
  - `Option`の分岐は**パターンマッチ**で明確化し、`None`時の仕様をドキュメント化
  - シンボルの `signature` は**本体除外**（既存方針）で要約を保つ

## Observability (Logging, Metrics, Tracing)

- ログ
  - 公開APIでのエラー（初期化失敗）時は `PhpParseError` で十分だが、**構文エラー**時の情報は**返却していない**。必要なら `SyntaxError` を用いて報告するAPIを追加
- メトリクス
  - `NodeTracker` で解析済みノード種別が収集可能。解析カバレッジや未知ノードの割合をメトリクス化
  - シンボル/関係の件数、ASTノード総数、再帰深さ上限到達回数など
- トレーシング
  - 大規模ファイルでのパフォーマンス診断のため、主要関数開始/終了のスパン計測（feature-gated）を検討

## Risks & Unknowns

- tree-sitter-php の**ノード種別名**（例: `base_clause` vs `implements_list`）の正確性はこのチャンクでは検証不能。アップグレード時の互換性リスクあり（ファイル先頭コメントにも注意書きあり）
- `tree_sitter::Parser` の **Send/Sync 特性**は不明。並列解析の設計には要確認
- UTF-8境界に関する **Node byte_range の保証**は外部ライブラリに依存
- `extract_imports_from_node` の **子インデックス前提**は grammar差異で脆弱。フィールド名での抽出に改修が必要

以上の通り、このファイルはPHP解析の中核であり、公開APIは充実しているものの、メソッド呼び出し抽出や文字列処理の安全性など、いくつかの重要な改善点が存在します。