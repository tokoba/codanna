# parser.rs Review

## TL;DR

- 目的: **Tree-sitter**ベースのRustコード解析器。構文木から**シンボル抽出**、**import抽出**、**呼び出し関係**、**実装関係**、**型使用**、**メソッド定義**、**変数型推定**を提供
- 主な公開API: `RustParser::parse`, `extract_imports`, `find_calls`, `find_method_calls`, `find_implementations`, `find_uses`, `find_defines`, `find_inherent_methods`, `find_variable_types`, `new`, `with_debug`
- 複雑箇所: `extract_symbols_from_node`の多分岐・スコープ管理、`find_method_calls_in_node`の受け手推定、`extract_full_type_name`の再帰的型生成
- 重大リスク: `&str`の**バイト範囲スライス**によるパニック可能性、**パース失敗時の沈黙**（空ベクトル返却）、**ドキュメントコメント抽出の不整合**（シンボルへの格納は外部のみ、APIは外部+内部を結合）
- 安全性: `unsafe`未使用、**再帰深度ガード**あり、並行性は**可変参照ベース**でシングルスレッド前提
- 設計改善: **UFCSでの明示呼び出し**、**Result返却**によるエラー設計強化、**UTF-8安全な抽出**（`utf8_text`）への統一、**Visibilityの詳細化**（`pub(crate)`, `pub(super)`など）

## Overview & Purpose

このファイルは、Tree-sitterのRust文法（ABI-15、`tree-sitter-rust 0.24.0`）を用いた**Rust言語解析器**の実装です。構文木を手動走査し、以下の機能を提供します。

- シンボル抽出（関数、メソッド、構造体、列挙型、型エイリアス、定数、static、トレイト、モジュール、マクロ）
- import文抽出（単一/スコープ付き/エイリアス/グロブ/グループ）
- 呼び出し関係（関数呼び出し、メソッド呼び出し、静的メソッド）
- トレイト実装関係（`impl Trait for Type`）
- 型使用（構造体フィールド型、関数パラメータ/戻り値）
- メソッド定義（`impl`ブロック内のメソッド、トレイト内のメソッド宣言）
- 変数の型推定（構造体リテラル、識別子、`Type::new()`等の静的メソッドからの推定）

設計上のトレードオフとして、**ゼロコスト**志向と**安定API**のバランスを取り、現在は`Vec<T>`ベースの結果返却として実装されています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | RustParser | pub | Tree-sitterのRust言語設定、解析API群、コンテキスト/スコープ管理、ノードトラッキング | High |
| Enum | DocCommentType | private | ドキュメントコメント分類（外部/内部、ライン/ブロック） | Low |
| Trait Impl | LanguageParser for RustParser | public (trait経由) | 汎用解析インターフェース準拠（parse, find_* 等の委譲/拡張） | Med |
| Trait Impl | NodeTracker for RustParser | public (trait経由) | 取り扱ったノード種別の記録 | Low |
| Helper Macro | debug_print! | private | `self.debug`で制御されるデバッグ出力 | Low |
| Context | ParserContext | 外部依存 | スコープ（関数/クラス）管理、親コンテキスト保存/復元 | Med |
| State | NodeTrackingState | 外部依存 | ハンドリング済みノード集合の管理 | Low |

### Dependencies & Interactions

- 内部依存
  - `extract_symbols_from_node` → `create_symbol`, `extract_*_signature`, `extract_type_name`
  - `find_method_calls_in_node` → `find_containing_function`, `MethodCall::new`
  - `find_implementations_in_node` → `extract_type_name`
  - `find_uses_in_node` → `extract_type_name`
  - `find_variable_types_in_node` → `extract_variable_name`, `extract_value_type`
  - `extract_doc_comment` → `extract_doc_comments` + `extract_inner_doc_comments`

- 外部依存（推奨表）
  | 依存 | 役割 |
  |------|------|
  | tree_sitter::{Parser, Node} | パースとASTノード取得 |
  | tree_sitter_rust::LANGUAGE | Rust文法設定 |
  | crate::parsing::{LanguageParser, ParserContext, ScopeType, NodeTracker, NodeTrackingState, HandledNode} | 汎用解析トレイト・スコープ管理・ノード追跡 |
  | crate::parsing::Import | import表現 |
  | crate::parsing::method_call::MethodCall | メソッド呼び出し表現（受け手含む） |
  | crate::{Symbol, SymbolKind, SymbolCounter, Range, FileId} | シンボル/カウンタ/位置/ファイルID |

