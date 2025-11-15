# parsers\kotlin\test_nested_scopes.rs Review

## TL;DR

- 目的: Kotlinのネスト構造（入れ子クラス、companion object、メソッド内ローカル関数）での**定義検出と親コンテキスト復元**を検証するテスト。
- 主要API: **KotlinParser::new**（生成）と **LanguageParser::find_defines**（定義抽出）。戻り値は「(definer, defined, range)」タプルの配列。
- 複雑箇所: コンテキストの**保存・復元**（ネストの入出やcompanion object後のコンテキスト復帰）を正しく扱えているかの検証。
- 重大リスク: find_definesの**出力仕様がこのチャンクには現れない**ため、companion objectやローカル関数の扱いが**不明**。テストは「インスタンスメソッドのみ」を前提にassertしている。
- パフォーマンス: テスト側の存在確認は`Vec.contains`の**線形探索**で、定義数が多い場合に非効率。`HashSet`を使うと改善。
- セキュリティ/安全性: **unsafeなし**、並行性なし、I/Oなし。テスト内`expect`のpanicは許容。重大な脆弱性は**該当なし**。
- 追加検証案: named companion、オブジェクト宣言、拡張関数、トップレベル関数、同名メソッドの重複などへの対応確認。

## Overview & Purpose

このファイルはRustのテストモジュールで、`codanna::parsing::kotlin::KotlinParser`がKotlinコード中のメソッドやクラス定義を正しく抽出し、親子関係（どのクラスがどのメソッドを定義したか）を正しく維持・復元できるかを検証します。

- ネストしたクラス（Outer → Middle → Inner）
- companion objectの内部メソッドとインスタンスメソッドの区別
- メソッド内部に定義されたローカル関数の無視（トップレベル定義としては数えない）
- ネスト構造から抜けた後の**コンテキスト復帰**（「save/restore pattern」）

が主な検証対象です。

このチャンクにはパーサ本体は含まれていないため、コアロジックの詳細は**不明**ですが、テストから出力の期待仕様を逆照射しています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_nested_class_context | private (test) | 入れ子クラス階層のメソッド検出と親コンテキスト復元の検証 | Med |
| Function | test_companion_object_context | private (test) | companion objectとインスタンスメソッドの識別、コンテキスト復帰の検証 | Med |
| Function | test_nested_function_context | private (test) | メソッド内ローカル関数の無視と入れ子クラス後のコンテキスト復帰の検証 | Med |
| External Struct | KotlinParser | 外部（codanna） | Kotlinコードの解析器 | 不明 |
| External Trait | LanguageParser | 外部（codanna） | 共通パーサインターフェイス（find_definesを提供） | 不明 |

### Dependencies & Interactions

- 内部依存
  - テスト関数間の直接依存はなし。各テストは独立。
- 外部依存（使用クレート・モジュール）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | codanna::parsing::kotlin::KotlinParser | パーサ生成・解析 | `new()`で生成し、`find_defines`で定義抽出 |
  | codanna::parsing::LanguageParser | トレイト境界 | `find_defines`メソッドの提供者（推定） |
- 被依存推定
  - Kotlinパーサのスコープ管理をCIで回帰検証するためのテストスイートとして、このモジュールが参照される可能性。

## API Surface (Public/Exported) and Data Contracts

このファイル自身に公開APIはありません（テストのみ）。ただし、テストで使用する外部APIの契約が重要なため、それらを列挙します。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| KotlinParser::new | 不明（Result想定） | パーサインスタンスの生成 | 不明 | 不明 |
| LanguageParser::find_defines | 不明（引数: &str, 戻り値: Vec<(Definer, Defined, Range)>想定） | Kotlinコード中の定義抽出 | O(n)（推定） | O(d)（定義数d）推定 |

詳細説明:

1) KotlinParser::new
- 目的と責務
  - Kotlin言語の構文解析器を初期化する。
- アルゴリズム
  - このチャンクには現れない（初期化内部は不明）。
