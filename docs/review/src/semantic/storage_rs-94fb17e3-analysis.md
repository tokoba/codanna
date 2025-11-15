# semantic/storage.rs Review

## TL;DR

- 目的: メモリマップドファイルを用いた高速なセマンティック埋め込みの永続化ラッパーとして、MmapVectorStorage をセマンティック用途に特化した API で包む
- 主要公開API: new/open/open_or_create, save_embedding/save_batch, load_embedding/load_all, embedding_count/dimension/exists/file_size
- 複雑箇所: SymbolId ⇄ VectorId 変換と次元整合性チェック、open_or_create 時の次元一貫性の扱い、load_all の unwrap による潜在的パニック
- 重大リスク: new で既存 segment_0.vec を削除する設計によるデータ喪失リスク、シンボリックリンク経由の削除リスク、複数プロセス/スレッド同時アクセス時の競合、load_all の SymbolId 変換 unwrap のパニック
- パフォーマンス: 単一読み書き O(d)、バッチ O(n·d)、全件読み O(N·d)。mmap によりアクセスは高速だが I/O/ページフォールトの影響は残る
- 安全性: unsafe なし。エラーは SemanticSearchError にマップされるが、一部で Option→unwrap によるパニック可能性あり
- テストは基本機能をカバー（単体/永続化/バッチ/次元チェック）が、競合・破損ファイル・重複IDなどは未網羅

## Overview & Purpose

このファイルはセマンティック検索の埋め込みベクトルを永続化するためのバックエンドを提供する。内部的には既存の MmapVectorStorage（メモリマップドファイルベースの高効率ベクトル格納）を利用し、セマンティック検索でのユースケースに合わせたシンプルな API を公開する。具体的には、SymbolId（セマンティックな識別子）と VectorId（ストレージ内部識別子）の相互変換、埋め込みベクトル次元の検証、バッチ書き込み、全件読取などを提供する。

目的:
- セマンティック検索に必要な Embedding の高速アクセス（<1μs 目標）と永続化
- シンプルで型安全な API によるユースケース最適化
- ベクトル次元の整合性を強制し、破損データを防ぐ

このチャンク内での根拠箇所は関数名のみを併記（行番号は本チャンクに含まれないため「不明」）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | SemanticVectorStorage | pub | MmapVectorStorage のセマンティック用ラッパー。ID変換と次元検証、I/O操作を提供 | Low |
| Field | storage: MmapVectorStorage | private | 実ストレージ（mmapベース） | Low |
| Field | dimension: VectorDimension | private | 埋め込み次元の保持と検証に使用 | Low |
| Function | new | pub | セマンティック用セグメントの新規作成（既存ファイル削除） | Low |
| Function | open | pub | 既存ストレージを開く | Low |
| Function | open_or_create | pub | 既存がなければ作成 | Low |
| Function | save_embedding | pub | 単一埋め込みの保存（次元検証/ID変換） | Med |
| Function | load_embedding | pub | 単一埋め込みの読取 | Low |
| Function | load_all | pub | 全埋め込みの読取（ID再変換） | Med |
| Function | save_batch | pub | 複数埋め込みの一括保存（次元一括検証） | Med |
| Function | embedding_count | pub | ベクトル数の取得 | Low |
| Function | dimension | pub | 次元の取得 | Low |
| Function | exists | pub | ストレージ存在確認 | Low |
| Function | file_size | pub | ファイルサイズの取得 | Low |

### Dependencies & Interactions

- 内部依存
  - SemanticVectorStorage → MmapVectorStorage（全 I/O を委譲）
  - SemanticVectorStorage → VectorId / SegmentOrdinal / VectorDimension（ID変換・セグメント固定・次元保持）
  - SemanticVectorStorage → SymbolId（公開 API の ID）
  - SemanticVectorStorage → SemanticSearchError（エラー型）

- 外部依存（クレート/モジュール）

| 依存 | 目的 | 備考 |
|------|------|------|
| crate::vector::MmapVectorStorage | ベクトルの mmap 永続化 | 低レイテンシ・ファイル管理 |
| crate::vector::SegmentOrdinal | セグメント識別子 | ここでは 0 固定 |
| crate::vector::VectorDimension | 次元ラッパー | .get() で usize 取得 |
| crate::vector::VectorId | 内部 ID | SymbolId から変換 |
| crate::SymbolId | 外部公開 ID | new/to_u32 |
| crate::semantic::SemanticSearchError | エラー型 | StorageError/DimensionMismatch/InvalidId 等 |
| std::path::Path | パス操作 | join など |
| std::fs::remove_file | 既存ファイル削除 | new で使用 |

