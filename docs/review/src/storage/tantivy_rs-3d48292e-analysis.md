# tantivy.rs Review

## TL;DR

- 目的: Tantivyを用いたドキュメント・コード・シンボルの全文検索および付随メタデータの索引化を提供。ベクター検索（クラスタ割当）との連携も可能。
- 主要公開API: DocumentIndex::new/start_batch/add_document/commit_batch/search/find_symbol_by_id/find_symbols_by_name/update_cluster_assignments/get_all_indexed_paths/store_import/get_imports_for_file など。
- 複雑箇所: 検索クエリ構築（QueryParser失敗時のフォールバック、ngram＋2系統のFuzzy）、ベクター埋め込み生成とクラスタ同期、セグメント横断のクラスタキャッシュ構築。
- Rust安全性/並行性: Arc<Mutex>とArc<RwLock>で同期。ポイズンロックは一部回復（commit/delete系）だがDebugや他箇所にunwrapあり。EmbeddingGeneratorのSend/Sync境界が未明確。
- エラー設計: StorageResult/StorageErrorでラップ。I/O権限エラー時の助言メッセージあり。クエリ組み立て失敗時のフォールバックを実装。
- 重大リスク: クラスタキャッシュ世代識別がセグメント数依存で脆弱、SymbolKind文字列の手動マッピングの不一致、scope_contextの文字列パースが脆弱、Embedding生成は同期的で大規模時の遅延懸念。
- テスト: 設計意図に沿って幅広く整備（言語フィルタ、ngram部分一致、fuzzy typo耐性、インポート永続化、クラスタキャッシュ、メタデータ）。ベクター割当の統合テスト（update_cluster_assignmentsの一連動作）は拡充余地あり。

## Overview & Purpose

このモジュールは、Tantivyによる高速全文検索インデックスを構築・操作し、コードシンボル・関係・ファイル情報・メタデータ・インポートメタデータを同一インデックス上で管理します。さらに、オプションでベクター検索エンジン（Arc<Mutex<VectorSearchEngine>）と埋め込み生成器（Arc<dyn EmbeddingGenerator>）を接続し、クラスタIDの同期・キャッシュによりセマンティック検索の高速化を狙います。

主な責務:
- Schema定義（IndexSchema）
- インデックスの作成・読み込み（DocumentIndex::new）
- バッチ書き込み（start_batch/add_document/commit_batch）
- 検索（search、find_*群）
- 関係・ファイル・インポート・メタデータの保管/取得
- ベクター埋め込み生成後のクラスタ割当のインデックス反映（update_cluster_assignments）
- クラスタキャッシュ構築・ウォームアップ

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | IndexSchema | pub | Tantivy Schemaの全Field定義とbuild() | Med |
| Struct | VectorMetadata | pub | ベクターID・クラスタID・埋め込みモデル版を保持、JSON序列化 | Low |
| Struct | VectorMetadataJson | private | VectorMetadataの内部JSON表現 | Low |
| Struct | ClusterCache | private | SegmentOrdinal→ClusterId→DocIdのキャッシュ、世代管理 | Med |
| Struct | SearchResult | pub | 検索結果の転送用DTO（得点・ハイライト等） | Low |
| Struct | TextHighlight | pub | ハイライト範囲保持 | Low |
| Struct | DocumentIndex | pub | インデックス本体、Reader/Writer管理、検索/保管API、ベクター連携 | High |

### Dependencies & Interactions

- 内部依存
  - DocumentIndex → IndexSchema（全フィールドアクセス）
  - DocumentIndex → ClusterCache（クラスタキャッシュの構築/取得）
  - DocumentIndex → EmbeddingGenerator/VectorSearchEngine（ベクター生成・割当取得・同期）
  - DocumentIndex → document_to_symbol（Tantivy Document→crate::Symbolの変換）
- 外部依存（主要）
  - tantivy: Index, IndexReader, IndexWriter, Schema, Query系, Collector系, tokenizer (Ngram)
  - serde/serde_json: VectorMetadataのJSON序列化
  - std::sync: Arc, Mutex, RwLock（並行制御）
  - crate::{FileId, SymbolId, SymbolKind, Relationship/RelationKind, RelationshipMetadata, vector::{...}, parsing::{Import, registry}, config::Settings}