- 被依存推定
  - 検索/インデクシング機能（呼び出しグラフ、型参照マップ）
  - ドキュメント生成（docコメント取得）
  - 言語横断パーサー管理（`LanguageParser`トレイトを通じて）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | `pub fn new() -> Result<Self, String>` | 解析器生成（デバッグ無） | O(1) | O(1) |
| with_debug | `pub fn with_debug(debug: bool) -> Result<Self, String>` | デバッグ有無指定で生成 | O(1) | O(1) |
| parse | `pub fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | AST歩査によるシンボル抽出 | O(N) | O(S) |
| extract_imports | `pub fn extract_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import>` | use宣言抽出 | O(N) | O(I) |
| find_calls | `pub fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 関数/メソッド呼び出し（識別子/フィールド/スコープ付） | O(N) | O(C) |
| find_method_calls | `pub fn find_method_calls(&mut self, code: &str) -> Vec<MethodCall>` | 受け手付きメソッド呼び出し抽出（拡張） | O(N) | O(M) |
| find_implementations | `pub fn find_implementations<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | `impl Trait for Type`関係抽出 | O(N) | O(E) |
| find_uses | `pub fn find_uses<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 型使用（構造体フィールド、関数params/return） | O(N) | O(U) |
| find_defines | `pub fn find_defines<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | トレイトおよびimpl内メソッド定義抽出 | O(N) | O(D) |
| find_inherent_methods | `pub fn find_inherent_methods(&mut self, code: &str) -> Vec<(String, String, Range)>` | トレイトなし`impl`のメソッド抽出 | O(N) | O(H) |
| find_variable_types | `fn find_variable_types<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` (LanguageParser経由はpub相当) | `let`束縛からの型推定 | O(N) | O(B) |
| find_imports (trait) | `fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import>` | `extract_imports`委譲 | O(N) | O(I) |
| language | `fn language(&self) -> Language` | 言語種別（Rust）返却 | O(1) | O(1) |
| extract_doc_comment (trait) | `fn extract_doc_comment(&self, node: &Node, code: &str) -> Option<String>` | 外部+内部docコメント結合 | O(K) | O(D) |

以下、主要APIの詳細。

1) parse
- 目的と責務
  - ファイル単位でASTを構築し、トップレベルから**シンボル**（関数/メソッド/型等）を抽出
  - スコープ管理（関数/クラス=trait/impl/struct）と**親コンテキスト**の設定/復元
- アルゴリズム（抽象）
  - `Parser::parse`でツリー取得（失敗で空ベクトル）
  - `extract_symbols_from_node(root, ...)`を再帰呼び出し
  - 分岐ごとに`create_symbol`、署名抽出、子ノード走査、スコープenter/exit
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析対象コード |
  | file_id | FileId | ファイル識別子 |
  | symbol_counter | &mut SymbolCounter | シンボルID採番 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<Symbol> | 抽出されたシンボル一覧 |
- 使用例
  ```rust
  let mut parser = RustParser::new().unwrap();
  let file_id = FileId::new(1).unwrap();
  let mut counter = SymbolCounter::new();
  let symbols = parser.parse("fn add(a:i32,b:i32)->i32{a+b}", file_id, &mut counter);
  ```
- エッジケース
  - パース失敗（`Parser::parse`がNone）→ 空Vec返却
  - 深い再帰→ `check_recursion_depth`で中断（extract_symbols_from_node:行番号 不明）

2) extract_imports
- 目的
  - `use`宣言ノードから**importパス/エイリアス/グロブ**を抽出
- アルゴリズム
  - `use_declaration`→`argument`→各種ノード（identifier/scoped_identifier/use_as_clause/use_wildcard/use_list/scoped_use_list）を処理
  - グループは親の`scoped_use_list`の`path`をprefixとして結合
- 引数/戻り値
  | 引数 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析対象コード |
  | file_id | FileId | ファイル識別子 |
  | 戻り値 | Vec<Import> | importの一覧 |
- 使用例
  ```rust
  let imports = parser.extract_imports("use std::io::{Read, Write};", file_id);
  ```
- エッジケース
  - `use foo::*;`→ `is_glob=true`
  - ネスト/複雑なグループ（`use {self, super::*}`等）は*未対応の可能性*（extract_import_from_node:行番号 不明）

3) find_calls
- 目的
  - 関数呼び出し（識別子、フィールド式、スコープ付き識別子）を抽出し、呼び出し元関数名と対象名を返す
- アルゴリズム
  - `call_expression`の`function`フィールドのkind分岐（identifier/field_expression/scoped_identifier）
  - `find_containing_function`で呼び出し元関数名取得
- 引数/戻り値
  | 引数 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析対象コード |
  | 戻り値 | Vec<(&str, &str, Range)> | (caller, target, range) |
- 使用例
  ```rust
  let calls = parser.find_calls("fn a(){b();}\nfn b(){}",);
  ```
- エッジケース
  - メソッド呼び出し→ターゲットはメソッド名のみ（`field_expression`扱い）
  - スコープ付き（`Type::method`）→完全修飾の文字列（scoped_identifier）を返す

4) find_method_calls
- 目的
  - 受け手（`self`, 識別子, チェーン, 型）付きの**メソッド呼び出し**を高精度に抽出
- アルゴリズム
  - `call_expression`→ `function`のkindにより分岐
    - `identifier`→関数呼び出し
    - `field_expression`→`value`から受け手テキスト抽出
    - `scoped_identifier`→`Type::method`分離、`static_method()`フラグ
- 引数/戻り値
  | 引数 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析対象コード |
  | 戻り値 | Vec<MethodCall> | caller/method/receiver/range |
- 使用例
  ```rust
  let calls = parser.find_method_calls("fn main(){let v=Vec::new();v.push(1);}");
  ```
- エッジケース
  - 受け手が複雑な式（`self.field.method()`）→受け手文字列をそのまま格納（find_method_calls_in_node:行番号 不明）

5) find_implementations
- 目的
  - `impl Trait for Type`ブロック検出
- アルゴリズム
  - `impl_item`で`trait`と`type`フィールドを抽出→`extract_type_name`で表示名取得
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(&str, &str, Range)> | (type_name, trait_name, range) |

6) find_uses
- 目的
  - 構造体フィールドの型、関数パラメータ/戻り値の型を抽出
- アルゴリズム
  - `struct_item`の`field_declaration`から`type`取得
  - `function_item`の`parameters`と`return_type`を走査
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(&str, &str, Range)> | (context_name, type_name, range) |

7) find_defines
- 目的
  - トレイト内のメソッドシグネチャ、`impl`内のメソッド定義の抽出
- アルゴリズム
  - `trait_item`の`body`中の`function_signature_item`/`function_item`
  - `impl_item`の`body`中の`function_item`
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(&str, &str, Range)> | (definer_name, method_name, range) |

8) find_inherent_methods
- 目的
  - トレイトなしの`impl Type { ... }`に定義されたメソッドのみ抽出
- アルゴリズム
  - `impl_item`で`trait`フィールドがNoneなら対象、`type`から`extract_full_type_name`で完全型文字列生成
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(String, String, Range)> | (type_name, method_name, range) |

9) find_variable_types
- 目的
  - `let`束縛の**推測可能な型**（構造体リテラル/識別子/`Type::method()`）を抽出
- 制約
  - 参照型（`&T`）や複合/関数戻り値からの推定は*スキップ*
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(&str, &str, Range)> | (var_name, type_name, range) |

10) extract_doc_comment（LanguageParser）
- 目的
  - 外部（`///`, `/** ... */`）＋内部（`//!`, `/*! ... */`）ドキュメントの結合
