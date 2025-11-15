# parsing\parser.rs Review

## TL;DR

- 目的: 各言語向けのパーサーの共通インターフェースを定義し、インデクサーが統一的にシンボル抽出・呼び出し解析・型使用などを実行できるようにする。補助としてASTノード追跡とUTF-8安全な文字列操作、再帰深度ガードを提供。
- 主要公開API: trait **LanguageParser**（parse/find_*群、as_any、language）、trait **ParserFactory**（create）、trait **NodeTracker** と **NodeTrackingState**、関数 **safe_truncate_str** / **safe_substring_window** / **check_recursion_depth** / **truncate_for_display**、定数 **MAX_AST_DEPTH**。
- 複雑箇所: ゼロコピー設計（&strスライスを返す find_* 群）と所有/lifetime管理、find_method_calls の後方互換変換、AST再帰深度制御。
- 重大リスク: safe_substring_window が「end_byte」を文字境界として検証していないため、end_byte が文字境界でない場合にスライスがパニックする可能性。find_* に &mut self を要求しているため、読み取り中心の処理での並行利用性が低下。
- セキュリティ/安全性: unsafe未使用、メモリ安全性は概ね良好。ログは eprintln でのデバッグのみ（情報漏えいリスクは低いが標準出力汚染あり）。
- 推奨改善: end側のUTF-8境界チェックの追加、find_* 群を &self に緩和、統一的な文字境界ユーティリティの導入、構造化ログ（tracing）への移行とメトリクス追加。

## Overview & Purpose

このファイルは、インデクサーに接続されるすべての言語パーサーが実装すべき共通トレイト **LanguageParser** を定義します。これにより、ソースコードからのシンボル抽出（関数、型、実装、使用、インポートなど）と、コード内の呼び出し関係・継承・型利用などの横断的解析が言語に依存しない形で行えます。  
補助として、ツリーシッターASTノードのハンドリング状況を追跡する **NodeTracker** / **NodeTrackingState**、UTF-8安全なスライスを行う **safe_truncate_str** / **safe_substring_window**、再帰深度ガード **check_recursion_depth**、表示向けの短縮 **truncate_for_display**、および再帰深度の上限 **MAX_AST_DEPTH** を提供します。

このチャンクでは具体的な各言語固有の実装は存在せず、インターフェースとユーティリティのみが示されています（各 find_* の詳細実装は「不明／このチャンクには現れない」）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Trait | LanguageParser | pub | パーサー共通IF（parse、find_*、language 等） | Med |
| Trait | ParserFactory | pub | パーサーインスタンスの生成 | Low |
| Struct | HandledNode | pub | 取り扱ったASTノード種別の記録（名前・ID） | Low |
| Trait | NodeTracker | pub | 取り扱いノード集合の公開・登録 | Low |
| Struct | NodeTrackingState | pub | NodeTrackerの既定実装（HashSetで追跡） | Low |
| Const | MAX_AST_DEPTH | pub | AST再帰の最大深度（500） | Low |
| Fn | safe_truncate_str | pub | UTF-8境界を守った前方トランケーション（ゼロコピー） | Low |
| Fn | safe_substring_window | pub | UTF-8境界を守ったウィンドウ抽出（ゼロコピー） | Med |
| Fn | truncate_for_display | pub | 表示用に省略記号付き短縮（ヒープ確保あり） | Low |
| Fn | check_recursion_depth | pub | AST再帰深度の安全チェックと警告ログ | Low |

### Dependencies & Interactions

- 内部依存
  - LanguageParser::find_method_calls は **find_calls** の戻り値を **MethodCall::from_legacy_format** で変換（後方互換）します（関数名:行番号不明）。
  - NodeTrackingState は **HashSet<HandledNode>** を用いて重複なくノード種別を追跡（register_handled_node）（関数名:行番号不明）。
  - check_recursion_depth は **crate::config::is_global_debug_enabled()** に依存して警告ログ出力制御（関数名:行番号不明）。

