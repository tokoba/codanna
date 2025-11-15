# lib.rs Review

## TL;DR

- 本ファイルはクレートのエントリポイント兼フェイスードで、サブモジュールの公開と主要型/関数の再エクスポートを行う
- 唯一のロジックは、ランタイムフラグで標準エラー出力にデバッグメッセージを出すマクロ**debug_print!**（L8-L15）で、フラグは**config::is_global_debug_enabled()**（このチャンクには実装なし）に依存
- 外部依存は**tree_sitter_kotlin_codanna**を**tree_sitter_kotlin**に別名割り当て（L5）
- 公開APIとして多数の型・エラー型・ユーティリティを再エクスポート（L36-49）が、本チャンクでは実装詳細は不明
- 重大なリスクは少ないが、debugマクロの設計（$self引数未使用、二重フォーマット、機密情報のログ漏洩の可能性）に改善余地あり
- 並行性・安全性は概ね問題ないが、グローバルなデバッグフラグの実装がスレッドセーフかは不明（このチャンクには現れない）

## Overview & Purpose

- 本ファイルはライブラリクレートの中核エントリーポイントとして、以下を提供する:
  - 外部クレートの別名化: **tree_sitter_kotlin_codanna → tree_sitter_kotlin**（L5）
  - デバッグ出力マクロ: **debug_print!**（L8-L15）
  - サブモジュールの公開宣言: **config, display, error, indexing, init, io, mcp, parsing, plugins, profiles, project_resolver, relationship, retrieve, semantic, storage, symbol, types, vector**（L17-L34）
  - 主要型・関数の再エクスポートで、利用側からの参照パスを単純化（L36-49）
- 目的は、利用者が codanna クレートから一貫した API を import できるようにする「フェイスード」と、統一的なデバッグ出力手段の提供

注: 行番号は本チャンク内の目安です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Extern Crate Alias | tree_sitter_kotlin | crate内 | Kotlin用Tree-sitter依存の別名（L5） | Low |
| Macro | debug_print! | pub（#[macro_export]） | グローバルデバッグフラグに基づく一貫したデバッグ出力（L8-L15） | Low |
| Module | config | pub | 設定（is_global_debug_enabled() を提供、詳細不明）（L17） | 不明 |
| Module | display | pub | 表示/レンダリング関連（推定）（L18） | 不明 |
| Module | error | pub | エラー型と結果型（L19） | 不明 |
| Module | indexing | pub | インデクシング（SimpleIndexer, calculate_hash 再エクスポート）（L20） | 不明 |
| Module | init | pub | 初期化ロジック（推定）（L21） | 不明 |
| Module | io | pub | 入出力（推定）（L22） | 不明 |
| Module | mcp | pub | MCP関連（推定）（L23） | 不明 |
| Module | parsing | pub | パーシング（RustParser 再エクスポート）（L24） | 不明 |
| Module | plugins | pub | プラグイン拡張（推定）（L25） | 不明 |
| Module | profiles | pub | プロファイル（推定）（L26） | 不明 |
| Module | project_resolver | pub | プロジェクト解決（推定）（L27） | 不明 |
| Module | relationship | pub | 関係グラフ（Relationshipなど）（L28） | 不明 |
| Module | retrieve | pub | 取得系（推定）（L29） | 不明 |
| Module | semantic | pub | セマンティック解析（推定）（L30） | 不明 |
| Module | storage | pub | ストレージ/永続化（IndexPersistence）（L31） | 不明 |
| Module | symbol | pub | シンボル/スコープ（Symbolなど）（L32） | 不明 |
| Module | types | pub | 基本型（CompactStringなど）（L33） | 不明 |
| Module | vector | pub | ベクタ/ベクトル検索（推定）（L34） | 不明 |

### Dependencies & Interactions

- 内部依存
  - **debug_print! → config::is_global_debug_enabled()**（L11）: グローバルなデバッグフラグに依存して出力の有無を切り替える
  - その他のモジュール間の呼び出しはこのチャンクには現れない
- 外部依存（例）
  - | 依存名 | 目的 | 備考 |
    |--------|------|------|
    | tree_sitter_kotlin_codanna → tree_sitter_kotlin | Kotlin構文解析 | 将来的に upstream 0.3.9+ への移行意図コメントあり（L2-L4） |
    | std::io::eprintln!, std::fmt::format! | 標準エラー出力と文字列整形 | マクロ内部で使用（L12） |
