# parsing\c\parser.rs Review

## TL;DR

- 目的: Tree-sitterを用いたC言語ASTからの**シンボル抽出**、**インポート抽出**、**呼び出し・使用・定義**の解析を提供する。
- 主要公開API: **CParser::new**, **CParser::parse**（inherent）、および**LanguageParser**トレイトの各メソッド（find_calls/find_method_calls/find_uses/find_defines/find_imports 等）。
- コアロジック: 再帰関数**extract_symbols_from_node**でノード種別ごとにシンボル化、**create_symbol**でC特有の可視性（static）を反映。
- 重大リスク: 
  - 文字列スライスにおけるUTF-8境界の不一致による**潜在panic**（Node::byte_rangeをそのまま&strへ適用）。
  - 分岐の一部で**子ノードの二重走査**により、シンボルの**重複生成**が発生する可能性（例: declaration/init_declarator、struct/union/enum 内部）。
- Rust安全性: unsafeなし。所有権・借用は概ね安全だが、文字列スライスの境界・u16カラムへのダウンキャストに要注意。
- 並行性: 全APIが**&mut self**でパーサを共有、**スレッド非安全**（同時使用不可）。非同期なし。
- エラー設計: 解析失敗時は**空Vec返却**で黙示的に失敗を隠す。Resultでのエラー伝播を検討すべき。

## Overview & Purpose

このファイルは、Tree-sitterのC言語グラマーを用いてソースコードを解析し、以下を抽出するC言語パーサの実装である。

- シンボル抽出（関数、構造体/共用体、列挙、定数、変数、フィールド、パラメータ、マクロ）
- インポート（`#include`）解析
- 関数呼び出し（ASTベース）・識別子使用・定義の抽出
- ノードトラッキング（処理済み種別を記録）とスコープ管理（モジュール/関数/ブロック/クラスとしての構造体）

Tree-sitterのASTノード種別（kind）に応じてシンボルを生成し、スコープ・可視性などのC特有の属性を付与する。大量の分岐を有する再帰的トラバーサルが中核。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | CParser | pub | C言語用パーサ。Tree-sitterパーサ、スコープコンテキスト、ノードトラッカー保持 | Med |
| Impl (inherent) | CParser::new | pub | Tree-sitter C言語設定と初期化 | Low |
| Impl (inherent) | CParser::parse | pub | LanguageParser::parse の委譲（利便性API） | Low |
| Impl (private) | extract_imports_from_node | private | ASTから`#include`を再帰抽出 | Low |
| Impl (private) | create_symbol | private | Range/可視性/スコープを含むSymbol生成 | Med |
| Impl (private) | find_function_name_node | private | Cの複雑な宣言子から関数名ノード抽出 | Med |
| Impl (private) | find_declarator_name | private | 変数/パラメータ名抽出 | Med |
| Impl (private) | extract_symbols_from_node | private | AST全体のシンボル抽出（コア） | High |
| Impl (private) | extract_calls_from_node | private | call_expressionからMethodCall抽出 | Low |
| Impl (private) | find_calls_in_node | private | 簡易な関数呼び出し抽出（タプル） | Low |
| Impl (private) | find_uses_in_node | private | 識別子使用箇所の抽出 | Low |
| Impl (private) | find_defines_in_node | private | 変数宣言/マクロ定義抽出（簡易） | Low |
| Trait impl | NodeTracker | pub (impl) | 処理済みノード種別の収集 | Low |
| Trait impl | LanguageParser | pub (impl) | パース/抽出API群（calls/uses/defines/imports, language等） | Med |

### Dependencies & Interactions

- 内部依存
  - CParser::parse → LanguageParser::parse（委譲）
  - LanguageParser::parse → extract_symbols_from_node（深い再帰）
  - extract_symbols_from_node → create_symbol, find_function_name_node, find_declarator_name, ParserContext（enter_scope/exit_scope）, NodeTracker（register_handled_node）
  - find_* 系 → 各専用の再帰関数（find_calls_in_node, extract_calls_from_node, find_uses_in_node, find_defines_in_node, extract_imports_from_node）

