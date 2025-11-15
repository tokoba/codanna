# io\args.rs Review

## TL;DR

- 提供機能は、Unix風の**key:value**形式と先頭の**位置引数**をパースするユーティリティ。公開APIは4つ（parse_positional_args, get_required_string, get_usize_param, get_string_param）。
- **引用付き値の復元**（シェル分割対策）が実装されており、分割された `"..."` を結合して値を再構成する点が複雑箇所。
- **エラー設計**は軽量で、get_required_stringは存在しない場合に**Result<String, String>**で文字列エラーを返す。数値パラメータはパース失敗時に**デフォルト値へフォールバック**。
- **重大リスク**は低いが、負数や不正な数値入力の扱いが「黙ってデフォルト」になるため、意図しない挙動が起こり得る。ログは**eprintln!**で出力され、ライブラリ利用時の副作用になり得る。
- **メモリ安全性・並行性**の問題は特に無し。unsafe未使用、所有権・借用も安全。スレッド安全性に関する懸念もなし。
- パフォーマンスは実用上十分（O(n)）。ただし、分割引用復元のため**複数文字列コピー**が発生し、巨大な入力では追加コストあり。

## Overview & Purpose

このファイルは、CLIなどでユーザーから受け取る引数群（Vec<String>）を、先頭の**位置引数**（最初の非 key:value 文字列）と、**key:value**のパラメータ群に分解するための**軽量なパーサ**です。さらに、分解後のパラメータから値を取り出すヘルパ群（必須文字列、任意のusize、任意の文字列）を提供します。

特徴:
- `"query:\"foo bar\""` のような**引用付き値**を適切に処理。
- シェルにより引用文字列が分割されたケース（例: `"query:\"error", "handling\""`）を再構成。
- 先頭の非 key:value は**位置引数**として返し、それ以降の非 key:value は**警告**して無視。

対象範囲:
- このチャンクには他モジュールとの統合コードは現れないため、**CLI全体の文脈は不明**。
- ロギングや構成管理、拡張形式（例: `key=value`）などは**未サポート**。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | parse_positional_args | pub | 引数配列を位置引数と key:value HashMap に分解。引用復元を含む | Med |
| Function | get_required_string | pub | 位置引数または params[key] を必須として取得。なければエラー文字列 | Low |
| Function | get_usize_param | pub | params から usize を取得。失敗時はデフォルト値 | Low |
| Function | get_string_param | pub | params から文字列を取得（任意） | Low |
| Module | tests | cfg(test) | 単体テスト群 | Low |

### Dependencies & Interactions

- 内部依存
  - 各 get_* 関数は独立。parse_positional_args の出力を前提として利用されることが多いが、直接呼び出し関係はなし。
- 外部依存（標準）
  - std::collections::HashMap
  
  | 依存 | 用途 |
  |------|------|
  | std::collections::HashMap | key:value の格納 |
  
- 被依存推定
  - CLIエントリポイントやサブコマンド実装が、このモジュールの**parse_positional_args**で引数を分解し、続けて**get_required_string/get_usize_param/get_string_param**で各種値を抽出している可能性が高い（このチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| parse_positional_args | fn parse_positional_args(args: &[String]) -> (Option<String>, HashMap<String, String>) | 位置引数と key:value の分解、引用復元 | O(n + S) | O(k + S) |
| get_required_string | fn get_required_string(positional: Option<String>, params: &HashMap<String, String>, key: &str, error_msg: &str) -> Result<String, String> | 必須文字列の取得（位置引数優先、次にキー） | O(1) | O(1) |
| get_usize_param | fn get_usize_param(params: &HashMap<String, String>, key: &str, default: usize) -> usize | 任意 usize の取得（失敗時デフォルト） | O(1) | O(1) |
| get_string_param | fn get_string_param(params: &HashMap<String, String>, key: &str) -> Option<String> | 任意文字列の取得 | O(1) | O(1) |

注:
- n = 引数数、S = すべての文字列長の合計、k = key:value ペア数。
- HashMap は順序を保持しないため、キーの上書きが起こりうる（最後の値が残る）。

### parse_positional_args

1) 目的と責務
- 入力の String スライスから、最初の非 key:value を**位置引数**として抽出し、それ以外の key:value ペアを**HashMap**に格納。
- 値が引用で開始し、シェルで分割された場合は、**終端の引用**が見つかるまで連結し復元。

