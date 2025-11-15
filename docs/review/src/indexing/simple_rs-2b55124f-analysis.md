# simple.rs Review

## TL;DR

- 目的: Tantivyベースの単一ソース・オブ・トゥルース索引器。抽出したシンボル・関係・ファイル情報をTantivyに保存し、言語ごとのLanguageBehaviorで解決（imports/継承/可視性）を行う。ベクター/セマンティック検索も任意で併用。
- 主要公開API: index_file/index_directory/search/semantic_search_docs/get_symbol_context/build_symbol_cache/resolve_cross_file_relationships（内部公開）など。全てTantivyに即時反映するバッチ型コミットを提供。
- コアロジック: 2段階フロー。1) 解析・抽出・保存（未解決関係を蓄積）→ 2) resolve_cross_file_relationshipsでLanguageBehaviorとインポートを考慮して解決・保存。Callsはメタデータ付き・双方向関係自動生成。
- 安全性/並行性: 共有状態はArc/Mutexで保護（VectorEngine, SimpleSemanticSearch）。DocumentIndexはエラーをIndexErrorに変換。lock().unwrap()が多く、パニックリスクあり（改善余地）。
- 複雑箇所: resolve_cross_file_relationships（多分岐・多依存）、メソッド呼び出しの高度解決（MethodCall＋メタデータ、LanguageBehaviorコンテキスト、外部呼び出し除外）、バッチコミット後の埋め込み生成とシンボルキャッシュ構築。
- 重大リスク: ロック毒化時のunwrapパニック、言語ビヘイビア未格納時の解決失敗、セマンティック/ベクターの保存非同期性による不整合、Windowsのキャッシュファイルロック。可視性/モジュール近接の簡易実装による誤解決も潜在。
- 既知のバグ対策: importのTantivy永続化・外部呼び出しの除外・シンボルIDのペンディングカウンタ更新・埋め込み削除後即保存など、重要箇所に明示的Fixが入っている（例: update_pending_symbol_counter, store_import, remove_embeddings後save）。

## Overview & Purpose

このファイルは、Tantivyを唯一のストレージとして利用するSimpleIndexerの実装である。主な責務は以下:

- ファイル探索・言語判定・パーサでのシンボル/関係抽出
- Tantivy（DocumentIndex）へのファイル・シンボル・関係・メタデータ（インポート含む）の保存/更新/削除
- 未解決関係の二段階解決（言語ごとのLanguageBehavior + ResolutionScope）
- セマンティック検索（SimpleSemanticSearch）とベクター検索（VectorSearchEngine, EmbeddingGenerator）の統合
- 進捗表示、インデックスパスのトラッキング、キャッシュ（SymbolHashCache）構築

目的は、プロジェクト横断で高速かつ正確なコードナレッジ検索（全文/構造/関係/セマンティック）を行える堅牢なインデクサを提供すること。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Macro | debug_print! | crate内 | 設定に応じたデバッグ出力 | Low |
| Struct | TantivyTransaction | pub | 互換API用ダミートランザクション | Low |
| Struct | UnresolvedRelationship | private | 未解決関係の保持（2段階解決用） | Low |
| Struct | SimpleIndexer | pub | 解析・保存・解決・検索・キャッシュの中核 | High |

### Dependencies & Interactions

- 内部依存
  - SimpleIndexer::index_file → index_file_internal → reindex_file_content → extract_and_store_symbols/store_symbol/extract_and_store_relationships → update_symbol_counter
  - コミット後: commit_tantivy_batch → process_pending_embeddings → build_symbol_cache
  - バッチ完了後: resolve_cross_file_relationships（未解決関係の確定）
  - 構築系: build_symbol_cache/clear_symbol_cache/load_symbol_cache
  - 検索系: search, semantic_search_docs*, get_symbol*, get_dependencies/dependents, get_symbol_context など
- 外部依存（抜粋）

| 依存モジュール | 用途 | 備考 |
|---|---|---|
| crate::storage::DocumentIndex | Tantivyラッパ、全永続化I/O | すべての保存/検索の要 |
| crate::parsing::{ParserFactory, LanguageParser, LanguageBehavior, ResolutionScope, get_registry} | 言語別解析・解決 | imports/継承/可視性/式型などを管理 |
| crate::semantic::SimpleSemanticSearch | セマンティック検索 | Arc<Mutex>で共有 |
| crate::vector::{VectorSearchEngine, EmbeddingGenerator, create_symbol_text} | ベクター検索統合 | Arc<Mutex>（エンジン）＋Embed生成 |
| crate::storage::symbol_cache::* | 高速ハッシュキャッシュ | Mmapキャッシュの構築/ロード |
| crate::{IndexError, IndexResult, Relationship, RelationKind, Symbol, SymbolId…} | 型群 | エラー/モデル |

