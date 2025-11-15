# storage/error.rs Review

## TL;DR

- 目的: ストレージ層で発生する多様なエラーを、単一の型に統一して取り扱いやすくする（thiserrorによる人間可読なメッセージと自動的なエラー変換）。
- 公開API: **StorageError**（L6-L45）と **StorageResult<T>**（L47）。From変換があるのは、TantivyError（L8）、QueryParserError（L11）、std::io::Error（L26）、OpenDirectoryError（L41）。
- 複雑箇所: 文字列ベースのバリアント（例: TantivyOperationのcause、General）が原因のチェーンを失いがち。並行性関連のバリアント（LockPoisoned, L38）は存在するがPoisonErrorからの自動変換は未提供。
- 重大リスク: エラーメッセージに含まれるユーザー入力や内部情報がログ漏えいする可能性、文字列causeによりデバッグ容易性が低下。
- Rust安全性: unsafe未使用、所有権/借用の複雑性なし。Send/Syncは構成要素的に満たす可能性が高いが、このチャンクには明示なし。
- 推奨: バリアントの原因をStringではなく型化（source）し、エラーコード/カテゴリを付与。PoisonErrorのFrom実装追加。#[non_exhaustive]で将来拡張に強く。

## Overview & Purpose

このファイルは、ストレージ層で発生するエラーを統合するための専用エラー型 **StorageError** と、利便性のための **StorageResult<T>** 型エイリアスを提供します。外部ライブラリ（tantivy）や標準IOを含む多岐のエラーをラップし、開発者が「?」演算子でシンプルに扱えるようにしています。thiserrorを用いて人間可読なメッセージと、エラー連鎖（source）を自動実装します。

用途の範囲は、以下のような操作での失敗を網羅します。
- Tantivyのインデックス/クエリ関連の失敗（L8, L11, L14）
- ドキュメント検索での未発見（L17）
- スキーマやフィールド値の検証失敗（L20, L23）
- IO/ディレクトリ操作の失敗（L26, L41）
- バッチ操作の状態不整合（L35）
- ロックのポイズン（L38）
- シリアライズ/メタデータ関連の失敗（L29, L32）
- 一般的なその他の失敗（L44）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | StorageError | pub | ストレージ層の包括的なエラー型。外部/内部エラーの統一、メッセージ整形、エラー連鎖 | Low |
| Type Alias | StorageResult<T> | pub | Result<T, StorageError> の短縮形でAPI整形 | Low |

- 定義位置:
  - StorageError: L6-L45
  - StorageResult: L47
- 重要な派生: #[derive(Error, Debug)]（L5）により std::error::Error と Debug を自動実装

### Dependencies & Interactions

- 内部依存（関数/構造体間の呼び出し関係）
  - このチャンク内には関数がないため、直接の内部呼び出しは「不明」。
  - バリアントから推測される内部モジュール:
    - バッチ操作管理（NoActiveBatch, L35）
    - ロック（LockPoisoned, L38）
    - スキーマ/フィールド検証（InvalidFieldValue, SchemaError, L20, L23）
    - メタデータ管理（Metadata, L32）
- 外部依存（使用クレート・モジュール）
  | 依存名 | 用途 | 備考 |
  |--------|------|------|
  | thiserror::Error（L3） | エラー型の派生とメッセージ整形 | #[error(...)] 属性を使用 |
  | tantivy::TantivyError（L2） | Tantivyの汎用エラー | #[from] により自動変換（L8） |
  | tantivy::query::QueryParserError（L2） | クエリパーサのエラー | #[from] により自動変換（L11） |
  | tantivy::directory::error::OpenDirectoryError（L1） | ディレクトリ操作のエラー | #[from] により自動変換（L41） |
  | std::io::Error（暗黙） | IOエラー | #[from] により自動変換（L26） |
- 被依存推定（このモジュールを使用する可能性のある箇所）
  - ストレージのリポジトリ層（読み書き、インデックス構築）
  - 検索API層（QueryParser利用）
  - バッチ投入/コミット機構
  - ロックを伴う共有状態管理（Mutex/RwLock）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| StorageError | pub enum StorageError | エラーの型統一、メッセージ整形、エラー連鎖 | O(1) | O(1) |
| StorageResult | pub type StorageResult<T> = Result<T, StorageError> | 返り値の簡略化と一貫性維持 | O(1) | O(1) |

### StorageError

