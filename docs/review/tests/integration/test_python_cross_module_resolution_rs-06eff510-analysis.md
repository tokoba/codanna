# integration\test_python_cross_module_resolution.rs Review

## TL;DR

- 目的: Pythonコードの関数呼び出しを、**単純名**と**完全修飾モジュールパス**の両方で解決できるかを検証する統合テスト。
- 主要API: **PythonParser::find_calls**、**PythonResolutionContext::add_symbol**、**PythonResolutionContext::resolve**（いずれも外部クレートcodannaのAPI）。
- 複雑箇所: パーサは呼び出し先を「単純名」で抽出する一方、解決対象は「完全修飾名」であるケースがある不一致をどう吸収するか。
- 重大リスク: シンボルの登録が「名前」だけだと「完全修飾パス」を解決できない。テストはこのバグを再現し、修正（両方のキーで登録）を検証。
- Rust安全性: unsafeなし。unwrap/expectによる**panic**があるがテストとしては許容。並行性は無し。
- セキュリティ: 入出力限定で外部I/Oなし。インジェクションや秘密情報漏洩の懸念は低い。
- パフォーマンス: パーサは入力長に比例、解決は典型的にO(1)（推定）。本テストは軽量。

## Overview & Purpose

このファイルは、Pythonのクロスモジュール関数呼び出し解決に関する挙動を検証するための**統合テスト**を2本含みます。

- test_python_cross_module_call_resolution_step_by_step: 「単純名」での解決と、「完全修飾モジュールパス」での解決の両方が可能であることを段階的に検証。
- test_python_resolution_shows_the_problem: 過去の問題（「単純名」でしか解決できない）を再現し、修正の効果が出ているかを示す。

本テストは、codannaクレートのPythonパーサと解決コンテキストのAPI利用例を通じて、インデックス済みシンボルの解決キー（名前・モジュールパス）を併用すべきであることを示します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_python_cross_module_call_resolution_step_by_step | private (test) | 単純名と完全修飾パスの両方で解決できることを段階的に検証 | Med |
| Function | test_python_resolution_shows_the_problem | private (test) | 問題（完全修飾パスが解決不可）を再現し、修正確認を補助 | Low |

### Dependencies & Interactions

- 内部依存
  - 両テストは独立。共有状態なし、グローバル変更なし。
- 外部依存（codannaクレート）
  | モジュール/型 | 用途 | 備考 |
  |---------------|------|------|
  | parsing::python::parser::PythonParser | Pythonコードから呼び出し関係抽出 | `find_calls`を使用 |
  | parsing::python::resolution::PythonResolutionContext | シンボル解決のためのコンテキスト | `add_symbol`, `resolve`を使用 |
  | parsing::{LanguageParser, ResolutionScope, ScopeLevel} | インタフェースとスコープ種別 | LanguageParser/ResolutionScopeは未使用、ScopeLevel::Global使用 |
  | {FileId, Range, Symbol, SymbolId, SymbolKind, Visibility} | シンボル構築・識別 | テスト用ダミーシンボル生成 |
- 被依存推定
  - このファイルは統合テストのみ。利用者はRustのテストランナー（cargo test）。本モジュールを参照するアプリコードは「該当なし」。

## API Surface (Public/Exported) and Data Contracts

公開API（このファイルからエクスポート）は「該当なし」。以下は本テストが利用する外部APIの一覧（型は使用状況からの推定。厳密なシグネチャはこのチャンクには現れない）。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PythonParser::new | -> Result<PythonParser, Err>（推定） | パーサ生成 | O(1) | O(1) |
| PythonParser::find_calls | (&str) -> Vec<(String, String, Range)>（推定） | コード中の関数呼び出し抽出（from, to, range） | O(n)（入力長） | O(k)（検出件数） |
| PythonResolutionContext::new | (FileId) -> PythonResolutionContext | 解決用コンテキスト生成 | O(1) | O(1) |
| PythonResolutionContext::add_symbol | (String, SymbolId, ScopeLevel) -> () | 解決キーにシンボルを登録 | O(1)（推定: HashMap） | O(1) |
| PythonResolutionContext::resolve | (&str) -> Option<SymbolId> | キーに対応するシンボルIDを返す | O(1)（推定） | O(1) |
| Symbol::new | (SymbolId, &str, SymbolKind, FileId, Range) -> Symbol | シンボル生成 | O(1) | O(1) |
| SymbolId::new | (u64/usize?) -> Result<SymbolId, Err>（不明） | シンボルID生成 | O(1) | O(1) |
| FileId::new | (u64/usize?) -> Result<FileId, Err>（不明） | ファイルID生成 | O(1) | O(1) |
| Range::new | (i32?, i32?, i32?, i32?) -> Range（不明） | 位置情報生成 | O(1) | O(1) |

