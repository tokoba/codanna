# parsers\c\test_resolution.rs Review

## TL;DR

- 目的: Cの包括的コード例を実際にパースし、抽出されたシンボルを**解決コンテキスト**へ投入して、名前解決が正しく機能するかを統合テストで検証。
- 主要利用API: **CParser::new/parse**, **CBehavior::new/create_resolution_context**, **ResolutionContext::add_symbol/resolve**, **SymbolCounter::new**。
- 複雑箇所: 複数のテストケース（関数/構造体/存在しないシンボル/総合検証）に跨る分岐と結果検証ロジック。
- 重大リスク: `unwrap`使用によるテストのパニック可能性、外部ファイル読み込み（パス/文字コード）の不確実性、重複名やスコープの競合ケースが未検証。
- パフォーマンス: 大半はパーサの計算量に支配される（O(n)想定）。解決はハッシュマップならO(1)期待だが実装詳細は*不明*。
- セキュリティ・安全性: `unsafe`なしでメモリ安全。I/Oに依存するため例外系処理の明示的検証は不足。ログに秘匿情報は含まれない。
- 改善提案: テスト補助関数による重複削減、失敗時の詳細診断、重複シンボル/スコープ/オーバーロードのテスト追加、`expect`/`unwrap`の削減。

## Overview & Purpose

このファイルは、C言語パーサと名前解決ロジック（Resolution Context）の実装が現実的なCコードに対して期待通り動作するかを確認する**統合テスト**および**基本的ユニットテスト**を提供します。

- 統合テスト `test_c_resolution_with_real_code`: 実ファイル `examples/c/comprehensive.c` を読み取り、`CParser`でシンボルを抽出し、`CBehavior`が作る解決コンテキストに登録して様々な名前を解決します。
- ユニットテスト `test_c_resolution_context_basic`: 最小限のシンボルを手動で登録して、期待通り解決されるかをチェックします。

テストは結果を**標準出力**へ詳細にログ出力して可視化し、最後に`assert`で合否判定します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_c_resolution_with_real_code | crate内テスト | 実Cコードのパース、シンボル抽出、解決コンテキストへの投入、複数テストケース検証 | Med |
| Function | test_c_resolution_context_basic | crate内テスト | 基本的な解決コンテキストの作成と単純な解決検証 | Low |

- 外部型/関数（このファイル外のため参考記載）
  - CParser（`codanna::parsing::c::parser::CParser`）
  - CBehavior（`codanna::parsing::c::behavior::CBehavior`）
  - SymbolCounter（`codanna::types::SymbolCounter`）
  - FileId（`codanna::FileId`）
  - SymbolKind（`codanna::SymbolKind`）
  - ScopeLevel（`codanna::parsing::resolution::ScopeLevel`）
  - SymbolId（`codanna::SymbolId`）

### Dependencies & Interactions

- 内部依存
  - `test_c_resolution_with_real_code` → `CParser::parse`で`symbols`取得 → `CBehavior::create_resolution_context`で`context`作成 → `context.add_symbol`で一括登録 → `context.resolve`で複数ケースを検証。
  - `test_c_resolution_context_basic` → `CBehavior::create_resolution_context` → `context.add_symbol` → `context.resolve`。

- 外部依存（このファイル内で使用）

| 依存 | 役割 | 備考 |
|-----|------|------|
| std::fs::read_to_string | Cコードの読み込み | 外部ファイルI/O |
| CParser::new/parse | Cコードのパース | シンボル抽出 |
| CBehavior::new/create_resolution_context | 解決コンテキストの生成 | 言語固有の挙動 |
| SymbolCounter::new | シンボル採番/重複管理？ | 詳細は不明（使用から推測） |
| ResolutionContext::add_symbol/resolve | シンボル登録/名前解決 | スコープレベルはModule |
| FileId, SymbolKind, SymbolId, ScopeLevel | 型 | データ契約の一部 |

- 被依存推定
  - このテストモジュールは**テストハーネス**によって実行され、プロジェクトのC言語解析・解決機能の回帰検証に用いられます。他モジュールから直接参照されることは*不明/該当なし*。

