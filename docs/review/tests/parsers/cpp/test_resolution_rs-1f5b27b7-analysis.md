# parsers\cpp\test_resolution.rs Review

## TL;DR

- 目的: 実際のC++コードを用いて、CppParserで抽出したシンボルをCppBehaviorの解決コンテキストに投入し、名前解決が期待通りに機能するかを統合テストする。
- 公開API: 本ファイル自身の公開APIは**なし**。外部APIとしてcodannaのCppParser、CppBehavior、ResolutionContext（resolve/add_symbol）を使用。
- 複雑箇所: 大規模C++入力のパース後に複数種類のシンボル（Class/Struct/Function/Method/Namespaceなど）を一括で解決検証するフロー。
- 重大リスク: テストが外部ファイル examples/cpp/comprehensive.cpp に依存し、存在/内容により不安定。expect/unwrapによるパニック発生可能性。解決の衝突（オーバーロード/重複名）時の期待値が曖昧。
- Rust安全性: unsafeなし。所有権/借用は安全だが、to_string/unwrap/expect利用により不要なコピーとパニック可能性。並行性の利用は**なし**。
- パフォーマンス: 入力サイズに対してパースは概ねO(n)。解決は実装依存（多くはHashMapならO(1)期待）だが、このチャンクでは**不明**。
- セキュリティ: ファイルI/O失敗時のハンドリングがpanic依存。ログにシンボル名を出力するが秘密情報は扱っていない。

## Overview & Purpose

このファイルは、codannaクレートのC++パーサと解決コンテキストの統合テストを行うRustテストモジュール。主に以下を目的とする:

- 実例のC++コード（examples/cpp/comprehensive.cpp）を読み込み、CppParserでシンボル抽出を行う。
- CppBehaviorの解決コンテキストに抽出シンボルを登録し、クラス/関数/名前空間等の名前解決が期待通りかを確認する。
- 追加の基礎テストで、手動追加したシンボルの解決が基本機能として動作するかを検証する。

