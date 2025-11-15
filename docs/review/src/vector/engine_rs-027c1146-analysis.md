# vector\engine.rs Review

## TL;DR

- 目的: **メモリマップストレージ**と**K-meansベースのクラスタリング（IVFFlat的）**でベクトルをインデックス・検索するエンジン
- 主な公開API: **new**, **index_vectors**, **search**, **get_cluster_for_vector**, **as_centroids**, **vector_count**, **dimension**, **get_all_cluster_assignments**
- 複雑箇所: **search**は「最も近い重心クラスタの候補のみ」を類似度計算するが、候補抽出のために全割り当てを走査する
- 重大リスク: **cosine_similarityの負値をScore::newが拒否**する可能性が高く、候補が静かに欠落（コメントの「[0,1]」主張が誤りの可能性）
- エラー設計: 次元の検証とストレージエラーのラップは適切。**Score作成失敗を握りつぶしている**点は改善余地
- 並行性: ストレージは**ConcurrentVectorStorage**でスレッドセーフ。エンジンはインデックス時に**&mut self**を要求し、検索は**&self**。エンジン自体のSend/Syncは*不明*
- パフォーマンス: 検索はクラスタ一致のチェックのため**O(N)**走査し、候補に対して**O(m·D)**類似度計算。Top-kは**全件ソート**で**O(m·log m)**

## Overview & Purpose

このモジュールは、ベクトル検索の中心的エンジンを提供します。主な責務は以下です。

- ベクトルを**メモリマップファイル**で永続化（MmapVectorStorage）
- バッチインデックス時に**K-meansクラスタリング**で重心を作成し、各ベクトルをクラスタに割り当て
- 検索時に**最も近い重心のクラスタ**内のベクトルに絞って**コサイン類似度**を計算し、上位k件を返す

外部依存はcrate::vectorに集約されており、ストレージ・クラスタリング・距離計算などの下位機能を呼び出して**オーケストレーション**します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | VectorSearchEngine | pub | ストレージ・クラスタリング・検索の調整役 | Med |
| Const | MIN_CLUSTERS | private | 最小クラスタ数 | Low |
| Const | MAX_CLUSTERS | private | 最大クラスタ数 | Low |
| Method | new | pub | ストレージ初期化・状態初期化 | Low |
| Method | index_vectors | pub | 次元検証・書き込み・K-means・割当更新 | Med |
| Method | search | pub | 次元検証・重心選択・候補抽出・類似度計算・Top-k | Med |
| Method | get_cluster_for_vector | pub | ベクトルIDのクラスタ参照 | Low |
| Method | as_centroids | pub | 重心配列の参照 | Low |
| Method | vector_count | pub | 割当済ベクトル数 | Low |
| Method | dimension | pub | 次元情報取得 | Low |
| Method | get_all_cluster_assignments | pub | 全割当の取り出し | Low |

### Dependencies & Interactions

- 内部依存
  - VectorSearchEngine::index_vectors → ConcurrentVectorStorage::write_batch
  - VectorSearchEngine::index_vectors → kmeans_clustering
  - VectorSearchEngine::search → assign_to_nearest_centroid, ConcurrentVectorStorage::read_vector, cosine_similarity, Score::new
  - VectorDimension::validate_vector を new/index_vectors/search で使用

- 外部依存（crate::vector 他）
  | 依存名 | 用途 | 備考 |
  |--------|------|------|
  | ConcurrentVectorStorage | スレッドセーフなベクトルストレージ | write_batch, read_vector |
  | MmapVectorStorage | メモリマップ基盤ストレージ | new |
  | kmeans_clustering | K-meansによる重心生成と割当 | 返却: centroids, assignments |
  | assign_to_nearest_centroid | クエリに最も近い重心選択 | クエリ→ClusterId |
  | cosine_similarity | 類似度計算 | f32戻り（[-1,1]の可能性） |
  | VectorDimension | 次元検証 | validate_vector |
  | VectorId, ClusterId, Score, SegmentOrdinal, VectorError | データ型・エラー |

- 被依存推定
  - 上位層のアプリケーションや検索APIサーバがこのエンジンを生成・利用している可能性
  - 具体的呼び出し箇所はこのチャンクには現れない（不明）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| VectorSearchEngine::new | fn new(storage_path: impl AsRef<Path>, dimension: VectorDimension) -> Result<Self, VectorError> | エンジン初期化（ストレージ準備、状態設定） | O(1)〜O(初期化コスト) | O(1) |
