# parsing\gdscript\parser.rs Review

## TL;DR

- 目的: tree-sitter を用いた GDScript の軽量パーサで、クラス/関数/シグナル/変数などのシンボル抽出、呼び出し関係、継承関係、インポート（extends/preload/class_name）を収集。
- 主な公開API: new(), parse(), find_calls(), find_uses(), find_extends(), find_imports(), extract_doc_comment(), language(), get_handled_nodes()（LanguageParser/NodeTracker 実装を含む）。
- コアロジック: 再帰的AST走査。シンボル抽出は extract_symbols_from_node()、関係抽出は collect_calls()/collect_uses()/collect_extends()/find_imports_in_node() が担う。
- 複雑箇所: 呼び出しターゲットの特別扱い（emit_signal/preload）、スコープ管理（ParserContext）とシンボル種別決定、ドキュメンテーションコメント（##）の遡及抽出。
- 重大リスク/バグ候補:
  - 文字列スライスでの UTF-8 境界パニックの可能性（text_for_node(), extract_* が &code[byte_range] を直接使用）。
  - extends/import のクォート剥がしが不統一（collect_extends() は " のみ、find_imports_in_node() は未剥離）。
  - find_imports_in_node() が obj.preload("...") 形式を検出しない一方、collect_uses()/extract_preload_path() は検出する非対称性。
  - 再帰深度チェックがシンボル抽出パスのみ（collect_* 系には未適用）。
  - コメントノードが named でない場合、doc_comment_for() が機能しない可能性。
- パフォーマンス: 各 find_* は毎回 parse を実行し O(N)。大規模入力での重複コスト。AST キャッシュや単一パス化を検討。

## Overview & Purpose

本ファイルは、tree-sitter-gdscript を利用して GDScript のソースコードから以下を抽出するパーサ実装です。

- シンボル抽出: クラス/関数/メソッド/コンストラクタ/シグナル/変数/定数/class_name の Symbol 化と署名・ドキュメント（##）付与。
- 関係抽出:
  - 関数呼び出し（emit_signal の特別扱い）
  - 継承関係（extends）
  - 使用関係（preload によるリソースパス使用、extends）
  - インポート（extends, class_name, preload）
- 監査・追跡: NodeTrackingState による処理済みノード種別の記録。

LanguageParser トレイトに準拠した API を提供し、IDE 機能やコードインデクシングに必要なメタ情報を生成します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | GdscriptParser | pub | tree-sitter パーサ保持、AST 走査、抽出API群の実装 | Med |
| Const | SCRIPT_SCOPE | private | モジュール/スクリプトスコープ名 "<script>" の定数化 | Low |
| Method | new() -> Result<Self, String> | pub | パーサ初期化（言語設定） | Low |
| Trait Impl | LanguageParser for GdscriptParser | pub | parse, find_calls/uses/extends/imports などの公開 API | Med |
| Trait Impl | NodeTracker for GdscriptParser | pub | 監査用に処理ノード種別を記録/参照 | Low |
| Helper | extract_symbols_from_node(...) | private | 再帰走査でシンボル抽出（中心） | High |
| Helper | find_imports_in_node(...) | private | 再帰走査で extends/class_name/preload を収集 | Med |
| Helper | collect_calls(...) | private | 再帰走査で呼び出し関係を収集 | Med |
| Helper | collect_uses(...) | private | 再帰走査で使用関係を収集（extends/preload） | Med |
| Helper | collect_extends(...) | private | 継承関係専用の収集 | Low |
| Helper | handle_* 系 | private | 各構文（class/function/constructor/signal/var/const/class_name）ごとのシンボル生成 | Med |
| Helper | doc_comment_for(...) | private | ## 付きコメントの統合抽出 | Low |
| Helper | extract_signal_name(...) | private | emit_signal("...") からシグナル名抽出 | Low |
| Helper | extract_preload_path(...) | private | preload("...") からパス抽出 | Low |
| Helper | strip_string_quotes(...) | private | 文字列クォート除去 | Low |
| Helper | node_to_range(...) | private | tree-sitter Node → Range 変換 | Low |
| Helper | text_for_node(...) | private | Node のソーススライス取得 | Med |
| Helper | register_node(...) | private | 監査用 handled ノード登録 | Low |

