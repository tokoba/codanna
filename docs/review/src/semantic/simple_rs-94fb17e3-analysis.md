# semantic\simple.rs Review

## TL;DR

- 目的: ドキュメントコメントを対象に、FastEmbedの埋め込みを用いて自然言語でコード探索を可能にするシンプルなセマンティック検索エンジン。
- 主な公開API: new/from_model_name/with_model、index_doc_comment、search/search_with_language/search_with_threshold、save/load、remove_embeddings/clear、metadata、embedding_count。
- 複雑箇所: loadでのメタデータとベクタストレージの整合性確認、モデル初期化/ダウンロードのI/O、searchでの類似度計算とソート。
- 重大リスク: Mutexのlock/unwrapによるパニック、f32のpartial_cmp.unwrapによるNaN時パニック、モデル初期化/埋め込み時のエラー取り扱い、ログ出力の標準出力/エラーへの直接出力。
- 並行性: TextEmbeddingをMutexで保護（内的可変性）。複数スレッドからの検索は可能性ありだが、lock失敗時のPoisonが未処理。
- セキュリティ/I/O: 保存/読込時の権限・ディスク容量・破損ファイルへの耐性あり（StorageError）だが、より詳細な検証/再試行は未実装。
- パフォーマンス: 類似度計算はO(N·D)、ソートはO(N log N)。大規模コードベースではANNやTop-K選抜が必要。

## Overview & Purpose

このモジュールは、コードベースのドキュメントコメントを対象に、自然言語クエリで関連シンボルを検索するためのシンプルなセマンティック検索を提供します。FastEmbedのテキスト埋め込みモデルを利用し、各シンボルのドキュメントからベクトル埋め込みを生成・保持し、クエリベクトルとのコサイン類似度でランキングします。保存/読込に対応し、インデックス再利用を可能にします。言語情報に基づくフィルタリング検索も提供します。

主な利用シーン:
- 「JSONをパースする関数」など、自然言語でドキュメントに近い記述から該当シンボルを見つける。
- 言語別検索（例: Rustドキュメントのみ対象）。
- インデックスの永続化と再ロード。

根拠:
- 構造体/メソッド定義（SimpleSemanticSearch:行番号:不明）
- search/search_with_language/search_with_threshold実装（行番号:不明）
- save/load実装（行番号:不明）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | SemanticSearchError | pub | モデル初期化/埋め込み/ストレージ/次元不一致/IDエラーの分類 | Low |
| Struct | SimpleSemanticSearch | pub | 埋め込み管理、検索、保存/読込、言語マッピング、メタデータ保持 | Med |
| Field | embeddings: HashMap<SymbolId, Vec<f32>> | private | シンボルIDに紐づく埋め込みベクトル | Low |
| Field | symbol_languages: HashMap<SymbolId, String> | private | 言語情報のマッピング | Low |
| Field | model: Mutex<TextEmbedding> | private | FastEmbedモデルの内的可変性保護 | Med |
| Field | dimensions: usize | private | 埋め込み次元の検証 | Low |
| Field | metadata: Option<crate::semantic::SemanticMetadata> | private | モデル名・次元・件数のメタ | Low |
| Method | new() -> Result<Self, SemanticSearchError> | pub | デフォルトモデル(AllMiniLML6V2)で初期化 | Med |
| Method | from_model_name(&str) -> Result<Self, SemanticSearchError> | pub | 文字列モデル名から初期化 | Med |
| Method | with_model(EmbeddingModel) -> Result<Self, SemanticSearchError> | pub | 明示モデル指定で初期化（ダウンロード/キャッシュ使用） | Med |
| Method | index_doc_comment(SymbolId, &str) -> Result<(), SemanticSearchError> | pub | ドキュメントから埋め込み生成・登録 | Med |
| Method | index_doc_comment_with_language(SymbolId, &str, &str) -> Result<(), SemanticSearchError> | pub | 言語付インデックス | Low |
| Method | search(&str, usize) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> | pub | 類似度計算しランキング | Med |
| Method | search_with_language(&str, usize, Option<&str>) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> | pub | 事前言語フィルタ後に検索 | Med |
| Method | search_with_threshold(&str, usize, f32) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> | pub | スコアしきい値で絞り込み | Low |
| Method | embedding_count(&self) -> usize | pub | 登録数取得 | Low |
| Method | clear(&mut self) | pub | 全削除 | Low |
| Method | remove_embeddings(&mut self, &[SymbolId]) | pub | 指定ID削除 | Low |
| Method | metadata(&self) -> Option<&SemanticMetadata> | pub | メタ参照 | Low |
| Method | save(&self, &Path) -> Result<(), SemanticSearchError> | pub | ベクタストレージ＋言語JSON保存 | Med |
| Method | load(&Path) -> Result<Self, SemanticSearchError> | pub | メタ/ストレージ/モデルの復元＋言語読込 | High |
| Fn | cosine_similarity(&[f32], &[f32]) -> f32 | private | コサイン類似度算出 | Low |

