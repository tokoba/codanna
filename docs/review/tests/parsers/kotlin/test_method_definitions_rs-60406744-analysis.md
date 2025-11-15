# test_method_definitions.rs Review

## TL;DR

- 目的: Kotlinのクラス・オブジェクト・入れ子クラス内のメソッド定義を、KotlinParser.find_definesで検出できるかを検証するユニットテスト。
- 公開API: 本ファイル自身の公開APIはなし。間接的に使用するのは**KotlinParser::new**と**LanguageParser::find_defines**（このチャンクには定義が現れない）。
- コアロジック: コード文字列をパース→(definer, method, range)のタプル列を収集→(definer, method)のペア集合でアサート。
- 複雑箇所: 特になし。検出結果の順序非依存チェックは良いが、余分な検出や行番号の検証は未実施。
- 重大リスク: 正確性の担保が限定的（件数・範囲・行番号未検証、重複検出未検出）。テスト対象のカバレッジが狭い（拡張関数/トップレベル/コンパニオン/オーバーロード等が未検証）。
- Rust安全性: unsafeなし、借用は不変参照、所有権/ライフタイム問題なし。テストでexpectを使用しパニック許容は妥当。並行実行における共有可変状態なし。
- 追加提案: 期待セットの完全一致検証、行番号やソース範囲の検証、ケース追加（拡張関数・トップレベル関数・コンパニオン等）、ヘルパー関数で重複コード削減。

## Overview & Purpose

このファイルは、Kotlinコードからメソッド定義を抽出する**KotlinParser**（trait: **LanguageParser**）の機能をテストするRustのユニットテスト集です。3つのテストで以下を網羅しています。

- クラス内メソッドの検出（public/private混在）
- object（シングルトン）内メソッドの検出
- 入れ子クラス（Nested class）内メソッドの検出

各テストは、Kotlinコード文字列を用意し、`KotlinParser::new()`（L22, L66, L114）と`find_defines`（L23, L67, L115）を呼び出して、得られた定義群から(定義主体, メソッド名)ペアを抽出して期待要素の存在を`assert!`で確認します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_kotlin_class_method_definitions | crate内テスト | クラス内メソッド定義の検出を検証 | Low |
| Function | test_kotlin_object_method_definitions | crate内テスト | object内メソッド定義の検出を検証 | Low |
| Function | test_kotlin_nested_class_method_definitions | crate内テスト | 入れ子クラス内メソッド定義の検出を検証 | Low |
| 外部型/トレイト（使用） | KotlinParser | 不明（このチャンクには現れない） | Kotlinコード解析器の生成と解析 | 不明 |
| 外部トレイト（使用） | LanguageParser | 不明（このチャンクには現れない） | find_definesの提供（パースエントリポイント） | 不明 |

### Dependencies & Interactions

- 内部依存
  - 各テスト関数は共通のパターンで動作:
    1. Kotlinのサンプルコード文字列を作成（L6-L20, L54-L64, L100-L112）
    2. `KotlinParser::new().expect(...)`でパーサを生成（L22, L66, L114）
    3. `parser.find_defines(code)`で定義を抽出（L23, L67, L115）
    4. 検出結果を出力（`println!`、L25-L31, L69-L75, L117-L123）
    5. `(definer, method)`に写像してベクタ化（L33-L36, L77-L80, L125-L128）
    6. `assert!(...contains(...))`で所望の組が含まれるか検証（L38-L49, L82-L95, L130-L137）

- 外部依存（クレート/モジュール）
  | 依存 | 要素 | 用途 |
  |------|------|------|
  | codanna::parsing | trait LanguageParser | `find_defines`を呼ぶためのトレイト境界 |
  | codanna::parsing::kotlin | struct KotlinParser | Kotlinパーサのインスタンス生成 |

- 被依存推定
  - Kotlin言語のパーステストスイートの一部として、CIでの回帰テストに使用される可能性が高い。
  - 将来的に他のKotlinコード認識機能（クラス定義、トップレベル関数、拡張関数等）のテストからも参照されるヘルパーパターンの素地。

## API Surface (Public/Exported) and Data Contracts

- 本ファイル自体の公開API: 該当なし（全て`#[test]`関数で外部公開はない）。