| index_vectors | fn index_vectors(&mut self, vectors: &[(VectorId, Vec<f32>)]) -> Result<(), VectorError> | バッチ書き込み、K-meansで重心生成、クラスタ割当更新 | O(N·D + K-means) | O(N + k·D) |
| search | fn search(&self, query: &[f32], k: usize) -> Result<Vec<(VectorId, Score)>, VectorError> | 最近重心を選び、そのクラスタ内でコサイン類似度Top-k | O(N + m·D + m·log m) | O(m) |
| get_cluster_for_vector | fn get_cluster_for_vector(&self, id: VectorId) -> Option<ClusterId> | 単一IDのクラスタを取得 | O(1)期待（HashMap） | O(1) |
| as_centroids | fn as_centroids(&self) -> &[Vec<f32>] | 重心の参照を返す | O(1) | O(1) |
| vector_count | fn vector_count(&self) -> usize | 割当済みベクトル数の取得 | O(1) | O(1) |
| dimension | fn dimension(&self) -> VectorDimension | 次元情報の取得 | O(1) | O(1) |
| get_all_cluster_assignments | fn get_all_cluster_assignments(&self) -> Vec<(VectorId, ClusterId)> | 全割当の取り出し | O(N) | O(N) |

注: N=総ベクトル数、m=対象クラスタのベクトル数、D=次元。K-meansの正確な計算量はこのチャンクには現れない（一般に O(N·k·I·D); k=クラスタ数、I=反復回数）。

### VectorSearchEngine::new

1. 目的と責務
   - メモリマップストレージを初期化し、エンジン内部状態（割当と重心）をクリアに設定。

2. アルゴリズム（ステップ）
   - SegmentOrdinal::new(0) で単一セグメントを選択
   - MmapVectorStorage::new を呼び出し、ConcurrentVectorStorage にラップ
   - cluster_assignments, centroids を初期化

3. 引数
   | 名前 | 型 | 必須 | 説明 |
   |------|----|------|------|
   | storage_path | impl AsRef<Path> | 必須 | ストレージのベースパス |
   | dimension | VectorDimension | 必須 | ベクトル次元 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Result<Self, VectorError> | 初期化成功時はエンジン、失敗時はエラー |

5. 使用例
   ```rust
   use tempfile::TempDir;
   use crate::vector::{VectorDimension};

   let temp = TempDir::new().unwrap();
   let dim = VectorDimension::new(128).unwrap();
   let engine = VectorSearchEngine::new(temp.path(), dim).unwrap();
   ```

6. エッジケース
   - storage_path が存在しない/権限なし → VectorError::Storage（メッセージにヒント付与）
   - dimension 不整合 → このチャンクでは明示エラーなし（ストレージ側に委譲の可能性・不明）

### index_vectors

1. 目的と責務
   - バッチのベクトルを次元検証後にストレージへ書込み、K-meansで重心を計算し割当を更新。

2. アルゴリズム
   - vectors が空なら何もしないで Ok
   - 各ベクトルの次元検証（VectorDimension::validate_vector）
   - ConcurrentVectorStorage::write_batch で書込み
   - クラスタ数 k を sqrt(N) を基準に MIN/MAX でクランプ
   - kmeans_clustering を実行し重心と割当を受け取る
   - centroids を更新、cluster_assignments を更新

3. 引数
   | 名前 | 型 | 必須 | 説明 |
   |------|----|------|------|
   | vectors | &[(VectorId, Vec<f32>)] | 必須 | 識別子とベクトルのペア |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Result<(), VectorError> | 成功時は空、失敗時は次元・ストレージ・クラスタリングのいずれかのエラー |

5. 使用例
   ```rust
   let mut engine = VectorSearchEngine::new(temp.path(), dim).unwrap();
   let vectors: Vec<(VectorId, Vec<f32>)> = vec![
       (VectorId::new(1).unwrap(), vec![0.0; 128]),
       (VectorId::new(2).unwrap(), vec![0.1; 128]),
   ];
   engine.index_vectors(&vectors).unwrap();
   ```

6. エッジケース
   - vectors 空 → 即時 Ok（インデックスなし）
   - 1件のみ → k=1（MIN_CLUSTERS）でクラスタ一つ
   - 次元不一致 → VectorError（validate_vectorが返す）
   - kmeans_clustering が失敗 → VectorError::ClusteringFailed

### search

1. 目的と責務
   - クエリに最も近い重心を選んで、そのクラスタ内のベクトルに対しコサイン類似度でTop-kを返す。

