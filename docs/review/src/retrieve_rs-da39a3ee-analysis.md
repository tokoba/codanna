# retrieve.rs Review

## TL;DR

- 目的: 統一スキーマ（UnifiedOutput）でシンボル/関数の取得系コマンドを実装し、検索結果や関連関係を整形して出力する。
- 主要公開API: retrieve_symbol / retrieve_callers / retrieve_calls / retrieve_implementations / retrieve_search / retrieve_impact(DEPRECATED) / retrieve_describe。
- 複雑箇所: 名前/IDの両方でシンボルを特定する分岐、曖昧一致処理（callers/calls/describe）、関係取得とコンテキスト変換。
- 重大リスク: retrieve_callsのコメントにある「数値のみID対応」が未実装、retrieve_implementationsは曖昧一致を無視して最初の一致のみ採用、結果メタデータの一部を破棄。
- エラー設計: NotFoundはUnifiedOutputで返すが、曖昧時はstderrとGeneralErrorに偏り統一性が弱い。
- Rust安全性: unsafeは無し。unwrapは事前チェックありで妥当。Cowの使用でクエリ文字列の所有権管理も適切。
- 並行性: 非同期処理や共有可変状態は登場しない。レースやデッドロックの懸念は現時点では低い。

## Overview & Purpose

このファイルは、CLIなどから呼ばれる「retrieve」系コマンドをまとめて実装し、**UnifiedOutput**スキーマに沿って結果を整形します。対象は以下です。

- シンボル取得（名前またはsymbol_id指定）
- 関数の呼び出し元（callers）/呼び出し先（calls）
- トレイトの実装（implementations）
- 検索（search）
- 影響範囲（impact, 非推奨）
- シンボル詳細記述（describe）

各関数は **SimpleIndexer** に依存して、**SymbolContext**（関係情報付き）を生成し、**OutputManager** を用いてフォーマット別に出力します。クエリメタデータ（query, tool, timing_msなど）を添付可能な構造になっています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | retrieve_symbol | pub | 名前/IDでシンボルを特定し、関係付きコンテキストを出力 | Med |
| Function | retrieve_callers | pub | 関数の呼び出し元を取得し、関係付きコンテキストを出力 | Med |
| Function | retrieve_calls | pub | 関数の呼び出し先を取得し、関係付きコンテキストを出力 | Med |
| Function | retrieve_implementations | pub | トレイト/インターフェイスの実装を取得し、関係付きコンテキストを出力 | Low |
| Function | retrieve_search | pub | クエリ＋フィルタで検索し、結果のコンテキストを出力 | Med |
| Function | retrieve_impact | pub (deprecated) | 非推奨。影響範囲の取得 | Low |
| Function | retrieve_describe | pub | 単一シンボルの関係詳細（calls/called_by/defines/implemented_by）を出力 | Med |

### Dependencies & Interactions

- 内部依存
  - 各関数は共通して以下を利用します。
    - OutputManager::new(format) による出力管理の初期化。
    - UnifiedOutput / UnifiedOutputBuilder による統一的な出力組み立て。
    - SimpleIndexer によるシンボル/関係の取得。
    - SymbolContext への変換。
  - 関数同士の直接呼び出しは「該当なし」。

- 外部依存（このチャンクに現れるもの）

| モジュール/型 | 目的 |
|---------------|------|
| crate::io::{EntityType, ExitCode, OutputFormat, OutputManager, OutputStatus} | 出力の基本型と終了コード |
| crate::io::schema::{OutputData, OutputMetadata, UnifiedOutput, UnifiedOutputBuilder} | UnifiedOutputスキーマ |
| crate::symbol::context::{SymbolContext, ContextIncludes} | シンボルのファイルパス・関係を含むコンテキスト変換 |
| crate::{SimpleIndexer, Symbol} | インデクサとシンボル型 |
| crate::{SymbolId, SymbolKind, RelationKind} | シンボルID/種類/関係種別 |