### Dependencies & Interactions

- 内部依存（このチャンクに定義なし: 概要のみ）
  - Symbol, SymbolKind, Range, FileId: シンボルのデータモデルと位置表現。
  - SymbolCounter: シンボルIDの単調増分供給。
  - ParserContext, ScopeType: クラス/関数スコープ状態の追跡（current_class/current_function, enter/exit）。
  - NodeTrackingState, NodeTracker, HandledNode: 監査用の処理ノード種別の集合管理。
  - check_recursion_depth(...): 再帰防止（スタックオーバーフロー回避）。

- 外部依存（クレート/モジュール）

| 依存名 | 用途 | 備考 |
|--------|------|------|
| tree_sitter | Parser, Node, 走査API | AST 生成・走査の基盤 |
| tree_sitter_gdscript | 言語定義 | GDScript 文法のロードに使用 |

- 被依存推定
  - このパーサは crate::parsing のファクトリ/ディスパッチャから呼ばれ、IDE 機能（シンボル一覧、参照、ナビゲーション）やインデクサが利用。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | fn new() -> Result<Self, String> | パーサの初期化 | O(1) | O(1) |
| parse | fn parse(&mut self, code: &str, file_id: FileId, counter: &mut SymbolCounter) -> Vec<Symbol> | シンボル抽出（モジュール/クラス/関数/変数/定数/シグナル/class_name） | O(N) | O(S) |
| find_calls | fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 呼び出し関係抽出（caller, callee, Range） | O(N) | O(K) |
| find_uses | fn find_uses<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 使用関係抽出（extends/preload の owner→対象） | O(N) | O(K) |
| find_extends | fn find_extends<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 継承関係抽出 | O(N) | O(K) |
| find_imports | fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<Import> | extends/class_name/preload の import 候補収集 | O(N) | O(K) |
| extract_doc_comment | fn extract_doc_comment(&self, node: &Node, code: &str) -> Option<String> | ## ドキュメントコメント抽出 | O(L) | O(L) |
| language | fn language(&self) -> Language | 言語識別子取得 | O(1) | O(1) |
| get_handled_nodes | fn get_handled_nodes(&self) -> &HashSet<HandledNode> | 監査用に処理済みノードの集合を参照 | O(1) | O(H) |
| as_any | fn as_any(&self) -> &dyn Any | ダウンキャスト補助 | O(1) | O(1) |
| find_defines | fn find_defines<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 未実装（空） | O(1) | O(1) |
| find_implementations | fn find_implementations<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 未実装（空） | O(1) | O(1) |

注: N=ASTノード数、S=出力シンボル数、K=関係エッジ数、L=コメント行数、H=handledノード数。

以下、主要APIの詳細:

1) parse
- 目的と責務
  - tree-sitter で AST を構築し、モジュール (<script>) シンボルを生成後、extract_symbols_from_node(...) で再帰的にシンボルを収集（関数名: parse, 行番号: 不明）。
- アルゴリズム
  - Parser::parse(code) でツリー作成。
  - ルートの Range で Module Symbol を生成。
  - ParserContext を新規作成し extract_symbols_from_node(root, ...) を実行。
  - 結果の Vec<Symbol> を返す。
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析対象 GDScript |
  | file_id | FileId | ファイル識別子 |
  | counter | &mut SymbolCounter | 一意ID供給源 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<Symbol> | 抽出された全シンボル（先頭に <script> モジュール） |
- 使用例
  ```rust
  let mut p = GdscriptParser::new().unwrap();
  let mut counter = SymbolCounter::default();
  let symbols = p.parse("class_name Player\nfunc run():\n  pass\n", file_id, &mut counter);
  assert!(symbols.iter().any(|s| s.name == "Player"));
  ```
- エッジケース
  - 構文解析に失敗した場合は空ベクタを返す。
  - 深い再帰の場合は check_recursion_depth により早期中止（extract_symbols_from_node で適用）。

2) find_calls
- 目的と責務
  - 呼び出し元（関数名または <script>）と呼び出し先（関数名/シグナル名）のペアを Range 付きで返す（関数名: find_calls, 行番号: 不明）。
