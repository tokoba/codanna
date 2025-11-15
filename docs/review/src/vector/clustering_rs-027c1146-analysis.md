# vector\clustering.rs Review

## TL;DR

- 目的: **IVFFlat向けのK-meansクラスタリング**（距離はコサイン類似度、初期化はK-means++、最大100イテレーション、閾値1e-4）（kmeans_clustering）
- 主要公開API: **kmeans_clustering**, **assign_to_nearest_centroid**, **cosine_similarity**, **KMeansResult**, **ClusteringError**
- 複雑箇所: **K-means++初期化**（確率分布に基づく選択）、**空クラスタの再初期化**、**収束判定（割当と重心移動）**
- 重大リスク: **NaN/Infを含む入力**で初期化失敗、**デバッグアサートのみの次元不一致チェック**（cosine_similarity）、**非決定的な乱数利用**による結果の再現性低下
- Rust安全性: **unsafe未使用**。ただし**ClusterId::new_unchecked**の不変条件（1-indexed）に依存
- 並行性: 現状**単一スレッド**。割当ステップは容易に**Rayonで並列化可能**。グローバル状態は非使用
- 性能ボトルネック: **コサイン類似度のnorm計算を毎回実施**（入力ベクトルのnormは不変）。前処理での正規化/ノルムキャッシュで改善可能

## Overview & Purpose

このモジュールは、コード埋め込みベクトルを対象とした**K-meansクラスタリング**実装です。IVFFlat（Inverted File with Flat）インデクシングのための**クラスタ中心（centroid）計算と割当**を提供し、距離尺度として**コサイン類似度**を使用します。初期化には**K-means++**を採用することで、ランダム初期化よりも**収束性の改善**を狙っています。

- 距離: **コサイン類似度**（Euclideanではない）
- 初期化: **K-means++**
- 収束判定: 割当の変化と**重心移動平均**（cosine distance）
- 再現性: デフォルトで**非決定的**（rand::rng利用）
- エラー設計: 極端な/異常入力に対して**ClusteringError**で明確に報告

根拠（関数名:行番号）: kmeans_clustering, initialize_centroids_kmeans_plus_plus, cosine_similarity（行番号不明・このチャンクは行番号情報なし）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | KMeansResult | pub | クラスタリング結果（centroids, assignments, iterations） | Low |
| Enum | ClusteringError | pub | 入力検証と実行時エラーの種別 | Low |
| Fn | kmeans_clustering | pub | K-means主処理（初期化、割当、更新、収束判定） | Med |
| Fn | assign_to_nearest_centroid | pub | コサイン類似度で最近傍重心に割り当て | Low |
| Fn | cosine_similarity | pub | 2ベクトルのコサイン類似度計算 | Low |
| Fn | update_centroids | private | 割当結果に基づく重心更新と正規化 | Med |
| Fn | initialize_centroids_kmeans_plus_plus | private | K-means++に基づく初期重心選択 | Med |
| Fn | calculate_centroid_movement | private | 古い/新しい重心間の移動量（cosine distance平均） | Low |
| Fn | normalize_vector | private | ベクトルの単位長正規化（インプレース） | Low |
| Fn | normalize_vector_copy | private | ベクトルの正規化コピー作成 | Low |
| Mod | tests | private | 単体テスト一式 | Low |

### Dependencies & Interactions

- 内部依存
  - kmeans_clustering → initialize_centroids_kmeans_plus_plus, assign_to_nearest_centroid, update_centroids, calculate_centroid_movement
  - update_centroids → normalize_vector, normalize_vector_copy, rand::rng
  - initialize_centroids_kmeans_plus_plus → cosine_similarity, normalize_vector_copy, rand::rng
  - assign_to_nearest_centroid → cosine_similarity
  - calculate_centroid_movement → cosine_similarity
  - normalize_vector_copy → normalize_vector

- 外部依存（このチャンク内使用）
  | ライブラリ/モジュール | 用途 |
  |----------------------|------|
  | crate::vector::types::ClusterId | クラスタID（1-indexed）。new_unchecked, get |
  | crate::vector::types::VectorError | エラー伝搬用（From）※このチャンクでは実際に発生しない |
  | rand | 乱数（初期化/空クラスタ再初期化） |
  | thiserror::Error | エラー型導出 |

- 被依存推定
  - IVFFlatインデックス構築ロジック（クラスタ中心生成）
  - 検索前のクラスタ割当プリプロセス
  - ベクトルストア構築（シャーディング/クラスタリング）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| kmeans_clustering | `pub fn kmeans_clustering(vectors: &[Vec<f32>], k: usize) -> Result<KMeansResult, ClusteringError>` | K-means主処理（初期化→反復→収束） | O(n·k·d·iter) | O(k·d) |
