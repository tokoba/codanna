# parser.rs Review

## TL;DR

- 目的: **Tree-sitter**ベースで**TypeScript/TSX**コードからシンボル・インポート・型/呼び出し・継承/実装関係・JSX利用を抽出するパーサ
- 公開API: **TypeScriptParser::new**, **TypeScriptParser::parse**（固有メソッド）, および**LanguageParser**トレイト実装（find_imports, find_uses, find_calls など）
- コアロジック: **extract_symbols_from_node**でノード種別に応じた分岐処理（関数/クラス/変数/型/インターフェース/エラー/エクスポート/JSX 等）
- 可視性判定: **export_statement**祖先/兄弟/テキスト前置きを用いた三段階判定（AST欠損時のヒューリスティック）
- ヒューリスティック対応: **ERROR**ノード断片化に対応（identifier + formal_parametersのパターンを関数として合成）
- 重大リスク: テキストスライスに依存するため**UTF-8バイト境界**不一致時にpanicの可能性、**export**キーワード検出のヒューリスティック誤検出、**行番号表記の不整合**（+1加算の揺れ）
- パフォーマンス: ノード全走査ベースで概ね**O(n)**、多数の**to_string**生成と**child_by_field_name**反復によるオーバーヘッドが潜在的ボトルネック

## Overview & Purpose

このファイルは、**tree-sitter-typescript (TSX grammar)** を用いて TypeScript/TSX ソースコードをパースし、以下の情報を抽出するための実装です（ABI-14, node types 383, fields 40）。

- シンボル抽出（関数、クラス、インターフェース、型エイリアス、列挙、フィールド/プロパティ、変数/定数、矢関数）
- 可視性（export の有無に応じた **Visibility** 付与）
- インポート/再エクスポート（default/named/namespace/type-only/side-effect）
- 呼び出し関係（関数/メソッド）と型利用（関数引数/戻り値/クラス継承/implements/ジェネリクス、JSXコンポーネント利用）
- 継承・実装関係抽出（class extends / implements, interface extends）
- 監査用途のノードトラッキング（ハンドリングしたノード種別の記録）

中心となる関数は**extract_symbols_from_node**で、ノード種別に応じた分岐を行い、必要に応じてスコープ管理（ParserContext）を更新します。**ERROR**ノードや TSX の特殊ケースに対しても堅牢に動作するようヒューリスティックを含みます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | TypeScriptParser | pub | Tree-sitter Parserの保持、抽出状態（ParserContext/NodeTrackingState）と抽出ロジックの集約 | High |
| Impl | TypeScriptParser::new | pub | TSX言語設定でParser初期化 | Low |
| Impl | TypeScriptParser::parse (固有) | pub | ソースをAST化し、シンボル抽出とexportによるVisibility反映 | High |
| Trait Impl | NodeTracker | pub (trait impl) | ハンドリング済みノードの記録（監査/統計目的） | Low |
| Trait Impl | LanguageParser | pub (trait impl) | 解析系API（find_imports/find_uses/find_calls/find_extends/find_implementations など） | High |
| Fn | extract_symbols_from_node | private | ASTを深さ制限つき再帰で走査し、種別ごとに抽出 | High |
| Fn | process_import_statement | private | import文のバリエーション（default/named/namespace/type/side-effect）を抽出 | Med |
| Fn | find_implementations_in_node | private | extends/implementsを抽出（extends_onlyフラグで切替） | Med |
| Fn | extract_calls_recursive | private | 呼び出し抽出（関数コンテキスト推定含む） | High |
| Fn | extract_type_uses_recursive | private | 型利用抽出（引数/戻り値/フィールド/呼び出しジェネリクス等） | High |

### Dependencies & Interactions

- 内部依存
  - TypeScriptParser::parse → extract_symbols_from_node → process_*（function/class/interface/type_alias/enum/property/method/variable）→ context.enter_scope / exit_scope
  - Visibility判定: determine_visibility, determine_method_visibility
  - 関係抽出: find_implementations_in_node / process_heritage_clauses / process_extends_clause / extract_type_name
  - 呼び出し抽出: extract_calls_recursive / extract_function_name
  - 型利用抽出: extract_type_uses_recursive / extract_parameter_types / extract_type_from_annotation / extract_simple_type_name
  - インポート抽出: extract_imports_from_node / process_import_statement / process_export_statement
  - 監査: register_node_recursively / register_handled_node（NodeTracker）