本チャンクに本体ロジックの実装は存在せず、外部APIへの呼び出しとテストロジックのみが含まれる。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_cpp_resolution_with_real_code | private (#[test]) | 実C++コードでの統合テスト: パース→コンテキスト登録→複数ケースの名前解決検証 | Med |
| Function | test_cpp_resolution_context_basic | private (#[test]) | 基本的な解決コンテキストの単体テスト: 手動シンボル登録→解決→未知シンボル非解決 | Low |

### Dependencies & Interactions

- 内部依存
  - 両テスト関数は独立しており、相互依存は**なし**。
  - 共通してcodanna::parsing::cpp::behavior::CppBehaviorを使用し解決コンテキストを生成。
  - 統合テストではCppParserとSymbolCounterを追加で使用。

- 外部依存（推奨: 表）
  
  | クレート/モジュール | 使用シンボル | 用途 |
  |---------------------|--------------|------|
  | std::fs | read_to_string | C++ファイルの読み込み |
  | std | println!, assert!, panic! | ログ出力・検証・失敗処理 |
  | codanna::parsing::cpp::parser | CppParser::new, parser.parse | C++コードのパースとシンボル抽出 |
  | codanna::parsing::LanguageBehavior | トレイト境界 | 言語毎の解決振る舞い |
  | codanna::parsing::cpp::behavior | CppBehavior::new, create_resolution_context | C++向け解決コンテキスト生成 |
  | codanna::parsing::resolution | ScopeLevel::{Module,Local} | シンボル登録時のスコープ指定 |
  | codanna::types | SymbolCounter::new | シンボル採番補助 |
  | codanna | FileId, SymbolKind, SymbolId | ファイル識別子、シンボル種別、シンボルID判定 |

- 被依存推定
  - 本ファイルはテスト専用であり、ライブラリやアプリから直接使用されることは**ない**。cargo test時に使用される。

## API Surface (Public/Exported) and Data Contracts

- このファイルの公開API: **該当なし**（テスト関数のみ、エクスポートされない）。
- 代わりに、テストが利用する外部APIの一覧を示す。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CppParser::new | fn new() -> Result<CppParser, E>（Eは不明） | C++パーサの生成 | O(1) | O(1) |
| CppParser::parse | fn parse(&mut self, code: &str, file: FileId, counter: &mut SymbolCounter) -> Vec<Symbol>（型詳細不明） | C++コードからシンボル抽出 | O(n)（n=入力長） | O(k)（k=抽出シンボル数） |
| CppBehavior::new | fn new() -> CppBehavior | C++言語向けの解決振る舞いインスタンス生成 | O(1) | O(1) |
| create_resolution_context | fn create_resolution_context(&self, file: FileId) -> ResolutionContext（型詳細不明） | 名前解決コンテキストの生成 | O(1) | O(1) |
| ResolutionContext::add_symbol | fn add_symbol(name: String, id: SymbolId, scope: ScopeLevel) | シンボルのコンテキスト登録 | O(1)期待（実装依存） | O(1) |
| ResolutionContext::resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | 名前からシンボルIDを取得 | O(1)期待（実装依存） | O(1) |
| SymbolCounter::new | fn new() -> SymbolCounter | シンボル採番ヘルパの生成 | O(1) | O(1) |

各APIの詳細説明（このチャンクから読み取れる範囲）:

1) CppParser::parse
- 目的と責務: C++コード文字列からシンボル（名前、種別、位置情報等）を抽出する。
- アルゴリズム: 不明（このチャンクには現れない）。抽出後はVec<Symbol>を返す。
- 引数:

  | 引数名 | 型 | 説明 |
  |--------|----|------|
  | code | &str | 入力のC++コード |
  | file | FileId | ファイル識別子 |
  | counter | &mut SymbolCounter | シンボルID採番のためのカウンタ |

- 戻り値:

  | 型 | 説明 |
  |----|------|
  | Vec<Symbol> | 抽出されたシンボルの配列（Symbolの定義はこのチャンクには現れない） |

- 使用例:

  ```rust
  let mut parser = CppParser::new().expect("Failed to create CppParser");
  let mut counter = SymbolCounter::new();
  let symbols = parser.parse(&cpp_code, FileId(1), &mut counter);
  ```

- エッジケース:
  - 空入力: 空Vecが返る可能性。詳細は不明。
  - 不正な構文: エラー処理の種類は不明（ResultではなくVec返却のため、内部で黙って捨てる可能性あり）。

2) ResolutionContext::{add_symbol, resolve}
- 目的と責務: シンボル名とスコープを登録し、名前からIDを解決する。
- アルゴリズム: 不明（このチャンクには現れない）。一般的にはHashMapベースが想定される。
- 引数（add_symbol）:

  | 引数名 | 型 | 説明 |
  |--------|----|------|
  | name | String | シンボル名 |
  | id | SymbolId | シンボルID |
  | scope | ScopeLevel | スコープ（Module/Local等） |

- 戻り値（resolve）:

  | 型 | 説明 |
  |----|------|
  | Option<SymbolId> | 見つかればSome(ID)、なければNone |

- 使用例:

  ```rust
  let mut ctx = behavior.create_resolution_context(FileId(1));
  ctx.add_symbol("TestClass".to_string(), SymbolId(200), ScopeLevel::Module);
  assert_eq!(ctx.resolve("TestClass"), Some(SymbolId(200)));
  ```

- エッジケース:
  - 重複名: 最後に登録したもので上書きされるか、多重登録を保持するかは不明。
  - スコープ競合: ModuleとLocalに同名がある場合の優先度は不明。

