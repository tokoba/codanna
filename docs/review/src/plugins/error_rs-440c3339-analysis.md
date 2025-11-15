# plugins\error.rs Review

## TL;DR

- 目的: プラグイン管理操作における失敗を表現するための統一的なエラー型（**PluginError**）と、CLI向けの**ExitCode**へのマッピング関数を提供
- 主要公開API: **PluginError (pub enum)**、**PluginResult<T> = Result<T, PluginError)**、および **PluginError::exit_code(&self) -> ExitCode**
- コアロジック: thiserrorにより各バリアントにわかりやすいメッセージと提案を付与し、`#[from]`で下位エラー（io/serde_json/git2）を自動変換
- 複雑箇所: エラー種別から**ExitCode**へのグルーピング（match）によりUXを一定に保つ
- 重大リスク: 「成功」をエラー型バリアント（DryRunSuccess）で表現しているため、`Result`の通常の意味論と混同する可能性
- セキュリティ/安全性: unsafeなし・メモリ安全、インジェクションなし。ログにパスなどが含まれる可能性に配慮が必要
- 並行性: 共有状態なし。`Send/Sync`は構成要素に依存（git2::Error等の性質はこのチャンクでは不明）

## Overview & Purpose

このファイルは、プラグインの追加・削除・更新・検証などの操作に関する失敗を網羅的に表現するためのエラー型を定義し、CLIアプリケーションが一貫した終了コード（ExitCode）を返すためのマッピングを提供します。

特徴:
- ユーザー向けに理解しやすい**エラーメッセージ**と**改善提案（Suggestion）**を各バリアントに付与
- `io::Error`、`serde_json::Error`、`git2::Error`の**自動変換（#[from]）**を備え、`?`演算子での伝播を簡素化
- エラーを**ExitCode**にマップし、CLIのUXを統一

適用範囲:
- マーケットプレイス参照、プラグイン存在確認、マニフェスト検証、Git操作、ファイル競合、依存関係、ローカルの変更検出、ネットワーク・IO・JSON・git2エラーまで多岐に渡る

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | PluginError | pub | プラグイン操作に関する失敗を網羅的に表現 | Med |
| Type alias | PluginResult<T> | pub | `Result<T, PluginError>` の短縮 | Low |
| Impl | PluginError::exit_code | pub | 各エラーをCLI用のExitCodeにマッピング | Low |
| Module | tests | private | メッセージ/変換の単体テスト | Low |

### Dependencies & Interactions

- 内部依存
  - `PluginError::exit_code` は `PluginError` バリアントのmatchにより `crate::io::exit_code::ExitCode` を返す
  - `#[from]` による `IoError` / `JsonError` / `Git2Error` バリアントへの自動変換

- 外部依存（このチャンクに現れるもの）
  | 依存 | 用途 |
  |------|------|
  | thiserror::Error | エラー型の派生（Display/Debug/Source） |
  | std::io::Error, std::path::PathBuf | IOエラーの取り込み、パス情報保持 |
  | serde_json::Error | JSONパースエラーの取り込み |
  | git2::Error | Git操作エラーの取り込み |
  | crate::io::exit_code::ExitCode | CLIの終了コード表現 |

- 被依存推定
  - プラグイン管理コマンド（インストール/削除/更新/検証）ロジックで利用される可能性が高い（詳細な利用箇所はこのチャンクには現れない）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PluginError | `pub enum PluginError` | プラグイン操作失敗の語彙を統一し、ユーザー向けメッセージを提供 | O(1) | O(1) |
| PluginResult | `pub type PluginResult<T> = Result<T, PluginError>` | 関数戻り値のための標準化されたResult型 | — | — |
| exit_code | `impl PluginError { pub fn exit_code(&self) -> ExitCode }` | エラーをCLI終了コードにマップ | O(1) | O(1) |
| From(io::Error) | `impl From<io::Error> for PluginError` | IOエラーを自動的にPluginErrorへ | O(1) | O(1) |
| From(serde_json::Error) | `impl From<serde_json::Error> for PluginError` | JSONエラーを自動的にPluginErrorへ | O(1) | O(1) |
| From(git2::Error) | `impl From<git2::Error> for PluginError` | Git2エラーを自動的にPluginErrorへ | O(1) | O(1) |

以下、主要APIの詳細。

1) PluginError（pub enum）
- 目的と責務
  - プラグイン管理の失敗をバリアントとして分類し、ユーザーにわかりやすいメッセージと改善提案を提供する