- 引数
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | なし | 不明 | - | 生成器（追加設定があるかは不明） |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Result<KotlinParser, E>（推定） | `expect("Failed to create parser")`が使用されているため、`Result`または`Option`を返すが、一般的には`Result`が妥当。エラー型Eは不明。 |
- 使用例
  ```rust
  let mut parser = KotlinParser::new().expect("Failed to create parser");
  ```
- エッジケース
  - 初期化失敗時に`expect`によりpanic（テストとしては妥当）。
  - 追加設定や言語バージョン指定が必要なケースはこのチャンクには現れない。

2) LanguageParser::find_defines
- 目的と責務
  - 入力されたKotlinコード文字列から「定義（メソッドやクラス）」を抽出し、各定義の親コンテキスト（definer）とソース範囲（range）を返す。
- アルゴリズム（テストから推測）
  - コードを走査し、クラス・メソッド宣言を検出。
  - スコープスタックで現在のコンテキスト（親クラスなど）を管理。
  - ネスト開始時にコンテキストを保存し、終了時に復元（save/restore）。
  - メソッド内の「ローカル関数」はトップレベル定義としては返さない。
  - companion object内部の関数は「別トラッキング」または返却対象外（このチャンクのassertにより推定）。
- 引数
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | code | &str | 必須 | Kotlinソースコード文字列 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(Definer, Defined, Range)> | Definer/Definedは`Display`/`ToString`実装あり（printlnとto_stringが可能）。`Range`は`start_line`フィールドあり。具体型は不明。 |
- 使用例
  ```rust
  let defines = parser.find_defines(code);
  for (definer, defined, range) in &defines {
      println!("{} defines {} at line {}", definer, defined, range.start_line);
  }
  ```
- エッジケース
  - companion objectの関数扱い（返却されない可能性）
  - メソッド内ローカル関数は返却対象外（テストから確証）
  - ネストクラス内のメソッドは返却される
  - トップレベル関数や拡張関数の扱いはこのチャンクには現れない

## Walkthrough & Data Flow

テストはいずれも次の一般パターンで動作します。

- Kotlinコードの準備（入れ子構造やcompanion objectを含む）
- パーサ生成（`KotlinParser::new().expect(...)`）
- 定義抽出（`find_defines(code)`）
- 抽出結果のログ出力（definer/defined/start_line）
- `define_pairs`（(definer, defined)のStringペア）を生成
- `contains`で期待ペアの存在確認
- 必要に応じて`defines.len()`で総数検証

具体的なテストごとのフロー:

1) test_nested_class_context
- 入れ子クラス Outer → Middle → Inner と、それぞれのメソッドを持つ。
- 期待する抽出:
  - ("Outer", "outerMethod")
  - ("Outer", "anotherOuterMethod")
  - ("Middle", "middleMethod")
  - ("Inner", "innerMethod")
- 目的は「Middle/Innerの内側に入っても、外側に戻ったらOuterコンテキストが復元される」ことの確認。

```mermaid
flowchart TD
  A[準備: Kotlinコード (Outer/Middle/Inner)] --> B[parser = KotlinParser::new()]
  B --> C[defines = parser.find_defines(code)]
  C --> D[define_pairs = map(definer, defined)]
  D --> E{contains (Outer, outerMethod)?}
  D --> F{contains (Outer, anotherOuterMethod)?}
  D --> G{contains (Middle, middleMethod)?}
  D --> H{contains (Inner, innerMethod)?}
  E -->|Yes| I[Assert OK]
  F -->|Yes| I
  G -->|Yes| I
  H -->|Yes| I
```
上記の図は`test_nested_class_context`関数の主要分岐を示す（行番号: 不明）。

2) test_companion_object_context
- MyClassにインスタンスメソッド2つ、companion objectにメソッド2つ。
- 期待する抽出:
  - ("MyClass", "instanceMethod")
  - ("MyClass", "anotherInstanceMethod")
- 総数検証: `defines.len() == 2`（companion objectのメソッドは「別扱い」または非対象）