### Dependencies & Interactions

- 内部依存
  - search/search_with_language → cosine_similarity（行番号:不明）
  - index_doc_comment/index_doc_comment_with_language → model.lock().embed（行番号:不明）
  - save → crate::semantic::SemanticVectorStorage / SemanticMetadata, serde_json（行番号:不明）
  - load → SemanticMetadata::load、SemanticVectorStorage::open/load_all、TextEmbedding::try_new、serde_json（行番号:不明）
  - from_model_name/with_model/new → crate::vector::parse_embedding_model/model_to_string、InitOptions/TextEmbedding（行番号:不明）

- 外部依存（本ファイルに現れるもの）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | fastembed::{EmbeddingModel, InitOptions, TextEmbedding} | 埋め込みモデル初期化/ベクトル生成 | モデルダウンロード/キャッシュ利用あり |
  | serde_json | 言語マッピングのJSONシリアライズ/デシリアライズ | 保存/読込 |
  | std::sync::Mutex | TextEmbeddingの内的可変性保護 | lock().unwrapでPoison未処理 |
  | std::collections::HashMap | 埋め込み/言語マッピング | メモリ使用量はN·D |
  | std::fs / std::path::Path | ファイルシステム操作（保存/読込） | 権限/容量/破損対応 |
  | crate::semantic::{SemanticMetadata, SemanticVectorStorage} | メタ/ベクタストレージ | 次元整合性検証あり |
  | crate::vector::{VectorDimension, parse_embedding_model, model_to_string} | 次元型/モデル名パース | モデル検証 |
  | crate::init::models_dir() | モデルキャッシュディレクトリ | 進捗表示メッセージ |

- 被依存推定
  - インデクサ（ドキュメント抽出周辺）
  - CLI/サービス層（semantic indexの保存/読込、検索API提供）
  - 言語別検索UI
  - 具体的な利用箇所はこのチャンクには現れない（不明）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | fn new() -> Result<Self, SemanticSearchError> | デフォルトモデルで初期化 | O(D) + モデル初期化 | O(1) |
