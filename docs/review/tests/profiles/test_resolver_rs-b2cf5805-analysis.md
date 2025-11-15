# test_resolver.rs Review

## TL;DR

- 目的: **ProfileResolver**の優先順位ロジック（CLI > local > manifest）の挙動を単体テストで検証する 🧪
- 主要公開API（このチャンクで使用）: **ProfileResolver::new**, **resolve_profile_name**（推定: `Option<String>`×3を受けて`Option<String>`を返す）
- コアロジックの複雑度: 直列優先判定で**O(1)**、分岐は少なく実装は単純
- 重大リスク: 空文字列や無効値の扱い、バリデーション有無が**不明**。テストは優先順位のみを確認しており、入力検証は未網羅 ⚠️
- Rust安全性: `Option<String>`の所有権移動のみで**安全**。`unsafe`なし、並行性なし、パニックなし（`assert_eq!`のみ）
- カバレッジ: 優先順位4パターンを網羅。入力フォーマット・正規化・エラー伝播は未テスト

## Overview & Purpose

このファイルは、外部モジュールの**ProfileResolver**に対して、プロファイル名解決の優先順位ルールを検証するための単体テスト群です。

検証される優先順位:
- CLI指定（`cli_profile`）が最優先
- ローカル設定（`local_profile`）が次点
- マニフェスト（`manifest_profile`）が最後
- 全て未指定の場合は`None`

このチャンクはテストコードのみであり、**本体ロジック（ProfileResolverの実装）**は含まれていません（「このチャンクには現れない」）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_resolve_from_manifest_only | private (test) | manifestのみ指定時にその値が返ることを検証 | Low |
| Function | test_resolve_local_overrides_manifest | private (test) | localがmanifestを上書きすることを検証 | Low |
| Function | test_resolve_cli_overrides_all | private (test) | CLIがlocal/manifestを上書きすることを検証 | Low |
| Function | test_resolve_none_when_empty | private (test) | 全て未指定で`None`になることを検証 | Low |
| Struct | ProfileResolver | pub?（不明、外部） | プロファイル名解決の本体ロジックを提供 | Med（推定） |

### Dependencies & Interactions

- 内部依存
  - 各テスト関数は独立しており、互いの呼び出しはありません
- 外部依存（推定・表）
  | 依存種別 | 名前 | 用途 |
  |----------|------|------|
  | crate/module | codanna::profiles::resolver::ProfileResolver | リゾルバのインスタンス生成、名前解決呼び出し |
  | 標準マクロ | assert_eq! | 期待値検証 |
  | 標準型 | Option<String> | 入力と出力のデータ契約 |

- 被依存推定
  - 本ファイルはテスト専用であり、プロダクションコードから直接参照はされません

## API Surface (Public/Exported) and Data Contracts

このファイル自身に公開APIはありません。以下は「このチャンクで使用されている外部API」の一覧（仕様はテストからの推定）です。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ProfileResolver::new | `fn new() -> ProfileResolver` | リゾルバの初期化 | O(1) | O(1) |
| ProfileResolver::resolve_profile_name | `fn resolve_profile_name(cli: Option<String>, local: Option<String>, manifest: Option<String>) -> Option<String>` | 3つの入力ソースから優先順位により最終プロファイル名を決定 | O(1) | O(1) |

各APIの詳細（推定、根拠: 以下のテストコード引用。正確な行番号はこのチャンクに含まれず不明）:

1) ProfileResolver::new
- 目的と責務
  - 新規の**ProfileResolver**インスタンスを生成する
- アルゴリズム
  - デフォルト状態の初期化（詳細は不明）
- 引数
  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | なし | - | - | - |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | ProfileResolver | リゾルバのインスタンス |
- 使用例
  ```rust
  let resolver = ProfileResolver::new();
  ```
- エッジケース
  - 特になし（初期化失敗の可能性はこのチャンクでは不明）