## API Surface (Public/Exported) and Data Contracts

公開API（このファイルから直接エクスポートされるもの）はありません。以下はこのファイルが利用・検証している主要APIの一覧です（外部APIのため詳細シグネチャは一部*不明*）。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| test_c_resolution_with_real_code | `fn()` | 統合テスト実行 | O(n + m) | O(n + m) |
| test_c_resolution_context_basic | `fn()` | 基本的ユニットテスト | O(1〜m) | O(m) |
| CParser::new | `fn() -> Result<CParser, _>` | パーサ生成 | O(1) | O(1) |
| CParser::parse | `fn(&self, &str, FileId, &mut SymbolCounter) -> Vec<Symbol>`（推定） | シンボル抽出 | O(n) | O(m) |
| CBehavior::new | `fn() -> CBehavior` | 言語挙動の提供 | O(1) | O(1) |
| CBehavior::create_resolution_context | `fn(file_id: FileId) -> ResolutionContext`（推定） | 解決コンテキスト生成 | O(1) | O(1) |
| ResolutionContext::add_symbol | `fn(String, SymbolId, ScopeLevel)`（推定） | シンボル登録 | O(1)（推定） | O(1) |
| ResolutionContext::resolve | `fn(&self, &str) -> Option<SymbolId>`（使用から確定） | 名前解決 | O(1〜log m)（推定） | O(1) |

- データ契約（使用から読み取れるフィールド）
  - `Symbol`（推定）：`name: String`, `kind: SymbolKind`, `id: SymbolId`, `range.start_line: usize`
  - `SymbolKind`: 列挙型。少なくとも`Function`, `Struct`, `TypeAlias`が存在。
  - `ScopeLevel`: 少なくとも`Module`を使用。

以下、テスト関数の詳細（内部APIだが本ファイルのコアロジック）:

1) test_c_resolution_with_real_code
- 目的と責務
  - 実コードから抽出したシンボルを解決コンテキストへ登録し、複数ケースの名前解決を検証する。
- アルゴリズム（ステップ分解）
  1. `read_to_string`でCコード読込。
  2. `CParser::new`でパーサ生成。
  3. `SymbolCounter::new`でカウンタ生成。
  4. `parse`で`symbols: Vec<Symbol>`抽出。
  5. `CBehavior::create_resolution_context`で`context`生成。
  6. `symbols`を`context.add_symbol`で全登録。
  7. ケース別に`context.resolve`実行：
     - `"add"`: `Function`であることを検証。
     - `"Point"`: `Struct`または`TypeAlias`であることを検証。
     - `"main"`: `Function`であることを検証。
     - `"unknown_function_xyz"`: 解決不可であることを検証。
     - 全シンボルについて`resolve(name)`が同一`id`を返すか総合検証。
  8. 合格要件: `resolved_count > 0`かつ`total_count > 0`。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | なし | なし | テスト関数 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | なし | パニック/アサートで合否 |
- 使用例
  ```rust
  // 抜粋: "add"の解決検証
  let add_resolved = context.resolve("add");
  if let Some(symbol_id) = add_resolved {
      let add_symbol = symbols.iter().find(|s| s.id == symbol_id).unwrap();
      assert_eq!(&*add_symbol.name, "add");
      assert_eq!(add_symbol.kind, SymbolKind::Function);
  }
  ```
- エッジケース
  - ファイル未読込（パス誤り・非UTF-8）
  - シンボル重複（同名異種）
  - スコープ衝突（同名、別スコープ）
  - 予約語/マクロ由来シンボル
  - 極端に大量のシンボル

2) test_c_resolution_context_basic
- 目的と責務
  - 解決コンテキストの基本動作（登録・解決・未登録の不可）確認。
- アルゴリズム
  1. `CBehavior`から`context`生成。
  2. 任意の`symbol_id`を`add_symbol`で登録。
  3. `resolve`が同一`symbol_id`を返すことを検証。
  4. 未登録名は`None`であることを検証。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | なし | なし | テスト関数 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | なし | パニック/アサートで合否 |