- 外部依存（表）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | tree_sitter::{Parser, Language, Node} | AST生成・走査 | TSX言語で設定 |
  | tree_sitter_typescript::LANGUAGE_TSX | 言語定義 | TSX対応（JSX含む） |
  | crate::parsing::{LanguageParser, NodeTracker, ParserContext, ScopeType, Import, MethodCall, NodeTrackingState} | トレイト/API, スコープ・監査・DTO | ParserContextは関数/クラススコープ管理 |
  | crate::types::{SymbolCounter, SymbolId} | シンボルID採番 | 可変カウンタ |
  | crate::{FileId, Range, Symbol, SymbolKind, Visibility} | コアDTO/メタ | Symbolはwith_*チェーン可能 |
  | crate::config::is_global_debug_enabled | デバッグ出力制御 | eprintlnベース |

- 被依存推定
  - プロジェクトのパーサ統合層（LanguageParserポリモーフィック利用）
  - 言語横断の解析器で TypeScriptParser をプラグイン的に使用
  - インデクサ/関係グラフ生成器（シンボル/インポート/呼び出し/型/継承）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| TypeScriptParser::new | `pub fn new() -> Result<Self, String>` | TSX言語設定でParser初期化 | O(1) | O(1) |
| TypeScriptParser::parse | `pub fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | ソースからシンボル抽出と可視性反映 | O(n) | O(k) |
| LanguageParser::parse | `fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | トレイト経由の同等処理委譲 | O(n) | O(k) |
| LanguageParser::find_imports | `fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import>` | import/exportからImport DTO抽出 | O(n) | O(m) |
| LanguageParser::find_uses | `fn find_uses<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 型利用 + JSXコンポーネント利用抽出 | O(n) | O(u) |
| LanguageParser::find_calls | `fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 関数呼び出し抽出（caller→callee） | O(n) | O(c) |
| LanguageParser::find_method_calls | `fn find_method_calls(&mut self, code: &str) -> Vec<MethodCall>` | メソッド呼び出し抽出（receiver含む） | O(n) | O(mc) |
| LanguageParser::find_implementations | `fn find_implementations<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | class implements インターフェース抽出 | O(n) | O(r) |
| LanguageParser::find_extends | `fn find_extends<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | class/interface の extends 抽出 | O(n) | O(r) |
| LanguageParser::find_defines | `fn find_defines<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | メソッド定義（class/interface/type literal）抽出 | O(n) | O(d) |
| LanguageParser::find_variable_types | `fn find_variable_types<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | `const x = new Type()`型推定 | O(n) | O(b) |
| LanguageParser::extract_doc_comment | `fn extract_doc_comment(&self, node: &Node, code: &str) -> Option<String>` | JSDoc/TSDoc (`/** */`)抽出 | O(1) | O(s) |
| LanguageParser::language | `fn language(&self) -> crate::parsing::Language` | 言語列挙返却 | O(1) | O(1) |
| NodeTracker::get_handled_nodes | `fn get_handled_nodes(&self) -> &HashSet<HandledNode>` | 監査用ノード集合参照 | O(1) | O(N) |
| NodeTracker::register_handled_node | `fn register_handled_node(&mut self, node_kind: &str, node_id: u16)` | 監査用ノード登録 | O(1) | O(N) |

注: n=ASTノード数, k=抽出シンボル数, m=Import数, u=型/JSX利用数, c=関数呼び出し数, mc=メソッド呼び出し数, r=関係数, d=定義数, b=バインディング数。行番号は「行番号不明」（このチャンクには行番号が明示されていません）。

以下、主要APIの詳細:

1) TypeScriptParser::new
- 目的と責務: **TSX**言語をParserに設定し、解析コンテキスト/トラッカーを初期化
- アルゴリズム:
  - Parser::new
  - LANGUAGE_TSXをset_language
  - 各内部セットを空に初期化