- 外部依存（主なもの）
  | クレート/モジュール | 用途 |
  |--------------------|------|
  | tree_sitter::{Node, Parser} | AST生成と走査 |
  | tree_sitter_c::LANGUAGE | C言語グラマー |
  | crate::parsing::{LanguageParser, ParserContext, NodeTrackingState, NodeTracker, ScopeType, Import, Language, HandledNode} | 解析インタフェース/スコープ/トラッキング/言語情報 |
  | crate::types::{Range, SymbolCounter} | 範囲表現、シンボルID採番 |
  | crate::{FileId, Symbol, SymbolKind, Visibility?} | シンボルモデルとファイルID、可視性 |

- 被依存推定
  - 上位のインデクサ/コードインテリジェンス（シンボル表/参照解析）、言語汎用パーサファクトリ、グラフ生成（呼び出し関係）、ナビゲーション機能（Find Usages, Go to Definition）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CParser::new | `pub fn new() -> Result<Self, String>` | Cパーサの初期化（Tree-sitterにC言語設定） | O(1) | O(1) |
| CParser::parse | `pub fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | シンボル抽出の利便メソッド（LanguageParser::parse委譲） | O(N) | O(S) |
| LanguageParser::parse | `fn parse(&mut self, code: &str, file_id: FileId, symbol_counter: &mut SymbolCounter) -> Vec<Symbol>` | AST構築→シンボル抽出（中核） | O(N) | O(S) |
| LanguageParser::find_calls | `fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 呼び出しをタプルで抽出 | O(N) | O(C) |
| LanguageParser::find_method_calls | `fn find_method_calls(&mut self, code: &str) -> Vec<MethodCall>` | 呼び出しを構造体で抽出 | O(N) | O(C) |
| LanguageParser::find_uses | `fn find_uses<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 識別子使用の抽出 | O(N) | O(U) |
| LanguageParser::find_defines | `fn find_defines<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)>` | 変数宣言/マクロ定義抽出 | O(N) | O(D) |
| LanguageParser::find_imports | `fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import>` | `#include`抽出 | O(N) | O(I) |
| LanguageParser::extract_doc_comment | `fn extract_doc_comment(&self, _node: &Node, _code: &str) -> Option<String>` | ドキュメントコメント抽出（Cでは未対応） | O(1) | O(1) |
| LanguageParser::language | `fn language(&self) -> Language` | 言語種別返却（C） | O(1) | O(1) |
| NodeTracker::get_handled_nodes | `fn get_handled_nodes(&self) -> &HashSet<HandledNode>` | 処理済みノード参照 | O(1) | O(H) |
| NodeTracker::register_handled_node | `fn register_handled_node(&mut self, node_kind: &str, node_id: u16)` | 処理済みノード登録 | O(1) | O(1) |

各APIの詳細:

1) CParser::new
- 目的と責務: Tree-sitterパーサをC言語に設定し、コンテキスト/トラッカーを初期化。
- アルゴリズム:
  1. Parser::new
  2. set_language(tree_sitter_c::LANGUAGE.into())
  3. ParserContext::new, NodeTrackingState::new
- 引数: なし
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Result<CParser, String> | 成功時CParser、失敗時エラーメッセージ |
- 使用例:
  ```rust
  let mut parser = CParser::new().expect("C parser init failed");
  ```
- エッジケース:
  - 言語設定に失敗: Err(String)で理由が返る。

2) CParser::parse（inherent）
- 目的と責務: LanguageParser::parseへ委譲してシンボル抽出。
- アルゴリズム:
  1. `<Self as LanguageParser>::parse(self, ...)` を呼ぶのみ
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | code | &str | 解析対象Cコード（UTF-8） |
  | file_id | FileId | ファイル識別子 |
  | symbol_counter | &mut SymbolCounter | シンボルID採番器 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Vec<Symbol> | 抽出されたシンボル一覧 |