- 被依存推定
  - CLIコマンド層・MCP/ツール側から呼ばれるユースケースが想定されるが、詳細は「不明」。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| retrieve_symbol | fn retrieve_symbol(indexer: &SimpleIndexer, name: &str, language: Option<&str>, format: OutputFormat) -> ExitCode | シンボル名またはIDで検索し関係付きコンテキストを返す | O(k·Cctx) | O(k) |
| retrieve_callers | fn retrieve_callers(indexer: &SimpleIndexer, function: &str, language: Option<&str>, format: OutputFormat) -> ExitCode | 関数の呼び出し元一覧を取得しコンテキスト化 | O(k·Cctx) | O(k) |
| retrieve_calls | fn retrieve_calls(indexer: &SimpleIndexer, function: &str, language: Option<&str>, format: OutputFormat) -> ExitCode | 関数の呼び出し先一覧を取得しコンテキスト化 | O(k·Cctx) | O(k) |
| retrieve_implementations | fn retrieve_implementations(indexer: &SimpleIndexer, trait_name: &str, language: Option<&str>, format: OutputFormat) -> ExitCode | トレイトの実装一覧を取得しコンテキスト化 | O(k·Cctx) | O(k) |
| retrieve_search | fn retrieve_search(indexer: &SimpleIndexer, query: &str, limit: usize, kind: Option<&str>, module: Option<&str>, language: Option<&str>, format: OutputFormat) -> ExitCode | 検索＋フィルタ結果をコンテキスト化 | O(min(limit, m)·Cctx) | O(min(limit, m)) |
| retrieve_impact | fn retrieve_impact(indexer: &SimpleIndexer, symbol_name: &str, max_depth: usize, format: OutputFormat) -> ExitCode | 非推奨。影響半径の取得（calls/callers） | O(r·Cctx) | O(r) |
| retrieve_describe | fn retrieve_describe(indexer: &SimpleIndexer, symbol_name: &str, language: Option<&str>, format: OutputFormat) -> ExitCode | 単一シンボルの詳細関係をまとめて出力 | O(Ccalls + Ccallers + Cdeps + Cimpl) | O(k) |

注:
- k = 結果件数
- Cctx = get_symbol_contextのコスト（「このチャンクには詳細実装がない」）
- m = インデックス内一致件数
- r = 影響半径内のシンボル件数

以下、各APIの詳細。

### retrieve_symbol

1) 目的と責務
- 名前または "symbol_id:<id>" の形式でシンボルを特定し、実装/定義/呼び出し元の関係を含む **SymbolContext** のリストを **UnifiedOutput** として返す。（関数名: retrieve_symbol, 行番号: 不明）

2) アルゴリズム（ステップ）
- OutputManager 初期化。
- 入力が "symbol_id:" で始まるか判定し、IDなら indexer.get_symbol で1件を取得。そうでなければ find_symbols_by_name。
- 結果が空なら NotFound を UnifiedOutput で返す。
- 結果があれば、各シンボルに対し indexer.get_symbol_context(...IMPLEMENTATIONS|DEFINITIONS|CALLERS) でコンテキスト化。
- UnifiedOutputBuilder で items とメタデータを構築し出力。

3) 引数

| 引数 | 型 | 意味 | 制約 |
|------|----|------|------|
| indexer | &SimpleIndexer | 取得用インデクサ | 非null |
| name | &str | シンボル名または "symbol_id:<id>" | idはu32 |
| language | Option<&str> | 言語フィルタ | 任意 |
| format | OutputFormat | 出力フォーマット | 有効なフォーマット |

4) 戻り値

| 戻り値 | 型 | 意味 |
|--------|----|------|
| code | ExitCode | 実行結果コード（Success/NotFound/GeneralError） |

5) 使用例

```rust
use crate::{SimpleIndexer};
use crate::io::OutputFormat;

fn example(indexer: &SimpleIndexer) {
    // 名前で検索
    let _ = retrieve_symbol(indexer, "process_request", Some("rust"), OutputFormat::Json);

    // IDで検索
    let _ = retrieve_symbol(indexer, "symbol_id:42", None, OutputFormat::Yaml);
}
```

