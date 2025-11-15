# vector\embedding.rs Review

## TL;DR

- 目的: fastembed を用いたテキストからのベクトル埋め込み生成。検索用ベクトル次元の検出と整合性チェックを提供。
- 主要公開API: **EmbeddingGenerator** トレイト、**FastEmbedGenerator**（new/with_model/from_settings/model_name）、**parse_embedding_model**、**model_to_string**、**create_symbol_text**。
- 核心ロジック: モデル初期化時に埋め込みを1件生成して次元を自動検出、生成時は **Mutex** でモデルを同期し、全埋め込みの次元を検証。
- 並行性: TextEmbedding に対する同時利用を **Mutex** で直列化（スレッドセーフだがスループットに影響）。
- 重大リスク: 初期化時の「test」埋め込みに対する `unwrap()` で空結果時に panic の可能性。生成時の巨大入力と大量バッチでメモリ・CPU負荷増。
- エラー設計: さまざまな失敗を `VectorError::EmbeddingFailed` に集約、未知モデルは同種のエラー返却。より厳密なエラー分類の余地あり。
- セキュリティ: 外部I/Oはモデルダウンロードのみ。インジェクションや認可は本ファイルに該当なし。ログへの秘密情報露出なし。

## Overview & Purpose

本モジュールは、コードシンボル（名前やシグネチャ等）から検索用のベクトル埋め込みを生成するための汎用インターフェースと **fastembed** 実装を提供します。特定モデル（既定: AllMiniLML6V2）をダウンロード・初期化し、テキストのバッチ埋め込みを生成します。埋め込み次元の整合性を検証し、上位のベクトルエンジンと安全に連携できるよう設計されています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Trait | EmbeddingGenerator | pub | テキストからの埋め込み生成の抽象インターフェース（Send+Sync） | Low |
| Struct | FastEmbedGenerator | pub | fastembed を用いてモデル初期化・バッチ埋め込み生成 | Med |
| Function | parse_embedding_model | pub | 文字列から EmbeddingModel への変換 | Low |
| Function | model_to_string | pub | EmbeddingModel からモデル名文字列への変換 | Low |
| Function | create_symbol_text | pub | シンボル情報から埋め込み用テキストへ整形 | Low |
| Struct | MockEmbeddingGenerator | cfg(test) | テスト用決定的埋め込み生成 | Low |

### Dependencies & Interactions

- 内部依存
  - FastEmbedGenerator → EmbeddingGenerator（トレイト実装）
  - FastEmbedGenerator → VectorDimension, VectorError（crate::vector）
  - FastEmbedGenerator → TextEmbedding, InitOptions, EmbeddingModel（fastembed）
  - FastEmbedGenerator → crate::init::models_dir（モデルキャッシュディレクトリ）
  - create_symbol_text → crate::types::SymbolKind

- 外部依存（推奨表）

| 依存 | 用途 | 備考 |
|-----|-----|-----|
| fastembed::{TextEmbedding, InitOptions, EmbeddingModel} | モデル初期化と埋め込み生成 | モデルダウンロード、バッチ埋め込み |
| crate::vector::{VectorDimension, VectorError} | 次元表現とエラー型 | 次元検証、失敗原因伝搬 |
| crate::init::models_dir() | モデルキャッシュ場所 | 初回ダウンロード時に利用 |
| crate::types::SymbolKind | シンボル種別 | create_symbol_text で整形 |

- 被依存推定
  - インデクサ（例: SimpleIndexer）からのバッチ埋め込み生成呼び出し
  - ベクトル検索エンジン（例: VectorSearchEngine）への埋め込み提供
  - シンボル抽出フェーズでの `create_symbol_text` 利用
  - これらの具体的型・関数はこのチャンクには現れない（不明）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| parse_embedding_model | `pub fn parse_embedding_model(model_name: &str) -> Result<EmbeddingModel, VectorError>` | 文字列→モデル列挙の変換 | O(1) | O(1) |