| assign_to_nearest_centroid | `pub fn assign_to_nearest_centroid(vector: &[f32], centroids: &[&[f32]]) -> ClusterId` | 最近傍重心への割当（コサイン類似度） | O(k·d) | O(1) |
| cosine_similarity | `pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32` | 2ベクトルのコサイン類似度 | O(d) | O(1) |
| KMeansResult | `#[derive(Debug, Clone, PartialEq)] pub struct KMeansResult { pub centroids: Vec<Vec<f32>>, pub assignments: Vec<ClusterId>, pub iterations: usize }` | クラスタリング結果のデータ契約 | - | - |
| ClusteringError | `#[derive(Error, Debug)] pub enum ClusteringError { ... }` | 入力検証/実行時エラー | - | - |

詳細:

1) kmeans_clustering
- 目的と責務
  - ベクトル集合（同次元, 非空）をkクラスタに分割。初期化はK-means++、割当はコサイン類似度、重心更新は平均→正規化。収束は割当不変または重心移動量が閾値以下。
- アルゴリズム（ステップ分解）
  1. 入力検証（空, k=0, 次元不一致）
  2. 初期重心: K-means++（最初は一様ランダム、以降は距離2乗に比例）
  3. 反復:
     - 割当: 各ベクトルを最もコサイン類似度の高い重心に割当
     - 更新: 各クラスタで平均を取り、その後単位長に正規化
     - 収束判定: 割当が不変 もしくは 重心移動平均 < 1e-4
     - 反復上限: 100
  4. 上限到達時は警告をstderr出力して結果を返却
- 引数

  | 名前 | 型 | 意味 | 制約 |
  |------|----|------|------|
  | vectors | `&[Vec<f32>]` | 入力ベクトル集合 | 非空、同一次元、有限値推奨 |
  | k | `usize` | クラスタ数 | 1 ≤ k ≤ vectors.len() |

- 戻り値

  | 型 | 内容 |
  |----|------|
  | `Ok(KMeansResult)` | 正常終了（centroids, assignments, iterations） |
  | `Err(ClusteringError)` | 入力検証/初期化失敗/収束失敗（InitializationFailed など） |

- 使用例

  ```rust
  use crate::vector::clustering::{kmeans_clustering, KMeansResult};

  let vectors = vec![
      vec![1.0, 0.1, 0.0],
      vec![0.9, 0.2, 0.1],
      vec![1.1, 0.0, 0.2],
      vec![0.1, 1.0, 0.0],
  ];
  let result = kmeans_clustering(&vectors, 2)?;
  println!("centroids: {:?}", result.centroids);
  println!("assignments: {:?}", result.assignments);
  println!("iterations: {}", result.iterations);
  # Ok::<(), Box<dyn std::error::Error>>(())
  ```

- エッジケース
  - vectorsが空 → Err(EmptyVectorSet)
  - k=0, k>n → Err(InvalidClusterCount(k))
  - 次元不一致 → Err(DimensionMismatch)
  - K-means++で重心をk個選べない（全点一致/NaN等） → Err(InitializationFailed)
  - 収束せずMAX_ITERATIONS到達 → 警告出力（結果は返却）

2) assign_to_nearest_centroid
- 目的と責務
  - 入力ベクトルを、与えられた重心配列の中からコサイン類似度最大のクラスタへ1-indexedで割当
- アルゴリズム
  1. 全重心に対してcosine_similarityを計算
  2. 最大類似度のインデックスを選択
  3. ClusterId::new_unchecked(best_index+1)で返す
- 引数

  | 名前 | 型 | 意味 | 制約 |
  |------|----|------|------|
  | vector | `&[f32]` | 割当対象ベクトル | centroidsと次元一致前提（debug_assert） |
  | centroids | `&[&[f32]]` | 重心群（スライス） | 次元一致、k≥1 |

- 戻り値

  | 型 | 内容 |
  |----|------|
  | `ClusterId` | 1〜kのクラスタID |

- 使用例

  ```rust
  use crate::vector::clustering::assign_to_nearest_centroid;

  let centroids = vec![vec![1.0,0.0], vec![0.0,1.0]];
  let refs: Vec<&[f32]> = centroids.iter().map(|c| c.as_slice()).collect();
  let id = assign_to_nearest_centroid(&[0.8, 0.2], &refs);
  assert_eq!(id.get(), 1);
  ```