各APIの詳細（このチャンクから観測できる範囲のみ）

1) PythonParser::find_calls
- 目的と責務
  - Pythonコードから関数呼び出しを抽出し、呼び出し元・呼び出し先・範囲（位置）を返す。   - 本テストでは「単純名（process_data）」で抽出されることを確認。
- アルゴリズム（推定）
  - トークン化/構文解析を行い、関数呼び出し式を検出。
  - importにより導入された名前を追跡（属性アクセスは追跡しない旨がコメントあり）。
- 引数
  | 名称 | 型 | 説明 |
  |------|----|------|
  | code | &str | Pythonソース文字列 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(String, String, Range)>（推定） | (from, to, range)。toは「単純名」想定 |
- 使用例
  ```rust
  let mut parser = PythonParser::new().unwrap();
  let calls = parser.find_calls(code);
  assert_eq!(calls.len(), 1);
  let (_from, to, _range) = &calls[0];
  assert_eq!(*to, "process_data");
  ```
- エッジケース
  - import alias（as）による名称変更
  - 属性呼び出し（module.func()）の非対応（コメント: L28-L29相当、行番号は不明）
  - 同一関数名の複数定義による曖昧性

2) PythonResolutionContext::add_symbol
- 目的と責務
  - 解決用のキー（名前、または完全修飾モジュールパス）とSymbolIdを対応付ける。
- アルゴリズム（推定）
  - スコープ別のテーブル（HashMap相当）にキーを追加。
- 引数
  | 名称 | 型 | 説明 |
  |------|----|------|
  | name_or_path | String | 解決キー（単純名 or 完全修飾パス） |
  | id | SymbolId | 対応するシンボルID |
  | level | ScopeLevel | スコープ（Globalなど） |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例
  ```rust
  context.add_symbol("process_data".to_string(), process_data_id, ScopeLevel::Global);
  // または
  context.add_symbol("app.utils.helper.process_data".to_string(), process_data_id, ScopeLevel::Global);
  ```
- エッジケース
  - 既存キーへの再登録（上書き or 衝突扱いかは不明）
  - 異なるScopeLevel間で同名キー
  - 空文字キーの扱い

3) PythonResolutionContext::resolve
- 目的と責務
  - 与えられたキー（&str）が登録済みのSymbolIdに解決できるかを返す。
- アルゴリズム（推定）
  - スコープ順序に従って検索（単一Globalのみなら直接Map照会）。
- 引数
  | 名称 | 型 | 説明 |
  |------|----|------|
  | key | &str | 解決したい名前または完全修飾パス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Option<SymbolId> | 見つかった場合はSome、未登録ならNone |
- 使用例
  ```rust
  let resolved = context.resolve("process_data"); // Some(SymbolId(42))
  let resolved_full = context.resolve("app.utils.helper.process_data"); // Some(SymbolId(42))（修正後）
  ```
- エッジケース
  - 同名・異スコープの競合
  - 大文字小文字の差異（Pythonはケースセンシティブ）
  - 前方一致/後方一致ではなく完全一致であるべき

このファイル内での`LanguageParser`と`ResolutionScope`の直接使用は「該当なし」。

## Walkthrough & Data Flow

- ステップ1（シンボル生成）
  - ダミーの`SymbolId(42)`と`Symbol`を作成。`module_path = Some("app.utils.helper.process_data")`、`visibility = Public`に設定。
- ステップ2（パース）
  - Pythonコード（importして直接`process_data()`呼び出し）を`PythonParser`で解析。
  - 抽出された呼び出しは1件で、呼び出し先`to`は「process_data」（単純名）。
- ステップ3（解決コンテキスト構築）
  - `PythonResolutionContext::new(FileId(2))`を生成し、従来どおり「単純名」で登録。
- ステップ4（単純名の解決）
  - `resolve("process_data")`が`Some(SymbolId(42))`になることを確認。
- ステップ5（完全修飾パスの解決の検証・修正）
  - `module_path`でも同じ`SymbolId(42)`を追加登録。
  - `resolve("app.utils.helper.process_data")`でも`Some(SymbolId(42))`になることを確認。