- 注意
  - `create_symbol`は外部コメントのみ使用しており、シンボル格納では内部コメントが含まれない不整合あり（create_symbol:行番号 不明）

## Walkthrough & Data Flow

- フロー概要
  - `new/with_debug` → Tree-sitterにRust言語設定
  - `parse(code)` → `parser.parse(code, None)` → `extract_symbols_from_node(root, ...)`（全ノード再帰走査）
    - 種別ごとにシンボル生成（`create_symbol`）
    - 署名抽出（`extract_signature`等）→ `Symbol.with_signature`
    - スコープ管理（`ParserContext.enter_scope/exit_scope`）
    - フィールド/メソッド/トレイト内シグネチャなども抽出
  - `find_calls`/`find_method_calls` → `call_expression`走査
  - `find_implementations` → `impl_item`走査
  - `find_uses` → `struct_item`および`function_item`走査
  - `find_defines` → `trait_item`/`impl_item`走査
  - `find_variable_types` → `let_declaration`から推定

- スコープ/コンテキスト
  - 関数/クラス（`ScopeType::Function`, `ScopeType::Class`）のenter/exitで親コンテキスト（現在の関数名/クラス名）を一時保存し、終了時に復元。これによりネストした定義でも**親名のトラッキング**が維持されます（extract_symbols_from_node:行番号 不明）。