- エッジケース
  - ベクトルと重心の次元不一致 → debug_assert（リリースでは未検出）
  - 全重心に対してcosine_similarityが同値 → 最初に出現した重心を選択

3) cosine_similarity
- 目的と責務
  - コサイン類似度 = dot(a,b) / (||a||·||b||)。零ベクトルが含まれる場合は0.0
- アルゴリズム
  1. dot: Σ a[i]*b[i]
  2. norm: sqrt(Σ a[i]^2), sqrt(Σ b[i]^2)
  3. normが0なら0.0、そうでなければ割り算
- 引数

  | 名前 | 型 | 意味 | 制約 |
  |------|----|------|------|
  | a | `&[f32]` | ベクトルA | 次元一致（debug_assert） |
  | b | `&[f32]` | ベクトルB | 次元一致（debug_assert） |

- 戻り値

  | 型 | 内容 |
  |----|------|
  | `f32` | [-1,1]範囲を想定（零ベクトル含むと0.0） |

- 使用例

  ```rust
  use crate::vector::clustering::cosine_similarity;
  assert!((cosine_similarity(&[1.0,0.0], &[1.0,0.0]) - 1.0).abs() < f32::EPSILON);
  ```

- エッジケース
  - 次元不一致 → debug_assertに依存（リリースビルドでは未検出）
  - 零ベクトル → 0.0を返す
  - NaNを含む入力 → 結果がNaNになる可能性（上位で検知していない）

データ契約: KMeansResult（centroids: k×d, assignments: n×ClusterId(1..=k), iterations: 1..=MAX）、ClusteringError（EmptyVectorSet, InvalidClusterCount(k), DimensionMismatch, InitializationFailed, ConvergenceFailed(iter), VectorError）

## Walkthrough & Data Flow

処理の中心は kmeans_clustering です。

- 入力検証
  - 空集合/不正k/次元不一致を早期Err（入力品質の保証）
- 初期化（K-means++）
  - 最初の重心は一様ランダムに選択し正規化
  - 各ベクトルの既存重心への最小「コサイン距離（1-類似度）」を計算し、その二乗で確率分布を形成
  - 累積確率に基づいて次の重心を選択、正規化
  - 全距離合計が極小なら早期停止→重心数がk未満ならエラー
- 反復（最大100回）
  - 割当: assign_to_nearest_centroid（n×k×d）
  - 収束1: 割当が不変なら終了
  - 更新: update_centroids（クラスタ平均→正規化、空クラスタはランダム再初期化）
  - 収束2: calculate_centroid_movement < 1e-4 なら終了
- 非収束
  - 100回到達でstderrに警告しつつ結果返却

Mermaidフローチャート（主要分岐）

```mermaid
flowchart TD
  A[Start: validate inputs] -->|ok| B[Init centroids: K-means++]
  A -->|Err| Z[Return ClusteringError]
  B --> C{centroids.len()==k?}
  C -->|no| Z
  C -->|yes| D[iterations=0]
  D --> E[Assign: nearest centroid for each vector]
  E --> F{assignments changed?}
  F -->|no| Y[Return result]
  F -->|yes| G[Update centroids: mean & normalize]
  G --> H[Movement = avg(1 - cosine)]
  H --> I{Movement < tol?}
  I -->|yes| Y
  I -->|no| J{iterations >= 100?}
  J -->|yes| K[eprintln! warning] --> Y
  J -->|no| D
```

上記の図は `kmeans_clustering` 関数の主要分岐（行番号不明・このチャンクは行番号情報なし）を示します。

## Complexity & Performance

- 時間計算量
  - 割当: O(n·k·d)
  - 更新: O(n·d + k·d)
  - 初期化（K-means++）: O(n·k·d)
  - 総合: O(n·k·d·iterations)
- 空間計算量
  - 重心: O(k·d)
  - 割当: O(n)
- ボトルネック
  - cosine_similarityで毎回normを計算（入力ベクトル側のnormは不変）。n·k回×dのsqrt/加算が支配的
  - K-means++時の全点距離計算
- スケール限界/実運用負荷
  - 高次元（d≫）かつ大規模（n≫）では初期化と割当の計算負荷が増大
  - I/O/ネットワーク/DBは未関与。このモジュール単体ではCPUバウンド

改善提案（短く）
- 入力ベクトルの**事前正規化**または**ノルムキャッシュ**でcosine_similarityを高速化
- **Rayon**による割当ステップの並列化（n独立）
- K-means++の距離計算を**ベクトル化/SIMD**（将来的に）