- 被依存推定
  - ストレージ層・検索UI・解析パイプライン（インデックス投入）・コード関係解析・インポート解決レイヤ。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| IndexSchema::build | fn build() -> (Schema, IndexSchema) | Tantivyスキーマ生成 | O(1) | O(1) |
| DocumentIndex::new | fn new(path, settings) -> StorageResult<Self> | インデックス作成/読み込み | O(1) | O(1) |
| DocumentIndex::with_vector_support | fn with_vector_support(self, Arc<Mutex<VectorSearchEngine>>, path) -> Self | ベクター検索機能の有効化 | O(1) | O(1) |
| DocumentIndex::with_embedding_generator | fn with_embedding_generator(self, Arc<dyn EmbeddingGenerator>) -> Self | 埋め込み生成器の設定 | O(1) | O(1) |
| DocumentIndex::start_batch | fn start_batch(&self) -> StorageResult<()> | Writer開始＋IDカウンタ初期化 | O(1) | O(1) |
| DocumentIndex::add_document | fn add_document(&self, ...) -> StorageResult<()> | シンボル1件の索引化 | O(1) | O(1) |
| DocumentIndex::commit_batch | fn commit_batch(&self) -> StorageResult<()> | バッチコミット＋Readerリロード＋後処理 | O(n)※ | O(1) |
| DocumentIndex::search | fn search(&self, query, limit, kind, module, language) -> StorageResult<Vec<SearchResult>> | 全文検索（ngram＋fuzzy＋フィルタ） | O(k + log n)※ | O(k) |
| DocumentIndex::find_symbol_by_id | fn find_symbol_by_id(&self, id) -> StorageResult<Option<crate::Symbol>> | IDでシンボル取得 | O(log n) | O(1) |
| DocumentIndex::find_symbols_by_name | fn find_symbols_by_name(&self, name, language) -> StorageResult<Vec<crate::Symbol>> | 名前（STRING）で厳密検索 | O(log n + k) | O(k) |
| DocumentIndex::update_cluster_assignments | fn update_cluster_assignments(&self) -> StorageResult<()> | ベクターエンジンのクラスタIDをインデックスに同期 | O(m · log n) | O(1) |
| DocumentIndex::get_all_indexed_paths | fn get_all_indexed_paths(&self) -> StorageResult<Vec<PathBuf>> | 監視用にfile_infoの全パス一覧 | O(f) | O(f) |
| DocumentIndex::store_import | fn store_import(&self, &Import) -> StorageResult<()> | インポートメタデータの保存 | O(1) | O(1) |
| DocumentIndex::get_imports_for_file | fn get_imports_for_file(&self, file_id) -> StorageResult<Vec<Import>> | ファイル別インポート取得 | O(log n + k) | O(k) |
| DocumentIndex::delete_imports_for_file | fn delete_imports_for_file(&self, file_id) -> StorageResult<()> | ファイル別インポート削除 | O(log n) | O(1) |
| DocumentIndex::clear | fn clear(&self) -> StorageResult<()> | 全ドキュメント削除 | O(n) | O(1) |
| DocumentIndex::path | fn path(&self) -> &Path | インデックスディレクトリパス取得 | O(1) | O(1) |
| VectorMetadata::new | fn new(embedding_version) -> Self | 空メタデータ生成 | O(1) | O(1) |
| VectorMetadata::with_vector | fn with_vector(vector_id, cluster_id, embedding_version) -> Self | ベクター付きメタデータ生成 | O(1) | O(1) |
| VectorMetadata::to_json | fn to_json(&self) -> StorageResult<String> | JSON序列化 | O(1) | O(1) |
| VectorMetadata::from_json | fn from_json(&str) -> StorageResult<Self> | JSON逆序列化 | O(1) | O(1) |

※ commit_batchのO(n): クラスタキャッシュ再構築（has_vector/cluster_idのFASTフィールド走査）でセグメント内の全docを走査します。  
※ searchの計算量はTantivyのインデックス構造（倒置インデックス）依存で、おおむね上記のオーダ。kは返却件数。

以下、主要APIの詳細（抜粋）

