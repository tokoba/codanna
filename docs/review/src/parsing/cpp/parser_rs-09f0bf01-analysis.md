# parsing\cpp\parser.rs Review

## TL;DR

- 目的: **tree-sitter** によるC++コードのAST解析から、**シンボル抽出**、**関数/メソッド呼び出し**、**継承関係**、**識別子利用**、**定義**、**変数型**、**クラス内メソッド**、**インポート**を収集するパーサー。
- 公開API: `CppParser::new`, `CppParser::parse`（明示的pubは3件: 構造体とこの2メソッド）。加えて、`LanguageParser`トレイト実装経由で `find_*` 群にアクセス可能（公開可否はトレイトの公開状態に依存・不明）。
- コア複雑箇所: `extract_symbols_from_node` と `extract_calls_recursive` の再帰・分岐ロジック（行番号不明）。
- 重大リスク: 文字列ベースの分割（`::` や `=`）に依存する素朴抽出により、**テンプレート/修飾子/関数ポインタ**などで誤認識の可能性。`extract_doc_comment` は直前ノードのみを参照。
- Rust安全性: `unsafe`なし。返却で `&'a str` を含むAPI（例: `find_calls`）は入力文字列のライフタイムに依存。`tree_sitter::Node::byte_range` に基づくスライスは範囲整合性が前提。
- エラー設計: `new` は `Result<Self, String>`、多くの `find_*` は解析失敗時に空Vecを返却（`Result`ではなく黙殺）。観測性（ログ/メトリクス）は未整備。
- 並行性: すべて `&mut self`、`tree_sitter::Parser` と内部コンテキストは可変状態を持つため**並行使用非推奨**。

## Overview & Purpose

このファイルはC++言語向けのパーサー `CppParser` を定義し、**tree-sitter** から得られるASTを走査して各種のメタ情報（シンボル、呼び出し、継承、識別子利用、定義、変数型、クラス内メソッド、インポート）を抽出します。最終的な目的は、コードインデクサやシンボルテーブル構築、コードナビゲーション、参照解析に必要なデータを提供することです。

主なロジックは以下です（根拠: 関数名、行番号はこのチャンクでは不明）。
- シンボル抽出: `extract_symbols_from_node`（関数/メソッド/クラス/構造体/列挙/クラス内メソッド宣言）
- 呼び出し抽出（呼び出し元コンテキスト付与）: `extract_calls_recursive`
- 呼び出し抽出（簡易）: `extract_calls_from_node`
- 実装位置（Class::method）抽出: `find_implementations_in_node`
- 継承抽出: `find_extends_in_node` + `extract_base_classes_in_node`
- 識別子利用: `find_uses_in_node`
- 変数定義/マクロ: `find_defines_in_node`
- 変数型: `find_variable_types_in_node`
- クラス内メソッド定義: `find_inherent_methods_in_node`
- ドキュメンテーションコメント抽出: `extract_doc_comment`

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | CppParser | pub | C++コード解析の中核。Parser/Context/NodeTrackerを保持 | Med |
| Impl | std::fmt::Debug for CppParser | crate内 | デバッグ表示（言語名） | Low |
| Impl | LanguageParser for CppParser | 不明（トレイト公開に依存） | parseおよび各種find_* APIの提供 | Med |
| Impl | NodeTracker for CppParser | crate内 | 処理したノード種の記録 | Low |
| Fn | CppParser::new | pub | tree-sitterの初期化とC++言語設定 | Low |
| Fn | CppParser::parse | pub | トレイトのparseを委譲 | Low |
| Fn | create_symbol | private | Symbol生成（署名/ドキュメント/モジュール/可視性/スコープ） | Low |
| Fn | extract_symbols_from_node | private | ASTからシンボル抽出（再帰） | High |
| Fn | extract_imports_from_node | private | #include抽出 | Low |
| Fn | extract_calls_from_node | private | 呼び出し抽出（簡易） | Med |
| Fn | extract_calls_recursive | private | 呼び出し抽出（関数コンテキスト追跡） | High |
| Fn | find_*_in_node 群 | private | それぞれの関係/利用/定義の再帰抽出 | Med |
| Fn | extract_methods_from_class_body | private | クラス本体からメソッド名抽出 | Med |