## Edge Cases, Bugs, and Security

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空ベクトル集合 | `[]` | Err(EmptyVectorSet) | kmeans_clusteringの前半でチェック | OK |
| k=0 | n=1, k=0 | Err(InvalidClusterCount(0)) | 事前検証 | OK |
| k>n | n=2, k=3 | Err(InvalidClusterCount(3)) | 事前検証 | OK |
| 次元不一致 | `[vec![1,2], vec![1,2,3]]` | Err(DimensionMismatch) | 事前検証 | OK |
| K-means++選択不能 | 全点が同一/NaN混入 | Err(InitializationFailed) | total_distance<EPSILONでブレーク後長さ検査 | OK（だが保守的） |
| 零ベクトル | `[0,0,...]` | cosine_similarityは0.0 | norm==0で0.0返す | OK |
| 空クラスタ発生 | 反復中 | ランダムに再初期化して正規化 | update_centroidsでrand再初期化 | OK/非決定 |
| NaN/Inf含む | `vec![NaN,...]` | エラー/除外が望ましい | 明示的検知なし、初期化失敗に波及 | 要対策 |
| 次元不一致のassign呼出 | 外部利用 | Err/panicが望ましい | debug_assertのみ | 要対策 |
| 非正規化入力 | 任意 | 類似度は問題なし（関数内でnorm計算） | 想定通り | OK |
| 非収束 | 難例 | 警告出力、結果返却 | eprintln! | OK |

セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow: インデックスアクセスは`cluster_id.get()-1`に依存。assign_to_nearest_centroidが生成するIDは1..=kで安全。外部から不正IDが渡ると危険だが、このモジュール内では安全（update_centroidsが使うassignmentsは内部生成）（根拠: update_centroids, assign_to_nearest_centroid）
  - Use-after-free: 所有権/借用は安全（スライス/Vecのみ）
  - Integer overflow: k, n, dはusize/f32で安全領域。浮動の演算はオーバーフロー未検知だが通常範囲
- インジェクション
  - SQL/Command/Path: 該当なし
- 認証・認可
  - 該当なし
- 秘密情報
  - Hard-coded secrets: なし
  - Log leakage: eprintln!で非機密警告のみ
- 並行性
  - Race condition/Deadlock: 単一スレッド、グローバル状態なし
  - rand::rng(): スレッドローカル想定、競合なし

Rust特有の観点
- 所有権
  - 参照主体（`&[Vec<f32>]`受領, `&[f32]`スライスを渡す）。ムーブは`normalize_vector_copy`でコピー生成のみ（根拠: normalize_vector_copy）
- 借用
  - `centroid_refs`は`&[f32]`スライス参照のベクタ。ライフタイムは反復内のみ
- ライフタイム
  - 明示的ライフタイムなしで十分（関数境界内完結）
- unsafe境界
  - unsafeブロック: なし
  - `ClusterId::new_unchecked`は名前上uncheckedだがunsafeではない。1-indexed不変条件の遵守が前提（根拠: assign_to_nearest_centroid, kmeans_clustering初期化）
- 並行性・非同期
  - Send/Sync: `Vec<f32>`/`f32`はSend+Sync。関数は純粋計算でスレッドセーフ
  - await/cancel: 該当なし
- エラー設計
  - Result: kmeans_clustering, update_centroids, initialize_centroids_kmeans_plus_plusで使用
  - Option: 未使用
  - panic: インデックス越境の潜在性はassignmentsが内部生成な限り低い。cosine_similarityの次元不一致はdebug_assertのみ（リリースでは未検知）

潜在的バグ/改善点
- cosine_similarityの次元不一致がリリースビルドで未検知。外部APIとして安全にするなら**実行時チェック**または`Result`返却が望ましい
- K-means++がk個選べない場合は即Err。代替として「ランダム補完」や「重心重複許容」などで続行可能にする選択肢を提供
- 乱数シード制御が不可。**再現性の要求**がある場合に問題

## Design & Architecture Suggestions

- API設計
  - KMeansConfig（max_iterations, tolerance, seed, distance metric）を受け取る**Builderパターン**導入で柔軟性を向上
  - `cosine_similarity`を外部API利用時にも安全にするため**次元チェックを実施**し`Result<f32, VectorError>`返却や`assert!`に変更（公開APIでのパニック方針は要検討）
- 再現性
  - `seed: Option<u64>`をkmeans_clusteringへ追加。rand RNGを明示的に初期化可能に
- 距離戦略の抽象化
  - `trait Similarity { fn sim(a:&[f32], b:&[f32]) -> f32 }`で将来的に**他の距離**（内積、ユークリッド）へ拡張可能