6) エッジケース
- 空文字列の name
- "symbol_id:" だが数値変換失敗
- 見つかったが get_symbol_context が None を返す（filter_mapで除外される）
- 大量ヒット時のパフォーマンス

### retrieve_callers

1) 目的と責務
- 特定関数の「呼び出し元」を取得し、各呼び出し元に対する **CALLS|DEFINITIONS** 関係のコンテキストを返す。（関数名: retrieve_callers, 行番号: 不明）

2) アルゴリズム
- "symbol_id:" 接頭辞ならID解析→get_symbol。失敗時は NotFound または GeneralError（ID形式不正）。
- 名前検索の場合: 空なら NotFound。複数一致なら stderrに曖昧一覧を出し GeneralError。
- 1件なら get_calling_functions_with_metadata で呼び出し元一覧を取得。
- 得られた Symbol を get_symbol_context(...CALLS|DEFINITIONS) でコンテキスト化。
- UnifiedOutput で返却。

3) 引数

| 引数 | 型 | 意味 |
|------|----|------|
| indexer | &SimpleIndexer | インデクサ |
| function | &str | 関数名または "symbol_id:<id>" |
| language | Option<&str> | 言語フィルタ |
| format | OutputFormat | 出力形式 |

4) 戻り値

| 戻り値 | 型 | 意味 |
|--------|----|------|
| code | ExitCode | 成否コード |

5) 使用例

```rust
let _ = retrieve_callers(indexer, "handle_input", Some("rust"), OutputFormat::Json);
let _ = retrieve_callers(indexer, "symbol_id:99", None, OutputFormat::Text);
```

6) エッジケース
- ID文字列不正 → GeneralError
- 名前複数一致 → GeneralError（統一出力ではない）
- メタデータを捨てている（mapで (caller, _metadata) → caller）

### retrieve_calls

1) 目的と責務
- 特定関数の「呼び出し先」を取得し、各呼び出し先に対して **CALLERS|DEFINITIONS** 関係のコンテキストを返す。（関数名: retrieve_calls, 行番号: 不明）

2) アルゴリズム
- "symbol_id:" 接頭辞ならID解析→get_symbol。失敗時は NotFound/GeneralError。
- 名前検索の場合: 空なら NotFound。複数一致なら stderrに曖昧一覧→GeneralError。
- 1件なら get_called_functions_with_metadata で呼び出し先一覧を取得。
- 各 Symbol を get_symbol_context(...CALLERS|DEFINITIONS) でコンテキスト化。
- UnifiedOutput で返却。

注: コメントに「数値のみID（例: "123"）」対応と書かれているが、実装では未対応。

3) 引数

| 引数 | 型 | 意味 |
|------|----|------|
| indexer | &SimpleIndexer | インデクサ |
| function | &str | 関数名または "symbol_id:<id>" |
| language | Option<&str> | 言語フィルタ |
| format | OutputFormat | 出力形式 |

4) 戻り値

| 戻り値 | 型 | 意味 |
|--------|----|------|
| code | ExitCode | 成否コード |

5) 使用例

```rust
let _ = retrieve_calls(indexer, "do_work", None, OutputFormat::Yaml);
```

6) エッジケース
- コメントと実装の乖離（数値ID未対応）
- 名前複数一致 → GeneralError（統一出力ではない）
- メタデータ破棄

### retrieve_implementations

1) 目的と責務
- トレイト/インターフェイスの実装を一覧取得し、各実装の **DEFINITIONS|CALLERS** 関係をコンテキスト化。（関数名: retrieve_implementations, 行番号: 不明）

2) アルゴリズム
- find_symbols_by_name でトレイト候補取得。
- 先頭の1件があれば get_implementations で実装一覧を取得（複数候補は無視）。
- 各実装 Symbol を get_symbol_context(...DEFINITIONS|CALLERS) でコンテキスト化。
- UnifiedOutput で返却。

3) 引数