| from_model_name | fn from_model_name(&str) -> Result<Self, SemanticSearchError> | 文字列モデル名から初期化 | O(D) + モデル初期化 | O(1) |
| with_model | fn with_model(EmbeddingModel) -> Result<Self, SemanticSearchError> | 明示モデル指定で初期化 | O(D) + モデル初期化 | O(1) |
| index_doc_comment | fn index_doc_comment(&mut self, SymbolId, &str) -> Result<(), SemanticSearchError> | ドキュメントの埋め込み生成/登録 | O(D) | O(D) 追加 |
| index_doc_comment_with_language | fn index_doc_comment_with_language(&mut self, SymbolId, &str, &str) -> Result<(), SemanticSearchError> | 言語情報付き登録 | O(D) | O(D) + O(1) |
| search | fn search(&self, &str, usize) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> | 類似度計算・ランキング | O(N·D + N log N) | O(N) |
| search_with_language | fn search_with_language(&self, &str, usize, Option<&str>) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> | 言語フィルタ後に検索 | O(F·D + F log F) | O(F) |
| search_with_threshold | fn search_with_threshold(&self, &str, usize, f32) -> Result<Vec<(SymbolId, f32)>, SemanticSearchError> | しきい値で絞り込み | O(N·D + N log N) | O(N) |
| embedding_count | fn embedding_count(&self) -> usize | 件数取得 | O(1) | O(1) |
| clear | fn clear(&mut self) | 全削除 | O(N) | O(1) |
| remove_embeddings | fn remove_embeddings(&mut self, &[SymbolId]) | 指定削除 | O(K) | O(1) |
| metadata | fn metadata(&self) -> Option<&SemanticMetadata> | メタ参照 | O(1) | O(1) |
| save | fn save(&self, &Path) -> Result<(), SemanticSearchError> | ベクタ＋言語保存 | O(N·D + N) + I/O | O(N·D) 一時 |
| load | fn load(&Path) -> Result<Self, SemanticSearchError> | メタ/ベクタ/言語読込・モデル起動 | O(N·D) + I/O | O(N·D) |

以下、主なAPIの詳細。

### SimpleSemanticSearch::new

1) 目的と責務
- デフォルトモデル AllMiniLML6V2 で検索インスタンスを初期化。

2) アルゴリズム（ステップ）
- models_dir()を取得し、キャッシュ有無をチェックしてメッセージ出力。
- TextEmbedding::try_new(InitOptions)でモデル初期化。
- "test"で埋め込み生成し次元を取得。
- SemanticMetadataを作成。
- 構造体を返す。
- 根拠: new/with_model（行番号:不明）

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| なし | - | なし |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<Self, SemanticSearchError> | 初期化成功か失敗 |

5) 使用例
```rust
let search = SimpleSemanticSearch::new()?;
assert_eq!(search.embedding_count(), 0);
```

6) エッジケース
- モデル初期化失敗 → ModelInitError
- test埋め込み失敗 → EmbeddingError
- キャッシュ/ダウンロードの進捗出力はstderr/stdoutに出る（静的構成不可）

### SimpleSemanticSearch::from_model_name

1) 目的と責務
- 文字列のモデル名をパースし、当該モデルで初期化。

2) アルゴリズム
- parse_embedding_model(model_name)でEmbeddingModelへ変換。
- with_model(model)を呼ぶ。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| model_name | &str | モデル名文字列 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<Self, SemanticSearchError> | 初期化結果 |

5) 使用例
```rust
let search = SimpleSemanticSearch::from_model_name("MultilingualE5Small")?;
```

6) エッジケース
- 無効なモデル名 → ModelInitError("Invalid model name: ...")

### SimpleSemanticSearch::with_model

1) 目的と責務
- EmbeddingModel指定で初期化。キャッシュ/ダウンロードを制御。

2) アルゴリズム
- models_dir()の内容を検査、メッセージ出力。
- TextEmbedding::try_new(InitOptions)でモデル起動。
- "test"埋め込みで次元取得。
- SemanticMetadataを作成して返却。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| model | EmbeddingModel | FastEmbedのモデル列挙 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<Self, SemanticSearchError> | 初期化結果 |

5) 使用例
```rust
use fastembed::EmbeddingModel;
let search = SimpleSemanticSearch::with_model(EmbeddingModel::AllMiniLML6V2)?;
```

6) エッジケース
- モデル初期化失敗、埋め込み生成失敗
- ダウンロード時のネットワーク障害 → ModelInitError/EmbeddingError

### SimpleSemanticSearch::index_doc_comment

1) 目的と責務
- ドキュメント文字列から埋め込みを生成し、指定SymbolIdに関連付けて登録。

