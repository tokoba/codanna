# profiles\template.rs Review

## TL;DR

- テンプレート文字列内の**{{variable}}**パターンを、与えられた**HashMap<String, String>**から値で置換する単一の公開関数が提供される
- 未定義の変数が参照された場合は**ProfileError::InvalidManifest**を返す（substitute_variables）
- 置換は正規表現で抽出した各一致ごとに全置換を繰り返すため、時間計算量は概ね**O(k · n)**（k=一致数、n=文字列長）
- 再帰的/二段階の置換は行われない（置換後に新たに現れた{{...}}は処理されない）
- **unsafeなし**、所有権・借用は健全だが、正規表現のコンパイルに**expect**使用で理論上はpanic可能（定数パターンのため実質安全）
- セキュリティ上の重大な懸念は少ないが、非常に大規模な入力で**多重コピー**によるパフォーマンス劣化・メモリ負荷があり得る
- 最適化提案: 正規表現の**静的コンパイル**、または**replace_all + 事前検証**や**一回走査の手書きパーサ**で**O(n)**化

## Overview & Purpose

このファイルは、テンプレート文字列に含まれるプレースホルダ「{{name}}」を、与えられたキー/バリューの辞書（HashMap）で置換する機能を提供します。主な目的は、テンプレート生成時に外部コンテキスト（プロフィール・マニフェスト情報等）を注入することです。未定義変数の参照はエラーを返し、誤ったテンプレート使用を早期に検知します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | substitute_variables | pub | テンプレート中の{{var}}を辞書の値で置換し、未定義ならエラーを返す | Low |

### Dependencies & Interactions

- 内部依存
  - このチャンクには他の内部関数・構造体は現れない（不明）。
  - super::error::{ProfileError, ProfileResult} を使用。ProfileError::InvalidManifest { reason: String } バリアントを生成していることがコードから推測可能。

- 外部依存（推奨表）

| クレート/モジュール | 用途 | 備考 |
|--------------------|------|------|
| regex              | {{...}}パターン抽出 | Regex::new と captures_iter を使用 |
| std::collections::HashMap | 変数辞書 | 置換元データの取得 |

- 被依存推定
  - テンプレート適用が必要なプロファイル機能やマニフェスト読み込みロジックから呼ばれる可能性が高い（このチャンクには呼び出し元情報は現れない）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| substitute_variables | pub fn substitute_variables(template: &str, variables: &HashMap<String, String>) -> ProfileResult<String> | テンプレート{{var}}を辞書値で置換し、未定義変数ならエラー | O(k · n) | O(n) |

詳細説明

1) 目的と責務
- テンプレート文字列中の**{{variable}}**を正規表現で抽出し、対応する値で置換する。
- **未定義変数検出**時に**ProfileError::InvalidManifest**を返す。
- 置換後に新たに出現するトークンの再置換は行わない（単一パス設計）。

2) アルゴリズム（ステップ分解）
- template を String にコピーして result を作成
- 正規表現 r"\{\{(\w+)\}\}" をコンパイル
- template に対して captures_iter で一致を列挙
- 各一致から variable 名（capture[1]）を取得
- variables.get(var_name) で値を参照、なければ InvalidManifest エラー
- result = result.replace(full_match, value) で一致全体 "{{var}}" の全出現を置換
- 全一致処理後に Ok(result) を返却

3) 引数

| 引数名 | 型 | 必須 | 意味 |
|-------|----|------|------|
| template | &str | 必須 | 置換対象のテンプレート文字列 |
| variables | &HashMap<String, String> | 必須 | 変数名→値の辞書 |

4) 戻り値

| 型 | 意味 |
|----|------|
| ProfileResult<String> | 成功時は置換後の文字列。失敗時は ProfileError を返す。 |

5) 使用例

```rust
use std::collections::HashMap;
// 仮に super::error がこのスコープにあるとする
// use crate::profiles::template::substitute_variables;

let template = "Hello, {{name}}! Your id is {{id}}.";
let mut vars = HashMap::new();
vars.insert("name".to_string(), "Alice".to_string());
vars.insert("id".to_string(), "42".to_string());

let out = substitute_variables(template, &vars)?;
assert_eq!(out, "Hello, Alice! Your id is 42.");
```

6) エッジケース
- 変数が辞書に存在しない場合はエラー（ProfileError::InvalidManifest）
- "{{}}" のような空名はマッチしない（\w+ のため）
- ハイフン・ドットなど、\w に含まれない文字を含む変数名はマッチしない
- 置換後に新たに発生した "{{var}}" は置換されない（template に対する走査のみ、result は再走査しない）
- 非ASCIIの単語文字（Unicode \w）もマッチ対象（regex クレートは既定で Unicode 対応）