間接的に使用しているAPI（このチャンクには定義が現れないため仕様は不明、使用箇所のみ記載）:

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| KotlinParser::new | 不明（期待: Resultを返すファクトリ） | パーサの生成（L22, L66, L114） | 不明 | 不明 |
| LanguageParser::find_defines | 不明（引数: &str想定） | Kotlinコードから定義を抽出（L23, L67, L115） | 不明 | 不明 |

データコントラクト（このファイルから観測できる範囲）:
- `find_defines(code)`の戻り値`defines`は、反復で`(definer, method, range)`のタプルを返すコレクション（L26, L70, L118）。
- `definer`と`method`は`.to_string()`できる（L35, L79, L127）。
- `range`は`.start_line`フィールドを持つ（L29, L73, L121）。
- 戻り順序は規定されていない（本テストは順序に依存せず包含で検証）。

## Walkthrough & Data Flow

各テストの共通フロー（例: test_kotlin_class_method_definitions, L5-L50）:

1. Kotlinコードを生文字列リテラルで定義（L6-L20）
2. `KotlinParser::new().expect("...")`でパーサ生成（L22）
   - 失敗時はテストをpanicで中断
3. `find_defines(code)`で定義抽出（L23）
4. デバッグ出力（件数と各定義の開始行、L25-L31）
5. `defines.iter().map(|(d,m,_)| (d.to_string(), m.to_string())).collect()`で比較用ペアベクタ生成（L33-L36）
6. `assert!(define_pairs.contains(&(...)))`で期待メソッドの存在を検証（L38-L49）

同様に、object（L53-L96）と入れ子クラス（L98-L138）でもフローは同一。差分はKotlinコードの内容と期待する(定義主体, メソッド名)のペアのみ。

重要箇所抜粋:

```rust
// パーサ生成と定義抽出（test_kotlin_class_method_definitions, L22-L23）
let mut parser = KotlinParser::new().expect("Failed to create parser");
let defines = parser.find_defines(code);
```

```rust
// 比較用の(定義主体, メソッド名)ペアへ投影（test_kotlin_class_method_definitions, L33-L36）
let define_pairs: Vec<(String, String)> = defines
    .iter()
    .map(|(definer, method, _)| (definer.to_string(), method.to_string()))
    .collect();
```

```rust
// 検証（例: クラス内メソッド, L38-L49）
assert!(
    define_pairs.contains(&("UserService".to_string(), "createUser".to_string())),
    "Should detect createUser method in UserService"
);
```

## Complexity & Performance

- 時間計算量
  - テストコード側: `defines`の線形走査と`to_string()`によるコピーでO(k)（k = 検出された定義数）
  - アサートは定数個の`.contains`（VecであればO(k)を数回）
- 空間計算量
  - `define_pairs`を構築するためにO(k)の追加メモリ
- ボトルネック
  - 実質的なコストは`find_defines`の実装に依存（このチャンクには現れない）。テスト側は軽量。
- スケール限界
  - 巨大ファイルに対してもテスト側のオーバーヘッドは小さいが、`define_pairs`がVecのままだと`.contains`は線形探索。必要に応じて`BTreeSet`/`HashSet`で改善可能。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト
- メモリ安全性: unsafe未使用。バッファオーバーフロー/Use-after-free/整数オーバーフローの懸念なし（このファイル内）。
- インジェクション: SQL/コマンド/パストラバーサルなし。入力はテスト内の固定文字列。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密情報なし（例のJDBC URLはダミー）。ログ漏えいリスク低。
- 並行性: 共有可変状態なし。各テストは独立インスタンスのパーサを使用。デッドロック・競合なし。

観測された/潜在的な問題
- 余分な検出を許容する可能性
  - 期待の存在のみを検証し、総数や完全一致を検証していないため、過検出（コメント内の`fun`等）が混入しても検知できない。
- 行番号や範囲の未検証
  - `range.start_line`を出力するが、期待値で検証していない（L25-L31等）。オフバイワンや先頭改行の影響を検知できない。
- 重複検出未検証
  - 同名メソッドのオーバーロードなどで重複が起きた場合の挙動不明。
- 名前解決の粒度
  - 入れ子クラスは`"InnerClass"`として検証（L134-L137）。期待仕様が`"OuterClass.InnerClass"`であるべきかは不明（仕様未提示）。

Rust特有の観点
- 所有権/借用
  - `defines.iter()`を不変借用（L34, L78, L126）。`to_string()`で所有データへコピーするため、その後のライフタイム非依存で安全。
