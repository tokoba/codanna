# storage/mod.rs Review

## TL;DR

- このファイルはストレージ層の**ファサードモジュール**で、複数のサブモジュールを公開し、主要な型を再輸出して統一的なAPIを提供します（L1-L12）。
- 公開APIは主に型の再輸出で構成され、**StorageError/StorageResult**, **DataSource/IndexMetadata**, **MetadataKey**, **IndexPersistence**, **DocumentIndex/SearchResult**が含まれます（L8-L12）。
- 実行時ロジックはなく、**複雑度は低**。しかし再輸出の整合性が崩れると**ビルド破壊**のリスクがあります（型名の変更や非公開化など）。
- Rust安全性・並行性の観点はこのファイル単体では**問題なし**。内部モジュールの設計はこのチャンクでは**不明**。
- 設計上の提案として、**crate-level docs**で公開意図と安定APIの指針を明記し、必要なら**feature gating**で外部依存（推測: tantivy）を制御することを推奨。

## Overview & Purpose

このファイルはストレージ関連の複数モジュールをまとめる「mod.rs」です。役割は以下のとおりです。

- サブモジュールの宣言と公開（error, memory, metadata, metadata_keys, persistence, symbol_cache, tantivy）を行い（L1-L7）、外部からのアクセスを可能にします。
- よく使われる型を**pub use**で再輸出し、利用側が`storage::...`から直接アクセスできるようにすることで、**APIの一貫性と利便性**を高めています（L8-L12）。
- 実行時の処理は一切なく、**名前解決（コンパイル時）**のためのエントリーポイントです。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | error | pub | ストレージ層のエラー型・結果型の定義（推測） | 不明 |
| Module | memory | pub | メモリベースのストレージ実装（推測） | 不明 |
| Module | metadata | pub | インデックスやデータソースのメタデータ定義（推測） | 不明 |
| Module | metadata_keys | pub | メタデータキーの定義・管理（推測） | 不明 |
| Module | persistence | pub | 永続化戦略／インデックス持続化（推測） | 不明 |
| Module | symbol_cache | pub | シンボルキャッシュ（推測） | 不明 |
| Module | tantivy | pub | Tantivy連携ラッパー（推測） | 不明 |
| Re-export | StorageError | pub use | エラー型の公開 | Low |
| Re-export | StorageResult | pub use | 結果型の公開 | Low |
| Re-export | DataSource | pub use | データソース定義の公開 | Low |
| Re-export | IndexMetadata | pub use | インデックスメタデータの公開 | Low |
| Re-export | MetadataKey | pub use | メタデータキーの公開 | Low |
| Re-export | IndexPersistence | pub use | 永続化インターフェースの公開 | Low |
| Re-export | DocumentIndex | pub use | ドキュメントインデックスの公開 | Low |
| Re-export | SearchResult | pub use | 検索結果型の公開 | Low |

Dependencies & Interactions

- 内部依存: 本ファイルからの関数呼び出しや具体的な依存関係はこのチャンクには現れない。モジュール間の詳細な相互作用は不明。
- 外部依存（推測、非確定）: `tantivy`という名前から外部クレート「tantivy」との連携が存在する可能性があるが、このチャンクには現れないため不明。次の表は現状の確認結果です。

  | 依存先 | 用途 | 状態 |
  |--------|------|------|
  | tantivy crate | 検索エンジン連携 | 不明（このチャンクには現れない） |

- 被依存推定: 上位レイヤ（APIハンドラ、サービス、コマンド処理、CLI/HTTP）から`storage::{DocumentIndex, SearchResult, IndexPersistence, IndexMetadata, DataSource, MetadataKey, StorageResult, StorageError}`が参照される可能性が高い（推定）。

## API Surface (Public/Exported) and Data Contracts