- 外部依存（このチャンクで参照のみ）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | tree_sitter::Node | ASTノード表現 | 行・列位置、深度チェックで使用 |
  | std::any::Any | ダウンキャスト支援（as_any） | ランタイム型判定 |
  | std::collections::HashSet | ノード追跡集合 | 重複排除 |
  | crate::types::SymbolCounter | シンボル採番 | 実装詳細は不明 |
  | crate::{FileId, Range, Symbol} | パーサーI/Oの型 | 実装詳細は不明 |
  | crate::parsing::method_call::MethodCall | 呼び出し表現 | from_legacy_format を使用 |
  | crate::parsing::{Import, Language} | インポート・言語識別 | 実装詳細は不明 |
  | crate::config | デバッグフラグ | ログ制御 |

- 被依存推定
  - 各言語別パーサー実装（Rust/TS/Pythonなど）が **LanguageParser** を実装。
  - インデクサー/解析器が **ParserFactory** 経由でパーサーを生成し、parse/find_* を呼び出し。
  - レポート/監査機能が **NodeTracker** の集合を参照して「対応済みノード種別」を動的に可視化。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| LanguageParser::parse | fn parse(&mut self, code: &str, file_id: FileId, counter: &mut SymbolCounter) -> Vec<Symbol> | コードからシンボル抽出 | 実装依存（典型O(n)） | 実装依存 |
| LanguageParser::as_any | fn as_any(&self) -> &dyn Any | 具体型へのダウンキャスト支援 | O(1) | O(1) |
| LanguageParser::extract_doc_comment | fn extract_doc_comment(&self, node: &Node, code: &str) -> Option<String> | ドキュメンテーションコメント抽出 | 実装依存 | 実装依存（String生成） |
| LanguageParser::find_calls | fn find_calls<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 関数/メソッド呼び出し検出（ゼロコピー） | 実装依存 | O(k) 返却ベクトル |
| LanguageParser::find_method_calls | fn find_method_calls(&mut self, code: &str) -> Vec<MethodCall> | リッチな受け手情報付き呼び出し | 実装: O(m) 変換 | O(m) |
| LanguageParser::find_implementations | fn find_implementations<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 型のトレイト/インタフェース実装検出 | 実装依存 | O(k) |
| LanguageParser::find_extends | fn find_extends<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 継承関係検出（デフォルト空） | O(1) | O(1) |
| LanguageParser::find_uses | fn find_uses<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 型使用（フィールド/引数/戻り）検出 | 実装依存 | O(k) |
| LanguageParser::find_defines | fn find_defines<'a>(&mut self, code: &'a str) -> Vec<(&'a str, &'a str, Range)> | メソッド定義検出 | 実装依存 | O(k) |
| LanguageParser::find_imports | fn find_imports(&mut self, code: &str, file_id: FileId) -> Vec<crate::parsing::Import> | インポート構文抽出 | 実装依存 | O(k) |
| LanguageParser::language | fn language(&self) -> crate::parsing::Language | このパーサーの言語種別 | O(1) | O(1) |
| LanguageParser::find_variable_types | fn find_variable_types<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> | 変数と型の抽出（ゼロコピー） | 既定O(1) | O(1) |
| LanguageParser::find_variable_types_with_substitution | fn find_variable_types_with_substitution(&mut self, _code: &str) -> Option<Vec<(String, String, Range)>> | ジェネリクス置換を伴う型抽出（所有文字列） | 既定O(1) | 既定O(1) |
| LanguageParser::find_inherent_methods | fn find_inherent_methods(&mut self, _code: &str) -> Vec<(String, String, Range)> | 型に直接定義されたメソッド抽出 | 既定O(1) | 既定O(1) |
| ParserFactory::create | fn create(&self) -> Result<Box<dyn LanguageParser>, String> | パーサー生成 | 実装依存 | Box割当 |
| NodeTracker::get_handled_nodes | fn get_handled_nodes(&self) -> &HashSet<HandledNode> | 取り扱いノード集合の参照 | O(1) | O(n) |
| NodeTracker::register_handled_node | fn register_handled_node(&mut self, node_kind: &str, node_id: u16) | ノード種別登録 | 平均O(1) | O(n) |
| NodeTrackingState::new | fn new() -> Self | 空の追跡状態を生成 | O(1) | O(1) |
| safe_truncate_str | fn safe_truncate_str(s: &str, max_bytes: usize) -> &str | UTF-8安全な末尾切り捨て（ゼロコピー） | O(1)（最大3-4ステップ） | O(1) |
| safe_substring_window | fn safe_substring_window(code: &str, end_byte: usize, window_size: usize) -> &str | UTF-8安全な窓スライス（ゼロコピー） | O(1)（最大3-4ステップ） | O(1) |
| truncate_for_display | fn truncate_for_display(s: &str, max_bytes: usize) -> String | 表示向けの省略付き短縮 | O(1) + alloc | O(len) |
| check_recursion_depth | fn check_recursion_depth(depth: usize, node: Node) -> bool | AST再帰深度上限チェック | O(1) | O(1) |
| MAX_AST_DEPTH | pub const MAX_AST_DEPTH: usize | 再帰深度の上限（500） | - | - |

