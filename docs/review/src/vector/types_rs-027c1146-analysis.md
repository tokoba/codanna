# vector/types.rs Review

## TL;DR

- 目的: ベクトル検索のための**型安全なラッパー**（VectorId/ClusterId/SegmentOrdinal/Score/VectorDimension）と、**明確なエラー型**（VectorError）を提供。
- 主な公開API: VectorId/ClusterIdのID生成・シリアライズ、Scoreの検証・重み付き合成、VectorDimensionの検証、SegmentOrdinalのシリアライズ、定数VECTOR_DIMENSION_384。
- コアロジック: Scoreの範囲/NaN検証と安全な比較（Ord）、NonZeroU32によるIDのゼロ禁止、VectorDimensionの長さ一致検証。
- 重要な注意点: new_uncheckedのパニック、Score::cmpがNaN前提破りでpanicしうる（ただしScore生成がNaNを拒否する設計）、エンディアンをリトルエンディアンに固定。
- セキュリティ/安全性: unsafe未使用、インジェクション面の懸念なし、並行性の共有状態なし。ID/Scoreの入力検証あり。
- テスト: 主要経路（ID/Score/Dimension/序数）の単体テストあり。プロパティテスト・シリアライズ互換テストの追加が有用。
- 推奨改善: TryFrom/Fromの導入、serdeサポート、VectorId/ClusterIdのDisplay実装、new_uncheckedの使用方針の明確化、エラーコード/分類の整備。

## Overview & Purpose

このモジュールは、ベクトル検索システムで頻出するプリミティブ（u32, f32, usize）を直接扱わず、誤用の余地を減らすための**新しい型（newtype）ラッパー**を提供します。目的は以下です。

- VectorId/ClusterId: ゼロ無効（NonZeroU32）による不正値の排除、安定したシリアライズ。
- SegmentOrdinal: 0許容のTantivyセグメント序数。
- Score: [0.0, 1.0]かつ非NaNの類似度スコア、比較可能、重み付き合成。
- VectorDimension: ゼロ禁止の次元値、ベクトル長の検証。
- VectorError: 操作時の失敗を**行動可能なメッセージ**で表現。

設計方針は「型安全」「明確な契約」「わかりやすい失敗理由」を重視しています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Const | VECTOR_DIMENSION_384 | pub | 標準の384次元定数 | Low |
| Struct | VectorId | pub | ベクトルID（NonZeroU32、シリアライズ） | Low |
| Struct | ClusterId | pub | クラスタID（NonZeroU32、シリアライズ） | Low |
| Struct | SegmentOrdinal | pub | Tantivyセグメント序数（0許容、表示） | Low |
| Struct | Score | pub | 類似度スコアの検証・比較・合成 | Med |
| Struct | VectorDimension | pub | 次元値の検証・ベクトル長チェック | Low |
| Enum | VectorError | pub | ベクトル関連の失敗（詳細メッセージ） | Med |

### Dependencies & Interactions

- 内部依存
  - Score → VectorError（検証失敗時のエラー返却）
  - VectorDimension → VectorError（ゼロ次元・長さ不一致）
  - VectorId/ClusterId → NonZeroU32（ゼロ防止）
  - SegmentOrdinal → std::fmt::Display（表示）
  - ScoreのOrd → f32::partial_cmp（NaN非想定）
- 外部依存

  | クレート/モジュール | 用途 |
  |--------------------|------|
  | std::num::NonZeroU32 | ゼロ禁止IDの実現 |
  | thiserror::Error | エラー型の派生（Display実装含む） |
  | std::io::Error | VectorError::Storage 変換元 |

- 被依存推定
  - ストレージ層（IDの永続化やキー管理）
  - インデクサ（IVFFlatのClusterId、セグメント管理でSegmentOrdinal）
  - 検索評価・ランキング（Score）
  - ベクトル前処理・検証（VectorDimension）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| VECTOR_DIMENSION_384 | pub const VECTOR_DIMENSION_384: usize | 標準384次元の定数 | O(1) | O(1) |