1) DocumentIndex::new
- 目的と責務
  - インデックスディレクトリの作成/オープン、Schema生成、Reader初期化、ngramトークナイザ登録を行います。
- アルゴリズム
  - 設定からheapサイズとリトライ回数を取得
  - 既存インデックスか判定（meta.json）してopen or create
  - NgramTokenizer(min_gram=3,max_gram=10)登録
  - IndexReaderをManualリロードポリシーで構築し、必要ならreload
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | index_path | impl AsRef<Path> | インデックス格納先 |
  | settings | &crate::config::Settings | heap/リトライ設定 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | StorageResult<Self> | 初期化済みDocumentIndex |
- 使用例
  ```rust
  let temp_dir = tempfile::TempDir::new().unwrap();
  let settings = crate::config::Settings::default();
  let index = DocumentIndex::new(temp_dir.path(), &settings)?;
  ```
- エッジケース
  - 権限不足/ファイルロック: Tantivy open/create失敗時はStorageErrorへ伝播
  - 既存インデックス読み込み: meta.json存在時にreader.reload()を実施

2) DocumentIndex::start_batch
- 目的と責務
  - IndexWriterを作成し、シンボル/ファイルIDのペンディングカウンタを初期化
- アルゴリズム
  - writerがNoneならcreate_writer_with_retry()で作成
  - query_metadataでSymbolCounter/FileCounterを取得し+1をペンディングに設定
- 引数/戻り値
  | 引数 | 型 | 説明 |
  |------|----|------|
  | &self |  | 内部状態にwriter設定 |
  | 戻り値 | StorageResult<()> | 成否 |
- 使用例
  ```rust
  index.start_batch()?;
  ```
- エッジケース
  - ロックポイズン: StorageError::LockPoisonedで返す
  - writerが既に存在: 何もしない

3) DocumentIndex::add_document
- 目的と責務
  - 1シンボルをインデックスに追加。フィルタ用STRINGフィールドと全文検索用TEXTフィールド（name_text）双方に投入。ベクターサポートがある場合は初期vector_id/cluster_id/has_vectorも設定。
- アルゴリズム
  - doc_type="symbol"など各フィールド設定
  - self.has_vector_support()ならvector_id=SymbolId, cluster_id=0, has_vector=0
  - writer.add_document(doc)
  - 埋め込み生成器が設定されている場合、pending_embeddingsにシンボルテキストをpush
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | symbol_id | SymbolId | シンボルID |
  | name | &str | シンボル名（STRING/全文検索両対応） |
  | kind | SymbolKind | 種別 |
  | file_id | FileId | 所属ファイル |
  | file_path | &str | 文字列パス |
  | line/column/end_line/end_column | u32/u16 | 範囲 |
  | doc_comment/signature/context | Option<&str> | 付随情報 |
  | visibility | crate::Visibility | 可視性（u64保存） |
  | scope_context | Option<crate::ScopeContext> | スコープ（文字列化保存） |
  | language_id | Option<&str> | 言語識別子（STRING保存） |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | StorageResult<()> | 成否 |
- 使用例
  ```rust
  index.add_document(
      SymbolId::new(1).unwrap(),
      "parse_json",
      SymbolKind::Function,
      FileId::new(1).unwrap(),
      "src/parser.rs",
      42, 5, 50, 0,
      Some("Parse JSON string into a Value"),
      Some("fn parse_json(input: &str) -> StorageResult<Value, Error>"),
      "crate::parser",
      None,
      crate::Visibility::Public,
      Some(crate::ScopeContext::Module),
      Some("rust"),
  )?;
  ```
- エッジケース
  - NoActiveBatch: writerがNoneならエラー
  - 文字列の長さ/空文字: Tantivy側で許容、検索結果品質に影響

4) DocumentIndex::commit_batch
- 目的と責務
  - バッチをコミットし、Readerをリロード。ペンディングカウンタリセット。ベクター埋め込み後処理/post_commit_vector_processingとクラスタキャッシュ構築を実行。