| model_to_string | `pub fn model_to_string(model: &EmbeddingModel) -> String` | モデル列挙→名前文字列 | O(1) | O(1) |
| EmbeddingGenerator::generate_embeddings | `fn generate_embeddings(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, VectorError>` | バッチ埋め込み生成 | O(N·C) 注 | O(N·D) |
| EmbeddingGenerator::dimension | `fn dimension(&self) -> VectorDimension` | 埋め込み次元の取得 | O(1) | O(1) |
| FastEmbedGenerator::new | `pub fn new() -> Result<Self, VectorError>` | 既定モデルで初期化 | O(1)+I/O | O(1) |
| FastEmbedGenerator::new_with_progress | `pub fn new_with_progress() -> Result<Self, VectorError>` | 既定モデル＋進捗表示で初期化 | O(1)+I/O | O(1) |
| FastEmbedGenerator::with_model | `pub fn with_model(model: EmbeddingModel, show_progress: bool) -> Result<Self, VectorError>` | 指定モデルで初期化 | O(1)+I/O | O(1) |
| FastEmbedGenerator::from_settings | `pub fn from_settings(model_name: &str, show_progress: bool) -> Result<Self, VectorError>` | 設定文字列からモデル選択 | O(1)+I/O | O(1) |
| FastEmbedGenerator::model_name | `pub fn model_name(&self) -> &str` | モデル名の取得 | O(1) | O(1) |
| create_symbol_text | `pub fn create_symbol_text(name: &str, kind: crate::types::SymbolKind, signature: Option<&str>) -> String` | シンボル→埋め込み用テキスト整形 | O(|name|+|sig|) | O(|name|+|sig|) |

注:
- N = texts の件数、D = 埋め込み次元、C = モデルの計算コスト（トークン数などにほぼ線形）。fastembed の実際の複雑度はモデルと入力長に依存します。

以下、主要APIの詳細。

### parse_embedding_model

1. 目的と責務
   - モデル名文字列を **fastembed::EmbeddingModel** 列挙に変換し、未知名は `VectorError::EmbeddingFailed` で通知。

2. アルゴリズム（ステップ）
   - `match model_name` で既知のバリアントへマッピング。
   - 該当なしの場合、詳細メッセージ付きの `EmbeddingFailed` を返す。

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| model_name | &str | Yes | モデル名（例: "AllMiniLML6V2"） |

4. 戻り値

| 型 | 説明 |
|----|------|
| Result<EmbeddingModel, VectorError> | 成功時は列挙、失敗時は未知モデルエラー |

5. 使用例

```rust
use crate::vector::embedding::parse_embedding_model;
use fastembed::EmbeddingModel;

let model = parse_embedding_model("MultilingualE5Small")?;
assert!(matches!(model, EmbeddingModel::MultilingualE5Small));
```

6. エッジケース
- 空文字列 → 未知モデルエラー
- 大文字小文字の違い → 現状は厳密一致（"allminilml6v2" は不一致）
- 新モデルの追加時 → テーブル更新が必要

### model_to_string

1. 目的と責務
   - EmbeddingModel 列挙から人間可読なモデル名文字列へ変換。

2. アルゴリズム
   - 列挙ごとに固定文字列を返却。

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| model | &EmbeddingModel | Yes | モデル列挙 |

4. 戻り値

| 型 | 説明 |
|----|------|
| String | モデル名 |

5. 使用例

```rust
use crate::vector::embedding::{model_to_string, parse_embedding_model};

let m = parse_embedding_model("AllMiniLML6V2")?;
assert_eq!(model_to_string(&m), "AllMiniLML6V2".to_string());
```

6. エッジケース
- 未知モデルは入力に現れないため想定外（parseで排除）
- 生成文字列の余計なアロケーションは O(1) で軽微

### EmbeddingGenerator::generate_embeddings（FastEmbedGenerator 実装）

1. 目的と責務
   - テキスト配列から埋め込みをバッチ生成。次元の整合性を保証。

