# parsing\python\parser.rs Review

## TL;DR

- 目的: **tree-sitter**でPythonコードをパースし、関数/メソッド/クラス/代入/型注釈/呼び出し/継承/インポートを抽出するパーサ
- 主要公開API: **PythonParser::new**, **PythonParser::parse**, LanguageParserトレイト経由の**find_calls**, **find_method_calls**, **find_imports**, **find_implementations**, **find_variable_types**, **find_defines**
- コアロジック: 再帰的AST走査と**ParserContext**によるスコープ管理、**docstring**抽出、**シグネチャ**組み立て、**呼び出し/継承/型**の抽出
- 重大リスク: 
  - モジュールレベルのdocstring未対応（テストで確認）
  - 単純インポートの「asエイリアス」を未対応（aliased_import未処理）
  - 文字列スライスでの**UTF-8バイト境界**問題の潜在的panic
  - エラー型（PythonParseError）が大半未使用で、**構文エラーを結果に反映しない**
- パフォーマンス: ASTサイズに対して**O(n)**で線形、ただしシグネチャ構築・文字列結合による追加割当が多い
- セキュリティ/安全性: **unsafe未使用**、但し**&strのbyte_rangeスライス**はUTF-8境界依存のため注意。並行性は現状**非対応/不明**


## Overview & Purpose

このファイルは、tree-sitter-python（ABI-14）を用いてPythonコードを解析し、コードインテリジェンスのための構造化メタデータ（Symbol）を生成するパーサの実装です。以下の目的を果たします。

- ソースコードから**シンボル（関数、メソッド、クラス、変数、定数、型エイリアス）**を抽出
- **docstring**（関数/クラス）と**シグネチャ**（引数型、デフォルト値、戻り値型、async）を付与
- **関数呼び出し**、**メソッド呼び出し（レシーバ含む）**、**継承関係**、**インポート情報**、**変数の型注釈**を抽出
- **スコープ管理**（モジュール、クラス、関数）による適切な**シンボル名（修飾付き）**生成
- AST走査のトラッキング（NodeTracker）と再帰深度の監視による**安全性/健全性**の確保

ABI-14に依存しており、ABI-15への移行時にはノード種別名の互換性確認が必要です。


## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | PythonParseError | pub | Pythonパーサ専用エラー型（初期化、構文、型注釈、未対応機能） | Low |
| Struct | PythonParser | pub | tree-sitterパーサとノードトラッカーを保持し、Pythonのシンボル抽出を実施 | Med |
| Trait Impl | LanguageParser for PythonParser | crate公開（推定） | 汎用パーサAPI: parse, language, extract_doc_comment, find_calls 等 | Med |
| Trait Impl | NodeTracker for PythonParser | crate公開（推定） | 処理したノード種別の記録 | Low |

内部主要ロジック（関数群）:
- extract_symbols_from_node: ノード種別に応じて分岐し、再帰的に子ノード処理
- process_function / process_class / process_assignment / process_type_alias: シンボル生成
- build_function_signature: 引数・戻り値・asyncの**シグネチャ**生成
- docstring抽出: extract_function_docstring / extract_class_docstring / extract_docstring_from_body
- find_calls / find_method_calls / find_implementations / find_imports / find_variable_types / find_defines: AST走査系ユーティリティ

### Dependencies & Interactions

- 内部依存（主な呼び出し関係）
  - parse → extract_symbols_from_node → process_function/process_class/... → process_children → extract_symbols_from_node（再帰）
  - process_function → extract_function_name, extract_function_docstring, build_function_signature, is_inside_class
  - build_function_signature → build_parameters_string, is_async_function, extract_return_type
  - find_calls → find_calls_in_node → process_function_node_for_calls / process_call_node
  - find_method_calls → find_method_calls_in_node → process_call_node_for_method_calls
  - find_imports → find_imports_in_node → process_import_statement / process_from_import_statement（→ process_aliased_import）
  - find_implementations → find_implementations_in_node → process_class_inheritance
  - find_variable_types → find_variable_types_in_node → process_assignment_with_type

