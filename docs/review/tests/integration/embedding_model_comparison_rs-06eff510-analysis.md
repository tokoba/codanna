# embedding_model_comparison.rs Review

## TL;DR

- 目的: 複数の埋め込みモデルを使い、コード/ドキュメントの意味的類似性に最適なモデルを比較・評価するベンチマーク兼統合テスト。
- 主要ロジック: evaluate_model で各モデルを初期化し、テストケースごとに埋め込み生成・類似度計算し、メトリクスを集計。
- 公開API: 該当なし（全てテスト/内部関数）。ただし内部APIとして get_test_cache_dir/evaluate_model/cosine_similarity が中核。
- 重要な複雑点: 類似度のしきい値設計（0.7/0.75）とメトリクス計算（NaN/ゼロ除算の可能性）、Debug表現に依存したモデルサイズ推定。
- 潜在的リスク: results/max_by の unwrap、accuracy/平均値のゼロ除算、cosine_similarity のゼロベクトル分母0、異次元ベクトルの暗黙切り捨て、外部ダウンロード失敗。
- Rust安全性: unsafeなし。主なパニックポイントは unwrap とインデクシング、ゼロ除算。非同期/並行処理は未使用。
- テスト: 3つの #[test] があり全て #[ignore]（大容量モデルDLのため）。しきい値探索やコードドキュメント類似性の検証を含む。

## Overview & Purpose

このファイルは、fastembed クレートの埋め込みモデル（現状は **EmbeddingModel::AllMiniLML6V2**）を用い、コード/ドキュメントの短文ペアに対する意味的類似性の評価を行う統合テスト群です。評価は以下の観点で行われます。

- 埋め込み生成の性能（平均生成時間）
- モデル特性（次元数、サイズの概算）
- 品質（類似/非類似ペアの平均類似度、閾値による二値判定の正解率）

主に開発時にローカルで実行するベンチマーク用途を想定し、大容量モデルのダウンロードが発生するため全テストはデフォルトで無効（#[ignore]）になっています。

注: 行番号は本チャンクに含まれないため、関数名のみで根拠を示します（行番号: 不明）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | get_test_cache_dir | private | テストごとに一意なキャッシュディレクトリを生成 | Low |
| Struct | CodeExample | private | テスト用の短文ペアと期待ラベルを保持 | Low |
| Struct | ModelEvaluation | private | モデル評価結果（性能/品質メトリクス）を集約 | Low |
| Test Function | compare_embedding_models | private(test) | 複数モデルを横断評価して結果比較 | Medium |
| Function | evaluate_model | private | 1モデルの初期化、埋め込み、類似度計算、集計を実施 | Medium |
| Function | cosine_similarity | private | 2ベクトルのコサイン類似度を計算 | Low |
| Test Function | test_code_specific_similarity | private(test) | コードドキュメント例での類似度判定テスト | Medium |
| Test Function | test_similarity_thresholds | private(test) | テキストペアと期待レンジでしきい値の目安を得る | Low/Medium |

### Dependencies & Interactions

- 内部依存
  - compare_embedding_models → evaluate_model → cosine_similarity
  - test_code_specific_similarity → cosine_similarity
  - test_similarity_thresholds → cosine_similarity
  - get_test_cache_dir は compare_embedding_models/test_* から使用

- 外部依存（クレート/標準ライブラリ）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | anyhow::Result | テスト関数のエラー伝搬 | シンプルなエラー集約 |
  | fastembed::{EmbeddingModel, InitOptions, TextEmbedding} | 埋め込みモデルの初期化と推論 | 大容量モデルDLあり |
  | std::time::Instant | 時間計測 | ナノ秒精度 |
  | std::env::temp_dir, std::process::id, std::path::PathBuf | キャッシュパス生成 | プロセスIDで衝突回避 |

- 被依存推定
  - このファイルは統合テスト専用。通常のライブラリ/アプリ本体からは参照されない想定。

## API Surface (Public/Exported) and Data Contracts

