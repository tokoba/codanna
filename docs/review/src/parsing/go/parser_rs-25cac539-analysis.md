# parsing\go\parser.rs Review

## TL;DR

- 目的: tree-sitter-goを用いたGoコード解析器。関数/メソッド/型/定数/変数/インポート/呼び出し/型使用/メソッド定義を抽出し、スコープ・可視性・ドキュメントを付与してシンボルへ変換する。
- 公開API: GoParser::new、LanguageParserトrait実装（parse/find_imports/find_calls/find_method_calls/find_uses/find_defines/extract_doc_comment/language）、NodeTracker実装（register_handled_node/get_handled_nodes）。
- コアロジック: 再帰ASTウォークextract_symbols_from_node（条件分岐多数）＋Scope管理（関数・ブロックスコープ）＋受信側/パラメータ処理＋generic型パラメータ抽出＋resolution_contextへの型登録。
- 複雑箇所: 多種ノード種別分岐、メソッドレシーバ・パラメータ・range_clauseの特殊処理、型名抽出（pointer/array/slice/map/channel）とより広いqualified_type対応、ドキュメントコメント抽出の前方兄弟探索。
- 重大リスク: &code[node.byte_range()]のUTF-8境界不一致によるpanicの可能性、module_path未設定でのGoResolutionContext登録の一貫性欠如、関数スコープ種別「hoisting_function」のGo言語仕様との不整合、インポートや型の一部ノード種別未対応による取りこぼし。
- パフォーマンス: 時間O(N)（ASTノード数）、空間O(S+I+C+U+D)（シンボル/インポート/呼び出し/型使用/定義の件数）。再帰と子反復が主、実運用ではコードサイズに線形。
- テスト: インポート抽出/ジェネリクス/可視性/Goの実装関係（空）などが網羅。追加でパニック回避の境界テスト・相対インポート/qualified_typeの扱いなど拡充推奨。

## Overview & Purpose

このファイルは、tree-sitter-goのLANGUAGEを使用するGo言語パーサを提供する。GoParserはコード文字列を解析し、以下を抽出する。

- 関数宣言／メソッド宣言（受信側含む）
- 型宣言（struct/interface/type alias）とそのフィールド／メソッド
- 変数／定数宣言（var/const/short var）
- インポート宣言（単独/グループ/エイリアス/ドット/ブランク/相対）
- 呼び出し（関数/メソッド）
- 型使用箇所（パラメータ／戻り値／フィールド／汎用関数の型引数）
- メソッド定義（受信側タイプ＋メソッド名）

抽出結果はSymbolにまとめ、可視性（先頭大文字→Public）、シグネチャ、ドキュメントコメント、スコープコンテキスト（関数/ブロック/パラメータ/ローカル）を付与する。さらに、GoResolutionContext（型解決用コンテキスト）へ型情報を登録する。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | GoParser | pub | Goコード解析の総合実装（AST解析、シンボル抽出、呼び出し/型使用/定義、インポート抽出、ドキュメント抽出、ノード追跡） | High |
| Impl Trait | LanguageParser for GoParser | pub（トレイト経由） | parse/as_any/extract_doc_comment/find_imports/find_calls/find_method_calls/find_uses/find_defines/find_implementations/find_extends/language | High |
| Impl Trait | NodeTracker for GoParser | pub（トレイト経由） | ハンドリング済みノードの追跡（デバッグ/観測向け） | Low |
| Fn | GoParser::new | pub | tree-sitter-go言語設定と初期化 | Low |
| Fn | GoParser::parse | pub | 1ファイル解析、シンボル抽出、スコープ管理、解決コンテキスト生成 | High |
| Fn | extract_symbols_from_node | private | AST再帰走査の中心、ノード種別ごとに分岐処理 | High |
| Fn | process_function/process_method_declaration | private | 関数/メソッドシンボル生成、スコープ進入、子の再帰処理 | Med |
| Fn | process_type_declaration/process_type_spec | private | struct/interface/type alias抽出、signature/doc/visibility、フィールド/メソッド展開、型登録 | High |
| Fn | process_var_declaration/process_var_spec | private | var宣言抽出（複数名・型） | Med |
| Fn | process_const_declaration/process_const_spec | private | const宣言抽出（複数名・型） | Med |
| Fn | process_short_var_declaration | private | 短変数宣言（:=）抽出、ローカルスコープ設定 | Med |
| Fn | process_method_receiver/process_method_parameters | private | レシーバ/引数の抽出、パラメータスコープ設定 | Med |
| Fn | process_range_clause | private | for rangeのインデックス/値変数抽出、ローカルスコープ設定 | Med |
| Fn | determine_go_visibility | private | 大文字/小文字でPublic/Private判定 | Low |
| Fn | extract_*_signature | private | 構造体/関数/メソッド/インターフェイスのヘッダ部署名抽出 | Med |
| Fn | extract_imports_from_node/process_go_import_* | private | import宣言処理、エイリアス/ドット/ブランク対応 | Med |
| Fn | find_calls/extract_calls_recursive | pub（トレイト経由）/private | 関数呼び出し抽出（selectorを除外） | Med |
| Fn | find_method_calls/extract_method_calls_recursive | pub/private | メソッド呼び出し抽出（selector_expression） | Med |
| Fn | find_uses/extract_type_uses_recursive | pub/private | 型使用抽出（パラメータ、戻り値、フィールド、型引数） | Med |
| Fn | find_defines/extract_method_defines_recursive | pub/private | メソッド定義抽出（受信側タイプとメソッド名） | Med |
| Fn | extract_go_type_name | private | typeノードから名前抽出（pointer/array/slice/map/channel/qualified） | Med |
| Fn | extract_generic_params_from_signature | private | signatureから[]内の型パラメータ名抽出 | Low |
| Fn | create_symbol | private | Symbol構築（signature/doc/module_path/visibility/scope_context設定） | Low |