- 引数:
  | 名称 | 型 | 説明 |
  |------|----|------|
  | なし | - | - |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Result<TypeScriptParser, String> | 成功時パーサ、失敗時エラーメッセージ |
- 使用例:
  ```rust
  let mut ts = TypeScriptParser::new().expect("TS parser");
  ```
- エッジケース:
  - set_language失敗時にStringでエラー返却

2) TypeScriptParser::parse（固有）
- 目的と責務: ソースコードから**Symbol**ベクタを抽出
- アルゴリズム（簡略）:
  1. ParserContext/exportsセットをリセット
  2. `self.parser.parse(code, None)`でAST化
  3. root_nodeから`extract_symbols_from_node`再帰
  4. default/named exportセットに該当するシンボルの**Visibility::Public**化
- 引数:
  | 名称 | 型 | 説明 |
  |------|----|------|
  | code | &str | ソース |
  | file_id | FileId | ファイル識別子 |
  | symbol_counter | &mut SymbolCounter | シンボルID採番 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Vec<Symbol> | 抽出されたシンボル |
- 使用例:
  ```rust
  let mut counter = SymbolCounter::new();
  let symbols = ts.parse("export function f(){}", file_id, &mut counter);
  ```
- エッジケース:
  - ERRORノードの断片関数を合成（identifier + formal_parameters）
  - TSXでもERRORルートを許容
  - module_pathは空文字指定（ここでは算出しない）

3) LanguageParser::find_imports
- 目的と責務: `import`/`export ... from` を走査し、**Import**（path, alias, is_glob, is_type_only）を抽出
- アルゴリズム:
  - ASTを生成
  - `extract_imports_from_node`再帰で
    - import_statement → `process_import_statement`
    - export_statement（sourceあり）→ `process_export_statement`
- 引数:
  | 名称 | 型 | 説明 |
  |------|----|------|
  | code | &str | ソース |
  | file_id | FileId | ファイル識別子 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Vec<Import> | インポート関係 |
- 使用例:
  ```rust
  let imports = ts.find_imports("import React from 'react';", file_id);
  ```
- エッジケース:
  - type-only import検出（import直後の`type`キーワードの位置ヒューリスティク）
  - named importsは**specifierごと**にImport生成
  - namespace importは`is_glob=true` + alias（`* as name`）
  - side-effect import（import 'x';）はaliasなし、type-only=false

4) LanguageParser::find_uses
- 目的と責務: **型利用**（関数/メソッド引数・戻り値、クラス継承/implements、ジェネリクス）と**JSXコンポーネント利用**を抽出
- アルゴリズム:
  - `extract_type_uses_recursive`（プリミティブ型はフィルタ）で型利用抽出
  - `extract_jsx_uses_recursive`でJSXタグ名をUppercaseのみ抽出
- 引数/戻り値:
  | 引数 | 型 | 説明 |
  |------|----|------|
  | code | &str | ソース |
  | 戻り | Vec<(&str, &str, Range)> | (コンテキスト名, 型/コンポーネント名, 範囲) |
- 使用例:
  ```rust
  let uses = ts.find_uses("const x = useState<Session>();");
  ```
- エッジケース:
  - `predefined_type`（string/number等）は除外
  - `generic_type`はベース型名＋ネストしたtype_argumentsを再帰抽出
  - JSXは大文字名のみ（HTMLタグ除外）

5) LanguageParser::find_calls
- 目的と責務: **関数呼び出し**（caller → callee）を抽出
- アルゴリズム:
  - `extract_calls_recursive`で
    - 関数文脈推定（function_declaration/method/arrow/function_expression、ERROR断片）
    - call_expressionの被呼び出し名抽出（identifier/member_expression/await_expression）
    - 文脈未確定の時は祖先から推定
- 使用例:
  ```rust
  let calls = ts.find_calls("function a(){b()}");
  ```
- エッジケース:
  - `await foo()`対応
  - ルートERRORやprogram直下の断片関数対応
  - member_expressionは`console.log`等のドット名全体をcalleeに