- 初期化のフォールバック
  - K-means++が失敗した場合、**ランダム選択**で補完しつつ続行。エラーではなく**Warning**にダウングレード可能
- 代数的最適化
  - 入力を**事前正規化**（ベクトル側norm=1）すればcosine_similarityはdotのみになり高速化

## Testing Strategy (Unit/Integration) with Examples

既存テスト
- cosine_similarityの基礎ケース（同一/直交/逆/零）
- assign_to_nearest_centroidの選択検証
- kmeans_clusteringの基本（3クラスタで各軸に近い群を正しく分離）
- エッジケース（空、k=0/k>n、次元不一致）
- normalize_vectorの正規化検証

追加推奨テスト
- 反復停止条件の検証
  - 割当不変での停止と、重心移動量閾値での停止の両方
- 初期化失敗の厳密検証
  - 全点完全一致/NaN混入時にInitializationFailedを返すか
- 再現性テスト（seed導入後）
  - 同seedで同結果、異seedで異なる可能性
- 空クラスタ再初期化
  - update_centroidsでsize==0の分岐を強制し、その後の挙動が健全か
- 大規模ケースの性能ベンチ
  - n=1e5, d=768程度での実行時間計測（ベンチマーク）

例: NaN入力の初期化失敗

```rust
#[test]
fn test_kmeans_init_nan_failure() {
    let vectors = vec![vec![f32::NAN, 0.0], vec![0.0, 1.0]];
    let res = kmeans_clustering(&vectors, 2);
    assert!(matches!(res, Err(ClusteringError::InitializationFailed)));
}
```

例: 割当不変での停止確認

```rust
#[test]
fn test_kmeans_converges_on_stable_assignments() {
    // 既に明確に分離されたデータ
    let vectors = vec![vec![1.0, 0.0], vec![0.9, 0.0], vec![0.0, 1.0], vec![0.0, 0.9]];
    let res = kmeans_clustering(&vectors, 2).unwrap();
    assert!(res.iterations >= 1);
}
```

## Refactoring Plan & Best Practices

- 高速化
  1. 入力ベクトルを事前に`normalize_vector`（新規API: normalize_all(vectors)）し、`cosine_similarity`をdotのみで計算
  2. 事前正規化しない場合でも、ベクトル側の**ノルムをキャッシュ**して再利用（HashMap<index, norm>またはVec<f32>）
- 並列化
  3. 割当ステップを`par_iter()`で並列化（Rayon）。重心は読み取り専用のスライス参照で安全。例:

     ```rust
     use rayon::prelude::*;

     let centroid_refs: Vec<&[f32]> = centroids.iter().map(|c| c.as_slice()).collect();
     let new_assignments: Vec<ClusterId> = vectors.par_iter()
         .map(|v| assign_to_nearest_centroid(v, &centroid_refs))
         .collect();
     ```

- API & 安全性
  4. `cosine_similarity`に次元一致の**実行時チェック**を追加（Option/Result）
  5. `ClusterId::new_unchecked`の使用箇所で**不変条件のコメント**明記（1..=k）
- 初期化改善
  6. K-means++失敗時に**ランダム補完**で続行可能にし、`InitializationFailed`はオプション化
- ログ
  7. `eprintln!`を`log`クレートへ移行し、**レベル制御（warn）**と**可観測性**を向上

## Observability (Logging, Metrics, Tracing)

- ログ
  - 収束失敗/上限到達時: warn
  - 各イテレーション: movement値、変更された割当数（debugレベル）
- メトリクス
  - iterations（ゲージ）
  - avg centroid movement（ヒストグラム）
  - reassignment count per iteration（ヒストグラム）
  - empty cluster occurrences（カウンタ）
- トレーシング
  - span: "kmeans_clustering"（fields: n, k, d）
  - event: "iteration"（movement, changes）

例（logクレート）

```rust
log::warn!(
    "K-means did not fully converge after {} iterations",
    MAX_ITERATIONS
);
log::debug!("iter {} movement {:.6} reassigned {}", iterations, movement, changes);
```

## Risks & Unknowns

- NaN/Infを含む入力の扱いが未定義に近く、**初期化失敗**や**不安定な割当**を引き起こす可能性
- 再現性の要求（同じ入力で同じ結果）に対して**seed未提供**。利用側要件次第で問題化
- `ClusterId`型の内部仕様（1-indexed不変条件の厳密性、`new_unchecked`の安全性保証）は**このチャンクには現れない**
- `VectorError`の実際の発生箇所は**このチャンクには現れない**（From導入のみ）
- 高次元・大規模データでの**性能要件**は利用側次第。必要に応じてSIMD/並列化が必須になる可能性