- アルゴリズム（該当なし）
- 引数（構造体フィールド）
  | バリアント | フィールド |
  |-----------|-----------|
  | MarketplaceNotFound | url: String |
  | PluginNotFound | name: String |
  | InvalidMarketplaceManifest | reason: String |
  | InvalidPluginManifest | reason: String |
  | GitOperationFailed | operation: String |
  | FileConflict | path: PathBuf, owner: String |
  | IntegrityCheckFailed | plugin: String, expected: String, actual: String |
  | LockfileCorrupted | — |
  | PermissionDenied | path: PathBuf |
  | AlreadyInstalled | name: String, version: String |
  | NotInstalled | name: String |
  | HasDependents | name: String, dependents: Vec<String> |
  | McpServerConflict | key: String |
  | MissingArgument | String |
  | InvalidReference | ref_name: String, reason: String |
  | NetworkError | String |
  | IoError | io::Error（#[from]） |
  | JsonError | serde_json::Error（#[from]） |
  | Git2Error | git2::Error（#[from]） |
  | LocalModifications | name: String |
  | DryRunSuccess | — |
- 戻り値（該当なし）
- 使用例
  ```rust
  use std::path::PathBuf;
  use crate::plugins::error::{PluginError, PluginResult};

  fn read_manifest(path: &PathBuf) -> PluginResult<String> {
      let content = std::fs::read_to_string(path)?; // io::Error -> PluginError::IoError に自動変換
      if content.is_empty() {
          return Err(PluginError::InvalidPluginManifest {
              reason: "empty file".to_string(),
          });
      }
      Ok(content)
  }
  ```
- エッジケース
  - DryRunSuccess は成功状態をエラー型で表現しており、Resultの意味論と混同しうる
  - NetworkError/MissingArgument は Stringのみで保持し、原因の構造化情報（エラー種別、階層）を欠く

2) PluginError::exit_code(&self) -> ExitCode（pub）
- 目的と責務
  - ユーザー体験を一貫させるため、エラーからCLI終了コードへ確定的にマッピング
- アルゴリズム（ステップ）
  1. `self` を `match` で分岐
  2. 失敗種類をグループ化し、適切な `ExitCode` を返す
- 引数
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &PluginError | Yes | マッピング対象のエラー |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | ExitCode | CLIの終了コード |
- 使用例
  ```rust
  use crate::plugins::error::{PluginError, PluginResult};
  use crate::io::exit_code::ExitCode;

  fn handle() -> ExitCode {
      let result: PluginResult<()> = Err(PluginError::NotInstalled { name: "x".into() });
      match result {
          Ok(_) => ExitCode::Success,
          Err(e) => e.exit_code(),
      }
  }
  ```
- エッジケース
  - 新規バリアント追加時にマッピング漏れがあると意図しない `ExitCode` にフォールバックしない（コンパイラが未処理パターンを検出）ため安全だが、設計上の分類誤りがUXに影響する

3) PluginResult<T>（type alias）
- 目的と責務
  - 関数の戻り値型に共通の語彙（PluginError）を採用するための短縮
- 使用例
  ```rust
  use crate::plugins::error::{PluginResult, PluginError};

  fn install(name: &str) -> PluginResult<()> {
      if name.is_empty() {
          return Err(PluginError::MissingArgument("name".into()));
      }
      Ok(())
  }
  ```
- エッジケース
  - DryRunSuccess を Err 側で返す設計の場合、呼び出し側は `Ok`/`Err` の意味解釈に注意が必要

## Walkthrough & Data Flow

- データフロー（典型）
  1. 下位層で `io::Error` や `serde_json::Error`、`git2::Error` が発生
  2. `?` 演算子により自動的に `PluginError::{IoError, JsonError, Git2Error}` へ変換（`#[from]`）
  3. 呼び出し側で `Err(PluginError)` を受け取り、`exit_code()` で `ExitCode` に変換
  4. CLIが `ExitCode` に基づいて終了