2. アルゴリズム
   - 入力が空なら空ベクトルを返す。
   - `&[&str]` → `Vec<String>` に変換（fastembed API要件）。
   - `Mutex<TextEmbedding>` を lock し `embed()` 実行。
   - 各埋め込みの長さが期待次元と一致するか検証。
   - 成功時に `Vec<Vec<f32>>` を返す。

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| texts | &[&str] | Yes | 埋め込み対象テキスト（UTF-8） |

4. 戻り値

| 型 | 説明 |
|----|------|
| Result<Vec<Vec<f32>>, VectorError> | 各テキストの埋め込み |

5. 使用例

```rust
use std::sync::Arc;
use crate::vector::embedding::{FastEmbedGenerator, EmbeddingGenerator, create_symbol_text};
use crate::types::SymbolKind;

let gen = Arc::new(FastEmbedGenerator::new()?);

// シンボルからテキストを作り埋め込み
let s1 = create_symbol_text("parse_json", SymbolKind::Function, Some("fn parse_json(input: &str) -> Result<Value>"));
let s2 = create_symbol_text("Point", SymbolKind::Struct, None);
let texts: Vec<&str> = vec![&s1, &s2];

let embeddings = gen.generate_embeddings(&texts)?;
assert_eq!(embeddings.len(), 2);
assert_eq!(embeddings[0].len(), gen.dimension().get());
```

6. エッジケース
- 入力空 → 空ベクトル（Ok(Vec::new())）
- lock 取得失敗（毒化）→ `EmbeddingFailed` で通知
- fastembed 側の失敗 → `EmbeddingFailed`
- 次元不一致（ライブラリ不整合）→ `VectorError::DimensionMismatch`
- 非常に長いテキストや大量バッチ → CPU/メモリ負荷上昇

### EmbeddingGenerator::dimension（FastEmbedGenerator 実装）

1. 目的と責務
   - ジェネレータが返す埋め込み次元の取得。

2. アルゴリズム
   - 初期化時に検出した `VectorDimension` を返す。

3. 引数
- なし（self のみ）

4. 戻り値

| 型 | 説明 |
|----|------|
| VectorDimension | 事前検出済みの次元値 |

5. 使用例

```rust
use crate::vector::embedding::{FastEmbedGenerator, EmbeddingGenerator};

let gen = FastEmbedGenerator::new()?;
assert!(gen.dimension().get() > 0);
```

6. エッジケース
- 初期化に失敗した場合はそもそもインスタンス生成不可

### FastEmbedGenerator::with_model / new / new_with_progress / from_settings

1. 目的と責務
   - fastembed モデルを初期化し、モデル名と次元を確定。必要ならダウンロード。

2. アルゴリズム
   - `TextEmbedding::try_new(InitOptions::new(model).with_cache_dir(...).with_show_download_progress(...))`
   - 単一テキスト `"test"` の埋め込みを生成し、その長さから次元を検出。
   - `VectorDimension::new(dimension_size)` で型化。
   - フィールドを持つ `FastEmbedGenerator` を返却。

3. 引数（with_model / from_settings）

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| model | EmbeddingModel | Yes | 使用モデル（with_model） |
| model_name | &str | Yes | 設定からのモデル名（from_settings） |
| show_progress | bool | Yes | ダウンロード進捗表示 |

4. 戻り値

| 型 | 説明 |
|----|------|
| Result<FastEmbedGenerator, VectorError> | 初期化済みジェネレータ |

5. 使用例

```rust
use crate::vector::embedding::FastEmbedGenerator;
use fastembed::EmbeddingModel;

let gen = FastEmbedGenerator::with_model(EmbeddingModel::AllMiniLML6V2, true)?;
assert_eq!(gen.model_name(), "AllMiniLML6V2");

let gen2 = FastEmbedGenerator::from_settings("MultilingualE5Small", false)?;
assert_eq!(gen2.dimension().get(), gen2.dimension().get()); // 整合チェック
```

