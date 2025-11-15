# parsers\kotlin\test_type_usage.rs Review

## TL;DR

- 目的: Kotlinコード中の型使用（コンストラクタ引数・関数引数・戻り値・プロパティ）の検出を、外部パーサーの出力で検証するユニットテスト。
- 主要外部API: **KotlinParser::new** と **KotlinParser::find_uses**（いずれも codanna クレート由来）。公開API（pub）はこのファイルには存在しない。
- 複雑箇所: プリミティブ型（String, Int）のフィルタリング検証、戻り値型の検出、利用コンテキスト名（関数名/クラス名/プロパティ名）の取り扱い。
- 重大リスク: 署名や返却データ構造の詳細が不明。テストは string マッチに依存し、コンテキストの命名仕様が曖昧な場合に誤判定の可能性。Kotlinの複合型（List<Row>等）の扱いはテスト未網羅。
- エラー設計: new().expect(...) によりパーサー生成失敗時に panic（テストとしては妥当だが、失敗理由の観測性は低い）。
- セキュリティ: 外部入力やI/Oはなく、インジェクション等のリスクは実質なし。ログ出力は println! のみ。
- パフォーマンス: find_uses の計算量は不明（推定: 入力コード長に比例）。テスト側のオーバーヘッドは極小。

## Overview & Purpose

このファイルは、codanna::parsing::kotlin::KotlinParser が提供する型使用検出機能を、Kotlinコード断片を用いたユニットテストで検証する。具体的には以下の観点を確認する。

- クラスコンストラクタのパラメータ型の使用検出
- 関数パラメータ型および戻り値型の使用検出
- プロパティ（val/var）の型使用検出
- プリミティブ型（String, Int）の除外（フィルタリング）

テストは、find_uses が返す「(コンテキスト名, 使用型名, 位置情報 range)」のタプル列から、期待するペアの存在を assert する方式で記述されている。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_kotlin_constructor_parameter_types | test | Kotlinクラスのコンストラクタ引数型（PgClient, ReadWritePgClient）の検出確認 | Low |
| Function | test_kotlin_function_parameter_types | test | 関数引数型（User, UserValidator）、戻り値型（Result, User）検出とプリミティブ型（String, Int）の除外確認 | Med |
| Function | test_kotlin_property_types | test | プロパティ型（Database, CacheManager, Logger）の検出確認 | Low |

### Dependencies & Interactions

- 内部依存
  - 各テストは重複する共通処理を実施:
    - Kotlinコード文字列の定義
    - `KotlinParser::new().expect(...)` によるパーサ生成
    - `parser.find_uses(code)` の呼び出し
    - 出力から `(context, used_type)` のペア集合 `use_pairs` を作り `contains` を用いて検証
  - テスト間の直接呼び出し関係はなし。

- 外部依存（推定を含む）

| 依存対象 | 種別 | 用途 | 備考 |
|---------|------|------|------|
| codanna::parsing::LanguageParser | Trait | KotlinParserが実装している可能性 | シグネチャ詳細は不明 |
| codanna::parsing::kotlin::KotlinParser | Struct/Parser | `new()` による生成、`find_uses(&str)` による型使用検出 | 返却型の内部構造は不明だが `(context, used_type, range)` を反復可能 |
| range.start_line | Structフィールド | 検出位置の行番号出力 | Range型の定義はこのチャンクに現れない |

- 被依存推定
  - このテストモジュールは、プロジェクトのKotlin解析機能の品質保証に利用される。ほかのモジュールからの直接依存はなく、テストランナー（cargo test）による実行のみ。

## API Surface (Public/Exported) and Data Contracts

このファイル自体の公開APIはない（テストのみ）。ただし外部APIの使用状況を整理する。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| KotlinParser::new | 不明（Option/Resultを返す可能性） | パーサの生成 | O(1)（推定） | O(1)（推定） |
| KotlinParser::find_uses | 不明（推定: `fn find_uses(code: &str) -> Vec<(Context, Type, Range)>`） | Kotlinコードから型使用を抽出 | O(n)（推定、n=コード長） | O(k)（推定、k=検出件数） |

詳細（各API）:

1) KotlinParser::new
- 目的と責務
  - Kotlinコード解析用パーサのインスタンス生成。
- アルゴリズム（推定）
  - 内部状態初期化のみ（詳細不明）。
- 引数

| 名前 | 型 | 説明 |
|------|----|------|
| なし | なし | 引数は存在しないように見える（このチャンクの呼び出し形からの推定） |