- マッピングロジック（コード引用）
  ```rust
  impl PluginError {
      pub fn exit_code(&self) -> ExitCode {
          match self {
              PluginError::MarketplaceNotFound { .. }
              | PluginError::PluginNotFound { .. }
              | PluginError::NotInstalled { .. } => ExitCode::NotFound,
              PluginError::InvalidMarketplaceManifest { .. }
              | PluginError::InvalidPluginManifest { .. }
              | PluginError::JsonError(_)
              | PluginError::MissingArgument(_)
              | PluginError::LockfileCorrupted => ExitCode::ConfigError,
              PluginError::FileConflict { .. }
              | PluginError::IntegrityCheckFailed { .. }
              | PluginError::HasDependents { .. }
              | PluginError::McpServerConflict { .. }
              | PluginError::LocalModifications { .. } => ExitCode::BlockingError,
              PluginError::PermissionDenied { .. }
              | PluginError::IoError(_)
              | PluginError::GitOperationFailed { .. }
              | PluginError::Git2Error(_)
              | PluginError::NetworkError(_)
              | PluginError::InvalidReference { .. } => ExitCode::GeneralError,
              PluginError::AlreadyInstalled { .. } => ExitCode::UnsupportedOperation,
              PluginError::DryRunSuccess => ExitCode::Success,
          }
      }
  }
  ```
  上記の図は`exit_code`関数（行番号:不明）の主要分岐を示す。

- マッピングの可視化（Mermaid）
  ```mermaid
  flowchart TD
    A[PluginError] -->|MarketplaceNotFound / PluginNotFound / NotInstalled| B[ExitCode::NotFound]
    A -->|Invalid*Manifest / JsonError / MissingArgument / LockfileCorrupted| C[ExitCode::ConfigError]
    A -->|FileConflict / IntegrityCheckFailed / HasDependents / McpServerConflict / LocalModifications| D[ExitCode::BlockingError]
    A -->|PermissionDenied / IoError / GitOperationFailed / Git2Error / NetworkError / InvalidReference| E[ExitCode::GeneralError]
    A -->|AlreadyInstalled| F[ExitCode::UnsupportedOperation]
    A -->|DryRunSuccess| G[ExitCode::Success]
  ```
  上記の図は`PluginError::exit_code`関数（行番号:不明）の分岐を整理したもの。

## Complexity & Performance

- exit_code: 時間 O(1)、空間 O(1)。単純な`match`分岐のみ。
- エラー生成/Display: フォーマットは各バリアントで固定文字列＋フィールド挿入のためO(n)（nは文字列長）。通常は軽量。
- I/O/ネットワーク/DB: このファイル自体はI/Oを行わず、下位レイヤのエラーを受け取るのみ。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - unsafeなし。所有権/借用の特殊な扱いはこのチャンクには現れない。Buffer overflow / Use-after-free / Integer overflow の懸念は該当なし。

- インジェクション
  - SQL/Command/Path traversalの実行はない。メッセージにパスやURLを埋め込むが、出力時のエスケープは呼び出し側に依存（CLI表示であれば一般的に安全）。

- 認証・認可
  - 該当なし（このチャンクには現れない）。

- 秘密情報
  - ハードコードされたシークレットなし。ログ出力時にパスやURLが露出するが、機密性リスクは低い。

- 並行性
  - 共有状態なし。`PluginError` が `Send/Sync` であるかは構成要素に依存するが、`git2::Error` 等の特性はこのチャンクでは不明。並行使用の設計は呼び出し側で要確認。

- 潜在的なバグ/設計上の懸念
  - DryRunSuccess がエラー型に含まれ、成功を `Err` 側で表現する可能性があるのは混乱の元。
  - NetworkError/MissingArgument は Stringのみで保持するため、原因の粒度が粗く、診断性がやや低い。
  - メッセージの「Claude's schema」など固有名詞が含まれ、プロジェクト内の命名/ブランド一貫性が外部要因で変化した際に陳腐化する可能性。

- エッジケース詳細

  | エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
  |-------------|--------|----------|------|------|
  | DryRunの成功をErrで伝搬 | DryRunSuccess | 成功はOkで返す | エラー型にDryRunSuccessを含有 | 要検討 |
  | 不明瞭なネットワーク原因 | NetworkError("timeout") | 原因に応じた詳細診断 | Stringのみ保持 | 改善余地 |
  | 参照不正 | InvalidReference{ ref_name: "feature@", reason: "invalid char" } | ユーザーに修正指針を提供 | Suggestionを含むメッセージ | 適切 |
  | 依存関係による削除不可 | HasDependents{ name, dependents } | 依存一覧を提示し、操作停止 | BlockingErrorにマップ | 適切 |

## Design & Architecture Suggestions