- 被依存推定（このモジュールを使う側）
  - セマンティック検索インデクサ
  - 埋め込み生成パイプライン（モデル推論→保存）
  - 検索実行時の前処理（クエリ埋め込みの読取）
  - メンテナンスユーティリティ（ダンプ、検査、再構築）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | fn new(path: &Path, dimension: VectorDimension) -> Result<Self, SemanticSearchError> | 新規作成（既存 segment_0.vec 削除） | O(1) + I/O | O(1) |
| open | fn open(path: &Path) -> Result<Self, SemanticSearchError> | 既存ストレージを開く | O(1) + I/O | O(1) |
| open_or_create | fn open_or_create(path: &Path, dimension: VectorDimension) -> Result<Self, SemanticSearchError> | 既存を開く/なければ作成 | O(1) + I/O | O(1) |
| save_embedding | fn save_embedding(&mut self, id: SymbolId, embedding: &[f32]) -> Result<(), SemanticSearchError> | 単一保存 | O(d) | O(1) |
| load_embedding | fn load_embedding(&mut self, id: SymbolId) -> Option<Vec<f32>> | 単一読取 | O(d) | O(d) |
| load_all | fn load_all(&mut self) -> Result<Vec<(SymbolId, Vec<f32>)>, SemanticSearchError> | 全件読取 | O(N·d) | O(N·d) |
| save_batch | fn save_batch(&mut self, embeddings: &[(SymbolId, Vec<f32>)]) -> Result<(), SemanticSearchError> | 一括保存 | O(n·d) | O(n) |
| embedding_count | fn embedding_count(&self) -> usize | ベクトル数取得 | O(1) | O(1) |
| dimension | fn dimension(&self) -> VectorDimension | 次元取得 | O(1) | O(1) |
| exists | fn exists(&self) -> bool | ファイル存在確認 | O(1) | O(1) |
| file_size | fn file_size(&self) -> Result<u64, SemanticSearchError> | ファイルサイズ取得 | O(1) + I/O | O(1) |

以下、各 API の詳細。

### new

1) 目的と責務
- セマンティック用セグメント（segment_0）を新規作成し、クリーンな状態を保証するために既存の segment_0.vec を削除する（関数: new、行番号: 不明）

2) アルゴリズム
- path.join("segment_0.vec") の存在を確認
- 存在すれば remove_file で削除
- MmapVectorStorage::new(path, SegmentOrdinal::new(0), dimension) を呼び出し
- Self { storage, dimension } を返す

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| path | &Path | ベースディレクトリ |
| dimension | VectorDimension | 埋め込み次元 |

4) 戻り値

| 型 | 説明 |
|----|------|
| Result<Self, SemanticSearchError> | 成功時に新規ストレージ。失敗時に StorageError |

5) 使用例
```rust
let dir = std::path::Path::new("/tmp/semantic");
let dim = VectorDimension::new(384).unwrap();
let storage = SemanticVectorStorage::new(dir, dim)?;
```

6) エッジケース
- 既存ファイル削除失敗（権限/ロック）
- ディレクトリが存在しない
- dimension がモデルと不一致（実際の保存時に検知）

### open

1) 目的と責務
- 既存のセマンティックストレージを開く（関数: open、行番号: 不明）

2) アルゴリズム
- MmapVectorStorage::open(path, SegmentOrdinal::new(0)) を呼ぶ
- storage.dimension() を読み取り Self に設定

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| path | &Path | 既存ストレージのベースパス |

4) 戻り値

| 型 | 説明 |
|----|------|
| Result<Self, SemanticSearchError> | 成功時にストレージ。失敗時に StorageError |

5) 使用例
```rust
let mut storage = SemanticVectorStorage::open(dir)?;
```

6) エッジケース
- ファイル未存在/破損
- 権限不足

### open_or_create

1) 目的と責務
- 既存を開き、なければ新規作成（関数: open_or_create、行番号: 不明）

2) アルゴリズム
- MmapVectorStorage::open_or_create(path, seg=0, dimension) を委譲
- Self { storage, dimension } を返す（dimension は引数の値をそのまま設定）

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| path | &Path | ベースパス |
| dimension | VectorDimension | 期待次元 |

4) 戻り値

| 型 | 説明 |
|----|------|
| Result<Self, SemanticSearchError> | 成功/失敗 |

5) 使用例
```rust
let mut storage = SemanticVectorStorage::open_or_create(dir, dim)?;
```

6) エッジケース
- 既存ストレージの次元と引数 dimension の不一致（MmapVectorStorage 側の仕様に依存。ここでは不明）

### save_embedding