2) アルゴリズム（ステップ分解）
- i = 0 から while i < args.len():
  - arg = args[i]
  - if arg に ':' が含まれる:
    - key, value に分割（最初の ':' まで）
    - if value が '"' 始まりかつ '"' 終わりではない:
      - i を進めながら、次のトークンを**空白区切りで連結**し、終端が '"' のトークンで**停止**（または末尾まで）
      - 得られた文字列の両端が '"' なら取り除く
    - else if value が '"' 始まりかつ '"' 終わり:
      - 両端の '"' を除去
    - else:
      - そのまま value を採用
    - params.insert(key, final_value)
  - else if first_positional が未設定:
    - first_positional = arg.clone()
  - else:
    - 追加位置引数は無視し、**eprintln!**で警告
  - i += 1
- 戻り値: (first_positional, params)

3) 引数

| 名前 | 型 | 意味 | 制約 |
|------|----|------|------|
| args | &[String] | CLIやユーザー入力の生引数 | key:value 形式を想定。引用分割はサポート |

4) 戻り値

| 要素 | 型 | 意味 |
|------|----|------|
| .0 | Option<String> | 先頭の非 key:value を位置引数として返す。無ければ None |
| .1 | HashMap<String, String> | key:value ペア（引用復元済み） |

5) 使用例
```rust
use std::collections::HashMap;
use codanna::io::args::parse_positional_args;

let args = vec![
    "search term".to_string(),
    "limit:10".to_string(),
    "kind:function".to_string(),
    "query:\"foo bar\"".to_string(),
];

let (positional, params) = parse_positional_args(&args);
assert_eq!(positional, Some("search term".to_string()));
assert_eq!(params.get("limit"), Some(&"10".to_string()));
assert_eq!(params.get("kind"), Some(&"function".to_string()));
assert_eq!(params.get("query"), Some(&"foo bar".to_string()));
```

6) エッジケース
- 複数の位置引数は**2つ目以降を無視**して警告。
- 終端引用が存在しない場合、**先頭の '"' が残る**可能性がある。
- `limit:` のような**空値**もそのまま格納（空文字）。
- `key:value` で value に引用が含まれていても、**両端の '"' のみ除去**。内部の `:` は許容。
- 重複キーは**最後の値で上書き**。

### get_required_string

1) 目的と責務
- 必須文字列の取得。まず**位置引数**を優先、それが None の場合は**params[key]** を返す。無ければ**error_msg**で Err。

2) アルゴリズム
- positional.or_else(|| params.get(key).cloned()).ok_or_else(|| error_msg.to_string())

3) 引数

| 名前 | 型 | 意味 | 制約 |
|------|----|------|------|
| positional | Option<String> | 位置引数 | None可 |
| params | &HashMap<String, String> | パラメータ群 | key による検索 |
| key | &str | 探すキー | 存在しない可能性 |
| error_msg | &str | 失敗時のエラーメッセージ | 任意 |

4) 戻り値

| 型 | 意味 |
|----|------|
| Result<String, String> | Ok: 値、Err: error_msg |

5) 使用例
```rust
use std::collections::HashMap;
use codanna::io::args::get_required_string;

let positional = None;
let mut params = HashMap::new();
params.insert("query".to_string(), "rust".to_string());

let query = get_required_string(positional, &params, "query", "query is required")?;
assert_eq!(query, "rust".to_string());
# Ok::<(), String>(())
```

6) エッジケース
- 位置引数が Some の場合、params[key] があっても**位置引数が優先**。
- 両方無い場合、**Err(error_msg)**。

### get_usize_param

1) 目的と責務
- usize パラメータの取得。パース失敗やキー欠如時は**default**を返す。

2) アルゴリズム
- params.get(key).and_then(|s| s.parse::<usize>().ok()).unwrap_or(default)

3) 引数

| 名前 | 型 | 意味 | 制約 |
|------|----|------|------|
| params | &HashMap<String, String> | パラメータ群 | key による検索 |
| key | &str | 探すキー | 任意 |
| default | usize | フォールバック値 | 失敗時に使用 |

4) 戻り値

| 型 | 意味 |
|----|------|
| usize | パース結果または default |

5) 使用例
```rust
use std::collections::HashMap;
use codanna::io::args::get_usize_param;

let mut params = HashMap::new();
params.insert("limit".to_string(), "5".to_string());
assert_eq!(get_usize_param(&params, "limit", 10), 5);
assert_eq!(get_usize_param(&params, "missing", 10), 10);
params.insert("limit".to_string(), "abc".to_string());
assert_eq!(get_usize_param(&params, "limit", 10), 10); // パース失敗でデフォルト
```