- Range生成・文字列抽出
  - 構文ノードの`start_position/end_position`から`Range::new`を生成
  - 名前・型・パスの抽出は`code[node.byte_range()]`または`utf8_text`により実施

### Mermaid: extract_symbols_from_node の主要分岐とスコープ制御

```mermaid
flowchart TD
    A[extract_symbols_from_node(node)] --> B{node.kind()}
    B -->|function_item| F1[set kind=Function/Method<br/>create_symbol<br/>extract_signature<br/>enter Function scope<br/>set current_function<br/>recurse children<br/>exit scope<br/>restore context]
    B -->|struct_item| S1[create_symbol(kind=Struct)<br/>extract_struct_signature<br/>enter Class scope<br/>set current_class<br/>extract fields (Field)<br/>recurse children<br/>exit scope<br/>restore context]
    B -->|enum_item| E1[create_symbol(kind=Enum)<br/>extract_enum_signature<br/>extract enum_variant as Constant]
    B -->|type_item| T1[create_symbol(kind=TypeAlias)<br/>extract_type_alias_signature]
    B -->|const_item| C1[create_symbol(kind=Constant)<br/>extract_const_signature]
    B -->|static_item| C2[create_symbol(kind=Constant)<br/>extract_const_signature]
    B -->|trait_item| TR1[create_symbol(kind=Trait)<br/>extract_trait_signature<br/>enter Class scope<br/>extract method signatures (Method)<br/>exit scope]
    B -->|impl_item| I1[enter Class scope<br/>set current_class=impl type<br/>recurse children<br/>exit scope<br/>restore context]
    B -->|mod_item| M1[create_symbol(kind=Module)<br/>recurse children (skip 'identifier')]
    B -->|macro_definition| MD1[create_symbol(kind=Macro)]
    B -->|その他| R[recurse children]
```

上記の図は`extract_symbols_from_node`関数（行番号:不明）の主要分岐を示す。

## Complexity & Performance

- 解析処理のビッグオー
  - `parse`, `find_calls`, `find_method_calls`, `find_implementations`, `find_uses`, `find_defines`, `find_inherent_methods`, `find_variable_types`: すべて**ASTノード数Nに対してO(N)**（完全走査）
  - 空間計算量は抽出件数（シンボルS、呼び出しC、実装E、使用U等）に比例

- ボトルネック
  - 再帰走査＋多分岐（`extract_symbols_from_node`）による**分岐密度**と**文字列抽出**がCPU/メモリコスト
  - `code[node.byte_range()]`による**UTF-8境界検証**が実行時コスト＋潜在パニックリスク

- スケール限界
  - 非lazyな`Vec`返却により**一次メモリ使用増大**
  - `String`の生成（署名、型名）により**アロケーション**が増加
  - 大規模ファイルでは`find_*`群の複数パス走査により**合計O(N)×関数数**（ただしAST再構築は各関数で1回）

- 実運用負荷要因
  - I/O: ファイル読み込みは外部（テストで`std::fs::read_to_string`）
  - ネットワーク/DB: 該当なし
  - CPU: 再帰走査＋文字列操作中心
  - メモリ: Vec累積・署名文字列・Docコメント結合

## Edge Cases, Bugs, and Security

- メモリ安全性
  - 所有権/借用: メソッドは`&mut self`中心で**単一スレッド前提の内部状態**安全管理（Rust準拠）
  - `unsafe`: 使用なし（ファイル全体）
  - パニックリスク: `code[node.byte_range()]`で**UTF-8境界外スライス**が起きるとパニックの可能性（複数箇所：`extract_import_from_node`, `create_symbol`, `find_calls_in_node`等、行番号:不明）
  - 整数オーバーフロー: 位置変換で`as`キャスト（row→u32, col→u16）は安全（Tree-sitterの行/列は小さめ、ただし極端に長い行はu16に収まらない可能性は理論上あり）