6. エッジケース
- ネットワーク断 → 初回ダウンロード失敗（EmbeddingFailed）
- キャッシュディレクトリの権限問題 → 初期化失敗（EmbeddingFailed）
- テスト埋め込みが空を返す（想定外）→ 現実装では `unwrap()` が panic の可能性（要修正）
- 次元値が不正（0など）→ `VectorError::EmbeddingFailed`（VectorDimension::new エラーをラップ）

### create_symbol_text

1. 目的と責務
   - シンボル名・種別・シグネチャを一つの自然言語テキストに整形。検索適合性向上。

2. アルゴリズム
   - `SymbolKind` を英単語へマップし、`format!("{kind} {name} {sig?}")` を生成。

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| name | &str | Yes | シンボル名 |
| kind | crate::types::SymbolKind | Yes | 種別 |
| signature | Option<&str> | No | シグネチャ（ある場合のみ付加） |

4. 戻り値

| 型 | 説明 |
|----|------|
| String | 埋め込み用テキスト |

5. 使用例

```rust
use crate::vector::embedding::create_symbol_text;
use crate::types::SymbolKind;

let t1 = create_symbol_text("parse_json", SymbolKind::Function, Some("fn parse_json(input: &str) -> Result<Value>"));
let t2 = create_symbol_text("Point", SymbolKind::Struct, None);
assert_eq!(t2, "struct Point");
```

6. エッジケース
- signature が None → 「kind name」のみ（余計な空白なし）
- 未知の SymbolKind → このチャンクには現れない（列挙は exhaustively マッチ）
- name/sig が極端に長い → 文字列サイズ増（性能影響）

## Walkthrough & Data Flow

- 初期化フロー（FastEmbedGenerator::with_model）
  - モデル選択 → fastembed 初期化 → テスト埋め込み生成 → 次元検出 → ジェネレータ構築
- 生成フロー（EmbeddingGenerator::generate_embeddings）
  - 入力検査（空）→ &str から String へ変換 → Mutex lock → fastembed の embed 呼び出し → 次元検証 → 返却

```mermaid
flowchart TD
  A[with_model(model, show_progress)] --> B{TextEmbedding::try_new}
  B -- Ok --> C[embed(vec!["test"])]
  B -- Err --> E[EmbeddingFailed: 初期化失敗]
  C -- Ok --> D{dimension_size = len(first embedding)}
  C -- Err --> F[EmbeddingFailed: 次元検出失敗]
  D -- invalid --> G[EmbeddingFailed: VectorDimension::new エラー]
  D -- valid --> H[Self { model(Mutex), dimension, model_name }]
```

上記の図は `FastEmbedGenerator::with_model` 関数の主要分岐を示す（行番号: 不明）。

```mermaid
flowchart TD
  A[generate_embeddings(&self, texts)] --> B{texts.is_empty()}
  B -- Yes --> Z[Ok(Vec::new())]
  B -- No --> C[Vec<String>に変換]
  C --> D{model.lock()}
  D -- Err --> E[EmbeddingFailed: lock poisoned]
  D -- Ok --> F[TextEmbedding::embed(texts)]
  F -- Err --> G[EmbeddingFailed: 生成失敗]
  F -- Ok --> H{各embeddingのlen==expected?}
  H -- No --> I[DimensionMismatch]
  H -- Yes --> J[Ok(Vec<Vec<f32>>)]
```

上記の図は `EmbeddingGenerator::generate_embeddings`（FastEmbedGenerator 実装）の主要分岐を示す（行番号: 不明）。

## Complexity & Performance

- 時間計算量
  - parse_embedding_model, model_to_string, dimension, model_name: O(1)
  - with_model: モデル初期化（I/O）＋1件の埋め込み生成 O(C)（Cはモデル計算コスト）
  - generate_embeddings: O(N·C)（N 件、各入力のトークン数にほぼ線形）