- ライフタイム
  - 明示的ライフタイム指定は不要。文字列リテラルを引数に渡すのみ。
- unsafe境界
  - なし。
- 並行性・非同期
  - 非同期/awaitなし。各テストは独立。テストハーネスの並列実行でも共有資源なし。
- エラー設計
  - `KotlinParser::new().expect("...")`はテストとして妥当（初期化失敗は即時失敗が望ましい）。`find_defines`のエラー取り扱いはこのチャンクに現れない。

エッジケース詳細（本ファイルでの実装/状態は未確定）
| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空入力 | "" | 定義なし（空コレクション） | このチャンクには現れない | 未テスト |
| コメント内のfun | "// fun fake()" | 無視される | このチャンクには現れない | 未テスト |
| 文字列内のfun | "println(\"fun not a def\")" | 無視される | このチャンクには現れない | 未テスト |
| トップレベル関数 | "fun top() {}" | definerはファイル/パッケージ（仕様次第） | このチャンクには現れない | 未テスト |
| 拡張関数 | "fun String.ext() {}" | definerはレシーバ型か宣言スコープ（仕様次第） | このチャンクには現れない | 未テスト |
| companion object | "class A { companion object { fun f(){} } }" | definerが"A"または"Companion"（仕様次第） | このチャンクには現れない | 未テスト |
| オーバーロード | "fun f(x:Int){} fun f(x:String){}" | 両方検出 | このチャンクには現れない | 未テスト |
| アノテーション付与 | "@Test fun f(){}" | 正しく検出 | このチャンクには現れない | 未テスト |

## Design & Architecture Suggestions

- 期待の完全一致検証
  - 包含チェックに加え、検出集合のサイズ一致や正確な集合等価を検証すると、過検出/過少検出を検知可能。
- 行番号・範囲の検証
  - `range.start_line`（および可能ならend_line）を期待値で検証。先頭改行の影響やインデントの扱い、0/1始まりの仕様を固定化。
- 名前解決ポリシーの明確化
  - 入れ子クラスやcompanionのdefiner表現を仕様化（例: "Outer.Inner" or "Inner"）。それに合わせてテストを更新。
- テストのDRY化
  - 共通処理（パーサ生成、定義抽出、ペア化）をヘルパーに切り出し。ケース追加が容易に。
- 検出性能への配慮（将来）
  - 大規模入力向けには集合型（HashSet/BTreeSet）で高速包含検証。

## Testing Strategy (Unit/Integration) with Examples

追加で検証すべきケースの例:

1) トップレベル関数
```rust
#[test]
fn kotlin_top_level_functions_are_detected() {
    let code = r#"
fun top() {}
"#;
    let mut parser = KotlinParser::new().expect("create parser");
    let defines = parser.find_defines(code);

    let pairs: std::collections::BTreeSet<(String, String)> = defines
        .iter()
        .map(|(d, m, _)| (d.to_string(), m.to_string()))
        .collect();

    // 期待するdefiner表現は仕様次第（例: "<file>" や パッケージ名）
    assert!(pairs.iter().any(|(_, m)| m == "top"));
    // 件数の完全一致
    assert_eq!(pairs.len(), 1);
}
```

2) 拡張関数
```rust
#[test]
fn kotlin_extension_functions_are_detected() {
    let code = r#"
fun String.ext(): Int = 0
"#;
    let mut parser = KotlinParser::new().expect("create parser");
    let defines = parser.find_defines(code);

    let items: Vec<_> = defines.iter().collect();
    // definerが"String"なのか、ファイル/パッケージなのかは仕様に依存
    assert!(items.iter().any(|(_, m, _)| m.to_string() == "ext"));
}
```

3) companion object
```rust
#[test]
fn kotlin_companion_object_methods() {
    let code = r#"
class A {
    companion object {
        fun factory(): A = A()
    }
}
"#;
    let mut parser = KotlinParser::new().expect("create parser");
    let defines = parser.find_defines(code);

    let pairs: Vec<(String, String)> = defines
        .iter()
        .map(|(d, m, _)| (d.to_string(), m.to_string()))
        .collect();

    // 期待definerは仕様で決定（"A" or "Companion"）
    assert!(pairs.iter().any(|(_, m)| m == "factory"));
}
```