API一覧

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| StorageError | 型（詳細不明） | ストレージ層のエラー表現 | N/A | N/A |
| StorageResult | 型（詳細不明） | 操作結果のラッパー（推測: Result<T, StorageError>） | N/A | N/A |
| DataSource | 型（詳細不明） | データソースの識別/定義 | N/A | N/A |
| IndexMetadata | 型（詳細不明） | インデックス構成/状態のメタ情報 | N/A | N/A |
| MetadataKey | 型（詳細不明） | メタデータキーの定義 | N/A | N/A |
| IndexPersistence | 型（詳細不明） | インデックスの永続化インターフェース | N/A | N/A |
| DocumentIndex | 型（詳細不明） | ドキュメントインデックス操作 | N/A | N/A |
| SearchResult | 型（詳細不明） | 検索結果表現 | N/A | N/A |

各APIの詳細説明（このチャンクに型定義は現れないため、責務のみ記述。アルゴリズム/引数/戻り値は該当なし）

1) StorageError
- 目的と責務: ストレージ関連の失敗を表す共通エラー型。
- アルゴリズム: 該当なし。
- 引数: 該当なし。
- 戻り値: 該当なし。
- 使用例:
  ```rust
  use crate::storage::{StorageError, StorageResult};

  // ストレージ操作の典型的なシグネチャ例（推奨形）
  fn rebuild_index() -> StorageResult<()> {
      // 実装はこのチャンクには現れない
      Ok(())
  }

  fn handle_error(e: StorageError) {
      // ログ出力や分類処理（詳細は不明）
      let _ = e; // 未使用抑制
  }
  ```
- エッジケース:
  - エラーの分類やエラーコードが未整備だと、ハンドリングが曖昧になる可能性。

2) StorageResult
- 目的と責務: ストレージ操作の成功/失敗を統一表現。
- アルゴリズム: 該当なし。
- 引数: 該当なし。
- 戻り値: 該当なし。
- 使用例:
  ```rust
  use crate::storage::StorageResult;

  fn save_all() -> StorageResult<()> {
      // 実装は不明
      Ok(())
  }
  ```
- エッジケース:
  - `Result`型でない場合の誤用。型定義が不明なため、ジェネリクスの使い方はこのチャンクでは断定不可。

3) DataSource
- 目的と責務: インデックス対象データの供給源の識別/設定。
- 使用例:
  ```rust
  use crate::storage::DataSource;

  // 関数パラメータで受け取る想定例
  fn attach_source(_src: &DataSource) {
      // 実装は不明
  }
  ```
- エッジケース:
  - データソースの生存期間や所有権の扱い（詳細不明）。

4) IndexMetadata
- 目的と責務: インデックスのスキーマ・状態・バージョンなどのメタ情報管理（推測）。
- 使用例:
  ```rust
  use crate::storage::IndexMetadata;

  fn update_metadata(_meta: &IndexMetadata) {
      // 実装は不明
  }
  ```
- エッジケース:
  - 互換性のないスキーマ更新の扱い（不明）。

5) MetadataKey
- 目的と責務: メタデータキーの型安全な表現（推測）。
- 使用例:
  ```rust
  use crate::storage::MetadataKey;

  fn get_value_by_key(_key: &MetadataKey) {
      // 実装は不明
  }
  ```
- エッジケース:
  - キーの重複や名前衝突の処理（不明）。

6) IndexPersistence
- 目的と責務: インデックスの保存/ロード・ジャーナル管理などの抽象化（推測）。
- 使用例:
  ```rust
  use crate::storage::IndexPersistence;

  fn persist(_p: &IndexPersistence) {
      // 実装は不明
  }
  ```
- エッジケース:
  - 書き込みの原子性やクラッシュリカバリの保証（不明）。

7) DocumentIndex
- 目的と責務: ドキュメントの追加・削除・検索のためのインデックス（推測）。
- 使用例:
  ```rust
  use crate::storage::{DocumentIndex, StorageResult};

  fn reindex(_idx: &mut DocumentIndex) -> StorageResult<()> {
      // 実装は不明
      Ok(())
  }
  ```
- エッジケース:
  - 並行更新時の整合性・ロック戦略（不明）。

8) SearchResult
- 目的と責務: 検索結果の表現（ヒット情報、スコア等の可能性はあるが不明）。
- 使用例:
  ```rust
  use crate::storage::{DocumentIndex, SearchResult, StorageResult};

  fn search(_idx: &DocumentIndex, _query: &str) -> StorageResult<SearchResult> {
      // 実装は不明
      unimplemented!()
  }
  ```
