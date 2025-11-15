# test_variables.rs Review

## TL;DR

- 目的: 変数のスコープ（Global/Manifest/Local/CLI）の優先順位に基づくマージ動作を検証し、**CLI > Local > Manifest > Global** の上書きルールを確認する。
- 主要API（本ファイルで利用）: **Variables::new, set_global, set_manifest, set_local, set_cli, merge**（定義は別ファイル。シグネチャ詳細は不明）。
- コアロジック: `merge()` の結果は map 的コレクションで、`get(&str) -> Option<&String>` と `is_empty()` を提供することがテストから推定される。
- 複雑箇所: 同一キーが複数スコープに存在する場合の上書き順序と完全性（取りこぼしのない統合）。
- 重大リスク: 実装詳細（データ構造、エラー型、スレッド安全性）がこのチャンクに現れず**不明**。大規模データや競合時の性能・並行性・ロギングも**不明**。
- セキュリティ観点: このチャンクでは危険な操作は見当たらないが、実運用での文字列注入/秘密情報の扱いは**不明**。

## Overview & Purpose

このファイルは、`codanna::profiles::variables::Variables` に対するテスト群で、複数スコープの変数をマージする際の優先順位と期待される結果を検証する。具体的には以下を確認している:

- 空入力の場合は空の結果が得られる。
- Global のみ設定した場合、そのまま結果に反映される。
- Manifest が Global を上書きする。
- Local が Manifest/Global を上書きする。
- CLI が Local/Manifest/Global を上書きする。
- 異なるキーに対して同時に複数スコープが混在する場合でも、期待通りの優先順位で選択される。

このファイル自体は公開APIを提供せず、テスト関数のみを含む。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_merge_empty | private (test) | 変数未設定時の `merge()` 出力が空か検証 | Low |
| Function | test_merge_global_only | private (test) | Global のみ設定時の `merge()` 出力を検証 | Low |
| Function | test_merge_manifest_overrides_global | private (test) | Manifest が Global を上書きすることを検証 | Low |
| Function | test_merge_local_overrides_all | private (test) | Local がすべての下位スコープを上書きすることを検証 | Low |
| Function | test_merge_cli_overrides_all | private (test) | CLI がすべてを上書きすることを検証 | Low |
| Function | test_merge_respects_priority | private (test) | 複数キーでの優先順位の整合性を包括的に検証 | Low |

### Dependencies & Interactions

- 内部依存:
  - 各テスト関数は独立しており、他のテスト関数を呼び出さない。
  - すべてのテストは `Variables` 型のメソッド群（`new`, `set_*`, `merge`）に依存。

- 外部依存（このファイルで使用）:

| 依存名 | 種別 | 用途 |
|-------|------|------|
| codanna::profiles::variables::Variables | 外部モジュール（同プロジェクト内想定） | 変数スコープの管理・マージ |
| std（組込マクロ） | 標準 | `assert!`, `assert_eq!` による検証 |

- 被依存推定:
  - このテストファイルを直接参照する他モジュールはない。`cargo test` 実行時にテストとして読み込まれる。

## API Surface (Public/Exported) and Data Contracts

このファイル自体の公開APIはないが、テストで使用している `Variables` のAPIを列挙する。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| Variables::new | 不明（このチャンクには現れない） | 新規の変数コンテナを生成 | O(1) | O(1) |
| Variables::set_global | 不明（このチャンクには現れない） | Global スコープにキー/値を設定 | O(1) | O(1) |
| Variables::set_manifest | 不明（このチャンクには現れない） | Manifest スコープにキー/値を設定 | O(1) | O(1) |
| Variables::set_local | 不明（このチャンクには現れない） | Local スコープにキー/値を設定 | O(1) | O(1) |
| Variables::set_cli | 不明（このチャンクには現れない） | CLI スコープにキー/値を設定 | O(1) | O(1) |
| Variables::merge | 不明（このチャンクには現れない） | 優先順位に従ってマージ済みのキー/値集合を返す | O(n) | O(n) |

注: Time/Space は一般的なマップ操作を前提とした推定。正確な複雑度は実装依存で、このチャンクには現れない。

### 各APIの詳細説明

