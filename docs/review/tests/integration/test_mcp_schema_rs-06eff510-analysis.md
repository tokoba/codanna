# test_mcp_schema.rs Review

## TL;DR

- 目的: MCP向けリクエスト型（SearchSymbolsRequest/SemanticSearchRequest/AnalyzeImpactRequest）のJSON Schemaに非標準の**"format":"uint"**が混入していないか検証する統合テスト。
- 公開API: 本ファイルは公開APIなし。単一のテスト関数のみ（test_mcp_schema_uint_format）。
- 複雑箇所: 条件分岐は4箇所と少ないが、検出ロジックが「文字列の部分一致」に依存しており堅牢性が低い。
- 重大リスク:
  - to_string_prettyの出力はコロン後にスペースが入るため、現在の検索キー「"format":"uint"」では検出に失敗する可能性が高い（実質的な検出不全）。
  - いかなる状況でもassertせずに成功終了するため、CIで問題を見逃す。
  - usizeの使用はプラットフォーム依存（32/64bit）でスキーマの互換性に影響しうる。
- 推奨修正: 文字列検索ではなくJSONをパースして"format"=="uint"を再帰的に検出し、**assert!**でテストを失敗させる。もしくはto_string（非pretty）に変更し、検索文字列を正す。
- 補足: "uint"はJSON Schema標準のformatではない。APIのフィールドは**u32/u64**等の固定幅整数+最小値制約に置換を検討。

## Overview & Purpose

このファイルは、MCPクライアント（例: Gemini）との互換性に影響する可能性のあるJSON Schemaの**format: "uint"**使用を検知するための統合テストである。対象は以下の3つのリクエスト型（codanna::mcp名前空間）:

- AnalyzeImpactRequest
- SearchSymbolsRequest
- SemanticSearchRequest

テストは、rmcp::schemars::schema_for!マクロで各型のスキーマを生成し、serde_jsonで整形文字列化した上で、**"format":"uint"**という部分文字列の有無をチェック、結果を標準出力に表示する。