2. アルゴリズム
   - 次元検証
   - centroids が空なら空結果を返す
   - assign_to_nearest_centroid でクラスタ選択
   - cluster_assignments を走査し、当該クラスタのIDを候補に
   - storage.read_vector でベクトルを取得
   - cosine_similarity を計算し Score::new に渡して成功のみ採用
   - 類似度降順にソートし、k件に切り詰める

3. 引数
   | 名前 | 型 | 必須 | 説明 |
   |------|----|------|------|
   | query | &[f32] | 必須 | クエリベクトル |
   | k | usize | 必須 | 上位件数 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Result<Vec<(VectorId, Score)>, VectorError> | 類似度降順のペア配列。エラーは次元不一致など |

5. 使用例
   ```rust
   let results = engine.search(&query_vec, 10).unwrap();
   for (id, score) in results {
       println!("id={:?}, score={}", id, score.get()); // get() は仮定、実装は不明
   }
   ```

6. エッジケース
   - centroids 空（未インデックス） → 空のVecを返す（非エラー）
   - k=0 → 空のVec（truncateで自然に発生）
   - read_vector が None → 対象をスキップ
   - cosine_similarity が負値 → Score::new が失敗し対象スキップ（重大な設計注意）

🧭 フローチャート（searchの主要分岐）

```mermaid
flowchart TD
    A[次元検証] --> B{centroidsは空か?}
    B -- Yes --> Z[空Vecを返す]
    B -- No --> C[最近重心を選択]
    C --> D[cluster_assignments を全走査]
    D --> E{cluster一致?}
    E -- No --> D
    E -- Yes --> F[storage.read_vector(id)]
    F --> G{Some(vec)?}
    G -- No --> D
    G -- Yes --> H[similarity = cosine(query, vec)]
    H --> I{Score::new(similarity)成功?}
    I -- No --> D
    I -- Yes --> J[候補にpush]
    J --> D
    D --> K[候補をScore降順でsort]
    K --> L[上位k件にtruncate]
    L --> M[返却]
```

上記の図は`search`関数（行番号不明）の主要分岐を示す。

## Walkthrough & Data Flow

- new
  - 入力: storage_path, dimension
  - 生成: MmapVectorStorage → ConcurrentVectorStorage
  - 状態: cluster_assignmentsを空に、centroidsを空に、dimensionを保持

- index_vectors
  - 入力: (VectorId, Vec<f32>)のスライス
  - 検証: dimension.validate_vector
  - 書込: write_batch(&[(VectorId, &[f32])]) へ借用ベクトルを渡す
  - クラスタ数算定: sqrt(N)→clamp([MIN, MAX])
  - kmeans_clusteringで centroids, assignments が返る
  - 状態更新: centroidsに重心、cluster_assignmentsにID→クラスタの対応を格納

- search
  - 入力: query(&[f32]), k
  - 検証: dimension.validate_vector(query)
  - 空インデックス: centroidsが空なら空Vec
  - 重心選択: assign_to_nearest_centroid(query, &centroids)
  - 候補抽出: cluster_assignmentsを全走査しクラスタ一致のみ対象
  - 類似度: read_vector → cosine_similarity → Score::new
  - Top-k: sort_by降順 → truncate(k)

データ契約上、Score::newは値域制約（おそらく0〜1）を持ちます。cosine_similarityが負値を返すと候補から除外される可能性があり、返却集合が意図せず小さくなることがあります。

## Complexity & Performance

- index_vectors
  - 次元検証: O(N·D)
  - write_batch: O(N·D)想定（詳細不明）
  - K-means: 一般に O(N·k·I·D)（詳細は不明）
  - 空間: cluster_assignments O(N)、centroids O(k·D)

- search
  - クラスタ選択: O(k·D)（重心数に比例）
  - 候補抽出: HashMapの全走査 O(N)（一致チェックのみ）
  - 類似度計算: 候補数 m に対し O(m·D)
  - Top-k: sort O(m·log m)、truncate O(1)
  - 空間: 候補ベクトル数 m に比例

ボトルネック
- 候補抽出が全割当 O(N) で、スケール時に非効率
- sortを全件実施（mが大きいとコスト増）

スケール限界
- Nが大きい場合、検索時間がクラスタ選択後も**全割当の走査**に支配される
- K-meansの再計算コストが高いため、頻繁な再インデックスは高コスト

実運用負荷要因
- ストレージI/O（mmapのページフォールト）
- K-meansの反復計算
- 類似度計算のCPU負荷

## Edge Cases, Bugs, and Security

セキュリティチェックリストの評価

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: *このコードでは不明。Rust安全性により直接的な未定義動作は避けられている*
  - 明示的unsafe: なし（このチャンクには現れない）