2) アルゴリズム
- docが空白のみならスキップ。
- model.lock().unwrap().embed(vec![doc], None)で生成。
- 次元検証（self.dimensionsと一致か確認）。
- embeddings.insert(symbol_id, embedding)で登録。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| symbol_id | SymbolId | シンボル識別子 |
| doc | &str | ドキュメント文字列 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<(), SemanticSearchError> | 登録結果 |

5) 使用例
```rust
let mut search = SimpleSemanticSearch::new()?;
search.index_doc_comment(SymbolId::new(1).unwrap(), "Parse JSON data")?;
assert_eq!(search.embedding_count(), 1);
```

6) エッジケース
- 空文字/空白 → 何もしないでOk
- 埋め込み生成失敗 → EmbeddingError
- 次元不一致 → EmbeddingError（専用のDimensionMismatchではない）

### SimpleSemanticSearch::index_doc_comment_with_language

1) 目的と責務
- 追加で言語コード（例: "en", "ja"）を記録。

2) アルゴリズム
- index_doc_commentを呼ぶ。
- embeddingsに存在するならsymbol_languagesへ格納。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| symbol_id | SymbolId | シンボルID |
| doc | &str | ドキュメント |
| language | &str | 言語コード |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<(), SemanticSearchError> | 登録結果 |

5) 使用例
```rust
let mut search = SimpleSemanticSearch::new()?;
search.index_doc_comment_with_language(SymbolId::new(2).unwrap(), "データベース接続", "ja")?;
```

6) エッジケース
- indexが失敗した場合 → そのままエラー伝播
- embeddingsに無い場合は言語登録しない

### SimpleSemanticSearch::search

1) 目的と責務
- クエリ文字列の埋め込みを生成し、全埋め込みについてコサイン類似度を計算して上位limit件を返す。

2) アルゴリズム
- embeddingsが空ならNoEmbeddings。
- queryの埋め込み生成。
- 全エンベディングと類似度計算（cosine_similarity）。
- 類似度降順にソート（partial_cmp.unwrap）。
- limitでtruncateして返す。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| query | &str | 検索クエリ |
| limit | usize | 最大返却件数 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<Vec<(SymbolId, f32)>, SemanticSearchError> | (ID, 類似度)のリスト |

5) 使用例
```rust
let results = search.search("parse JSON", 10)?;
for (id, score) in results {
    println!("id={:?} score={}", id, score);
}
```

6) エッジケース
- embeddingsが空 → NoEmbeddings
- f32にNaNが混入 → partial_cmpがNoneになりunwrapでパニックの可能性
- limit > 件数 → truncateは安全に動作（そのまま全件）

### SimpleSemanticSearch::search_with_language

1) 目的と責務
- languageで事前フィルタしてから類似度計算。対象件数を減らすことで速度と精度を両立。

2) アルゴリズム
- embeddings空チェック。
- query埋め込み生成。
- languageがSomeならsymbol_languagesで一致するIDに限定。
- 類似度計算→降順ソート→truncate。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| query | &str | 検索クエリ |
| limit | usize | 最大返却件数 |
| language | Option<&str> | 言語フィルタ（Noneなら全件） |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<Vec<(SymbolId, f32)>, SemanticSearchError> | フィルタ後の結果 |

5) 使用例
```rust
let results = search.search_with_language("user authentication", 5, Some("en"))?;
```

6) エッジケース
- 該当言語ゼロ → 空の結果（正常）
- NaNによるソートパニックの可能性はsearchと同様

### SimpleSemanticSearch::search_with_threshold

1) 目的と責務
- search結果からしきい値以上を返す。

2) アルゴリズム
- search(query, limit)を呼び出し、スコアでフィルタ。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| query | &str | 検索クエリ |
| limit | usize | 上位候補数 |
| threshold | f32 | 最小スコア |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<Vec<(SymbolId, f32)>, SemanticSearchError> | しきい値以上の結果 |

5) 使用例
```rust
let results = search.search_with_threshold("calculate hash", 10, 0.6)?;
```

6) エッジケース
- 内部searchがNoEmbeddings等を返す場合はそのまま伝播
- thresholdが極端（>1 or < -1）でもフィルタとして動く