6) エッジケース
- `-1` や非数値は**デフォルトになり黙って続行**。
- 極端に大きい数値は usize の範囲外なら**デフォルト**。

### get_string_param

1) 目的と責務
- 任意の文字列パラメータを Option で返す（存在しない場合 None）。

2) アルゴリズム
- params.get(key).cloned()

3) 引数

| 名前 | 型 | 意味 | 制約 |
|------|----|------|------|
| params | &HashMap<String, String> | パラメータ群 | key による検索 |
| key | &str | 探すキー | 任意 |

4) 戻り値

| 型 | 意味 |
|----|------|
| Option<String> | Some: 値、None: キーなし |

5) 使用例
```rust
use std::collections::HashMap;
use codanna::io::args::get_string_param;

let mut params = HashMap::new();
params.insert("kind".to_string(), "function".to_string());
assert_eq!(get_string_param(&params, "kind"), Some("function".to_string()));
assert_eq!(get_string_param(&params, "missing"), None);
```

6) エッジケース
- 重複キーが挿入されると**最後の値が返る**。
- 値が空文字でも**Some("")**。

## Walkthrough & Data Flow

全体のデータフローはシンプルです。入力 `&[String]` を走査し、**key:value** と **位置引数**を分離します。key:value の場合は、値が**引用で始まり**かつ**終わりが引用でない**とき、次のトークン群を**空白で連結**して終端引用まで復元します。最後に、位置引数は先頭の非 key:value のみを採用し、それ以降の非 key:value は**警告の上で無視**します。

以下のフローチャートは `parse_positional_args` の主要分岐を示します（parse_positional_args: 行番号不明）。

```mermaid
flowchart TD
  A[Start: i=0, params={}, first_positional=None] --> B{ i < args.len()? }
  B -->|No| Z[Return (first_positional, params)]
  B -->|Yes| C[arg = args[i]]
  C --> D{arg contains ':'?}
  D -->|No| E{first_positional is None?}
  E -->|Yes| F[Set first_positional = arg.clone()]
  E -->|No| G[Warn: Ignoring extra positional via eprintln!]
  F --> H[Increment i] --> B
  G --> H --> B
  D -->|Yes| I[Split into key, value]
  I --> J{value startswith '\"' AND not endswith '\"'?}
  J -->|Yes| K[full_value = value; i += 1]
  K --> L{ i < args.len()? }
  L -->|No| N[Remove quotes if both sides; else keep as is]
  L -->|Yes| M[next_part = args[i]; full_value +=' ' + next_part]
  M --> O{next_part endswith '\"'?}
  O -->|Yes| N
  O -->|No| P[i += 1] --> L
  J -->|No| Q{value startswith '\"' AND endswith '\"' AND len>1?}
  Q -->|Yes| R[final_value = value[1..len-1]]
  Q -->|No| S[final_value = value.to_string()]
  N --> T[If full_value starts & ends with '\"', strip; else keep]
  T --> U[params.insert(key, final_value)]
  R --> U
  S --> U
  U --> H --> B
```

データ契約の要点:
- **HashMap**に格納されるキー・値は**クローンされた所有文字列**（本APIは所有境界が明確）。
- 値の**トリミング**は行わず、**引用の両端のみ除去**。

## Complexity & Performance

- parse_positional_args
  - 時間計算量: O(n + S)（n: 引数数、S: 文字列結合総量）
  - 空間計算量: O(k + S)（k: key:value ペア数、結合結果の文字列長）
  - ボトルネック: 分割引用の復元で**文字列の連結とコピー**が発生。巨大な値や多数の分割があると相応のコスト。
- get_required_string / get_usize_param / get_string_param
  - 時間・空間は O(1)。
- 実運用負荷要因
  - 入力のサイズが大きい場合の文字列結合。
  - 多数の key:value を扱う場合の HashMap 再ハッシュ（ただし通常は軽微）。

## Edge Cases, Bugs, and Security