- 公開API: 該当なし（このチャンクには現れない）。以下は内部APIの一覧。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| get_test_cache_dir | fn get_test_cache_dir(test_name: &str) -> PathBuf | 一意なキャッシュディレクトリ生成 | O(|test_name|) | O(|test_name|) |
| evaluate_model | fn evaluate_model(model_type: EmbeddingModel, test_cases: &[CodeExample], cache_dir: PathBuf) -> Result<ModelEvaluation> | モデルの初期化・類似度評価・集計 | O(T · D) + 埋め込み推論 | O(D) |
| cosine_similarity | fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 | コサイン類似度計算 | O(d) | O(1) |

詳細:

1) get_test_cache_dir
- 目的と責務: テスト名とプロセスIDで一意なキャッシュディレクトリを生成し、モデルのダウンロードキャッシュ衝突を回避。
- アルゴリズム:
  1. std::env::temp_dir() を基点にする。
  2. "codanna_test_fastembed_{test_name}_{pid}" というサブディレクトリ名を join。
- 引数:
  | 名称 | 型 | 説明 |
  |------|----|------|
  | test_name | &str | テストケース識別子 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | PathBuf | 生成した一意ディレクトリ |
- 使用例:
  ```rust
  let cache_dir = get_test_cache_dir("compare_embedding_models");
  ```
- エッジケース:
  - test_name にファイルシステム的に不正な文字: 現状無検証（通常のASCIIなら問題なし）。
  - temp_dir の権限不足: 後続でDL失敗。

2) evaluate_model
- 目的と責務: 指定モデルで複数テキストペアを埋め込み→コサイン類似度→しきい値判定→メトリクス集計を行い、ModelEvaluation を返す。
- アルゴリズム（概略）:
  1. Instant で初期化時間計測開始。
  2. TextEmbedding::try_new(InitOptions::new(model_type).with_cache_dir(cache_dir).with_show_download_progress(true)) でモデル初期化。
  3. ダミーテキストで埋め込みし次元数を取得。
  4. 各テストケースについて:
     - 2文をまとめて embed、計測時間を保存。
     - cosine_similarity を計算。
     - しきい値（0.7）で予測ラベルを生成し、正解数をカウント。類似/非類似ごとのスコアに蓄積。
  5. 正解率、ペアごとの平均スコア、平均埋め込み時間を計算。
  6. model_name の Debug 表現に基づき model_size_mb を概算。
  7. ModelEvaluation を構築して返す。
- 引数:
  | 名称 | 型 | 説明 |
  |------|----|------|
  | model_type | EmbeddingModel | 評価対象の埋め込みモデル |
  | test_cases | &[CodeExample] | 短文ペアと期待ラベル |
  | cache_dir | PathBuf | モデルキャッシュ用ディレクトリ |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Result<ModelEvaluation> | 成功時: 評価結果、失敗時: anyhow エラー |
- 使用例:
  ```rust
  let test_cases = vec![
    CodeExample { name: "ex", code1: "a", code2: "b", expected_similar: true }
  ];
  let eval = evaluate_model(EmbeddingModel::AllMiniLML6V2, &test_cases, get_test_cache_dir("demo"))?;
  println!("acc={}", eval.accuracy);
  ```
- エッジケース:
  - test_cases が空: 率/平均の分母ゼロで NaN/パニックの恐れ。
  - similar_scores/different_scores が片方空: 平均計算でゼロ除算。
  - モデル初回DL失敗/ネットワーク不通: try_new/embed の ? で早期 Err。
  - 埋め込みがゼロベクトル/NaN を含む: cosine_similarity で NaN。

3) cosine_similarity
- 目的と責務: 2つの f32 スライス間のコサイン類似度を返す。
- アルゴリズム:
  1. 要素積の総和で内積を求める。
  2. 各ベクトルの二乗和の平方根でノルムを求める。
  3. 内積 / (ノルムの積) を返す。
- 引数:
  | 名称 | 型 | 説明 |
  |------|----|------|
  | a | &[f32] | ベクトルA |
  | b | &[f32] | ベクトルB |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | f32 | コサイン類似度（理論上 [-1,1]） |
- 使用例:
  ```rust
  let s = cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]);
  assert!((s - 1.0).abs() < 1e-6);
  ```
- エッジケース:
  - a または b がゼロベクトル: 分母0で NaN/inf。
  - a と b の次元不一致: zip により短い方に暗黙切り詰め（静かに情報損失）。