- 被依存推定
  - 利用者は codanna クレートから直接、再エクスポートされた型/関数（例: **SimpleIndexer, RustParser, Relationship, IndexPersistence** など）を use/import する
  - 具体的な利用箇所・呼び出し関係はこのチャンクには現れない

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| debug_print! | macro debug_print!($self:expr, $($arg:tt)*) | デバッグ時の一貫した標準エラー出力 | O(m) | O(m) |
| Settings | struct Settings（詳細不明） | 設定管理 | — | — |
| IndexError | enum/struct（詳細不明） | インデクシングのエラー型 | — | — |
| IndexResult | type alias（詳細不明） | 結果型（Result） | — | — |
| McpError | enum/struct（詳細不明） | MCP関連エラー型 | — | — |
| McpResult | type alias（詳細不明） | 結果型（Result） | — | — |
| ParseError | enum/struct（詳細不明） | パースエラー型 | — | — |
| ParseResult | type alias（詳細不明） | 結果型（Result） | — | — |
| StorageError | enum/struct（詳細不明） | ストレージエラー型 | — | — |
| StorageResult | type alias（詳細不明） | 結果型（Result） | — | — |
| SimpleIndexer | struct/trait（詳細不明） | インデクサ | 不明 | 不明 |
| calculate_hash | fn（詳細不明） | ハッシュ計算 | 不明 | 不明 |
| RustParser | struct/trait（詳細不明） | Rust解析器 | 不明 | 不明 |
| RelationKind | enum（詳細不明） | 関係の種類 | — | — |
| Relationship | struct（詳細不明） | エンティティ間関係 | — | — |
| RelationshipEdge | struct（詳細不明） | 関係エッジ | — | — |
| IndexPersistence | trait/struct（詳細不明） | インデックス永続化 | 不明 | 不明 |
| CompactSymbol | struct（詳細不明） | 軽量シンボル表現 | — | — |
| ScopeContext | struct（詳細不明） | スコープ情報 | — | — |
| StringTable | struct（詳細不明） | 文字列テーブル | — | — |
| Symbol | struct（詳細不明） | シンボル | — | — |
| Visibility | enum（詳細不明） | 可視性 | — | — |
| CompactString | struct（詳細不明） | メモリ効率の良い文字列 | — | — |
| FileId | newtype（詳細不明） | ファイル識別子 | — | — |
| IndexingResult | type alias（詳細不明） | インデクシング結果 | — | — |
| Range | struct（詳細不明） | 位置範囲 | — | — |
| SymbolId | newtype（詳細不明） | シンボルID | — | — |
| SymbolKind | enum（詳細不明） | シンボル種別 | — | — |
| compact_string | fn/macro（詳細不明） | CompactString作成ヘルパ | 不明 | 不明 |

注: 上記の多くは再エクスポートであり、具体的なデータ契約やシグネチャは「このチャンクには現れない」。

### debug_print! の詳細

1) 目的と責務
- 目的: グローバルなデバッグフラグに応じて、統一フォーマットでデバッグメッセージを標準エラー出力へ出力する
- 根拠: マクロ定義（L8-L15）、フラグ参照（L11）、eprintln!による出力（L12）

2) アルゴリズム（ステップ）
- config::is_global_debug_enabled() を評価
- true の場合のみ、format! で整形したメッセージに "DEBUG: " を付けて eprintln! に渡す

3) 引数

| 引数 | 種類 | 説明 |
|------|------|------|
| $self | expr | 呼び出し側で self を渡す前提の形にしているが、マクロ内で未使用（設計意図は呼び出し形の統一と推測） |
| $($arg:tt)* | token tree | format! に渡す可変長引数（フォーマット文字列＋パラメータ） |

4) 戻り値

| 戻り値 | 説明 |
|--------|------|
| なし | マクロのため副作用のみ（標準エラー出力） |

5) 使用例

```rust
impl MyType {
    fn do_something(&self, x: i32) {
        // デバッグフラグが有効なら "DEBUG: value=42" のように出力
        debug_print!(self, "value={}", x);
    }
}
```

6) エッジケース
- デバッグ無効時: 出力されない（L11 の条件で制御）
- フォーマット不一致: コンパイル時フォーマット検査によりエラー
- 巨大なメッセージ: 一時文字列の割り当て（format!）コストがかかる
- $self に副作用がある式を渡しても評価されない（未展開のため）。驚き最小のためガイドの明記を推奨