### SimpleSemanticSearch::embedding_count / clear / remove_embeddings / metadata

1) 目的と責務
- embedding_count: 件数取得
- clear: 全埋め込み/言語クリア
- remove_embeddings: 指定IDを削除
- metadata: メタデータ参照

2) アルゴリズム
- HashMap操作（len/clear/remove/Option参照）

3) 引数と戻り値
- embedding_count: 引数なし / usize
- clear: 引数なし / 戻り値なし
- remove_embeddings: &[SymbolId] / 戻り値なし
- metadata: なし / Option<&SemanticMetadata>

5) 使用例
```rust
search.clear();
search.remove_embeddings(&[SymbolId::new(1).unwrap()]);
if let Some(meta) = search.metadata() {
    println!("model: {}", meta.model_name);
}
```

6) エッジケース
- remove対象が存在しない → 何も起きない（正常）

### SimpleSemanticSearch::save

1) 目的と責務
- ベクタストレージに埋め込みを保存し、言語マッピングをJSONで保存する。

2) アルゴリズム
- ディレクトリ作成（create_dir_all）。
- メタデータ保存（モデル名、次元、件数）。
- VectorDimensionを作成しSemanticVectorStorage::new。
- HashMap→Vecに変換しsave_batch。
- 言語マッピングをHashMap<u32, String>に変換しJSONファイルに書き出し。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| path | &Path | 保存先ディレクトリ |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<(), SemanticSearchError> | 保存結果 |

5) 使用例
```rust
use std::path::Path;
search.save(Path::new("./semantic"))?;
```

6) エッジケース
- ディレクトリ作成失敗 → StorageError
- VectorDimension不正 → StorageError（1〜4096が必要）
- save_batch失敗 → StorageError（SemanticVectorStorageのエラー伝播）
- 言語JSONのシリアライズ/書き込み失敗 → StorageError

### SimpleSemanticSearch::load

1) 目的と責務
- 既存のメタ・ストレージ・言語JSONを読み取り、モデルを起動して検索インスタンスを復元。

2) アルゴリズム
- SemanticMetadata::load。
- metadata.model_nameをparse_embedding_modelしモデル取得（無効ならStorageError）。
- SemanticVectorStorage::open。
- ストレージ次元とmetadata.dimensionが一致か検査（不一致ならDimensionMismatch）。
- storage.load_allで全埋め込み取得しHashMapに変換。
- TextEmbedding::try_newでモデル起動（ダウンロードなし、進捗非表示）。
- languages.jsonが存在すれば読み取り→serde_json::from_str→u32からSymbolIdへ変換（SymbolId::new(id)がSomeのもののみfilter_mapで取り込む）。
- 構造体を返す。

3) 引数
| 引数 | 型 | 説明 |
|------|----|------|
| path | &Path | 読込元ディレクトリ |

4) 戻り値
| 型 | 説明 |
|----|------|
| Result<Self, SemanticSearchError> | 復元結果 |

5) 使用例
```rust
use std::path::Path;
let loaded = SimpleSemanticSearch::load(Path::new("./semantic"))?;
let results = loaded.search("parse JSON", 10)?;
```

6) エッジケース
- メタファイル欠如/破損 → StorageError
- モデル名無効 → StorageError（サジェストあり）
- 次元不一致 → DimensionMismatch（サジェストあり）
- 言語JSON欠如 → 空マップ（正常）
- 言語JSON破損 → StorageError

## Walkthrough & Data Flow

- インデックスフロー
  1) index_doc_commentでdoc→embed生成、次元検証、embeddingsへ登録。
  2) index_doc_comment_with_languageならsymbol_languagesへ言語登録。

- 検索フロー
  1) searchでembeddings空チェック。
  2) query→embed生成。
  3) embeddings各要素にcosine_similarity。
  4) 類似度降順ソート→上位limit返却。
  5) search_with_languageは事前フィルタ後に3〜4を実行。