6) LanguageParser::find_method_calls
- 目的と責務: **メソッド呼び出し**（receiver含む）抽出
- アルゴリズム: `extract_method_calls_recursive`でcall_expressionのfunctionがmember_expressionのときに`extract_method_signature`でreceiver/property抽出
- 使用例:
  ```rust
  let mcs = ts.find_method_calls("sdk.createChat()");
  ```
- エッジケース:
  - 静的判定は型情報なしのため**常にis_static=false**
  - 関数文脈推定はfind_callsと同様のロジック

7) LanguageParser::find_extends / find_implementations
- 目的と責務: **class extends** / **class implements**、および**interface extends**抽出
- アルゴリズム:
  - `find_implementations_in_node`で`extends_only`フラグによる分岐
  - class_heritage内のextends_clause/implements_clauseを処理（generic/nested/identifier対応）
  - interfaceの場合、`extends_type_clause`または`extends_clause`を走査
- 使用例:
  ```rust
  let ex = ts.find_extends("class A extends B {}");
  let im = ts.find_implementations("class A implements I {}");
  ```
- エッジケース:
  - ABI差異（`extends_type_clause`）に両対応
  - generic_typeのnameフィールドからベース型抽出

8) LanguageParser::find_defines
- 目的と責務: **メソッド定義**の列挙（class/interface/type literal）
- アルゴリズム: `extract_method_defines_recursive`で対象ノード（method_definition/method_signature/abstract_method_signature）抽出
- 使用例:
  ```rust
  let defs = ts.find_defines("interface I{ m():void }");
  ```

データ契約（抜粋）
- Symbol: id, name, kind, file_id, range, signature?, doc_comment?, module_path?, visibility, scope_context
- Import: path, alias?, file_id, is_glob, is_type_only
- MethodCall: caller, method_name, receiver?, is_static, range
- Range: start_line/u32, start_column/u16, end_line/u32, end_column/u16

## Walkthrough & Data Flow

- parse（固有メソッド）
  - コンテキスト/エクスポート集合をクリア
  - AST化 → rootから`extract_symbols_from_node`
  - 抽出後、default_exported_symbols / named_exported_symbols を用いて**Visibility**をPublicに補正

- extract_symbols_from_node
  - 再帰深さガード: `check_recursion_depth(depth, node)`（関数名:行番号不明）
  - ノード種別に応じて分岐し、対応するprocess_*へ委譲
  - スコープ管理:
    - 関数宣言: `ScopeType::hoisting_function()`へenter → body抽出 → exit + 親コンテキスト復元
    - クラス宣言: `ScopeType::Class`へenter → メンバ抽出 → exit + 親コンテキスト復元
    - メソッドのbody: `ScopeType::Function { hoisting: false }`
    - 矢関数のbody: `ScopeType::function()`でenter
  - ERRORノード: 断片的関数（identifier + formal_parameters）を**合成シンボル**として抽出し、子も再帰処理
  - export_statement: default/namedの抽出、必要に応じて子も再帰処理
  - JSX: `track_jsx_component_usage`でUppercase名のみ使用関係に記録し、子も再帰

- インポート抽出
  - `extract_imports_from_node`: import_statement / export_statement（sourceあり）を検出
  - `process_import_statement`: default/named/namespace/type-only/side-effect に対応
  - `process_export_statement`: 再エクスポートの `* from` と named を扱う

- 呼び出し/型利用
  - `extract_calls_recursive`: 文脈推定 + call_expressionのcallee抽出
  - `extract_method_calls_recursive`: member_expressionからreceiver/property抽出
  - `extract_type_uses_recursive`: params/return/type_annotation/class_heritage/type_arguments を再帰的に抽出
  - JSX利用は`extract_jsx_uses_recursive`で併行抽出

### Flow（Mermaid）