- 外部依存（クレート/モジュール）
  | 依存名 | 用途 | 備考 |
  |--------|------|------|
  | tree_sitter | AST生成・走査（Node, Parser） | 基盤パーサ、UTF-8バイトオフセット |
  | tree_sitter_python | Python言語定義（LANGUAGE） | ABI-14 |
  | thiserror | エラー型導出（derive(Error)） | PythonParseError |
  | std::collections::HashSet | ノード追跡（NodeTrackingState内部） | トラッキング用途 |
  | crate::parsing::{...} | Import, ParserContext, ScopeType, NodeTrackerなど | スコープ管理/トラッキング |
  | crate::{FileId, Range, Symbol, SymbolKind} | 出力データ契約 | シンボル等 |
  | crate::types::SymbolCounter | ID発行 | スレッドセーフ性は不明 |

- 被依存推定（このモジュールを利用しそうな箇所）
  - 検索インデクサ（シンボル・リレーション抽出）
  - クロスリファレンス（呼び出し/実装/インポート解析）
  - ドキュメント生成（docstring/シグネチャ提示）
  - 型ヒント可視化（変数型注釈）


## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PythonParser::new | ```pub fn new() -> Result<Self, PythonParseError>``` | パーサ初期化 | O(1) | O(1) |
| PythonParser::parse | ```pub fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>``` | Pythonコードからシンボル抽出 | O(n) | O(k) |
| LanguageParser::parse | ```fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>``` | トレイト経由の同機能 | O(n) | O(k) |
| LanguageParser::language | ```fn language(&self) -> Language``` | 言語種別取得 | O(1) | O(1) |
| LanguageParser::extract_doc_comment | ```fn extract_doc_comment(&self, node: &Node, code: &str) -> Option<String>``` | ノードからdocstring抽出 | O(1〜m) | O(s) |
| LanguageParser::find_calls | ```fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>``` | 呼び出し（caller→callee）抽出 | O(n) | O(c) |
| LanguageParser::find_method_calls | ```fn find_method_calls(&mut self, code: &str) -> Vec<MethodCall>``` | メソッド呼び出し（レシーバ付き）抽出 | O(n) | O(c) |
| LanguageParser::find_implementations | ```fn find_implementations<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>``` | クラス継承関係抽出 | O(n) | O(r) |
| LanguageParser::find_defines | ```fn find_defines<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>``` | クラス内メソッド定義一覧 | O(n) | O(d) |
| LanguageParser::find_imports | ```fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import>``` | インポート抽出 | O(n) | O(i) |
| LanguageParser::find_variable_types | ```fn find_variable_types<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>``` | 変数型注釈抽出 | O(n) | O(t) |
| LanguageParser::as_any | ```fn as_any(&self) -> &dyn Any``` | ダウンキャスト用 | O(1) | O(1) |
| NodeTracker::register_handled_node | ```fn register_handled_node(&mut self, node_kind: &str, node_id: u16)``` | 処理ノード記録 | O(1) | O(1) |
| NodeTracker::get_handled_nodes | ```fn get_handled_nodes(&self) -> &HashSet<HandledNode>``` | 記録集合取得 | O(1) | O(m) |

注: n=ASTノード数、k=生成シンボル数、c=呼び出し数、r=継承数、i=インポート数、t=型注釈数、m=記録ノード数、s=docstring長

主要データ契約（抜粋）:
- Symbol: id, name, kind（Module/Class/Function/Method/Variable/Constant/TypeAlias）, file_id, range, doc_comment, signature, scope_context
- Import: path, alias, file_id, is_glob, is_type_only
- MethodCall: caller, method_name, receiver（Option）、range
- Range: start_line, start_column, end_line, end_column

以下、主APIの詳細:

1) PythonParser::new
- 目的と責務: tree-sitterにPython言語を設定し、パーサを構築する
- アルゴリズム:
  1. Parser::newでパーサ作成
  2. set_language(tree_sitter_python::LANGUAGE)に失敗した場合、PythonParseError::ParserInitFailedを返す
  3. NodeTrackingState初期化
- 引数: なし
- 戻り値: Result<PythonParser, PythonParseError>
- 使用例:
  ```rust
  let mut parser = PythonParser::new().expect("Python parser init");
  ```
- エッジケース:
  - tree_sitter_pythonがリンク不全 → ParserInitFailed
  - ABI不一致 → set_language失敗（現在ABI-14前提）

2) PythonParser::parse
- 目的と責務: ソースからSymbol列を抽出する主関数
- アルゴリズム（簡略）:
  1. self.parser.parse(code, None)でASTを生成（失敗ならVec::new）
  2. ルートノードからモジュールSymbol（name="<module>"）を追加
  3. ParserContext::newでスコープ初期化
  4. extract_symbols_from_nodeを再帰呼出し
- 引数:
  | 名称 | 型 | 説明 |
  |------|----|------|
  | code | &str | Pythonソースコード |
  | file_id | FileId | ファイル識別子 |
  | symbol_counter | &mut SymbolCounter | Symbol ID発行 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Vec<Symbol> | 抽出された全シンボル |
- 使用例:
  ```rust
  let symbols = parser.parse("def hello(): pass", file_id, &mut SymbolCounter::new());
  ```
- エッジケース:
  - 空文字列 → モジュールSymbolのみ or 空（現在は空Vec）
  - 構文エラー → ツリーが生成される場合、部分的抽出。エラーは返さない

3) LanguageParser::find_calls
- 目的: 関数呼び出し（caller→callee）を抽出
- アルゴリズム:
  1. AST生成
  2. find_calls_in_node再帰で"call"ノードを収集
  3. "function_definition"でcurrent_functionを更新
  4. calleeはidentifierまたはattribute（dotted path）を抽出
- 引数:
  | 名称 | 型 | 説明 |
  |------|----|------|
  | code | &str | ソース |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Vec<(&str, &str, Range)> | (caller, callee, 位置) |
- 使用例:
  ```rust
  let calls = parser.find_calls(code);
  for (caller, callee, range) in calls { /* ... */ }
  ```
- エッジケース:
  - ネスト関数内 → callerは内側の関数名
  - 修飾呼び出し（a.b.c()）→ calleeは"dotted path"文字列

4) LanguageParser::find_method_calls
- 目的: メソッド呼び出しを抽出し、レシーバを付与
- アルゴリズム: "attribute"ノードからobjectとattributeを分離、MethodCall構築
- 使用例:
  ```rust
  let mcs = parser.find_method_calls(code);
  ```
- エッジケース:
  - チェーン呼び出し（obj.a().b()）→ レシーバは各段階で異なる可能性

5) LanguageParser::find_imports
- 目的: import文とfrom import文の抽出
- アルゴリズム:
  - import_statement: dotted_name/identifierを抽出（注: aliased_import未処理）
  - import_from_statement: base_pathを抽出、wildcard or individual名（dotted_name/aliased_import）を処理
- エッジケース:
  - 「import x as y」→ 現実装のimport_statementではalias未対応（改善要）

6) LanguageParser::find_implementations
- 目的: class_definitionから基底クラスを抽出し、(派生, 基底, Range)を生成
- アルゴリズム: superclassesフィールドを辿り、identifier/attributeを収集

7) LanguageParser::find_variable_types
- 目的: 代入ノードのtypeフィールドから変数型注釈を抽出
- アルゴリズム: assignmentのleftから変数名、typeから型を取得（self.attrは属性名を抽出）
- エッジケース:
  - 型のみ注釈（x: int）→ tree-sitterではassignmentになるため取得可能

8) LanguageParser::find_defines
- 目的: クラス内のメソッド定義一覧（クラス名, メソッド名, Range）
- アルゴリズム: class_definitionのbody内のfunction_definitionを列挙