以下、主要APIの詳細説明（このチャンクで実装があるものはアルゴリズム記述、抽象メソッドは意図と契約を記述します）。

### LanguageParser::parse

1) 目的と責務  
- コード文字列からツリーシッターなどを用いて **Symbol** を抽出し返す。インデクサー側の主入力。

2) アルゴリズム（抽象・このチャンクには実装なし）  
- 字句/構文解析 → AST走査 → シンボル発見 → **SymbolCounter** で採番 → Vec<Symbol> に収集して返却。

3) 引数
| 名称 | 型 | 説明 |
|------|----|------|
| code | &str | ソースコード |
| file_id | FileId | ファイル識別子 |
| symbol_counter | &mut SymbolCounter | シンボルの採番器 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Vec<Symbol> | 抽出されたシンボルのリスト |

5) 使用例
```rust
fn index_file(factory: &dyn ParserFactory, file_id: FileId, code: &str, counter: &mut SymbolCounter) -> Vec<Symbol> {
    let mut parser = factory.create().expect("parser");
    let symbols = parser.parse(code, file_id, counter);
    symbols
}
```

6) エッジケース
- 巨大ファイルや深いAST（→ check_recursion_depth の利用推奨）
- 非UTF-8（入力は &str 前提、UTF-8保証外なら事前検証が必要）
- 言語毎のコメント/ドキュメント抽出差異

### LanguageParser::find_calls

1) 目的  
- 呼び出し元・呼び出し先・範囲をゼロコピーで抽出（&str スライス）。

2) アルゴリズム（抽象・このチャンクには実装なし）  
- ASTから呼び出し構文を検出 → 名前・範囲を &str, Range で返却。

3) 引数・戻り値  
| 引数 | 型 | 説明 |
|------|----|------|
| code | &'a str | ソースコード。返却する &str はこのライフタイムに束縛される |

| 戻り値 | 説明 |
|--------|------|
| Vec<(&'a str, &'a str, Range)> | (caller, callee, 範囲) のリスト |

5) 使用例
```rust
let mut parser = factory.create().unwrap();
let calls = parser.find_calls(code);
for (caller, callee, range) in calls {
    println!("{} -> {} @ {:?}", caller, callee, range);
}
```

6) エッジケース
- 受け手不明の関数呼び出し（グローバル関数）
- マクロ/DSL的呼び出し（検出対象外の可能性）
- 文字列埋め込み/動的呼び出しは静的解析では検出できない

### LanguageParser::find_method_calls（既定実装あり）

1) 目的  
- **MethodCall**（リッチな受け手情報）へ変換して返す。後方互換性のため **find_calls** を内部利用。

2) アルゴリズム  
- find_calls(code) → 反復して **MethodCall::from_legacy_format(caller, target, range)** により変換 → Vec<MethodCall>（関数名:行番号不明）