- インジェクション
  - SQL/Command: 該当なし
  - Path traversal: storage_pathは外部入力だが、そのままMmapVectorStorageに渡す。標準Path利用で直接的な脅威は低いが、検証・サニタイズは上位層で考慮
- 認証・認可
  - 該当なし（ストレージ操作のみ）
- 秘密情報
  - Hard-coded secrets: なし
  - Log leakage: エラー文字列を組み立てて返すのみ。ログ機構は未実装
- 並行性
  - Race condition: エンジンのインデックスは &mut self 要求で競合回避。検索は &self で読み取り。ストレージは ConcurrentVectorStorage 依存（詳細不明）
  - Deadlock: このチャンクではロック操作の詳細なし（不明）

詳細エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空ベクトルバッチ | [] | Ok(()) | index_vectorsが即時Ok | 対応済 |
| 次元不一致（インデックス） | [(id, vec64)] with dim=128 | Err(VectorError) | validate_vectorでErr | 対応済 |
| 次元不一致（検索） | query len=64, dim=128 | Err(VectorError) | validate_vectorでErr | 対応済 |
| 未インデックス検索 | centroids空 | Ok([]) | centroids.is_emptyで空返却 | 対応済 |
| k=0 | 任意 | Ok([]) | truncate(0) | 対応済 |
| ストレージ不整合（ID未存在） | cluster_assignmentsにIDがあるがread_vectorがNone | 該当IDをスキップ。必要なら警告ログ | if let Someでスキップ | 部分対応（ログなし） |
| cosineが負値 | 類似度 -0.2 | Score::newがErrならスキップ | Errは握りつぶし | 未対応（設計課題） |
| K-meansが失敗 | 入力がNaN等 | Err(VectorError::ClusteringFailed) | map_errで変換 | 対応済 |

重大なバグ/懸念
- コメント「Convert similarity to score (already in [0, 1] range)」は誤りの可能性が高い。コサイン類似度は一般に[-1,1]。負値をScore::newが拒否すると結果から除外され、検索の品質に影響。

## Design & Architecture Suggestions

- 候補抽出の高速化
  - いまは全割当のHashMapを走査してクラスタ一致を選別。**ClusterId → Vec<VectorId>**の反転インデックスを持つことで、検索時の候補抽出をO(m)に短縮可能（現状O(N)）。
- Top-k選択の最適化
  - 現在は全候補をsortしてからtruncate。**BinaryHeap（最大/最小ヒープ）**で**O(m·log k)**にできる。
- 類似度とScore整合性
  - cosine_similarityが[-1,1]であるなら、Score::newの仕様に合わせて**(similarity + 1) / 2**への射影、またはScoreが負値を許容する設計変更。
- K-meansの入力コピー削減
  - TODOコメントの通り、`&[&[f32]]`を受け取るAPIに変更し**clone回避**。
- クラスタ数決定の戦略化
  - sqrt(N)は単純。データ分布に応じた**Elbow法**や**Silhouette**（要メトリクス）に基づく選定へ拡張（現状は不明）。
- ストレージとインデックスの整合性検査
  - index_vectors後に**整合性チェック**や**バージョン管理**を導入すると堅牢性向上。
- エンジンの並行性モデル明確化
  - VectorSearchEngineが**Send/Sync**かどうかの明示。複数スレッドでの検索並列実行可否のドキュメント化。

## Testing Strategy (Unit/Integration) with Examples

現行テストの網羅状況
- 生成: 状態初期化を検証
- インデックス＋検索: 基本動作と結果のソート検証
- 未インデックス検索: 空結果
- 次元検証: インデックス、検索ともにエラー確認
- 割当参照: 全ベクトルがクラスタを持つことを確認

追加を推奨するテスト
- cosine負値時のScore::new失敗パス
- k=0の振る舞い
- read_vectorがNoneのケース（ストレージ不整合のシミュレーション）
- 大規模データ（Nが大きい）での検索性能テスト（プロファイル）

使用例（単体テスト相当）