- 被依存推定
  - CLI/サービスの「index」「search」コマンド
  - IDE拡張のシンボル/関係ナビゲーション
  - サーバサイドのドキュメント/ナレッジ検索API

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | fn new() -> Self | 既定設定でインデクサ生成 | O(1) | O(1) |
| with_settings | fn with_settings(settings: Arc<Settings>) -> Self | 設定付インデクサ生成 | O(1) | O(1) |
| with_settings_lazy | fn with_settings_lazy(Arc<Settings>) -> Self | 遅延初期化（高速起動） | O(1) | O(1) |
| with_vector_search | fn with_vector_search(self, VectorSearchEngine, Arc<dyn EmbeddingGenerator>) -> Self | ベクター検索有効化 | O(1) | O(1) |
| has_vector_search | fn has_vector_search(&self) -> bool | ベクター検索の有無 | O(1) | O(1) |
| enable_semantic_search | fn enable_semantic_search(&mut self) -> IndexResult<()> | セマンティック検索準備 | O(1) | O(1) |
| has_semantic_search | fn has_semantic_search(&self) -> bool | セマンティック検索の有無 | O(1) | O(1) |
| semantic_search_embedding_count | fn semantic_search_embedding_count(&self) -> IndexResult<usize> | 埋め込み件数 | O(1) | O(1) |
| get_semantic_metadata | fn get_semantic_metadata(&self) -> Option<SemanticMetadata> | メタデータ取得 | O(1) | O(1) |
| save_semantic_search | fn save_semantic_search(&self, path: &Path) -> Result<(), SemanticSearchError> | セマンティック状態保存 | O(N) | O(1) |
| load_semantic_search | fn load_semantic_search(&mut self, path: &Path, info: bool) -> IndexResult<bool> | セマンティック状態読込 | O(N) | O(1) |
| start_tantivy_batch | fn start_tantivy_batch(&self) -> IndexResult<()> | バッチ開始 | O(1) | O(1) |
| commit_tantivy_batch | fn commit_tantivy_batch(&mut self) -> IndexResult<()> | バッチ確定＋埋め込み処理＋キャッシュ構築 | O(S+E) | O(S) |
| begin_transaction | fn begin_transaction(&self) -> IndexTransaction | 互換トランザクション開始 | O(1) | O(1) |
| commit_transaction | fn commit_transaction(&mut self, IndexTransaction) -> IndexResult<()> | 互換トランザクション確定 | O(S+E) | O(S) |
| rollback_transaction | fn rollback_transaction(&mut self, IndexTransaction) | 互換トランザクション破棄 | O(1) | O(1) |
| index_file | fn index_file(&mut self, path: impl AsRef<Path>) -> IndexResult<IndexingResult> | 単一ファイル索引 | O(S+R) | O(S+R) |
| index_file_with_force | fn index_file_with_force(&mut self, path, force: bool) -> IndexResult<IndexingResult> | 強制再索引 | O(S+R) | O(S+R) |
| index_file_no_resolve | fn index_file_no_resolve(&mut self, path) -> IndexResult<IndexingResult> | 解決遅延で索引 | O(S+R) | O(S+R) |
| remove_file | fn remove_file(&mut self, path) -> IndexResult<()> | ファイルの全削除（関係/インポート/埋め込み含む） | O(S+R) | O(1) |
| search | fn search(&self, query, limit, kind_filter, module_filter, language_filter) -> IndexResult<Vec<SearchResult>> | 自然言語/全文検索 | O(logN + K) | O(K) |
| document_count | fn document_count(&self) -> IndexResult<u64> | ドキュメント数 | O(1) | O(1) |
| index_directory | fn index_directory(&mut self, dir, progress, dry_run) -> IndexResult<IndexStats> | ディレクトリ索引 | O(F·(S+R)) | O(S) |
| index_directory_with_force | fn index_directory_with_force(&mut self, dir, progress, dry_run, force) -> IndexResult<IndexStats> | 強制再索引 | O(F·(S+R)) | O(S) |
| index_directory_with_options | fn index_directory_with_options(&mut self, dir, progress, dry_run, force, max_files) -> IndexResult<IndexStats> | 詳細オプション | O(F·(S+R)) | O(S) |
| semantic_search_docs | fn semantic_search_docs(&self, query, limit) -> IndexResult<Vec<(Symbol,f32)>> | ドキュメントのセマンティック検索 | O(logE + L) | O(L) |
| semantic_search_docs_with_language | fn semantic_search_docs_with_language(&self, query, limit, language_filter) -> IndexResult<Vec<(Symbol,f32)>> | 言語フィルタ付 | O(logE + L) | O(L) |
| semantic_search_docs_with_threshold | fn semantic_search_docs_with_threshold(&self, query, limit, threshold) -> IndexResult<Vec<(Symbol,f32)>> | 閾値付 | O(logE + L) | O(L) |
| semantic_search_docs_with_threshold_and_language | fn ... | 閾値＋言語フィルタ | O(logE + L) | O(L) |
| get_called_functions | fn get_called_functions(&self, symbol_id) -> Vec<Symbol> | 呼出先一覧 | O(outdeg) | O(outdeg) |
| get_called_functions_with_metadata | fn get_called_functions_with_metadata(&self, symbol_id) -> Vec<(Symbol,Option<RelationshipMetadata>)> | 呼出先＋メタ | O(outdeg) | O(outdeg) |
| get_calling_functions | fn get_calling_functions(&self, symbol_id) -> Vec<Symbol> | 呼出元一覧 | O(indeg) | O(indeg) |
| get_calling_functions_with_metadata | fn ... | 呼出元＋メタ | O(indeg) | O(indeg) |
| get_symbol_context | fn get_symbol_context(&self, symbol_id, include) -> Option<SymbolContext> | 関係を束ねた文脈 | O(deg) | O(deg) |
| get_implementations | fn get_implementations(&self, trait_id) -> Vec<Symbol> | 実装一覧 | O(indeg) | O(indeg) |
| get_all_symbols | fn get_all_symbols(&self) -> Vec<Symbol> | 最大1万件の全取得 | O(N) | O(N) |
| get_dependencies | fn get_dependencies(&self, symbol_id) -> HashMap<RelationKind,Vec<Symbol>> | 依存 | O(deg) | O(deg) |
| get_dependents | fn get_dependents(&self, symbol_id) -> HashMap<RelationKind,Vec<Symbol>> | 逆依存 | O(deg) | O(deg) |
| get_impact_radius | fn get_impact_radius(&self, symbol_id, max_depth) -> Vec<SymbolId> | 影響半径（幅優先） | O(E) | O(V) |
| symbol_count | fn symbol_count(&self) -> usize | シンボル数 | O(1) | O(1) |
| get_symbols_by_file | fn get_symbols_by_file(&self, file_id) -> Vec<Symbol> | ファイル単位取得 | O(S_file) | O(S_file) |
| file_count | fn file_count(&self) -> u32 | ファイル数 | O(1) | O(1) |
| relationship_count | fn relationship_count(&self) -> usize | 関係数 | O(1) | O(1) |
| get_file_path | fn get_file_path(&self, file_id) -> Option<String> | ファイルパス | O(1) | O(1) |
| get_all_indexed_paths | fn get_all_indexed_paths(&self) -> Vec<PathBuf> | 全索引済みファイル | O(F) | O(F) |
| add_indexed_path | fn add_indexed_path(&mut self, dir_path) -> IndexResult<()> | インデックス対象ディレクトリの追跡 | O(P) | O(P) |
| get_indexed_paths | fn get_indexed_paths(&self) -> &HashSet<PathBuf> | 追跡対象取得 | O(1) | O(P) |
| sync_with_config | fn sync_with_config(&mut self, stored_paths, config_paths, progress) -> IndexResult<(usize,usize,usize,usize)> | 設定と同期（追加/削除） | O(F+P) | O(F) |
| build_symbol_cache | fn build_symbol_cache(&mut self) -> IndexResult<()> | 1万件規模の高速キャッシュ構築 | O(N) | O(N) |
| clear_symbol_cache | fn clear_symbol_cache(&mut self, delete_file) -> IndexResult<()> | キャッシュ解放/削除 | O(1) | O(1) |
| load_symbol_cache | fn load_symbol_cache(&mut self) -> IndexResult<()> | キャッシュMMAPロード | O(1) | O(1) |
| symbol_cache | fn symbol_cache(&self) -> Option<&ConcurrentSymbolCache> | キャッシュ参照 | O(1) | O(1) |
| find_symbol | fn find_symbol(&self, name) -> Option<SymbolId> | 名称検索（キャッシュ優先） | O(1)～O(logN) | O(1) |
| find_symbols_by_name | fn find_symbols_by_name(&self,name,language_filter) -> Vec<Symbol> | 名称検索（一覧） | O(logN + K) | O(K) |
| get_symbol | fn get_symbol(&self, id) -> Option<Symbol> | ID検索 | O(1) | O(1) |
| clear_tantivy_index | fn clear_tantivy_index(&mut self) -> IndexResult<()> | 全消去 | O(N+E) | O(1) |