- エッジケース:
  - 大量結果のページングやスコア計算の安定性（不明）。

データ契約（不明点）
- 直列化/互換性、フィールド構成、エンコード方式はこのチャンクには現れないため不明。
- API安定性ポリシー（破壊的変更の取り扱い）も不明。

## Walkthrough & Data Flow

このファイル単体にはデータフローはありません。再輸出により、利用側が`storage::`名前空間から型を取得します。

- 名前解決フロー（コンパイル時）:
  - `use crate::storage::DocumentIndex;` → `pub use tantivy::DocumentIndex;`（L12） → `crate::storage::tantivy::DocumentIndex`に解決。
- 実行時フロー: このチャンクには現れない。内部モジュール間のデータの流れや関数呼び出しは不明。

## Complexity & Performance

- このファイルは**コンパイル時の名前解決**のみで、実行時コストはありません。
- Time/Space: O(1)/O(1)（モジュール再輸出による参照だけ）。
- ボトルネック: なし（このファイル単体）。
- スケール限界: なし。公開シンボルの数が増えるとAPIの可読性に影響する可能性はあるが、パフォーマンスとは無関係。
- 実運用負荷要因（I/O/ネットワーク/DB）: このチャンクには現れない。

## Edge Cases, Bugs, and Security

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 再輸出先の型が非公開化 | metadata::DataSourceがpubでなくなる | コンパイル時に「private type」のエラー | pub useで露出（L9） | 監視必要 |
| 型のリネーム/削除 | tantivy::DocumentIndexが削除 | コンパイル時に「unresolved import」のエラー | pub use（L12） | 監視必要 |
| 名前衝突 | 同名型を別モジュールで定義 | 名前解決の曖昧性→ビルド失敗 | 複数再輸出時の衝突対策なし | 潜在リスク |
| 不要公開 | 内部実装詳細まで公開 | APIリークにより変更困難 | 現状は型のみの公開 | 設計配慮要 |
| Feature切替 | tantivy機能を無効化 | 機能gatingでビルド制御 | このチャンクには現れない | 不明 |

セキュリティチェックリスト（このファイル単体の評価）

- メモリ安全性: 実行コードなし。Buffer overflow / Use-after-free / Integer overflowの懸念なし。
- インジェクション（SQL/Command/Path traversal）: 該当なし。
- 認証・認可: 該当なし。
- 秘密情報（ハードコード/ログ漏えい）: 該当なし。
- 並行性（Race/Deadlock）: 該当なし。

Rust特有の観点（詳細チェックリスト）

- 所有権/借用/ライフタイム: 実行コードやデータ型の定義がこのチャンクには現れないため不明。
- unsafe境界: unsafeブロックは本ファイルには存在せず（L1-L12）。内部モジュールのunsafe使用は不明。
- Send/Sync/データ競合: 不明（このチャンクには現れない）。
- await境界/キャンセル: 非同期処理は不明。
- エラー設計:
  - Result vs Option: StorageResultの正体は不明だが、一般的にはResultの別名である可能性。断定不可。
  - panic箇所: 該当なし。
  - エラー変換: From/Into実装の有無は不明。

## Design & Architecture Suggestions

- 公開ポリシーの明確化: 再輸出している型が「外部公開の安定API」なのか、「内部便宜的公開」なのかをcrate-level docsで明記。破壊的変更の指針も含める。
- プレリュードの導入: 使用頻度の高い型（StorageResult, StorageError, DocumentIndex, SearchResultなど）を`storage::prelude`でまとめると、インポートが簡潔になります。
- 名前の一貫性: `IndexPersistence`と`DocumentIndex`など抽象と具体の命名スコープが混在。階層（traits vs structs）やプレフィックスのルール化で理解性向上を期待。
- Feature gating: 外部依存が存在する場合（推測: tantivy）、`cfg(feature = "search")`等で再輸出を条件付きにし、ビルド柔軟性を高める。
- 階層化: `metadata`と`metadata_keys`が分離されている理由をドキュメント化。統合可能なら統合、分離が妥当なら責務境界を明記。

## Testing Strategy (Unit/Integration) with Examples

このファイルは構造的な公開を保証するテストが中心になります。