- 空間計算量
  - generate_embeddings: O(N·D)（N 件 × 次元 D の f32 ベクトル）
- ボトルネック
  - Mutex による直列化で並列呼び出し時のスループット低下
  - &str → String の一時アロケーション（大量バッチで GC/メモリ圧）
  - モデル初回ダウンロード（ネットワークI/O）
- スケール限界
  - 大量テキストの一括埋め込みではメモリ使用量増加（N·D·4バイト）
  - CPU/GPU加速の有無は fastembed 実装依存（このチャンクには現れない）

## Edge Cases, Bugs, and Security

- メモリ安全性
  - unsafe 不使用。所有権/借用は安全に管理。
  - 初期化時の `unwrap()` が空埋め込みの場合に panic の可能性（要修正）。
- インジェクション
  - SQL/Command/Path traversal 等の入力を外部コマンドへ渡す箇所なし。
- 認証・認可
  - 該当なし（このチャンクには現れない）。
- 秘密情報
  - ハードコード秘密なし。エラーメッセージに敏感情報なし。
- 並行性
  - `Mutex<TextEmbedding>` により同時呼び出しは排他される。lock 毒化時のエラー処理あり。
  - `EmbeddingGenerator: Send + Sync` 制約を満たすが、内部は直列化されるため高並行負荷時の性能劣化。

詳細エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空テキスト配列 | `[]` | `Ok([])` | generate_embeddings 冒頭で空返却 | OK |
| 未知モデル名 | `"UnknownModel123"` | `Err(EmbeddingFailed)` | parse_embedding_model の `_` アーム | OK |
| ネットワーク断（初回DL） | モデル初期化 | `Err(EmbeddingFailed)`（詳細メッセージ） | try_new の map_err | OK |
| テスト埋め込みが空 | `"test"` → `[]` | `Err(...)` or graceful | `unwrap()` により panic | 要改善 |
| 次元不一致 | fastembed が異なる長さを返す | `Err(DimensionMismatch)` | generate_embeddings でチェック | OK |
| lock 毒化 | Mutex が Poisoned | `Err(EmbeddingFailed)` | map_err で文言返却 | OK |
| 巨大テキスト群 | 何万件 | 成功だが遅延/メモリ増 | バッチ化未対応 | 注意 |
| create_symbol_text with None | `signature=None` | `"kind name"` | 条件分岐で空白なし | OK |

## Design & Architecture Suggestions

- エラー分類の明確化
  - 未知モデルは `EmbeddingFailed` よりも `InvalidModelName` のような専用バリアントが望ましい。
- 初期化次元検出の堅牢化
  - `embed(vec!["test"])` の結果が空でも panic しないよう、`first()` を安全に扱い、空なら `EmbeddingFailed("dimension detection returned empty")` を返す。
- 並行性能の向上
  - モデルが実際に thread-safe なら `Mutex` を外す、もしくは `RwLock` で read 多重化。
  - 1インスタンス＝1モデル共有より、スレッドごとにモデル複製（メモリとトレードオフ）。
- 入力のアロケーション削減
  - `Vec<String>` 変換の回避。APIを `&[impl AsRef<str>]` や `Cow<'a, str>` に変更して不要なコピーを減らす（fastembed 側API制約があるため要検討）。
- モデル次元の静的定義
  - 可能なら EmbeddingModel→次元の静的マッピングを持ち、テスト埋め込みを不要化。fastembed が将来仕様変更する場合は動的検出のフォールバック併用。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト
  - parse_embedding_model: 各既知モデル名で Ok、未知名で Err。
  - create_symbol_text: signature 有/無、各 `SymbolKind` マッピング。
  - FastEmbedGenerator::with_model: ネットワーク可用時に初期化成功、dimension>0。
  - generate_embeddings: 空入力、次元不一致（モックで再現）、lock 毒化（擬似的に Arc<Mutex> を使った失敗は困難だがエラー経路確認）。