4) コメント・文字列に含まれる`fun`の無視
```rust
#[test]
fn ignores_fun_in_comments_and_strings() {
    let code = r#"
// fun fake() {}
val s = "fun not a def"
class C { fun real(){} }
"#;
    let mut parser = KotlinParser::new().expect("create parser");
    let defines = parser.find_defines(code);

    let pairs: std::collections::BTreeSet<(String, String)> = defines
        .iter()
        .map(|(d, m, _)| (d.to_string(), m.to_string()))
        .collect();

    assert!(pairs.contains(&("C".into(), "real".into())));
    assert_eq!(pairs.len(), 1, "No false positives from comments/strings");
}
```

5) 行番号検証
```rust
#[test]
fn validates_start_line_numbers() {
    let code = r#"
class C {
    fun a() {}
    fun b() {}
}
"#;
    let mut parser = KotlinParser::new().expect("create parser");
    let defines = parser.find_defines(code);

    // 先頭の改行を含めた行数カウント仕様に基づき期待を固定化
    // 例: "fun a" が3行目、"fun b" が4行目（仕様に合わせて調整）
    let mut map = std::collections::HashMap::new();
    for (d, m, r) in &defines {
        if d == "C" {
            map.insert(m.to_string(), r.start_line);
        }
    }
    assert_eq!(map.get("a").copied(), Some(3));
    assert_eq!(map.get("b").copied(), Some(4));
}
```

## Refactoring Plan & Best Practices

- 重複ロジックの抽出
  - ヘルパー関数で共通処理を集約
```rust
fn parse_def_pairs(code: &str) -> std::collections::BTreeSet<(String, String)> {
    let mut parser = KotlinParser::new().expect("create parser");
    parser.find_defines(code)
        .iter()
        .map(|(d, m, _)| (d.to_string(), m.to_string()))
        .collect()
}
```
- 完全一致アサートの導入
```rust
use std::collections::BTreeSet;
fn assert_pairs_eq(actual: &BTreeSet<(String, String)>, expected: &BTreeSet<(String, String)>) {
    assert_eq!(actual, expected, "Detected pairs must match exactly");
}
```
- 見やすい差分出力
  - `pretty_assertions`の導入で失敗時の差分可視化。
- 命名規約・仕様のドキュメント化
  - definerの表記（入れ子・companion・拡張関数など）の仕様をコメントで明文化し、テストもそれに整合。

## Observability (Logging, Metrics, Tracing)

- 現状`println!`で検出結果を出力（L25-L31, L69-L75, L117-L123）。テストランで標準出力がノイズになりうる。
- 改善案
  - 解析器内部で`tracing`/`log`を使用し、必要時のみ`RUST_LOG`で可視化。
  - テストでは通常ログ抑制、失敗時にのみ詳細ログを表示（`test -- --nocapture`を併用）。
  - 解析メトリクス（検出件数、解析時間）を計測可能な形でエクスポート（このチャンクには現れない実装）。

## Risks & Unknowns

- 仕様不明点
  - definer表記の正規仕様（入れ子、companion、拡張関数、トップレベル）が不明。
  - `find_defines`の戻り値の厳密な型と順序性の保証が不明。
  - `start_line`の起点（0/1始まり）、改行/空白の取り扱い仕様が不明。
- テストカバレッジの不足
  - コメント・文字列内の`fun`、オーバーロード、アクセス修飾子の網羅（public/protected/internal）、`suspend`、`inline`、`operator`、アノテーション、ジェネリクス、デフォルト引数、単行定義など未検証。
- 並行実行時のパーサのThread-safety
  - 各テストで新規インスタンス生成のため直接の問題は見えないが、内部実装の`Send/Sync`は不明（このチャンクには現れない）。

## Structure & Key Components（補足: 重要根拠 行番号）

- KotlinParser::new の使用: test_kotlin_class_method_definitions: L22、test_kotlin_object_method_definitions: L66、test_kotlin_nested_class_method_definitions: L114
- find_defines の使用: test_kotlin_class_method_definitions: L23、test_kotlin_object_method_definitions: L67、test_kotlin_nested_class_method_definitions: L115
- range.start_line の参照: L29、L73、L121
- (definer, method)への写像: L33-L36、L77-L80、L125-L128
- 具体的アサート例: L38-L49、L82-L95、L130-L137

以上の観測はすべて本チャンク中のコード断片に基づく。