```rust
use tempfile::TempDir;
use crate::vector::{VectorId, VectorDimension};

fn mk_engine() -> VectorSearchEngine {
    let temp = TempDir::new().unwrap();
    let dim = VectorDimension::new(4).unwrap();
    VectorSearchEngine::new(temp.path(), dim).unwrap()
}

#[test]
fn search_handles_negative_cosine_by_skipping() {
    let mut engine = mk_engine();
    // ベクトルを2つ用意し、互いに反対方向（cosine=-1）を想定
    let v1 = vec![1.0, 0.0, 0.0, 0.0];
    let v2 = vec![-1.0, 0.0, 0.0, 0.0];
    let vectors = vec![
        (VectorId::new(1).unwrap(), v1.clone()),
        (VectorId::new(2).unwrap(), v2.clone()),
    ];
    engine.index_vectors(&vectors).unwrap();

    let results = engine.search(&v1, 10).unwrap();
    // Score::new が負値を拒否するなら、反対方向のv2は候補に入らない可能性がある
    // 正確な断言はScoreの仕様次第のため、ここではサイズのみ確認
    assert!(results.len() <= 2);
}
```

## Refactoring Plan & Best Practices

- データ構造の最適化
  - `HashMap<VectorId, ClusterId>`に加え、`HashMap<ClusterId, Vec<VectorId>>`（もしくはVec<Vec<VectorId>>）を保持して**検索時の候補抽出O(1)**化。
- Top-k用アルゴリズム
  - `candidates.sort_by(...)`から**二分ヒープ**へ切替。kが小さい場合の速度向上。
- エラーとロギング
  - `Score::new`失敗時に**警告ログ**。集計メトリクス（失敗率）で品質監視。
- API契約の明確化
  - `cosine_similarity`の値域に関する**ドキュメント**追記。Scoreとの整合を明記。
- コピー回避
  - `index_vectors`の`vecs`作成を避けるため、クラスタリングAPIのシグネチャ拡張（借用スライス対応）。
- モジュール境界の明確化
  - VectorSearchEngineの**Send/Sync**可否の明示。必要なら`#[derive(Clone)]`などの導入と内部共有戦略検討（Arc等）。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 重要イベント: インデックス開始/終了、クラスタ数と反復回数、検索リクエスト、候補数、`Score::new`失敗件数。
  - レベル: info（開始/終了）、debug（候補数・クラスタID）、warn（失敗ケース）
- メトリクス
  - インデックス時間・検索時間（ヒストグラム）
  - 候補数分布（m）
  - `Score::new`失敗率
  - クラスタサイズ分布
- トレーシング
  - span: index_vectors（ストレージ書込、クラスタリング）、search（重心選択、候補抽出、スコアリング）
  - ラベル: ClusterId, k, N, D

## Risks & Unknowns

- crate::vector の実装詳細（kmeans_clustering, cosine_similarity, Score::new, ConcurrentVectorStorageの内部）
  - 値域や誤差、反復回数、収束条件…このチャンクには現れない（不明）
- Send/Sync 境界
  - VectorSearchEngineや内部ストレージが多スレッド利用可能かどうかの正式保証は不明
- ストレージI/Oの特性
  - Mmapのページング・キャッシュ挙動やread_vectorのコストは不明
- K-meansのパラメータ
  - kの算出以外（初期化法、反復数、停止条件）は不明
- 行番号
  - 重要主張の行番号はこのチャンクには現れない（不明）

---

### Rust特有の観点（詳細チェックリスト）

- メモリ安全性（所有権・借用・ライフタイム）
  - index_vectorsで`write_batch`に`&[f32]`を渡すため、**所有権の移動なし**。cloneした`vecs`はK-meansに所有権で渡される。
  - searchで`centroids`から`&[f32]`の借用を生成し、関数呼び出し中のみ有効。**ライフタイムはスコープに限定**され適切。
  - 明示的ライフタイムパラメータは不要。

- unsafe境界
  - unsafeブロックの使用: なし（このチャンクには現れない）
  - 不変条件: 該当なし
  - 安全性根拠: すべて安全なAPIの組み合わせ。

- 並行性・非同期
  - Send/Sync: VectorSearchEngineがSend/Syncかは*不明*。`ConcurrentVectorStorage`はスレッドセーフ設計と推測だが保証はこのチャンクには現れない。
  - データ競合: インデックスは`&mut self`で排他、検索は`&self`で読み取りのみ。**エンジン内のデータ競合は回避**されている設計。
  - await境界/キャンセル: 非同期コードは*該当なし*。

- エラー設計
  - Result vs Option: ストレージの読み取りは`Option`（存在しない可能性）。上位APIは`Result`で業務エラー（次元・ストレージ・クラスタリング）を返す。妥当。
  - panic箇所: 本体コードに`unwrap/expect`はなし（テストのみ）。
  - エラー変換: `map_err`で詳細メッセージを付加し`VectorError`へ集約。良好。

このレビューにおける重要主張は、コードの静的検読に基づくものであり、正確な行番号は「行番号不明（このチャンクには行番号情報が含まれていない）」とします。