| VectorId::new | pub fn new(id: u32) -> Option<Self> | ゼロでないIDの生成（失敗はNone） | O(1) | O(1) |
| VectorId::new_unchecked | pub fn new_unchecked(id: u32) -> Self | ゼロチェックなし（ゼロならpanic） | O(1) | O(1) |
| VectorId::get | pub fn get(&self) -> u32 | 内部値取得 | O(1) | O(1) |
| VectorId::to_bytes | pub fn to_bytes(&self) -> [u8; 4] | リトルエンディアンでシリアライズ | O(1) | O(1) |
| VectorId::from_bytes | pub fn from_bytes([u8; 4]) -> Option<Self> | バイト列から復元（ゼロはNone） | O(1) | O(1) |
| ClusterId::new | pub fn new(id: u32) -> Option<Self> | ゼロでないクラスタID生成 | O(1) | O(1) |
| ClusterId::new_unchecked | pub fn new_unchecked(id: u32) -> Self | ゼロチェックなし（ゼロならpanic） | O(1) | O(1) |
| ClusterId::get | pub fn get(&self) -> u32 | 内部値取得 | O(1) | O(1) |
| ClusterId::to_bytes | pub fn to_bytes(&self) -> [u8; 4] | リトルエンディアンでシリアライズ | O(1) | O(1) |
| ClusterId::from_bytes | pub fn from_bytes([u8; 4]) -> Option<Self> | バイト列から復元（ゼロはNone） | O(1) | O(1) |
| SegmentOrdinal::new | pub const fn new(u32) -> Self | 序数の生成（0許容） | O(1) | O(1) |
| SegmentOrdinal::get | pub const fn get(&self) -> u32 | 内部値取得 | O(1) | O(1) |
| SegmentOrdinal::to_bytes | pub fn to_bytes(&self) -> [u8; 4] | リトルエンディアンでシリアライズ | O(1) | O(1) |
| SegmentOrdinal::from_bytes | pub fn from_bytes([u8; 4]) -> Self | バイト列から復元 | O(1) | O(1) |
| Score::new | pub fn new(f32) -> Result<Self, VectorError> | スコア検証（[0,1], 非NaN） | O(1) | O(1) |
| Score::zero | pub const fn zero() -> Self | 0スコアの生成 | O(1) | O(1) |
| Score::one | pub const fn one() -> Self | 1スコアの生成 | O(1) | O(1) |
| Score::get | pub fn get(&self) -> f32 | 内部値取得 | O(1) | O(1) |
| Score::weighted_combine | pub fn weighted_combine(&self, other: Score, weight: f32) -> Result<Self, VectorError> | 重み付き平均で合成（weight検証） | O(1) | O(1) |
| VectorDimension::new | pub fn new(usize) -> Result<Self, VectorError> | 次元の検証（ゼロ禁止） | O(1) | O(1) |
| VectorDimension::dimension_384 | pub const fn dimension_384() -> Self | 標準384次元の生成 | O(1) | O(1) |
| VectorDimension::get | pub const fn get(&self) -> usize | 内部値取得 | O(1) | O(1) |
| VectorDimension::validate_vector | pub fn validate_vector(&self, vector: &[f32]) -> Result<(), VectorError> | ベクトル長の一致検証 | O(1) | O(1) |
| VectorError | pub enum VectorError | 失敗状態の表現（Error/Display） | — | — |

以下、主要APIを型ごとに詳細化します。

### VectorId

1. 目的と責務
   - ベクトルIDをゼロ禁止で安全に管理。
   - ストレージ向けの固定幅シリアライズ/デシリアライズ。

2. アルゴリズム（ステップ）
   - new: NonZeroU32::new(id)で検証し、Some(Self) or None。
   - new_unchecked: NonZeroU32::new(id).expect(..)でpanic。
   - to_bytes/get/from_bytes: u32のリトルエンディアン変換。

3. 引数

   | 名前 | 型 | 説明 |
   |------|----|------|
   | id | u32 | 生成時のID値（ゼロ禁止） |
   | bytes | [u8; 4] | リトルエンディアンのu32表現 |