※ 行番号はこのチャンクに明示されていないため、根拠の参照は「関数名:行番号不明」と記載。

## Walkthrough & Data Flow

- 入力: template（&str）、variables（&HashMap<String, String>）
- 初期化: result = template.to_string（所有文字列化）
- パターン準備: Regex::new(r"\{\{(\w+)\}\}") をコンパイル（定数パターン、panic 実質非発生）
- 抽出: pattern.captures_iter(template) で各一致（{{var}}）を列挙
- ルックアップ: var_name を variables から検索、なければ Err(ProfileError::InvalidManifest)
- 置換: result = result.replace(full_match, value) で一致文字列を全置換
- 完了: Ok(result) を返す

データフローは直線的で状態は result のみ更新。辞書アクセスは読み取りのみで副作用なし。

## Complexity & Performance

- 時間計算量
  - マッチ抽出: O(n)（template の走査）
  - 各一致での全置換: O(n) を k 回（k=一致数）実施
  - 合計: **O(k · n)**
- 空間計算量
  - result の作成と置換ごとの新規 String 生成で最大 **O(n)**（複数回の再割当あり）
- 主なボトルネック
  - 一致ごとに全置換を行う繰り返しが非効率（複数の変数/同一変数の多数出現でコスト増）
  - 正規表現のコンパイルが呼び出しごと（小さいが再利用で削減可能）
- スケール限界
  - 非常に長いテンプレート、非常に多くのプレースホルダで CPU とメモリ使用が増大
- 実運用負荷要因
  - 大規模テンプレートの多回呼び出し、同時多数リクエストで GC/アロケーション負荷

最適化案
- 正規表現の**静的初期化**（once_cell::sync::Lazy など）
- **重複変数名の除去**後に置換（HashSet でユニーク名抽出 → O(u · n)）
- **replace_all + 事前検証**で1パス置換（全名が辞書にあることを先に検査）
- さらに高速化するなら**手書きパーサ**で1回走査して出力バッファへ書き出す（O(n)）

## Edge Cases, Bugs, and Security

エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 未定義変数 | "Hello {{name}}" + variables={} | Err(ProfileError::InvalidManifest) | ok_or_else でエラー | 対応済み |
| 空名 | "Hello {{}}" | マッチしない（変更なし） | \w+ により非マッチ | 対応済み |
| 非ASCII名 | "こんにちは、{{名前}}" | Unicode \w なら置換 | regex の \w が Unicode | 対応状況は環境依存 |
| 非単語文字含む名 | "{{user-name}}" | 非マッチ（変更なし） | \w+ のみ許容 | 意図次第 |
| 同一変数の複数出現 | "{{x}} {{x}}" | 両方置換 | 全置換で一度に置換 | 対応済み |
| 置換後に新たな{{...}}出現 | "{{a}}" with a="{{b}}" | "{{b}}"は未置換 | templateのみ走査 | 仕様上の制約 |
| 未閉じトークン | "Hello {{name" | 非マッチ（変更なし） | 正規表現で非マッチ | 対応済み |
| 非常に長い入力 | 数MBのテンプレート | 成功だが遅い | O(k · n) で重い | パフォーマンス課題 |

セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全性により問題なし（unsafe不使用、関数名:行番号不明）
- インジェクション
  - SQL/Command/Path traversal: 純粋な文字列置換のみ。外部システムへの実行はなし。後段がこれをコマンド等に流用する場合は別途エスケープが必要（このチャンクには現れない）。
- 認証・認可
  - 該当なし（このチャンクには現れない）
- 秘密情報
  - Hard-coded secrets: なし
  - Log leakage: ログ出力なし。エラー文言に変数名が含まれるが、通常は安全（機密な値は含まれない）。
- 並行性
  - Race condition / Deadlock: 共有状態なし。関数は純粋関数的でスレッド安全。

潜在的なバグ/懸念
- 正規表現の**expect**による panic 可能性（定数のため実質ゼロだが、API設計上は避けたい）
- パフォーマンス（繰り返しの全置換）による**アロケーション過多**と**CPU負荷**

## Design & Architecture Suggestions

- 正規表現の再利用
  - once_cell::sync::Lazy を用いて Regex を静的に初期化し、毎回のコンパイルを避ける
- 仕様拡張の検討
  - **変数名にハイフン・ドット**などを許容する（正規表現を r"\{\{([^}]+)\}\}" などへ変更。ただしエッジケース増加に注意）
  - **デフォルト値**構文（例: {{var|default}}）サポート（このチャンクには現れないため提案のみ）
  - **ネスト/再帰置換**を許容するか明確化（現実装は単回）
- 置換戦略の改善
  - 事前に全一致の**ユニークな変数名**を抽出・検証してから置換を実施
  - regex::Regex::replace_all + 事前検証で**1パス**化
  - さらなる高速化が必要なら、手書きパーサでテンプレートを1回走査して**O(n)**で構築