記号: N=総シンボル数, E=総埋め込み数, F=総ファイル数, S=ファイル内シンボル数, R=ファイル内関係数, deg=シンボルの次数, K=ヒット件数, L=返却件数, P=追跡ディレクトリ数

以降では主要APIの詳細のみ抜粋する。

### API詳細: index_file

1) 目的と責務  
- 単一ファイルの読み込み→言語判定→パース→シンボル/関係抽出→Tantivy格納→未解決関係の蓄積→コミット→関係解決を一気通貫で行う。

2) アルゴリズム（高レベル）
- start_tantivy_batch → index_file_internal(path, force=false) → commit_tantivy_batch → resolve_cross_file_relationships

3) 引数

| 名前 | 型 | 意味 |
|---|---|---|
| path | impl AsRef<Path> | 対象ファイルパス |

4) 戻り値

| 型 | 意味 |
|---|---|
| IndexResult<IndexingResult> | 成功時: Indexed/Cached と FileId |

5) 使用例
```rust
let mut indexer = SimpleIndexer::new();
let result = indexer.index_file("src/lib.rs")?;
println!("indexed: {:?}", result);
```

6) エッジケース
- ファイル未存在/非UTF-8（read_file_with_hashはlossy変換）→ IndexError::FileRead
- 非対応拡張子 → IndexError::UnsupportedFileType
- 変更なし → Cachedを返す
- 既存ファイルの再索引時、過去シンボルの埋め込み削除＋即保存（セマンティック同期）