2) ProfileResolver::resolve_profile_name
- 目的と責務
  - 入力ソース（**CLI**, **local**, **manifest**）のうち、優先順位で有効な最初の値を返す
- アルゴリズム（推定）
  1. `cli`が`Some`ならそれを返す
  2. そうでなければ`local`が`Some`ならそれを返す
  3. そうでなければ`manifest`が`Some`ならそれを返す
  4. それ以外は`None`
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | cli | Option<String> | CLIが指定したプロファイル名 |
  | local | Option<String> | ローカル設定のプロファイル名 |
  | manifest | Option<String> | マニフェスト定義のプロファイル名 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Option<String> | 解決されたプロファイル名。入力がすべて`None`なら`None` |
- 使用例（このファイルのテストより）
  ```rust
  // CLIが最優先
  let resolved = resolver.resolve_profile_name(
      Some("override".to_string()),
      Some("my-custom".to_string()),
      Some("claude".to_string()),
  );
  assert_eq!(resolved, Some("override".to_string()));
  ```
- エッジケース（このチャンクでは不明点あり）
  - `Some("")`（空文字列）の扱い
  - 前後空白の扱い（トリム有無）
  - 大文字小文字の正規化有無
  - 無効な文字を含む場合の扱い
  - 不存在のプロファイル名に対する検証やエラー

## Walkthrough & Data Flow

各テストのデータフローは直線的で、以下のパターンを検証しています（行番号はこのチャンクでは正確に取得不可のため関数単位で記述）。

1) test_resolve_from_manifest_only
```rust
#[test]
fn test_resolve_from_manifest_only() {
    let resolver = ProfileResolver::new();

    let manifest_profile = Some("claude".to_string());
    let local_profile = None;
    let cli_profile = None;

    let resolved = resolver.resolve_profile_name(cli_profile, local_profile, manifest_profile);
    assert_eq!(resolved, Some("claude".to_string()));
}
```
- 入力: CLI=None, local=None, manifest=Some("claude")
- 出力: Some("claude")

2) test_resolve_local_overrides_manifest
```rust
#[test]
fn test_resolve_local_overrides_manifest() {
    let resolver = ProfileResolver::new();

    let manifest_profile = Some("claude".to_string());
    let local_profile = Some("my-custom".to_string());
    let cli_profile = None;

    let resolved = resolver.resolve_profile_name(cli_profile, local_profile, manifest_profile);
    assert_eq!(resolved, Some("my-custom".to_string()));
}
```
- 入力: CLI=None, local=Some("my-custom"), manifest=Some("claude")
- 出力: Some("my-custom")

3) test_resolve_cli_overrides_all
```rust
#[test]
fn test_resolve_cli_overrides_all() {
    let resolver = ProfileResolver::new();

    let manifest_profile = Some("claude".to_string());
    let local_profile = Some("my-custom".to_string());
    let cli_profile = Some("override".to_string());

    let resolved = resolver.resolve_profile_name(cli_profile, local_profile, manifest_profile);
    assert_eq!(resolved, Some("override".to_string()));
}
```
- 入力: CLI=Some("override"), local=Some("my-custom"), manifest=Some("claude")
- 出力: Some("override")

4) test_resolve_none_when_empty
```rust
#[test]
fn test_resolve_none_when_empty() {
    let resolver = ProfileResolver::new();

    let resolved = resolver.resolve_profile_name(None, None, None);
    assert_eq!(resolved, None);
}
```
- 入力: 全てNone
- 出力: None

データはすべて**所有されたString**（`to_string()`）を`Option<String>`で渡し、戻り値も`Option<String>`です。関数呼び出し間に共有状態・副作用は見受けられません（このチャンクでは不明）。

## Complexity & Performance