9) LanguageParser::extract_doc_comment
- 目的: function/classノードに対するdocstring抽出
- 備考: メソッドdocstringもfunction_definition扱いで抽出可能（シンボル名の探索時に注意）

10) LanguageParser::language / as_any / NodeTracker各種
- 目的: メタ/補助API


## Walkthrough & Data Flow

parseのトップレベルフロー:
- AST作成 → ルートノード取得
- **モジュールSymbol**作成（name="<module>"）
- ParserContext初期化
- extract_symbols_from_nodeで再帰走査
  - function_definition: メソッドかどうか判定、docstring/シグネチャ抽出、スコープenter/exit、親コンテキスト保存復元
  - class_definition: docstring/シグネチャ（継承）抽出、スコープenter/exit、親コンテキスト保存復元
  - assignment: 左辺がidentifierならVariable/Constantを生成
  - type_alias_statement: TypeAliasを生成
  - import関連/ラムダ/包括表記: 子ノード走査のみ

主要分岐の流れ図:

```mermaid
flowchart TD
    A[extract_symbols_from_node(node)] --> B{node.kind}
    B -->|function_definition| F1[register_handled_node; process_function]
    F1 --> C1[context.enter_scope(Function)]
    C1 --> S1[save parent ctx; set current_function]
    S1 --> P1[process_children(...)]
    P1 --> E1[context.exit_scope; restore ctx]

    B -->|class_definition| F2[register_handled_node; process_class]
    F2 --> C2[context.enter_scope(Class)]
    C2 --> S2[save parent ctx; set current_class(with nesting)]
    S2 --> P2[process_children(...)]
    P2 --> E2[context.exit_scope; restore ctx]

    B -->|assignment| F3[register_handled_node; process_assignment]
    F3 --> PC1[process_children(...)]

    B -->|type_alias_statement| F4[register_handled_node; process_type_alias]

    B -->|import* / lambda / comprehension / decorator / for_statement / type| F5[register_handled_node; process_children(...)]

    B -->|other| F6[register_handled_node; process_children(...)]
```

上記の図は`extract_symbols_from_node`関数の主要分岐を示す（行番号不明：このチャンクには行番号が含まれない）。


## Complexity & Performance

- パース（tree-sitter）: O(n)（n=トークン/ノード数）
- AST再帰走査（シンボル抽出）: O(n)
- 呼び出し検出/継承/インポート/型抽出: 各O(n)
- 空間計算: O(k + メタデータ), k=生成シンボル数
- ボトルネック:
  - 文字列生成・結合（シグネチャ、docstring正規化、dotted path抽出）
  - 大規模ファイルでは子ノード再帰走査やスコープ管理のオーバーヘッド
- スケール限界:
  - 非同期や並行パース（複数ファイル同時）に最適化されていない設計（ParserのSend/Sync境界は不明）
  - ヒープ割当多用。大量の関数/注釈/docstringでGC/アロケーションが増大
- 実運用負荷要因:
  - I/Oは外部（コード取得）。本モジュールではメモリ/CPUのみ
  - tree-sitterのパースコストは入力サイズ線形。再解析/差分パースは未利用（常にparse）


## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:
- メモリ安全性:
  - Buffer overflow: なし（Rust安全）
  - Use-after-free: なし（所有権/借用に従う）
  - Integer overflow: なし（インデックス計算ほぼなし）
  - UTF-8境界問題: node.byte_range()で`&code[..]`を切り出し。非ASCII識別子/文字列でバイト境界が**文字境界と一致しない場合panic**の恐れがある
- インジェクション:
  - SQL/Command/Path traversal: 未関与（解析のみ）
- 認証・認可: 該当なし
- 秘密情報: ハードコード秘密なし。ログ出力はテストのみ
- 並行性:
  - Race condition/Deadlock: 該当なし（同期化なし）
  - ParserのSend/Sync不明により**並列利用は避けるべき**（このチャンクには境界記載なし）