### Dependencies & Interactions

- 内部依存
  - CppParser → ParserContext（スコープ/現在のクラス・関数の追跡）
  - CppParser → NodeTrackingState（処理ノード種の収集）
  - `extract_symbols_from_node` → `check_recursion_depth`（深さガード）
  - `create_symbol` → Symbol/Range/Visibility/ScopeContext（シンボル生成）

- 外部依存（表）

| 依存 | 用途 | 備考 |
|------|------|------|
| tree_sitter::Parser, Node | AST生成/走査 | `parse(code, None)` |
| tree_sitter_cpp::LANGUAGE | 言語設定 | C++グラマー |
| crate::parsing::context::ParserContext | スコープ追跡 | current_class/current_function |
| crate::parsing::parser::check_recursion_depth | 再帰深さ制御 | 詳細不明 |
| crate::parsing::{Import, Language, LanguageParser, NodeTracker, NodeTrackingState} | API/データ構造 | トレイト公開状況は不明 |
| crate::types::{Range, SymbolCounter} | 位置/ID生成 | Rangeは行/列ベース |
| crate::{FileId, Symbol, SymbolKind, Visibility} | シンボル定義 | 可視性はPublic固定で設定 |

- 被依存推定
  - 同一プロジェクト内のインデクサ/解析パイプラインから呼び出される可能性が高い（具体的呼び出し元はこのチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

明示的な公開（pub）は3件（根拠: 本ファイル内のpub宣言。行番号不明）:
- `pub struct CppParser`
- `pub fn CppParser::new() -> Result<Self, String>`
- `pub fn CppParser::parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>`