- 使用例:
  ```rust
  let mut sym_counter = SymbolCounter::default();
  let symbols = parser.parse(r#"
    #include <stdio.h>
    static int s = 0;
    int add(int a, int b) { return a + b; }
    struct Point { int x; int y; };
  "#, file_id, &mut sym_counter);
  ```
- エッジケース:
  - 空文字入力: 空Vec返却。
  - 破損入力: Tree-sitterのパースがNoneなら空Vec返却。

3) LanguageParser::parse（実装）
- 目的と責務: AST生成→抽出の中核。スコープリセットと再帰トラバース。
- アルゴリズム:
  1. `self.context = ParserContext::new()`（スコープ初期化）
  2. `self.parser.parse(code, None)`でTree生成
  3. ルートノードを取得
  4. `extract_symbols_from_node(root, ...)` で再帰抽出
- 引数/戻り値: 上記と同様
- 使用例: 上記CParser::parse参照
- エッジケース:
  - 大規模ファイル: 再帰深さ制限により探索が中断し得る（check_recursion_depth）。

4) LanguageParser::find_calls / find_method_calls
- 目的: 関数呼び出しの抽出（タプル or MethodCall）。
- アルゴリズム:
  1. Tree-sitterでパース
  2. rootから`call_expression`を探索
  3. 子フィールド`function`のテキストを抽出
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | code | &str | 入力コード |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Vec<(&str, &str, Range)> | (caller="", callee, 範囲)のタプル |
  | Vec<MethodCall> | caller=""でのメソッド呼び出し（範囲付） |
- 使用例:
  ```rust
  let calls = parser.find_calls("int main(){ add(1,2); printf(\"%d\", 3); }");
  for (_caller, callee, range) in calls {
      println!("call to {} at {:?}", callee, range);
  }
  let mcalls = parser.find_method_calls("main(){ foo(); bar(); }");
  ```
- エッジケース:
  - 関数ポインタ/メンバアクセス: `function`子が複合式の場合、名前が式文字列になる。

5) LanguageParser::find_uses
- 目的: 識別子使用の抽出。
- アルゴリズム: `identifier`ノードを列挙し、そのテキストと範囲を記録。
- 使用例:
  ```rust
  let uses = parser.find_uses("int x; x = y + func(z);");
  ```
- エッジケース:
  - 宣言部のidentifierも含まれる（使用と宣言の区別なし）。

6) LanguageParser::find_defines
- 目的: 変数宣言と`#define`の抽出（簡易）。
- アルゴリズム:
  - `declaration`で子フィールド`declarator`のテキストから`=`前を変数名と推測（C grammar的に不安定）。
  - `preproc_def`で`name`を抽出。
- 使用例:
  ```rust
  let defs = parser.find_defines("#define MAX 10\nint a=3; int b;");
  ```
- エッジケース:
  - 複数宣言（`int a,b;`）や`init_declarator_list`には未対応。精度低。

7) LanguageParser::find_imports
- 目的: `#include "..."`/`<...>`の抽出。
- 使用例:
  ```rust
  let imports = parser.find_imports(r#"#include "mylib.h""#, file_id);
  ```
- エッジケース:
  - パスにクォート/山括弧のトリム処理あり。

8) LanguageParser::language
- 目的: 言語識別子（C）を返す。

9) NodeTracker（get_handled_nodes/register_handled_node）
- 目的: 処理済みノード種別の記録。観測やデバッグ用。

## Walkthrough & Data Flow