```mermaid
flowchart TD
  A[root_node] --> B{extract_symbols_from_node}
  B -->|function_declaration| C[process_function + enter ScopeType::hoisting_function]
  C --> D[process body children]
  D --> E[exit scope & restore context]

  B -->|class_declaration/abstract_class_declaration| F[process_class + enter ScopeType::Class]
  F --> G[extract_class_members]
  G --> H[exit scope & restore context]

  B -->|interface_declaration| I[process_interface]
  B -->|type_alias_declaration| J[process_type_alias]
  B -->|enum_declaration| K[process_enum]
  B -->|lexical/variable_declaration| L[process_variable_declaration]
  L -->|arrow function value| M[enter ScopeType::function + body recurse + exit]
  B -->|arrow_function| N[process_arrow_function (None)]
  B -->|export_statement| O[collect default/named exports; recurse children if needed]
  B -->|ERROR| P[fragmented function synthesis; recurse children]
  B -->|jsx_element/self_closing| Q[track_jsx_component_usage; recurse children]
  B -->|others| R[register & recurse children]
```

上記の図は`extract_symbols_from_node`関数（行番号不明）の主要分岐を示す。

## Complexity & Performance

- 時間計算量
  - AST生成: tree-sitterのパースは概ね**O(n)**（n=ソース長/ノード数）
  - 抽出処理（シンボル/インポート/呼び出し/型/継承）: すべて再帰走査で**O(n)**
- 空間計算量
  - 出力コレクションに比例（シンボル数、Import数、関係数）で**O(k)**〜**O(n)**
  - NodeTrackingStateの監査セットで**O(n)**
- ボトルネック・スケール限界
  - 多数の`to_string()`（識別子/シグネチャ/コメント）による**短命ヒープ割り当て**
  - `child_by_field_name`と`walk()`の反復呼び出しが多く、**ASTナビゲーションオーバーヘッド**
  - `code[start..end]`の頻繁なスライス生成
- 実運用負荷要因
  - **TSX/JSX**が多いコードは`ERROR`ノードやタグ走査が増えコスト上昇
  - 多量のインポート/再エクスポート（モノレポ）でImport抽出結果が膨張
  - **深いネスト**のクラス/関数で再帰深さに注意（`check_recursion_depth`で制御）

## Edge Cases, Bugs, and Security

- メモリ安全性
  - **unsafe**未使用（確認: ファイル内なし）
  - `code[node.byte_range()]`は**UTF-8バイト境界**に依存。tree-sitterの`byte_range`は通常UTF-8境界だが、もし不整合があれば**panic**（関数名:行番号不明）。現実的には安全だが、念のため防御的ユーティリティの活用を推奨。
  - 大規模/深いASTの再帰は**スタックオーバーフロー**リスク→`check_recursion_depth`で緩和（関数名:行番号不明）

- インジェクション
  - SQL/Command/Path: 該当なし。本コードは静的解析のみで外部コマンド未使用。
  - Path traversal: インポートパスは文字列抽出のみでファイルアクセスなし。

- 認証・認可
  - 該当なし。

- 秘密情報
  - ハードコード秘密: なし
  - ログ漏えい: `eprintln!`による**デバッグ出力**は機密を含む可能性があるため**本番で抑制**推奨（`is_global_debug_enabled()`で制御）

- 並行性
  - グローバル状態なし。ただし`TypeScriptParser`は**内部に状態を持つ**（ParserContext、node_tracker）。メソッドは`&mut self`を要求し、**並行実行非対応**設計。`Send/Sync`境界は不明。

- バグ/仕様リスク
  - 可視性判定の**prefixヒューリスティック**（`export `を文字列で検出）は誤検出の可能性
  - `Range`の**行番号加算の揺れ**（一部+1、一部そのまま）による整合性不一致
  - `process_import_statement`のtype-only検出は`child.kind() == "type" && i == 1`に依存（文法変更で破綻する恐れ）
  - JSX利用の**component_usages**ベクタは内部で収集されるが**外部APIで返却されない**（活用不明）
  - `process_arrow_function`は**常にNone**返却。命名済み矢関数は変数経由で抽出されるが、矢関数単体の扱いは限定的