## Walkthrough & Data Flow

- コアのデータフロー（このチャンクに存在するもの）
  - 呼び出し側 → debug_print! → config::is_global_debug_enabled()（ブール） → 条件成立なら eprintln! で "DEBUG: ..." を出力
- 実装根拠
  - マクロ本体（L8-L15）
  - フラグ参照（L11）
  - 出力（L12）

抜粋（短いため全体引用）:

```rust
#[macro_export]
macro_rules! debug_print {
    ($self:expr, $($arg:tt)*) => {
        if $crate::config::is_global_debug_enabled() {
            eprintln!("DEBUG: {}", format!($($arg)*));
        }
    };
}
```

- 再エクスポートされたAPIの連携・データフローはこのチャンクには現れない

## Complexity & Performance

- debug_print!
  - 時間計算量: O(m)（メッセージ長 m に比例、format!＋eprintln!）
  - 空間計算量: O(m)（format! による一時 String 割り当て）
  - ボトルネック: デバッグが大量に出る場合、標準エラー出力が I/O ボトルネックに
- その他（SimpleIndexer, calculate_hash など）
  - 本チャンクに実装がないため不明

スケール限界・実運用負荷要因
- 標準エラー出力は同期I/Oであり、多スレッド高頻度出力時にスループットが低下しうる
- デバッグフラグが無効なら実行経路が回避されるためオーバーヘッドは最小

## Edge Cases, Bugs, and Security

セキュリティチェックリスト観点

- メモリ安全性
  - Rustの安全なマクロと標準ライブラリのみ使用。unsafe 不使用 → Buffer overflow / UAF / Integer overflow の懸念なし（このファイルに限る）
- インジェクション
  - SQL/Command/Path 等の組み立て無し → 該当なし
- 認証・認可
  - 機能無し → 該当なし
- 秘密情報
  - デバッグ出力が秘匿情報を含む可能性。フラグ誤設定や本番での有効化に注意
- 並行性
  - eprintln! は内部でロックされるが、高頻度時に競合・スループット低下あり
  - config::is_global_debug_enabled() の実装がスレッドセーフかは不明（このチャンクには現れない）

潜在バグ・設計懸念
- マクロの第1引数 $self が未使用（L10）。呼び出し一貫性の狙いだとしても、式の評価が行われないため副作用がある式を渡すと驚きを生む可能性
- eprintln!("DEBUG: {}", format!(...)) により一時 String を生成（不要な割り当て）。format_args! を使えば割り当て削減が可能
- #[macro_export] によりマクロがクレートルートにエクスポートされるため、他クレートでの名前衝突リスクは小さいがゼロではない

エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| デバッグ無効 | フラグ=false | 何も出力しない | if 条件（L11） | 実装済 |
| デバッグ有効 | フラグ=true, "x={}" 1 | "DEBUG: x=1" を出力 | eprintln!（L12） | 実装済 |
| フォーマット不一致 | "x={}" に引数なし | コンパイルエラー | Rustのformat! 検査 | 言語仕様 |
| 長いメッセージ | 100KB文字列 | 出力はされるがI/O遅延 | 一時割り当て＋I/O | 想定通り |
| $selfに副作用 | debug_print!(do_side_effect(), "...") | 副作用は起きない（未使用） | マクロ未展開 | 想定外の驚き |

## Design & Architecture Suggestions

- debug_print! 改善
  - $self を引数から外し、単純な形に統一するか、明確に未使用であることをドキュメント化
  - 不要な String 割り当てを避けるため format_args! を使う
  - フォーマットリテラル経路の最適化（concat!）を追加パターンで用意

例（代替実装案）

```rust
#[macro_export]
macro_rules! debug_print {
    // 最適化: フォーマットがリテラルの場合は concat! で "DEBUG: " をマージ
    ($fmt:literal $(, $args:expr )* $(,)?) => {
        if $crate::config::is_global_debug_enabled() {
            eprintln!(concat!("DEBUG: ", $fmt) $(, $args )*);
        }
    };
    // フォーマット文字列が動的な場合
    ($fmt:expr $(, $args:expr )* $(,)?) => {
        if $crate::config::is_global_debug_enabled() {
            eprintln!("DEBUG: {}", format_args!($fmt $(, $args )*));
        }
    };
}
```