既知/推定エッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| モジュールdocstring抽出 | `"""mod docs"""`先頭 | Module Symbolへdocstring | 未実装 | 既知GAP（テストで確認） |
| メソッドdocstring抽出 | class内のdefで先頭string | メソッドSymbolにdocstring | 実装済（function扱い） | OK（テストの探索名に注意） |
| import alias（単純） | `import os as o` | path=os, alias=o | 未対応（import_statementでaliased_import未処理） | 不具合 |
| from import alias | `from a import b as c` | path=a.b, alias=c | 実装済（process_aliased_import） | OK |
| ワイルドカード | `from x import *` | is_glob=true | 実装済 | OK |
| タプル代入 | `a, b = f()` | a/bを抽出しない | 非対応（左辺identifierのみ） | 現仕様 |
| *args/**kwargs型 | `def f(*args: T, **kw: U)` | 型付きvarargs表現 | typed_parameterの詳細分岐未対応 | 改善余地 |
| Unicode識別子 | `def 𝒻(): pass` | 正常抽出 | byte_rangeスライスの境界によりpanicの可能性 | 要検証 |
| ネストクラス名解決 | class A: class B: def m | シンボル名`A.B.m` | 実装済（current_class連結） | OK |
| 構文エラー | `def:`など | エラー返却 or 部分抽出 | エラー型未使用、空Vec返し | 改善余地 |

Rust特有の観点:

- 所有権:
  - `parse`は`&mut self`でParser内部状態を変更。ASTツリー/Nodeは値コピーで安全（行番号不明）
- 借用:
  - `code: &str`のスライスを多数返すが参照生存期間は関数内に限定
- ライフタイム:
  - `'a`を使った戻り値（find_calls等）で`&str`は`code`に束縛。明確で安全
- unsafe境界:
  - unsafeブロックなし
- 並行性・非同期:
  - Send/Sync境界不明。`PythonParser`を複数スレッドで共有しない方が安全
  - await境界: 該当なし
  - キャンセル: 該当なし
- エラー設計:
  - ResultではなくVec返しが中心。パース失敗/構文エラーを**黙って無視**する設計
  - panic可能性（UTF-8境界）。unwrap/expectはテストのみ


## Design & Architecture Suggestions

- エラー設計の強化
  - PythonParser::parseを`Result<Vec<Symbol>, PythonParseError>`に変更し、構文エラーやAST生成失敗を伝播
  - find_*系も`Result`を返す選択肢を検討
- モジュールdocstring対応
  - ルート直下の最初のexpression_statementがstringの場合、`<module>` Symbolにdoc_commentを設定
- インポートalias対応拡充
  - import_statementにおける`aliased_import`ノードを処理（現在はdotted_name/identifierのみ）
- UTF-8安全なスライス
  - `&code[node.byte_range()]`の代替として、可能なら**byte→char境界検証**または**lossy処理**を導入。あるいは`from_utf8_unchecked`回避のため、**nodeの範囲テキスト取得ユーティリティ**を実装
- パフォーマンス最適化
  - シグネチャ/docstring構築の文字列結合を最小化（String::with_capacity、Cowの活用）
  - 再帰をループ化（必要なら）や`walk()`の再利用でイテレータ割当軽減
- スコープ/修飾名の一元化
  - メソッド名生成のフォーマット`{class}.{func}`をヘルパー化
- API整備
  - LanguageParser::parse内で`self.parse(...)`を明示的に`PythonParser::parse`へフルパス指定（可読性向上）
- オブザーバビリティ追加（後述）
- ABI-15移行準備
  - ノード名差分チェック、自動テストでABI差異を検出


## Testing Strategy (Unit/Integration) with Examples

既存テストは充実（関数/クラス/メソッド、docstring、呼び出し、インポート、継承、型注釈、async）。追加提案:

- モジュールdocstring
  ```rust
  #[test]
  fn module_docstring_is_extracted() {
      let mut p = PythonParser::new().unwrap();
      let code = r#""\"\"Module docs\"\"""#;
      let syms = p.parse(code, FileId::new(1).unwrap(), &mut SymbolCounter::new());
      let module = syms.iter().find(|s| s.kind == SymbolKind::Module).unwrap();
      assert!(module.doc_comment.as_deref().unwrap().contains("Module docs"));
  }
  ```
- import alias（単純インポート）
  ```rust
  #[test]
  fn import_alias_simple() {
      let mut p = PythonParser::new().unwrap();
      let imports = p.find_imports("import os as o", FileId::new(1).unwrap());
      assert!(imports.iter().any(|i| i.path == "os" && i.alias.as_deref() == Some("o")));
  }
  ```
- Unicode識別子
  ```rust
  #[test]
  fn unicode_identifier_slicing_safe() {
      let mut p = PythonParser::new().unwrap();
      let code = "def 𝒻(): pass";
      let _ = p.parse(code, FileId::new(1).unwrap(), &mut SymbolCounter::new());
      // ここでpanicが起きないことを確認（テストは成功でよし）
  }
  ```
- *args, **kwargs型注釈
  ```rust
  #[test]
  fn varargs_types_in_signature() {
      let mut p = PythonParser::new().unwrap();
      let code = "def f(*args: Any, **kw: Dict[str, Any]) -> None: pass";
      let syms = p.parse(code, FileId::new(1).unwrap(), &mut SymbolCounter::new());
      let f = syms.iter().find(|s| s.name.as_ref() == "f").unwrap();
      assert!(f.signature.as_ref().unwrap().contains("*args"));
      assert!(f.signature.as_ref().unwrap().contains("**kw"));
  }
  ```

統合テスト（多ファイル/大量ノード、ABIアップグレード差分チェック）も推奨。


## Refactoring Plan & Best Practices

- コード構造の分離
  - シンボル抽出、呼び出し抽出、インポート抽出、継承抽出、型抽出を**モジュール別関数群**へ分離
- 共通ユーティリティ
  - `node_to_range`、`slice(code, node)`などの**安全スライス**ラッパを共通化
- コンテキスト管理改善
  - enter_scope/exit_scopeと「親コンテキスト保存/復元」を**RAIIガード**にまとめる（dropでexit+restore）
- 文字列操作の最適化
  - `String::with_capacity`、`push_str`の利用、`Cow<'_, str>`採用
- エラー伝播
  - `PythonParseError::SyntaxError`などを活用する`Result` APIへ移行
- 命名の一貫性
  - `ScopeType::function()` vs `ScopeType::Class` の表記統一
- テストの探索キー是正
  - メソッドdocstring確認時は`"Class.method"`の完全修飾名で探索


## Observability (Logging, Metrics, Tracing)

- 現状: 本体コードはログなし（テストのみprintln）
- 追加提案:
  - ログ: パース開始/終了、ノード種別統計、エラー（初期化/構文）
  - メトリクス: ノード数、シンボル数、抽出時間、docstring抽出成功率、インポート数
  - トレーシング: 関数単位の走査時間、深さ、ノード種別のヒートマップ
  - NodeTrackerの可視化: 記録済みノードの種別一覧をデバッグ出力するフック


## Risks & Unknowns

- tree-sitter ABI依存:
  - ABI-14に固定。ABI-15移行時のノード名/フィールド差異は**不明**（このチャンクには現れない）
- ParserのSend/Sync特性:
  - **不明**（このチャンクには現れない）。並列使用時に安全性問題の可能性
- check_recursion_depthの挙動:
  - しきい値や停止条件は**不明**（このチャンクには現れない）。極端なネストでの停止保証に依存
- NodeTrackingStateの内部構造:
  - 記録の粒度/コストは**不明**（このチャンクには現れない）
- UTF-8境界:
  - tree-sitterのbyte_rangeが常に有効な文字境界を指す保証は**不明**。非ASCIIコードでpanic可能性