- 並行テスト
  - 複数スレッドから `generate_embeddings` を呼び直列化されること（順序保証は難しいがクラッシュしないこと）を確認。

例1: parse_embedding_model のテスト

```rust
#[test]
fn test_parse_embedding_model_known_unknown() {
    use crate::vector::embedding::parse_embedding_model;
    assert!(parse_embedding_model("AllMiniLML6V2").is_ok());
    assert!(parse_embedding_model("MultilingualE5Small").is_ok());
    assert!(parse_embedding_model("DefinitelyNotAModel").is_err());
}
```

例2: 生成の並行呼び出し

```rust
use std::sync::Arc;
use std::thread;

#[test]
fn test_generate_embeddings_concurrent() {
    use crate::vector::embedding::{FastEmbedGenerator, EmbeddingGenerator};
    let gen = Arc::new(FastEmbedGenerator::new().unwrap());
    let texts = vec!["hello", "world", "rust", "fastembed"];

    let mut handles = Vec::new();
    for _ in 0..4 {
        let g = gen.clone();
        let t: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        handles.push(thread::spawn(move || {
            let res = g.generate_embeddings(&t);
            assert!(res.is_ok());
            let emb = res.unwrap();
            assert_eq!(emb.len(), t.len());
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}
```

例3: create_symbol_text の確認（既存テストに準じ）

```rust
#[test]
fn test_create_symbol_text_variants() {
    use crate::vector::embedding::create_symbol_text;
    use crate::types::SymbolKind;

    let t = create_symbol_text("Point", SymbolKind::Struct, None);
    assert_eq!(t, "struct Point");

    let t2 = create_symbol_text("parse_json", SymbolKind::Function, Some("fn parse_json(input: &str) -> Result<Value>"));
    assert!(t2.starts_with("function parse_json"));
}
```

注: fastembed 実行は CI 環境依存（ネットワーク・キャッシュ）。必要に応じて `#[ignore]` を付与。

## Refactoring Plan & Best Practices

- `with_model` 内の `unwrap()` を安全な `ok_or_else` に変更し、空埋め込みの場合の明示的エラーを返す。
- `generate_embeddings` 入力を `impl IntoIterator<Item=&'a str>` にし、内部で `String` へのコピーを必要最小化（fastembed が `Vec<String>` を要求する場合は変換箇所を分離）。
- エラー型 `VectorError` の粒度向上（InvalidModelName, InitFailed, DownloadFailed, PoisonedLock, DimensionMismatch 等）。
- バッチサイズ制御（上位からの大量入力に対し、適切なチャンク分割）をガイド（このチャンクには現れない）。
- `model_to_string` と `parse_embedding_model` のマッピング維持のためユニットテストを追加（全ペアの往復テスト）。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 初期化開始/成功/失敗（モデル名、キャッシュディレクトリ、進捗有無）
  - 埋め込み生成のバッチサイズ・経過時間・失敗詳細
- メトリクス
  - カウンタ: 生成要求数、失敗数、lock 取得失敗数
  - ヒストグラム: 埋め込み生成時間、バッチサイズ、1埋め込みあたりの時間
- トレーシング
  - `with_model` と `generate_embeddings` に span を設け、上位のリクエストと関連付け可能に
- 現状
  - このチャンクにはロギング/計測は現れない。導入は上位層と合わせて設計が望ましい。

## Risks & Unknowns

- fastembed の内部スレッドセーフ性や GPU/アクセラレーション状況はこのチャンクには現れない（不明）。現在は Mutex による直列化で安全側運用。
- `crate::init::models_dir()` の戻り値や権限・パスの取り扱いは不明。環境依存の失敗要因。
- `VectorError` と `VectorDimension` の詳細仕様（0次元許容／最大次元など）は不明。
- 大規模入力でのパフォーマンス特性（モデル・環境依存）は上位設計に依存。
- ドキュコメントにある SimpleIndexer/VectorSearchEngine 連携点は設計案であり、このチャンクには現れない（不明）。