3) CppBehavior::new / create_resolution_context
- 目的と責務: C++特有の解決ルールを持つコンテキストの生成。
- 引数/戻り値: 詳細は不明。
- 使用例:

  ```rust
  let behavior = codanna::parsing::cpp::behavior::CppBehavior::new();
  let context = behavior.create_resolution_context(FileId(1));
  ```

- エッジケース: 不明。

「重要な主張には根拠（関数名:行番号）」について、このチャンクには正確な行番号メタデータが存在しないため、行番号は「不明」と記します。

## Walkthrough & Data Flow

全体フロー（test_cpp_resolution_with_real_code）:

```mermaid
flowchart TD
  A[読み込み: comprehensive.cpp] -->|std::fs::read_to_string| B[CppParser::new]
  B --> C[SymbolCounter::new]
  C --> D[parser.parse(&cpp_code, file_id, &mut counter)]
  D --> E[CppBehavior::new]
  E --> F[create_resolution_context(file_id)]
  D --> G[for each symbol]
  G --> H[context.add_symbol(name,id,ScopeLevel::Module)]
  F --> I[Test1: resolve("Logger")]
  F --> J[Test2: resolve("main")]
  F --> K[Test3: resolve("std")]
  F --> L[Test4: resolve("NonExistentSymbol123")]
  D --> M[Test5: 全シンボル巡回しresolve(name)]
  M --> N[統計/ログ出力/アサーション]
```

上記の図は`test_cpp_resolution_with_real_code`関数の主要フローを示す（行番号: 不明）。

データの流れ:
- 入力: C++ソース文字列 cpp_code（所有: String）。
- パース結果: symbols（Vec<Symbol>）。各Symbolには name（String想定）、kind（SymbolKind）、range.start_line（位置情報）など。
- 解決コンテキスト: context。add_symbolで name と id と scope を登録。resolveで name→Option<SymbolId> を返却。
- ロギング: println!でシンボル一覧と解決結果を表示。
- 検証: assert_eq!, assert!, panic! により成功/失敗を判定。

補助フロー（test_cpp_resolution_context_basic）:
- CppBehaviorからcontext生成。
- 手動で class_id, method_id を add_symbol。
- resolveで一致を検証。unknown_symbolでNoneを検証。

## Complexity & Performance

- 読み込み: read_to_string は入力サイズ n に対して O(n) 時間・O(n) メモリ。
- パース: CppParser::parse は一般に O(n)（n=コード長）だが正確なアルゴリズムは不明。
- 登録: add_symbol は内部構造次第だが HashMapなら O(1) 平均・O(1) 追加メモリ。
- 解決: resolve も HashMapなら O(1) 平均・O(1) 追加メモリ。実装は不明のため*期待値*。
- ボトルネック: 大規模C++コードのパース時間・メモリ。symbols全件の逐次resolve（Test5）は O(k) 呼び出し（k=シンボル数）。
- スケール限界: 巨大ファイルではパースのメモリ確保が支配的。登録・解決は名前数に比例。
- 実運用負荷要因: I/O（ファイル）、パーサの構文解析、シンボルの位置情報管理。

## Edge Cases, Bugs, and Security