- アルゴリズム
  - AST を構築し collect_calls(root, ...) を再帰実行。
  - emit_signal は第1引数の文字列をシグナル名として採用。preload は呼び出しグラフから除外。
- 引数/戻り値
  | 引数 | 型 | 説明 |
  |------|----|------|
  | code | &'a str | 解析対象 |
  | 戻り値 | Vec<(&'a str, &'a str, Range)> | (caller, callee, 位置) |
- 使用例
  ```rust
  let mut p = GdscriptParser::new().unwrap();
  let calls = p.find_calls(r#"func run(): emit_signal("clicked")"#);
  assert_eq!(calls[0].0, "run");
  assert_eq!(calls[0].1, "clicked"); // emit_signal の第1引数
  ```
- エッジケース
  - obj.emit_signal("...") も対応（ends_with(".emit_signal")）。
  - preload/obj.preload は無視される仕様。

3) find_uses
- 目的と責務
  - 使用関係（owner→使用対象）を抽出。対象は extends の基底クラス、preload のパス（関数内/束縛）等（関数名: find_uses, 行番号: 不明）。
- アルゴリズム
  - collect_uses(...) で class/function スコープ追跡。
  - extends は derived→base、preload は owner（関数名 or 束縛名 or <script>）→パス。
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(&str, &str, Range)> | (source, used_target, 位置) |
- 使用例
  ```rust
  let mut p = GdscriptParser::new().unwrap();
  let uses = p.find_uses(r#"extends "res://Base.gd"\nvar a = preload("res://x.tscn")"#);
  // 例: ("<script>", "res://Base.gd"), ("a", "res://x.tscn")
  ```
- エッジケース
  - obj.preload(...) 形式にも対応（extract_preload_path が ends_with(".preload") を考慮）。

4) find_extends
- 目的と責務
  - 継承関係のみ抽出（関数名: find_extends, 行番号: 不明）。
- 仕様詳細
  - class 定義内の extends フィールドと extends_statement を対象。
  - 現実装は " のみ trim（collect_extends 内）。' 単引用には未対応。要修正。
- 使用例
  ```rust
  let mut p = GdscriptParser::new().unwrap();
  let rel = p.find_extends(r#"class X extends "res://Base.gd": pass"#);
  ```

5) find_imports
- 目的と責務
  - インポート的な参照（extends/class_name/preload）を Import として収集（関数名: find_imports, 行番号: 不明）。
- 仕様詳細
  - extends_statement: path をそのまま格納（クォート未剥離）。
  - class_definition 内 extends: 同上（クォート未剥離）。
  - class_name_statement: is_glob=true としてグローバル公開名を登録。
  - preload("..."): 最初の文字列引数からクォートを除去して登録。
  - obj.preload("...") は未対応（call の第0子が identifier の "preload" のみ対象）。要修正。
- 使用例
  ```rust
  let mut p = GdscriptParser::new().unwrap();
  let imps = p.find_imports(r#"extends "res://Base.gd"\nconst T = preload("res://x.tscn")"#, file_id);
  ```

6) extract_doc_comment
- 目的
  - ノード直前の連続した ## コメント行を結合して返す（関数名: doc_comment_for, 行番号: 不明）。
- 注意
  - prev_named_sibling() を用いるため、comment ノードが named でない文法の場合は取得不可の可能性あり。

7) language
- Language::Gdscript を返す。

8) get_handled_nodes
- 監査用（処理したノード種類の集合）を返す。

9) as_any, find_defines, find_implementations
- as_any: ダウンキャスト用。
- find_defines/find_implementations: 現在は空実装。

## Walkthrough & Data Flow

- parse のデータフロー
  1) tree-sitter で AST を生成
  2) <script> モジュールシンボル作成
  3) extract_symbols_from_node(root) を呼び出し、以下の分岐で処理
  4) シンボル毎に Symbol を作成し、ParserContext を使ってスコープ情報（クラス/関数）を付与
  5) 再帰的に body を走査

- extract_symbols_from_node の主要分岐と再帰