1) 目的と責務
- 単一埋め込みを保存（関数: save_embedding、行番号: 不明）

2) アルゴリズム
- 埋め込み長と self.dimension を比較
- SymbolId → VectorId 変換（0 は無効）
- write_batch(&[(vector_id, embedding)]) で一括 API を利用

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| id | SymbolId | 論理 ID |
| embedding | &[f32] | 埋め込み（長さ = 次元） |

4) 戻り値

| 型 | 説明 |
|----|------|
| Result<(), SemanticSearchError> | 成功/失敗 |

5) 使用例
```rust
let id = SymbolId::new(42).unwrap();
let vec = vec![0.1_f32; dim.get()];
storage.save_embedding(id, &vec)?;
```

6) エッジケース
- 次元不一致で DimensionMismatch
- 0 ID で InvalidId
- ディスクいっぱい/権限不足で StorageError

### load_embedding

1) 目的と責務
- 単一埋め込みの読取（存在しなければ None）（関数: load_embedding、行番号: 不明）

2) アルゴリズム
- SymbolId → VectorId 変換（失敗時 None）
- storage.read_vector(vector_id) を返す

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| id | SymbolId | 対象 ID |

4) 戻り値

| 型 | 説明 |
|----|------|
| Option<Vec<f32>> | 見つかればベクトル、なければ None |

5) 使用例
```rust
if let Some(vec) = storage.load_embedding(id) {
    // 使用
}
```

6) エッジケース
- 無効 ID（0）→ None
- 物理破損時の挙動は MmapVectorStorage に依存（ここでは不明）

### load_all

1) 目的と責務
- 全埋め込みの読取（関数: load_all、行番号: 不明）

2) アルゴリズム
- storage.read_all_vectors() → Vec<(VectorId, Vec<f32>)>
- 各 VectorId を SymbolId::new(..).unwrap() で再生成
- Vec<(SymbolId, Vec<f32>)> に変換して返す

3) 引数
- なし

4) 戻り値

| 型 | 説明 |
|----|------|
| Result<Vec<(SymbolId, Vec<f32>)>, SemanticSearchError> | 全データ or エラー |

5) 使用例
```rust
let all = storage.load_all()?;
for (id, vec) in all {
    // 使用
}
```

6) エッジケース
- VectorId→SymbolId 変換で unwrap によりパニックの可能性（不正 ID が混入していた場合）
- 破損ファイルで StorageError

### save_batch

1) 目的と責務
- 複数埋め込みの一括保存（関数: save_batch、行番号: 不明）

2) アルゴリズム
- 事前に全要素の長さを検証（短絡的に最初でなく全件）
- SymbolId→VectorId を検証しつつ変換
- write_batch に (VectorId, &[f32]) のスライスで委譲

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| embeddings | &[(SymbolId, Vec<f32>)] | 保存対象のペア列 |

4) 戻り値

| 型 | 説明 |
|----|------|
| Result<(), SemanticSearchError> | 成功/失敗 |

5) 使用例
```rust
let batch = vec![
  (SymbolId::new(1).unwrap(), vec![1.0, 2.0]),
  (SymbolId::new(2).unwrap(), vec![3.0, 4.0]),
];
storage.save_batch(&batch)?;
```

6) エッジケース
- 1件でも次元不一致があれば即エラー
- 無効 ID（0）が混入で InvalidId
- 重複 ID の取り扱いは下層実装に依存（上書き？）→ 不明

### embedding_count

- 目的: 登録済みベクトル数（関数: embedding_count、行番号: 不明）
- 戻り値: usize
- 例:
```rust
assert_eq!(storage.embedding_count(), 3);
```

### dimension

- 目的: 次元取得（関数: dimension、行番号: 不明）
- 戻り値: VectorDimension

### exists

- 目的: バッキングファイルの存在確認（関数: exists、行番号: 不明）
- 戻り値: bool

### file_size

- 目的: バッキングファイルのサイズ取得（関数: file_size、行番号: 不明）
- 戻り値: Result<u64, SemanticSearchError>

## Walkthrough & Data Flow

- 保存フロー（save_embedding）
  - 入力: (SymbolId, &[f32])
  - 次元チェック: embedding.len() == self.dimension.get()
  - ID 変換: SymbolId::to_u32 → VectorId::new(u32)（0 は無効）
  - 下層へ: storage.write_batch(&[(VectorId, &[f32])])
  - 出力: Result<(), StorageError など）

- 一括保存フロー（save_batch）
  - 全件の次元を事前検証（O(n)）
  - 各要素の ID を VectorId に変換（失敗で InvalidId）
  - (VectorId, &[f32]) のバッファを構築
  - storage.write_batch に委譲