- 初期化: CParser::newでTree-sitterをC言語に設定し、コンテキストとトラッカーを初期化。
- 解析フロー:
  1. LanguageParser::parseでコンテキストをリセットし、ASTを構築。
  2. ルート（translation_unit）から**extract_symbols_from_node**を再帰呼び出し。
  3. 各ノード種別に応じて:
     - 関数定義: find_function_name_nodeで名前ノードを抽出、create_symbolでFunction登録。関数スコープに入って本文を再帰処理。
     - 構造体/共用体: nameでStruct登録、bodyをClassスコープで再帰処理。
     - 列挙: nameでEnum、enumeratorでConstant登録。
     - 変数宣言: declaration/init_declaratorでVariable登録。
     - フィールド宣言: field_declaratorのfield_identifierでField登録。
     - プリプロセッサ: include/def/callでMacro登録、条件ディレクティブは子を再帰。
     - 制御構文: if/while/for/do/switchはBlockスコープを開閉。
  4. create_symbolで範囲Range、可視性（static→Private、それ以外→Public）、scope_contextを設定。

- 文字列抽出: `&code[node.byte_range()]`で識別子/パス等のテキストを取り出す。

- 呼び出し/使用/定義/インポート抽出は、それぞれの再帰関数が独立してASTを走査。

### Mermaid（extract_symbols_from_nodeの主要分岐）

```mermaid
flowchart TD
  A[extract_symbols_from_node(node, depth)] --> B{check_recursion_depth}
  B -- false --> Z[return]
  B -- true --> C{node.kind()}
  C --> C1[translation_unit\nenter Module scope\nrecurse children\nexit scope\nreturn]
  C --> C2[function_definition\nfind name -> Function symbol\nenter Function scope\nrecurse\nexit scope\nreturn]
  C --> C3[struct_specifier\nname -> Struct symbol\nenter Class scope\nrecurse body\nexit scope\n(then default recursion - potential duplicate)]
  C --> C4[union_specifier\nStruct symbol\nClass scope\nrecurse body\n(then default recursion - potential duplicate)]
  C --> C5[enum_specifier\nname -> Enum\nenumerator -> Constant\n(then default recursion - potential duplicate)]
  C --> C6[declaration\ninit_declarator -> Variable\n(then default recursion - potential duplicate)]
  C --> C7[init_declarator\nVariable\n(then default recursion)]
  C --> C8[compound_statement\nenter Block scope\nrecurse children\nexit scope\nreturn]
  C --> C9[parameter_declaration\nParameter\n(then default recursion)]
  C --> C10[field_declaration\nfield_identifier -> Field\n(then default recursion)]
  C --> C11[preproc_include/def/call\nMacro symbol\nrecurse children\n(一部returnあり)]
  C --> C12[if/while/for/do/switch\nenter Block\nrecurse\nexit\nreturn]
  C --> C13[その他\nデフォルトで子を再帰]
```

上記の図は`extract_symbols_from_node`関数（行番号:不明）の主要分岐を示す。

## Complexity & Performance

- パース（Tree-sitter）: O(L)（L=入力コード長）
- 再帰走査（シンボル抽出）: O(N)（N=ASTノード数）
- メモリ使用: O(S)（S=抽出されたシンボル/呼び出し等の件数）
- ボトルネック:
  - 再帰深さと大量分岐。大規模ファイルでは関数呼び出し頻度が高い。
  - 一部分岐で**子ノードを手動再帰後に再度デフォルト再帰**しており、処理重複と出力重複でCPU/メモリに悪影響。
  - 文字列スライスは`&str`の再割当・検証コスト。`utf8_text`活用で安全性を上げつつ若干のコスト増。
- 実運用負荷:
  - I/Oなし、純CPU。Tree-sitterの解析は高速だが重複処理を削減すべき。
  - ネットワーク/DBアクセスなし。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性
  - 文字列スライスのUTF-8境界: `&code[node.byte_range()]`は**非UTF-8境界でpanic**の可能性（例: 多バイト文字を含む識別子/コメントなど）。安全な代替: `node.utf8_text(code.as_bytes())`.
  - 整数ダウンキャスト: `column as u16`は非常に長い行で**オーバーフロー**し得る。u32等へ拡張を検討。
  - Buffer overflow / Use-after-free: 該当なし（Rust安全/所有権管理内）。