- インジェクション
  - SQL/Command/Path traversal: 該当なし（解析専用）
  - コメント解析はテキスト処理のみ

- 認証・認可
  - 該当なし

- 秘密情報
  - Hard-coded secrets: なし
  - Log leakage: `debug_print!`は`eprintln!`で出力、機密は扱わないが**大量出力**によるノイズの可能性

- 並行性
  - `Parser`と`RustParser`は`&mut self`を要求、**非Sync/非Send前提**（トレイト境界はファイルからは不明）
  - グローバル共有状態なし、**データレース不在**
  - 非同期/await: 該当なし
  - キャンセル: 該当なし

- エラー設計
  - `new/with_debug`は`Result<Self, String>`で言語設定失敗を返す
  - その他解析系は**失敗時に空Vec返却**（`Parser::parse`がNone）→ 失敗の区別ができない
  - `unwrap/expect`: テストで使用、本体は使用なし

### Edge Cases詳細化

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 非ASCII識別子の抽出 | `fn 你好(){}` | 正しく名前抽出、パニックなし | `code[node.byte_range()]` | 潜在的に危険（UTF-8境界） |
| パース失敗 | 破損入力 | エラー返却 or 明示通知 | 空Vec返却 | 改善余地あり |
| useグループの複雑形 | `use {self, super::*, crate::x};` | すべて抽出 | 限定的処理 | 未対応の可能性 |
| 参照型の推定 | `let r = &x;` | `&T`推定 | スキップ | 仕様通り（ゼロコスト方針） |
| 複合式から型推定 | `let y = Some(x);` | `Option<T>`等 | スキップ | 仕様通り |
| トレイト内メソッドの抽出 | `trait T { fn a(); }` | `a`抽出 | 対応 | 良好 |
| 内部docコメントのシンボル格納 | `struct S { //! inner }` | シンボルdocに含む | `create_symbol`は外部のみ | 不整合 |

## Design & Architecture Suggestions

- エラー設計の強化
  - `parse/find_*`系の戻り値を`Result<Vec<_>, ParseError>`へ拡張し、**パース失敗**と**結果なし**を区別
- 文字列抽出の安全化
  - `code[node.byte_range()]`の直接スライスを**`node.utf8_text(code.as_bytes())`**へ統一（`Result<&str, _>`）
  - 必要な場合のみ`to_string`、それ以外は借用（`&str`）で保持
- ドキュメントコメントの一貫性
  - `create_symbol`でも`extract_doc_comment`（外部+内部統合）を利用し、**シンボルdoc**を完全化
- Visibilityの詳細化
  - `visibility_modifier`の内容に応じて`pub(crate)`, `pub(super)`, `pub(in X)`を表現（`crate::Visibility`拡張）
- トレイト同名メソッドの委譲明示
  - `impl LanguageParser for RustParser`内の`parse`で`RustParser::parse(self, ...)`のUFCS使用により**自己再帰誤用防止**と**明瞭化**
- 将来のゼロコスト移行
  - `Vec<T>`から`impl Iterator<Item=T>`へ段階的移行
  - 生成器/状態機械による**遅延トラバース**

## Testing Strategy (Unit/Integration) with Examples

- 既存テストは広範（関数/構造体/インポート/呼び出し/使用/定義/実装/ドキュメント/可視性/型推定）。追加推奨:

1) 非ASCII/UTF-8境界テスト
```rust
#[test]
fn test_utf8_identifier() {
    let mut parser = RustParser::new().unwrap();
    let code = "fn 你好(a: i32) { }";
    let file_id = FileId::new(1).unwrap();
    let mut counter = SymbolCounter::new();
    let symbols = parser.parse(code, file_id, &mut counter);
    assert!(symbols.iter().any(|s| s.name.as_ref() == "你好"));
}
```