- アルゴリズム
  - writer_lock.take()→writer.commit()
  - 権限エラーなら詳細助言メッセージで返す
  - reader.reload()
  - pending_symbol_counter/pending_file_counterをNone化
  - ベクターサポート有＆EmbeddingGenerator有ならpost_commit_vector_processing()
  - has_vector/cluster_id FASTフィールド走査でbuild_cluster_cache()
- 引数/戻り値
  | 引数 | 型 | 説明 |
  |------|----|------|
  | &self |  | 内部状態更新 |
  | 戻り値 | StorageResult<()> | 成否 |
- 使用例
  ```rust
  index.commit_batch()?;
  ```
- エッジケース
  - I/O権限エラー: 詳細説明を含むStorageError::General
  - ロックポイズン: eprintlnで警告し回復を試みる

5) DocumentIndex::search
- 目的と責務
  - クエリ文字列をQueryParserで解析（失敗時フォールバックでTermQuery群）、ngram対象のFuzzy(距離1)と非トークナイズnameへのFuzzy(距離1)を併用し、doc_type="symbol"等のフィルタで検索。
- アルゴリズム
  - QueryParserでname_text/doc_comment/signature/contextを対象にparse
  - 失敗時はTermQueryのOccur::Should群でフォールバック
  - name_textのFuzzyTermQuery（ngram）＋name（STRING）へのFuzzyTermQuery（Whole word）を併用
  - kind/module/languageフィルタ（Occur::Must）適用
  - TopDocs::with_limit(limit)収集→SearchResultへ整形
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | query_str | &str | 検索語 |
  | limit | usize | 上限件数 |
  | kind_filter | Option<SymbolKind> | 種別フィルタ |
  | module_filter | Option<&str> | モジュールフィルタ |
  | language_filter | Option<&str> | 言語フィルタ |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | StorageResult<Vec<SearchResult>> | 検索結果 |
- 使用例
  ```rust
  let results = index.search("Archive", 10, None, None, Some("csharp"))?;
  ```
- エッジケース
  - Tantivy構文衝突（例: "Vec<T>"）→フォールバックTermQueryが実施
  - 未知kind文字列→SearchResult整形で既定にFunctionへフォールバック（品質リスク）

6) DocumentIndex::find_symbol_by_id / find_symbols_by_name（厳密一致）
- 目的: IDまたは名前（STRING）での厳密検索。言語フィルタ対応版あり。
- アルゴリズム: TermQueryでdoc_type="symbol"をMust、id/nameをMust、必要ならlanguageもMust
- 使用例
  ```rust
  let s = index.find_symbol_by_id(SymbolId::new(1).unwrap())?;
  let vs = index.find_symbols_by_name("main", Some("rust"))?;
  ```
- エッジケース
  - 名前の部分一致はname_text（search）を使うべき。find_symbols_by_nameは厳密一致のため、"Archive"では"ArchiveService"を返さない。

7) DocumentIndex::update_cluster_assignments
- 目的: ベクターエンジンから全クラスタ割当を取得し、既存ドキュメントをdelete+re-addでcluster_id/has_vectorを更新。
- アルゴリズム
  - engine.lock→get_all_cluster_assignments→unlock
  - VectorId→SymbolId変換
  - reader.searcherでsymbol_id検索→doc取得→writer.delete_term→新doc（全フィールドコピー、cluster_id/has_vector差し替え）→writer.add_document
  - commit_batch()
- エッジケース
  - 対象docが見つからない場合: スキップ（明示的ログなし）
  - ベクターID→SymbolID変換失敗: スキップ

8) DocumentIndex::get_all_indexed_paths / store_import / get_imports_for_file / delete_imports_for_file
- 目的: ファイル監視や解決層のためのメタデータ操作（doc_type="file_info"/"import"）
- 備考: インポートは純粋に保存のみ。解決は別レイヤ。

9) VectorMetadata::to_json / from_json
- 目的: ベクターメタデータをTantivy保存用にJSON化。エラーはStorageError::Serialization。
- 使用例
  ```rust
  let m = VectorMetadata::with_vector(VectorId::new(42).unwrap(), ClusterId::new(2).unwrap(), 1);
  let json = m.to_json()?;
  let restored = VectorMetadata::from_json(&json)?;
  ```

## Walkthrough & Data Flow