参考実装の方向性（概念例、エラー伝播のため二段構成）

```rust
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

static VAR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{\{(\w+)\}\}").unwrap());

pub fn substitute_variables_fast(
    template: &str,
    variables: &HashMap<String, String>,
) -> ProfileResult<String> {
    // 事前検証：すべての変数が存在するか
    for caps in VAR_RE.captures_iter(template) {
        let var = &caps[1];
        if !variables.contains_key(var) {
            return Err(ProfileError::InvalidManifest {
                reason: format!("Variable '{var}' not found in context"),
            });
        }
    }
    // 1パス置換
    let out = VAR_RE.replace_all(template, |caps: &regex::Captures| {
        variables.get(&caps[1]).map(String::as_str).unwrap_or("")
    });
    Ok(out.into_owned())
}
```

## Testing Strategy (Unit/Integration) with Examples

ユニットテスト観点
- 正常系
  - 単一変数置換、複数変数置換、同一変数の複数出現
- 異常系
  - 未定義変数の参照で InvalidManifest が返る
- マッチ境界
  - 空名 "{{}}", 未閉じ "{{x", 非ASCII "{{名前}}", 非単語 "{{user-name}}"
- 置換後の再帰性
  - 値に "{{...}}" を含めても再置換されないことを確認
- パフォーマンス（軽量のベンチマーク）
  - 多数プレースホルダでの処理時間比較（改善案実装との比較）

例テストコード

```rust
#[test]
fn replaces_multiple_variables() {
    use std::collections::HashMap;
    let template = "Hi {{name}}, id={{id}} and {{id}} again.";
    let mut vars = HashMap::new();
    vars.insert("name".to_string(), "Bob".to_string());
    vars.insert("id".to_string(), "007".to_string());
    let out = substitute_variables(template, &vars).unwrap();
    assert_eq!(out, "Hi Bob, id=007 and 007 again.");
}

#[test]
fn error_on_missing_variable() {
    use std::collections::HashMap;
    let template = "Hello {{name}}";
    let vars = HashMap::new();
    let err = substitute_variables(template, &vars).unwrap_err();
    // 具体的なエラー型判定（このチャンクには詳細不明）
    // 文字列メッセージの一部を確認
    let msg = format!("{err}");
    assert!(msg.contains("Variable 'name' not found"));
}

#[test]
fn non_word_variable_not_replaced() {
    use std::collections::HashMap;
    let template = "Hello {{user-name}}";
    let mut vars = HashMap::new();
    vars.insert("user-name".to_string(), "Alice".to_string());
    // マッチしないので置換されない
    let out = substitute_variables(template, &vars).unwrap();
    assert_eq!(out, "Hello {{user-name}}");
}

#[test]
fn no_recursive_replacement() {
    use std::collections::HashMap;
    let template = "A={{a}} B={{b}}";
    let mut vars = HashMap::new();
    vars.insert("a".to_string(), "{{b}}".to_string());
    vars.insert("b".to_string(), "X".to_string());
    let out = substitute_variables(template, &vars).unwrap();
    // A は "{{b}}" に、B は "X" に。A の "{{b}}" は再置換されない。
    assert_eq!(out, "A={{b}} B=X");
}
```

## Refactoring Plan & Best Practices

- 正規表現の**静的初期化**（once_cell::sync::Lazy）でコンパイルコスト削減
- **二段処理**（事前検証 → replace_all）で失敗時の早期終了と1パス置換を両立
- 多回の**String再生成**を避けるため、出力の**容量予約**や**Cow**活用
- 仕様の明確化
  - 変数名の許容文字セット（ASCII限定か、Unicode単語文字か）
  - 未閉じトークンの扱い（現状は「そのまま」だが、厳格にエラーにするか）
  - 再帰置換の要否（現状は非対応）
- エラー文言の一貫性・ローカライズ可能性の確保（reason の標準化）

## Observability (Logging, Metrics, Tracing)

- 現状ロギング等はない
- 提案
  - 未定義変数発生時の**イベントカウント**（メトリクス）でテンプレート品質を観測
  - 大規模テンプレート処理の**処理時間計測**（ヒストグラム）
  - トレースは不要だが、呼び出し元でリクエスト単位のタグ付けがあるとデバッグ容易

## Risks & Unknowns

- このチャンクには ProfileError/Result の完全な定義が現れないため、**データ契約の詳細は不明**
- 呼び出し側の仕様（テンプレートの生成元、変数の命名規則、再帰置換の要件）は**不明**
- 正規表現の Unicode 設定（regex クレートは既定で Unicode 対応だが、プロジェクト設定による変更があるか）も**不明**