- ロギング基盤との統合
  - 環境や要件に応じて、log や tracing クレートとの連携を検討（本番では eprintln! ではなく適切なサブスクライバ/アペンダへ）
- フィーチャーフラグ
  - compile-time で無効化できる feature（例: "debug-logs"）を導入し、不要なブランチ条件を完全に削除可能に
- extern crate エイリアス
  - 2018 edition 以降は Cargo.toml の package エイリアスか `pub use ... as ...` の利用を検討し、extern crate を排除

## Testing Strategy (Unit/Integration) with Examples

ユニットテスト（マクロ）

- デバッグ無効時に出力されないことの検証
  - 前提: config::is_global_debug_enabled() を切り替え可能な API があること（このチャンクには現れないため擬似コード）

```rust
#[test]
fn debug_print_is_noop_when_disabled() {
    // arrange: フラグを false に
    // config::set_global_debug_enabled(false); // 仮API
    // act
    debug_print!((), "hello {}", 42);
    // assert: 何も stderr に出ていないこと（テストハーネスがCaputre）
    // 注: Rust のテストハーネスは標準出力/標準エラーを捕捉するため、失敗しなければOKとする運用も可
}
```

- デバッグ有効時に所定のプレフィックス付きで出力される

```rust
#[test]
fn debug_print_outputs_when_enabled() {
    // config::set_global_debug_enabled(true); // 仮API
    debug_print!((), "answer={}", 42);
    // 実際の検証には一時的に eprintln! を io::Write に差し替えるヘルパや
    // テストフレームワークのキャプチャ機能を活用
}
```

統合テスト（再エクスポート）
- APIの import が期待通りに機能することのコンパイルテスト

```rust
use codanna::{Settings, SimpleIndexer, RustParser, Relationship, IndexPersistence};

#[test]
fn reexports_compile() {
    // 型を参照するだけのコンパイルテスト
    let _settings: Option<Settings> = None;
}
```

注意
- 本チャンクには config の切り替え API が存在しないため、テスト実装はモジュール側の提供に依存

## Refactoring Plan & Best Practices

- マクロ
  - $self 引数の廃止 または ドキュメント化（未使用で副作用も発生しない旨）
  - format_args! とパターン分岐で割り当て削減
  - ログレベルの概念拡張（debug/info/warn/error）を検討（log/tracing 連携）
- エクスポートの整理
  - prelude モジュール（use codanna::prelude::*;）を用意し、よく使う型/トレイト/関数を集約
  - 公開APIのドキュメント（cargo doc）に再エクスポート元へのリンクを明示
- 依存解決
  - tree_sitter_kotlin の upstream が 0.3.9+ になったら Cargo.toml と alias 方針を見直す（L2-L4のTODOコメントに沿う）
- Edition 準拠
  - 2018+ では extern crate を避け、Cargo 側エイリアスや pub use で統一

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 本番運用では eprintln! ではなく **tracing**（推奨）や **log** へ出力し、出力先・レベル・構造化フィールドを制御
  - デバッグメッセージにコンテキスト（ファイル/行/モジュール）を付加するなら `file!()`, `line!()`, `module_path!()` を活用
- メトリクス
  - デバッグ出力回数のメトリクス化は通常不要だが、スパム検知やレート制御が必要な場合はカウンタ導入を検討
- トレーシング
  - 重要な経路には `tracing::instrument` 相当のスパンを導入（このチャンクには対象コードなし）

## Risks & Unknowns

- config::is_global_debug_enabled() のスレッドセーフ性や設定の生存期間（グローバル状態）: 不明（このチャンクには現れない）
- 再エクスポートされた各APIのシグネチャ・不変条件・エラーモデル: 不明（このチャンクには現れない）
- calculate_hash のアルゴリズム・安定性・衝突特性: 不明（このチャンクには現れない）
- IndexPersistence の永続化戦略（同期/非同期、トランザクション、破損耐性）: 不明（このチャンクには現れない）
- tree_sitter_kotlin のバージョン差異・互換性: 将来の移行TODOがあるが詳細は不明

以上の通り、本ファイルは公開APIのハブとして適切に機能している一方、debugマクロ周辺の設計・性能・運用性には小さな改善余地がある。全体の安全性・並行性・エラー設計は、本チャンク外の各モジュール実装に依存する。