```mermaid
flowchart TD
  A[準備: Kotlinコード (class/companion object)] --> B[KotlinParser::new()]
  B --> C[find_defines(code)]
  C --> D[define_pairs 生成]
  D --> E{contains (MyClass, instanceMethod)?}
  D --> F{contains (MyClass, anotherInstanceMethod)?}
  C --> G{defines.len() == 2?}
  E -->|Yes| H[Assert OK]
  F -->|Yes| H
  G -->|Yes| H
```
上記の図は`test_companion_object_context`関数の主要分岐を示す（行番号: 不明）。

3) test_nested_function_context
- Container.outerFunction内にローカル関数localFunction、NestedClass.nestedMethod内にもローカル関数。
- 期待する抽出:
  - ("Container", "outerFunction")
  - ("NestedClass", "nestedMethod")
  - ("Container", "afterNestedClass")
- 総数検証: `defines.len() == 3`（ローカル関数は非対象）

```mermaid
flowchart TD
  A[準備: Kotlinコード (メソッド内関数/入れ子クラス)] --> B[KotlinParser::new()]
  B --> C[find_defines(code)]
  C --> D[define_pairs 生成]
  D --> E{contains (Container, outerFunction)?}
  D --> F{contains (NestedClass, nestedMethod)?}
  D --> G{contains (Container, afterNestedClass)?}
  C --> H{defines.len() == 3?}
  E -->|Yes| I[Assert OK]
  F -->|Yes| I
  G -->|Yes| I
  H -->|Yes| I
```
上記の図は`test_nested_function_context`関数の主要分岐を示す（行番号: 不明）。

## Complexity & Performance

- 時間計算量（テスト側）
  - `define_pairs`構築: O(d)（dは定義数）
  - 各`contains`チェック: O(d)（`Vec`線形探索）
  - テストあたりの合計: O(d + k·d) ≒ O(k·d)（kはassert件数、各テストで約4）
- 空間計算量（テスト側）
  - `define_pairs`: O(d)
- ボトルネック
  - `Vec.contains`の線形探索。定義が多い場合非効率。
- スケール限界
  - 大規模ファイルの定義数が増えると、テストの`contains`が遅くなる可能性。
- 改善案
  - `HashSet<(String, String)>`で高速化（平均O(1)）。例:
    ```rust
    use std::collections::HashSet;

    let define_pairs: HashSet<(String, String)> = defines
        .iter()
        .map(|(definer, defined, _)| (definer.to_string(), defined.to_string()))
        .collect();

    assert!(define_pairs.contains(&("Outer".to_string(), "outerMethod".to_string())));
    ```

- 実運用負荷要因
  - このファイルはテストのみでI/Oやネットワーク・DBは使用なし。
  - 実解析のコストは`find_defines`実装次第（このチャンクには現れない）。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - 所有権・借用: `defines.iter()`で不変借用、`to_string()`で所有データを複製。移動・借用の不整合は**なし**。
  - unsafe境界: **使用なし**。
  - ライフタイム: 明示的ライフタイムは不要。`&defines`のスコープはテスト関数内に限定。
- インジェクション
  - SQL/Command/Path traversal: **該当なし**（外部コマンドやファイルアクセスなし）。
- 認証・認可
  - **該当なし**（テスト環境のみ）。
- 秘密情報
  - ハードコード秘密情報: **なし**。
  - ログ漏えい: `println!`で定義を出力するが、テストログの範囲内で問題性は低い。
- 並行性
  - **該当なし**（同期コードのみ、共有可変状態なし）。
- エラー設計
  - `KotlinParser::new().expect("Failed to create parser")`により初期化失敗時**panic**。テストでは妥当だが、ライブラリコードではResult伝播推奨。
  - `find_defines`のエラー戻りは**不明**（このチャンクには現れない）。