- 使用例
  ```rust
  let mut context = behavior.create_resolution_context(FileId(1));
  let symbol_id = codanna::SymbolId(100);
  context.add_symbol("test_func".to_string(), symbol_id, codanna::parsing::resolution::ScopeLevel::Module);
  assert_eq!(context.resolve("test_func").unwrap(), symbol_id);
  assert!(context.resolve("unknown_func").is_none());
  ```

## Walkthrough & Data Flow

- 入力: `examples/c/comprehensive.c`の内容（文字列）
- 出力: 標準出力へのレポート、アサート合否

データフロー（高レベル）
1. ファイルI/OでCコード文字列取得。
2. `CParser::parse`が文字列と`FileId`、`SymbolCounter`から`Vec<Symbol>`を生成。
3. `CBehavior::create_resolution_context`がファイル単位の解決コンテキストを生成。
4. 各`Symbol`（`name`, `id`, `kind`, `range.start_line`）を`Module`スコープで登録。
5. `resolve(name)`が`Option<SymbolId>`を返し、元の`symbols`から合致するか確認。
6. 集計（`resolved_count` vs `total_count`）を行い合否判定。

Mermaidフローチャート（条件分岐多数のため使用）。上記の図は`test_c_resolution_with_real_code`関数の主要分岐を示す（このチャンクには行番号情報がないため関数全体に対応）。

```mermaid
flowchart TD
  A[Start test_c_resolution_with_real_code] --> B[Read C file]
  B -->|Ok| C[CParser::new + SymbolCounter::new]
  B -->|Err| Z[panic: Failed to read comprehensive.c]
  C --> D[parser.parse -> symbols]
  D --> E[create_resolution_context]
  E --> F[add all symbols to context]
  F --> G1[Resolve 'add']
  G1 -->|Some| H1[assert name=='add' && kind==Function]
  G1 -->|None| I1[log: list found functions]
  F --> G2[Resolve 'Point']
  G2 -->|Some| H2[assert name=='Point' && kind in {Struct,TypeAlias}]
  G2 -->|None| I2[log: list structs/types]
  F --> G3[Resolve 'main']
  G3 -->|Some| H3[assert name=='main' && kind==Function]
  G3 -->|None| I3[log: not resolved]
  F --> G4[Resolve 'unknown_function_xyz']
  G4 -->|None| H4[log: correct not resolved]
  G4 -->|Some| Z4[panic: should not resolve]
  F --> G5[Iterate all symbols]
  G5 -->|match id| H5[log: correctly resolved]
  G5 -->|mismatch| I5[log: wrong id]
  G5 -->|None| J5[log: failed to resolve]
  H1 --> K[Summary + Asserts]
  H2 --> K
  H3 --> K
  H4 --> K
  H5 --> K
  I1 --> K
  I2 --> K
  I3 --> K
  I5 --> K
  J5 --> K
  K --> L[End]
```

## Complexity & Performance

- 時間計算量
  - ファイル読み込み: O(n)（nはファイルサイズ）
  - パース: O(n)（字句・構文解析の一般的想定。厳密な計算量は*不明*）
  - シンボル登録: O(m)（mはシンボル数、1件あたりO(1)想定）
  - 解決: O(1)〜O(log m)（内部構造がハッシュかツリーか*不明*）
  - 総計: O(n + m)
- 空間計算量
  - パース結果`Vec<Symbol>`: O(m)
  - 解決コンテキスト：O(m)
  - 総計: O(n + m)（文字列とシンボル保持）
- ボトルネック/スケール限界
  - 大規模Cソースではパーサの性能・メモリ消費が支配的。
  - 出力ログが大量になり、テストの可読性と速度低下の可能性。
- 実運用負荷要因
  - I/O（ファイルサイズ、ストレージ速度）
  - パーサの内部アルゴリズム（*このチャンクには現れない*）
  - 名前解決の内部構造（*不明*）

## Edge Cases, Bugs, and Security