- 時間計算量: resolveは単純な3段階チェックで**O(1)**。
- 空間計算量: 入出力に`Option<String>`を使うのみで**O(1)**（入力の所有コストは呼び出し側）。
- ボトルネック: 特になし。I/O・ネットワーク・DB非依存。
- スケール限界: なし（値1つの選択問題）。大量呼び出し時もCPU・メモリ負荷は軽微。

## Edge Cases, Bugs, and Security

セキュリティ・安全性チェックリスト（このチャンクで判定可能な範囲）:
- メモリ安全性: 
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（`Option<String>`と安全なAPIのみ）
- インジェクション:
  - SQL / Command / Path traversal: 該当なし（文字列選択のみ）
- 認証・認可:
  - 権限チェック漏れ / セッション固定: 該当なし
- 秘密情報:
  - Hard-coded secrets / Log leakage: 該当なし（ログなし）
- 並行性:
  - Race condition / Deadlock: 該当なし（同期処理のみ）

詳細なエッジケース評価（仕様はテストからの推定。未実装/不明は明記）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 全て未指定 | cli=None, local=None, manifest=None | Noneを返す | resolve_profile_name | テスト済 |
| manifestのみ指定 | cli=None, local=None, manifest=Some("claude") | "claude"を返す | resolve_profile_name | テスト済 |
| localがmanifestを上書き | cli=None, local=Some("my-custom"), manifest=Some("claude") | "my-custom"を返す | resolve_profile_name | テスト済 |
| CLIが全てを上書き | cli=Some("override"), local=Some("my-custom"), manifest=Some("claude") | "override"を返す | resolve_profile_name | テスト済 |
| 空文字列（CLI） | cli=Some(""), local=Some("x"), manifest=Some("y") | 「空は無効ならlocal/yへフォールバック、有効なら""」のどちらか | 不明 | 未テスト |
| 空白のみ | cli=Some("   "), local=None, manifest=None | トリムして空扱いならNone、非トリムなら"   " | 不明 | 未テスト |
| 無効名（記号など） | cli=Some("!@#") | バリデーションによりErr/None/そのまま返すのいずれか | 不明 | 未テスト |
| 大文字小文字差 | cli=Some("Claude"), manifest=Some("claude") | 正規化の有無に依存 | 不明 | 未テスト |
| 同値重複 | cli=Some("a"), local=Some("a"), manifest=Some("a") | "a" | resolve_profile_name | 未テスト（動作は推定通り） |

潜在的なバグ・仕様不明点:
- `Some("")`や空白の扱いが仕様化されていない可能性
- 不正なプロファイル名（存在しない名前）の扱い（検証やエラー化）不明
- 入力の正規化（trim、case-fold）の有無が不明

## Design & Architecture Suggestions

- 入力契約の明確化
  - `Option<String>`の中身に対するバリデーションポリシーを定義（空文字、空白、無効文字、正規化）
  - ドキュメントコメントで優先順位とエッジケースを明示
- API改善
  - 引数型を`Option<impl AsRef<str>>`や`Option<&str>`にすることで不要なアロケーションを削減（呼び出し側で`to_string()`不要）
  - 返り値に「採用ソース（CLI/local/manifest）」のメタ情報を付ける案（例: `enum Source`と`ResolvedProfile { name: String, source: Source }`）
- 設定オブジェクト化
  - 3引数を1つの構造体（例: `ProfileInputs { cli, local, manifest }`）にまとめ、拡張性・可読性向上
- バリデーション・正規化の責務分離
  - 解決関数の前段で入力を正規化・検証するヘルパーを分離し、テスト容易性とSRPを担保

## Testing Strategy (Unit/Integration) with Examples

既存テストは優先順位を十分にカバーしています。以下を追加すると堅牢性が増します。

- 空文字・空白の扱い
  ```rust
  #[test]
  fn test_empty_cli_is_ignored_when_policy_disallows_empty() {
      let resolver = ProfileResolver::new();
      // ポリシー次第で期待値変更。ここでは空文字を無効としてlocalにフォールバックする例。
      let resolved = resolver.resolve_profile_name(Some("".to_string()), Some("local".to_string()), Some("manifest".to_string()));
      // 仕様次第: assert_eq!(resolved, Some("local".to_string()));
  }
  ```