- 戻り値（不明）
  - Option<KotlinParser> または Result<KotlinParser, E> のいずれか（`.expect("...")` が呼べるため）。
- 使用例
```rust
let mut parser = KotlinParser::new().expect("Failed to create parser");
```
- エッジケース
  - new が失敗するケース（構成不全など）はテストで panic となる。失敗理由の詳細は観測できない。

2) KotlinParser::find_uses
- 目的と責務
  - 与えられたKotlinコード文字列から、型使用の発生箇所を抽出し、各使用の「コンテキスト名」「型名」「位置情報」を返す。
- アルゴリズム（推定）
  - コードを走査し、構文上の型位置（コンストラクタ引数、関数引数、戻り値、プロパティ宣言等）を識別。
  - プリミティブ型（String, Int 等）をフィルタリング。
  - 使用コンテキスト名（関数名／クラス名／プロパティ名）を付与。
  - 位置情報（range.start_line）を計算。
- 引数

| 名前 | 型 | 説明 |
|------|----|------|
| code | &str（推定） | Kotlinコード文字列（このチャンクでは生文字列リテラルから渡されている） |

- 戻り値（推定）

| 要素 | 型 | 説明 |
|------|----|------|
| context | Display/ToString可能な型（推定: String） | 使用が発生したコンテキスト名 |
| used_type | Display/ToString可能な型（推定: String） | 使用された型名 |
| range | Struct（詳細不明） | 位置情報。少なくとも `start_line` を持つ |

- 使用例
```rust
let uses = parser.find_uses(code);
let use_pairs: Vec<(String, String)> = uses
    .iter()
    .map(|(context, used_type, _)| (context.to_string(), used_type.to_string()))
    .collect();
```
- エッジケース
  - プリミティブ型の扱い（String, Int は除外されることが期待）
  - ジェネリクス（List<Row>）の内包型抽出可否は不明
  - ネスト型、関数型、Nullable型（User?）の検出可否は不明
  - インポート／型エイリアスの解決は不明

## Walkthrough & Data Flow

3つのテストは概ね同型のデータフローで動作する。

1) Kotlinコード文字列を raw string リテラルで定義
2) `KotlinParser::new().expect(...)` でパーサを生成
3) `parser.find_uses(code)` を実行して使用一覧（uses）を取得
4) `uses` を列挙してログ出力（println!）
5) `(context, used_type)` のペアベクタを作成（to_string で所有文字列化）
6) `assert!` で期待する使用ペアの存在検証、および除外条件の検証

代表的抜粋（test_kotlin_function_parameter_types より）:

```rust
let mut parser = KotlinParser::new().expect("Failed to create parser");
let uses = parser.find_uses(code);

println!("Found {} type uses:", uses.len());
for (context, used_type, range) in &uses {
    println!(
        "  {} uses {} at line {}",
        context, used_type, range.start_line
    );
}

let use_pairs: Vec<(String, String)> = uses
    .iter()
    .map(|(context, used_type, _)| (context.to_string(), used_type.to_string()))
    .collect();

/* ... 省略: 個別の assert チェック ... */
```

データフロー上のポイント:
- `uses` は3要素タプルの反復可能コレクション（推定: Vec）。
- ログには `range.start_line` が使われるが、テストの合否には影響しない（純粋に目視用）。
- 判定は `use_pairs.contains(&(ctx, ty))` による集合包含。順序非依存。

このチャンクには分岐が4つ以上の複雑ロジックや状態遷移は存在せず、Mermaid図の要件を満たさないため図示は省略。

## Complexity & Performance

- 時間計算量
  - テスト処理自体は O(m + k) 程度（m=文字列長、k=検出件数）。主たる計算量は **find_uses** に帰属（推定: O(n)、n=コード長）。
- 空間計算量
  - `uses` と `use_pairs` の二重保持により O(k)（k=検出件数）。to_string により文字列の複製が発生。
- ボトルネック
  - find_uses の内部パース（このチャンクからは不明）。テスト側は軽微。
- スケール限界・運用負荷
  - テストは小さなコード片を対象としており、スケール課題は実質なし。運用負荷（I/O/ネットワーク/DB）はなし。

## Edge Cases, Bugs, and Security