- 成功を表す型の分離
  - DryRunSuccess は `Ok(())` か `Ok(DryRunReport)` などで表現し、エラー型から分離するのが自然。

- エラー語彙の拡張と構造化
  - `NetworkError(String)` を `NetworkError { source: reqwest::Error, kind: ... }` のように構造化すると診断性向上（このチャンクには実装なし）。

- 非破壊的拡張のための`#[non_exhaustive]`
  - `pub enum PluginError` に `#[non_exhaustive]` を付けると外部クレートへの公開時に将来のバリアント追加が容易（このチャンクでの公開範囲の要件に依存）。

- ExitCodeマッピングの集中管理
  - `exit_code` の分類は良いが、将来的な拡張に備えて別モジュール/テーブル化も検討可能（テスト容易性向上）。

- メッセージの国際化・一貫性
  - 現在は英語固定メッセージ。i18nが必要なら`Display`をラッパーで差し替える設計を検討。

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト
  - メッセージにSuggestionが含まれること、FileConflictのメッセージ内容検証、`io::Error` が `PluginError::IoError` に変換されること。

- 追加推奨テスト
  - バリアントごとの `exit_code` マッピング検証
  - `JsonError` / `Git2Error` の `#[from]` 変換確認
  - `Display` のフォーマット（主要バリアント）に期待するフィールドが含まれること
  - `HasDependents` の dependents リストがメッセージに含まれること

- コード例（マッピングのテスト）
  ```rust
  use crate::plugins::error::PluginError;
  use crate::io::exit_code::ExitCode;

  #[test]
  fn exit_code_mapping_basic() {
      assert_eq!(
          PluginError::NotInstalled { name: "x".into() }.exit_code(),
          ExitCode::NotFound
      );
      assert_eq!(
          PluginError::InvalidPluginManifest { reason: "bad".into() }.exit_code(),
          ExitCode::ConfigError
      );
      assert_eq!(
          PluginError::FileConflict { path: ".p".into(), owner: "o".into() }.exit_code(),
          ExitCode::BlockingError
      );
      assert_eq!(
          PluginError::IoError(std::io::Error::new(std::io::ErrorKind::Other, "x")).exit_code(),
          ExitCode::GeneralError
      );
      assert_eq!(
          PluginError::AlreadyInstalled { name: "x".into(), version: "1.0".into() }.exit_code(),
          ExitCode::UnsupportedOperation
      );
      assert_eq!(PluginError::DryRunSuccess.exit_code(), ExitCode::Success);
  }
  ```

## Refactoring Plan & Best Practices

- DryRunSuccessの扱いを整理
  - 成功/レポート型での返却に置き換え、`PluginError` から除外する。

- エラーの構造化
  - `NetworkError` を `reqwest::Error` などに紐づけ（`#[from]`）し、`source()` チェーンで原因追跡可能に。

- 一貫した命名とメッセージ
  - 固有名詞（Claudeなど）の文言を設定可能にし、将来の変更に備える。

- 拡張容易性
  - `#[non_exhaustive]` の付与、`exit_code` の分類ルールをドキュメント化。

- ベストプラクティス
  - `thiserror` を継続活用し、`source`/`from` を適切に付与
  - ユーザーに具体的行動を促す「Suggestion」を各メッセージに保持（既に実施済み）

## Observability (Logging, Metrics, Tracing)

- ロギング
  - `err.to_string()` はユーザー向け文言。開発者向け詳細は `err.source()` 連鎖（`IoError`/`JsonError`/`Git2Error`）で追跡。
  - パスやURLを含むため、ログレベルに応じてフィルタ/非表示を検討。

- メトリクス
  - バリアント別の発生回数をカウントすると、UX改善や品質計測に有用（このチャンクには現れない）。

- トレーシング
  - エラー発生地点でspanに `error.variant` / `exit_code` をタグ付けしておくとCLI全体の分析が容易（実装はこのチャンクには現れない）。

## Risks & Unknowns

- `git2::Error` 等の `Send/Sync` 特性はこのチャンクでは不明。並行使用時は要検証。
- `NetworkError(String)` の原因の粒度が粗く、根本原因の特定が困難になり得る。
- メッセージ内の固有名詞・パス表記の将来互換性（ブランドや構成の変更）リスク。
- このモジュールの利用箇所と実際のCLIフローはこのチャンクには現れないため、全体設計への影響度の評価は不明。