1) 目的と責務
- ストレージ操作で発生する代表的エラー原因を列挙し、外部エラー（Tantivy、IO等）を #[from] により自動的に取り込む。
- 人間可読なメッセージを提供し、エラー分類を表現する。

2) アルゴリズム（ステップ分解）
- thiserrorの #[error("...")] 属性で表示文字列を定義。
- #[from] 属性で外部エラーから自動的に StorageError へ変換する。
- フィールド付きバリアント（例: TantivyOperation, InvalidFieldValue）で追加コンテキストを保持。

3) 引数（バリアント別フィールド）
| バリアント | フィールド | 型 | 説明 | 行番号 |
|------------|-----------|----|------|-------|
| Tantivy | (source) | TantivyError | Tantivy一般エラー | L8 |
| QueryParser | (source) | QueryParserError | Tantivyクエリパーサエラー | L11 |
| TantivyOperation | operation, cause | String, String | 操作名と原因文字列 | L14 |
| DocumentNotFound | id | String | 見つからなかったドキュメント識別子 | L17 |
| InvalidFieldValue | field, reason | String, String | フィールド名と理由 | L20 |
| SchemaError | message | String | スキーマ異常の説明 | L23 |
| Io | (source) | std::io::Error | 標準IOエラー | L26 |
| Serialization | message | String | シリアライズ失敗理由 | L29 |
| Metadata | message | String | メタデータ関連失敗理由 | L32 |
| NoActiveBatch | - | - | バッチ未開始 | L35 |
| LockPoisoned | - | - | ロックのポイズン状態 | L38 |
| Directory | (source) | OpenDirectoryError | ディレクトリエラー | L41 |
| General | message | String | その他の一般エラー | L44 |

4) 戻り値
- 該当なし（列挙型であり関数ではない）。

5) 使用例
```rust
use tantivy::{Index, query::QueryParser};
use crate::storage::error::{StorageError, StorageResult};

// Tantivy エラー/QueryParserError を自動ラップ
fn parse_user_query(index: &Index, default_fields: Vec<tantivy::schema::Field>, q: &str)
    -> StorageResult<tantivy::query::Query>
{
    let parser = QueryParser::for_index(index, default_fields);
    // QueryParserError -> StorageError::QueryParser に変換される（L11）
    let query = parser.parse_query(q)?;
    Ok(query)
}

// ドキュメント未発見時の明示的エラー
fn get_doc_by_id(id: &str) -> StorageResult<String> {
    // ここでは存在チェックの例のみ
    if id.is_empty() {
        return Err(StorageError::InvalidFieldValue {
            field: "id".into(),
            reason: "empty".into(),
        });
    }
    // 見つからない場合
    Err(StorageError::DocumentNotFound(id.into()))
}

// IO エラーの自動変換（L26）
fn read_file(path: &std::path::Path) -> StorageResult<Vec<u8>> {
    let mut f = std::fs::File::open(path)?; // std::io::Error -> StorageError::Io
    let mut buf = Vec::new();
    use std::io::Read;
    f.read_to_end(&mut buf)?;
    Ok(buf)
}
```

6) エッジケース
- QueryParser: ユーザー入力に依存し、パースエラーが頻発する可能性。
- TantivyOperation: 原因がStringのため、sourceチェーンが失われる。
- LockPoisoned: PoisonErrorからの自動変換がないため、手動で生成する必要あり。
- NoActiveBatch: バッチ制御ロジックと強い整合性が必要。

### StorageResult<T>

1) 目的と責務
- API層での戻り値を Result<T, StorageError> に統一し、呼び出し側のパターン（?演算子）を簡素化。

2) アルゴリズム
- 型エイリアスのみでロジックなし。

3) 引数
- ジェネリック T（戻り値の成功型）。

4) 戻り値
- Result<T, StorageError> と同義。

5) 使用例
```rust
use crate::storage::error::StorageResult;

fn create_document(doc: &str) -> StorageResult<()> {
    if doc.is_empty() {
        // 例: バリデーション失敗
        Err(crate::storage::error::StorageError::InvalidFieldValue {
            field: "doc".into(),
            reason: "empty".into(),
        })
    } else {
        Ok(())
    }
}
```

6) エッジケース
- T が大きな構造の場合もエラー型は軽量（O(1)）なため影響は軽微。

## Walkthrough & Data Flow