エッジケース（このファイルで検証・未検証の整理）

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| コンストラクタ引数の型検出 | `class AuroraCurrencyRepository(private val client: PgClient, ...)` | `("AuroraCurrencyRepository","PgClient")` 等が含まれる | `assert!(use_pairs.contains(&(...)))` | 検証済 |
| 関数引数の型検出 | `fun processUser(user: User, validator: UserValidator)` | `("processUser","User")`, `("processUser","UserValidator")` が含まれる | `assert!(...)` | 検証済 |
| 戻り値型の検出 | `fun processUser(...): Result` / `fun createUser(...): User` | 当該戻り値型が含まれる | `assert!(...)` | 検証済 |
| プリミティブ型の除外 | `String`, `Int` | `String`, `Int` は use_pairs に現れない | `assert!(!use_pairs.iter().any(...))` | 検証済 |
| プロパティ型の検出 | `val cache: CacheManager` 等 | 当該プロパティ名と型のペアが含まれる | `assert!(...)` | 検証済 |
| コレクション/ジェネリクス内包型 | `List<Row>` | `Row` の扱い（抽出/非抽出）は仕様次第 | 該当なし | 不明 |
| 未定義型/外部型 | `UUID`, `Row` | 取り扱い（検出有無）は仕様次第 | 該当なし | 不明 |
| suspend 関数 | `suspend fun updateCurrencyCollections(...)` | suspend による影響なしで型検出 | 該当なし（戻り値未検証） | 不明 |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（安全なRust、unsafe未使用）。
- インジェクション
  - SQL/Command/Path traversal: 該当なし（外部入力なし、I/Oなし）。
- 認証・認可
  - 権限チェック漏れ / セッション固定: 該当なし（テストコードのみ）。
- 秘密情報
  - Hard-coded secrets / Log leakage: 該当なし（テスト用の固定文字列のみ）。
- 並行性
  - Race condition / Deadlock: 該当なし（同期処理のみ）。

Rust特有の観点（詳細チェック）:
- 所有権
  - `uses.iter()` による不変借用。`.to_string()` で新規所有文字列を生成。移動はなし（関数: 各 test_*）。
- 借用
  - ループ内の参照はスコープ内で完結。可変借用なし。
- ライフタイム
  - 明示的ライフタイムは不要。生文字列リテラル（`&'static str`）を `find_uses` に渡す。
- unsafe 境界
  - unsafe 不使用（このチャンクには存在しない）。
- Send/Sync・非同期
  - 非同期なし。マルチスレッド利用なし。`suspend` はKotlin側の記法でありRust側の非同期とは無関係。
- エラー設計
  - `new().expect("...")` により生成失敗で panic。テストとしては妥当だが、失敗詳細の観測は弱い。
  - `Result` / `Option` のどちらが返るかは不明。

## Design & Architecture Suggestions

- 重複処理の共通化
  - `run_find_uses(code: &str) -> Vec<(String, String)>` のようなヘルパーを作り、`use_pairs` の生成を共通化することでテストの簡潔性と一貫性を向上。
- 期待集合の定義
  - 期待する `(context, type)` を `HashSet<(String, String)>` として定義し、包含・差分をまとめて検証するユーティリティを導入。
- 出力整形の統一
  - `println!` はテストのノイズになりがち。必要なら `eprintln!` や `--nocapture` を前提とした限定的ログ、または `#[cfg(test)]` の専用ロガーを使用。
- コンテキスト命名仕様の明確化
  - 関数名、クラス名、プロパティ名で異なる命名規約が混在しうるため、`context` の値仕様（例えば `fn:processUser`, `class:AuroraCurrencyRepository`, `prop:cache` 等）を標準化すると誤判定低減。
- 位置情報の契約
  - `range.start_line` の 1-based/0-based、列情報の有無等をドキュメント化し、将来的に位置依存のテスト（特定行での検出）に拡張可能に。
- 複合型対応の仕様化
  - `List<Row>` の内包型 `Row` を検出するか否か、戻り値型のジェネリクス、Nullable、関数型の扱い等の仕様を決め、テストを追加。

## Testing Strategy (Unit/Integration) with Examples

追加で網羅したい観点と例（擬似テストコード）:

- ジェネリクス内包型
```rust
#[test]
fn test_kotlin_generic_types() {
    let code = r#"
class C {
    fun q(): List<Row> = emptyList()
    fun r(map: Map<String, User>): Int = 0
}
"#;
    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);
    let use_pairs: Vec<(String, String)> = uses.iter()
        .map(|(c, t, _)| (c.to_string(), t.to_string()))
        .collect();

    // 方針次第: Row/User を検出する仕様なら以下を有効化
    // assert!(use_pairs.contains(&("q".into(), "Row".into())));
    // assert!(use_pairs.contains(&("r".into(), "User".into())));
    // String はプリミティブとして除外
    assert!(!use_pairs.iter().any(|(_, t)| t == "String"));
}
```