- 単一読取フロー（load_embedding）
  - SymbolId→VectorId 変換（失敗で None）
  - storage.read_vector(VectorId) の結果をそのまま返却（Option<Vec<f32>>）

- 全件読取フロー（load_all）
  - storage.read_all_vectors() を取得
  - VectorId→u32→SymbolId::new(..).unwrap() で再変換（不正 ID 混入時にパニックリスク）
  - (SymbolId, Vec<f32>) のベクタに詰め直して返却

データ境界・契約:
- VectorDimension は全埋め込みで一定であることを契約とする
- SymbolId はゼロ以外（VectorId::new が None を返すケースは InvalidId）
- セグメントは 0 固定（Semantic 用）

## Complexity & Performance

- save_embedding: 時間 O(d), 空間 O(1)
- save_batch: 時間 O(n·d), 空間 O(n)（(VectorId, &[f32]) の一時ベクタ）
- load_embedding: 時間 O(d), 空間 O(d)
- load_all: 時間 O(N·d), 空間 O(N·d)
- embedding_count/exits/dimension/file_size: O(1)

ボトルネック/スケール限界:
- 全件読取は N·d に比例してメモリ使用が増大（大規模コーパスでは一括読みを避けるべき）
- mmap はページフォールト/キャッシュミスの影響を受ける。ランダムアクセスは速いが、全件スキャン時は I/O 帯域が支配
- バッチ書き込みはシステムコールを減らすため有利

運用要因:
- ディスク空き容量とファイルシステムの制限
- ファイルロック/共有アクセス（複数プロセス）時の一貫性は未定義（下層次第）

## Edge Cases, Bugs, and Security

セキュリティ/堅牢性チェックリスト:
- メモリ安全性: unsafe 未使用。バッファ境界は Vec/スライスで安全
- インジェクション: SQL/Command/Path Traversal は直接なし。ただし new で任意パス配下のファイル削除を行うため、シンボリックリンク攻撃の懸念あり
- 認証・認可: 対象外
- 秘密情報: ハードコード秘密なし。ログ出力もなし（情報漏洩の経路は今のところなし）
- 並行性: マルチプロセス/スレッドの同時アクセス時の明確な同期はなし。レース/破損の可能性は下層に依存

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空/次元不一致の埋め込み | dim=3, vec.len=2 | DimensionMismatch エラー | save_embedding/save_batch で検証 | OK |
| 無効ID(0) | SymbolId(0) | InvalidId エラー/None | save_embedding: Err, load_embedding: None | OK |
| 既存ファイルの削除 | new() 呼び出し時に segment_0.vec が存在 | 削除される | remove_file 実行 | ⚠️ データ喪失リスク |
| 破損ファイル | read_all_vectors が失敗 | StorageError | load_all で map_err | OK |
| VectorId→SymbolId 変換失敗 | read_all で不正 ID 混入 | エラーで返す | unwrap により panic 可能 | ⚠️ 潜在バグ |
| 重複IDバッチ | [(id, vec), (同じid, vec2)] | 定義済みルールで上書き等 | 下層に委譲（不明） | 不明 |
| open_or_create 次元不一致 | 既存: d=384, 引数: d=768 | エラー or 既存優先 | 下層仕様に依存 | 不明 |
| シンボリックリンク経由削除 | path が悪意あるリンク | 任意ファイル削除回避 | バリデーションなし | ⚠️ セキュリティ懸念 |
| 多プロセス同時アクセス | 2 プロセスで new/open | 一貫性確保 | 同期なし | ⚠️ レース懸念 |

潜在バグ（根拠: 関数名のみ・行番号不明）:
- load_all 内の SymbolId::new(...).unwrap() は、ストレージ破損/他コンポーネント由来の不正 VectorId が混入するとパニック
- new が既存ファイルを消す設計は直感に反し、誤用でデータ喪失を招く

## Design & Architecture Suggestions

- データ喪失対策
  - new での自動削除をオプトインに（例: new_with_mode(path, dim, CreateMode::Fresh|FailIfExists|OpenExisting)）
  - もしくは new は「作成のみ」、既存があれば Err を返し、open_or_create を推奨

- 変換失敗の取り扱い
  - load_all の unwrap を安全に（map_err で InvalidId/CorruptedData を返す）
  - SymbolId/VectorId 変換を TryFrom/From 実装に統一

- API 整合性
  - load_embedding, load_all を &self にできるならそうする（下層が内的キャッシュ更新で &mut を要求していないなら緩和）
  - open_or_create の dimension フィールドは、既存を開いた場合 storage.dimension() を反映して整合性を保証

