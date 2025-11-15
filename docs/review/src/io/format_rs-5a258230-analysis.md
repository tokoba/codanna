# io/format.rs Review

## TL;DR

- 目的: **CLI入出力の標準フォーマット**を提供し、特に**JSONレスポンス**を安定・一貫化して将来のJSON-RPC統合に備える。
- 主要公開API: **OutputFormat**（`from_json_flag`/`is_json`）、**JsonResponse**（`success`/`with_meta`/`with_system_message`/`not_found`/`error`/`from_error`）、**format_utc_timestamp**。
- データ契約: **JsonResponse**は`status`/`code`/`message`/`data`/`error`/`exit_code`/`meta`を持つ汎用構造。`ErrorDetails`と`ResponseMeta`が補助。
- 複雑箇所: `JsonResponse<T>`の汎用型境界（常にT: Serialize）、`ExitCode`→`code`文字列化の一貫性、`ExitCode as u8`のキャスト。
- 重大リスク: `error()`での**コード文字列の不統一**（`NOT_FOUND` vs `NOTFOUND`）、`ExitCode as u8`の**潜在的な切り詰め**、汎用Tの**Serialize/Deserialize両立要件**。
- Rust安全性: **unsafeなし**、所有権は移動中心で安全。並行性の共有状態なし。エラーは`IndexError`から**構造化変換**可能。
- パフォーマンス: すべて**O(1)**（例外: 提案文字列の文字列化で**O(n)**）で軽量。I/Oなし。

## Overview & Purpose

このファイルはCLIの入出力における**フォーマット定義**を集約し、特に**JSON**による機械可読なレスポンス生成を提供します。狙いは以下の通りです。