- 保存/読込フロー
  1) saveでメタ保存→ベクタ保存→言語JSON保存。
  2) loadでメタ読込→モデル名検証→ストレージオープン→次元検証→埋め込み読込→モデル起動→言語JSON読込→復元。

### Mermaid Flowchart: loadの主要分岐

```mermaid
flowchart TD
    A[Start: load(&Path)] --> B[SemanticMetadata::load(path)]
    B -->|Err| E1[StorageError: メタ読込失敗] --> Z[Return Err]
    B --> C[parse_embedding_model(metadata.model_name)]
    C -->|Err| E2[StorageError: モデル無効] --> Z
    C --> D[SemanticVectorStorage::open(path)]
    D -->|Err| E3[StorageError: ストレージオープン失敗] --> Z
    D --> E{storage.dimension == metadata.dimension?}
    E -->|No| E4[DimensionMismatch] --> Z
    E -->|Yes| F[storage.load_all()]
    F -->|Err| E5[StorageError: 埋め込み読込失敗] --> Z
    F --> G[HashMapに詰替え]
    G --> H[TextEmbedding::try_new(InitOptions...)]
    H -->|Err| E6[ModelInitError: モデル起動失敗] --> Z
    H --> I{languages.json exists?}
    I -->|Yes| J[read_to_string + serde_json::from_str]
    J -->|Err| E7[StorageError: 言語JSON破損] --> Z
    J --> K[u32→SymbolId::new→filter_map]
    I -->|No| L[空マップ]
    K --> M[SimpleSemanticSearchを構築]
    L --> M
    M --> Z2[Return Ok(Self)]
```

上記の図は`load`関数（行番号:不明）の主要分岐を示す。

## Complexity & Performance

- インデックス（index_doc_comment）
  - 時間: O(D)（1件のdocから1回の埋め込み生成）
  - 空間: O(D)（1件分のベクトルを保持）
- 検索（search）
  - 時間: O(N·D + N log N)（N件の類似度計算＋ソート）
  - 空間: O(N)（スコアリスト）
- 言語フィルタ付き検索（search_with_language）
  - 時間: O(F·D + F log F)（Fはフィルタ後件数）
  - 空間: O(F)
- 保存（save）
  - 時間: O(N·D + N) + I/O（バッチ保存＋JSON書込）
  - 空間: O(N·D) 一時（Vecへ詰め替え）
- 読込（load）
  - 時間: O(N·D) + I/O（バッチ読込）
  - 空間: O(N·D)（HashMapへの展開）

ボトルネック:
- 大規模Nでの検索はソートが重い。Top-K選抜（選択アルゴリズム/ヒープ）によりO(N·D + N log K)化が有効。
- ANN（近似最近傍、HNSW/FAISS相当）の導入でスケール改善。
- save時の埋め込みコピーによるメモリピーク。

実運用負荷:
- モデル初回ダウンロード（~86MBとテストに記載）→ネットワーク負荷。
- ディスクI/O（ベクタ保存/読込、JSONファイル）。
- CPU負荷（高次元Dでの大量コサイン類似度計算）。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空ドキュメントのインデックス | doc="   " | 何もしないでOk | index_doc_comment内でtrimチェック | ✅ 実装済 |
| 埋め込み生成失敗 | モデル不具合 | EmbeddingErrorを返す | embedのErrをmap_err | ✅ 実装済 |
| 次元不一致（インデックス） | 生成ベクトルlen≠self.dimensions | エラーとして扱う | EmbeddingErrorとして返却 | ⚠️ DimensionMismatchではない |
| 検索時に埋め込みゼロ | embeddings.len()==0 | NoEmbeddingsを返す | 明示チェック | ✅ 実装済 |
| f32 NaN含みソート | 類似度にNaNが含まれる | パニックせず扱う | partial_cmp.unwrap | ❌ パニック可能（要修正） |
| Mutex Poison | model.lock()失敗 | エラーとして返す | unwrap | ❌ パニック可能（要修正） |
| 言語フィルタゼロ件 | language="ja"で一致なし | 空結果 | フィルタ後空Vec | ✅ 実装済 |
| 保存先ディレクトリ権限なし | path=/root/... | StorageError | create_dir_allのErrをStorageError | ✅ 実装済 |
| VectorDimension外 | dimensions>4096 | StorageError | VectorDimension::new Err処理 | ✅ 実装済 |
| 読込時メタファイル欠如/破損 | path空 | StorageError | Metadata::loadの? | ✅ 実装済 |
| 次元不一致（読込） | metadata vs storage | DimensionMismatchを返す | 明示チェック | ✅ 実装済 |
| 言語JSON破損 | languages.json壊れ | StorageError | serde_json::from_str Err処理 | ✅ 実装済 |
| 無効なSymbolId（言語JSON） | u32→SymbolId::newがNone | 該当IDを捨てる | filter_map | ✅ 実装済 |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rustの型安全により該当なし。このチャンクにunsafeは現れない（不明/未使用）。
  - 所有権/借用: &mut selfで更新操作の排他を保証、modelはMutexで内的可変性を保護。