### Dependencies & Interactions

- 内部依存
  - parse → extract_symbols_from_node（中心再帰）→ 各process_*（宣言/受信側/パラメータ/フィールド/メソッド）→ extract_*_signature / determine_go_visibility / create_symbol
  - extract_symbols_from_node → context.enter_scope/exit_scope, set_current_function/class（スコープ管理）
  - process_type_spec → resolution_context.register_type（型解決用の登録）
  - find_imports → extract_imports_from_node → process_go_import_declaration → process_go_import_spec
  - find_calls/find_method_calls/find_uses/find_defines → 各extract_*_recursive
  - NodeTracker: register_handled_nodeを各主要ノードで呼び出し

- 外部依存（クレート・モジュール）
  | 依存 | 用途 |
  |------|------|
  | tree_sitter::{Parser, Node} | AST構築・ノード走査 |
  | tree_sitter_go::LANGUAGE | Go言語定義の設定 |
  | crate::parsing::{LanguageParser, NodeTracker, ParserContext, ScopeType, Import, HandledNode, MethodCall, NodeTrackingState} | パーサ共通インターフェース、スコープ/ノード追跡、インポート型、メソッド呼び出し型 |
  | crate::{FileId, Range, Symbol, SymbolKind, Visibility} | 識別子/位置/シンボル/可視性 |
  | crate::types::SymbolCounter | ID発行 |
  | super::resolution::GoResolutionContext (+TypeInfo, TypeCategory) | 型登録（struct/interface/alias） |

- 被依存推定
  - 多言語対応の解析フレームワークのGoフロントエンドとして、コードインデックス生成、ナビゲーション、検索、依存関係解析に利用される。
  - find_*系は静的解析機能（呼び出し関係、型使用、メソッド定義一覧）のバックエンドとして利用される。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| GoParser::new | fn new() -> Result<GoParser, String> | パーサ初期化（言語設定） | O(1) | O(1) |