3) 引数/戻り値  
| 引数 | 型 | 説明 |
|------|----|------|
| code | &str | ソースコード |

| 戻り値 | 説明 |
|--------|------|
| Vec<MethodCall> | 構造化された呼び出し情報 |

5) 使用例
```rust
let mut parser = factory.create().unwrap();
let mcalls = parser.find_method_calls(code);
```

6) エッジケース
- find_calls が未実装/空の場合は空結果
- 受け手型の不明確さは from_legacy_format では表現に限界があるため、言語側で上書き実装推奨

（他の LanguageParser の抽象メソッド: extract_doc_comment, find_implementations, find_extends, find_uses, find_defines, find_imports, language, find_variable_types, find_variable_types_with_substitution, find_inherent_methods は「このチャンクには実装がない」。目的はコメント抽出、関係検出、インポート抽出、言語識別、型推論など。）

### ParserFactory::create

1) 目的  
- 新規パーサーインスタンス生成。失敗時は String エラー。

2) 使用例
```rust
let parser: Box<dyn LanguageParser> = factory.create()?;
```

3) エッジケース
- 言語リソース（grammar）不在による失敗
- 依存ライブラリ初期化失敗

### NodeTracker / NodeTrackingState

1) 目的  
- 処理済みASTノード種別の追跡（監査/可視化用）。

2) アルゴリズム（register_handled_node）  
- HandledNode{name: node_kind.to_string(), id: node_id} を生成 → HashSet に insert（関数名:行番号不明）

3) 引数/戻り値（register_handled_node）
| 引数 | 型 | 説明 |
|------|----|------|
| node_kind | &str | ノード種別名 |
| node_id | u16 | ツリーシッターID |

| 戻り値 | 説明 |
|--------|------|
| なし | HashSetへの登録（重複は自動排除） |

5) 使用例
```rust
let mut track = NodeTrackingState::new();
track.register_handled_node(node.kind(), node.kind_id());
let handled = track.get_handled_nodes();
```

6) エッジケース
- ノード種別が極端に多い場合のメモリ増加（HashSet）

### safe_truncate_str

1) 目的  
- UTF-8文字境界を守りつつ、指定バイト数以下に末尾切り捨て（ゼロコピー）。

2) アルゴリズム  
- s.len() <= max_bytes なら s を返す  
- 境界 = max_bytes から後方へ、is_char_boundary を満たす位置まで最大3-4バイト単位でデクリメント  
- &s[..boundary] を返す（関数名:行番号不明）

3) 引数/戻り値
| 引数 | 型 | 説明 |
|------|----|------|
| s | &str | 対象文字列 |
| max_bytes | usize | 上限バイト数 |

| 戻り値 | 説明 |
|--------|------|
| &str | ゼロコピーで安全なスライス |

5) 使用例
```rust
let s = "Status: 🔍 Active";
assert_eq!(safe_truncate_str(s, 10), "Status: ");
```

6) エッジケース
- max_bytes == 0 → "" を返す
- 多バイト文字断片にかかる → 直前境界まで後退
- s が短い → s そのもの

### safe_substring_window

1) 目的  
- end_byte の直前 window_size バイトぶんの安全な &str スライスを抽出（ゼロコピー）。

2) アルゴリズム  
- end = min(end_byte, code.len())  
- start_raw = end.saturating_sub(window_size)  
- start_raw が境界でなければ、前方（start_raw..=start_raw+3, endまで）で最初の境界を探す。見つからなければ start=end（空文字列）  
- &code[start..end] を返す（関数名:行番号不明）

3) 引数/戻り値
| 引数 | 型 | 説明 |
|------|----|------|
| code | &str | コード |
| end_byte | usize | 終端バイト位置（排他的） |
| window_size | usize | 最大ウィンドウサイズ |

| 戻り値 | 説明 |
|--------|------|
| &str | 安全なウィンドウスライス（ゼロコピー） |

5) 使用例
```rust
let code = "export class 🔍 Scanner";
let window = safe_substring_window(code, 20, 10);
```