- 問題再現テスト（2つ目のテスト）
  - 「単純名」でのみ登録し、「完全修飾パス」での`resolve`が`None`となる既存の問題を再現。
  - 修正が入っているなら`Some`になることをメッセージで示す（アサーションは短名に対してのみ）。

データフローまとめ
- 入力: Pythonコード文字列。
- 中間: パーサが抽出する呼び出しタプル（from, to, range）。
- 状態: 解決コンテキストにキー→SymbolIdの対応を追加（単純名、完全修飾パス）。
- 出力: `resolve(&str) -> Option<SymbolId>`。

根拠（関数名:行番号）
- 「パーサは単純名を抽出」: test_python_cross_module_call_resolution_step_by_step（行番号は不明、`assert_eq!(*to, "process_data")`付近）
- 「単純名で解決成功」: 同上（`assert_eq!(resolved, Some(process_data_id))`付近）
- 「完全修飾パスでも解決成功（修正後）」: 同上（`assert_eq!(resolved_full, Some(process_data_id))`付近）
- 「問題再現」: test_python_resolution_shows_the_problem（`resolve(call_target)`がNoneの可能性をログ出力）

## Complexity & Performance

- PythonParser::find_calls
  - 時間: O(n)（n=入力コード長。推定）
  - 空間: O(k)（k=抽出された呼び出し数。推定）
- PythonResolutionContext::{add_symbol, resolve}
  - 時間: O(1)（推定。内部がHashMapの場合）
  - 空間: O(m)（m=登録シンボル数）
- テスト全体
  - 非常に軽量。I/Oやネットワーク、DBアクセスなし。
- ボトルネック/スケール限界
  - 大規模コード解析時には`find_calls`の線形スキャンが支配的。
  - 大規模なシンボル表では解決コンテキストのメモリ使用量増加。衝突時の解決規則が複雑化する可能性。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト
- メモリ安全性: unsafe使用なし。コンストラクタでunwrap使用によりpanic可能（テストでは許容）。
- インジェクション: SQL/Command/Path traversalなし。外部入力を評価しないためリスク低い。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密なし。ログに秘密情報は含まれない。
- 並行性: 共有可変状態なし。Race/Deadlockの懸念なし。

詳細エッジケース

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 未登録シンボルの解決 | resolve("unknown") | None | ？ | 不明 |
| 完全修飾パスのみ登録 | add_symbol("a.b.c.d", id) → resolve("a.b.c.d") | Some(id) | あり（本テストで検証） | 確認済み（テストで想定どおり） |
| 単純名のみ登録 | add_symbol("process_data", id) → resolve("a.b.c.process_data") | None（修正前）/ Some（修正後） | あり（再現テスト） | 修正の有無に依存 |
| 重複キー登録（上書き挙動） | 同じキーで異なるidをadd | 最後勝ち or エラー | ？ | 不明 |
| ScopeLevel差による競合 | GlobalとLocalに同名登録 | スコープ優先規則に従う | ？ | 不明 |
| import alias | from x import y as z; resolve("z") | yへ解決 | パーサ/解決の対応状況不明 | 不明 |
| 属性呼び出し解析 | module.func() | to="func" or "module.func" | コメントで非対応と明記 | 既知の制約 |
| 大文字小文字差 | "Process_Data" vs "process_data" | ケース区別 | 実装方針不明 | 不明 |

Rust特有の観点
- 所有権/借用
  - `let calls = parser.find_calls(code);`の後、`for (from, to, _) in &calls { ... }`で不変借用。`calls`のライフタイム内で安全。
  - `let (_from, to, _range) = &calls[0];`で要素への参照を保持し、後続で`resolve(call_target)`へ`&str`を渡す（`&String`からのDeref）。安全。
- ライフタイム
  - 明示的ライフタイムパラメータ不要。参照は関数スコープ内に限定。
- unsafe境界
  - unsafeブロックなし。
- 並行性・非同期
  - Send/Syncに関する要求なし。同期コードのみ。
- エラー設計
  - `unwrap`/`assert_eq!`を使用（テストでは妥当）。本番コードでは適切なエラー伝播が望ましい。

## Design & Architecture Suggestions

- 解決キーの正規化
  - パーサが「単純名」を返し、解析対象が「完全修飾パス」を含む場合に備え、**解決コンテキストで両方のキーを受け付ける二重登録**（単純名/完全修飾）を標準化。