`LanguageParser` トレイト実装のメソッド群はトレイトの公開状態に依存し、外部から利用可能かは不明。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CppParser::new | `pub fn new() -> Result<Self, String>` | C++用parser初期化 | O(1) | O(1) |
| CppParser::parse | `pub fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | シンボル抽出のエントリポイント | O(N) | O(S) |
| LanguageParser::parse | `fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | AST生成→シンボル抽出 | O(N) | O(S) |
| LanguageParser::find_calls | `fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 呼び出し（呼び出し元関数名付き）抽出 | O(N) | O(C) |
| LanguageParser::find_method_calls | `fn find_method_calls(&mut self, code: &str) -> Vec<MethodCall>` | 呼び出し抽出（呼び出し元不含） | O(N) | O(C) |
| LanguageParser::find_implementations | `fn find_implementations<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | `Class::method` 実装位置抽出 | O(N) | O(I) |
| LanguageParser::find_extends | `fn find_extends<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 継承関係抽出 | O(N) | O(E) |
| LanguageParser::find_uses | `fn find_uses<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 識別子利用抽出 | O(N) | O(U) |
| LanguageParser::find_defines | `fn find_defines<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 変数/マクロ定義抽出 | O(N) | O(D) |
| LanguageParser::find_imports | `fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import>` | `#include` 抽出 | O(N) | O(M) |
| LanguageParser::language | `fn language(&self) -> Language` | 言語種別取得 | O(1) | O(1) |
| LanguageParser::find_variable_types | `fn find_variable_types<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 変数名と型の紐付け抽出 | O(N) | O(T) |
| LanguageParser::find_inherent_methods | `fn find_inherent_methods(&mut self, code: &str) -> Vec<(String, String, Range)>` | クラス内メソッド抽出 | O(N) | O(H) |

詳細（主要API）:

1) CppParser::new
- 目的と責務: **tree-sitter** をC++言語に設定し、解析に必要なコンテキストを初期化。
- アルゴリズム:
  - `Parser::new()` を作成
  - `set_language(tree_sitter_cpp::LANGUAGE)` を適用
  - `ParserContext` と `NodeTrackingState` を初期化
- 引数: なし
- 戻り値:
  - 成功: `CppParser`
  - 失敗: `String`（エラー文）
- 使用例:
  ```rust
  let mut parser = CppParser::new().expect("C++ parser init failed");
  ```
- エッジケース:
  - `set_language`失敗（例: ランタイムでC++言語をロードできない）→ `Err(String)` を返す。

2) CppParser::parse
- 目的と責務: 入力C++コードから**Symbol**を抽出するメイン入口。
- アルゴリズム:
  - `parser.parse(code, None)` でAST生成
  - 失敗時は空`Vec`
  - ルートから `extract_symbols_from_node` 再帰走査
- 引数:

| 引数 | 型 | 説明 |
|------|----|------|
| code | &str | 対象C++ソース |
| file_id | FileId | 呼び出し元が付与するファイルID |
| symbol_counter | &mut SymbolCounter | 一意ID採番 |

- 戻り値:

| 型 | 説明 |
|----|------|
| Vec<Symbol> | 関数/メソッド/クラス/構造体/列挙などのシンボル |

- 使用例:
  ```rust
  let mut parser = CppParser::new().unwrap();
  let mut counter = SymbolCounter::default(); // 実型詳細は不明
  let symbols = parser.parse(r#"class A { void f(); }; void A::f(){}"#, /*file_id*/ 1.into(), &mut counter);
  ```
- エッジケース:
  - 空文字列 → 空Vec
  - 極端なネスト/巨大ファイル → `check_recursion_depth` により深さ制限（詳細不明）で一部ノード未処理の可能性

3) LanguageParser::find_calls
- 目的と責務: 関数定義コンテキスト付きで**関数/メソッド呼び出し**を抽出。
- アルゴリズム:
  - AST生成
  - `extract_calls_recursive(root, code, None, calls)` を呼び出し
    - 関数定義に入ると現在の関数名を特定
    - `call_expression` の `function` フィールドからターゲット名抽出（`field_expression` の `field`を優先）
    - コンテキストが存在する場合のみ `(caller, callee, range)` 追加
- 引数:

| 引数 | 型 | 説明 |
|------|----|------|
| code | &'a str | 対象C++ソース |

- 戻り値:

| 型 | 説明 |
|----|------|
| Vec<(&'a str, &'a str, Range)> | 呼び出し元関数名、呼び出し先名、位置 |

- 使用例:
  ```rust
  let calls = parser.find_calls(r#"void g(){ obj.f(); h(); }"#);
  // 例: [("g", "f", Range{...}), ("g", "h", Range{...})]
  ```
- エッジケース:
  - グローバル初期化式など関数外の呼び出し → コンテキスト無しのため記録されない
  - `operator()` やテンプレート特殊化 → 文字列抽出で誤認の可能性

4) LanguageParser::find_method_calls
- 目的: 呼び出しのみ抽出（呼び出し元コンテキストなし）。
- アルゴリズム: `extract_calls_from_node` により `call_expression` を走査。
- 引数/戻り値/例:
  ```rust
  let calls = parser.find_method_calls(r#"obj.f(); g();"#);
  ```
- エッジケース:
  - 関数ポインタ/関数オブジェクトの呼び出し → `field_expression` でない場合の抽出が曖昧

5) LanguageParser::find_implementations
- 目的: `Class::method` 形式の**メソッド実装**位置抽出。
- アルゴリズム: `function_definition` の `declarator` を文字列検索して `::` 分割。
- エッジケース:
  - `Class<T>::method`、`ns::Class::method`、戻り値修飾子 → 単純分割による誤認

6) LanguageParser::find_extends
- 目的: **継承**関係抽出。
- アルゴリズム: `class_specifier` 内の `base_class_clause` を走査し `type_identifier` を収集。
- エッジケース:
  - `public virtual Base` など修飾 → `type_identifier` 以外の構造に基づく継承の取りこぼし

7) LanguageParser::find_uses
- 目的: **識別子利用**箇所抽出（文脈は未設定）。
- アルゴリズム: `identifier` ノードを列挙して `(context:"", name, range)` 追加。
- エッジケース:
  - 宣言/定義と利用の区別なし、マクロ展開結果の扱いなし

8) LanguageParser::find_defines
- 目的: **変数定義/マクロ**抽出。
- アルゴリズム:
  - `declaration` の `declarator` 文字列を `=` で分割して変数名推定
  - `preproc_def` の `name` からマクロ名抽出
- エッジケース:
  - 連続初期化/構造化束縛/複数宣言 → 誤または部分抽出
  - ポインタ/参照修飾（`*p`, `&r`） → 先頭記号が名前に残る可能性

9) LanguageParser::find_imports
- 目的: **#include** のパス抽出。
- アルゴリズム: `preproc_include` の `path` から `"<...>"` や `"\"...\""` を除去。
- エッジケース:
  - マクロ化されたinclude、条件付きinclude → 未対応

10) LanguageParser::find_variable_types
- 目的: **変数名と型**の組を抽出。
- アルゴリズム: `declaration` の `type` と `declarator` を取得。`=`より前を名前とする。
- エッジケース:
  - `auto`/decltype/テンプレート/複合宣言 → 型/名前抽出の誤り

11) LanguageParser::find_inherent_methods
- 目的: クラス本体内の**メソッド定義/宣言**抽出。
- アルゴリズム: `class_specifier` → `field_declaration_list` → `declarator` の括弧 `(` までをメソッド名と推定。
- エッジケース:
  - 演算子オーバーロード、コンストラクタ/デストラクタ、関数テンプレート → 正規化の曖昧さ

12) LanguageParser::language
- 目的: 言語種別返却。
- アルゴリズム: `Language::Cpp` を返却。

## Walkthrough & Data Flow

- `CppParser::new` でC++言語をセットした `tree_sitter::Parser` と、`ParserContext`/`NodeTrackingState` を準備。
- `CppParser::parse` → `LanguageParser::parse` へ委譲
  - `Parser.parse(code, None)` でAST生成
  - ルートノードから `extract_symbols_from_node` を再帰走査
  - ノード種に応じて `Symbol`（Function/Method/Class/Struct/Enum）を作成し `symbols.push`。
  - クラス内では `ParserContext` でスコープを管理してメソッド検出を補助。

Mermaidフローチャート（主要分岐: `extract_symbols_from_node` の流れ。行番号不明）:

```mermaid
flowchart TD
  A[Start: node] --> B{node.kind}
  B -- function_definition --> C[register_handled_node]
  C --> D{has declarator?}
  D -- yes --> E[Check qualified_identifier in declarator]
  E --> F{current_class is Some or qualified?}
  F -- yes --> G[is_method=true]
  F -- no --> H[is_method=false]
  E --> I[Extract method_name]
  I --> J{method_name empty?}
  J -- yes --> K[fallback: inner declarator field]
  J -- no --> L[Create Symbol (Method/Function)]
  K --> L
  B -- class_specifier --> M[Create Class Symbol]
  M --> N[enter_scope(Class); set_current_class]
  N --> O[Recurse children for methods]
  O --> P[exit_scope; restore context]
  B -- field_declaration --> Q{in class?}
  Q -- yes --> R[has function_declarator?]
  R -- yes --> S[Extract field_identifier as method_name; Create Symbol]
  B -- struct_specifier/enum_specifier --> T[Create Struct/Enum Symbol]
  L --> U[Recurse children]
  S --> U
  T --> U
```

上記の図は `extract_symbols_from_node` 関数の主要分岐を示す（行番号不明）。

呼び出し抽出（`extract_calls_recursive`）の概略（行番号不明）:

```mermaid
flowchart TD
  A[node] --> B{node.kind}
  B -- function_definition --> C[Set function_context (name from declarator)]
  B -- else --> D[Inherit current context]
  D --> E{call_expression?}
  E -- yes --> F[Extract callee from function/field_expression]
  F --> G{function_context exists?}
  G -- yes --> H[push (caller, callee, range)]
  G -- no --> I[skip]
  H --> J[Recurse children with context]
  I --> J
```

対応コード抜粋（`extract_calls_recursive`。行番号不明）:
```rust
fn extract_calls_recursive<'a>(
    node: Node,
    code: &'a str,
    current_function: Option<&'a str>,
    calls: &mut Vec<(&'a str, &'a str, Range)>,
) {
    // ... 関数定義なら function_context を更新
    // ... call_expression なら function/field_expression からターゲット名抽出
    // ... function_context が Some の時のみ push
    /* ... 省略 ... */
}
```

## Complexity & Performance

- 全体の時間計算量は概ね **O(N)**（NはASTノード数、文字列処理はノード数に比例）。
- 空間計算量は抽出結果に比例（シンボル/呼び出し等の件数: **O(R)**）。
- ボトルネック:
  - 再帰走査と複数の `find_*` におけるAST全走査の重複（同じコードに対し複数回`parser.parse`を呼んでいる各API）。解析を共通化できれば改善可能。
  - 文字列ベースの分割（`contains("::")`,`find('=')`）は**誤検知**の温床かつ費用は線形だが不要な抽出を増やす。
- スケール限界:
  - 非同期/並行処理がないため、巨大コードベースでは**逐次処理時間**が増加。
  - 深いネストで `check_recursion_depth` が働くが閾値不明のため解析漏れの可能性。
- 実運用負荷要因:
  - I/Oはこのファイルには現れない。
  - ネットワーク/DBも該当なし。
  - CPU: AST走査と文字列処理が中心。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rustの安全抽象と`tree_sitter`のAPI使用により**該当なし**が妥当。ただし `&code[node.byte_range()]` のスライスは**範囲が不整合**だとpanicしうるが、`tree_sitter`が正しく構築したASTであれば安全（根拠: `find_*`/`extract_*`の各所で利用。行番号不明）。
- インジェクション
  - SQL/Command/Path traversal: **該当なし**。入力はコード文字列であり外部実行なし。
- 認証・認可
  - 権限チェック漏れ / セッション固定: **該当なし**。
- 秘密情報
  - ハードコード秘密/ログ漏洩: **該当なし**。
- 並行性
  - Race condition / Deadlock: `&mut self`設計で単スレッド前提。複数スレッドで同一インスタンス共有は非推奨。**Send/Sync**境界はこのファイルには現れない（不明）。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| クラス外実装（Class::method） | `void A::f(){}` | A, fの抽出 | `find_implementations_in_node` は `declarator` を `::` で分割 | 正常（テンプレート/名前空間を除き） |
| テンプレート付き | `template<class T> void A<T>::f(){}` | A<T>, f抽出 | 単純な`::`分割で `A<T>`を抽出 | 誤認の可能性 |
| 複数変数宣言 | `int a=0,b=1;` | a,b両方定義抽出 | `find_defines_in_node` は`declarator`文字列を丸ごと扱う | 片方/誤抽出の可能性 |
| ポインタ/参照宣言 | `int *p; int &r = x;` | p, r抽出 | `*`/`&`が名前に残る恐れ | 誤抽出の可能性 |
| 演算子オーバーロード | `A::operator+(...)` | `operator+`抽出 | `(`前切り出しや`qualified_identifier`頼み | 抽出精度不明 |
| 連続`///`コメント | `/// a\n/// b` | "a\nb"抽出 | 直前の1行のみ扱う | 欠落 |
| クラス可視性 | `private:` `protected:` | 可視性反映 | 常に `Visibility::Public` で作成 | 誤り |
| 深すぎる再帰 | 非常に深いテンプレート/マクロ展開 | スタック保護で停止 | `check_recursion_depth` に依存 | 不明 |

## Design & Architecture Suggestions

- シンボル抽出の**可視性**対応: `class_specifier` 内の `access_specifier`（`public:`/`private:`/`protected:`）を追跡して `Symbol.visibility` に反映。
- 文字列ベースの抽出改善:
  - `qualified_identifier`/`field_identifier`/`type_identifier` など**AST構造**に基づいて、`::` や `=` の単純分割を減らす。
  - 複数宣言（`,`）はリスト構造から個別に取り出す。
- 再解析の共通化:
  - 各 `find_*` が毎回 `parser.parse` を呼んでいる。解析木を共有（キャッシュ）するAPI設計にして**重複コスト削減**。
- ドキュメントコメント集約:
  - 直前ノードだけでなく、連続`///`や前方コメントブロックを**連結**して抽出。
- エラーハンドリング/観測性:
  - 解析失敗時に**Result**で返し、原因（パースエラー/深さ超過など）を返却/ログ。
  - NodeTrackerの統計（ノード種数、深さ最大、未処理箇所）を**メトリクス**化。
- API整合:
  - `find_calls` と `find_method_calls` の重複を整理し、コンテキスト付与の有無をフラグで切替可能に。
- 並行性を考慮した設計:
  - `CppParser` をスレッドごとに生成し、解析木キャッシュをスレッドローカルにする等。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト方針
  - 正常系: 単純な関数/クラス/メソッド/継承/定義/型/インポートの抽出が期待どおりか。
  - 複雑系: 名前空間/テンプレート/演算子オーバーロード/複数宣言/クラス内可視性。
  - ドキュメントコメント: `/** ... */` と `///` の抽出。

- サンプルテスト（簡略化、行番号不明）:

```rust
#[test]
fn parse_symbols_basic() {
    let mut parser = CppParser::new().unwrap();
    let code = r#"
        /** Class docs */
        class Foo { public: void bar(); private: int x; };
        /// function docs
        void Foo::bar() { printf("hi"); }
        struct S {};
        enum E { A, B };
    "#;
    let mut counter = SymbolCounter::default(); // 実型は不明
    let symbols = parser.parse(code, /*file_id*/ 1.into(), &mut counter);
    // Class Foo, Method bar, Struct S, Enum E, Function?（Foo::bar 実装はMethodとして抽出）
    assert!(symbols.iter().any(|s| s.name == "Foo" && s.kind.is_class()));
    assert!(symbols.iter().any(|s| s.name == "bar" && s.kind.is_method()));
}

#[test]
fn find_calls_with_context() {
    let mut parser = CppParser::new().unwrap();
    let code = r#"void g(){ obj.f(); h(); }"#;
    let calls = parser.find_calls(code);
    assert!(calls.iter().any(|(caller, callee, _)| *caller == "g" && *callee == "f"));
    assert!(calls.iter().any(|(caller, callee, _)| *caller == "g" && *callee == "h"));
}

#[test]
fn find_imports_basic() {
    let mut parser = CppParser::new().unwrap();
    let code = r#"#include <vector>\n#include "my.h""#;
    let imports = parser.find_imports(code, 1.into());
    assert!(imports.iter().any(|i| i.path == "vector"));
    assert!(imports.iter().any(|i| i.path == "my.h"));
}
```

- 統合テスト
  - 大規模ファイルを解析し、`parse` と `find_*` の組み合わせで全体整合性（同一メソッドが両APIで同名になる等）を検証。

## Refactoring Plan & Best Practices

- 共通ASTのキャッシュレイヤーを導入し、`find_*` メソッドはルートノードを受け取る形に変更（または内部でキャッシュ）。
- `extract_symbols_from_node` の分岐を**小関数へ分割**（function/class/field/enum/struct で責務分離）。
- `ParserContext` に可視性トラッキングを追加し、`Symbol.visibility` を適切に設定。
- 文字列操作の**ASTフィールド優先**化（`qualified_identifier` のnameフィールドや、複数宣言リストの処理）。
- エラーを `Result` で返却し、失敗の種別を表現するエラー型を導入（From/Intoによる変換も検討）。
- ドキュメントコメント抽出の**連結処理**と**タグ除去**ロジックの強化。
- テンプレート/名前空間/演算子の**正規化**ユーティリティを追加。

## Observability (Logging, Metrics, Tracing)

- ログ
  - `parse` 失敗時の理由（tree-sitterから得られるエラー情報が乏しければ、入力長/先頭数行を含むデバッグログ）を出力。
  - `check_recursion_depth` のヒット回数と最大深度。
- メトリクス
  - 抽出されたシンボル数、コール数、継承数、定義数など。
  - ノード種別ごとの遭遇回数（既存の `NodeTrackingState` を活用）。
- トレーシング
  - 大規模解析時に関数境界でspanを開始/終了し、時間計測。

## Risks & Unknowns

- `LanguageParser` の公開状態が不明。従って `find_*` 群が外部APIかはこのチャンクでは判断不能。
- `check_recursion_depth` の閾値と方針が不明。どの程度で解析が打ち切られるかにより結果の網羅性が変動。
- `MethodCall::new` の具体仕様（所有/借用）が不明。返却の所有権・ライフタイムに影響しうる。
- `FileId`/`SymbolCounter` の具体型/初期化方法が不明。テスト/使用例は概念的に示した。
- `ParserContext::current_scope_context` の具体内容は不明（ScopeContextの構造/用途がこのチャンクには現れない）。

以上により、API設計の改善（可視性扱い、文字列ベース抽出の排除、AST共通化、観測性強化）を行うことで、実用性と拡張性が大幅に向上します。