1) Variables::new
- 目的と責務:
  - 空の `Variables` コンテナを生成する。
- アルゴリズム:
  - 内部ストレージ（不明）を初期化する。
- 引数:

| 引数 | 型 | 説明 |
|------|----|------|
| なし | なし | コンストラクタ |

- 戻り値:

| 型 | 説明 |
|----|------|
| Variables | 新規インスタンス |

- 使用例:

```rust
let vars = Variables::new();
```

- エッジケース:
  - 初期容量やプリセットの有無は不明。

2) Variables::set_global / set_manifest / set_local / set_cli
- 目的と責務:
  - 指定スコープにキー/値を設定する。後から `merge()` で統合される。
- アルゴリズム:
  - スコープ固有の内部マップに `key -> value` を保存。
- 引数（型は未公開。テストの使用形から推定値を併記）:

| 引数 | 型（不明/推定） | 説明 |
|------|-----------------|------|
| key | 不明（テストでは &str を渡している） | 変数名 |
| value | 不明（テストでは &str を渡している） | 変数値 |

- 戻り値:

| 型 | 説明 |
|----|------|
| なし/不明 | セッタのため通常は `()` の可能性 |

- 使用例:

```rust
let mut vars = Variables::new();
vars.set_global("author", "Global Author");
vars.set_manifest("author", "Manifest Author");
vars.set_local("author", "Local Author");
vars.set_cli("author", "CLI Author");
```

- エッジケース:
  - 同一キーの再設定時の振る舞い（上書き/エラー/累積）は不明。
  - キーや値のバリデーション（空文字/非UTF-8）は不明。

3) Variables::merge
- 目的と責務:
  - 複数スコープを優先順位 **CLI > Local > Manifest > Global** でマージし、最終的なキー/値集合を返す。
- アルゴリズム（推定。コードはこのチャンクに現れない）:
  1. 基本集合として Global をコピー。
  2. Manifest のキーで同名キーを上書き/追加。
  3. Local のキーで同名キーを上書き/追加。
  4. CLI のキーで同名キーを上書き/追加。
- 引数:

| 引数 | 型 | 説明 |
|------|----|------|
| なし | なし | 現在のスコープ状態からマージ |

- 戻り値（テストから推定）:

| 型（推定） | 説明 |
|------------|------|
| Map 的コレクション | `is_empty()` と `get(&str) -> Option<&String>` を提供 |

- 使用例:

```rust
let merged = vars.merge();
assert_eq!(merged.get("author"), Some(&"CLI Author".to_string()));
assert!(!merged.is_empty());
```

- エッジケース:
  - 重複キーの優先順位の適用（テストで検証済み）。
  - スコープ未設定時の挙動（空集合）を返すこと（test_merge_empty で検証）。

データ契約（このチャンクから読み取れる事実）:
- `merge()` 結果は `get(&str)` で `Option<&String>` を返す（テストの書き方から）。
- `merge()` 結果は `is_empty()` を持つ（コレクション互換）。

## Walkthrough & Data Flow

本ファイルのテストが検証するデータフロー（関数単位）。行番号はこのチャンクに明示されないため不明。

- test_merge_empty
  - Flow: `Variables::new` → `merge` → `is_empty() == true`
  - 期待: 空集合。

- test_merge_global_only
  - Flow: `new` → `set_global(author, ...)`, `set_global(license, ...)` → `merge` → `get("author")`, `get("license")`
  - 期待: Global の値がそのまま出力。

- test_merge_manifest_overrides_global
  - Flow: `new` → `set_global(author, ...)`, `set_global(license, ...)` → `set_manifest(author, ...)` → `merge` → `get("author")`, `get("license")`
  - 期待: `author` は Manifest によって上書き、`license` は Global のまま。

- test_merge_local_overrides_all
  - Flow: `new` → `set_global(author, ...)` → `set_manifest(author, ...)` → `set_local(author, ...)` → `merge` → `get("author")`
  - 期待: Local が最優先（CLI以外では最高）。

- test_merge_cli_overrides_all
  - Flow: `new` → `set_global(author, ...)` → `set_manifest(author, ...)` → `set_local(author, ...)` → `set_cli(author, ...)` → `merge` → `get("author")`
  - 期待: CLI が最優先。