エッジケース一覧（仕様と実装の整合性を明示）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空引数 | [] | (None, 空Map) | parse_positional_args | OK |
| 位置引数のみ | ["a"] | 位置引数が Some("a"), Map空 | parse_positional_args | OK |
| key:value のみ | ["limit:10"] | 位置引数 None, params["limit"]="10" | parse_positional_args | OK |
| 混在 | ["q", "limit:5"] | 位置引数 "q", params["limit"]="5" | parse_positional_args | OK |
| 分割引用 | ["query:\"error","handling\""] | "error handling" に復元 | parse_positional_args | OK |
| 終端引用なし | ["query:\"no closing"] | 値に先頭 '"' が残る | parse_positional_args | 注意 |
| 追加位置引数 | ["a","b"] | "b"は無視し警告 | parse_positional_args（eprintln!） | OK（副作用） |
| 空値 | ["limit:"] | params["limit"] = "" | parse_positional_args | OK |
| 重複キー | ["limit:1","limit:2"] | 最後で上書き（"2"） | parse_positional_args | OK |
| 非数値 usize | params["limit"]="abc" | デフォルトにフォールバック | get_usize_param | OK（黙示） |
| 負数 usize | params["limit"]="-1" | デフォルトにフォールバック | get_usize_param | OK（黙示） |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全機能により、現実的な懸念はなし。整数パース失敗時はデフォルト。unsafe未使用。
- インジェクション
  - SQL/Command/Path traversal: 該当なし。このモジュールは文字列処理のみ。
  - ログインジェクション: eprintln! にユーザー入力が含まれる可能性があるが、標準エラーへの出力のみで重大性は低い。
- 認証・認可
  - 該当なし（このチャンクには現れない）。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: eprintln! により、**意図しないログ出力**がライブラリ利用時の副作用となり得る。
- 並行性
  - Race condition / Deadlock: 該当なし。共有可変状態なし。

Rust特有の観点（詳細チェックリスト）:
- 所有権
  - 入力は `&[String]` を借用。戻り値は `Option<String>` と `HashMap<String, String>`（所有）。clone は必要最小限（位置引数と値）。
- 借用
  - 値取得は `&HashMap` 経由で行い、返却時に `cloned()` して所有を移すため、借用期間の問題はない。
- ライフタイム
  - 明示的ライフタイムは不要。すべて所有型へ移行するため安全。
- unsafe境界
  - unsafe ブロックは**なし**。
- 並行性・非同期
  - Send/Sync: 使用する型（String, HashMap<String, String>）は基本的に `Send`（ただし HashMap は `Sync` ではない共有参照時のミュータブル利用なし）。本APIは同期関数のみ、共有可変状態を持たないため**データ競合無し**。
  - await境界 / キャンセル: 該当なし。
- エラー設計
  - Result vs Option: 必須値は Result<String, String> で返す。Optional は Option。妥当。
  - panic箇所: **unwrap/expect 不使用**。安全。
  - エラー変換: 文字列エラーを返すため型安全性は限定的。必要なら**独自Error型**に拡張を推奨。

## Design & Architecture Suggestions

- ロギングの抽象化
  - 現状 **eprintln!** を直接使用。ライブラリとしての再利用性向上のため、**ログトレイト**や `log` クレートへの切替を推奨。呼出側でログレベル制御可能に。
- 引用復元のユーティリティ分離
  - 引用処理を関数に抽出（例: `fn reconstruct_quoted(i: &mut usize, args: &[String], initial: &str) -> String`）。テスト容易性・可読性向上。
- イテレータベース実装
  - 手動インデックス `i` の管理よりも、`Peekable` イテレータを用いると意図が明確に。例（参考実装案は下記リファクタリング案参照）。
- エラー報告改善
  - `get_usize_param` の黙示フォールバックは、誤入力の見逃し要因。必要に応じて**警告ログ**や `Result<usize, ParseIntError>` の別バージョンを提供。
- 柔軟なフォーマット
  - 将来的に `key=value` や `--key=value`、`--flag` のサポートを追加するなら、フォーマルなパーサ（nom など）採用や仕様拡張が有用（このチャンクには現れないが設計観点）。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本的なケースを網羅。追加推奨テスト:

- 終端引用無しの挙動確認（先頭 `"` が残る）
```rust
#[test]
fn test_unterminated_quote() {
    let args = vec!["query:\"no closing".to_string(), "limit:3".to_string()];
    let (pos, params) = parse_positional_args(&args);
    assert_eq!(pos, None);
    assert_eq!(params.get("query"), Some(&"\"no closing".to_string())); // 先頭の '"' が残る
    assert_eq!(params.get("limit"), Some(&"3".to_string()));
}
```

- 空値
```rust
#[test]
fn test_empty_value() {
    let args = vec!["limit:".to_string()];
    let (_, params) = parse_positional_args(&args);
    assert_eq!(params.get("limit"), Some(&"".to_string()));
}
```