| LanguageParser::parse | fn parse(&mut self, code: &str, file_id: FileId, counter: &mut SymbolCounter) -> Vec<Symbol> | Goコードからシンボル抽出 | O(N) | O(S) |
| LanguageParser::find_imports | fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import> | import宣言抽出 | O(N) | O(I) |
| LanguageParser::find_calls | fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 関数呼び出し抽出 | O(N) | O(C) |
| LanguageParser::find_method_calls | fn find_method_calls(&mut self, code: &str) -> Vec<MethodCall> | メソッド呼び出し抽出 | O(N) | O(Cm) |
| LanguageParser::find_uses | fn find_uses<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 型使用抽出 | O(N) | O(U) |
| LanguageParser::find_defines | fn find_defines<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | メソッド定義抽出 | O(N) | O(D) |
| LanguageParser::extract_doc_comment | fn extract_doc_comment(&self, node: &Node, code: &str) -> Option<String> | 直前の行コメント（//）をドキュメントとして抽出 | O(k) | O(k) |
| LanguageParser::find_implementations | fn find_implementations<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> | Goの暗黙実装は検出不可→空 | O(1) | O(1) |
| LanguageParser::find_extends | fn find_extends<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> | Goは継承なし→空 | O(1) | O(1) |
| LanguageParser::language | fn language(&self) -> crate::parsing::Language | 言語識別子（Go）を返す | O(1) | O(1) |
| LanguageParser::as_any | fn as_any(&self) -> &dyn Any | ダウンキャスト用 | O(1) | O(1) |
| NodeTracker::register_handled_node | fn register_handled_node(&mut self, node_kind: &str, node_id: u16) | ハンドル済みノード登録 | O(1) 平均（HashSet） | O(1) |
| NodeTracker::get_handled_nodes | fn get_handled_nodes(&self) -> &HashSet<HandledNode> | ハンドル済みノード集合取得 | O(1) | O(H) |

詳細（主要APIのみ記述）:

1) GoParser::new
- 目的と責務
  - tree-sitter Parserを生成し、tree_sitter_go::LANGUAGEを設定。パーサ状態とスコープ/解決/ノード追跡を初期化。
- アルゴリズム
  - Parser::new → set_language(&lang.into()) → 成功ならGoParserを構築、失敗はErr文字列。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | なし | - | なし |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Result<GoParser, String> | 成功時GoParser、失敗時エラー文字列 |
- 使用例
  ```rust
  let mut parser = GoParser::new().expect("Go parser init failed");
  ```
- エッジケース
  - set_language失敗時: Err("Failed to set Go language: ...")を返す。

2) LanguageParser::parse（GoParser内部のpub fn parseを呼ぶ）
- 目的と責務
  - コード全体をAST化し、extract_symbols_from_nodeでシンボルを再帰抽出。スコープ/受信側/パラメータ/フィールド/メソッド/変数/定数/短変数/if/for/switch/blockを処理。
- アルゴリズム（ステップ）
  1. ParserContextをリセット、GoResolutionContextをfile_idで作成。
  2. parser.parse(code, None) → rootを抽出。
  3. extract_symbols_from_node(root, ...) を呼び、必要な分岐処理を実施。
  4. 失敗時はeprintlnし、空Vecを返す。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析対象のGoソースコード |
  | file_id | FileId | ファイルID |
  | symbol_counter | &mut SymbolCounter | SymbolId発行カウンタ |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<Symbol> | 抽出されたシンボルのリスト |
- 使用例
  ```rust
  let code = r#"package main; func Hello(){}"#;
  let file_id = FileId::new(1).unwrap();
  let mut counter = SymbolCounter::new();
  let symbols = parser.parse(code, file_id, &mut counter);
  ```
- エッジケース
  - 解析失敗（None）: 標準エラー出力に「Failed to parse Go file」、戻り値は空。
  - module_path: 現状""固定。解決コンテキスト登録時のpackage_pathが空になる可能性。

3) LanguageParser::find_imports
- 目的と責務
  - import_declaration → import_spec/list → パス/エイリアス（package_identifier）/ドット/ブランクを抽出。
- アルゴリズム
  - AST生成→extract_imports_from_node再帰→process_go_import_declaration→process_go_import_specで個々のspecを解析。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | code | &str | Goソース |
  | file_id | FileId | ファイルID |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<Import> | インポート一覧（alias: Some("."/"_"/名前)、is_glob: ドット、is_type_only: false） |
- 使用例
  ```rust
  let imports = parser.find_imports(r#"import f "fmt"; import . "math""#, file_id);
  ```
