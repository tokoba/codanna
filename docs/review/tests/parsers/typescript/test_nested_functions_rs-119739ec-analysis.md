# parsers\typescript\test_nested_functions.rs Review

## TL;DR

- 目的: TypeScriptコードからの**ネスト関数の抽出**と、**関数間の呼び出し関係**の検出を、Reactコンポーネントパターンを含めて保証するテスト。
- 主要API（このファイル内）: `test_nested_function_extraction`, `test_nested_function_relationships`（いずれも#[test]）。
- 外部API（利用のみ）: `TypeScriptParser::new`, `TypeScriptParser::parse`, `TypeScriptParser::find_calls`, `FileId::new`, `SymbolCounter::new`。
- 複雑箇所: ネストされた**アロー関数**と**通常の関数宣言**を同時に検出し、正しいシンボル数と名称を確認する点。
- 重大リスク: `unwrap`/`expect`の**panic**可能性、`find_calls`の戻り値の第三要素の意味が**不明**、テストが**文字列一致**に依存し構文バリエーションに弱い可能性。
- 安全性: 本ファイルのコードは**安全なRust**（`unsafe`なし）、並行性なし、メモリ安全の観点で問題なし。
- パフォーマンス: テストの計算量は**パーサの処理量に依存**（概ね入力長に線形）で、I/Oなし・短時間。

## Overview & Purpose

このファイルは、`codanna`クレート内のTypeScriptパーサ（`TypeScriptParser`）に対して、以下を検証するユニットテストです。

- Reactコンポーネントに典型的な「コンポーネント関数の内部で定義されるネスト関数（ハンドラ等）」が、個別のシンボルとして抽出されること。
- 通常の関数宣言におけるネスト関数も抽出されること。
- ネスト関数間の呼び出し関係（親→子、子→子）が`find_calls`で検出されること。

コメントに「Sprint 3でのReactコンポーネント対応のクリティカル修正」の旨があり、その回帰を防ぐためのテストであることが示唆されます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | tests | private (`#[cfg(test)]`) | ユニットテストを集約 | Low |
| Function | test_nested_function_extraction | private (`#[test]`) | シンボル抽出（ネスト関数含む）の正しさ検証 | Low |
| Function | test_nested_function_relationships | private (`#[test]`) | 関数呼び出し関係の検出（ネスト関数含む）の正しさ検証 | Low |

### Dependencies & Interactions

- 内部依存
  - 両テストともに`TypeScriptParser`のインスタンスを生成して、メソッドを呼び出します。
  - テスト間での直接呼び出しや共有状態はありません（完全に独立しています）。

- 外部依存（このファイルで利用する項目）

  | クレート/モジュール | シンボル | 用途 |
  |---------------------|----------|------|
  | codanna::parsing::LanguageParser | importのみ | パーサのトレイト（型境界暗黙利用のためのインポートと推察、未使用） |
  | codanna::parsing::typescript::TypeScriptParser | TypeScriptParser | TypeScriptコードの解析（シンボル抽出、コールグラフ抽出） |
  | codanna::types::FileId | FileId::new | 解析対象ファイルIDの生成（`parse`に渡す） |
  | codanna::types::SymbolCounter | SymbolCounter::new | シンボルカウンタ（`parse`に渡す） |

- 被依存推定
  - このテストモジュールは`cargo test`実行時にのみ参照されます。CIやリリース前検証における**回帰テスト**として機能します。
  - `TypeScriptParser`の仕様変更（特にシンボル抽出とコール検出）に対する**安全網**。

## API Surface (Public/Exported) and Data Contracts

このファイル自体には公開APIはありません（`pub`関数なし）。以下にテスト関数を列挙し、役割と契約を整理します。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| test_nested_function_extraction | fn test_nested_function_extraction() | ネスト関数を含むシンボル抽出の検証 | O(N)（N=入力コード長、パーサ依存） | O(S)（S=抽出シンボル数） |
| test_nested_function_relationships | fn test_nested_function_relationships() | ネスト関数の呼び出し関係検出の検証 | O(N)（find_callsの計算量に依存） | O(C)（C=検出されたコール数） |

Nは入力コードサイズに比例、S/Cは抽出結果サイズに比例。実際の複雑度は`TypeScriptParser`の実装に依存（このチャンクには現れない）。

### test_nested_function_extraction