- セキュリティ/ファイル操作
  - 削除前に path がディレクトリであること、期待するレイアウトであること、かつシンボリックリンクでないことを検証
  - OS レベルのファイルロックやアトミック更新（temp ファイル→rename）を活用

- エラーメッセージ/観測性
  - tracing ログで操作単位の span とサイズ、ID 件数、所要時間を記録
  - メトリクス（保存件数、読み件数、エラー件数、I/O レイテンシ）を計測

## Testing Strategy (Unit/Integration) with Examples

追加で望まれるテスト:

- 破損・不正 ID の耐性
  - 準備: 下層 API をモック/テストダブル化し、read_all_vectors が (VectorId=0, vec) を返すよう偽装
  - 期待: load_all が panic せずエラーを返す（修正後）

- open_or_create 次元不一致
  - 準備: 既存 dim=3 を作成 → open_or_create(..., dim=4)
  - 期待: Err か、既存 dim が反映される（仕様を確定する）

- new の削除動作
  - 既存にデータを書き、new を呼んだ後に存在確認/読み込みが空になることを検証（現仕様の明示化）＋削除抑制モードのテスト（改善後）

- 重複 ID の挙動
  - save_batch で同一 ID を重複投入
  - 期待: 最後が勝つ等、結果を仕様化して検証（下層仕様に依存）

- ファイルロック/並行アクセス（統合テスト）
  - 2 つのインスタンスで並行書き込み/読取り
  - 期待: データ整合性が保たれるか、明示的にサポート外としてドキュメント

- exists/file_size の基本
  - 生成直後、保存後、削除後の戻り値検証

例（不正 ID テスト: 改修後を想定した擬似コード）
```rust
// 擬似: read_all_vectors が不正 VectorId を返す下層を注入できる場合
let mut storage = SemanticVectorStorage::open_or_create(tempdir.path(), dim).unwrap();
// 下層を差し替え or フックして (VectorId=0, vec) を返すようにする
let res = storage.load_all();
assert!(matches!(res, Err(SemanticSearchError::InvalidId { .. })));
```

## Refactoring Plan & Best Practices

- 破壊的 new の見直し
  - new_fresh と new_existing を分離 or モード引数を追加
  - ドキュメントでデータ削除の可能性を強調

- unwrap の排除
  - load_all: SymbolId::new(...) が None の場合は SemanticSearchError::InvalidId または CorruptedData を返却

- API の受け口統一
  - save_embedding は &[f32]、save_batch は Vec<f32> を要求。どちらも &[f32] に統一し、所有権移動を避けたい場合は Cow<[f32]> を検討
  - ID 変換は TryFrom 実装（SymbolId ↔ VectorId）

- 一貫性のある dimension 設定
  - open_or_create の戻り Self.dimension を storage.dimension() に合わせる

- 並行性を考慮
  - Send/Sync の要件を明示（必要なら Arc<Mutex<_>> ラッパーの提供）
  - 多プロセスアクセスについてドキュメント化

- エラー型の充実
  - CorruptedData（ID 不正/次元不一致）を追加し分類を明瞭に

## Observability (Logging, Metrics, Tracing)

- Logging（tracing）
  - span: "semantic_storage" 属性に path, segment=0, dimension
  - イベント: save_batch 件数 n、失敗理由 e、latency（histogram）
  - load_all の読み出し件数、所要時間、失敗時の提案

- Metrics
  - カウンタ: embeddings_saved_total, embeddings_loaded_total, storage_errors_total
  - ヒストグラム: write_batch_latency_seconds, read_all_latency_seconds, mmap_page_faults（OS 依存）

- Tracing
  - 上位リクエスト ID を引き継ぎ、ストレージ層でタグ付け（id, batch_size, file_size）

## Risks & Unknowns

- 下層実装依存
  - MmapVectorStorage の同時アクセス安全性、クラッシュ時の整合性保証（fsync/メタデータ原子性）→ 不明
  - open_or_create の次元不一致ハンドリング → 不明
  - write_batch の重複 ID の挙動 → 不明

- データ喪失
  - new で既存ファイルを削除する現仕様は誤用時のリスクが高い（削除前バックアップまたは確認が必要）

- 異常系のパニック
  - load_all の unwrap は破損ファイル時にプロセスを落とす可能性

- セキュリティ
  - シンボリックリンクや任意パス指定により意図しないファイルを削除するリスク（運用側でパス検証が必要）

以上の改善で、安全性・可観測性・運用性を高めつつ、現状の高性能な mmap ストレージの利点を維持できる。