6) エッジケース
- end_byte > code.len() → 末尾に丸め
- start_raw が文字中 → 前方の次境界へ（最大3バイト）
- 重要: 現在の実装は end が文字境界である保証をしないため、end が境界でない入力で &code[start..end] がパニックする可能性あり（詳細は「Edge Cases, Bugs, and Security」を参照）

### truncate_for_display

1) 目的  
- safe_truncate_str を用いて、必要なら "..." を付与した表示用短縮文字列。

2) アルゴリズム  
- truncated = safe_truncate_str(s, max_bytes)  
- truncated.len() < s.len() なら format!("{truncated}...") を返し、そうでなければ to_string（関数名:行番号不明）

3) 例
```rust
assert_eq!(truncate_for_display("This is a very long string", 10), "This is a ...");
```

### check_recursion_depth

1) 目的  
- 再帰的AST走査でのスタックオーバーフロー予防。

2) アルゴリズム  
- depth > MAX_AST_DEPTH なら、debug有効時 eprintln で警告出力し false を返す。それ以外は true（関数名:行番号不明）

3) 使用例（ドキュメント内の擬似コード）
```rust
if !check_recursion_depth(depth, node) { return; }
```

## Walkthrough & Data Flow

- ParserFactory::create → Box<dyn LanguageParser> を取得。
- LanguageParser::parse → コード全体から **Symbol** 群を抽出しインデクサーへ返却。再帰的処理を行う場合は **check_recursion_depth** を各ノードの処理入り口で使用。
- find_calls → &str スライスで呼び出し関係を抽出。必要なら find_method_calls が **MethodCall** へ変換（後方互換）。
- find_implementations / find_extends / find_uses / find_defines / find_imports → それぞれ関係抽出。戻り値はゼロコピー（インポートは構造体）。
- NodeTrackingState（NodeTracker） → パーサー内部で「明示的に処理したノード種別」を登録し、後で監査・レポートに利用可能。
- truncate_for_display / safe_truncate_str / safe_substring_window → サマリ表示・シグネチャ抜粋などで文字列を安全に扱う。

データのライフタイムは、find_* 群の戻り値 &str が常に入力 code のライフタイムに束縛される点が重要です（借用が有効な間のみ使用可能）。

## Complexity & Performance

- LanguageParser の抽出系（parse, find_*）は各言語の構文解析とAST走査に依存し、概ねコード長 n に対して O(n) 時間・O(k) 空間（抽出結果サイズ）となることが多い（実装依存）。
- NodeTrackingState の登録は HashSet の平均 O(1)。
- safe_truncate_str / safe_substring_window は最大4バイト境界調整のため、O(1) の定数時間。
- check_recursion_depth は O(1)。
- ボトルネックになりやすい箇所は言語固有の AST 構築と走査。I/Oやネットワークはこのファイルには登場しない。
- スケール限界は「抽出結果のベクトルサイズ」と「AST深度」。MAX_AST_DEPTH=500 が安全側。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| safe_substring_window の end が非境界 | code="aé", end_byte=1（"é"の先頭バイトの前） | パニックせず安全にスライス | end 側の境界チェックなし | 要修正 |
| safe_truncate_str: 0バイト | s="abc", max_bytes=0 | "" を返す | 後方境界探索で 0 に到達 | OK |
| safe_truncate_str: 絵文字断片 | s="🔍x", max_bytes=2〜3 | 直前境界に合わせて "" を返すか "🔍"を含まない | 後方探索あり | OK |
| safe_substring_window: 窓サイズ > 文字列長 | code="abc", end=3, window=100 | "abc" を返す | saturating_sub で安全 | OK |
| check_recursion_depth: 深度超過 | depth=501 | false を返し警告ログ | is_global_debug_enabled で制御 | OK |
| NodeTrackingState: 重複登録 | 同一 name/id | 一度だけ保持 | HashSet::insert | OK |
| find_calls のライフタイム | code の寿命終了後に &str 利用 | 不使用が正しい（借用違反） | ライフタイムで防止 | OK |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow: &str と安全なスライスを使用。unsafe 不使用。safe_substring_window の end 境界未検証が唯一のパニックリスク。
  - Use-after-free: 所有権・借用で防止。ゼロコピー &str は code ライフタイムに束縛。
  - Integer overflow: saturating_sub を使用し安全。その他の整数演算は境界チェック済み。