- test_merge_respects_priority
  - Flow: それぞれのスコープに `a/b/c/d/e` を配置 → `merge` → 全キーで期待値を検証。
  - 期待: `a=global-a`, `b=manifest-b`, `c=local-c`, `d=cli-d`, `e=cli-e`。

## Complexity & Performance

- `set_*` 系: 一般的にマップ挿入なら **時間 O(1) 平均**, **空間 O(1)**（キー/値追加分）。詳細は実装依存でこのチャンクには現れない。
- `merge()`:
  - 時間: **O(n)**（n=全スコープのユニークキー数。重複上書き含めて合計反復回数）。
  - 空間: **O(n)**（最終マップを構築すると仮定）。
- ボトルネック:
  - 大量キーのコピー/上書き。
  - 値が大型文字列の場合の再割当。
- スケール限界:
  - 単一スレッドでの大規模マージ時に CPU/メモリ消費が増加。
- 実運用負荷要因:
  - I/O/ネットワーク/DB はこのチャンクには現れない。
  - マージ頻度が高い場合は再計算コストが顕著になり得る。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト（このチャンクから推定可能な範囲で評価）:
- メモリ安全性:
  - Buffer overflow / Use-after-free / Integer overflow: このチャンクには現れない。Rust の安全なコレクション使用が期待される。
- インジェクション:
  - SQL / Command / Path traversal: このチャンクには現れない。値は文字列として保存されるのみ。
- 認証・認可:
  - 権限チェック漏れ / セッション固定: 該当なし。
- 秘密情報:
  - Hard-coded secrets / Log leakage: このチャンクには現れない。値が秘密かどうかの扱いは不明。
- 並行性:
  - Race condition / Deadlock: このチャンクには現れない。`Variables` の `Send/Sync` 安全性は不明。

Rust特有の観点:
- 所有権/借用:
  - `merged.get("key") -> Option<&String>` の形から、`merge()` 結果内部の所有データへの不変参照を返していると推測される。所有権の移動は見当たらない。
- ライフタイム:
  - 返却される参照のライフタイムは `merged` に束縛されることが推測される（型定義は不明）。
- unsafe 境界:
  - `unsafe` 使用はこのチャンクには現れない。
- 並行性/非同期:
  - 非同期境界や `await` はなし。`Variables` のスレッド安全性（Send/Sync）は不明。
- エラー設計:
  - セッタ/マージが `Result` を返すかどうかは不明。テストはパニックを誘発する `unwrap/expect` を使用していない。

詳細なエッジケース一覧（推奨検証事項。実装はこのチャンクには現れないため不明）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空入力 | なし | `merge()` が空集合 | `is_empty()` で確認 | テストで確認済み |
| 同一キーの多重定義 | `author` を各スコープに設定 | 優先順位に従って最上位で上書き | `merge()` の結果で確認 | テストで確認済み |
| 未定義キー取得 | `merged.get("unknown")` | `None` を返す | 不明 | 不明 |
| 空文字キー | `set_global("", "v")` | バリデーション方針に従う | 不明 | 不明 |
| 非UTF-8/長大文字列 | バイナリ/巨大値 | エラー/受容の定義 | 不明 | 不明 |
| 大量キー | n ≫ 10^5 | パフォーマンス劣化しない最小限の設計 | 不明 | 不明 |
| クリア/再マージ | 再度 `set_*` 後 `merge()` | 最新状態反映 | 不明 | 不明 |

## Design & Architecture Suggestions

- 優先順位の明示化:
  - スコープ列挙（Global/Manifest/Local/CLI）とその順位を型/定数で明示し、`merge()` がその配列に沿ってレイヤードオーバレイする設計にすると**可読性**と**保守性**が向上。
- データ構造:
  - 各スコープを `HashMap<String, String>` とし、`merge()` は新規 `HashMap` に順に `extend()` → `insert()` で上書きする単純戦略が妥当。
- 監査機能:
  - 上書きイベント（同一キーの競合）を検出し、必要に応じてロギング/イベントフックで観測可能にする。
- APIの明確化:
  - `set_*` のシグネチャを `(&mut self, key: impl Into<String>, value: impl Into<String>)` とするなど、呼び出し側利便性を高める（実装はこのチャンクには現れないため提案レベル）。