- キー管理の方針
  - 完全修飾パスを正とし、単純名はエイリアスとして同一`SymbolId`へリンク。重複/衝突時の規則（最後勝ち、エラー、警告）を明確化。
- スコープ活用
  - `ResolutionScope`/`ScopeLevel`の優先規則をドキュメント化し、同名解決時の一貫性を保証。
- パーサ拡張
  - 余力があれば属性呼び出し（`module.func()`）の追跡やimport alias対応を追加。解決側の正規化と相互補完。
- データ構造
  - 内部実装がHashMap想定で問題ないが、**複数キー→1 ID**の関係を持つためインデックス構造（2本のMapまたは多値辞書）を明示。

## Testing Strategy (Unit/Integration) with Examples

- 既存の統合テスト
  - 単純名・完全修飾パスの双方の解決を検証。
  - 問題の再現と修正確認を目的としたデモテスト。
- 追加すべきテスト
  - alias対応
    ```rust
    // 例（パーサ/解決が対応している前提）
    let code = r#"from app.utils.helper import process_data as pd
    def handle_request():
        pd()
    "#;
    // find_callsが"pd"を抽出し、add_symbolで"pd"とFQNを両登録 → resolve("pd")/resolve(FQN)が成功
    ```
  - 属性呼び出し（現状非対応のため、期待値を明確にしたスキップ/失敗テスト）
  - 重複キー登録時の挙動（最後勝ち/エラー）を検証
  - ScopeLevelが異なる場合の解決優先度テスト
- 例（本ファイルの短い抜粋）
  ```rust
  #[test]
  fn test_python_resolution_shows_the_problem() {
      let symbol_id = SymbolId::new(42).unwrap();
      let mut context = PythonResolutionContext::new(FileId::new(1).unwrap());
      context.add_symbol("process_data".to_string(), symbol_id, ScopeLevel::Global);
      let call_target = "app.utils.helper.process_data";
      let result = context.resolve(call_target);
      assert!(result.is_none(), "修正前はNoneを確認");
      assert_eq!(context.resolve("process_data"), Some(symbol_id));
  }
  ```

## Refactoring Plan & Best Practices

- 解決登録APIの改善
  - 高水準API例: `context.add_symbol_with_aliases(fqn: &str, simple: &str, id: SymbolId, scope: ScopeLevel)`で同時登録。
- キー正規化ユーティリティ
  - FQN→単純名抽出、単純名→FQN生成（インポート情報を活用）のヘルパー化。
- エラー処理
  - テスト以外では`unwrap`を避け、明示的な`Result`伝播とコンテキスト付きエラー（thiserror/anyhow）を利用。
- ドキュメント整備
  - パーサが返す「to」の仕様（単純名かFQNか）を明確化し、解決側の期待仕様と整合させる。
- ベストプラクティス
  - 重複/衝突時のルールを統一。ログで警告を残す。
  - ケースセンシティブの扱いを一貫化。

## Observability (Logging, Metrics, Tracing)

- 現状
  - `println!`でテスト進捗を出力。検証には十分だが、冗長になりがち。
- 提案
  - ライブラリ側は`tracing`クレートで**debug**/**info**を適切に出力し、テストでは`env_logger`/`tracing-subscriber`で制御。
  - メトリクス例
    - 登録キー数、衝突数、解決成功率、非解決率。
  - トレース
    - 解決キーの探索パス（どのスコープを検索したか）をイベントとして記録。

## Risks & Unknowns

- Unknown（このチャンクには現れない）
  - `PythonParser::find_calls`の厳密な戻り値型と仕様。
  - `PythonResolutionContext`内部のデータ構造（HashMapか、スコープ付き多重Mapか）。
  - `ResolutionScope`の役割・優先順位の仕様。
  - 既知修正がライブラリに恒久的に反映済みかどうか（テストメッセージでは修正前後を区別）。
- リスク
  - 大規模コードベースでの衝突/曖昧性増加。
  - パーサと解決の仕様不一致による誤解決/未解決。
  - alias・属性呼び出し非対応がユーザー期待に反する場合のUX低下。

以上により、本テストは「単純名と完全修飾パスの両対応」という設計方針の妥当性を確認する要点を押さえています。実運用に向けては、キー正規化・エイリアス管理・スコープ優先規則の明文化が重要です。