根拠: index_file/index_file_internal（行番号不明）

### API詳細: index_directory_with_options

1) 目的と責務  
- ディレクトリ配下のファイルをFileWalkerで列挙し、バッチコミットしながら順次索引。進捗バーとドライラン対応。

2) アルゴリズム
- walker.walk(dir) → max_filesで切詰 → 進捗準備 → start_tantivy_batch → 各ファイルでindex_file_internal（COMMIT_BATCH_SIZE=100毎にcommit/restart）→ 最終commit → resolve_cross_file_relationships → IndexStats集計

3) 引数

| 名前 | 型 | 意味 |
|---|---|---|
| dir | impl AsRef<Path> | ルートディレクトリ |
| progress | bool | 進捗表示有無 |
| dry_run | bool | 実行せず見積のみ |
| force | bool | 再索引強制 |
| max_files | Option<usize> | 上限件数 |

4) 戻り値: IndexResult<IndexStats>（総件数や失敗数）

5) 使用例
```rust
let stats = indexer.index_directory_with_options("src", true, false, false, Some(1000))?;
println!("files={}, symbols={}", stats.files_indexed, stats.symbols_found);
```

6) エッジケース
- dry_run=trueで実索引せず件数のみ
- 大量ファイル時に100件ごとコミットでメモリ抑制

根拠: index_directory_with_options（行番号不明）

### API詳細: resolve_cross_file_relationships（内部コア）

1) 目的と責務  
- 未解決関係（名称ベース）を言語ごとのResolutionScope＋importsで解決し、可視性/種別整合性を検査、Tantivyに保存（逆方向も自動）。CallsはMethodCall/メタデータを利用した高度解決。

2) アルゴリズム
- unresolvedを取り出し、file_id単位にグループ化 → behavior.build_resolution_context(_with_cache) → 各未解決関係について:
  - from_symbolはid既知なら直取得、未知なら名前＋file_idで絞込
  - Callsはresolve_method_call_enhancedで受け手/静的/行情報などをヒントに解決
  - 他はcontext.resolve_relationship
  - 未解決なら外部呼び出しマッピングの検討（externalはシンボル未作成でスキップ）
  - 種別整合性（is_compatible_relationship）と可視性（is_symbol_visible_from）確認
  - OKならstore_relationship（逆方向も自動生成）