### インデックス投入からコミット、ベクター処理、クラスタキャッシュまで

```mermaid
flowchart TD
    A[start_batch] --> B[add_document (N件)]
    B --> C[commit_batch]
    C -->|writer.commit + reader.reload| D{has_vector_support?}
    D -->|No| E[Done]
    D -->|Yes| F{embedding_generator?}
    F -->|No| G[build_cluster_cache]
    F -->|Yes| H[post_commit_vector_processing]
    H --> I[index_vectors(engine)]
    I --> J[build_cluster_cache]
    J --> K[Done]
```

上記の図は`commit_batch`関数とその後続呼び出し（post_commit_vector_processing, build_cluster_cache）の主要分岐を示す（行番号不明：このチャンクには行番号情報がない）。

ポイント:
- post_commit_vector_processingはpending_embeddingsを取り出し、埋め込み生成→engineへindex_vectors。ここではクラスタ割当の更新は行わず、後でupdate_cluster_assignments()が必要。
- build_cluster_cacheはFASTフィールド（cluster_id/has_vector）から各セグメントを走査してキャッシュを再構築。

### 検索クエリ構築の分岐

```mermaid
flowchart TD
    S[search(query_str,...)] --> P{QueryParser.parse成功?}
    P -->|Yes| Q[main_query(パーサ結果)]
    P -->|No| R[フォールバック: name_text/doc_comment/signature/contextへのTermQuery(Should)]
    Q --> T[ngram Fuzzy(距離1)]
    R --> T
    T --> U[whole-word nameへのFuzzy(距離1)]
    U --> V[doc_type='symbol' Must]
    V --> W{kind/module/language フィルタ}
    W --> X[BooleanQuery完成]
    X --> Y[TopDocs(limit)]
    Y --> Z[SearchResult整形]
```

上記の図は`search`関数の主要分岐を示す（行番号不明）。

## Complexity & Performance

- Indexing
  - start_batch/add_document: O(1)（TantivyへのDocument構築とバッファ投入）
  - commit_batch: writer.commitはI/Oバウンド。さらにbuild_cluster_cacheで全セグメントの全docを走査（has_vector==1のみ集計）→O(n_docs)。大規模インデックスでコミット時間が増大。
- Search
  - Tantivyの倒置インデックスにより、一般的にはO(log n + postings merge)の範囲。フィルタ数増加でクエリ結合コストは増加。TopDocs(limit)に比例してO(k)の結果整形。
- Vector processing
  - post_commit_vector_processing: texts→embeddings生成は外部推定（CPU/GPU）。N件バルク生成＋engine.index_vectorsの時間が支配。
  - update_cluster_assignments: 割当m件に対し、各シンボルlookup→delete+re-add→O(m·log n)程度＋commit_batchでO(n_docs)のキャッシュ再構築。
- スケール限界・ボトルネック
  - コミット後の毎回クラスタキャッシュ再構築がO(n_docs)。ベクター割当更新頻度が高い場合、コミット時間がボトルネック。
  - Embedding生成が同期的で、投入頻度が高いと遅延。非同期化やバッチサイズ調整が望ましい。
- 実運用負荷要因
  - I/O（mmapディレクトリ）、ファイルロック/権限エラー
  - ネットワークは不明（このチャンクでは外部サービス呼び出しなし）
  - ベクターエンジンのディスク/メモリ（外部crate::vector）

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| QueryParser構文エラー | "Vec<T>" | フォールバックでTermQuery検索 | searchでBooleanQuery(Should)にフォールバック | 対応済 |
| 名前の厳密検索に部分一致を期待 | "Archive" | 0件（厳密一致） | find_symbols_by_nameはSTRING TermQuery | 対応済（仕様） |
| fuzzy長語の1文字欠落 | "ArchivService" | "ArchiveService"にヒット | nameフィールドへのFuzzyTermQuery追加 | 対応済 |
| ベクター生成後クラスタ未同期 | pending_embeddings有 | update_cluster_assignmentsが必要 | post_commit_vector_processingは同期しない旨をコメント | 要注意（手動呼び出し） |
| SymbolKind文字列不一致 | kind="Class"等 | 期待どおりの種別 | search結果整形で手動matchに不足→Functionにフォールバック | 要改善 |
| scope_context文字列パース失敗 | "Local { ... }" | 適切なScopeContext | 手動文字列抽出が脆弱 | 要改善 |
| ロックポイズン発生 | Mutex/RwLock | エラー復帰/回復 | 一部はinto_innerで回復、Debugでunwrapあり | 部分対応 |
| writer未開始での操作 | add_document/delete_symbol等 | NoActiveBatch | 明示的エラー | 対応済 |

セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow: 文字列操作はTantivy/serdeに委譲、手動パース（scope_context）でインデックス保存時は安全だが取り出しパースは脆さあり。Overflowはなし。
  - Use-after-free: Arc/Mutex/RwLockで安全。unsafe不使用。
  - Integer overflow: u64→u32変換はSymbolId::new等でOptionチェック。to_symbol/from_symbol等で安全化。
- インジェクション
  - SQL/Command: 不使用。Tantivy Term/Query構築は構文解釈に失敗してもフォールバック。
  - Path traversal: file_pathは文字列で格納・返却のみ。外部ファイル操作なし。
- 認証・認可
  - 該当なし（このチャンクには現れない）。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: eprintlnでパスやエラーメッセージが表示されるが機密情報は含まず。
- 並行性
  - Race condition: writer（Mutex）/cluster_cache（RwLock）で基本保護。Debug fmtでunwrap使用はポイズン時panicの可能性。
  - Deadlock: 2種類のロック（writer, cluster_cache, vector_engine）のネストは限定的。post_commit_vector_processingでengineロック→drop→後続処理。致命的デッドロックの兆候は少ない。
  - Send/Sync境界: EmbeddingGeneratorはdyn traitにSend/Sync境界が付与されていない（このチャンクでは未保証）。マルチスレッド利用時の安全性が不明。

Rust特有の観点
- 所有権/値の移動
  - commit_batchでwriter_lock.take()によりIndexWriterをムーブしてcommit（関数: commit_batch）。
- 借用/ライフタイム
  - Index/Reader/Searcherの借用はスコープ内完結。明示的ライフタイム不要。
- unsafe境界
  - unsafe未使用。
- 並行性・非同期
  - Arc<Mutex<VectorSearchEngine>>/Arc<RwLock<Option<ClusterCache>>>で共有保護。
  - await等の非同期は不使用。埋め込み生成は同期。
- エラー設計
  - ResultをStorageResultで統一。unwrap/expectは主にテスト。運用コードでは?とエラー変換を採用。
  - エラー変換: tantivy::TantivyError等をStorageErrorへ（into）。

## Design & Architecture Suggestions

- クラスタキャッシュ世代管理の改善
  - 現状は「searcher.segment_readers().len()」を世代として使用。セグメント数が変わらない更新で不一致が起きる可能性。IndexReader::reload後のgenerationやopstampなど、より堅牢なメトリクスを用いる。
- SymbolKind/ScopeContextのシリアライズ/デシリアライズ統一
  - 現在はformat!("{kind:?}")で文字列化、検索時/復元時の手動match。serdeでEnumをバージョン安全に保存すべき。
  - scope_contextは手動文字列解析で脆弱。Serde（Tagged enum）で保存/復元することで保守性向上。
- EmbeddingGeneratorのトレイト境界
  - Arc<dyn EmbeddingGenerator>にSend + Sync境界を追加し、並行安全性を保証（trait定義側変更要）。
- ベクター同期フローの整理
  - post_commit_vector_processing後にupdate_cluster_assignmentsを自動トリガーする「ベクター更新ジョブ」層を導入。大規模投入時は非同期化/バックグラウンド処理でUI応答性を確保。
- 検索結果のkindマッピング
  - SearchResultのkind構築はSymbolKind::from_str_with_defaultを使用し一貫性を持たせる（現在のsearchでは独自match）。
- Writer/BatchのRAII化
  - BatchGuardを導入し、drop時に安全なcommit/rollback（設定）を行う。NoActiveBatchミスの回避、例外安全性向上。
- Readerウォームアップ/キャッシュ
  - reload_and_warmでクラスタキャッシュのみウォーム。頻出クエリのプリフェッチ、トークナイザのプリウォーム等拡充余地あり。