| 引数 | 型 | 意味 |
|------|----|------|
| indexer | &SimpleIndexer | インデクサ |
| trait_name | &str | トレイト名 |
| language | Option<&str> | 言語フィルタ |
| format | OutputFormat | 出力形式 |

4) 戻り値

| 戻り値 | 型 | 意味 |
|--------|----|------|
| code | ExitCode | 成否コード |

5) 使用例

```rust
let _ = retrieve_implementations(indexer, "Iterator", Some("rust"), OutputFormat::Json);
```

6) エッジケース
- 複数一致を考慮せず先頭のみ採用（曖昧性未処理）
- 実装ゼロ件時 → itemsが空の成功出力（仕様としては要確認）

### retrieve_search

1) 目的と責務
- クエリ＋種別/モジュール/言語フィルタで検索し、各結果の **IMPLEMENTATIONS|DEFINITIONS|CALLERS** を含むコンテキストを返す。（関数名: retrieve_search, 行番号: 不明）

2) アルゴリズム
- kindを文字列から SymbolKind に変換（未知は警告して無視）。
- indexer.search(query, limit, kind_filter, module, language) を実行し、失敗時は空にフォールバック（unwrap_or_default）。
- 各結果の symbol_id から get_symbol_context(...IMPLEMENTATIONS|DEFINITIONS|CALLERS)。
- UnifiedOutput で返却。

3) 引数

| 引数 | 型 | 意味 |
|------|----|------|
| indexer | &SimpleIndexer | インデクサ |
| query | &str | 検索文字列 |
| limit | usize | 最大件数 |
| kind | Option<&str> | 種別フィルタ（function/struct/...） |
| module | Option<&str> | モジュールフィルタ |
| language | Option<&str> | 言語フィルタ |
| format | OutputFormat | 出力形式 |

4) 戻り値

| 戻り値 | 型 | 意味 |
|--------|----|------|
| code | ExitCode | 成否コード |

5) 使用例

```rust
let _ = retrieve_search(indexer, "parse", 50, Some("function"), None, Some("rust"), OutputFormat::Json);
```

6) エッジケース
- kind未知 → 警告出力しフィルタ無効
- searchがErr → 空結果にフォールバック（エラー隠蔽）

### retrieve_impact（DEPRECATED）

1) 目的と責務
- 非推奨。影響半径（呼び出し元/先）を辿ってコンテキスト化。（関数名: retrieve_impact, 行番号: 不明）

2) アルゴリズム
- find_symbols_by_name で先頭シンボル取得。
- get_impact_radius(symbol.id, Some(max_depth)) で影響ID集合を取得。
- 各IDを get_symbol_context(...CALLERS|CALLS) でコンテキスト化。
- UnifiedOutput で返却。

3) 引数

| 引数 | 型 | 意味 |
|------|----|------|
| indexer | &SimpleIndexer | インデクサ |
| symbol_name | &str | シンボル名 |
| max_depth | usize | 影響半径の深さ |
| format | OutputFormat | 出力形式 |

4) 戻り値

| 戻り値 | 型 | 意味 |
|--------|----|------|
| code | ExitCode | 成否コード |

5) 使用例

```rust
// 非推奨APIの例
let _ = retrieve_impact(indexer, "old_fn", 3, OutputFormat::Json);
```

6) エッジケース
- 先頭一致のみ使用
- コマンド自体が非推奨、結果が空になりやすい

### retrieve_describe

1) 目的と責務
- 単一シンボルの詳細を記述し、**calls / called_by / defines / implemented_by** をまとめた **SymbolContext** を返す。（関数名: retrieve_describe, 行番号: 不明）

2) アルゴリズム
- "symbol_id:" の場合はID解析→get_symbol。そうでない場合は名前検索（空→NotFound、複数→曖昧でGeneralError）。
- SymbolContext::symbol_location(&symbol) でファイルパス取得。
- get_called_functions_with_metadata → callsがあれば relationships.calls に設定。
- get_calling_functions_with_metadata → callersがあれば relationships.called_by に設定。
- get_dependencies(symbol.id) から RelationKind::Defines を抽出し relationships.defines に設定。
- SymbolKind が Trait/Interface の場合、get_implementations → relationships.implemented_by に設定。
- UnifiedOutput（Single）で返却。