- コンパイル保証テスト（doctest/ユニットテスト）
  ```rust
  // tests/storage_api_compile.rs
  use crate::storage::{
      StorageError, StorageResult, DataSource, IndexMetadata, MetadataKey,
      IndexPersistence, DocumentIndex, SearchResult,
  };

  // 型がインポート可能であることのコンパイルチェック
  #[test]
  fn reexports_are_available() {
      fn _accept_types(
          _e: StorageError,
          _r: StorageResult<()>,
          _ds: DataSource,
          _im: IndexMetadata,
          _mk: MetadataKey,
          _ip: IndexPersistence,
          _di: DocumentIndex,
          _sr: SearchResult,
      ) {}
      assert!(true);
  }
  ```

- API安定性テスト（移動/リネーム検知用）
  ```rust
  // tests/api_stability.rs
  use crate::storage::{DocumentIndex, SearchResult};

  #[allow(dead_code)]
  fn requires_search(index: &DocumentIndex) -> crate::storage::StorageResult<SearchResult> {
      // 実装は不明だが、型の存在を要求することで再輸出破壊を検知
      unimplemented!()
  }
  ```

- feature gatingが導入された場合の条件付きテスト（例示）
  ```rust
  // cfg例（実際のfeature名は不明）
  #[cfg(feature = "search")]
  use crate::storage::{DocumentIndex, SearchResult};
  ```

## Complexity & Performance

- 実行コストなし（再掲）。ビルド時の型解決のみ。
- 依存モジュールのパフォーマンスはこのチャンクでは評価不可。I/O、インデックス構築、検索の計算量などは各モジュールの実装次第（不明）。

## Edge Cases, Bugs, and Security

- 再輸出の破壊的変更に注意: サブモジュール側の変更が外部API破壊を引き起こすため、**変更時はCIで公開APIチェック**を推奨。
- 非公開の型を再輸出しない: `pub use`の対象は必ず`pub`型である必要がある。
- 名前衝突の検知: 同名型が複数ある場合はモジュール経由の明示的パスに切り替え、衝突を回避。

（セキュリティチェックは上記に記載のとおり、このファイル単体では該当なし）

## Design & Architecture Suggestions

- ドキュメント化: 各再輸出の目的を`//!`モジュールレベルコメントで説明し、利用者が正しい型を把握できるようにする。
- 安定APIと内部APIの区分: `pub(crate)`活用や`pub use`対象の厳選。
- 将来的にモジュールが増える場合は**サブモジュールの階層化**（例: storage/indexing, storage/metadata）で探索性を高める。

## Testing Strategy (Unit/Integration) with Examples

- 再輸出の**存在保証**テスト（前述）。
- API破壊検知のための**コンパイルテスト**。
- 依存モジュールの実装テストは各モジュール側で実施（このチャンクには現れない）。

## Refactoring Plan & Best Practices

- crate-level docsでストレージAPIの入り口としての役割を明示。
- `storage::prelude`の導入により、利用頻度の高い型の一括インポートを提供。
- `cfg(feature = "...")`で外部連携（推測: tantivy）を制御し、ビルド構成に柔軟性を持たせる。
- 再輸出は最小限にし、**不要な内部詳細の漏れ**を防止。
- ライブラリ利用者目線の**例示コード**をドキュメントに追加。

## Observability (Logging, Metrics, Tracing)

- このファイル単体では観測処理はなし。
- 提案:
  - エラー型（StorageError）に**エラーコード**や**コンテキスト**を持たせ、トレーシング連携しやすくする。
  - インデックス操作（DocumentIndex, IndexPersistence）側で`tracing`を用いたspanを設計し、操作単位の観測を可能にする（このチャンクには現れないため提案のみ）。

## Risks & Unknowns

- 内部モジュールの詳細はこのチャンクには現れないため、アルゴリズム・スレッド安全性・I/O戦略などは不明。
- `tantivy`モジュールの外部依存の有無・バージョン互換性は不明。
- どの型がtraitかstructかenumかなどの**データ契約の詳細は不明**。
- 再輸出ポリシーの変更が外部ユーザに与える影響の範囲も不明。CIで公開API差分チェックを導入するのが望ましい。