4. 戻り値

   | 関数 | 型 | 説明 |
   |------|----|------|
   | new | Option<VectorId> | ゼロならNone |
   | new_unchecked | VectorId | ゼロ時panic |
   | get | u32 | 内部値 |
   | to_bytes | [u8; 4] | シリアライズ |
   | from_bytes | Option<VectorId> | 0表現ならNone |

5. 使用例
   ```rust
   let id = VectorId::new(42).expect("non-zero");
   assert_eq!(id.get(), 42);
   let bytes = id.to_bytes();
   let id2 = VectorId::from_bytes(bytes).expect("non-zero");
   assert_eq!(id, id2);
   ```

6. エッジケース
   - id=0 → newはNone、new_uncheckedはpanic。
   - from_bytesが[0,0,0,0] → None。
   - エンディアン固定（LE）で他システムとの互換性注意。

### ClusterId

1. 目的と責務
   - IVFFlatクラスタIDのゼロ禁止管理とシリアライズ。

2. アルゴリズム
   - VectorIdと同様にNonZeroU32ベース。

3. 引数/戻り値
   - VectorIdと同様（役割がClusterIdに置換）。

4. 使用例
   ```rust
   let cid = ClusterId::new(1).expect("non-zero");
   assert_eq!(cid.get(), 1);
   let cid2 = ClusterId::from_bytes(cid.to_bytes()).expect("non-zero");
   assert_eq!(cid, cid2);
   ```

5. エッジケース
   - id=0は不許可（None/panic）。
   - 既存のVectorError::InvalidClusterIdは本型のnewでは使われない（別コンテキストで使用想定）。

### SegmentOrdinal

1. 目的と責務
   - Tantivyセグメント序数（0許容）の保持、表示、シリアライズ。

2. アルゴリズム
   - new/getは単純な包み外し。
   - シリアライズはu32のLE変換。

3. 引数/戻り値

   | 関数 | 引数 | 戻り値 |
   |------|------|--------|
   | new | ordinal: u32 | Self |
   | get | &self | u32 |
   | to_bytes | &self | [u8;4] |
   | from_bytes | bytes: [u8;4] | Self |

4. 使用例
   ```rust
   let s0 = SegmentOrdinal::new(0);
   let s1 = SegmentOrdinal::new(42);
   assert!(s0 < s1);
   let restored = SegmentOrdinal::from_bytes(s1.to_bytes());
   assert_eq!(restored.get(), 42);
   ```

5. エッジケース
   - 0が有効値。
   - 整序性（PartialOrd/Ord派生）により比較可能。

### Score

1. 目的と責務
   - 類似度スコアの安全な管理（[0.0, 1.0], 非NaN）、比較可能性、重み付き合成。

2. アルゴリズム
   - new: NaNチェック、[0,1]範囲チェック→Ok(Self) or Err(VectorError::InvalidScore)。
   - weighted_combine: weightのNaN・範囲検証→合成 Self(self*weight + other*(1-weight))。
   - Ord: f32::partial_cmpにexpect（NaN不発生前提）。

3. 引数

   | 名前 | 型 | 説明 |
   |------|----|------|
   | value | f32 | スコア値（[0,1], 非NaN） |
   | other | Score | 合成対象 |
   | weight | f32 | 自分の重み（[0,1], 非NaN） |

4. 戻り値

   | 関数 | 型 | 説明 |
   |------|----|------|
   | new | Result<Score, VectorError> | 検証失敗時にエラー |
   | zero/one | Score | 定数スコア |
   | get | f32 | 内部値 |
   | weighted_combine | Result<Score, VectorError> | weight不正時にエラー |

5. 使用例
   ```rust
   let s1 = Score::new(0.8)?;
   let s2 = Score::new(0.6)?;
   let s = s1.weighted_combine(s2, 0.7)?;
   assert!((s.get() - 0.74).abs() < f32::EPSILON);
   # Ok::<(), VectorError>(())
   ```

