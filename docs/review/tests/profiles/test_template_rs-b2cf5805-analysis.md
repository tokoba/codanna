# profiles\test_template.rs Review

## TL;DR

- 目的: テンプレート置換関数**substitute_variables**の基本動作（単一/複数/重複プレースホルダと欠損時エラー）を検証するユニットテスト集。
- 主要API: 推定シグネチャは**fn substitute_variables(template: &str, vars: &HashMap<String, String>) -> Result<String, E>**（EはDisplay実装）。欠損キーでErrを返す（test_missing_variable_error）。
- 複雑箇所: 欠損変数の扱いとエラーメッセージ仕様、同一変数の複数回置換、複数プレースホルダの行跨ぎ置換。
- 重大リスク: テストがエラーメッセージに含まれる文字列「missing」へ依存しており、実装の文言変更で脆くなる。
- 未カバー: 未閉じ/不正なトークン、空キー、空白を含むトークン、エスケープ、非ASCIIキー、値に波括弧を含むケースの検証がない。
- 並行性/安全性: テストは同期で安全。実装側のスレッド安全性は不明。引数は借用で戻り値は所有Stringのためライフタイムは単純。

## Overview & Purpose

このファイルは、codanna::profiles::template::substitute_variablesのテンプレート文字列置換機能を検証するためのユニットテストを6件定義している。置換の基本仕様（存在するキーは正しく置換される、複数キーや重複出現も置換、存在しないキーはエラー、変数なし・空テンプレートはそのまま）を網羅的に確認し、公開APIのコア期待値を固定する意図がある。

根拠:
- test_substitute_single_variable（L7-L14）
- test_substitute_multiple_variables（L17-L26）
- test_substitute_same_variable_multiple_times（L29-L36）
- test_missing_variable_error（L39-L48）
- test_no_variables（L51-L57）
- test_empty_template（L60-L66）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Test fn | test_substitute_single_variable | private #[test] | 単一プレースホルダ置換の検証 | Low |
| Test fn | test_substitute_multiple_variables | private #[test] | 複数キーの置換と行跨ぎの検証 | Low |
| Test fn | test_substitute_same_variable_multiple_times | private #[test] | 同一キーの複数出現置換の検証 | Low |
| Test fn | test_missing_variable_error | private #[test] | 欠損キーでのエラー発生とメッセージの一部検証 | Low |
| Test fn | test_no_variables | private #[test] | 変数が存在しないテンプレートの不変性 | Low |
| Test fn | test_empty_template | private #[test] | 空テンプレートの不変性 | Low |

### Dependencies & Interactions

- 内部依存
  - 各テスト関数は共有のヘルパー等を持たず、共通して**substitute_variables**を直接呼び出す。可変/不変な共有状態は存在しない（各テストで独立にHashMapを作成）。

- 外部依存

| クレート/モジュール | 要素 | 用途 | 備考 |
|---------------------|------|------|------|
| codanna::profiles::template | substitute_variables | テンプレート置換の対象関数 | 実装はこのチャンクには現れない |
| std::collections | HashMap | 変数名→値の辞書 | String→String のマップ |

- 被依存推定
  - このテストモジュール自体を利用する箇所: Rustテストハーネスのみ。
  - substitute_variablesの利用者: プロファイル生成/テンプレート処理ロジック（推定）。このチャンクには現れない。

## API Surface (Public/Exported) and Data Contracts

このファイル自体に公開APIはないが、テスト対象の外部APIを明示する。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| substitute_variables | fn substitute_variables(template: &str, vars: &HashMap<String, String>) -> Result<String, E> | テンプレート中の{{key}}をvarsで置換 | 推定O(n) | 推定O(n) |

詳細:

1) 目的と責務
- テンプレート文字列中のプレースホルダ（推定: 「{{key}}」形式）を与えられた辞書で全て置換する。
- 未定義キーが存在する場合はエラーを返す（test_missing_variable_error, L43-L47）。