2) 複雑useグループ
```rust
#[test]
fn test_complex_use_group() {
    let mut parser = RustParser::new().unwrap();
    let file_id = FileId::new(1).unwrap();
    let code = "use {self, super::*, crate::x::y};";
    let imports = parser.extract_imports(code, file_id);
    // 期待値の定義（現状は不明/未対応ならfailしないようにコンディション緩め）
    assert!(!imports.is_empty());
}
```

3) パース失敗の顕在化
```rust
#[test]
fn test_parse_failure() {
    let mut parser = RustParser::new().unwrap();
    // 極端な未閉ブロックなど、tree-sitterがNoneを返し得るケースを模索（正確な例は不明）
    let code = "fn a( {";
    let file_id = FileId::new(1).unwrap();
    let mut counter = SymbolCounter::new();
    let symbols = parser.parse(code, file_id, &mut counter);
    // 現仕様では空Vec、将来的にはResultで検知したい
    assert!(symbols.is_empty());
}
```

4) 内部docコメントのシンボルへの反映（改善後前提）
```rust
#[test]
fn test_symbol_doc_includes_inner() {
    // 改修後: create_symbolがextract_doc_commentを使用
    // 現状は不整合のため、このテストは改修後に追加
}
```

## Refactoring Plan & Best Practices

- 短期
  - `impl LanguageParser for RustParser::parse`内をUFCSに変更
    ```rust
    fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol> {
        RustParser::parse(self, code, file_id, symbol_counter)
    }
    ```
  - `create_symbol`でのdoc抽出を`extract_doc_comment`に切替（外部+内部）
  - 文字列抽出で`utf8_text`を優先し、失敗時は安全にスキップ
- 中期
  - Visibility詳細化（`crate::Visibility`の拡張）
  - `Result`導入（`ParseError`型定義、失敗の顕在化）
  - 型推定の拡充（参照型・ジェネリクスの簡易処理）
- 長期
  - `Iterator`ベースAPIへの移行
  - メソッド呼び出し解析と**型解決**の連携（`find_variable_types`結果を用いたレシーバ型の照合）

## Observability (Logging, Metrics, Tracing)

- ログ
  - `debug_print!`で`eprintln!`出力。構造化ログ（例: `log`クレート）へ移行し、**カテゴリ/レベル**管理を推奨
  - 主要イベント（パース開始/終了、ノード種別数、失敗原因）をログ化
- メトリクス
  - 抽出件数（シンボル/呼び出し/実装/使用/束縛）をカウントし、**性能/品質**の把握に利用
- トレーシング
  - 深い再帰中の**分岐トレース**（`extract_symbols_from_node`）にスパンを導入し、ホットスポットを可視化

## Risks & Unknowns

- 行番号: 本チャンクには明示がないため**行番号は不明**。重要箇所は関数名で示した
- Tree-sitterの`Node.byte_range`とRustの`&str`スライスの境界整合性に依存
- `Parser::parse`のNone条件（再入/リソース不足など）は*Tree-sitter仕様依存*
- `ParserContext`, `NodeTrackingState`, `Symbol`, `Visibility`等の詳細は**このチャンクには現れない**
- Send/Sync境界は**不明**（このチャンクには現れない）

---

### Rust特有の観点（詳細チェックリスト）

- メモリ安全性
  - 所有権: `&mut self`メソッドで**内部状態の単一所有**を維持（例: `parse`, `extract_symbols_from_node`）
  - 借用: `node.walk()`カーソルはスコープ内でのみ使用。可変借用は`parser`のみ
  - ライフタイム: `find_calls`や`find_uses`の戻り値で`&str`スライスを返却→元の`code`に依存し、**呼び出し側が`code`を保持**している前提で安全

- unsafe境界
  - 使用箇所: なし
  - 不変条件: 該当なし
  - 安全性根拠: 純Rust + Tree-sitterの安全APIのみ使用

- 並行性・非同期
  - Send/Sync: **不明**（このチャンクには現れない）
  - データ競合: `&mut self`により**同時アクセス防止**
  - await境界: 該当なし
  - キャンセル: 該当なし

- エラー設計
  - Result vs Option: 解析器生成は`Result<Self, String>`。パース結果は**空Vec**による失敗表現で曖昧
  - panic箇所: `code[byte_range]`スライスで*潜在的*にpanic（複数箇所、行番号:不明）
  - エラー変換: `map_err`で`String`化。**独自エラー型**の導入が望ましい