- インジェクション
  - SQL/Command/Path traversal: 該当なし（実行/ファイルアクセスなし）。includeパスを扱うが解析のみ。
- 認証・認可
  - 該当なし。
- 秘密情報
  - 該当なし。ログ出力もなし。
- 並行性
  - &mut self要求のAPIで**同時実行不可**。Parserの内部状態共有により**Send/Sync**保証は不明。このままでは複数スレッドからの同時使用で**データ競合**の可能性（このチャンクではトレイト境界不明）。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| UTF-8境界問題 | `#include "日本語.h"` | 正常抽出・panicなし | `&code[path_node.byte_range()]` で抽出 | 要修正（utf8_text推奨） |
| 変数重複抽出 | `int x = 1;` | Variableを1件 | `declaration`で抽出後、子`init_declarator`も再帰で抽出 | 要修正（重複防止） |
| 構造体フィールド重複 | `struct S { int a; };` | Fieldを1件 | `struct_specifier`内で手動再帰後にデフォルト再帰でも再探索 | 要修正（return追加） |
| unionをStructとして扱う | `union U { int a; };` | Unionシンボル | `SymbolKind::Struct`に固定 | 設計判断（明示化検討） |
| 列挙子重複 | `enum E { A, B };` | Constant A/Bを各1件 | `enum_specifier`内処理+デフォルト再帰で再探索 | 要修正（return追加） |
| find_definesの不正確さ | `int a, b;` | a/bそれぞれ定義検出 | `declarator`フィールド依存（C grammar的に不確実） | 要改善（init_declarator_list解析） |
| 非ASCII識別子 | `int 変数 = 0;` | 正常抽出 | byte_range→&strでpanic可能 | 要修正（utf8_text） |

## Design & Architecture Suggestions

- シンボル抽出の重複防止
  - 方針: 「特別処理した分岐」では**必ずreturn**してデフォルト再帰を抑止。
  - 該当分岐: struct_specifier, union_specifier, enum_specifier, declaration, init_declarator, parameter_declaration, field_declaration, preproc_include/def（必要に応じ）。
- テキスト抽出の安全化
  - `&code[node.byte_range()]`を**`node.utf8_text(code.as_bytes())`**に置換し、`Result<&str, Utf8Error>`を処理。panic回避。
- 可視性判定の強化
  - `storage_class_specifier`の探索は親ノード限定。宣言全体（祖先）まで広げるか、型指定子集合を解析して**static**を精度高く検出。
- AST分岐設計の簡素化
  - `declaration`と`init_declarator`の責務を一本化し、**片側のみ**でVariable生成。
  - 構造体/共用体/列挙は「名前シンボル生成＋必要ならボディ専用関数でフィールド/列挙子抽出」に分離。
- シンボル種別の整合性
  - `preproc_include`をMacroではなく**Import系のシンボル**または別の種別へ。Symbolモデルに種別追加が必要なら検討。
- エラー設計
  - `parse`/`find_*`で**Result**を返す選択肢。不正UTF-8やTree-sitter失敗を明示。
- スレッド安全性/再入性
  - パーサを短命インスタンスにして各APIで内部に新規Parser作成（コスト増）か、**外側で同期**（Mutex）を推奨。
  - `&mut self`のままなら「同時使用不可」をAPIドキュメントに明記。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト（最小AST断片）
  - 正常系: 関数/変数/構造体/列挙/フィールド/マクロ/インポート抽出。
  - 重複防止: `int x = 1;`でVariableが1つであること。
  - 可視性: `static`を含む宣言でPrivateになること。
  - UTF-8: 非ASCIIを含む識別子・includeでpanicしないこと（utf8_text採用後）。

- 統合テスト（複合コード）
  - ネストした制御構文/ブロックスコープ、前処理ディレクティブとの組み合わせ。