- 機能的エッジケースと現行実装の挙動

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ファイル未存在/非UTF-8 | パス誤りやバイナリ | 明示的に失敗としてレポート | `expect`で即panic | 改善余地 |
| シンボル重複（同名） | `add`が複数定義 | スコープ/優先度に応じて一意解決 | 未検証 | 不明 |
| スコープ差異 | 同名が異なるスコープ | 適切なスコープで解決 | 全てModuleで登録 | リスク |
| マクロ生成シンボル | `#define`等 | パーサ仕様に準拠 | 未検証 | 不明 |
| 極端な行数 | 非常に長いファイル | 性能劣化なく完了 | ログ過多の懸念 | 改善余地 |
| 非関数`Point`種別 | `typedef struct`等 | Struct/TypeAliasとして識別 | 検証あり | OK |
| 未定義名の解決 | `unknown_function_xyz` | Noneを返す | panicで失敗扱い | OK |

- セキュリティチェックリスト
  - メモリ安全性: `unsafe`ブロックは*なし*。`unwrap`によるパニック以外の未定義動作は*不明/該当なし*。
    - `symbols.iter().find(...).unwrap()`（関数名: `test_c_resolution_with_real_code`、行番号: *不明*）は解決失敗時パニックの可能性。
    - `resolved.unwrap()`（関数名: `test_c_resolution_context_basic`、行番号: *不明*）は不正登録時にパニック。
  - インジェクション: SQL/Commandは*該当なし*。ファイルパスは固定文字列で入力由来ではなく、パス・トラバーサルの懸念は低い。
  - 認証・認可: *該当なし*（テストコード）。
  - 秘密情報: ハードコード秘密や漏洩ログは*なし*。
  - 並行性: 共有可変状態なし。Race/Deadlockは*該当なし*。

## Design & Architecture Suggestions

- シンボル登録スコープ
  - 現在は全て`ScopeLevel::Module`で登録。Cの**ブロックスコープ**や**ファイルスコープ/グローバル**、**ヘッダ由来**の区別をテストに追加し、解決の階層性を確認することを推奨。
- 失敗時挙動
  - `expect`/`unwrap`による即パニックはテストでは許容されるが、**何が原因で失敗したか**を詳細に出力する補助関数で改善可能。
- テスト設計
  - 統合テストに加えて、**重複名**、**オーバーロード風（Cでは別引数型だが名前は同じ）**、**影響範囲の異なる宣言**などのケースを単体テスト化。
- ロギング
  - 標準出力ではなく`log`クレートを利用し、`RUST_LOG`で制御可能にすることで、CI時のノイズを低減。

## Testing Strategy (Unit/Integration) with Examples

- 追加ユニットテスト案
  - 重複名の上書き/拒否挙動
  - スコープテスト（ローカル変数とグローバル関数が同名の場合の解決）
  - 異種同名（`struct`と`function`が同名）解決の優先順位

- 例: 補助関数で重複解決の可否を検証
```rust
fn assert_resolve_eq(context: &mut ResolutionContext, symbols: &[Symbol], name: &str, expected_kind: SymbolKind) {
    if let Some(id) = context.resolve(name) {
        let sym = symbols.iter().find(|s| s.id == id).expect("resolved id not found in symbols");
        assert_eq!(sym.kind, expected_kind, "kind mismatch for {}", name);
    } else {
        panic!("symbol '{}' was not resolved", name);
    }
}
```

- 統合テストの安定性向上
  - `comprehensive.c`の内容が変更されてもテストが壊れないよう、**期待シンボルの最小セット**と**柔軟なアサート（kindの集合を許容）**を保持。

## Refactoring Plan & Best Practices

- 重複コードの削減
  - `"add"`, `"Point"`, `"main"`などの解決と検証を**共通ヘルパー**に抽出。
```rust
fn check_resolve(symbols: &[Symbol], context: &ResolutionContext, name: &str, expected_kinds: &[SymbolKind]) {
    match context.resolve(name) {
        Some(id) => {
            let sym = symbols.iter().find(|s| s.id == id).unwrap();
            assert!(expected_kinds.iter().any(|k| *k == sym.kind), "kind mismatch for {}", name);
        }
        None => panic!("symbol '{}' not resolved", name),
    }
}
```