### Edge Cases詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ERROR断片関数 | `identifier formal_parameters`（TSXの"use client"等で断片化） | 関数シンボル合成 | `extract_symbols_from_node`で合成 | 対応済み |
| default export名抽出 | `export default Foo` | FooをPublic化 | `export_statement`で抽出 | 対応済み |
| named export抽出 | `export { A as B }` | BをPublic化（名前解決はB） | export_clause→export_specifier | 対応済み |
| type-only import | `import type { P } from './t'` | is_type_only=true | ヒューリスティック検出 | 対応済み（文法依存） |
| namespace import | `import * as u from './u'` | alias=u, is_glob=true | namespace_import処理 | 対応済み |
| side-effect import | `import './s.css'` | aliasなし、type_only=false | import_clauseなしで追加 | 対応済み |
| JSX lower-caseタグ | `<div>` | 無視（HTML） | 先頭が小文字ならskip | 対応済み |
| privateフィールド | `#x: number` | Visibility::Private | `determine_method_visibility`で`#`検知 | 対応済み |
| protectedメンバ | `protected x` | Visibility::Module | 文字列検知 | 対応済み |
| 可視性prefix誤検出 | `/* export */`周辺 | 誤ってPublic化しない | prefixウィンドウ検査 | 潜在バグ |
| Range行番号不一致 | 各APIで+1揺れ | 一貫した行番号返却 | 実装揺れあり | 改善要 |

## Design & Architecture Suggestions

- 可視性判定の強化
  - 文字列prefixによる`export`検出は*誤陽性*を招く可能性。**ASTベース**（祖先/兄弟のみ）を優先し、prefixチェックは最終手段に。
- Range整合性の統一
  - **行番号+1の一貫性**（0始まり→1始まりの規約統一）を実施。呼び出し/型/定義/インポートの全APIで統一。
- 矢関数APIの拡張
  - `process_arrow_function`がNone固定。**変数割当以外**（即時代入や戻り値）でも*匿名関数*を何らかの形で表現する方針検討。
- JSX利用の外部公開
  - `component_usages`を**LanguageParser::find_uses**へ統合（現状、別の再帰`extract_jsx_uses_recursive`で返している）か、**専用API**で公開。
- 文字列生成の削減
  - 頻繁な`to_string()`を**必要箇所のみ**に限定。特に`process_import_statement`とシンボル生成周りで削減。
- 文法差異の吸収レイヤ
  - ABIや文法差異（`extends_type_clause`など）対応を**ヘルパー関数**に集約し、ロジック重複を削減。
- エラー設計
  - 現状は`Option`多用で失敗時スキップ。**Result + エラーカテゴリ**を導入し、解析品質を可観測化。

## Testing Strategy (Unit/Integration) with Examples

- 既存のテストは広範:
  - import抽出（default/named/namespace/type/side-effect/re-export）
  - ジェネリック型抽出（constructor/callのtype_arguments）
  - extends/implements分離検証
  - 変数new式の型推定
  - メソッド呼び出し抽出
  - プリミティブ型フィルタ
  - 多様なパス形式
  - JSXコンポーネント利用

- 追加テスト提案
  1. exportヒューリスティック誤検出抑制
     ```rust
     #[test]
     fn test_export_prefix_false_positive() {
         let mut p = TypeScriptParser::new().unwrap();
         let file_id = FileId::new(1).unwrap();
         let code = r#"
           // not an export:
           /* export */ function f() {}
         "#;
         let mut counter = SymbolCounter::new();
         let syms = p.parse(code, file_id, &mut counter);
         assert!(syms.iter().any(|s| s.name.as_ref()=="f" && matches!(s.visibility, Visibility::Private)));
     }
     ```
  2. ERROR断片関数（"use client"前置）
     ```rust
     #[test]
     fn test_error_fragmented_function_synthesis() {
         let mut p = TypeScriptParser::new().unwrap();
         let file_id = FileId::new(1).unwrap();
         // 擬似的にERRORを誘発するケースが必要だが、ここでは簡略化
         let code = "identifier(param) {}"; // 実環境ではERRORノード下
         let mut counter = SymbolCounter::new();
         let syms = p.parse(code, file_id, &mut counter);
         // 合成関数がPublicで作られるロジックに合わせる
         assert!(syms.iter().any(|s| s.kind==SymbolKind::Function));
     }
     ```
  3. private/protectedフィールド
     ```rust
     #[test]
     fn test_field_visibility_private_protected() {
         let mut p = TypeScriptParser::new().unwrap();
         let file_id = FileId::new(1).unwrap();
         let code = r#"
           class C { 
             #x: number; 
             protected y: string; 
             public z: boolean; 
           }
         "#;
         let mut counter = SymbolCounter::new();
         let syms = p.parse(code, file_id, &mut counter);
         assert!(syms.iter().any(|s| s.name.as_ref()=="x" && matches!(s.visibility, Visibility::Private)));
         assert!(syms.iter().any(|s| s.name.as_ref()=="y" && matches!(s.visibility, Visibility::Module)));
         assert!(syms.iter().any(|s| s.name.as_ref()=="z" && matches!(s.visibility, Visibility::Public)));
     }
     ```