3) 引数

| 引数 | 型 | 意味 |
|------|----|------|
| indexer | &SimpleIndexer | インデクサ |
| symbol_name | &str | シンボル名または "symbol_id:<id>" |
| language | Option<&str> | 言語フィルタ |
| format | OutputFormat | 出力形式 |

4) 戻り値

| 戻り値 | 型 | 意味 |
|--------|----|------|
| code | ExitCode | 成否コード |

5) 使用例

```rust
let _ = retrieve_describe(indexer, "Parser::parse", Some("rust"), OutputFormat::Yaml);
let _ = retrieve_describe(indexer, "symbol_id:7", None, OutputFormat::Json);
```

6) エッジケース
- 名前複数一致 → GeneralError（stderr）
- 依存関係の一部欠損 → relationshipsの各OptionはNoneのまま

## Walkthrough & Data Flow

全体フローはおおむね以下です。

- 入力文字列を **名前** か **symbol_id** として解釈。
- **SimpleIndexer** を通して対象の **Symbol** または検索結果を取得。
- 必要に応じて **get_symbol_context** で関係付きの **SymbolContext** に変換。
- **UnifiedOutputBuilder** または **UnifiedOutput** を構築し、**OutputManager** でフォーマットに応じて出力。
- 終了コード（ExitCode）を返却。

以下は分岐が比較的多い retrieve_callers の主要分岐のフローチャートです。

```mermaid
flowchart TD
    A[Start retrieve_callers] --> B{function starts with "symbol_id:"?}
    B -- Yes --> C[parse u32 id]
    C -- Ok --> D[get_symbol(SymbolId)]
    C -- Err --> Z1[stderr: Invalid symbol_id] --> Z2[ExitCode::GeneralError]
    D -- Some(sym) --> E[query_str = "symbol_id:<id>"]
    D -- None --> F[UnifiedOutput NotFound(Function)] --> G[OutputManager::unified] --> H[ExitCode]
    B -- No --> I[find_symbols_by_name(function, language)]
    I -- Empty --> J[UnifiedOutput NotFound(Function)] --> K[OutputManager::unified] --> L[ExitCode]
    I -- >1 --> M[stderr: Ambiguous list] --> N[ExitCode::GeneralError]
    I -- Single --> O[symbol = single match; query_str = function]
    E --> P[get_calling_functions_with_metadata(symbol.id)]
    O --> P
    P --> Q[map (caller, _metadata) -> caller]
    Q --> R[for each caller: get_symbol_context(CALLS|DEFINITIONS)]
    R --> S[UnifiedOutputBuilder::items(EntityType::Function)]
    S --> T[OutputManager::unified]
    T --> U[ExitCode]
```

上記の図は retrieve_callers 関数（行番号: 不明）の主要分岐を示す。

## Complexity & Performance