1. 目的と責務
   - Reactコンポーネント（アロー関数）と通常関数内に定義された**ネスト関数**が、個別のシンボルとして抽出されることを検証。
   - 期待されるシンボル名と総数（5件）をアサート。

2. アルゴリズム（ステップ）
   - 入力コード文字列を定義（Reactコンポーネントと通常関数を含む）。
   - `TypeScriptParser::new().expect(...)`でパーサを生成。
   - `FileId::new(1).unwrap()`でファイルIDを生成。
   - `SymbolCounter::new()`を生成。
   - `parser.parse(code, file_id, &mut counter)`でシンボル一覧を取得。
   - `symbols.iter().map(|s| s.name.as_ref()).collect()`で名前一覧を抽出。
   - 件数が5であること、各名称（"Component", "handleClick", "toggleTheme", "outer", "inner"）が含まれることをアサート。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| なし | なし | テスト関数のため引数なし。内部でコード文字列等を生成。 |

4. 戻り値

| 型 | 説明 |
|----|------|
| なし | `#[test]`関数。アサーション失敗時にpanic。 |

5. 使用例

```rust
#[test]
fn test_nested_function_extraction() {
    let code = r#"
const Component = () => {
    const handleClick = () => {
        console.log('clicked');
        toggleTheme();
    };
    const toggleTheme = () => { console.log('theme'); };
    return { handleClick, toggleTheme };
};
function outer() { function inner() { console.log('inner'); } inner(); }
"#;

    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let file_id = FileId::new(1).unwrap();
    let mut counter = SymbolCounter::new();
    let symbols = parser.parse(code, file_id, &mut counter);

    let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_ref()).collect();
    assert_eq!(symbols.len(), 5);
    assert!(symbol_names.contains(&"Component"));
    assert!(symbol_names.contains(&"handleClick"));
    assert!(symbol_names.contains(&"toggleTheme"));
    assert!(symbol_names.contains(&"outer"));
    assert!(symbol_names.contains(&"inner"));
}
```

6. エッジケース
- 空入力の場合、シンボル数は0であるべき（このチャンクには現れない）。
- 同名関数が複数スコープに存在する場合の重複扱い（不明）。
- `return`でエクスポートされない内部関数の扱い（このテストは抽出のみを確認）。
- TypeScript特有構文（`export default`, `class`, `namespace`）は未検証。

### test_nested_function_relationships

1. 目的と責務
   - 親関数から子関数への呼び出し、および子関数から別の子関数への呼び出しが`find_calls`で検出されることを検証。

2. アルゴリズム（ステップ）
   - 入力コード文字列（`App`内の`doWork`が`helperFunction`を呼び出し、`App`が`doWork`を呼ぶ）を定義。
   - `TypeScriptParser`を生成。
   - `parser.find_calls(code)`で呼び出しタプル一覧（`(caller, callee, _)`）を取得。
   - `any`で`"doWork" -> "helperFunction"`の存在を確認。
   - `any`で`"App" -> "doWork"`の存在を確認。
   - 両者をアサート。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| なし | なし | テスト関数。内部でコード文字列を生成。 |

4. 戻り値

| 型 | 説明 |
|----|------|
| なし | `#[test]`関数。アサーション失敗時にpanic。 |

5. 使用例

```rust
#[test]
fn test_nested_function_relationships() {
    let code = r#"
const App = () => {
    const doWork = () => { helperFunction(); };
    const helperFunction = () => { console.log('helping'); };
    doWork();
};
"#;

    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let calls = parser.find_calls(code);

    let has_nested_call = calls.iter()
        .any(|(caller, callee, _)| *caller == "doWork" && *callee == "helperFunction");
    assert!(has_nested_call, "Should track doWork -> helperFunction call");

    let has_parent_call = calls.iter()
        .any(|(caller, callee, _)| *caller == "App" && *callee == "doWork");
    assert!(has_parent_call, "Should track App -> doWork call");
}
```

6. エッジケース
- 複数呼び出しが連鎖する場合の順序性（不明）。
- 同名関数が別スコープで存在する場合の曖昧性解消（不明）。
- 動的呼び出し（`obj[methodName]()`や`(cond ? a : b)()`）の扱い（不明）。

## Walkthrough & Data Flow