例:

```rust
#[test]
fn extracts_function_variable_struct_enum() {
    let mut parser = CParser::new().unwrap();
    let mut sc = SymbolCounter::default();
    let code = r#"
        #include "mylib.h"
        static int s = 0;
        int add(int a, int b) { int local = a+b; return local; }
        struct Point { int x; int y; };
        enum Color { RED, GREEN=2, BLUE };
    "#;
    let syms = parser.parse(code, FileId(1), &mut sc);

    // 期待: 関数add, パラメータa/b, ローカルlocal, static s, struct Point, fields x/y, enum Color, const RED/GREEN/BLUE など
    // 重複がないこと（現状バグあり -> 期待値は修正後）
    assert!(syms.iter().any(|s| s.name == "add" && s.kind.is_function()));
    assert!(syms.iter().any(|s| s.name == "Point" && s.kind.is_struct()));
    assert!(syms.iter().any(|s| s.name == "RED" && s.kind.is_constant()));
    // 可視性チェック
    let s_sym = syms.iter().find(|s| s.name == "s").unwrap();
    assert!(s_sym.visibility.is_private());
}

#[test]
fn finds_calls_and_imports() {
    let mut parser = CParser::new().unwrap();
    let calls = parser.find_calls("int main(){ add(1,2); printf(\"OK\"); }");
    assert!(calls.iter().any(|(_, name, _)| *name == "add"));
    assert!(calls.iter().any(|(_, name, _)| *name == "printf"));

    let imports = parser.find_imports("#include <stdio.h>\n#include \"x.h\"", FileId(1));
    assert_eq!(imports.len(), 2);
}
```

## Refactoring Plan & Best Practices

- 重複除去のためのreturn徹底
  - struct_specifier/union_specifier/enum_specifier/declaration/init_declarator/parameter_declaration/field_declaration分岐の末尾で**return**。
- 文字列抽出の安全化
  - 全ての`&code[node.byte_range()]`を**`node.utf8_text(code.as_bytes())?`**に変更、`Result`化。
- 論理の明確化
  - `find_defines_in_node`: C grammarに沿い、`init_declarator_list`から各`init_declarator`の識別子を取得。
- 可視性の安定化
  - 親ノードのみでなく、宣言全体のspecifiersから**static**判定（必要なら木を上方向に探索）。
- エラーハンドリング
  - LanguageParser各関数に`Result<_, ParseError>`採用、None→Errで理由を区別。
- パフォーマンス
  - 再帰走査中で不要な`children(&mut walk())`の多重呼び出し削減（カーソル再利用）。
- APIドキュメント
  - &mut selfによる再入性制限とスレッド非安全性を明記。

## Observability (Logging, Metrics, Tracing)

- 現状ログ/メトリクス/トレースは**該当なし**。
- 推奨:
  - ログ: 解析開始/終了、重大分岐（translation_unit、function_definition）、`check_recursion_depth`の拒否イベントを`debug`/`warn`で出力。
  - メトリクス: ノード訪問数、生成シンボル数、重複検出件数、解析時間。
  - トレース: ノード種別ごとの処理時間分布（feature gateで有効化）。

## Risks & Unknowns

- `check_recursion_depth`の閾値・挙動はこのチャンクでは不明（スタックオーバーフロー防止の想定）。
- `ParserContext`, `ScopeType`, `Symbol`, `SymbolKind`, `Visibility`の詳細仕様は不明（このチャンクには現れない）。
- `NodeTrackingState`は**処理済みノードの記録**以外の利用（重複防止など）がないため、活用方針は不明。
- `tree_sitter::Parser`のSend/Syncはバージョン/実装依存。現状APIは&mut selfで同時使用不可だが、**マルチスレッドでの安全性保証は不明**。
- C grammarのフィールド名（`declarator`など）と実装の一致性は未検証。`find_defines_in_node`の精度は**低い可能性**が高い。