2) アルゴリズム（推定・テストからの挙動）
- 文字列を走査し、トークン「{{...}}」を検出。
- 内部のキー文字列を抽出し、varsから検索。
- 見つかれば該当箇所を値に置換、見つからなければErrを返す。
- 同一キーが複数回出現する場合も全て置換（test_substitute_same_variable_multiple_times, L29-L36）。

3) 引数

| 引数 | 型 | 必須 | 説明 |
|------|----|------|------|
| template | &str | Yes | 置換対象のテンプレート文字列 |
| vars | &HashMap<String, String> | Yes | 変数名→値のマップ |

4) 戻り値

| 型 | 説明 |
|----|------|
| Ok(String) | 置換後の新しい文字列（所有） |
| Err(E) | エラー。EはDisplayを実装（to_stringが呼ばれている: test_missing_variable_error, L46） |

5) 使用例

```rust
use std::collections::HashMap;
use codanna::profiles::template::substitute_variables;

let template = "# {{project_name}}\nAuthor: {{author}}";
let mut vars = HashMap::new();
vars.insert("project_name".to_string(), "MyProject".to_string());
vars.insert("author".to_string(), "John Doe".to_string());

let out = substitute_variables(template, &vars).unwrap();
assert_eq!(out, "# MyProject\nAuthor: John Doe");
```

6) エッジケース
- 空テンプレートは空文字を返す（test_empty_template, L60-L66）。
- 変数が一切ない場合は入力をそのまま返す（test_no_variables, L51-L57）。
- 欠損キーがある場合はErr（test_missing_variable_error, L43-L47）。
- 同一キーの複数出現も全て置換（test_substitute_same_variable_multiple_times, L29-L36）。
- トークンの妥当性（未閉じ「{{」など）や空キー「{{}}」の扱いは不明（このチャンクには現れない）。

## Walkthrough & Data Flow

- test_substitute_single_variable（L7-L14）
  - 入力: "Project: {{project_name}}" と {"project_name":"MyProject"}
  - フロー: HashMap生成 → substitute_variables呼出 → Okをunwrap → 期待文字列と比較
  - データ: &strと&HashMap<String,String>借用 → Stringが返却（所有）

- test_substitute_multiple_variables（L17-L26）
  - 入力: 3つのプレースホルダ（行跨ぎ）
  - 期待: 全て対応する値で置換

- test_substitute_same_variable_multiple_times（L29-L36）
  - 入力: 同一キー"name"が2回出現
  - 期待: 両方"Alice"で置換

- test_missing_variable_error（L39-L48）
  - 入力: "{{missing}}" と空の辞書
  - 期待: Err。さらにerr.to_string()に"missing"を含むことを検証（エラーの文言仕様に依存）

- test_no_variables（L51-L57）
  - 入力: プレースホルダなし
  - 期待: 入出力同一

- test_empty_template（L60-L66）
  - 入力: 空文字
  - 期待: 空文字

Mermaid図の基準に照らし分岐数が少ないため図示は省略。

## Complexity & Performance

- substitute_variables（推定）
  - 時間計算量: O(n)（n=テンプレート長）。置換戦略がナイーブに都度検索/連結する場合はO(n + Σ置換長)程度。正規表現や二段階走査でも概ね線形。
  - 空間計算量: O(n)（出力Stringのため）。出力容量の事前確保で再割り当てを削減可能。
  - ボトルネック: 大量/巨大テンプレートでの文字列連結コスト、辞書探索（HashMapの平均O(1)は十分高速）。
  - I/O/ネットワーク/DB: 関与なし。

- テスト群
  - 極小規模。実行時間/メモリともに無視可能。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト:
- メモリ安全性: このテストは安全なRustのみを使用。未定義動作やunsafeはこのチャンクには現れない。
- インジェクション: 置換結果をコマンド/SQL/パスに用いる上位層があればリスク。関数自体はサニタイズを目的としないため、用途側でのエスケープ/検証が必要（このチャンクには現れない）。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密なし。ログ漏えいもなし。
- 並行性: テストは並列アクセスなし。実装のスレッド安全性は不明（グローバル状態が無ければ純関数で安全なはずだが不明）。

Rust特有の観点:
- 所有権/借用: &strと&HashMap<String,String>を借用して呼び出し、所有Stringを返すためライフタイム衝突の可能性は低い（test関数全般）。
- ライフタイム: 明示的ライフタイムは不要（戻り値が所有String）。
- unsafe境界: 未使用（このチャンクには現れない）。
- エラー設計: Resultを返すことは妥当。テストはError: Displayを前提（L46）。エラーの粒度や型は不明。

エッジケース詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | Ok("") | substitute_variables | テストあり/OK（L60-L66） |
| 変数なし | "Plain text" | Ok("Plain text") | 同上 | テストあり/OK（L51-L57） |
| 単一変数 | "Project: {{project_name}}" | Ok("Project: MyProject") | 同上 | テストあり/OK（L7-L14） |
| 複数変数 | "# {{project_name}} ... {{author}} ..." | 全て置換 | 同上 | テストあり/OK（L17-L26） |
| 同一変数複数回 | "{{name}} is {{name}}" | すべて同値で置換 | 同上 | テストあり/OK（L29-L36） |
| 欠損変数 | "{{missing}}" | Err（エラーメッセージに"missing"） | 同上 | テストあり/OK（L39-L48） |
| 未閉じトークン | "Hello {{" | 不明 | 不明 | 未テスト/不明 |
| 空キー | "{{}}" | 不明 | 不明 | 未テスト/不明 |
| 空白含むキー | "{{ name }}" | 不明 | 不明 | 未テスト/不明 |
| エスケープ | "{{{{name}}}}"や"\{{name}}" | 不明 | 不明 | 未テスト/不明 |
| 値に波括弧 | name -> "{X}" | 不明（そのまま出力か） | 不明 | 未テスト/不明 |
| 非ASCIIキー | "{{プロジェクト}}" | 不明 | 不明 | 未テスト/不明 |

既知の潜在的バグ/脆弱性:
- エラーメッセージの一部一致（"missing"）への依存は脆弱。エラー型/メッセージの変更でテストが壊れる。

## Design & Architecture Suggestions

- エラー型の明確化
  - thiserrorで専用エラーenum（例: TemplateError::MissingVariable { name: String }）を定義し、テストはパターンマッチで検証する方が堅牢。
- 欠損キーの扱いオプション
  - 厳格モード（現在の挙動: Err）と寛容モード（未定義はそのまま残す/空文字で置換）の切替を提供するAPIバリアントを用意。
- デリミタ/書式の拡張性
  - デリミタ（"{{", "}}"}）のカスタマイズ、トリム（"{{ name }}"の許容）やエスケープ仕様（"{{{{name}}}}"→"{{name}}"}）の設計を明文化。