詳細化（エッジケーステーブル）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | Err(Empty)または空集合 | このチャンクには現れない | 不明 |
| 巨大ファイル | 数万行 | 時間内に解析、メモリ過剰使用なし | このチャンクには現れない | 不明 |
| named companion | `companion object MyComp { fun f() }` | companion関数の扱い一貫性 | このチャンクには現れない | 不明 |
| ローカル関数検出 | `fun m() { fun l(){} }` | lはトップレベル定義としては非対象 | テストで非対象を検証 | OK（仕様推定） |
| 入れ子クラス深階層 | 4階層以上 | コンテキストが正しく復元 | このチャンクには現れない | 不明 |
| 同名メソッド重複 | `fun a()`が複数 | 重複検出と親クラス別管理 | このチャンクには現れない | 不明 |
| 拡張関数 | `fun A.ext(){}` | 親コンテキストの表現（A） | このチャンクには現れない | 不明 |
| トップレベル関数 | `fun top(){}` | definerがパッケージ/ファイル？ | このチャンクには現れない | 不明 |
| オブジェクト宣言 | `object O { fun f() }` | companionとの区別 | このチャンクには現れない | 不明 |
| コメント/文字列内キーワード | `"class"`など | 誤検出しない | このチャンクには現れない | 不明 |

Rust特有の観点（詳細チェックリスト）:
- 所有権: `define_pairs`生成時に`to_string()`で所有値を作成（関数: 全テスト関数）。ムーブによる不整合は**なし**。
- 借用: `defines.iter()`は不変借用。可変借用の衝突は**なし**。
- ライフタイム: 明示ライフタイム不要。ローカルスコープで完結。
- unsafe境界: **なし**。
- Send/Sync: スレッド境界**該当なし**。並行テストでない。
- データ競合: **なし**。
- await境界: **該当なし**。
- キャンセル: **該当なし**。
- Result vs Option: `new()`は`expect`でpanic。テストでは許容。`find_defines`はResultを返していない（このチャンクでは`let defines = ...`）ため、失敗時の扱いは**不明**。
- panic箇所: `expect`のみ。テストなので妥当。
- エラー変換: **該当なし**。

## Design & Architecture Suggestions

- 期待仕様の明文化
  - `find_defines`が返す対象（インスタンスメソッドのみか、companion内メソッドを含むか）をドキュメント化。テストコメントでは「companionは別トラッキング」と示唆するが、契約として明確化が必要。
- コンテキストモデルの検証強化
  - `range.start_line`の確認も加え、検出位置の正確性をテストに含める。
  - 入れ子クラス抜け後のコンテキスト復帰を「開始/終了の行番号」も使ってチェックするとより堅牢。
- テストユーティリティの抽象化
  - `extract_pairs(defines) -> HashSet<(String, String)>`などのヘルパー関数で重複コード削減。
  - マクロ`assert_pair!`でアサート文の可読性向上。
- 将来の拡張
  - named companion、object宣言、拡張関数、トップレベル関数などKotlin言語機能のカバレッジ拡張。

## Testing Strategy (Unit/Integration) with Examples

- 追加ユニットテスト例: named companion
```rust
#[test]
fn test_named_companion_object_context() {
    let code = r#"
class MyClass {
    companion object Factory {
        fun create() { println("create") }
    }
    fun after() { println("after") }
}
"#;
    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    let pairs: std::collections::HashSet<_> = defines
        .iter()
        .map(|(d, f, _)| (d.to_string(), f.to_string()))
        .collect();

    // 仕様次第だが、少なくともインスタンスメソッドは検出されるべき
    assert!(pairs.contains(&("MyClass".to_string(), "after".to_string())));
    // companionの扱いは契約次第。含めるなら以下を検証
    // assert!(pairs.contains(&("Factory".to_string(), "create".to_string())));
}
```

- 追加ユニットテスト例: オブジェクト宣言
```rust
#[test]
fn test_object_declaration() {
    let code = r#"
object Singleton {
    fun doWork() { println("work") }
}
"#;
    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    let pairs: Vec<_> = defines.iter().map(|(d, f, _)| (d.to_string(), f.to_string())).collect();
    // 定義の親が"Singleton"になるかの確認
    assert!(pairs.contains(&("Singleton".to_string(), "doWork".to_string())));
}
```