```mermaid
flowchart TD
  A[Start extract_symbols_from_node] --> B{check_recursion_depth OK?}
  B -- No --> Z[Return]
  B -- Yes --> C{node.kind()}
  C -->|class_definition| D[handle_class_definition; return]
  C -->|function_definition| E[handle_function_definition; return]
  C -->|constructor_definition| F[handle_constructor_definition; return]
  C -->|signal_statement| G[handle_signal_statement]
  C -->|variable_statement| H[handle_variable_statement]
  C -->|const_statement| I[handle_const_statement]
  C -->|class_name_statement| J[handle_class_name_statement]
  C -->|enum/extends/match/for/if/while/tool/export/annotation(s)| K[register_node]
  C -->|other| L[no-op]
  G --> M[recurse children]
  H --> M
  I --> M
  J --> M
  K --> M
  L --> M
  M --> N[for each child: extract_symbols_from_node(..., depth+1)]
```

上記の図は extract_symbols_from_node 関数の主要分岐を示す（行番号: 不明）。

- find_imports_in_node の分岐

```mermaid
flowchart TD
  A[Start find_imports_in_node] --> B{node.kind()}
  B -->|extends_statement| C[args[0] を path として Import 追加]
  B -->|class_definition| D{extends あり?}
  D -->|Yes| E[extends の args[0] を Import 追加]
  D -->|No| H[recurse children]
  B -->|class_name_statement| F[name を is_glob=true で Import 追加]
  B -->|call| G{callee==identifier "preload"?}
  G -->|Yes| I[第1引数の文字列からパス抽出し Import 追加]
  G -->|No| H[recurse children]
  C --> H[recurse children]
  E --> H
  F --> H
  I --> H
  H --> Z[for each child: 再帰]
```

上記の図は find_imports_in_node 関数の主要分岐を示す（行番号: 不明）。

- 関係抽出
  - collect_calls は関数/コンストラクタで current_function を設定し、call ノードで caller=<関数名 or "<script>"> と target=<callee or 文字列シグナル名> を収集。
  - collect_uses は current_class/current_function を追跡し、extends と preload の関係を収集。

## Complexity & Performance

- 解析（tree-sitter）: ほぼ O(N) 時間、O(N) 空間（AST 構築）。
- parse: O(N) 時間、O(S) 空間（出力シンボル + 一時ベクタ）。シンボル抽出の再帰はノード数に線形。
- find_calls/find_uses/find_extends/find_imports: 各 O(N) 時間、O(K) 空間。現状は毎回 parse() 相当の AST 構築を行うため、連続呼び出し時に重複コスト。
- ボトルネック/スケール限界
  - 大規模ファイルでの repeated parse が高コスト。AST のキャッシュや1回の走査で複数の関係を同時に収集する設計が有利。
  - 再帰により非常に深いネストでスタック使用が増大。extract_symbols_from_node は check_recursion_depth を使うが、collect_* 系は未対策。
- 実運用負荷要因
  - I/O/ネットワーク/DB は非関与。CPU バウンド（構文解析と走査）。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| UTF-8 境界でのスライス | code が多バイト文字含む | パニックなく文字列取得 | text_for_node: &code[node.byte_range()] | 要確認/改善（パニック可能性） |
| extends のクォート両対応 | extends 'res://Base.gd' | パスから ' と " を剥離 | collect_extends: trim_matches('"') のみ | 要修正 |
| find_imports の preload 検出（メソッド形式） | obj.preload("x") | 検出される | find_imports_in_node は identifier "preload" のみ | 要修正 |
| doc コメント抽出（##）で named でない comment | ## が named でない | 抽出可 | prev_named_sibling 使用 | 文法依存/要確認 |
| 深いネストでの再帰防止 | 深い if/for ネスト | 無限/過深再帰を防止 | extract_symbols_from_node でのみ check_recursion_depth | 要修正（collect_* に未適用） |
| emit_signal の名前抽出失敗時 | emit_signal(x) | ターゲット名として "emit_signal" などのフォールバック | collect_call_targets は名前をプッシュ | OK |
| preload 非文字列引数 | preload(1+2) | 無視/スキップ | extract_preload_path は第1引数文字列のみ | OK |
| class_name の Import 化 | class_name Foo | グローバルに見えるシンボルとして扱う | find_imports_in_node は is_glob=true | OK |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow/use-after-free: Rust の安全境界内。unsafe 不使用（unsafe ブロック: なし）。
  - UTF-8 インデックス: &str のバイトスライスは UTF-8 境界でないとパニック。text_for_node(...)（関数名: 行番号不明）で潜在的に発生し得る。回避策提案あり（下記）。
  - 整数オーバーフロー: Range の列は u16/u32 で通常安全。