狙いは正しいが、現状は「検出が不正確」「失敗をアサートしない」という点で品質上の課題がある。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_mcp_schema_uint_format | private (#[test]) | 3型のSchema生成と"uint"検出の出力 | Low |

### Dependencies & Interactions

- 内部依存:
  - なし（単一テスト関数内で完結）

- 外部依存（推奨表）

| 依存 | 種別 | 用途 | 備考 |
|------|------|------|------|
| codanna::mcp::{AnalyzeImpactRequest, SearchSymbolsRequest, SemanticSearchRequest} | 外部型 | スキーマ生成対象 | 型定義はこのチャンクには現れない |
| rmcp::schemars::schema_for! | マクロ | JSON Schema生成 | schemarsのre-exportと推定 |
| serde_json::to_string_pretty | 関数 | スキーマのJSON整形文字列化 | 現在の検出ロジックの誤検知要因 |
| println! | マクロ | テスト出力 | デフォルトは成功時非表示（--nocaptureが必要） |

- 被依存推定:
  - CI上の統合テストスイートから実行され、スキーマの互換性が壊れていないかの安全網として機能する意図。

## API Surface (Public/Exported) and Data Contracts

本ファイル自体に公開/エクスポートAPIはない。以下はテスト関数の一覧。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| test_mcp_schema_uint_format | fn test_mcp_schema_uint_format() | 3つのMCPリクエスト型のJSON Schemaを生成し"uint" formatの有無を調べる | O(S)（Sは各スキーマ文字列長） | O(S) |

詳細（テスト関数）:
1. 目的と責務
   - 各MCPリクエスト型のスキーマを生成し、非標準のformat値"uint"が含まれていないか検出する。
2. アルゴリズム（ステップ）
   - schema_for!(T)でRootSchemaを生成
   - to_string_prettyで整形JSON文字列に変換
   - 生成文字列に対して"format":"uint"の部分一致検索
   - 警告をprintlnで出力
   - 最後に3結果をORしてサマリ出力
3. 引数
   - なし
4. 戻り値
   - なし（副作用: printlnによる標準出力）
5. 使用例
```rust
// 通常は `cargo test --test test_mcp_schema` で実行
// 生成・検出の核となる手順（概念例）
let schema = rmcp::schemars::schema_for!(SearchSymbolsRequest);
let json = serde_json::to_string_pretty(&schema).unwrap();
let has_uint = json.contains(r#""format":"uint"#); // 現状はここが脆い
```
6. エッジケース
   - pretty出力のコロン後スペースで検出失敗
   - "uint64"等の類似フォーマットを検出できない
   - "format"が存在せず、代わりに"minimum":0で非負整数が表現されるケースの未検出
   - いずれも「このチャンクには現れない」スキーマ仕様次第

データ契約（型のフィールド等）はこのチャンクには現れない。不明。

## Walkthrough & Data Flow

処理シーケンス（直線的）:
- SearchSymbolsRequestのスキーマ生成 → 整形JSON化 → "uint"検出（条件分岐1）
- セパレータ出力
- SemanticSearchRequestのスキーマ生成 → 整形JSON化 → "uint"検出（条件分岐2）
- セパレータ出力
- AnalyzeImpactRequestのスキーマ生成 → 整形JSON化 → "uint"検出（条件分岐3）
- 3つの検出結果をORして集計 → サマリ出力（条件分岐4）

Mermaidフローチャート（条件分岐4つ以上のため作成）:

```mermaid
flowchart TD
  S([Start]) --> A1[Schema for SearchSymbolsRequest]
  A1 --> B1[to_string_pretty]
  B1 --> C1{contains "format":"uint"?}
  C1 -- Yes --> W1[Print warning (SearchSymbols)]
  C1 -- No --> N1[Skip warning (SearchSymbols)]

  N1 --> SEP1[Print separator]
  W1 --> SEP1

  SEP1 --> A2[Schema for SemanticSearchRequest]
  A2 --> B2[to_string_pretty]
  B2 --> C2{contains "format":"uint"?}
  C2 -- Yes --> W2[Print warning (SemanticSearch)]
  C2 -- No --> N2[Skip warning (SemanticSearch)]

  N2 --> SEP2[Print separator]
  W2 --> SEP2

  SEP2 --> A3[Schema for AnalyzeImpactRequest]
  A3 --> B3[to_string_pretty]
  B3 --> C3{contains "format":"uint"?}
  C3 -- Yes --> W3[Print warning (AnalyzeImpact)]
  C3 -- No --> N3[Skip warning (AnalyzeImpact)]

  N3 --> AGG[Aggregate: has_uint = any]
  W3 --> AGG

  AGG --> C4{has_uint?}
  C4 -- Yes --> SUMW[Print summary: ❌ contains 'uint']
  C4 -- No --> SUMG[Print summary: ✅ none]
  SUMW --> E([End])
  SUMG --> E([End])
```

上記の図は`test_mcp_schema_uint_format`関数（このチャンク全体）の主要分岐を示す。

## Complexity & Performance

- 時間計算量: O(S)（Sは各スキーマJSON文字列長）。文字列化と部分一致検索が支配的。
- 空間計算量: O(S)（整形JSON文字列の保持）。
- ボトルネック:
  - serde_json::to_string_prettyによる整形出力は非整形出力よりわずかに高コスト。
  - ただし統合テストとして許容範囲。実運用I/O/ネットワーク/DBは関与しない。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト:
- メモリ安全性: 安全なRustのみ。unsafeなし。Buffer overflow / UAF / 整数オーバーフローの懸念はなし。
- インジェクション: 外部入力を処理せず、SQL/Command/Path traversalの懸念なし。
- 認証・認可: 該当なし。
- 秘密情報: Hard-coded secretsなし。ログに秘密情報を出す恐れも低い（スキーマのみ）。
- 並行性: 並列化なし。Race/Deadlockの懸念なし。

既知/推定バグ:
- 検出ロジックの誤検知:
  - to_string_prettyはJSONで「"format": "uint"」とコロン後にスペースを挿入するのが標準的。現在の検索文字列は「"format":"uint"」でスペース非対応。結果として**常に未検出（偽陰性）**になる可能性が高い。
- テストが常に成功:
  - assertを用いていないため、"uint"が含まれてもテストはパスし、CIで見逃す。
- 仕様バリエーション未対応:
  - "uint64"や"uint32"など派生表現、あるいはformatを使わず"minimum":0で表現される非負整数を検出できない。
- 出力の可視性:
  - printlnは成功時にデフォルトでは非表示。-- --nocaptureを指定しないと気付きにくい。

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| prettyのスペース | 出力: `"format": "uint"` | 検出される | 文字列検索が`"format":"uint"`でスペース非対応 | 問題あり（偽陰性） |
| formatの亜種 | `"format": "uint64"` | 検出（少なくとも警告） | 厳密一致しか見ない | 未対応 |
| 非format表現 | `"type":"integer","minimum":0` | 必要に応じて検討（許容/非許容の設計） | formatのみ検出 | 未対応 |
| スキーマ巨大化 | 大規模オブジェクト | 正常に完了 | O(S)処理 | 概ね許容 |
| JSON生成失敗 | serdeエラー | テスト失敗 | unwrap()でpanic | 期待通りだがexpect推奨 |

Rust特有の観点（このチャンクで確認可能な範囲）:
- 所有権/借用/ライフタイム: 短命のローカル変数のみ。複雑な借用や明示ライフタイムは不要。問題なし。
- unsafe境界: なし。
- 並行性/非同期: 非該当。
- エラー設計:
  - unwrap()使用。テストでは許容されるが、失敗時の診断性向上のためexpect("...")を推奨。
  - Result/Optionの使い分けは不問。

## Design & Architecture Suggestions

- 検出ロジックを堅牢化
  - 文字列検索ではなく、serde_json::Valueへパースして再帰的に`"format"=="uint"`を探索するヘルパを実装。
  - あるいはschemarsの型（RootSchema）を直接走査する方法でも可読性が高い。
- テストを失敗させる
  - `assert!(!has_uint)`でCIにシグナルを伝える。
  - メッセージに対象型名と該当箇所を含める。
- DRY化
  - ジェネリック関数やマクロで「スキーマ生成→検出→アサート」を共通化。
- 仕様の明文化
  - 何を「非標準」と見なすかをREADME/コメントに明示（例: "format":"uint"は不可、"integer"+"minimum":0は可、等）。
- usizeの撤廃
  - 公開APIのフィールドから**usize**を排除し、**u32/u64**へ統一（プラットフォーム非依存化）。

## Testing Strategy (Unit/Integration) with Examples

- 改善版：JSONパース方式での検出＋アサート
```rust
use serde_json::Value;

fn contains_format_uint(v: &Value) -> bool {
    match v {
        Value::Object(map) => {
            if map.get("format") == Some(&Value::String("uint".into())) {
                return true;
            }
            map.values().any(contains_format_uint)
        }
        Value::Array(arr) => arr.iter().any(contains_format_uint),
        _ => false,
    }
}

fn assert_no_uint_format<T: ?Sized + schemars::JsonSchema>(name: &str) {
    let schema = rmcp::schemars::schema_for!(T);
    let json = serde_json::to_value(&schema).expect("schema to JSON");
    assert!(
        !contains_format_uint(&json),
        "Schema for {name} contains non-standard format \"uint\".\n\
         Consider changing usize to u32/u64 and/or removing format."
    );
}

#[test]
fn test_mcp_schema_uint_format() {
    assert_no_uint_format::<codanna::mcp::SearchSymbolsRequest>("SearchSymbolsRequest");
    assert_no_uint_format::<codanna::mcp::SemanticSearchRequest>("SemanticSearchRequest");
    assert_no_uint_format::<codanna::mcp::AnalyzeImpactRequest>("AnalyzeImpactRequest");
}
```

- 代替の簡易修正（文字列法を継続する場合）
  - prettyではなく`to_string`を使う、または検索文字列を`"format": "uint"`（スペースあり）に変更。
  - ただし、順序/空白依存は脆弱なため非推奨。

- スナップショットテスト（任意）
  - insta等でスキーマ全体をスナップショット化し回帰検出を容易にする。
```rust
// 例: instaを使う場合（依存追加が必要）
let schema = rmcp::schemars::schema_for!(codanna::mcp::SearchSymbolsRequest);
let json = serde_json::to_string_pretty(&schema).unwrap();
insta::assert_snapshot!("search_symbols_schema", json);
```

- 実行ヒント
  - stdout確認: `cargo test --test test_mcp_schema -- --nocapture`
  - ただし最終的にはassertで失敗させる方がCIには有効。

## Refactoring Plan & Best Practices

- ヘルパの抽出
  - `contains_format_uint(Value)`と`assert_no_uint_format<T>`をユーティリティに分離し、他のスキーマ検査にも再利用。
- マクロ化
  - 複数型に対する同一検査を`macro_rules!`で簡潔に表現。
- 明示的な失敗メッセージ
  - どの型で失敗したか、可能なら該当サブツリーをダンプして原因追跡を容易に。
- expectの使用
  - `unwrap()` → `expect("...")`で失敗時の文脈を強化。

## Observability (Logging, Metrics, Tracing)

- 現状は`println!`のみ。成功時は出力が見えないため、失敗時に情報が十分得られるよう以下を推奨:
  - 失敗メッセージに対象型名、該当formatキー周辺のJSON断片を含める。
  - ログを使う場合は`log`/`tracing`と`test`用の初期化を検討（ただしテストではassertで十分なことが多い）。
  - スナップショット併用で差分可視化。

## Risks & Unknowns

- 対象型の詳細（フィールド、usizeの有無）はこのチャンクには現れない。不明。
- rmcp::schemarsの具体的バージョン/挙動（format付与のポリシー）も不明。
- "uint"以外の非標準formatが存在するかは不明。検出ポリシーの拡張が必要な可能性。
- プラットフォーム差（usize幅）により生成スキーマが変動するリスク。固定幅整数が望ましい。