- 入力インタフェースの柔軟性
  - varsをIntoIterator<Item=(K,V)> where K: AsRef<str>, V: AsRef<str>に拡張し、借用/所有混在を受け入れるヘルパーを提供。
- パフォーマンス
  - 出力Stringにtemplate.len()程度のcapacity予約。
  - 置換は一回走査で実施（正規表現依存を避ける or Regexはキャッシュ）。
- ドキュメント
  - プレースホルダの仕様（キーに許される文字、空白の扱い、エスケープ、エラー条件）を明記。

## Testing Strategy (Unit/Integration) with Examples

- 既存テストの良点
  - 正常系/異常系の基本をカバーし、同一キー複数回も検証。

- 追加すると有益なユニットテスト
  - 空白を含むキー/トリムの扱い
  - 未閉じ/過剰閉じのトークン（構文エラー）
  - 空キー（"{{}}"}）
  - エスケープ仕様（もしサポートするなら）
  - 非ASCIIキー/値
  - 値に中括弧を含むケース
  - 大規模テンプレート（性能/割当回数の目安）

例:

```rust
#[test]
fn test_unclosed_token_is_error() {
    let template = "Hello {{name";
    let vars = std::collections::HashMap::new();
    let result = substitute_variables(template, &vars);
    // 仕様に応じて: Errを期待
    assert!(result.is_err());
}

#[test]
fn test_whitespace_inside_token_behavior() {
    let template = "Hello, {{ name }}!";
    let mut vars = std::collections::HashMap::new();
    vars.insert("name".to_string(), "Alice".to_string());
    // 設計によってOk("Hello, Alice!")かErrかを決め、テストを固定化
    let out = substitute_variables(template, &vars).unwrap();
    assert_eq!(out, "Hello, Alice!");
}

#[test]
fn test_non_ascii_keys_and_values() {
    let template = "プロジェクト: {{名称}} / 作者: {{作者}}";
    let mut vars = std::collections::HashMap::new();
    vars.insert("名称".to_string(), "蒼穹".to_string());
    vars.insert("作者".to_string(), "山田太郎".to_string());
    let out = substitute_variables(template, &vars).unwrap();
    assert_eq!(out, "プロジェクト: 蒼穹 / 作者: 山田太郎");
}

#[test]
fn test_value_contains_braces_not_interpreted() {
    let template = "Token: {{t}}";
    let mut vars = std::collections::HashMap::new();
    vars.insert("t".to_string(), "{RAW}".to_string());
    let out = substitute_variables(template, &vars).unwrap();
    assert_eq!(out, "Token: {RAW}");
}
```