- 進捗集計と最終commit

3) 引数/戻り値: なし / IndexResult<()>

4) 使用例: 内部から自動呼出。大量索引後に明示的に呼ぶことも可能。

5) エッジケース
- fromシンボル名がファイル内に存在しない
- Callsで外部import由来（context.is_external_import）の場合は解決しない
- 逆関係の自動生成（ImplementedBy/ExtendedBy/CalledBy/UsedBy/DefinedIn/ReferencedBy）

根拠: resolve_cross_file_relationships, resolve_method_call_enhanced, resolve_method_call, add_relationship_internal（行番号不明）

### API詳細: remove_file

- 目的: ファイルの全ドキュメント（シンボル/関係/インポート）とセマンティック埋め込みを削除
- ポイント: delete_imports_for_fileでインポートの整合性維持、remove_file_documentsで完全削除、必要なら埋め込みの削除、commit後にシンボルキャッシュ再構築
- 例外: ファイル未登録時はNo-Op

根拠: remove_file（行番号不明）

### API詳細: build_symbol_cache

- 目的: 現在の全シンボルから高速ハッシュキャッシュを再構築（Windowsのファイルロック対策込み）
- 手順: 既存キャッシュclear → get_all_symbols → SymbolHashCache::build_from_symbols → load_symbol_cache

根拠: build_symbol_cache, clear_symbol_cache, load_symbol_cache（行番号不明）

### API詳細: semantic_search_docs_with_language

- 目的: ドキュメントコメントの埋め込みに対し、言語フィルタで事前絞込し類似検索
- 例外: 未有効時はIndexError::General

根拠: semantic_search_docs_with_language（行番号不明）

### API詳細: get_symbol_context

- 目的: シンボルを中心に、必要な関係（呼出/呼出元/定義/実装）をまとめて返す
- 入力フラグ include に応じて部分取得

根拠: get_symbol_context（行番号不明）

## Walkthrough & Data Flow

- ファイル索引フロー（index_file → index_file_internal → reindex_file_content → commit → resolve）
  1. start_tantivy_batch
  2. read_file_with_hashで内容とハッシュ計算。既に同一ハッシュ存在ならCached返却
  3. 再索引時はremove_file_documentsし、セマンティック埋め込みをremove_embeddingsして即保存
  4. register_fileでFileId払い出し・メタ更新・ファイル情報保存
  5. detect_language → create_parser_with_behavior。behavior.register_fileでモジュールパス追跡
  6. extract_and_store_symbols
     - parser.parse → symbols。importsをfind_imports→behavior.add_import→Tantivyへstore_import（永続）
     - configure_symbolで言語ごとのモジュール/可視性補正
     - store_symbolでTantivy保存、doc_commentはセマンティック索引、ベクター対象はpending_embeddingsに積む
     - trait_symbols_by_fileを更新
  7. extract_and_store_relationships
     - MethodCall（receiver, static, range）でCalls候補保存＋メタデータ
     - find_calls, find_implementations, find_extends, find_inherent_methods, find_uses, find_defines
     - 変数型をvariable_typesに正規化格納、behavior.register_expression_types
     - 実際の関係は未解決としてunresolved_relationshipsへ蓄積（from_id既知なら持つ）
  8. update_symbol_counter（メタ＋pendingカウンタ更新）
  9. commit_tantivy_batch → process_pending_embeddings（埋め込み生成→VectorEngineに投入）→ build_symbol_cache
  10. resolve_cross_file_relationshipsで関係確定

- 関係解決（resolve_cross_file_relationships）  
  - file_id単位でResolutionScopeを生成（LanguageBehavior + キャッシュ利用可）
  - Callsはresolve_method_call_enhanced: MethodCall候補からreceiver/static/lineヒントで絞込み → receiver型推定（variable_types or context.resolve_expression_type）→ qualified名で解決 → 外部importなら解決しない
  - 他の関係はcontext.resolve_relationshipで名前解決
  - 種別整合性（is_compatible_relationship）と可視性（is_symbol_visible_from）を満たすものだけstore_relationship。逆方向も自動生成

### Mermaid（解決フロー・主要分岐）