- インジェクション
  - SQL/Command/Path traversal: 解析/抽出のみで実行なし。直接的なインジェクションリスクなし。
- 認証・認可
  - 対象外。
- 秘密情報
  - ハードコーディングなし。ログ出力もなし。
- 並行性
  - GdscriptParser は内部に tree_sitter::Parser を持ち、メソッドは &mut self。並行実行なし。Send/Sync についての明示境界はないが現状問題なし。
- レース/デッドロック
  - 該当なし。

Rust特有の観点（詳細）
- 所有権/借用/ライフタイム: Node/Tree は関数内スコープで使用し、Node の参照を保持しないため安全。返却する &str は code のスライス（ライフタイム 'a）で妥当。
- unsafe 境界: なし。
- 非同期: 未使用。
- エラー設計: parse 失敗時は空 Vec、new は Err(String)。panic は起こさない設計が望まれるが UTF-8 スライスで潜在的パニックあり。

## Design & Architecture Suggestions

- 文字列スライスの安全化
  - text_for_node を bytes ベースに変更し、from_utf8 で検証:
    ```rust
    fn text_for_node<'a>(&self, code: &'a str, node: Node) -> &'a str {
        let range = node.byte_range();
        std::str::from_utf8(&code.as_bytes()[range])
            .unwrap_or("") // 望ましくは Result を返す設計に
    }
    ```
  - あるいは get(range) と map_or("") でパニックを避ける。
- クォート剥離の一元化
  - extends/import/uses/extents の全箇所で strip_string_quotes を使用し、" と ' 両対応に統一。collect_extends, find_imports_in_node を修正。
- preload 検出の一貫性
  - find_imports_in_node でも obj.preload(...) を検出するよう、callee テキスト ends_with(".preload") を許可（extract_preload_path と同仕様）。
- 再帰深度の全走査適用
  - collect_calls, collect_uses, collect_extends, find_imports_in_node にも check_recursion_depth を導入。もしくは非再帰（スタック/キュー）走査へ。
- 走査の単一パス化/ASTキャッシュ
  - 同じ code に対して複数の find_* を呼ぶ場合、毎回 parse している。parse を一度だけ実行し、必要なら関係抽出をオプションで同一走査内にまとめて取得（Visitor パターン）。
  - tree-sitter のインクリメンタルパース（前回ツリーを第二引数に渡す）を活用できる設計検討。
- NodeTracker の適用範囲拡大
  - find_imports_in_node などにも register_node を入れ、監査の網羅性を高める。
- API の明確化
  - find_imports の "is_glob" などフィールド意味をドキュメント化。emit_signal や preload を呼び出しグラフに含める/含めない方針を README/Doc に明記。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト例（簡略化）

1) シンボル抽出
```rust
#[test]
fn parse_extracts_symbols() {
    let code = r#"
        ## Player actor
        class_name Player
        extends "res://Actor.gd"

        signal clicked(x)

        var speed := 10

        func _init(name: String) -> void:
            pass

        func run():
            emit_signal("clicked", 1)
    "#;
    let mut p = GdscriptParser::new().unwrap();
    let mut counter = SymbolCounter::default();
    let syms = p.parse(code, FileId(1), &mut counter);
    assert!(syms.iter().any(|s| s.name == "<script>" && matches!(s.kind, SymbolKind::Module)));
    assert!(syms.iter().any(|s| s.name == "Player" && matches!(s.kind, SymbolKind::Class)));
    assert!(syms.iter().any(|s| s.name == "_init" && matches!(s.kind, SymbolKind::Method)));
    assert!(syms.iter().any(|s| s.name == "clicked" && matches!(s.kind, SymbolKind::Field)));
}
```

2) 呼び出し関係（emit_signal の特別扱い）
```rust
#[test]
fn calls_extracts_emit_signal_name() {
    let code = r#"func run(): emit_signal("clicked")"#;
    let mut p = GdscriptParser::new().unwrap();
    let calls = p.find_calls(code);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "run");
    assert_eq!(calls[0].1, "clicked");
}
```