- 典型的なフローは次の通り:
  1. 下位呼び出しで外部エラー（TantivyError, QueryParserError, std::io::Error, OpenDirectoryError）が発生。
  2. 「?」演算子により、#[from] が適用され StorageError の対応バリアントへ自動変換（例: parse_query -> QueryParser）。
  3. 独自ロジックの失敗は意味のあるバリアントを明示的に返す（DocumentNotFound, InvalidFieldValue 等）。
  4. 呼び出し元は StorageResult<T> を受け取り、パターンマッチや上位伝播で処理する。

- このチャンクには分岐の多い関数や非同期処理は「このチャンクには現れない」ため、Mermaid図は不要。

## Complexity & Performance

- 時間計算量: すべてのバリアント生成・変換は **O(1)**。
- 空間計算量: バリアントごとのフィールド保持（Stringや外部エラー）で **O(1)**。
- ボトルネック:
  - エラーメッセージ生成（Stringの所有）が多い箇所では軽微なオーバーヘッド。
  - cause を String として複製すると不可視の負荷が増す可能性（再構築不能、デバッグ時間増）。
- 実運用負荷要因:
  - I/O/ネットワーク/DBは外部で発生し、この型は伝播のみ。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空IDで検索 | "" | InvalidFieldValue -> エラー | InvalidFieldValue（L20） | OK |
| 未存在ドキュメント | "abc123" | DocumentNotFound | DocumentNotFound（L17） | OK |
| バッチ未開始でコミット | - | NoActiveBatch | NoActiveBatch（L35） | OK |
| ロックポイズン | Mutexがpanic後 | LockPoisoned | LockPoisoned（L38） | 要改善（PoisonErrorからのFrom未実装） |
| ディレクトリオープン失敗 | 権限なし | Directory | Directory（L41） | OK |
| クエリ構文誤り | "AND OR" | QueryParser | QueryParser（L11） | OK |
| IO失敗 | ファイルなし | Io | Io（L26） | OK |
| 原因の詳細不足 | 不明な内部失敗 | General | General（L44） | 要改善（原因の型化） |

セキュリティチェックリスト
- メモリ安全性: unsafeブロック「不明（このチャンクには現れない）」だが当ファイルはunsafe未使用。Rustの型安全に依存。
- インジェクション: SQL/Command/Path Traversalの直接的な操作はないが、エラーメッセージにユーザー入力を含める可能性があるため、ログでの扱いに注意（例: DocumentNotFound(id)）。
- 認証・認可: 機能なし。「不明」。
- 秘密情報: エラー文字列に秘密を含めない設計が望ましい。ログ漏えい対策（PII遮断）を推奨。
- 並行性: LockPoisoned（L38）が示唆するように、共有状態の保護が必要。PoisonErrorからの自動マッピングがないため、整備推奨。

## Design & Architecture Suggestions

- 原因の型化とエラー連鎖の保持:
  - TantivyOperation { operation: String, cause: String }（L14）は、Stringではなく **#[source] Box<dyn Error + Send + Sync + 'static>** にすることでチェーン保持が可能。例:
    ```rust
    #[error("Tantivy operation error during {operation}")]
    TantivyOperation { operation: String, #[source] source: Box<dyn std::error::Error + Send + Sync> }
    ```
- PoisonErrorの変換を追加して利便性向上:
  ```rust
  impl<T> From<std::sync::PoisonError<T>> for StorageError {
      fn from(_: std::sync::PoisonError<T>) -> Self {
          StorageError::LockPoisoned
      }
  }
  ```
- エラーコード/カテゴリの導入:
  - ロギング/HTTPレスポンス/リトライ戦略のため、**ErrorCode**や**is_transient()**を提供するとよい。
- enumの将来拡張性:
  - **#[non_exhaustive]** を付与して、バリアント追加による破壊的変更を抑制。
- 文字列メッセージの標準化:
  - メッセージに機密情報を含めないガイドライン、フィールド名・操作名の正規化。

## Testing Strategy (Unit/Integration) with Examples

- 目的: From変換、Displayメッセージ、Error連鎖（source）、Send/Sync特性の検証。