セキュリティ・健全性チェックリスト:
- メモリ安全性: unsafe不使用。バッファオーバーフロー/Use-after-freeはRustの型安全により回避。整数オーバーフローの懸念は本チャンクでは**不明**。
- インジェクション: SQL/Command/Path traversal なし。ファイルパスは固定文字列のため*安全*。
- 認証・認可: 該当なし。
- 秘密情報: ハードコーディングされた秘密はなし。ログ漏えいの懸念は低い（シンボル名のみ出力）。
- 並行性: レース/デッドロックなし（同期コードのみ）。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ファイル不存在 | パス "examples/cpp/comprehensive.cpp" が存在しない | エラーを扱い、テストをスキップまたは失敗を明示 | read_to_string().expect(...) でpanic | 改善余地あり |
| 空ファイル/空文字列 | "" | symbolsが空、解決は0件、テストは適切に評価 | parse後の検証あり（total_count > 0 のassertで失敗） | 動作するが厳格 |
| シンボル重複（オーバーロード） | 同名の関数/メソッドが複数 | resolveの戻りIDが異なる可能性を許容 | "RESOLVED TO DIFFERENT ID" と警告表示 | 許容している |
| 期待シンボル不存在（Logger/main） | 例: comprehensive.cppに該当なし | 解決できないが、テスト自体は失敗しない | else分岐で警告、最終assertは総合判定 | 受容 |
| 名前空間std未抽出 | 標準ライブラリ非解析 | resolve("std")がNone | "may be expected" とログ出力 | 受容 |
| シンボル数0 | パース失敗/対象なし | 適切に失敗を通知 | assert!(total_count > 0) でテスト失敗 | OK |
| ローカル/モジュールスコープ競合 | 同名シンボルが異なるスコープに存在 | ルールに従い優先度で解決 | ルールは不明（このチャンクには現れない） | 不明 |

バグ候補:
- テストの外部ファイル依存により再現性が下がる。CI環境でファイルがないとpanic。
- to_stringで不要なヒープ割り当て（add_symbolのたびに文字列コピー）。
- unwrap/expectにより失敗時にpanicして原因の粒度が粗い。

## Design & Architecture Suggestions

- 外部ファイル依存の低減
  - examples/cpp/comprehensive.cpp 不在時はテストをスキップする仕組みに変更（環境変数やcfg(test)ガードで制御）。
  - 小さな自己完結のC++スニペットをテスト内に埋め込み、最低限の解決ケースを保証。

- エラーハンドリングの向上
  - read_to_string().expect(...) を Result ハンドリングに変更し、「スキップ」/「失敗」を明確に分離。
  - unwrapの代わりにパターンマッチで意味のあるエラーメッセージを返す。

- API呼び出し最適化
  - add_symbolにStringを渡すために毎回to_stringしている。もしAPIが&strを受け入れるオーバーロードを提供しているならそれを使用（提供がない場合は現状維持）。
  - 大量のsymbols登録前にフィルタリング（必要な種類のみ登録）で解決空間を縮小。

- テスト設計の分割
  - 統合テスト（大ファイル）とユニットテスト（小スニペット）を分離し、失敗箇所の特定性を高める。
  - 期待シンボル（Loggerやmain）が存在することを先にアサートしてから解決テストに進む。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト（既存のbasicに加えて）
  - スコープ優先度検証: Module と Local に同名を登録し、どちらが解決されるかを確認。
  - 重複名解決ポリシー検証: 同名を複数登録した場合の解決挙動の期待を定義。

- 統合テスト
  - 最小C++スニペット（自己完結）を用意し、ParserとResolutionの往復を安定検証。
  - 大規模ファイルは存在チェック後にoptionalに実行。

例1: スコープ優先度テスト（仮の期待動作。実装詳細は不明のため、検証用に両方許容/比較）

```rust
#[test]
fn test_scope_priority() {
    let behavior = codanna::parsing::cpp::behavior::CppBehavior::new();
    let file_id = FileId(1);
    let mut context = behavior.create_resolution_context(file_id);

    let module_id = codanna::SymbolId(1);
    let local_id = codanna::SymbolId(2);

    context.add_symbol("foo".to_string(), module_id, codanna::parsing::resolution::ScopeLevel::Module);
    context.add_symbol("foo".to_string(), local_id, codanna::parsing::resolution::ScopeLevel::Local);

    let resolved = context.resolve("foo");
    assert!(resolved.is_some());
    // 優先度が不明なため、両方の可能性を許容する例
    assert!(resolved.unwrap() == local_id || resolved.unwrap() == module_id);
}
```

例2: 小さな自己完結C++スニペットでのパース・解決（パーサ仕様が不明のため、擬似的な使用例）