```mermaid
flowchart TD
  A[Start resolve_cross_file_relationships] --> B[unresolvedをfile_idでグループ化]
  B --> C[各fileでResolutionScopeを構築<br/>(LanguageBehavior, cache可)]
  C --> D{関係.kind == Calls<br/>かつ from_symbols.len()==1?}
  D -- Yes --> E[resolve_method_call_enhanced<br/>MethodCall＋メタデータ活用]
  D -- No --> F[context.resolve_relationship]
  E --> G{解決できた?}
  F --> G
  G -- No --> H[外部呼び出しマッピング試行<br/>（externalならスキップ）]
  G -- Yes --> I[対象Symbol取得]
  I --> J{種別整合性OK?}
  J -- No --> X[skip: incompatible]
  J -- Yes --> K{可視性OK?（Defines除く）}
  K -- No --> Y[skip: visibility]
  K -- Yes --> L[store_relationship<br/>(逆方向も自動作成)]
  L --> M[次の関係]
  X --> M
  Y --> M
  H --> M
  M --> Z[Commit and End]
```

上記の図はresolve_cross_file_relationships関数（行番号不明）の主要分岐を示す。

### Mermaid（ディレクトリ索引フロー）

```mermaid
flowchart LR
  A[Walk files] --> B{dry_run?}
  B -- Yes --> C[件数表示のみ]
  B -- No --> D[start_tantivy_batch]
  D --> E[for file in files]
  E --> F[index_file_internal(file, force)]
  F --> G{files_in_batch >= 100?}
  G -- Yes --> H[commit_tantivy_batch; start_tantivy_batch; reset counter]
  G -- No --> E
  E --> I[最後に余りcommit]
  I --> J[resolve_cross_file_relationships]
  J --> K[統計更新→返却]
```

上記の図はindex_directory_with_options関数（行番号不明）の制御フローを示す。

## Complexity & Performance

- 単一ファイル索引: O(S + R)（抽出・保存）, 保存はTantivyへの文書投入（インデックス構築）が支配。Imports/シンボル/関係の個数に線形。
- ディレクトリ索引: O(F·(S+R))。100件ごとのコミットでメモリ増加を抑制（I/Oバーストを分散）。
- 関係解決: 未解決件数Uに対しO(U·logN)程度（キャッシュ/インデックス照会）。CallsはMethodCallマッチング＋型解決が追加でO(候補数)。
- シンボルキャッシュ構築: O(N)。Windowsロック対策で一旦drop→再構築。
- セマンティック検索: 事前埋め込み（E個）。検索はフィルタ→類似度計算（実装依存）でO(logE + L)想定。

ボトルネック/スケール限界
- 大規模プロジェクトではresolve_cross_file_relationshipsが重い。symbol_lookup_cacheで軽減しているが、さらなる近接優先（module_proximity）や前処理の改善が有効。
- ベクター埋め込み生成はモデル依存で高コスト。pending_embeddingsをバッチ処理するのは妥当。
- get_all_symbolsでのN=1万上限はキャッシュ構築用途として妥当だが、より大規模な場合は分割/ストリーミングが必要。

I/O/ネットワーク/DB
- Tantivyへの書込み/コミット、シンボルキャッシュのファイルI/O、セマンティック保存はストレージI/O負荷となる。
- EmbeddingGeneratorが外部推論の場合はネットワーク/CPU負荷に注意。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:
- メモリ安全性: unsafe未使用。Arc/Mutex利用で共有状態を保護。lock().unwrap()によりパニックの可能性（Mutex毒化時）。Buffer overflow/Use-after-free/Integer overflowの懸念はコード上なし。
- インジェクション: SQL/Command/Path traversalなし（Tantivy APIのみ、ファイル読みはfs::read+canonicalize）。Pathはユーザ入力だが、remove_file等で該当パスのドキュメントを削除するのみ。
- 認証・認可: 本コンポーネントはローカルインデックスで認可概念なし。
- 秘密情報: Hard-coded secretsなし。ログにdoc_commentを一部出すデバッグ（debug_print）あり。デバッグモードでのeprintln含む。個人情報がdoc_commentに含まれる場合の取扱に注意。
- 並行性: Arc<Mutex>でVectorEngine/SemanticSearchを保護。DocumentIndexは内部でバッチを管理。明示的なデッドロックは見られないが、複数ロックを同時に保持する複合処理は避けている。lock().unwrap()によりエラー→パニック（改善余地）。

エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空/非UTF-8パス | "a\0.rs" | FileReadエラー | read_file_with_hashでエラー生成 | OK |
| 非対応拡張子 | "foo.unknown" | UnsupportedFileType | detect_language → registry未登録 | OK |
| 未変更ファイル | ハッシュ一致 | Cachedを返す | get_file_info→content_hash比較 | OK |
| 再索引時セマンティック同期 | 既存シンボルのdoc埋め込み | 埋め込み削除→即保存 | remove_embeddings→save | OK |
| SymbolID重複 | バッチ中連番衝突 | ペンディングカウンタ更新で回避 | update_pending_symbol_counter | OK |
| 外部呼び出し誤解決 | indicatif::ProgressBar::new | 外部と判別し解決スキップ | context.is_external_importでチェック | OK |
| 可視性 | 異モジュールprivate | 解決しても保存しない | is_symbol_visible_from | OK |
| Windowsロック | symbol_cache.bin使用中 | 詳細メッセージで案内 | build_symbol_cacheのエラー整形 | OK |

既知/修正済のバグ（抜粋）
- Importsの永続化不足で外部記号がローカルに誤解決 → store_importで永続化（extract_and_store_symbols内）
- バッチ時のSymbolID再利用 → update_pending_symbol_counterを追加
- 埋め込み削除後のキャッシュ不整合 → remove_embeddings後に即save
- Windowsのmmapロックによりキャッシュ再生成失敗 → 先にdropしてから再作成、ユーザ向け案内

## Design & Architecture Suggestions

- エラー処理の一貫化
  - lock().unwrap()を可能な限りmap_errに置換しIndexErrorへ集約。ユーザに安定したエラー表示を行う。
  - thiserrorでIndexErrorの整形を改善。
- 解決の最適化
  - module_proximity（スコアリング）を実装し、候補が複数ある場合の優先順位を安定させる。
  - resolve_cross_file_relationshipsでBatchごとにcontextをキャッシュ（既にfile単位で構築済だが、インナーループでの不要なTantivyクエリ削減余地あり）。
- API分離
  - セマンティック/ベクター統合をStrategyとして分離（SimpleIndexerはフックを呼ぶだけ）。
  - Progress表示を別コンポーネントへ抽出（テスタビリティとSRP向上）。
- 観測性
  - debug_printからtracingへ段階移行。span/イベントを付与し、性能可視化（解決成功/失敗/skip理由）メトリクスを出す。
- キャッシュ/構築
  - build_symbol_cacheのN=10,000固定を設定値化、巨大プロジェクトで段階構築に対応。
- 型安全性
  - 正規化キー（normalize_expression_key）やモジュールパスをNewTypeで表現しミスの混入を抑止。

## Testing Strategy (Unit/Integration) with Examples

本ファイルには豊富なテストが含まれる（testsモジュール）。カテゴリ別例:

- ディレクトリトラッキング
  - test_indexer_skips_child_path_when_parent_tracked, test_indexer_replaces_children_when_parent_added
- Rustの関係解決/モジュールパス
  - test_trait_implementations_resolution（既知バグ表面化用）
  - test_symbol_module_paths
- 可視性/同一モジュール判定
  - test_symbols_in_same_module, test_is_symbol_visible_from
- Importsに基づく解決（修正確認）
  - test_import_based_relationship_resolution
- 互換性関数の整合（configure_symbol）
  - test_configure_symbol_baseline_* / test_configure_symbol_different_languages
- 言語ID付与とフィルタリング
  - test_symbols_get_language_id_during_indexing
  - test_find_symbols_with_language_filter, test_search_with_language_filter
- 実コードを用いた統合試験（REAL TDD）
  - test_real_relationship_resolution_integration（Rust）
  - test_real_python_resolution_integration（Python）
  - test_real_typescript_resolution_integration（TS）
  - test_real_php_resolution_integration（PHP）
  - test_real_rust_resolution_integration（Rust大型）

追加で推奨するテスト:
- ベクター埋め込みの件数/ID整合（commit_tantivy_batch後にpendingが消えること）
- Windowsでのキャッシュロック再現（E2E）
- Calls重複除去の健全性（メソッド呼びと関数呼びの重複シナリオ）
- 外部importの匿名再エクスポート（別名alias/glob）解決
- 例外/パニックテスト：Mutex毒化/lock失敗のハンドリング

使用例（最小統合）
```rust
let mut indexer = SimpleIndexer::new();
indexer.enable_semantic_search()?; // 任意
let stats = indexer.index_directory("src", true, false)?;
println!("files={}, symbols={}", stats.files_indexed, stats.symbols_found);

let results = indexer.search("parse_json", 10, None, None, Some("rust"))?;
for r in results {
    println!("{} in {}", r.name, r.file_path);
}
```