- 不変条件:
  - `merge()` の出力は「同一キーは一意」「優先順位に忠実」などの不変条件をドキュメント化。

## Testing Strategy (Unit/Integration) with Examples

- 追加ユニットテスト提案:
  - 未定義キー取得:

```rust
#[test]
fn test_get_unknown_key() {
    let vars = Variables::new();
    let merged = vars.merge();
    assert_eq!(merged.get("unknown"), None);
}
```

  - 再設定の上書き順序（同一スコープ内での再代入）:

```rust
#[test]
fn test_reassign_in_same_scope() {
    let mut vars = Variables::new();
    vars.set_global("k", "v1");
    vars.set_global("k", "v2");
    let merged = vars.merge();
    // 期待動作は実装仕様次第だが通常は後勝ち
    assert_eq!(merged.get("k"), Some(&"v2".to_string()));
}
```

  - 空文字/空値の扱い:

```rust
#[test]
fn test_empty_key_or_value() {
    let mut vars = Variables::new();
    vars.set_global("", "");
    let merged = vars.merge();
    // 仕様により許容/拒否が分かれるため、期待値は決める必要あり
    // ここでは許容の場合の例
    assert_eq!(merged.get(""), Some(&"".to_string()));
}
```

  - 大量キー（パフォーマンス観点のスモーク）:

```rust
#[test]
fn test_large_number_of_keys() {
    let mut vars = Variables::new();
    for i in 0..10_000 {
        vars.set_manifest(&format!("k{i}"), &format!("v{i}"));
    }
    let merged = vars.merge();
    assert_eq!(merged.get("k9999"), Some(&"v9999".to_string()));
}
```

- プロパティベーステスト（`proptest`の利用を推奨）:
  - ランダム生成したスコープ別キー集合に対し、期待順位に基づく参照実装（簡易オーバレイ関数）と `merge()` の結果が一致することを検証。

## Refactoring Plan & Best Practices

- テストの重複削減:
  - ヘルパーを導入し、同一キーを各スコープへ設定する定型処理を共通化:

```rust
fn setup_author_all_scopes(vars: &mut Variables) {
    vars.set_global("author", "Global Author");
    vars.set_manifest("author", "Manifest Author");
    vars.set_local("author", "Local Author");
    vars.set_cli("author", "CLI Author");
}
```

- パラメタライズ（擬似）:
  - マクロやループでキー/スコープ組合せを列挙し一括検証。
- ドキュメント強化:
  - モジュール内で優先順位を Rustdoc に明記し、テスト名に優先順位を含めてわかりやすくする（例: `test_priority_cli_local_manifest_global`）。

## Observability (Logging, Metrics, Tracing)

- ログ:
  - `merge()` 実行時、同一キーの競合と上書き（どのスコープがどのスコープを上書きしたか）を DEBUG ログで記録するとトラブルシューティングが容易。
- メトリクス:
  - マージごとのキー数、競合件数、スコープ別件数をカウント。
- トレーシング:
  - 大規模環境で頻繁に `merge()` が呼ばれる場合、スパンを付与してレイテンシ計測。

このチャンクではログ/メトリクス/トレーシングの実装は現れない。

## Risks & Unknowns

- 実装詳細の不明点:
  - 内部データ構造（`HashMap` 等か）、`merge()` の返却型の正確な型、`set_*` のシグネチャやエラー挙動は**不明**。
- スレッド安全性:
  - `Variables` が `Send/Sync` か、同時アクセス時の保証は**不明**。
- バリデーション:
  - キー/値の形式（空文字、重複、非UTF-8）の許容範囲は**不明**。
- パフォーマンス境界:
  - 大規模データでのメモリ消費・時間計測がないため、スケール時の挙動は**不明**。
- 互換性:
  - 将来の拡張（例: 追加スコープ、テンプレート展開、遅延評価）との整合性は**不明**。

以上の通り、このファイルは `Variables` のマージ優先順位が意図通り機能することを網羅的に確認しているが、実装詳細はこのチャンクには現れないため、API 契約とエッジケースの挙動はドキュメント/コード本体を参照して補完する必要がある。