## Testing Strategy (Unit/Integration) with Examples

既存テストは広範:
- インデックス作成/検索/言語フィルタ/インポート永続化/削除/クラスタキャッシュの整合性・性能、ngram部分一致・fuzzyタイポ耐性。

追加推奨テスト:
- update_cluster_assignments統合
  - シンボル追加→post_commit_vector_processingでembedding生成→engineにクラスタ割当設定→update_cluster_assignments→cluster_id/has_vectorが1に更新されること。
  ```rust
  // 疑似コード（概念）
  index.start_batch()?;
  index.add_document(...)?; // has_vector=0, cluster_id=0
  index.commit_batch()?;    // post_commit_vector_processing → engineにベクター投入
  {
      let engine = index.vector_engine().unwrap().lock().unwrap();
      engine.assign_cluster(VectorId::new(symbol_id.value()).unwrap(), ClusterId::new(3).unwrap());
  }
  index.update_cluster_assignments()?; // 書き戻し
  // cluster_id==3, has_vector==1を検証するクエリ（FASTフィールド参照）を追加
  ```
- commit_batchエラー経路
  - 権限エラー（可能ならモック）でエラーメッセージの内容検証。
- ロックポイズン復旧
  - Mutexのpoison状況を再現して、commit/deleteの回復処理が動くかを確認（ユニットレベルで可能）。

## Refactoring Plan & Best Practices

- スキーマ/フィールドの型安全化
  - doc_type、kind、scope_context、languageはEnum or newtypeでserde保存。検索側でも型安全なフィルタを構築。
- 検索クエリビルダを関数分割
  - main_query/fallback/fuzzy_ngram/fuzzy_whole_word/filtersをビルダパターン化。テスト容易性・保守性向上。
- ベクター処理の非同期化
  - pending_embeddingsをチャネルでワーカへ渡し、生成→engine登録→update_cluster_assignmentsの非同期パイプライン化。commit時間短縮。
- クラスタキャッシュの差分更新
  - 毎回フル走査ではなく、更新されたdocのみ反映する差分キャッシュ更新。Readerのセグメントメタ情報を活用。
- ログ/メトリクス導入
  - eprintlnからtracingへ。commit/検索/キャッシュ構築時間のメトリクスを計測。Prometheus等連携。
- APIの失敗時返し改善
  - update_cluster_assignmentsで見つからないsymbol_idがあれば警告ログ。品質改善。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 現状eprintln。tracing（info/warn/error/span）導入で検索クエリ内容・commit時間・キャッシュサイズ・ベクター処理件数を可視化。
- メトリクス
  - commit_batch時間、build_cluster_cache時間・総doc数・クラスタ数、searchクエリレイテンシ、TopDocs件数分布。
- トレーシング
  - バッチ開始→投入→コミット→ベクター生成→クラスタ更新のスパンを関連付け、障害時の根因解析を支援。

## Risks & Unknowns

- Reader/セグメント世代識別
  - 現在の世代判定がセグメント数依存。Readerの内部世代・opstamp使用が望ましいが、このチャンクでは利用情報が不明。
- EmbeddingGenerator/VectorSearchEngineの仕様
  - 生成時間・スレッド安全性・エラー特性はこのチャンクには現れない。不明。
- ベクター割当完了タイミング
  - post_commit_vector_processing後にupdate_cluster_assignmentsの呼び出し責務が誰にあるかは不明（このチャンクでは自動起動なし）。
- scope_contextの厳密復元
  - "Local { ... }"の文字列解析は入力フォーマットに依存。今後の仕様変更に脆弱。
- ハイライト機能
  - SearchResult.highlightsは未実装（TODOコメント）。必要なフィールド・整合は不明。

以上を踏まえ、本ファイルはTantivyの利用を適切に抽象化し、言語フィルタやtypo耐性、インポート永続化、関係保存など必要な検索・保存機能を広くカバーしています。一方で、型安全な保存/復元（serde）、ベクター同期の非同期化、クラスタキャッシュの世代管理改善、観測性の向上により、運用規模拡大時の堅牢性・性能・保守性が大きく改善されます。