- 統合テスト例（利用側）
  ```rust
  fn index_typescript(code: &str, file_id: FileId) {
      let mut parser = TypeScriptParser::new().unwrap();
      let mut counter = SymbolCounter::new();

      let symbols = parser.parse(code, file_id, &mut counter);
      let imports = parser.find_imports(code, file_id);
      let uses    = parser.find_uses(code);
      let calls   = parser.find_calls(code);
      let mcs     = parser.find_method_calls(code);
      let exts    = parser.find_extends(code);
      let impls   = parser.find_implementations(code);

      // ... グラフ構築などに流す ...
  }
  ```

## Refactoring Plan & Best Practices

- スコープ/文脈管理のユーティリティ化
  - `enter_scope`/`exit_scope`と`current_function/class`の保存/復元を**RAIIガード**（Drop）で自動化（ヒューマンエラー削減）。
- Range一貫性の整備
  - 返却**行番号規約**（1始まり）をモジュール全体で統一。ヘルパー`range_from(node, one_based: bool)`導入。
- インポート抽出の整理
  - `process_import_statement`内の**フラグ/抽出**ロジックを**小関数**へ分割（readability向上）。
- 可視性判定の強化
  - 祖先/兄弟チェックに**限定**し、prefix検査は削除または厳格なトークン化検査に置換。
- 文字列割り当て削減
  - `&str`を返せる箇所は**借用のまま**扱い、必要時のみ`to_string()`。シグネチャ抽出もなるべく借用に寄せる。
- メソッド呼び出しの静的判別
  - 型情報がないため推測は困難だが、簡易ルール（クラス名っぽい先頭大文字receiverは静的可能性）を導入可。ただし*誤検出*に注意。
- 監査/観測の拡張
  - NodeTrackerに**件数/種別メトリクス**集計APIを提供し、解析プロファイルを取得可能に。

## Observability (Logging, Metrics, Tracing)

- 現状: `eprintln!`と`is_global_debug_enabled()`の手動制御。ノード監査は**NodeTrackingState**で集合管理。
- 改善提案:
  - 構造化ログ（target/level/関数名）を**ログクレート**に移行（例: log/tracing）
  - 主要イベント（parse開始/終了、ERRORノード検知、合成関数作成、export検出）に**span**を付与
  - Metrics例:
    - 解析ノード総数、ERRORノード数、抽出シンボル数、Import数、呼び出し数
    - 再帰最大深さ（`check_recursion_depth`の閾値近傍検知）

## Risks & Unknowns

- **ABI/文法差異**: 冒頭コメントはABI-14。コード内コメントにABI-15言及あり（interfaceの`extends_type_clause`など）。両対応の努力は見えるが、**将来の文法変更**でヒューリスティックが壊れる可能性。
- **Send/Sync**境界: tree_sitter::ParserやNodeの**スレッドセーフ性**はこのチャンクには現れない。並行利用要件がある場合、別インスタンス生成や同期が必要。
- **Range一貫性**: 行番号（+1加算有無）の規約が不明確で、下流の可視化やLSP連携に影響しうる。
- **component_usagesの活用**: 収集はするが公開APIで返却されない。用途が不明。
- **JSDoc抽出**: `export_statement`包囲時の前方コメントを拾う設計だが、より複雑なコメント配置（デコレータや複数コメント）では取りこぼしの可能性。

以上、本チャンクに含まれる関数と挙動に基づく詳細レビューです（行番号は「不明」）。