- コマンドの成功/失敗を**一貫したスキーマ**で返し、ツール連携や将来の**JSON-RPC 2.0**互換構造への拡張を容易にする。
- 人間向けと機械向けの出力切り替え（**Text**/**Json**）を明示的に管理。
- エラーを**構造化**（コード、メッセージ、回復提案、コンテキスト）し、**ExitCode**と紐付けてCLI終了ステータスと整合させる。
- レスポンスに**メタデータ**（バージョン、タイムスタンプ、実行時間）を付与できる拡張点を用意。
- 汎用型`JsonResponse<T>`で任意のペイロード型を透過的に扱う（Tは**Serialize**必須）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | OutputFormat | pub | 出力形式の切替（Text/Json） | Low |
| Struct | JsonResponse<T=serde_json::Value> | pub | 成功/失敗共通のJSONレスポンススキーマ | Med |
| Struct | ErrorDetails | pub | エラー時の提案・追加コンテキスト | Low |
| Struct | ResponseMeta | pub | バージョン・タイムスタンプ・実行時間などのメタ情報 | Low |
| Fn | OutputFormat::from_json_flag | pub | フラグから形式決定 | Low |
| Fn | OutputFormat::is_json | pub | JSON形式判定 | Low |
| Fn | JsonResponse::success | pub | 成功レスポンス生成 | Low |
| Fn | JsonResponse::with_meta | pub | メタ情報付与 | Low |
| Fn | JsonResponse::with_system_message | pub | システム向けガイダンス追加 | Low |
| Fn | JsonResponse<serde_json::Value>::not_found | pub | 404系の標準エラー生成 | Low |
| Fn | JsonResponse<serde_json::Value>::error | pub | 一般エラー生成 | Low |
| Fn | JsonResponse<serde_json::Value>::from_error | pub | IndexErrorからのエラー変換 | Med |
| Fn | format_utc_timestamp | pub | UTCタイムスタンプの整形文字列生成 | Low |

### Dependencies & Interactions

- 内部依存
  - `JsonResponse`は**ExitCode**（`crate::io::exit_code::ExitCode`）を使用して`exit_code: u8`を設定（`success`/`not_found`/`error`/`from_error`）。
  - `from_error`は**IndexError**（`crate::error::IndexError`）から`status_code()`・`recovery_suggestions()`等を利用して**コード文字列**と**提案**を生成。
  - `ResponseMeta`は`JsonResponse::with_meta`で利用。
  - `format_utc_timestamp`は**chrono::Utc**を使用。

- 外部依存

| クレート/モジュール | 用途 |
|--------------------|------|
| chrono::Utc | 現在時刻取得とフォーマット |
| serde::{Serialize, Deserialize} | シリアライズ/デシリアライズ導出 |
| serde_json::Value | デフォルトのペイロード型 |
| crate::error::IndexError | エラー情報の変換元 |
| crate::io::exit_code::ExitCode | CLI終了コードのマッピング |

- 被依存推定
  - CLIコマンド実装層（結果を**JsonResponse**で返す）
  - エラー処理ユーティリティ（`IndexError`→`JsonResponse`）
  - スクリプト連携／APIゲートウェイ（JSONの機械可読性を活用）
  - レポート/監査ログ生成（`format_utc_timestamp`）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| OutputFormat::from_json_flag | `pub fn from_json_flag(json: bool) -> Self` | フラグから形式を選択 | O(1) | O(1) |
| OutputFormat::is_json | `pub fn is_json(&self) -> bool` | JSON形式判定 | O(1) | O(1) |
| JsonResponse::success | `pub fn success(data: T) -> Self` | 成功レスポンス生成 | O(1) | O(size of T) |
| JsonResponse::with_meta | `pub fn with_meta(self, meta: ResponseMeta) -> Self` | メタ付与 | O(1) | O(1) |
| JsonResponse::with_system_message | `pub fn with_system_message(self, message: &str) -> Self` | システム向けメッセージ付与 | O(len(message)) | O(len(message)) |
| JsonResponse::not_found | `pub fn not_found(entity: &str, name: &str) -> Self` | 標準404エラー生成 | O(len(entity)+len(name)) | O(...) |
| JsonResponse::error | `pub fn error(code: ExitCode, message: &str, suggestions: Vec<&str>) -> Self` | 一般エラー生成 | O(sum len(suggestions)+len(message)) | O(n) |
| JsonResponse::from_error | `pub fn from_error(error: &IndexError) -> Self` | ドメインエラーの構造化 | O(n) | O(n) |
| format_utc_timestamp | `pub fn format_utc_timestamp() -> String` | UTC時刻の文字列化 | O(1) | O(1) |

以下、各APIの詳細。

1) OutputFormat::from_json_flag
- 目的と責務
  - 入力フラグから**出力形式**を`Text`/`Json`に決定。
- アルゴリズム
  - `json == true`なら`OutputFormat::Json`、それ以外は`OutputFormat::Text`。
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| json | bool | JSON形式を要求するフラグ |

- 戻り値

| 型 | 意味 |
|----|------|
| OutputFormat | 選択された出力形式 |

- 使用例
```rust
use codanna::io::format::OutputFormat;
let fmt = OutputFormat::from_json_flag(true);
assert!(fmt.is_json());
```
- エッジケース
  - 特になし（分岐は2値、直線的）。

2) OutputFormat::is_json
- 目的と責務
  - 現在の形式が**JSONかどうか**判定。
- アルゴリズム
  - `matches!(self, Self::Json)`。
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| self | &Self | 現在の出力形式 |

- 戻り値

| 型 | 意味 |
|----|------|
| bool | JSONならtrue |

- 使用例
```rust
use codanna::io::format::OutputFormat;
let fmt = OutputFormat::Json;
assert!(fmt.is_json());
```
- エッジケース
  - 特になし。

3) JsonResponse::success
- 目的と責務
  - 成功を表す**標準レスポンス**を生成し、`data`にペイロードを格納。
- アルゴリズム
  - 固定値: `status="success"`, `code="OK"`, `message="Operation completed successfully"`, `exit_code=ExitCode::Success as u8`。
  - `data=Some(data)`、`error=None`、`meta=None`。
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| data | T | ペイロード（所有権移動） |

- 戻り値

| 型 | 意味 |
|----|------|
| JsonResponse<T> | 成功レスポンス |

- 使用例
```rust
use serde::Serialize;
use codanna::io::format::JsonResponse;

#[derive(Serialize)]
struct Item { id: u32 }
let resp = JsonResponse::success(Item { id: 1 });
assert_eq!(resp.status, "success");
```
- エッジケース
  - 大きな`T`の移動でも安全（所有権移動）。シリアライズ不可のTはコンパイルエラー。

4) JsonResponse::with_meta
- 目的と責務
  - レスポンスに**メタ情報**（バージョン/時刻/実行時間）を付与。
- アルゴリズム
  - `self.meta = Some(meta)`で自己消費（ムーブ）により設定。
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| self | Self | レスポンス（所有権消費） |
| meta | ResponseMeta | メタ情報 |

- 戻り値

| 型 | 意味 |
|----|------|
| Self | メタ付与済みレスポンス |

- 使用例
```rust
use codanna::io::format::{JsonResponse, ResponseMeta};
let resp = JsonResponse::success(serde_json::json!({"ok": true}))
    .with_meta(ResponseMeta { version: "1.0.0".into(), timestamp: None, execution_time_ms: Some(12) });
```
- エッジケース
  - 既に`meta`がある場合も上書き（直線的）。

5) JsonResponse::with_system_message
- 目的と責務
  - AIアシスタント向けの**システムガイダンス**を付与。
- アルゴリズム
  - `self.system_message = Some(message.to_string())`。
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| self | Self | レスポンス（所有権消費） |
| message | &str | ガイダンス文 |

- 戻り値

| 型 | 意味 |
|----|------|
| Self | ガイダンス付与済みレスポンス |

- 使用例
```rust
use codanna::io::format::JsonResponse;
let resp = JsonResponse::success(serde_json::json!({"next": "ok"}))
    .with_system_message("Next: run 'index update'");
```
- エッジケース
  - 空文字も許容。シリアライズ時は空で出る（`skip_serializing_if`は`Option::is_none`のみ）。

6) JsonResponse::not_found
- 目的と責務
  - **NOT_FOUND**の標準化されたエラーを生成。
- アルゴリズム
  - 固定値: `status="error"`, `code="NOT_FOUND"`, `exit_code=ExitCode::NotFound as u8`。
  - `message = format!("{entity} '{name}' not found")`。
  - `error.suggestions = ["Check the spelling", "Ensure the index is up to date"]`。
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| entity | &str | 種別（例: "Symbol"） |
| name | &str | 名前（例: "main"） |

- 戻り値

| 型 | 意味 |
|----|------|
| JsonResponse<serde_json::Value> | 標準404エラー |

- 使用例
```rust
use codanna::io::format::JsonResponse;
let resp = JsonResponse::not_found("Symbol", "main");
assert_eq!(resp.code, "NOT_FOUND");
```
- エッジケース
  - `entity`/`name`に特殊文字が含まれても安全（文字列生成のみ）。

7) JsonResponse::error
- 目的と責務
  - 任意の**ExitCode**に紐づく一般エラーを構築。
- アルゴリズム
  - `status="error"`、`code = format!("{code:?}").to_uppercase()`。
  - `error.suggestions = suggestions.iter().map(|s| s.to_string()).collect()`。
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| code | ExitCode | 終了コード |
| message | &str | 人間向け説明 |
| suggestions | Vec<&str> | 回復提案群 |

- 戻り値

| 型 | 意味 |
|----|------|
| JsonResponse<serde_json::Value> | 一般エラー |

- 使用例
```rust
use codanna::io::format::JsonResponse;
use codanna::io::exit_code::ExitCode;
let resp = JsonResponse::error(ExitCode::InvalidArgs, "Invalid CLI args", vec!["Run --help", "Check config"]);
```
- エッジケース
  - suggestionsが空でも可。`error.suggestions`は空配列としてシリアライズ。

8) JsonResponse::from_error
- 目的と責務
  - **IndexError**から構造化レスポンスへの**損失の少ない変換**。
- アルゴリズム
  - `code = error.status_code()`（ドメイン定義の文字列）。
  - `message = error.to_string()`。
  - `suggestions = error.recovery_suggestions().iter().map(|s| s.to_string()).collect()`。
  - `exit_code = ExitCode::from_error(error) as u8`。
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| error | &IndexError | ドメインエラー |

- 戻り値

| 型 | 意味 |
|----|------|
| JsonResponse<serde_json::Value> | 構造化エラー |

- 使用例
```rust
use codanna::io::format::JsonResponse;
use codanna::error::IndexError;
// let err = IndexError::...; // 実際の生成はこのチャンクには現れない
// let resp = JsonResponse::from_error(&err);
```
- エッジケース
  - `recovery_suggestions()`が空でも問題なし。
  - `status_code()`が規定外文字列でもシリアライズ可能。

9) format_utc_timestamp
- 目的と責務
  - 現在のUTC時刻を**"YYYY-MM-DD HH:MM:SS UTC"**形式で返す。
- アルゴリズム
  - `Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string()`。
- 引数
  - なし
- 戻り値

| 型 | 意味 |
|----|------|
| String | 整形済みUTCタイムスタンプ |

- 使用例
```rust
use codanna::io::format::format_utc_timestamp;
let ts = format_utc_timestamp();
assert!(ts.ends_with(" UTC"));
```
- エッジケース
  - ロケール非依存。クロスプラットフォームで安定。

### Data Contracts（JSONスキーマの解説）

- JsonResponse<T>
  - 必須: **status**（"success"/"error"）、**code**（例: "OK"/"NOT_FOUND"）、**message**、**exit_code**。
  - 成功時: **data: Option<T>** が Some。**error: None**。
  - 失敗時: **error: Option<ErrorDetails>** が Some。**data: None**。
  - 追加: **system_message: Option<String>**（AI支援向け）、**meta: Option<ResponseMeta>**。
- ErrorDetails
  - **suggestions: Vec<String>**（空でも可）、**context: Option<serde_json::Value>**。
- ResponseMeta
  - **version: String**、**timestamp: Option<String>**、**execution_time_ms: Option<u64>**。

## Walkthrough & Data Flow

- フロー例（CLIがJSONモードの場合）🧩
  1. フラグから形式決定: `let fmt = OutputFormat::from_json_flag(json_flag);`
  2. 成功時:
     - `let resp = JsonResponse::success(payload).with_meta(meta).with_system_message("Next: ...");`
     - シリアライズして出力（このチャンクには現れない）。
  3. 失敗時（ドメインエラー）:
     - `let resp = JsonResponse::from_error(&index_err);`
     - エラー提案/コード/exit_codeが自動充填される。
  4. 失敗時（汎用エラー）:
     - `let resp = JsonResponse::error(code, "message", vec!["..."]);`
  5. Not Foundユースケース:
     - `let resp = JsonResponse::not_found("Symbol", "main");`
- データ流れ
  - 入力: CLIフラグ/ドメインエラー（IndexError）。
  - 変換: **ExitCode**→`exit_code`、`IndexError`→`code`/`message`/`suggestions`。
  - 出力: **JsonResponse**（構造化）またはText（このファイルでは生成なし）。

上記の処理は単純直線的で分岐が少なく、Mermaid図の基準（分岐4以上/状態3以上）を満たさないため図は省略。

## Complexity & Performance

- OutputFormat関連: 時間O(1)、空間O(1)。
- JsonResponse::success/with_meta/with_system_message: 時間O(1)～O(len(message))、空間O(1)（ただし`data`の格納はTのサイズに依存）。
- JsonResponse::not_found: 時間O(len(entity)+len(name))、空間O(1)。
- JsonResponse::error: 時間O(n)（提案の文字列化数）、空間O(n)。
- JsonResponse::from_error: 時間O(n)（`recovery_suggestions`の長さ）、空間O(n)。
- format_utc_timestamp: 時間O(1)、空間O(1)。
- ボトルネック: 文字列割り当て程度。I/O・ネットワーク・DBアクセスは**なし**。
- スケール限界: `suggestions`や`data`が巨大な場合のJSONシリアライズコスト増。通常のCLI応答では**問題なし**。

## Edge Cases, Bugs, and Security

セキュリティチェックリストに沿った評価。

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rustの安全なAPIのみ。危険な型変換は`ExitCode as u8`（後述の意味的リスク）程度。unsafeは**未使用**。
- インジェクション
  - SQL/Command/Path traversal: 該当なし（文字列整形のみ）。
- 認証・認可
  - 該当なし（CLIローカルのフォーマットコード）。
- 秘密情報
  - Hard-coded secrets: **なし**。
  - Log leakage: **system_message**や**context**には潜在的に機微情報を入れる実装側のリスクがあるため、利用側でのフィルタリングポリシーが望ましい。
- 並行性
  - Race/Deadlock: **共有状態なし**。全て局所的構築で安全。

詳細なエッジケースと既知/潜在バグ:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| コード文字列の一貫性 | ExitCode::NotFound | "NOT_FOUND" を期待 | `error()`は`format!("{code:?}").to_uppercase()`で"NOTFOUND"になりうる | 問題あり（不一致） |
| ExitCodeのu8キャスト | ExitCodeの値が>255 | 適切な範囲内のみ使用 | `as u8`で切り詰めが起きる可能性 | 潜在リスク（設計で要制約） |
| 大量suggestions | 1万件の提案 | メモリ消費増だが動作継続 | Vec<String>へ変換 | パフォーマンス劣化の可能性 |
| Deserialize要件 | TがSerializeのみ | Deserializeも必要な場面 | `#[derive(Deserialize)]`によりTはDeserializeも必要 | 使用制約（意図次第） |
| system_message未設定 | None | フィールド非出力 | `skip_serializing_if`で非出力 | 問題なし |
| error.context未設定 | None | フィールド非出力 | `skip_serializing_if`で非出力 | 問題なし |

Rust特有の観点（詳細チェックリスト）:

- メモリ安全性（所有権/借用/ライフタイム）
  - 所有権: `JsonResponse::success(data: T)`は**所有権移動**で安全。`with_meta`/`with_system_message`は`self`を消費して新たな`Self`を返すため**不変条件**維持。
  - 借用: 引数`&str`や`&IndexError`は**不変借用**のみ。可変借用の長期保持なし。
  - ライフタイム: 明示的ライフタイムパラメータ**不要**（ヒープ所有の`String`/`Vec<String>`に変換）。

- unsafe境界
  - 使用箇所: **なし**。
  - 不変条件/安全性根拠: 標準ライブラリ/serde/chronoの安全APIのみ。

- 並行性・非同期
  - Send/Sync: 型境界に**Send/Sync**要求はなし。ただし共有しないため問題なし。
  - データ競合: 共有状態なし。ローカル構築のみ。
  - await境界/キャンセル: 非同期処理は**このチャンクには現れない**。

- エラー設計
  - Result vs Option: レスポンス構築APIはエラーを返さず、`Option`でフィールド存在を制御（JSON時の省略）。ドメインエラーは`IndexError`から**構造化**。
  - panic箇所: `unwrap`/`expect`などは**未使用**。
  - エラー変換: `ExitCode::from_error(error)`と`IndexError`→`JsonResponse`の**型安全な変換**。詳細は外部依存のため**不明**。

重要な主張の根拠（関数名:行番号）: 行番号はこのチャンクに含まれていないため**行番号不明**。関数名は本文に明記。

## Design & Architecture Suggestions

- コード文字列の一貫性（重要・⚠️）
  - `JsonResponse::error`での`code`生成を、`ExitCode`→**明示的な文字列マップ**に変更（例: `Display`実装や`serde(rename)`付きの列挙型）。`NOT_FOUND`と`NOTFOUND`の不一致を解消。
- ステータスの型安全化
  - `status: String`を**列挙型**（例: `enum Status { Success, Error }`）にし、`serde(rename)`で"success"/"error"へシリアライズ。誤入力を防ぐ。
- Builderパターン導入
  - `JsonResponseBuilder`で`data`/`error`/`system_message`/`meta`を段階的に設定し、最終的に整合性チェック（成功時にerror禁止、失敗時にdata禁止など）を行う。
- `ResponseMeta`の拡張
  - `timestamp`を既定で**RFC3339**（例: `2025-09-28T15:30:45Z`）形式へ（現在は"YYYY-MM-DD HH:MM:SS UTC"）。相互運用性向上。
- `ExitCode`の安全な数値化
  - `as u8`の代わりに`fn to_u8(&self) -> u8`を**明示制約**付きで実装（上限値の静的保証や`TryFrom`で検証）。

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト
  - `OutputFormat::from_json_flag`のブール分岐（✅）。
  - `JsonResponse::success`/`not_found`の基本フィールド検証（✅）。

- 追加ユニットテスト提案
  - `error()`のコード文字列一貫性検証（現状は不一致になりうるため、暫定的に期待値定義）。
  - `from_error()`の提案リスト変換と`exit_code`マッピングの整合性（モック`IndexError`が必要。このチャンクには定義がないため擬似型で代替）。
  - シリアライズの省略挙動
    - `system_message=None`/`meta=None`/`error=None`/`data=None`がJSONに含まれないことの確認。
  - 大量`suggestions`の性能（ベンチマーク、単体では軽くメモリ消費検証）。
  - `format_utc_timestamp()`のフォーマット検証（`UTC`サフィックス、パターン一致）。

- 使用例（シリアライズ検証）
```rust
use codanna::io::format::{JsonResponse, ResponseMeta};
let resp = JsonResponse::success(serde_json::json!({"k": "v"}))
    .with_meta(ResponseMeta { version: "1.2.3".into(), timestamp: Some("2024-10-31 00:00:00 UTC".into()), execution_time_ms: Some(5) })
    .with_system_message("Proceed");

let s = serde_json::to_string(&resp).unwrap();
assert!(s.contains("\"status\":\"success\""));
assert!(s.contains("\"version\":\"1.2.3\""));
assert!(s.contains("\"system_message\":\"Proceed\""));
```

## Complexity & Performance

- Big-O（時間/空間）: 前述API表を参照。全体として**定数時間**操作が大半。
- ボトルネック
  - 文字列割り当て（message/suggestions）・`serde_json`のシリアライズ。
- スケール限界
  - 大規模`data`や大量`suggestions`でのメモリ使用増。
- 実運用負荷要因
  - I/O/ネットワーク/DBなし。CPU/メモリに限定された非常に軽量な層。

## Refactoring Plan & Best Practices

- フェーズ1（互換性維持）
  - `JsonResponse::error`内の`code`生成ロジックを**手動マップ**へ置換（例: `match code { ExitCode::NotFound => "NOT_FOUND", ... }`）。
  - 補助関数`exit_code_to_str(ExitCode) -> &'static str`を追加。
- フェーズ2（型安全強化）
  - `status`を列挙型に置換。`serde`の`rename`で既存JSON互換維持。
  - `ExitCode`へ`to_u8()`を追加し、暗黙の`as`キャスト排除。
- フェーズ3（拡張と整合性）
  - Builder導入で成功/失敗の相互排他性を**型で担保**。
  - `ResponseMeta.timestamp`を**RFC3339**へ。`format_utc_timestamp_rfc3339()`の追加（既存は保持）。
- ベストプラクティス
  - **不変データ契約**を維持しつつ、内部表現は型安全へ。
  - 文字列コードの**定数化**（重複排除、テスト容易化）。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - `from_error`や`not_found`生成箇所で**発生源**・**エンティティ/名前**などのログ化は利用側で推奨（このファイルは純粋データ構築のみ）。
- メトリクス
  - エラー種類別カウント（`code`単位）、`exit_code`分布。
- トレーシング
  - `ResponseMeta`の`timestamp`と`execution_time_ms`で**簡易トレース**を可能に。将来的に**trace_id**追加を検討。

## Risks & Unknowns

- Unknowns（このチャンクには現れない）
  - `ExitCode`の全バリアント定義と数値範囲。
  - `IndexError`の`status_code()`/`recovery_suggestions()`の仕様詳細。
  - 実際のCLI出力時のシリアライザ設定（pretty/compactやフィールド順）。
- リスク
  - **コード文字列の不一致**（`error()` vs `not_found()`）。自動処理系が`NOT_FOUND`を期待する場合の互換性問題。
  - **u8キャストの切り詰め**。`ExitCode`の将来拡張で>255が導入されると不正な`exit_code`に。
  - **Serialize/Deserialize要件の過剰束縛**。`JsonResponse<T>`に`T: Serialize`が常に要求されるため、Deserializeのみを意図するユースケースに不適合。

以上により、このファイルはCLIのJSONフォーマット基盤としては堅牢で軽量ですが、**コード表現の一貫性**と**型安全性**に関して少数の改善余地があります。継続的なテストと小さなリファクタリングで、運用上の信頼性をさらに高められます。