- インジェクション
  - SQL/Command/Path traversal: 外部コマンド/SQLなし。Pathは呼び出し側提供であり、保存/読込は指定されたPathをそのまま利用（検証は呼び出し側に委譲）。悪意あるPathに対するチェックは未実装。
- 認証・認可
  - 本機能はローカルインデックスに対する操作で認可は未考慮（非該当）。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: eprintln/printlnでクエリ/モデル名などを標準出力/エラーへ出すため、機密クエリがログに残る可能性あり（SEARCH_DEBUG）。運用時はログレベル制御が望ましい。
- 並行性
  - Race condition: embeddings操作は&mut selfで保護、modelはMutexで保護。searchは&selfだがmodelへのembedでMutex取得あり。
  - Deadlock: 単一Mutexのみ、再入なし。デッドロックは低リスク。
  - Poison: lock().unwrapでPoison未処理、パニックの恐れ。

Rust特有の観点:
- 所有権: HashMapへembedding Vec<f32>をmove（index_doc_comment:行番号:不明）。saveではcloneしてVec化。
- 借用/ライフタイム: 明示的ライフタイム不要。Mutexのスコープ内でembed呼び出し、参照はスコープ終了で解放。
- unsafe境界: なし（このチャンクには現れない）。
- Send/Sync: TextEmbeddingのSend/Syncは不明だが、Mutexでアクセス制御。多スレッドでsearchを呼ぶ際、lockで同期される。
- 非同期/await: 非同期は使用なし。
- エラー設計: thiserrorで分類。unwrap/expectは複数箇所で使用（lockとsort）→回避推奨。エラー変換（From/Into）はこのチャンクには現れない。

## Design & Architecture Suggestions

- エラー安全化
  - Mutex lock: lock().map_err(|e| SemanticSearchError::EmbeddingError(format!("Model lock poisoned: {e}")))に変更し、unwrap廃止。
  - ソート: partial_cmp.unwrapの代わりに total_cmp 相当の実装やNaN除外処理を導入。
- ログ/観測性
  - println/eprintlnをlog/tracingへ置換。環境でログレベル制御可能に。SEARCH_DEBUGはデフォルト無効化。
- API一貫性
  - 次元不一致: index_doc_commentの次元不一致もDimensionMismatchを返すよう統一。
- 検索性能改善
  - 大規模N向けにTop-Kヒープ選抜（BinaryHeap）でO(N log K)化。
  - ANNインデックス（HNSWなど）の導入検討。
- ストレージ最適化
  - save時のHashMap→Vec変換のclone削減（所有ベクトルのイテレータでバッチ保存が可能なら利用）。
  - 言語JSONのフォーマット/スキーマ定義（バージョン管理）。
- 拡張性
  - EmbeddingProviderトレイトを導入し、TextEmbeddingに依存しないテスト/モックを可能に（このチャンクには現れない→提案）。