6. エッジケース
   - valueがNaN/範囲外 → InvalidScore。
   - weightがNaN/範囲外 → InvalidWeight。
   - Ord実装はNaN前提破りでpanicする可能性があるが、Score生成がNaN拒否により通常安全。

### VectorDimension

1. 目的と責務
   - 次元値の検証（ゼロ禁止）、ベクトル（&[f32]）長の一致検証。

2. アルゴリズム
   - new: 0ならInvalidDimension。
   - validate_vector: vector.len() != self.0ならDimensionMismatch。
   - dimension_384: 定数から生成。

3. 引数

   | 名前 | 型 | 説明 |
   |------|----|------|
   | dim | usize | 次元値（0禁止） |
   | vector | &[f32] | 長さ検証対象 |

4. 戻り値

   | 関数 | 型 | 説明 |
   |------|----|------|
   | new | Result<VectorDimension, VectorError> | 0でエラー |
   | dimension_384 | VectorDimension | 384固定 |
   | get | usize | 内部値 |
   | validate_vector | Result<(), VectorError> | 長さ不一致でエラー |

5. 使用例
   ```rust
   let dim = VectorDimension::new(384)?;
   dim.validate_vector(&vec![0.1; 384])?;
   assert!(dim.validate_vector(&vec![0.1; 100]).is_err());
   # Ok::<(), VectorError>(())
   ```

6. エッジケース
   - dim=0 → InvalidDimension。
   - 長さ不一致 → DimensionMismatch。

### VectorError

1. 目的と責務
   - 操作時の失敗を型で表し、ユーザに有用な提案を含めたメッセージを提供。

2. 主なバリアント（抜粋）
   - DimensionMismatch { expected, actual }
   - InvalidDimension { dimension, reason }
   - InvalidScore { value, reason }
   - CacheWarming(String)
   - InvalidClusterId(u32)
   - Storage(std::io::Error)（From変換あり）
   - EmbeddingFailed(String), ClusteringFailed(String), Serialization(String)
   - VectorNotFound(u32)
   - InvalidWeight { value, reason }
   - VersionMismatch { expected, actual }

3. 使用例
   ```rust
   fn make_score(x: f32) -> Result<Score, VectorError> {
       Score::new(x)
   }
   ```

4. データ契約
   - reasonは &'static str（固定文言）。
   - 一部はString（動的メッセージ）で詳細を保持。

## Walkthrough & Data Flow

- ID生成フロー（VectorId/ClusterId）
  - 入力u32 → NonZeroU32::new → Some(Self) or None。
  - ストレージ時は to_bytes（LE）を使用、復元時は from_bytes → 0ならNone。
- Score評価/合成フロー
  - Score::newでNaN/範囲外を拒否。
  - weighted_combineでweight検証 → Self(self*weight + other*(1-weight))。
  - 比較はOrd（高スコアほど大きい）。
- Dimension検証フロー
  - VectorDimension::newでゼロ拒否。
  - validate_vectorで vector.len() と self.get() を比較し一致を要求。
- SegmentOrdinalのシリアライズ
  - to_bytes/from_bytesでLE変換、表示はDisplay。
- 外部との連携（このチャンクには現れない/不明）
  - VectorError::Storage/EmbeddingFailed等は上位層で生成・ハンドリングされる想定。

※ 処理は直線的で分岐は少なく、Mermaid図の使用基準（条件分岐4以上/状態遷移3以上）に満たないため図は省略。

## Complexity & Performance

- すべての公開メソッドは**時間計算量O(1)、空間O(1)**。
- validate_vectorは長さ比較のみでO(1)。
- シリアライズは固定長（4バイト）で高速。
- パフォーマンス上のボトルネックは存在しないが、VectorErrorのString生成はヒープ割り当てを伴う（例: EmbeddingFailed）。
- 実運用負荷要因（このファイル単体では不明）
  - I/O/ネットワーク/DBは未使用。VectorError::Storageは外部I/Oに由来。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - unsafe未使用。全ての操作は安全な標準APIで実装。
  - バッファオーバーフロー/Use-after-free/整数オーバーフローの懸念なし。