- エッジケース
  - raw_string_literal (`) と interpreted_string_literal (") の両方に対応。
  - 相対パス("./internal", "../shared")も文字列として受理。

4) LanguageParser::find_calls
- 目的と責務
  - call_expressionのうちselector_expression以外の関数呼び出しを抽出し、呼び出し元関数コンテキストを紐付け。
- アルゴリズム
  - AST→extract_calls_recursive。関数/メソッド/func_literalに入るとコンテキスト更新。call_expressionでfunctionがselector以外ならextract_function_nameで識別子または完全修飾名を抽出。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | code | &'a str | ソース（返却タプルはこのライフタイムに束縛） |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(&'a str, &'a str, Range)> | (caller関数名, 呼び出し関数名, ソース範囲) |
- 使用例
  ```rust
  let calls = parser.find_calls(r#"func A(){ B(); pkg.C() }"#);
  ```
- エッジケース
  - 無名func_literal内: 名前なしの場合、親コンテキスト維持。
  - selector_expressionは除外（メソッド呼び出しはfind_method_callsへ）。

5) LanguageParser::find_method_calls
- 目的と責務
  - selector_expressionによるメソッド呼び出し抽出。MethodCall{caller, method_name, receiver, is_static=false, range}を返す。
- アルゴリズム
  - AST→extract_method_calls_recursive。call_expression→functionがselector_expressionならextract_go_method_signatureでoperand/fieldから受信側とメソッド名を抽出。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | code | &str | ソース |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<MethodCall> | メソッド呼び出しレコード |
- 使用例
  ```rust
  let mcs = parser.find_method_calls(r#"func A(){ x.Do(); (&y).Run() }"#);
  ```
- エッジケース
  - is_staticは型情報なしで判別不能→false固定。
  - 受信側はそのままコード断片（"x"や"(T{})"等）になる。

6) LanguageParser::find_uses
- 目的と責務
  - 型使用箇所抽出（パラメータ、戻り値、structフィールド、var/const型、generic type_arguments）。
- アルゴリズム
  - AST→extract_type_uses_recursive。対象ノードでextract_go_type_reference→extract_go_type_name（pointer/array/slice/map/channel/qualified）へ。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | code | &'a str | ソース |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(&'a str, &'a str, Range)> | (コンテキスト名, 型名, 範囲) |
- 使用例
  ```rust
  let uses = parser.find_uses(r#"func F(x *User) map[string]User { }"#);
  ```
- エッジケース
  - qualified_type（pkg.Type）も抽出できる。
  - channel_type, map_typeなど複合型の中から要素/値型を抽出。

7) LanguageParser::find_defines
- 目的と責務
  - interfaceのmethod_elemとmethod_declarationを抽出し、(受信側タイプ, メソッド名, 範囲)を収集。
- アルゴリズム
  - AST→extract_method_defines_recursive。interfaceの場合は親名未解決→"interface"固定（改善余地）。
- 引数/戻り値/使用例は省略（find_*と同様）
- エッジケース
  - interface名の取得が未実装→"interface"固定。改善推奨。

8) LanguageParser::extract_doc_comment
- 目的と責務
  - ノード直前の連続//コメント行をドキュメントとして抽出。type_specは親type_declarationの前方兄弟を探索。
- アルゴリズム
  - 前方兄弟prev_siblingを逆走査し、//で始まる連続コメントを収集→整形して返却。
- 引数/戻り値/使用例
  ```rust
  // This is a doc
  // Next line
  type T struct{}
  // ノード=type_spec、code=全文
  let doc = parser.extract_doc_comment(&node, code);
  ```
- エッジケース
  - /* ... */ブロックコメントは除外。
  - 空行や非コメントが見えたら停止。

9) LanguageParser::language / as_any / find_implementations / find_extends
- 目的と責務
  - Language::Go返却、ダウンキャスト、Goの実装/継承は空集合返却。

10) NodeTracker::{register_handled_node, get_handled_nodes}
- 目的と責務
  - ハンドリング済みノード種別の記録と取得（観測/デバッグ）。

## Walkthrough & Data Flow

- new
  - Parser生成→LANGUAGE設定→コンテキスト初期化（ParserContext/NodeTrackingState/Option<GoResolutionContext>=None）

- parse（全体フロー）
  1. ParserContextを初期化し、resolution_contextをSome(GoResolutionContext::new(file_id))に。
  2. parser.parse(code, None)→root_node取得。
  3. extract_symbols_from_node(root, ...) 実行。
     - 再帰深さcheck_recursion_depth(depth, node)で防御。
     - ノード種別で分岐し、各宣言を処理・シンボル化・スコープ設定。
     - 子ノードへ再帰。

- extract_symbols_from_nodeの主要分岐（条件分岐多数→図示）

```mermaid
flowchart TD
  A[root_node] --> B{kind}
  B -->|function_declaration| F1[process_function + enter_scope + params + children + exit_scope]
  B -->|method_declaration| F2[process_method_declaration + enter_scope + receiver + params + children + exit_scope]
  B -->|type_declaration| T1[process_type_declaration(type_spec...)]
  B -->|var_declaration| V1[process_var_declaration(var_spec...)]
  B -->|const_declaration| C1[process_const_declaration(const_spec...)]
  B -->|if_statement| S1[enter block + children + exit]
  B -->|for_statement| S2[enter block + range_clause? + children + exit]
  B -->|switch/type_switch| S3[enter block + children + exit]
  B -->|case(default/expression/type)| S4[enter block + children + exit]
  B -->|block| BL[enter block + children + exit]
  B -->|short_var_declaration| SV[process_short_var_declaration]
  B -->|その他| R[children再帰]
```

上記の図は`extract_symbols_from_node`関数（行番号:不明）の主要分岐を示す。

- 宣言処理詳細
  - process_function/process_method_declaration: name/visibility/doc/signature/Range→Symbol化。関数/メソッドスコープ（ScopeType::hoisting_function）へ入って、レシーバ/パラメータをSymbol化、ボディの子を再帰処理。終了後exit_scopeして親コンテキスト復元。
  - process_type_declaration/process_type_spec: name/type_node.kind()でstruct/interface/その他(alias)分岐。signature/doc/visibility/Range作成、generic_params抽出、Symbol追加、resolution_contextへTypeInfo登録（category: Struct/Interface/Alias）。struct→フィールド抽出、interface→method_elem抽出。
  - process_var_spec/process_const_spec: 複数identifierとtype識別、Symbol化。
  - process_short_var_declaration: 左辺のidentifier列を抽出、ローカルスコープ（ScopeContext::Local）に紐付けた変数としてSymbol化。
  - process_range_clause: expression_list/identifierからindex/value名を取得、ローカル変数Symbol化（signatureにindex/valueを明示）。

- ユーティリティ
  - extract_*_signature: ボディ（body/field_declaration_list/method_elemなど）開始直前までの宣言部分をトリム。
  - determine_go_visibility: 先頭文字が大文字ならPublic、それ以外Private。
  - extract_doc_comment: 直前の//行コメント群を整形して返す。

- find_*系
  - find_imports: import_declarationノードを再帰走査し、個別specからImport構築。
  - find_calls/extract_calls_recursive: 関数コンテキスト（function_declaration/method_declaration/func_literal）を追跡し、selector以外のcall_expressionを抽出。
  - find_method_calls/extract_method_calls_recursive: selector_expressionのoperand/fieldから受信側/メソッド名抽出。
  - find_uses/extract_type_uses_recursive: function/methodのparameters/result、structのfield_declaration_list、var_spec/const_spec、call_expressionのtype_argumentsから型使用を抽出。
  - find_defines/extract_method_defines_recursive: interfaceのmethod_elemとmethod_declarationから定義抽出（interface名は固定"interface"）。

## Complexity & Performance

- 時間計算量
  - parse: O(N)（ASTノード数Nに比例）。各ノードで子走査を行うため線形。
  - find_imports/find_calls/find_method_calls/find_uses/find_defines: いずれもAST全体走査でO(N)。
- 空間計算量
  - parse: O(S)（生成されたSymbol数Sに比例）。解決コンテキスト登録も型数に比例。
  - find_*: それぞれ抽出件数（I, C, Cm, U, D）に比例。中間スタックは再帰深さに比例。
- ボトルネック/スケール限界
  - 深いネストの再帰はスタック使用増。check_recursion_depthで抑制しているが深さ閾値は別モジュール（不明）。
  - 文字列スライス作成（&code[node.byte_range()]）が多用され、UTF-8境界不一致があるとpanicの可能性（詳細は次節のSecurity）。
  - signature抽出でボディ先頭探索に線形走査が入る場面あり（ただしノードに限定されるため軽微）。
- 実運用負荷要因
  - 大規模ファイルや多数ファイルの解析でパーサ生成/AST構築時間（tree-sitter側）と再帰走査コストが支配的。
  - インポート/型登録の件数が大きい場合、解決コンテキストの内部Map操作（O(1)平均）によるメモリ消費。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - unsafeブロック: なし（行番号:不明）。
  - &strスライス: &code[node.byte_range()]でUTF-8境界に合わないバイト範囲を指定するとpanicの可能性。tree-sitterはバイトオフセットを返すため、ノード範囲が文字境界と一致しない場合に問題化しうる。防御として、文字境界チェックまたはString::from_utf8_lossyによる緩和が必要。
  - 所有権/借用: find_*系は戻り値に&'a strを含み、codeのライフタイムに正しく束縛。関数内の&strは短命で、Vec<String>へ必要に応じて所有化済みのため、Use-after-freeの懸念はない。
  - 整数オーバーフロー: Range生成時のキャスト（row as u32, column as u16）は極端に長い行/列で桁溢れの可能性は理論上あるが、現実的には非常に稀。

- インジェクション
  - SQL/Command/Path traversal: 該当なし（このモジュールは解析のみ）。
  - ログインジェクション: eprintlnを用いるが、ユーザ入力を直接コマンドへ渡していないため重大な懸念は低い。

- 認証・認可
  - 該当なし。

- 秘密情報
  - ハードコードされたシークレット: なし。
  - ログ漏えい: eprintln("Failed to parse Go file")にコード内容は含まれない。安全。

- 並行性
  - &mut selfを要求するAPI（parse/find_*）により同時実行は不可。tree_sitter::Parserは通常&mutを要求するため、スレッド間共有は避ける設計。データ競合やデッドロックは現状発生しないが、外部から並行に同一インスタンスを操作しないようにする必要がある。

- 既知/潜在バグ
  - module_pathがparseから常に""で供給されており、TypeInfo.package_pathに空文字が入る可能性。型解決の精度低下。
  - ScopeType::hoisting_functionの使用とコメント「Goはhoistingしない」が不整合。スコープルールの誤表示/誤用リスク。
  - extract_interface_signatureのend = body_start.saturating_sub(2)はフォーマット前提が強く、署名抽出が不正確になる可能性。
  - process_*_spec/type抽出が"qualified_type"や他の複合型ノードに完全には対応していない箇所がある（var_spec/const_specではqualified_type未対応）。型シグネチャが欠落し得る。
  - find_definesのinterface名が"interface"固定。実名を付与できず、分析精度が落ちる。

- セキュリティチェックリスト評価表

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列解析 | "" | 空シンボル、エラー表示なし | parseでparser.parseがSomeなら空走査、Noneならeprintln | 概ねOK |
| 非UTF-8境界スライス | コメントに多バイト | panicせず安全に文字列抽出 | &code[byte_range]使用 | 要対策 |
| 極端な行・列 | 10^9桁 | Rangeのキャストで健全 | as u32/u16 | 低リスク |
| ドット/ブランクインポート | import . "fmt", import _ "db/sql" | alias="."/"_"、is_glob=true/false設定 | 実装済み | OK |
| 相対インポート | "./internal" | パス抽出のみ | 実装済み | OK |
| method_elemのinterface名 | type IF interface { M() } | IFで返す | "interface"固定 | 改善要 |
| structフィールド複数名 | "W, H float64" | 2フィールド抽出 | 実装済み | OK |
| range句の片側のみ | "for i := range v" | indexのみ抽出 | 実装済み | OK |
| qualified_typeのvar | "var u pkg.User" | 型名"pkg.User"抽出 | var_specで未対応 | 改善要 |

注: 行番号はこのチャンクに明示されていないため「行番号:不明」。

## Design & Architecture Suggestions

- module_pathの確定
  - package宣言からパッケージ名を抽出し、resolution_contextのpackage_pathへ反映する処理を追加。parseの最初のAST走査時に"package_identifier"を検知し、module_pathへ設定する。

- スコープ種別の整合性
  - ScopeType::hoisting_functionの使用はGoの仕様と不整合。ScopeType::Functionへ変更、コメントを正す。

- 型ノードの網羅性向上
  - var_spec/const_spec/parameter/fieldの型抽出に"qualified_type"を追加し、全箇所で共通関数（extract_go_type_reference）を用いるようにリファクタ。

- interface名の取得
  - extract_method_defines_recursiveで親type_specを遡ってnameを取得し、"interface"固定を解消。

- UTF-8境界安全なスライス
  - byte_rangeで得た範囲が文字境界か検査するか、可能ならノードテキスト取得を別APIで行う（たとえばコード断片の所有化: String::from_utf8_lossy(code[byte_range].as_bytes())、あるいは安全なトークン化）。

- エラー設計
  - parseで解析失敗時にResult型へ拡張して呼び出し側で取り扱い可能に。eprintlnはロギング層へ移譲。

- 汎用ヘルパーの統合
  - signature抽出系や型抽出系の重複を統合し、ノード種別パターンのスライスをテーブル化することで保守性を上げる。

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト（このファイル内）
  - test_go_import_extraction: 標準/グループ/エイリアス/ドット/ブランクを検証。
  - test_go_generic_type_extraction: ジェネリック関数/struct/interfaceのシグネチャ抽出。
  - test_go_interface_implementation_behavior: 実装/継承が空であることの検証。
  - test_go_complex_import_patterns, test_go_import_path_formats: 複雑なインポートパターン、相対含む。
  - test_go_visibility_variations: Public/Private可視性の検証。

- 追加推奨テスト
  1. UTF-8境界テスト
     ```rust
     #[test]
     fn test_utf8_boundary_slices() {
         let mut parser = GoParser::new().unwrap();
         let code = "package main\n// コメント😊\nfunc こんにちは(){}\n";
         let mut counter = SymbolCounter::new();
         let file_id = FileId::new(1).unwrap();
         let symbols = parser.parse(code, file_id, &mut counter);
         assert!(symbols.iter().any(|s| s.name.contains("こんにちは")));
     }
     ```
  2. qualified_typeのvar/const/param/field
     ```rust
     #[test]
     fn test_qualified_type_extraction() {
         let mut parser = GoParser::new().unwrap();
         let code = r#"package p; var u pkg.User; type T struct { f pkg.Type }"#;
         let uses = parser.find_uses(code);
         assert!(uses.iter().any(|(_, t, _)| *t == "pkg.User"));
         assert!(uses.iter().any(|(_, t, _)| *t == "pkg.Type"));
     }
     ```
  3. interface名取得の改善テスト（実装後）
  4. package名→module_path設定テスト
  5. range_clauseの多変数/1変数の両方テスト
  6. short_varで複数左辺（a, b := ...）の検証
  7. エラー時Result返却（インタフェース変更時）

- インテグレーションテスト
  - 複数ファイルの解析でresolution_contextの型登録と検索が一貫して行われることを検証（このファイル内ではGoResolutionContextの詳細が不明）。

## Refactoring Plan & Best Practices

- 反復する型抽出ロジックの統一
  - "type_identifier" | "pointer_type" | "array_type" | "slice_type" | "map_type" | "channel_type" | "qualified_type" を1箇所に定義し、参照するヘルパーを導入。

- extract_*_signatureの共通化
  - bodyやリスト開始位置を見つけてヘッダ抽出する汎用関数を作り、struct/interface/function/methodで再利用。

- スコープAPIの明確化
  - enter_scope/exit_scopeとcurrent_function/current_classの保存・復元の順序をユーティリティでラップし、例外パスでも必ず復元されるようにする（RAII風ガード）。

- ロギングとエラーの分離
  - eprintlnの使用を抑え、Resultで返すか、観測層（Observability）へ。

- module_pathの処理追加
  - package宣言を解析してmodule_pathへ反映。resolution_contextへの登録精度を上げる。

- find_definesのinterface名解決
  - method_elemの祖先type_specを辿る関数を導入して、正しいインターフェイス名を特定。

## Observability (Logging, Metrics, Tracing)

- 既存
  - parse失敗時eprintlnのみ。NodeTrackerはハンドル済みノード種別を保持（観測に活用可能）。

- 推奨
  - ログ（info/debug/warn/error）レベル導入、構造化ログ（ノード種別、位置、件数）を出力可能に。
  - Metrics
    - 解析時間（per file）、抽出シンボル総数、インポート数、呼び出し数、型使用数、定義数。
    - ノード種別カバレッジ（NodeTrackerのHashSetサイズ）。
  - Tracing
    - 関数境界（parse開始/終了）、再帰深さの警告、スコープのenter/exitイベント（デバッグ時のみ）。

## Risks & Unknowns

- GoResolutionContextの詳細はこのチャンクには現れない。型登録後の解決・検索仕様は不明。
- check_recursion_depthの閾値やポリシーは別モジュールで未確認。
- ParserContext/ScopeType::hoisting_functionの正確な意味付けは不明（Go仕様との違いが懸念）。
- tree_sitter_goのLANGUAGE.into()がABI-15に対応とあるが、言語機能（Go1.22等）に対する完全カバレッジは不明。
- 文字列スライスの安全境界保証はtree-sitterのノード境界に依存。多バイト文字混在時の境界一致は未検証。