- インジェクション（SQL/Command/Path）
  - 該当なし。このファイルはパース/文字列処理のみ。
- 認証・認可
  - 該当なし。
- 秘密情報
  - ログ出力は eprintln のみ。デバッグ時のみ深度超過警告を出す（機密情報の混入は設計次第だがこのチャンクでは不明）。
- 並行性
  - LanguageParser/ParserFactory は **Send + Sync** 制約。だが多くのメソッドが &mut self を要求しており、同時並行呼び出しは困難。読み取り中心の API は &self に緩和可能。
  - 共有状態はこのチャンクでは Mutex/RwLock 不使用。データ競合は実装側次第（不明）。

Rust特有の観点:
- 所有権: find_* は &str を返し、入力 code の所有者は呼び出し側。パーサーは借用のみ。
- 借用: find_* 群のライフタイム 'a は code に束縛。借用期間を超える保持は禁止。
- ライフタイム: 明示的パラメータ（'a）を find_* に付与。十分。
- unsafe境界: unsafe 不使用。
- 並行性/非同期: Send + Sync 制約あり。async/await は登場しない。
- エラー設計: ParserFactory::create が Result を返す。その他は空ベクトル/Option(None) をフォールバックに使う設計。

重要主張の根拠（関数名:行番号不明—このチャンクには行番号情報がないため関数名のみ記載）:
- safe_substring_window が end 境界未検証であること: 関数内で start 側のみ is_char_boundary を確認し、end は clamp のみ。
- find_method_calls が find_calls の結果を MethodCall::from_legacy_format で変換していること。

## Design & Architecture Suggestions

- find_* 群の受け取りを &mut self → &self に緩和
  - 読み取り中心のメソッド（find_calls, find_implementations, find_extends, find_uses, find_defines, find_imports, find_variable_types, find_inherent_methods）は、内部キャッシュや可変状態が不要なら **&self** にすることで並行性が向上。
- UTF-8境界ユーティリティの統合
  - end 側の境界検証を含む共通関数を作成し、safe_truncate_str / safe_substring_window のロジック重複・差異を解消。
- check_recursion_depth の通知改善
  - eprintln ではなく **tracing** クレートでレベル付きログ + カウンタメトリクス（例: recursion_depth_exceeded）を出す。
- NodeTrackingState の公開方法
  - 監査用途でのスナップショット/リセット API の追加（例: clear_handled_nodes）。
- API契約の明確化
  - find_* の戻り &str は code の境界にのみ基づくこと、返却範囲が文字境界であることを契約上明記。内部で UTF-8 境界調整を標準化。

## Testing Strategy (Unit/Integration) with Examples

既存テスト（このチャンクに含まれる）:
- safe_truncate_str の絵文字・多バイト境界テスト
- truncate_for_display の省略確認
- Issue #29 再現テスト

追加推奨テスト:
1) safe_substring_window の end 境界テスト（パニック回避を確認）
```rust
#[test]
fn test_safe_substring_window_end_boundary() {
    let s = "aé"; // 'é'は2バイト
    // end_byte=1 は 'a' の直後（境界）, 2 は 'é' の途中で非境界の可能性あり
    let window1 = safe_substring_window(s, 1, 10);
    assert_eq!(window1, "a");
    // ここで end_byte=2 を安全に扱えるよう関数が修正されているべき
    // 修正前は &s[start..2] がパニックする可能性あり
}
```