- 使用体験
  - from_model_nameのサポートモデル一覧問い合わせAPIやエラー時の詳細提案を改善。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト
  - cosine_similarityの性質テスト（既存: test_cosine_similarity）。
  - エラー伝播のテスト
    - embeddings空でsearch→NoEmbeddings。
    - 読込時の次元不一致→DimensionMismatch。
  - Mutex Poison/NaN対策（将来修正時）
    - lock失敗時のエラー化テスト。
    - NaNスコアが含まれる際にソートがパニックしないこと。

- 統合テスト
  - インデックス→保存→読込→検索の一連（既存: test_save_and_load）。
  - remove_embeddingsで正しく削除される（既存: test_remove_embeddings）。

- 重いモデル依存テスト
  - 既存テストは#[ignore]で実行制御（86MBダウンロード）。CIではキャッシュ利用/条件付き実行。

- 例（軽量テストの雛形）
```rust
#[test]
fn search_without_embeddings_returns_error() {
    // モデルを初期化せずに直接構築する術はこのチャンクには現れないため、正常系に沿って記述
    let mut search = SimpleSemanticSearch::new().unwrap();
    // クリアして0件にする
    search.clear();
    let err = search.search("anything", 5).unwrap_err();
    match err {
        SemanticSearchError::NoEmbeddings => {},
        _ => panic!("expected NoEmbeddings"),
    }
}
```

- プロパティテスト案（提案）
  - cosine_similarityの範囲が[-1, 1]であること、同一ベクトルが1、正反対が-1、直交が0に近いこと。

## Refactoring Plan & Best Practices

- unwrap除去
  - Mutex lockとソート比較でunwrapを除去し、堅牢化。NaNを弾くか最下位に配置。
- エラー整備
  - EmbeddingErrorとDimensionMismatchの使い分けを統一。
  - InvalidIdの未使用（loadの言語マッピングでは破棄している）を削除/活用方針の決定。
- ログ改善
  - println/eprintln → log/tracing（レベル: info/debug/warn）。
- パフォーマンス
  - Top-K選抜、ANN検討。
  - saveのclone削減、ストレージAPIにイテレート保存が可能なら利用。
- APIの小さな改善
  - search系でlimit=0時は空Vecを即返す最適化。
  - search_with_thresholdで、しきい値フィルタをソート前に実施し、処理件数削減（ただし精度と順序保証に留意）。

## Observability (Logging, Metrics, Tracing)

- ログ
  - SEARCH_DEBUGの標準出力/エラー出力をやめ、log/tracingへ移行。⚙️ 環境変数でレベル制御。
- メトリクス
  - embeddings件数、検索クエリ数、平均類似度、検索時間、モデルロード時間、I/O時間。
  - save/loadの成功/失敗カウンタ。
- トレース
  - 初期化（モデルダウンロード/キャッシュ読み出し）→埋め込み生成→検索→保存/読込のスパン。
- セキュリティログ
  - 失敗詳細に機密クエリを含めない（*ログ漏洩*防止）。

## Risks & Unknowns

- 不明点
  - crate::semantic::SemanticVectorStorage/SemanticMetadataの内部仕様（このチャンクには現れない）。
  - SymbolIdの詳細（to_u32/newの失敗条件）。
  - FastEmbedのTextEmbeddingのSend/Sync実装詳細。
- リスク
  - 大規模N/Dでの検索コスト増。
  - 初回モデルダウンロードの運用影響（CI/オフライン環境）。
  - unwrapによるパニックが本番で発生する可能性。
  - NaN混入時のソートパニック（外部モデル出力がNaNを返す可能性は低いがゼロではない）。
  - 言語JSON破損/互換性問題（スキーマ管理未整備）。

以上の通り、このモジュールはシンプルかつ実用的なセマンティック検索を提供しつつ、運用/拡張に向けたエラー安全性、パフォーマンス、観測性の改善余地があります。適切なエラーハンドリング、ログ整備、Top-K/ANNの導入で、大規模なコードベースにも対応可能になります。