- インジェクション
  - SQL/Command/Path traversalに関わる入力処理なし。
- 認証・認可
  - 対象外（このモジュールは値型のみ管理）。
- 秘密情報
  - ハードコードされた秘密情報なし。ログ漏洩の懸念なし。
- 並行性
  - 共有可変状態なし。全型はCopy可能でSend + Sync（数値ラッパー）となるのが通常。データ競合の懸念なし。
- 既知/潜在バグ・懸念
  - Score::Ordのcmpがpartial_cmpのexpectに依存。Scoreが外部から不正に生成（例: メモリ破壊、unsafeなtransmute）されない限り安全。ただし「NaNでpanic」するため、この前提はドキュメント化するのが望ましい。
  - new_uncheckedがゼロでpanic。ユースケースでは性能上の理由で許容されるが、使用方針（テスト済み値に限定）を明記すると良い。
  - エンディアン固定（LE）の仕様が外部システムと一致しない場合、互換性問題。プロトコル/フォーマット仕様に明記すべき。
  - VectorError::InvalidClusterIdはClusterId::newでは返らず、別ロジックで使われる設計。名前が混乱を招く恐れあり（ドメイン境界を明確化）。

### Edge Cases詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| VectorIdゼロ | 0 | None | VectorId::new | OK（テスト済み） |
| VectorId uncheckedゼロ | 0 | panic("VectorId cannot be zero") | VectorId::new_unchecked | OK（テスト済み） |
| VectorId from_bytesゼロ | [0,0,0,0] | None | VectorId::from_bytes | OK（テスト済み） |
| ClusterIdゼロ | 0 | None | ClusterId::new | OK（テスト済み） |
| SegmentOrdinalゼロ | 0 | 正常生成 | SegmentOrdinal::new | OK（テスト済み） |
| Score負値 | -0.1 | Err(InvalidScore) | Score::new | OK（テスト済み） |
| Score>1 | 1.1 | Err(InvalidScore) | Score::new | OK（テスト済み） |
| Score NaN | f32::NAN | Err(InvalidScore) | Score::new | OK（テスト済み） |
| Weight負値 | -0.2 | Err(InvalidWeight) | Score::weighted_combine | OK（テスト済み） |
| Weight>1 | 1.2 | Err(InvalidWeight) | Score::weighted_combine | OK（テスト済み） |
| Weight NaN | f32::NAN | Err(InvalidWeight) | Score::weighted_combine | OK（テスト済み） |
| 次元ゼロ | 0 | Err(InvalidDimension) | VectorDimension::new | OK（テスト済み） |
| 長さ不一致 | dim=384, len=100 | Err(DimensionMismatch) | VectorDimension::validate_vector | OK（テスト済み） |

（行番号はこのチャンクでは不明）

## Design & Architecture Suggestions

- 型間の変換
  - TryFrom/Fromの実装を追加（例: TryFrom<u32> for VectorId/ClusterId）でAPIの一貫性向上。
  - Into<u32>の実装によりget()不要の変換が容易に。
- 表示/デバッグ
  - VectorId/ClusterIdにDisplay実装を追加（SegmentOrdinal同様）し、ログ/メッセージ出力を統一。
- シリアライズ
  - serdeのSerialize/Deserializeを導入し、構造化データ保存/転送を簡素化（to_bytes/from_bytesに加えて）。
  - エンディアンポリシーをドキュメント化（プロトコル仕様化）。
- エラー設計
  - VectorErrorに分類（ErrorKind）やコードを付けるとプログラム的ハンドリングが容易。
  - InvalidClusterIdは作成時ではなく運用時の状態エラーである旨をドキュメント化。
- 次元管理
  - const generics（例: struct Vector<const N: usize>）を別層で用いると、次元一致をコンパイル時に保証可能（ただし柔軟性とのトレードオフ）。