## Refactoring Plan & Best Practices

- panic削減
  - .lock().unwrap() → .lock().map_err(|_| IndexError::General("..."))?
  - tests以外のunwrap/expect排除
- エラーメッセージの整備
  - IndexErrorにoperation/パラメータ/ヒントを含める
- 可視性/互換性
  - is_compatible_relationshipを言語拡張可能に（LanguageBehaviorへ委譲オプション）
  - module_proximity導入
- データフロー簡素化
  - 未解決関係の構造体にcaller_id/line/column/receiver_norm等を標準フィールドで保持
- 負荷削減
  - resolve_cross_file_relationshipsでsymbol_lookup_cacheのスコープ/キー戦略強化（file_id＋name）
  - COMMIT_BATCH_SIZEを設定化
- 観測性/メトリクス
  - 関係解決のresolved_count/skipped_count/理由内訳（種別不一致/可視性/未発見/外部）をメトリクスとして記録

## Observability (Logging, Metrics, Tracing)

- 現状: debug_print!とeprintln!で出力。ユーザ設定でオン。
- 推奨:
  - tracingでspan（index_file, resolve_cross_file_relationships, process_pending_embeddings等）を付与
  - メトリクス
    - relationship_resolution_total{result=resolved/skip_reason=...}
    - pending_embeddings_gauge
    - symbol_cache_build_seconds
    - tantivy_commit_seconds
  - ログのサンプリング/冪等ID（file_id/rel_id）で相関可能に

## Risks & Unknowns

- LanguageBehaviorの正確性依存（find_defines/resolve_external_call_target等の言語別実装が未成熟な場合、関係解決に漏れ）
- 大量未解決関係の処理時間増加（巨大プロジェクト）。将来的に分割/並列化を検討（ただしTantivyの一貫性維持に注意）。
- セマンティック/ベクターの保存が失敗した場合の部分不整合（警告ログのみに留まる箇所あり）
- Windowsロックなど環境依存I/O課題
- 行番号/位置情報の利用が限定的（より精緻な解決精度のために位置ベースのコンテキストを拡充可能）

---

【Rust特有の観点】

- 所有権/移動
  - store_symbolでSymbolを所有権移動してTantivyに格納（store_symbol: 行番号不明）
  - process_pending_embeddingsでpending_embeddingsをクリア（所有権消費）
- 借用/ライフタイム
  - parser: &mut Box<dyn LanguageParser>で逐次パース（extract_and_store_*内）
  - behavior: &dyn LanguageBehavior参照で設定・解決を委譲（configure_symbol 等）
  - 明示的ライフタイムは不要。Arcで共有寿命を管理。
- unsafe境界
  - unsafe未使用
- 並行性
  - Arc<Mutex<VectorSearchEngine>>/Arc<Mutex<SimpleSemanticSearch>>で同期
  - lock().unwrap()によるパニック余地あり。poisoned時の復旧戦略は未実装。
  - awaitなし（同期I/O）。将来的な非同期化はAPI変更を伴う。
- エラー設計
  - Result中心（IndexResult）。TantivyエラーはIndexError::TantivyErrorに変換
  - 一部でGeneral文字列メッセージ。エラー型の精緻化余地
  - unwrap/expectは主にテスト・ロギングで残存。本番パスはmap_errで概ね保護

【API使用時の注意】
- バッチ（start/commit）はindex_file/index_directory内で管理されるため、外部で乱用しない
- セマンティック/ベクター機能は明示的にenable/with_vector_searchが必要
- resolve_cross_file_relationshipsはindex_directory内で最後に実行されるが、単一ファイル索引後に他ファイルを利用した解決を期待する場合は明示呼出が必要な場面がある

【コード抜粋（短関数例）】
```rust
impl TantivyTransaction {
    pub fn new() -> Self { Self }
    pub fn complete(&mut self) { /* no-op */ }
}
```

【使用例（Calls解決の取り出し）】
```rust
let syms = indexer.find_symbols_by_name("main", Some("rust"));
if let Some(main) = syms.get(0) {
    for (callee, meta) in indexer.get_called_functions_with_metadata(main.id) {
        println!("{} calls {} at {:?}", main.name, callee.name, meta);
    }
}
```

以上。