- テーブルドリブン化（重複削減）

```rust
fn make_vars(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
    pairs.iter().map(|(k,v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn table_driven_substitution() {
    struct Case {
        template: &'static str,
        pairs: &'static [(&'static str, &'static str)],
        expect: Result<&'static str, &'static str>,
    }
    let cases = [
        Case { template: "A {{x}}", pairs: &[("x","1")], expect: Ok("A 1") },
        Case { template: "{{a}} {{a}}", pairs: &[("a","Z")], expect: Ok("Z Z") },
        Case { template: "{{missing}}", pairs: &[], expect: Err("missing") },
    ];
    for c in cases {
        let vars = make_vars(c.pairs);
        let res = substitute_variables(c.template, &vars);
        match (res, c.expect) {
            (Ok(s), Ok(exp)) => assert_eq!(s, exp),
            (Err(e), Err(substr)) => assert!(e.to_string().contains(substr)),
            _ => panic!("unexpected result for template {:?}", c.template),
        }
    }
}
```

## Refactoring Plan & Best Practices

- テストコードの重複削減
  - 変数マップ生成のヘルパーmake_varsを導入。
  - テーブルドリブンテストでケース追加を容易にする。
- エラー検証の堅牢化
  - 文字列一致ではなくエラー型/フィールド（例: MissingVariable{name}）で検証（公開APIが整備され次第）。
- ネーミング/可読性
  - 期待値を定数に分離し、失敗時の差分がわかりやすいようにメッセージを付与。
- 開発支援
  - maplit::hashmap!などのdev-dependencyを用いてマップ初期化を簡潔に（任意）。

## Observability (Logging, Metrics, Tracing)

- ライブラリ関数側（このチャンクには現れない）
  - エラーに文脈を付与（anyhow::Contextやthiserrorで変数名を含める）。
  - tracingでdebugレベルのトレース（検出したトークン数、欠損キー名）をオプション出力。
  - ただし文字列処理でロギングが性能ボトルネックにならないよう、デフォルトは無効で。

- テスト側
  - 失敗時の差分がわかるようassert_eq!にカスタムメッセージを追加する程度で十分。

## Risks & Unknowns

- 不明点（このチャンクには現れない）
  - プレースホルダの厳密仕様（許容文字、空白、エスケープ、未閉じ時の挙動）。
  - エラー型（具象型/enumか、anyhowか）と安定化されたメッセージ仕様。
  - 実装のアルゴリズム（正規表現使用、ワンパススキャナ、ストリーム的処理の有無）。
  - 大規模テンプレート時の性能最適化の有無（capacity予約、再割当削減）。
  - スレッド安全性（グローバル状態がなければ問題ないが確証なし）。

- リスク
  - エラーメッセージ依存のテストは将来の変更に脆弱。
  - 未カバーのトークン異常系（未閉じ等）が実装によってはpanicや予期せぬ成功になる可能性。