- 共通傾向
  - 検索段階はインデクサ実装に依存します。一般的には名前検索は O(S)（Sはインデックス規模）またはより効率化されている可能性がありますが「不明」。
  - コンテキスト取得（get_symbol_context）は関係展開のため、各件数に比例してコストが掛かる（O(Cctx)）。複数結果（k件）では O(k·Cctx）。
  - メモリは結果件数に比例（SymbolContextのベクタ/Single格納）。

- ボトルネック
  - 大量の結果に対して都度 **get_symbol_context** を呼び出す点。
  - 検索結果がエラーでも unwrap_or_default により握り潰すため、障害検知が遅れる可能性（retrieve_search）。

- スケール限界
  - 呼び出し元/先が大量な関数では、関係展開が重くなり出力サイズ・処理時間が増加。
  - metadataを破棄することで、後段の最適化やキャッシュ利用余地が失われる。

- 実運用負荷
  - I/O/ファイルパス解決は SymbolContext::symbol_location に依存（詳細不明）。
  - ネットワーク/DBの使用有無はこのチャンクには現れない。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト観点と併せ、具体的なエッジケースを以下に整理します。

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列（名前検索） | "" | NotFoundを統一出力 | find_symbols_by_name次第 | 要確認 |
| symbol_id形式不正 | "symbol_id:abc" | エラー（妥当なメッセージ） | eprintln→GeneralError | OK |
| 名前曖昧一致（callers/calls/describe） | "foo"が複数一致 | 統一出力でAmbiguous返却 | stderr+GeneralError | 改善余地 |
| retrieve_callsの数値ID対応 | "123" | IDとして扱う | 未対応（コメントと乖離） | Bug |
| retrieve_implementationsの曖昧一致 | "TraitX"が複数一致 | ユーザ選択/エラー | 先頭のみ採用 | Risk |
| 検索kind未知 | kind="unknown" | 無視して検索継続（警告） | Warning+None | OK |
| searchのErr | 内部エラー | エラー伝播 | unwrap_or_defaultで握り潰し | 改善余地 |
| メタデータ破棄 | callers/calls | 呼び出しメタ情報伝達 | (caller, _metadata)破棄 | 改善余地 |
| ログ漏えい | ambiguous時にfile_path表示 | パス情報取り扱い注意 | stderrへ出力 | 注意 |
| UnifiedOutput書き込み失敗 | 出力先不具合 | GeneralError | eprintln→GeneralError | OK |

Rust特有の観点
- メモリ安全性
  - **所有権**: retrieve_describeで symbol.clone() を使用（関数名: retrieve_describe, 行番号: 不明）。他は参照中心で移動は無し。
  - **借用**: indexerは &SimpleIndexer、languageは Option<&str> で不変借用のみ。可変借用なし。
  - **ライフタイム**: Cow の使用（Borrowed/Owned）でメタデータのライフタイム管理は適切。
- unsafe境界
  - unsafeブロックは「該当なし」。
- 並行性・非同期
  - Send/Sync/await等は登場しない。「このチャンクには現れない」。
- エラー設計
  - Option/Resultの使い分け: 多くが Option を返すインデクサAPIに依存。unwrapは事前チェック済みのためパニックは妥当（retrieve_callers/calls/describeの単一一致後のunwrap）。
  - エラー変換: 統一出力でNotFoundを返しているが曖昧時はstderr+GeneralErrorで統一性が欠ける。

## Design & Architecture Suggestions

- **共通ヘルパーの導入**: 名前/IDから単一シンボルを特定する共通関数（例: resolve_symbol(input, language) -> Result<(Symbol, query_str), ResolveError>）を用意し、retrieve_callers/calls/describe の重複を削減。
- **曖昧一致の統一出力**: stderrではなく **UnifiedOutput** に **Ambiguous** などのステータスや候補一覧を載せると、機械可読性とUXが向上。
- **数値ID対応の整合性**: retrieve_calls のコメントに沿って "123" の数値のみもID解釈する。retrieve_callers/describe 等にも適用し一貫性を確保。
- **メタデータの活用**: get_*_with_metadata のメタデータを **SymbolContext.relationships** に反映（呼び出し位置・重みなどがあるなら保持）。
- **エラー伝播**: retrieve_search の unwrap_or_default を廃し、検索エラーも **UnifiedOutput** の **OutputStatus::Error** 等で返す。
- **観測可能性の強化**: timing_ms を実測値で埋める、truncated を大規模出力時に適切に設定。
- **ログ/ガイダンス**: Ambiguous時に **guidance** フィールドへ推奨コマンド（例: "use: codanna retrieve callers symbol_id:<id>"）を出力（既存stderr文を移行）。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト方針
  - NotFoundパス検証: find_symbols_by_nameが空を返す場合の ExitCode と UnifiedOutput。
  - symbol_id不正: "symbol_id:abc" で GeneralError。
  - 曖昧一致: 複数一致で GeneralError（現在仕様）を返すこと。
  - シングル一致: callers/calls/describe が正しい ContextIncludes を使うこと。
  - search: kindフィルタが未知でも警告して検索続行、Errの握り潰しの挙動（改善後はError出力）。

- 依存のモック/スタブ
  - SimpleIndexer の具体的な生成・モック化は「このチャンクには現れない」。テストでは事前に用意されたインデクサを注入する。

- 使用例（テストスケルトン）

```rust
#[test]
fn test_retrieve_symbol_not_found() {
    let indexer: SimpleIndexer = /* 準備済みのインデクサ */ unimplemented!();
    let code = retrieve_symbol(&indexer, "nonexistent", Some("rust"), crate::io::OutputFormat::Json);
    assert!(matches!(code, crate::io::ExitCode::NotFound));
}

#[test]
fn test_retrieve_callers_ambiguous() {
    let indexer: SimpleIndexer = /* 準備済みのインデクサ */ unimplemented!();
    let code = retrieve_callers(&indexer, "ambiguous_fn", None, crate::io::OutputFormat::Json);
    assert!(matches!(code, crate::io::ExitCode::GeneralError));
}

#[test]
fn test_retrieve_calls_invalid_id() {
    let indexer: SimpleIndexer = /* 準備済みのインデクサ */ unimplemented!();
    let code = retrieve_calls(&indexer, "symbol_id:abc", None, crate::io::OutputFormat::Json);
    assert!(matches!(code, crate::io::ExitCode::GeneralError));
}

#[test]
fn test_retrieve_describe_single() {
    let indexer: SimpleIndexer = /* 準備済みのインデクサ */ unimplemented!();
    let code = retrieve_describe(&indexer, "symbol_id:42", None, crate::io::OutputFormat::Json);
    assert!(matches!(code, crate::io::ExitCode::Success));
}
```

- 統合テスト
  - 実際のインデックスを読み込んだ状態で、JSON/YAML/TEXTなどフォーマット別の出力整合性テスト。
  - 大量結果時の性能・truncatedフラグの検証（改善後）。

## Refactoring Plan & Best Practices

- **DRY原則**: 名前/ID解決、NotFound出力、Ambiguous処理の共通化。
- **堅牢なエラー処理**: Resultを可能な範囲で伝播し、統一出力にエラー情報を含める。
- **一貫したID取扱い**: 全コマンドで "symbol_id:<id>" と数値のみの両方に対応するか、ガイドラインを明確化。
- **メタデータの保持**: 呼び出しメタデータを SymbolContext 内へ統合し、後続のUX/分析を強化。
- **ログ改善**: eprintlnではなく構造化ログ（レベル/カテゴリ）や UnifiedOutput.guidance の活用。
- **パフォーマンス対策**: get_symbol_context の呼び出し回数削減のためのバッチAPIやキャッシュ導入を検討。

## Observability (Logging, Metrics, Tracing)

- 現状
  - エラー時のみ eprintln。**OutputMetadata.timing_ms** は常に None、**truncated** も未設定。
- 改善提案
  - **⏱️ 計測**: コマンド開始から出力までの実時間を計測し timing_ms を設定。
  - **📊 メトリクス**: NotFound件数、Ambiguous件数、成功件数、結果件数分布をカウント。
  - **🧭 ガイダンス**: Ambiguous時の推奨操作を guidance に設定。
  - **🔭 トレース**: indexer呼び出しのスパンを記録（このチャンク内にはトレーシング基盤は「不明」）。

## Risks & Unknowns

- **Indexer実装不明**: find/search/get_*_with_metadata/get_symbol_context の具体コストや失敗条件が「不明」。
- **出力先の信頼性**: OutputManager::unified が失敗時にGeneralErrorへフォールバックするのみ。再試行や詳細な失敗原因出力は無し。
- **仕様整合性**: retrieve_callsのコメントと実装の不一致、曖昧時の統一出力欠如。
- **拡張性**: メタデータ破棄により、将来の高度な表示（重み付け、位置情報、ヒートマップなど）への拡張余地が限定される可能性。
- **セキュリティ/プライバシー**: stderrにファイルパスを出力するため、パス情報の公開可否に注意が必要。