```rust
#[test]
fn test_parse_and_resolve_small_snippet() {
    let cpp_code = r#"
        struct Logger { void log(); };
        int main() { return 0; }
    "#;

    let mut parser = CppParser::new().expect("Failed to create CppParser");
    let behavior = codanna::parsing::cpp::behavior::CppBehavior::new();
    let file_id = FileId(1);
    let mut counter = SymbolCounter::new();

    let symbols = parser.parse(cpp_code, file_id, &mut counter);
    let mut context = behavior.create_resolution_context(file_id);
    for s in &symbols {
        context.add_symbol(s.name.to_string(), s.id, codanna::parsing::resolution::ScopeLevel::Module);
    }

    assert!(context.resolve("Logger").is_some());
    assert!(context.resolve("main").is_some());
}
```

## Refactoring Plan & Best Practices

- ロガー整備
  - println!の代わりにテスト時のみ有効化されるログレベル制御（env_loggerやtracing）で整え、冗長出力を抑制。

- 失敗の分類
  - ファイル読み込み失敗は「スキップ」として扱う（例えば、環境変数 `RUN_CPP_INTEGRATION=1` がセットされている場合のみ実行）。

- 共有セットアップの抽出
  - CppParser/Behavior/Context 初期化と symbols 登録をヘルパ関数化し、テスト間重複を削減。

- 期待値の明確化
  - 具体的に「LoggerはClass/Struct」「mainはFunction/Method」といった期待を、シンボル存在チェックの直後にアサート化。

## Observability (Logging, Metrics, Tracing)

- 現状: println!で詳細ログを出力。テスト実行時に標準出力に大量の情報が出る可能性。
- 推奨:
  - *構造化ログ*: tracingまたはlogクレートでコンテキスト付きメッセージ。
  - *メトリクス*: 抽出シンボル数、解決成功率などをカウンタで収集（テストでは簡易的にアサート対象にする）。
  - *トレーシング*: パース開始/終了、解決試行ごとにspanを付けることで、ボトルネック検出を容易にする（このチャンクにはトレーシング導入コードは現れない）。

## Risks & Unknowns

- 不明点:
  - CppParser::parse の具体アルゴリズム・エラーモデル（Resultではない点から、失敗時の挙動は不明）。
  - ResolutionContext の内部データ構造（HashMap等かどうか）と重複名の扱い方。
  - ScopeLevel の優先度ルール（Module vs Localの衝突時の解決順）。
  - Symbol の詳細構造（型定義、rangeの正確な仕様など）。

- リスク:
  - 外部ファイル変更により統合テストが不安定化。
  - 過度な出力がCIログを汚染し、重要な失敗が埋もれる。
  - expect/unwrapの使用により、細かなエラー分類ができずデバッグが困難。

---

## Additional Rust-specific Analysis

- メモリ安全性（所有権/借用）
  - cpp_code: read_to_stringで所有するStringを取得し、parseには&cpp_code（不変借用）を渡すため安全。
  - parser: 可変参照（&mut self）でparseを呼ぶ設計。同期テストなので競合なし。
  - symbol_counter: &mut 渡しで採番する設計。共有可変状態はテスト関数内で単一所有のため安全。
  - symbol.name.to_string(): 名前登録時にコピーが発生。大量シンボル時にヒープ負荷が増加。

- unsafe境界
  - unsafeブロックは本チャンクには**存在しない**。

- 並行性・非同期
  - Send/Syncの考慮は不要（テストは単一スレッドで同期実行）。
  - await境界・キャンセル対応は**該当なし**。

- エラー設計
  - Result vs Option: resolveはOptionで「見つからない」を表現しており妥当。
  - panic箇所: read_to_string().expect(...)、symbols.iter().find(...).unwrap() など。テストでは許容されるが、メッセージの明確化/条件分岐で改善可能。
  - エラー変換: From/Intoの実装は**このチャンクには現れない**。

---