2) check_recursion_depth の境界テスト
```rust
#[test]
fn test_check_recursion_depth_limits() {
    use tree_sitter::Node; // ダミーノード生成は難しいためモック化が必要（このチャンクにはない）
    // depth = MAX_AST_DEPTH までは true
    assert!(check_recursion_depth(MAX_AST_DEPTH, unsafe { std::mem::zeroed() })); // モック化例
    // depth 超過では false
    assert!(!check_recursion_depth(MAX_AST_DEPTH + 1, unsafe { std::mem::zeroed() }));
}
```
*注: Node の安全なモックはこのチャンクには現れないため、実際のテストでは木構築またはラッパーヘルパーが必要。*

3) find_method_calls の後方互換変換テスト（ダミー実装）
```rust
struct DummyParser;
impl LanguageParser for DummyParser {
    fn parse(&mut self, _c: &str, _f: FileId, _sc: &mut SymbolCounter) -> Vec<Symbol> { vec![] }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn extract_doc_comment(&self, _n: &tree_sitter::Node, _c: &str) -> Option<String> { None }
    fn find_calls<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> {
        vec![("A", "B", Range{start:0,end:1})] // Range の具体はこのチャンクでは不明
    }
    fn find_implementations<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> { vec![] }
    fn find_uses<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> { vec![] }
    fn find_defines<'a>(&mut self, _code: &'a str) -> Vec<(&'a str, &'a str, Range)> { vec![] }
    fn find_imports(&mut self, _code: &str, _file_id: FileId) -> Vec<crate::parsing::Import> { vec![] }
    fn language(&self) -> crate::parsing::Language { unimplemented!() }
}
#[test]
fn test_find_method_calls_legacy() {
    let mut p = DummyParser;
    let calls = p.find_method_calls("x");
    assert_eq!(calls.len(), 1);
}
```

4) プロパティテスト（UTF-8）
- ランダムなマルチバイト文字列に対して safe_truncate_str / safe_substring_window がパニックしないことを確認（quickcheck/proptest推奨）。

## Refactoring Plan & Best Practices

- safe_substring_window の end 境界検証追加
```rust
pub fn safe_substring_window(code: &str, end_byte: usize, window_size: usize) -> &str {
    let mut end = end_byte.min(code.len());
    // end側もUTF-8境界へ調整（後方へ最大3バイト）
    while end > 0 && !code.is_char_boundary(end) {
        end -= 1;
    }
    let start_raw = end.saturating_sub(window_size);
    let start = if start_raw > 0 && !code.is_char_boundary(start_raw) {
        (start_raw..=start_raw.saturating_add(3).min(end))
            .find(|&i| code.is_char_boundary(i))
            .unwrap_or(end)
    } else { start_raw };
    &code[start..end]
}
```
- find_* 群のシグネチャ見直し（可変参照不要なものは &self にする）
- 文字境界処理の共通化（helper: adjust_to_prev_boundary / adjust_to_next_boundary）
- ログの統一（tracing + feature flag）
- NodeTrackingState のAPI拡充（clear/get_snapshot）
- ドキュメント（extract_doc_comment）の言語別ポリシーを明確化（契約の明記）

## Observability (Logging, Metrics, Tracing)

- 現状: **check_recursion_depth** が debug 有効時に eprintln で警告。
- 改善提案:
  - tracing（info/warn）で行・列、ノード種別（可能なら）を構造化出力。
  - メトリクス: recursion_depth_exceeded カウンタ、最大観測深度ゲージ。
  - find_* の実行時間計測（ヒストグラム）と抽出件数記録。
  - NodeTracker の handled_nodes サイズを監視し、言語ごとの対応進捗を可視化。

## Risks & Unknowns

- Unknown（このチャンクには現れない）
  - Range, Symbol, SymbolCounter, Import, Language の内部構造と契約。
  - MethodCall::from_legacy_format の具体的な変換仕様。
  - インデクサー側の利用方法（同期/非同期、並行性要件）。
  - tree_sitter::Node のモック/生成方法（テスト容易性）。
- リスク
  - safe_substring_window の end 境界未検証によるパニック可能性（入力が非境界 end_byte のとき）。
  - &mut self 要求によりパーサーの同時利用が困難（設計上の並行性制限）。
  - eprintln による非構造化ログ（運用での収集・フィルタ困難）。