- パニック方針
  - new_uncheckedの使用条件をドキュメント化（データが検証済みであるパス限定）。
  - Score::cmpのNaN前提を明記。必要ならば「total ordering」の実装（NaNを最小扱い等）を検討。

## Testing Strategy (Unit/Integration) with Examples

- 既存の単体テスト
  - VectorId/ClusterIdの生成・シリアライズ、SegmentOrdinalの比較、Scoreの検証/合成、VectorDimensionの検証が網羅。
- 追加推奨（プロパティテスト）
  - proptestによる包括的検証:
    - 任意の非ゼロu32でVectorId/ClusterId round-trip（to_bytes→from_bytes）不変性。
    - 任意のscore∈[0,1]とweight∈[0,1]でweighted_combineの結果が[0,1]に収まる。
    - 任意のusize>0でVectorDimension::newが成功し、validate_vectorがlen一致で成功。
  ```rust
  // 例: proptestでのScore合成の不変性
  use proptest::prelude::*;
  proptest! {
      #[test]
      fn weighted_combine_is_bounded(s1 in 0.0f32..=1.0, s2 in 0.0f32..=1.0, w in 0.0f32..=1.0) {
          let a = Score::new(s1).unwrap();
          let b = Score::new(s2).unwrap();
          let c = a.weighted_combine(b, w).unwrap();
          prop_assert!(c.get() >= 0.0 && c.get() <= 1.0);
      }
  }
  ```
- 互換性テスト
  - エンディアン・シリアライズの仕様テスト（異システム想定のLE/BE確認）。
- ベンチマーク
  - ほぼ不要だが、エラーメッセージ生成のオーバーヘッドが問題化する場合のみ測定。

## Refactoring Plan & Best Practices

- API一貫性
  - VectorId/ClusterId/SegmentOrdinalにTryFrom/From/Intoを実装し、get/to_bytesの呼び出し頻度を減らす。
- ドキュメンテーション
  - ScoreのNaN非許容とOrdの前提、new_uncheckedの使用条件を明文化。
  - エンディアン仕様を明記。
- 機能追加
  - serdeサポートで外部フォーマットとの連携強化。
  - Displayの追加（VectorId/ClusterId）。
- エラー整備
  - VectorErrorに識別コード/レベル（recoverable/non-recoverable）付与。
- 安全性強化
  - new_uncheckedの内部利用範囲限定（pub(crate)化の検討）。
  - SegmentOrdinalにもfrom_bytesでの検証を追加（必要なら範囲制限）。

## Observability (Logging, Metrics, Tracing)

- 現状、値型のみでロギングは未実装。
- 推奨
  - エラー生成箇所で識別子（コード）を付与し、上位層でログ集計しやすくする。
  - Metrics: InvalidScore/InvalidWeight/DimensionMismatchの発生回数を上位層でカウント。
  - Tracing: 重要なIDのシリアライズ/デシリアライズ失敗時にスパンタグとしてID値を記録（0検出など）。

## Risks & Unknowns

- Unknowns
  - 本型の利用箇所（ストレージ、ネットワークプロトコル）の仕様詳細はこのチャンクには現れない。
  - VectorErrorの一部（EmbeddingFailed/ClusteringFailed/Serialization等）の具体的発生条件は不明。
- Risks
  - 異なるシステム間でのエンディアン不一致。
  - パニック（new_unchecked、Score::cmpのNaN前提破り）がシステムの信頼性に影響し得る。
  - InvalidClusterIdの意味が型の生成と混同される可能性（ドメインイベント由来である旨の明確化が必要）。

※ Rust特有の観点
- 所有権/借用/ライフタイム: すべてCopy・値型で、ライフタイム管理不要。可変借用の長期保持なし。
- unsafe境界: なし。
- Send/Sync: 数値ラッパーのみのため自動的にSend + Syncとなるのが通常。
- 非同期/await: 非該当。
- エラー設計: 結果が失敗しうる箇所にResult、検証が不要/不可能な箇所にOption（IDゼロの明確な非エラーケース）。unwrap/expectはnew_unchecked/Score::cmp内の設計上の前提に限定。