- `unwrap`の削減
  - `find(...).ok_or_else(|| ...)`で失敗時に**詳細メッセージ**を付与。
- 明確な失敗理由出力
  - アサート前に、**名前**, **期待種別**, **取得id**, **行番号**を含む診断を出す。
- テストデータの固定化
  - 極力小さな**fixture**を併用し、`comprehensive.c`の変更影響を限定。

## Observability (Logging, Metrics, Tracing)

- ログ
  - `println!`は簡易だが、`log`クレート + `env_logger`に置換し、レベルを`info`/`debug`で切替可能に。
- メトリクス
  - 解析時間、抽出シンボル数、解決成功率をカウンタとして出力すると**回帰検知**に有効。
- トレーシング
  - テストでは過剰だが、複雑な解決順序を追跡したい場合は`tracing`でスパンを導入。

## Risks & Unknowns

- パーサ実装の詳細
  - `CParser::parse`のトークナイザ/パーサ戦略、`SymbolCounter`の役割詳細は*このチャンクには現れない*。
- 解決コンテキストの内部構造
  - ハッシュ/ツリー/スコープ階層の仕組みは*不明*。計算量も推定に留まる。
- 外部ファイル依存
  - `examples/c/comprehensive.c`の変更でテストが不安定化するリスク。CI環境でのファイルパス/改行コード/文字コード差異にも注意。
- エラー設計
  - `expect`/`unwrap`由来の即パニックにより、**根因追跡が困難**なケースあり。改善余地。

---

Rust特有の観点（詳細チェックリスト）

- メモリ安全性
  - 所有権/借用: `parse(&c_code, file_id, &mut symbol_counter)`で`c_code`は共有参照、`symbol_counter`は可変借用。テストスコープ内でライフタイムが完結し、破壊的な再借用はなし。
  - ライフタイム: 明示的パラメータは不要。`symbols`は`Vec`の所有権を関数内で保持。
- unsafe境界
  - 使用箇所: *なし*。
- 並行性・非同期
  - `Send/Sync`: テストは単一スレッドで実行される前提。共有可変状態なし。
  - await境界/キャンセル: *該当なし*。
- エラー設計
  - Result vs Option: `resolve`は`Option<SymbolId>`で、存在しないシンボルを自然に表現。
  - panic箇所: `expect`と`unwrap`多数（関数名: `test_c_resolution_with_real_code`, `test_c_resolution_context_basic`、行番号: *不明*）。テストでは許容されるが、診断情報の拡充を推奨。
  - エラー変換: *該当なし*（テストコード）。

テストコード引用（重要部分のみ抜粋）

```rust
// Cコード読み込みとパース
let c_code = std::fs::read_to_string("examples/c/comprehensive.c")
    .expect("Failed to read comprehensive.c example");
let mut parser = CParser::new().expect("Failed to create CParser");
let behavior = codanna::parsing::c::behavior::CBehavior::new();
let file_id = FileId(1);
let mut symbol_counter = SymbolCounter::new();
let symbols = parser.parse(&c_code, file_id, &mut symbol_counter);

/* ... 省略 ... */

// 解決コンテキストへ登録
let mut context = behavior.create_resolution_context(file_id);
for symbol in &symbols {
    context.add_symbol(
        symbol.name.to_string(),
        symbol.id,
        codanna::parsing::resolution::ScopeLevel::Module,
    );
}

/* ... 省略 ... */

// 解決の総合検証
let mut resolved_count = 0;
let mut total_count = 0;
for symbol in &symbols {
    total_count += 1;
    if let Some(resolved_id) = context.resolve(&symbol.name) {
        resolved_count += 1;
        if resolved_id == symbol.id {
            println!("✅ CORRECTLY RESOLVED: {} ({:?})", symbol.name, symbol.kind);
        } else {
            println!("⚠️  RESOLVED TO WRONG ID: {} (expected {:?}, got {:?})",
                     symbol.name, symbol.id, resolved_id);
        }
    } else {
        println!("❌ FAILED TO RESOLVE: {} ({:?})", symbol.name, symbol.kind);
    }
}
```