単体テスト例
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_from_io_error() {
        let e = io::Error::new(io::ErrorKind::NotFound, "missing");
        let se: StorageError = e.into();
        // Displayが含むはずの文言（L25-L26）
        assert!(format!("{}", se).contains("IO error:"));
    }

    #[test]
    fn test_from_tantivy_error() {
        // TantivyErrorの生成は実プロジェクト環境に依存するため簡易検証に留める
        // このチャンクでは具体的なTantivyErrorのコンストラクタは「不明」。
        // 代わりにメッセージ整形の前提のみ確認。
        let dummy = tantivy::TantivyError::InvalidArgument("x".into());
        let se: StorageError = dummy.into();
        assert!(format!("{}", se).contains("Tantivy error:"));
    }

    #[test]
    fn test_query_parser_error() {
        let qp_err = tantivy::query::QueryParserError::SyntaxError("bad".into());
        let se: StorageError = qp_err.into();
        assert!(format!("{}", se).contains("Tantivy query parser error:"));
    }

    #[test]
    fn test_document_not_found_message() {
        let se = StorageError::DocumentNotFound("doc-1".into());
        assert_eq!(format!("{}", se), "Document not found: doc-1");
    }

    #[test]
    fn test_invalid_field_value_message() {
        let se = StorageError::InvalidFieldValue { field: "age".into(), reason: "negative".into() };
        assert!(format!("{}", se).contains("Invalid field value for age: negative"));
    }

    #[test]
    fn test_lock_poisoned_message() {
        let se = StorageError::LockPoisoned;
        assert_eq!(format!("{}", se), "Lock poisoned");
    }

    #[test]
    fn test_directory_error_message() {
        // OpenDirectoryErrorの具体生成は「不明」だが、thiserrorのメッセージ前提のみ確認
        // （環境依存のため簡易）
        // Skipped: 実際には tantivy::directory::error::OpenDirectoryError を生成して変換
        assert!(true);
    }

    #[test]
    fn storage_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StorageError>();
    }
}
```

統合テスト例（疑似コード）
```rust
// 例: インデックスを開いてクエリ解析 → 成功/失敗の流れを確認
fn search(index: &tantivy::Index, q: &str) -> StorageResult<tantivy::query::Query> {
    let parser = tantivy::query::QueryParser::for_index(index, vec![]);
    let query = parser.parse_query(q)?; // QueryParserError -> StorageError::QueryParser
    Ok(query)
}
```

## Refactoring Plan & Best Practices

- 段階的リファクタリング案:
  1. StorageErrorに **#[non_exhaustive]** を付与。
  2. TantivyOperation の cause を **#[source] Box<dyn Error + Send + Sync>** に変更。
  3. PoisonErrorの **impl<T> From<std::sync::PoisonError<T>>** を追加。
  4. General(String) の使用箇所を棚卸しし、適切な型化されたバリアントへ移行。
  5. エラーコード/カテゴリの trait（例: ErrorCategory）を導入し、HTTP/再試行/監視に対応。
  6. メッセージポリシー（PII除去、機密遮断）の導入とリンティング。

- ベストプラクティス:
  - **エラーは可能な限り型化しsource連鎖を保つ**。
  - **ユーザー入力をそのままメッセージに含めない**（ログ側でマスキング）。
  - 失敗を分類（NotFound/InvalidArgument/Conflict/Internal/Transient）し、上位が適切にハンドリングできるようにする。

## Observability (Logging, Metrics, Tracing)

- ログ: エラー種別ごとにレベルを調整（InvalidFieldValue→warn、Io/Directory→error、TantivyOperation→error）。
- メトリクス: バリアント別のカウンタ（storage_error_total{variant="Io"} 等）を導入。
- トレーシング: error sourceを保持し、Spanに紐づける。例:
```rust
use tracing::{error, instrument};

#[instrument(skip_all)]
fn do_indexing() -> StorageResult<()> {
    // 何らかの操作...
    let res: Result<(), tantivy::TantivyError> = Err(tantivy::TantivyError::InvalidArgument("x".into()));
    res.map_err(StorageError::from).map_err(|e| {
        error!(error = %e, "indexing failed");
        e
    })?;
    Ok(())
}
```

## Risks & Unknowns

- このチャンクには、実際のストレージ処理関数・モジュール構成は「不明」。
- TantivyError/OpenDirectoryError/QueryParserError の具体的生成パスは「不明」だが、#[from] により透過的に扱えることは明確。
- LockPoisonedの発生タイミング・ロック戦略は「不明」。PoisonErrorの扱いは要設計。
- NoActiveBatchの運用仕様（start_batch()/commit() 等の順序制約）は「不明」。エラー発生条件の整合性検証が必要。