- test_nested_function_extraction
  - 入力: TypeScriptコード文字列。
  - 生成: `TypeScriptParser`インスタンス、`FileId(1)`, `SymbolCounter`。
  - 処理: `parse(code, file_id, &mut counter)`を呼び、`Vec<Symbol>`（`Symbol`には`name: String`フィールドがあることが暗黙に示唆）を受領。
  - 射影: `symbols.iter().map(|s| s.name.as_ref())`で`Vec<&str>`に射影。
  - 検証: 件数と名称セットをアサート。

- test_nested_function_relationships
  - 入力: TypeScriptコード文字列。
  - 生成: `TypeScriptParser`インスタンス。
  - 処理: `find_calls(code)`で呼び出し一覧`Vec<(caller, callee, meta)>`を受領（`meta`の型・意味は不明）。
  - 検索: `iter().any`で目的の呼び出しペアが存在するかをチェック。
  - 検証: 親→子、子→子の両呼び出しの存在をアサート。

このチャンクの処理は直線的で分岐も少ないため、Mermaid図は不要。

## Complexity & Performance

- 時間計算量
  - `parse`: 一般的な字句解析＋構文解析＋シンボル抽出で概ねO(N)（N=コード長）。具体は`TypeScriptParser`実装に依存（不明）。
  - `find_calls`: トークン走査またはAST走査でO(N)〜O(N+E)（E=エッジ数）。詳細不明。
  - テスト関数自体はアサーションと簡易フィルタのみでO(S)またはO(C)。

- 空間計算量
  - `symbols`: O(S)（S=抽出シンボル数）。
  - `calls`: O(C)（C=検出コール数）。

- ボトルネック
  - `parse`/`find_calls`の内部実装。
  - 大規模コードの場合、`symbols`や`calls`ベクトルのメモリ消費。
  - 同一コード文字列に対してASTを共有せずに別解析を行う場合、コストが倍増する可能性。

- 実運用負荷
  - 本ファイルはテストのみのため運用負荷なし。ネットワーク・DB・ファイルI/Oは無し。

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | `""` | シンボル0、コール0 | このチャンクには現れない | 不明 |
| 同名関数の多重定義 | 同スコープで`const f = ...; function f(){}` | 識別の一貫性（上書き/警告/両方抽出のポリシー） | このチャンクには現れない | 不明 |
| 動的呼び出し | `obj[fnName]()` | 静的解析で検出不可の場合は非検出 | このチャンクには現れない | 不明 |
| クラスメソッド | `class C { m(){ n(); } n(){} }` | メソッド間コール検出 | このチャンクには現れない | 不明 |
| ネスト深度が深い | 5階層以上の入れ子 | 正しく全シンボル抽出・コール検出 | このチャンクには現れない | 不明 |

- セキュリティチェックリスト
  - メモリ安全性: 
    - Buffer overflow / Use-after-free / Integer overflow: 本ファイルは安全なRustのみを使用。該当なし。
    - 所有権/借用: `symbols.iter()`で不変借用、`s.name.as_ref()`で`&str`を取得。ライフタイムは`symbols`に依存（問題なし）。
  - インジェクション: SQL/Command/Path traversalの入力・I/Oなし。該当なし。
  - 認証・認可: 該当なし（テストコード）。
  - 秘密情報: ハードコードされた秘密情報なし。ログ出力もなし。
  - 並行性: レースコンディション/デッドロックの懸念なし（同期テスト）。
  
- Rust特有の観点（詳細チェック）
  - 所有権: `let mut parser = TypeScriptParser::new().expect(...)`で所有、`parse`/`find_calls`呼び出し時に可変参照の利用が示唆（正当）。
  - 借用: `symbols.iter()`は不変借用のみ。可変借用の競合なし。
  - ライフタイム: 明示的ライフタイムパラメータは不要。`&str`は`String`のスコープ内に限定され安全。
  - unsafe境界: unsafeブロックは存在しない（このチャンクには現れない）。
  - 並行性・非同期: Send/Syncの境界や`async`/`await`は不使用。
  - エラー設計: 
    - `expect("Failed to create parser")`と`unwrap()`を使用。テストでは妥当だがpanicの可能性あり。
    - `FileId::new(1).unwrap()`はID生成に失敗した場合panic。テスト環境では意図的（失敗時に早期検知）。

重要な主張の行番号は、このチャンクでは不明（テスト関数内に`unwrap`/`expect`が存在）。

## Design & Architecture Suggestions