データコントラクト:
- CodeExample: name(&'static str), code1(&'static str), code2(&'static str), expected_similar(bool)
- ModelEvaluation: model_name(String), dimensions(usize), avg_embedding_time_ms(f64), model_size_mb(f64), similar_pairs_score(f32), different_pairs_score(f32), accuracy(f32)

## Walkthrough & Data Flow

compare_embedding_models の流れ:
1. 一意な cache_dir を生成（get_test_cache_dir）。
2. 類似/非類似の CodeExample を定義（5件）。
3. 評価対象モデルのリストを定義（AllMiniLML6V2 のみ）。
4. 各モデルに対して evaluate_model を呼び出し、ModelEvaluation を収集。
5. モデルごとのサマリを表示し、max_by で最良モデルを選定して表示。

evaluate_model の詳細フロー:
- モデル初期化（TextEmbedding::try_new with InitOptions）→ ダミー埋め込みで次元数確認。
- 各テストケースで:
  - 2文を同時に embed → 計測時間記録。
  - cosine_similarity でスコア算出。
  - しきい値 0.7 超過=類似と判定し、期待ラベルと比較して正解数を加算。
  - 類似/非類似のグループ別にスコアを蓄積。
- 最終的に accuracy、グループ平均、平均埋め込み時間を算出し、ModelEvaluation を返す。

test_code_specific_similarity:
- コードドキュメントの3ペアで埋め込み→類似度計算→しきい値 0.75 で可否表示。

test_similarity_thresholds:
- 既知の関係の短文ペアで類似度を出力し、しきい値帯の目安を提示。

注: 並行処理・非同期処理は使用していません。I/O は fastembed のモデルDLとファイルキャッシュのみ。

## Complexity & Performance

- evaluate_model:
  - 時間計算量: O(T · C_embed) + O(T·d) 目安
    - T: テストケース数
    - d: 埋め込み次元数
    - C_embed: モデル推論（埋め込み生成）コスト（支配的）
  - 空間計算量: O(d)（一度に2件の埋め込みを保持）
- cosine_similarity:
  - 時間 O(d), 空間 O(1)
- ボトルネック:
  - モデル初回ダウンロード（86MB規模）
  - モデル推論時間（CPU/GPU/環境依存）
  - デバッグ出力（ println ）は軽微だが大量ループではノイズ

スケール限界:
- 現状は2文/ケースの逐次実行で、ケース数が大きくても線形に処理。並列化なし。
- しきい値やメトリクスはメモリ的には軽量。

実運用負荷要因:
- ネットワーク: 初回DL時
- ディスクI/O: キャッシュ保存/読込
- CPU: 埋め込み推論

## Edge Cases, Bugs, and Security

セキュリティチェックリスト:
- メモリ安全性: unsafe なし。インデクスアクセスは成功パス前提（embed の戻りサイズに依存）。
- インジェクション: 外部コマンド/SQL 等への入力なし。リスク低。
- 認証・認可: 対象外。
- 秘密情報: ログに機微情報なし。キャッシュパスが含まれるのみ。
- 並行性: 単一プロセス/単一テストでは競合なし。テスト間衝突は一意ディレクトリで回避。ただし同一プロセス・同一テスト名で並列に使う設計ではない。

不具合/リスク:
- unwrap のパニック:
  - compare_embedding_models: max_by(...).unwrap() および partial_cmp(...).unwrap()
    - models や results が空のときパニック。
    - accuracy が NaN の場合 partial_cmp が None を返しパニック。
- 分母ゼロ:
  - evaluate_model: test_cases が空、あるいは similar_scores または different_scores が空の場合に平均算出でゼロ除算。
- cosine_similarity:
  - ゼロベクトルや NaN を含むベクトルで分母0、NaN/inf を返却。
  - 次元不一致で zip による静かな切り捨て（気づきにくい品質劣化）。
- モデルサイズ推定:
  - Debug 表現文字列一致で 90MB を推定。将来の表現変更で誤推定（フォールバック100MB）。
- エラーハンドリング:
  - ネットワーク不通/キャッシュ破損時の再試行やフォールバックなし（? で失敗終了）。

エッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空テスト集合 | test_cases = [] | Err(InvalidInput) または安全に 0 件として処理 | 平均/accuracy 計算でゼロ除算/NaN | 要修正 |
| similar/different 片方ゼロ | 全て expected_similar=true | 片方の平均は None として扱うかスキップ | ゼロ除算の可能性 | 要修正 |
| 結果ゼロ件 | models = [] | 「モデルなし」扱い/スキップ | best_model の unwrap でパニック | 要修正 |
| NaN accuracy | test_cases=0 や類似度NaNが混入 | 安全に処理（NaNを無視/0扱い等） | partial_cmp unwrap でパニック | 要修正 |
| 埋め込みゼロベクトル | モデル出力が全ゼロ | 類似度を 0 とみなす等 | 分母0で NaN | 要対策 |
| ベクトル次元不一致 | a.len() != b.len() | 明示エラー | zipで短い側に合わせる | 要検討 |
| キャッシュ競合 | 同一プロセス同一test_name並列 | 避ける/ロック | プロセスIDのみで一意化 | 許容（低確率） |
| ダウンロード失敗 | オフライン | 明確なエラーメッセージと再試行 | ? で即 Err | 許容（改善余地） |

## Design & Architecture Suggestions

- しきい値/評価戦略の分離:
  - 類似度しきい値を引数/設定で外部化。グリッドサーチや ROC/AUC の計算に対応。
  - 二値判定だけでなく、平均類似度や分布、F1/Precision/Recall、AUC を計測。
- 安全なメトリクス計算:
  - 件数ゼロを許容し、平均は Option<f32>/Option<f64> で返すか、分母ゼロ時は None。
  - accuracy/平均で NaN を産まない設計（分母ゼロは 0 または None）。
- モデル識別:
  - Debug 表現依存を廃し、enum 値や固有IDで識別。サイズは実ファイルサイズから算出。
- エラーハンドリング:
  - unwrap を排し、空コレクション時は明示的エラー。partial_cmp の None をハンドリング（NaN ガード）。
- 抽象化/テスト容易性:
  - 埋め込み取得を Trait 抽象（Embedder）でモック可能にし、ネットワーク不要のユニットテストを実現。
- ベクトル正規化:
  - cosine_similarity 前に L2 正規化（または fastembed 側で正規化）を確認/適用し、ゼロ除算/スケール差を回避。
- ベンチマーク専用化:
  - criterion で安定したベンチマークを実施。テストから分離。
- 出力構造化:
  - println! から log/tracing に切り替え、JSON/CSV 出力（レポート保存）を可能に。

## Testing Strategy (Unit/Integration) with Examples

優先ユニットテスト（ネットワーク不要）:

- cosine_similarity の性質
  ```rust
  #[test]
  fn cosine_similarity_basic() {
      assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
      assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
      // 次元不一致は短い方に合わせる現仕様
      let s = cosine_similarity(&[1.0, 2.0, 3.0], &[1.0, 2.0]);
      assert!(s.is_finite());
  }
  ```

- ゼロベクトル安全化（改善後）
  ```rust
  fn cosine_similarity_safe(a: &[f32], b: &[f32]) -> f32 {
      let dot: f32 = a.iter().zip(b.iter()).map(|(x,y)| x*y).sum();
      let ma: f32 = a.iter().map(|x| x*x).sum::<f32>().sqrt();
      let mb: f32 = b.iter().map(|x| x*x).sum::<f32>().sqrt();
      if ma == 0.0 || mb == 0.0 { 0.0 } else { dot / (ma * mb) }
  }

  #[test]
  fn cosine_similarity_zero_guard() {
      assert_eq!(cosine_similarity_safe(&[0.0, 0.0], &[1.0, 2.0]), 0.0);
  }
  ```

- メトリクス計算のゼロ件保護（改善後）
  ```rust
  fn avg_or_none(xs: &[f32]) -> Option<f32> {
      if xs.is_empty() { None } else { Some(xs.iter().sum::<f32>() / xs.len() as f32) }
  }

  #[test]
  fn avg_or_none_handles_empty() {
      assert_eq!(avg_or_none(&[]), None);
      assert_eq!(avg_or_none(&[1.0, 3.0]), Some(2.0));
  }
  ```

統合テスト（現状の #[ignore] に加えて）:
- ネットワーク・キャッシュテスト: オフライン時のエラー伝搬、キャッシュヒット時の再DL抑制。
- しきい値スイープ: 0.5〜0.9 をステップでスキャンし、ベストしきい値を記録。
- モデル追加時の回帰テスト: 新モデルを増やしても平均時間/精度が退化しないことを確認。

プロパティテスト（任意）:
- proptest によるランダムベクトルでの cosine_similarity の [-1,1] 制約（ゼロ除算ガード後）。

## Refactoring Plan & Best Practices

- unwrap 除去とエラー型の明確化
  ```rust
  let best_model = results
      .iter()
      .filter(|m| m.accuracy.is_finite())
      .max_by(|a,b| a.accuracy.partial_cmp(&b.accuracy).unwrap_or(std::cmp::Ordering::Equal));
  if let Some(best) = best_model {
      println!("Best model: {}", best.model_name);
  } else {
      anyhow::bail!("No valid model evaluations");
  }
  ```

- 平均/率の安全計算ユーティリティを導入（Option 返却）。
- cosine_similarity にゼロ除算ガードを追加。または埋め込みを正規化して保持。
- モデル識別を Debug 文字列から enum/ID に変更。サイズは実ファイルサイズを測定（std::fs::metadata）。
- しきい値を引数化し、compare_embedding_models/evaluate_model でパラメータとして受け取る。
- 抽象 Embedder トレイト導入で TextEmbedding を注入（テストでモック可能に）。
- ログ/tracing を導入し、ベンチ結果を CSV/JSON で保存。

ベストプラクティス:
- 浮動小数点は f64 を優先（集計・平均・比較の安定性向上）。
- 統合テストとベンチマーク（criterion）を分離。
- 外部I/O（DL）は再試行やタイムアウト、明確なメッセージを付与。

## Observability (Logging, Metrics, Tracing)

- Logging:
  - println! から **tracing** もしくは **log + env_logger** に切替。モデル初期化/埋め込み/判定結果を info/debug レベルで出力。
- Metrics:
  - 埋め込み時間のヒストグラム（ms）
  - 類似度スコアの分布（類似/非類似）
  - ダウンロードバイト数、キャッシュヒット/ミス（fastembed 側の公開情報に依存）
- Tracing:
  - evaluate_model に span を張り、各ケースを子spanで可視化。
- レポート出力:
  - CSV/JSON で結果を保存し、CIでアーティファクト化。閾値/モデル比較の差分追跡を容易に。

## Risks & Unknowns

- fastembed のモデル・API 変更:
  - Debug 表現の変更でサイズ推定がずれる可能性。APIの互換性も不明。
- 埋め込みの正規化有無:
  - モデル出力が正規化済みか不明（このチャンクには現れない）。ゼロベクトルや極端なスケールで NaN/inf リスク。
- 環境依存:
  - CPU/GPU、OS、ファイルシステム権限、ネットワーク状況に影響される。CI 実行の安定性が未知。
- データセットの代表性:
  - テストペアは少数で簡潔。現実のコード/ドキュメント検索に対する外的妥当性は未確認。
- 並列実行時のキャッシュ:
  - 同一プロセス内での同名テストの並列利用は想定外。fastembed のキャッシュ書き込みがスレッド/プロセスセーフかは不明（このチャンクには現れない）。

## Edge Cases, Bugs, and Security

- Rust特有の観点（メモリ安全性）
  - 所有権: モデルインスタンスは関数ローカルで所有し、スコープ終了で解放。移動/借用の複雑性は低い。
  - 借用/ライフタイム: &'static str を入力として使用し、明示的ライフタイムは不要。
  - unsafe: 使用なし（このチャンクには現れない）。
- 並行性・非同期
  - Send/Sync: モデル型の Send/Sync 特性は未検証（このチャンクには現れない）。当該コードは単一スレッド。
  - データ競合: 共有状態なし。キャッシュは一意ディレクトリを使用。
  - await境界/キャンセル: 非async。該当なし。
- エラー設計
  - Result vs Option: テスト関数は anyhow::Result を返却。二値判定/平均値で Option を使う設計に改善余地。
  - panic箇所: unwrap 使用（best_model, partial_cmp）。配列インデクスは埋め込み結果を仮定。
  - エラー変換: From/Into の独自実装なし。anyhow で集約。

以上を踏まえ、しきい値・平均計算のガード、unwrapの除去、ログ/メトリクスの拡充、抽象化によるテスト容易性向上を優先して改善するのが有効です。