- 重複キーの上書き
```rust
#[test]
fn test_duplicate_keys() {
    let args = vec!["limit:1".to_string(), "limit:2".to_string()];
    let (_, params) = parse_positional_args(&args);
    assert_eq!(params.get("limit"), Some(&"2".to_string()));
}
```

- usize の不正入力（負数・非数値）
```rust
#[test]
fn test_usize_param_invalid_values() {
    use std::collections::HashMap;
    let mut params = HashMap::new();
    params.insert("limit".to_string(), "-1".to_string());
    assert_eq!(get_usize_param(&params, "limit", 10), 10);

    params.insert("limit".to_string(), "abc".to_string());
    assert_eq!(get_usize_param(&params, "limit", 10), 10);
}
```

- 引用内コロン
```rust
#[test]
fn test_colon_inside_quotes() {
    let args = vec!["query:\"a:b:c\"".to_string()];
    let (_, params) = parse_positional_args(&args);
    assert_eq!(params.get("query"), Some(&"a:b:c".to_string()));
}
```

- 余分な位置引数の警告確認（標準エラー出力を捕捉できる環境で）
  - このチャンクには現れない（テスト環境依存）。可能なら `assert_output` 系ユーティリティで eprintln! を検証。

## Refactoring Plan & Best Practices

- reconstruct_quoted の抽出
```rust
// 概念例：実際の統合は必要に応じて
fn reconstruct_quoted(i: &mut usize, args: &[String], initial: &str) -> String {
    let mut full = initial.to_string();
    *i += 1;
    while *i < args.len() {
        let next = &args[*i];
        full.push(' ');
        full.push_str(next);
        if next.ends_with('"') { break; }
        *i += 1;
    }
    if full.starts_with('"') && full.ends_with('"') && full.len() > 1 {
        full[1..full.len()-1].to_string()
    } else {
        full
    }
}
```

- イテレータ中心の書き換え（可読性向上）
```rust
pub fn parse_positional_args(args: &[String]) -> (Option<String>, std::collections::HashMap<String, String>) {
    use std::collections::HashMap;
    let mut params = HashMap::new();
    let mut first_positional: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if let Some((key, mut value)) = arg.split_once(':') {
            let final_value = if value.starts_with('"') && !value.ends_with('"') {
                // 分割引用の復元
                let reconstructed = reconstruct_quoted(&mut i, args, value);
                reconstructed
            } else if value.starts_with('"') && value.ends_with('"') && value.len() > 1 {
                value[1..value.len()-1].to_string()
            } else {
                value.to_string()
            };
            params.insert(key.to_string(), final_value);
        } else if first_positional.is_none() {
            first_positional = Some(arg.clone());
        } else {
            eprintln!("Warning: Ignoring extra positional argument: {arg}");
        }
        i += 1;
    }
    (first_positional, params)
}
```

- エラーAPIの拡張
  - `get_usize_param_result(params, key) -> Result<usize, ParseIntError>` の追加で**黙示フォールバック**を避けたい利用者への選択肢を提供。
- ログの非同期化・レベル化
  - `log` クレート利用と `warn!` への移行で、**可観測性**と**制御性**を改善。

## Observability (Logging, Metrics, Tracing)

- 現状の観測可能性は**eprintln!**依存で限定的。
- 推奨:
  - **ログレベル**（warn）で余分な位置引数を記録。
  - パース統計（例: パラメータ数、分割引用の復元回数）を**メトリクス**として出力可能にするとデバッグが容易。
  - トレーシングは不要だが、CLI全体での**span**活用（このチャンクには現れない）を検討。

## Risks & Unknowns

- 仕様上の不明点
  - `key=value` や `--flag` 形式のサポート有無は**不明**。このチャンクには現れない。
  - シェル分割の扱いは空白結合に依存。より複雑なケース（エスケープシーケンス、内包引用など）への対応は**不明**。
- 挙動リスク
  - **黙示フォールバック**により、数値の誤入力を見逃す可能性。
  - **ログの副作用**（eprintln!）がライブラリ利用時に望まれない出力を生む可能性。
- スケールリスク
  - 極端に長い引用値や多数の分割を含む入力では、文字列連結により**一時的なメモリ使用量**が増加。通常のCLIでは問題になりにくい。

以上の評価はこのファイルチャンク内のコードに基づくものであり、他コンポーネントとの統合や追加仕様は**このチャンクには現れない**ため、未評価です。