- パーサ内のAST構築を共有化
  - `parse`と`find_calls`で同じ入力を別々に解析している場合、ASTを共有して再利用する設計にすると計算量・メモリ使用を低減可能（内部実装は不明、方針提案）。

- テスト補助ヘルパの導入
  - シンボル名抽出やコール存在確認の定型処理をヘルパ関数化して重複を削減し、可読性を向上。

- ケース拡張の体系化
  - React特有のパターン（hooks, useCallback, useMemo内部の関数）やTypeScriptの多様な構文（`export`, `class`, `interface`, `namespace`, `enum`）を網羅するテーブル駆動テストを検討。

- コントラクトの明文化
  - `find_calls`の戻り値タプル第三要素の意味（位置情報/スコープ情報等）を型で明確化すると、テスト記述の精度が上がる。

## Testing Strategy (Unit/Integration) with Examples

- テーブル駆動（複数ケースを一括検証）

```rust
fn assert_symbols(code: &str, expected: &[&str], expected_len: usize) {
    let mut parser = TypeScriptParser::new().expect("parser");
    let file_id = FileId::new(1).unwrap();
    let mut counter = SymbolCounter::new();
    let symbols = parser.parse(code, file_id, &mut counter);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_ref()).collect();
    assert_eq!(symbols.len(), expected_len, "len mismatch: {:?}", names);
    for &name in expected {
        assert!(names.contains(&name), "missing: {}", name);
    }
}

#[test]
fn test_various_ts_constructs() {
    // 空入力
    assert_symbols("", &[], 0);

    // クラスメソッド（期待は不明。仕様に応じて更新）
    let code = r#"class C { m(){ n(); } n(){} }"#;
    // 仕様確定後に expected を記述
    // assert_symbols(code, &["C", "m", "n"], 3);
}
```

- コールグラフ検証の拡張
  - 同スコープ同名関数、ネスト深度、クラスメソッドに対する呼び出し検出のテストを追加。
  - 第三要素（メタデータ）が位置情報なら、行・列番号に基づくアサートを導入。

```rust
fn has_call(calls: &[(String, String, /* meta */)], caller: &str, callee: &str) -> bool {
    calls.iter().any(|(c, d, _)| c == caller && d == callee)
}

#[test]
fn test_multiple_calls() {
    let code = r#"
function A(){ B(); C(); }
function B(){ }
function C(){ }
A();
"#;
    let mut parser = TypeScriptParser::new().expect("parser");
    let calls = parser.find_calls(code);
    assert!(has_call(&calls, "A", "B"));
    assert!(has_call(&calls, "A", "C"));
}
```

## Refactoring Plan & Best Practices

- ヘルパ導入
  - シンボル抽出・コール検出のアサートを共通化する関数を作り、重複コードを削減。

- 明確な失敗メッセージ
  - 既に詳細メッセージを含む`assert!`があるが、差分表示を強化（例: 期待セットと実際セットの差分出力）。

- 入力コードの最小化
  - テストケースのコードは必要最小限にし、不要なログやコメントを削減して意図が明確になるよう調整。

- `unwrap/expect`のルール化
  - テストではOKだが、ライブラリコードでは`Result`の伝播やカスタムエラー型への変換を推奨（このファイルはテストのため現状維持で可）。

## Observability (Logging, Metrics, Tracing)

- ログ
  - パーサ内部のデバッグログがあるなら、テストで一時的に有効化し、解析の失敗時に原因追跡を容易にする。
  - テスト側は基本ログ不要だが、失敗メッセージに期待値・実測値を具体的に出すのは有用。

- メトリクス/トレース
  - テストのため不要。パーサ側で解析時間を計測するHooksがあるならベンチで活用（このチャンクには現れない）。

## Risks & Unknowns

- Unknowns
  - `TypeScriptParser::parse`/`find_calls`の厳密なアルゴリズム・計算量・戻り値の型詳細（このチャンクには現れない）。
  - `Symbol`構造体の完全なフィールド一覧・意味（このチャンクには現れない）。
  - `calls`の第三要素（メタデータ）の型と意味（不明）。

- Risks
  - 文字列一致による検証の脆弱性（エイリアスやスコープ修飾に弱い）。
  - パーサの仕様変更時にテストが過不足なく追随しない可能性（期待値の更新が必要）。
  - `FileId::new(1).unwrap()`などのpanicはテストでは妥当だが、将来的にユーティリティ化して再利用すると本番コードに混入するリスク。