3) 使用関係（preload/extends）
```rust
#[test]
fn uses_extracts_preload_and_extends() {
    let code = r#"
        extends "res://Base.gd"
        const T = preload("res://x.tscn")
        func f():
            var l = preload('res://y.tscn')
    "#;
    let mut p = GdscriptParser::new().unwrap();
    let uses = p.find_uses(code);
    assert!(uses.iter().any(|(src, tgt, _)| *src == "<script>" && *tgt == "res://Base.gd"));
    assert!(uses.iter().any(|(src, tgt, _)| *src == "T" && *tgt == "res://x.tscn"));
    assert!(uses.iter().any(|(src, tgt, _)| *src == "f" && *tgt == "res://y.tscn"));
}
```

4) インポート（class_name/preload/extends の一貫性）
```rust
#[test]
fn imports_detects_class_name_and_preload_and_extends() {
    let code = r#"
        class_name Foo
        extends 'res://Base.gd'
        const T = preload("res://x.tscn")
        $Node.preload("res://y.tscn") # 将来的にメソッド形式も対応
    "#;
    let mut p = GdscriptParser::new().unwrap();
    let imps = p.find_imports(code, FileId(1));
    assert!(imps.iter().any(|i| i.path == "Foo" && i.is_glob));
    // extends のクォート剥離が修正後にパス==res://Base.gd になることを期待
}
```

5) ドキュメントコメント（## 連結順序/中断条件）
```rust
#[test]
fn doc_comments_are_collected_above_symbol() {
    // コメントが named node であることが前提のテスト（文法依存）
}
```

6) 安全性（UTF-8 境界）プロパティテスト
- 多バイト文字を含むコードで panic が起きないことを確認。

7) 深いネストの耐性
- 入れ子の if/while を深く生成して、collect_* 系にも深さ制限が必要であることを検証（現状は潜在的に失敗）。

## Refactoring Plan & Best Practices

- Utility の集中
  - text_for_node を安全版に一元化。strip_string_quotes を全利用箇所へ適用。
- 仕様の一貫性
  - preload の検出仕様を find_imports_in_node と extract_preload_path で統一。
  - extends のクォート処理を strip_string_quotes に統一。
- 再帰の安全化
  - check_recursion_depth を collect_calls/uses/extends/find_imports 系にも導入。または非再帰DFS実装へ。
- 単一パスの抽出
  - 1回の AST 走査でシンボルと関係（calls/uses/extends/imports）を同時抽出できる Visitor を導入。構成例:
    - Visitor { on_symbol, on_call, on_use, on_import } のコールバック集
- パフォーマンス改善
  - 複数 API 呼び出し間で AST を共有（parse tree を返す/保持する）。tree-sitter のインクリメンタルパースを活用。
- 観測性（後述）と監査性
  - NodeTracker の登録を全走査パスに拡張。未対応ノードの収集/レポート。

## Observability (Logging, Metrics, Tracing)

- Logging
  - フィーチャーフラグ（例: feature = "trace"）で、未対応ノード種類、過深再帰の検出、doc コメント抽出失敗などを warn! 出力。
- Metrics
  - 走査ノード数、抽出シンボル数、関係エッジ数、処理時間（パース時間/走査時間）を計測。
- Tracing
  - tracing クレートで parse / extract / collect_* に instrument 属性を付与しプロファイル可能に。
- 監査
  - get_handled_nodes に基づき、未処理ノードの一覧をテスト/CIで可視化。

## Risks & Unknowns

- tree-sitter-gdscript の文法仕様に依存
  - comment ノードが named か否か（doc_comment_for の成否に影響）: 不明。
  - call ノードの第0子の kind/表現（identifier か member_expression か）: 不明。
- check_recursion_depth の閾値/動作は外部に依存: 不明。
- FileId, SymbolCounter, ParserContext, NodeTrackingState の詳細実装はこのチャンクには現れない。
- Range の line/column が 0-based/1-based の規約: このチャンクからは不明（tree-sitter は 0-based で返すため現状 0-based と推定）。

以上により、本実装はシンプルかつ拡張しやすい構成だが、クォート処理の一貫性、UTF-8 安全性、走査の重複（パフォーマンス）と深さ制御（安全性）に改善余地があります。