- 追加ユニットテスト例: 拡張関数
```rust
#[test]
fn test_extension_function() {
    let code = r#"
class A
fun A.ext() { println("ext") }
"#;
    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    // extの親（definer）をどう表現するかは契約次第（"A"か、特殊表現か）
    for (d, f, _) in &defines {
        println!("{} defines {}", d, f);
    }
    // assertは契約確定後に追加
}
```

- 追加ユニットテスト例: コメント/文字列中キーワード誤検出防止
```rust
#[test]
fn test_no_false_positive_in_strings() {
    let code = r#"
class C {
    fun f() {
        println("class Inner { fun fake() {} }")
        // class Fake { fun fake2() {} }
    }
}
"#;
    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);
    let pairs: Vec<_> = defines.iter().map(|(d, f, _)| (d.to_string(), f.to_string())).collect();

    // fのみ検出され、文字列やコメント中の宣言は無視されるべき
    assert!(pairs.contains(&("C".to_string(), "f".to_string())));
    assert_eq!(defines.len(), 1);
}
```

## Refactoring Plan & Best Practices

- ヘルパー導入
  ```rust
  fn to_pair_set(defines: &Vec<(impl std::fmt::Display, impl std::fmt::Display, impl std::fmt::Debug)>) 
      -> std::collections::HashSet<(String, String)> 
  {
      defines.iter().map(|(d, f, _)| (d.to_string(), f.to_string())).collect()
  }
  ```
  注: 具体型が不明なため、トレイト境界は概念的記述。実コードではジェネリクスまたは具体型に合わせる。

- containsの高速化
  ```rust
  let pairs = to_pair_set(&defines);
  assert!(pairs.contains(&("Outer".to_string(), "outerMethod".to_string())));
  ```

- 重複アサーションの簡素化マクロ
  ```rust
  macro_rules! assert_has {
      ($set:expr, $d:expr, $f:expr) => {
          assert!($set.contains(&(String::from($d), String::from($f))), 
                  "missing {}::{}", $d, $f);
      }
  }
  ```

- ログ抑制
  - `println!`はテストの標準出力が煩雑になりがち。必要時のみ表示、あるいは`cargo test -- --nocapture`で明示的に出す運用にする。
  - 確認が目的なら`assert_eq!`や`pretty_assertions`の活用を推奨。

## Observability (Logging, Metrics, Tracing)

- 現状
  - `println!`で抽出結果を可視化。テスト中のデバッグには有用。
- 改善案
  - ログ出力をテスト失敗時のみに出すヘルパー（失敗時に`defines`をdump）。
  - `insta`スナップショットテストで抽出結果の差分を継続的に監視。
  - `range.start_line`も含めた検証で「どの行で検出されたか」をログとして残す。

例（失敗時のみダンプするヘルパー）:
```rust
fn assert_pairs(pairs: &std::collections::HashSet<(String, String)>, expected: &[(&str, &str)]) {
    for (d, f) in expected {
        if !pairs.contains(&(d.to_string(), f.to_string())) {
            eprintln!("Missing pair: {}::{}", d, f);
            for (pd, pf) in pairs {
                eprintln!("Found: {}::{}", pd, pf);
            }
            panic!("assert_pairs failed");
        }
    }
}
```

## Risks & Unknowns

- 不明点（このチャンクには現れない）
  - `find_defines`の正確な戻り値型（Definer/Defined/Rangeの具体型）。
  - companion objectメソッドの扱い（返す/返さない/別コレクション）。
  - トップレベル関数や拡張関数、オブジェクト宣言の扱い。
  - コメント・文字列中のキーワード無視ロジック。
  - 解析の計算量やメモリ使用量、悪ケース（多重入れ子、匿名クラス、ラムダ内宣言）の扱い。
- 仕様の前提
  - テストは「インスタンスメソッド数を2とする」「ローカル関数を非対象」として設計されている。実装がこれと異なる仕様を採用している場合、テストと実装の乖離が生じる。
- 変更リスク
  - `find_defines`の対象定義範囲（何を「定義」と見なすか）を拡張すると、既存テストが壊れる可能性。契約（仕様）を文書化して合意形成が必要。