- 正規化（trim/case-fold）の有無
  ```rust
  #[test]
  fn test_whitespace_trim_behavior() {
      let resolver = ProfileResolver::new();
      let resolved = resolver.resolve_profile_name(Some("  dev  ".to_string()), None, None);
      // 仕様次第: トリム有なら Some("dev"), 無なら Some("  dev  ")
  }
  ```

- 無効名・存在確認（存在しないプロファイルへの対応）
  ```rust
  #[test]
  fn test_invalid_profile_name_handling() {
      let resolver = ProfileResolver::new();
      let resolved = resolver.resolve_profile_name(Some("not-found".to_string()), None, None);
      // 仕様次第: NoneやErrを期待
  }
  ```

- 重複同値（安定動作確認）
  ```rust
  #[test]
  fn test_same_value_in_all_sources() {
      let resolver = ProfileResolver::new();
      let resolved = resolver.resolve_profile_name(Some("same".to_string()), Some("same".to_string()), Some("same".to_string()));
      assert_eq!(resolved, Some("same".to_string()));
  }
  ```

- プロパティベーステスト（優先順位が常に保たれることの一般性検証）
  - 乱択で`Option<String>`の組み合わせを作り、`cli > local > manifest`の不変条件をチェック

## Refactoring Plan & Best Practices

- パラメータの型最適化
  - `Option<&str>`や`Option<impl AsRef<str>>`で文字列コピーを削減（呼び出し側は`to_string()`不要）
- 命名の一貫性
  - `resolve_profile_name(cli, local, manifest)`の順序とドキュメントの順序を一致
- ドキュメント強化
  - 優先順位・入力ポリシー・エッジケース例をAPIドキュメントに明記
- エラー設計
  - 「存在しないプロファイル名」などに対して`Result<Option<String>, Error>`を検討（仕様次第）
- テストの拡充
  - 前述の追加テストと、ドキュメントサンプル（doctest）を用意

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 解決過程で「採用ソース（CLI/local/manifest）」と入力値を`debug`レベルでログ出力（PII配慮）
- メトリクス
  - 採用ソース別のカウンタ（例: `profile_resolver_source{source="cli"}`）
- トレーシング
  - `resolve_profile_name`でスパンを開始し、選択分岐をタグ化（`trace`）

このチャンクにはロギング・メトリクス・トレーシングのコードは現れません（テストのみ）。

## Risks & Unknowns

- 空文字列や空白のみの入力の扱いが**不明**
- 入力の正規化（trim, case-fold）有無が**不明**
- 無効名・存在確認・エラー伝播の方針が**不明**
- `ProfileResolver`の内部状態・同期特性（`Send`/`Sync`）は**不明**。ただし本テストでは共有や並行利用はなく、競合は発生しない見込み

追加のRust特有観点（このチャンクで確認可能な範囲）:
- 所有権: `to_string()`で作成した`String`は`Option<String>`に包まれ、`resolve_profile_name`呼び出し時にムーブされる。再利用しないため問題なし
- 借用・ライフタイム: 参照や明示的ライフタイムは登場せず、ライフタイム問題なし
- unsafe境界: **unsafe**は使用されていない
- 並行性・非同期: マルチスレッド・`async`未使用。`Send`/`Sync`境界はこのチャンクでは不明
- エラー設計: `Result`ではなく`Option`で不在を表現。`unwrap/expect`は使用せず、パニックを誘発しない。エラー変換（`From`/`Into`）はこのチャンクには現れない

総じて、このテストファイルは優先順位ロジックの正しさを簡潔に保証しており、追加の入力検証・正規化・観測可能性を組み合わせることで、運用上の堅牢性がさらに高まります。