- Nullable型・関数型
```rust
#[test]
fn test_kotlin_nullable_and_function_types() {
    let code = r#"
class U {
    fun f(cb: (User) -> Result?): Result? = null
}
"#;
    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);
    let use_pairs: Vec<(String, String)> = uses.iter()
        .map(|(c, t, _)| (c.to_string(), t.to_string()))
        .collect();

    // 仕様次第で User, Result を検出対象とする
    // assert!(use_pairs.contains(&("f".into(), "User".into())));
    // assert!(use_pairs.contains(&("f".into(), "Result".into())));
}
```

- プロパティ初期化・注釈
```rust
#[test]
fn test_kotlin_property_with_annotation() {
    let code = r#"
class A {
    @Inject lateinit var logger: Logger
    val cache: CacheManager = DefaultCache()
}
"#;
    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);
    let use_pairs: Vec<(String, String)> = uses.iter()
        .map(|(c, t, _)| (c.to_string(), t.to_string()))
        .collect();

    assert!(use_pairs.contains(&("logger".into(), "Logger".into())));
    assert!(use_pairs.contains(&("cache".into(), "CacheManager".into())));
    // DefaultCache を型使用として扱うかは仕様次第
}
```

- インポート／型エイリアス
```rust
#[test]
fn test_kotlin_type_alias_and_imports() {
    let code = r#"
typealias U = User
class S {
    fun p(u: U): U = u
}
"#;
    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let use_pairs: Vec<(String, String)> = parser.find_uses(code).iter()
        .map(|(c, t, _)| (c.to_string(), t.to_string()))
        .collect();

    // alias 展開の仕様次第
    // assert!(use_pairs.contains(&("p".into(), "User".into())));
}
```

## Refactoring Plan & Best Practices

- ステップ1: 共通ユーティリティ導入
  - `fn to_use_pairs(uses: &[(impl ToString, impl ToString, _)]) -> HashSet<(String, String)>` を導入し、重複コード削減。
- ステップ2: 期待値をセットで管理
  - `expected: HashSet<(String, String)>` と `actual` の差分を一括検証。見落としや余計な検出の検出が容易に。
- ステップ3: ログの抑制
  - テストでは不要な `println!` を削減。必要に応じて `--nocapture` 実行時のみ出すガードを追加。
- ステップ4: ケース追加と分類
  - 機能別（コンストラクタ、関数、プロパティ、ジェネリクス、Nullable、関数型）にモジュールを分割し、読みやすさ向上。
- ステップ5: ドキュメント化
  - `context` の命名仕様、`range` の基準（1-based/0-based）をREADMEやRustdocに反映。

## Observability (Logging, Metrics, Tracing)

- 現状: テスト内で `println!` により検出結果を表示。`cargo test` は標準で出力をキャプチャするため、失敗時にのみ出力が表示される。
- 推奨:
  - 解析ライブラリ側で、必要に応じて `log` クレートを用いた **debug** ログを提供し、テストでは `env_logger` 等を初期化して診断可能に。
  - メトリクス（検出件数、解析時間）を返す仕組みがあると、性能変化の回帰検出に有効（このチャンクには現れないため詳細不明）。
  - `range` の充実（列番号、ファイル名）によりトレーシング容易化。

## Risks & Unknowns

- 返却型の契約が不明
  - `find_uses` の完全なデータ構造（`Context`/`Type`/`Range` の型）が不明。「to_string」が通る前提で記述されているが、将来の型変更にテストが脆弱。
- コンテキスト命名の仕様
  - 関数名・クラス名・プロパティ名の区別方法が不明。仕様が変化するとテストの一致条件が崩れる可能性。
- 複合型・Nullable・関数型
  - `List<Row>` 等の内包型扱いが未定義。期待と実装がずれると誤検出/過検出/検出漏れになりうる。
- 位置情報の粒度
  - `start_line` のみの利用。行番号の基準や列番号未提供だと、精密な位置検証が困難。
- パーサ生成の失敗時挙動
  - `expect` による panic はテストとして許容だが、失敗原因の可観測性が低く、CI診断が難しい場合あり。

以上の点を踏まえ、テストの共通化・仕様ドキュメント化・検証ケースの拡充を行うことで、将来の変更に強い品質保証体制を整えられる。