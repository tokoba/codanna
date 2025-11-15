# semantic/metadata.rs Review

## TL;DR

- 目的: セマンティック検索の永続化に必要なメタデータ（モデル名・次元数・埋め込み数・作成/更新時刻・フォーマット版）をJSONとして保存/読み込みし、バージョン整合性を担保する。
- 公開API: Struct SemanticMetadata（pubフィールド）とメソッド new, update, save, load, exists（すべて pub）。
- コアロジック: loadでファイル読み取り→JSONデシリアライズ→バージョン整合性チェック（現在版より新しいとエラー）。saveでJSONシリアライズ→ファイル書き込み。
- 複雑箇所: I/Oのエラー変換とユーザ向けのsuggestionメッセージ付与、バージョン互換性判定（CURRENT_VERSIONより大の場合に拒否）。
- 重大リスク: 併行書き込みの競合・部分書き込みリスク（atomicではない）、入力検証不足（dimension=0やmodel_name空文字等）、古いバージョンの解釈/移行パスなし。
- Rust安全性: unsafeなし、所有権/借用は標準的で安全。エラーはResultで表現。テスト内でのみunwrap使用。
- 追加テスト/改善推奨: 破損JSON/権限エラー/ディレクトリ不存在/大きな数値のプラットフォーム依存性/atomic write等のケースを網羅。

## Overview & Purpose

このモジュールはセマンティック検索のインデックスに付随するメタデータを管理・永続化するための仕組みを提供する。具体的には以下を扱う。

- 埋め込みモデル名（model_name）
- 埋め込みベクトルの次元（dimension）
- 保存されている埋め込み数（embedding_count）
- 作成時刻（created_at）と更新時刻（updated_at）をUnix時刻で管理
- メタデータフォーマットのバージョン（version）を保持し、読み込み時に互換性チェック

用途は、保存/読み込みサイクル間でのメタデータ整合性の確保、およびバージョン進化時の安全性の担保である。

根拠:
- データ構造: SemanticMetadata（impl内、行番号: 不明）
- バージョン定数: CURRENT_VERSION = 1（impl内、行番号: 不明）
- 保存/読込: save, load（行番号: 不明）
- 時刻取得: crate::indexing::get_utc_timestamp（外部依存、行番号: 不明）
- エラー型: crate::semantic::SemanticSearchError（外部依存、行番号: 不明）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | SemanticMetadata | pub | メタデータモデル（名前・次元・件数・時刻・バージョン） | Low |
| Const | CURRENT_VERSION | private (impl内) | メタデータフォーマットの現在版 | Low |
| Method | new | pub | 現在時刻で初期化し、初期値を設定 | Low |
| Method | update | pub | 埋め込み件数と更新時刻の更新 | Low |
| Method | save | pub | JSONにシリアライズして metadata.json に保存 | Med |
| Method | load | pub | metadata.json を読み取りデシリアライズ、版互換性チェック | Med |
| Method | exists | pub | metadata.json の存在確認 | Low |
| Mod | tests | private | 単体テスト群（保存/読込/更新/存在/版互換性） | Low |

Dependencies & Interactions

- 内部依存
  - SemanticMetadata.new → get_utc_timestamp（作成/更新時刻の取得）
  - SemanticMetadata.update → get_utc_timestamp（更新時刻の取得）
  - SemanticMetadata.save → serde_json::to_string_pretty, std::fs::write
  - SemanticMetadata.load → std::fs::read_to_string, serde_json::from_str, バージョンチェック
- 外部依存（表）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | crate::indexing::get_utc_timestamp | 現在UTC時刻の取得 | u64 Unix time |
  | crate::semantic::SemanticSearchError | ストレージ関連エラー表現 | StorageError variantにマッピング |
  | serde/serde_json | シリアライズ/デシリアライズ | pretty-printで保存 |
  | std::fs, std::path::Path | ファイルI/O・パス操作 | metadata.json固定名 |
- 被依存推定
  - セマンティック検索インデックスの保存/読み込み機構
  - インデックスビルダー/ローダーがメタデータの検証/ロードに利用
  - 実際の利用箇所は本チャンクには現れない（不明）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| SemanticMetadata | struct SemanticMetadata { pub model_name: String, pub dimension: usize, pub embedding_count: usize, pub created_at: u64, pub updated_at: u64, pub version: u32 } | メタデータのデータ契約 | N/A | N/A |
| new | pub fn new(model_name: String, dimension: usize, embedding_count: usize) -> Self | 新規メタデータ作成 | O(1) | O(1) |
| update | pub fn update(&mut self, embedding_count: usize) | 件数と更新時刻の更新 | O(1) | O(1) |
| save | pub fn save(&self, path: &Path) -> Result<(), SemanticSearchError> | JSON保存 | O(n) n=JSON長 | O(n) |
| load | pub fn load(path: &Path) -> Result<Self, SemanticSearchError> | JSON読込と版チェック | O(n) n=ファイル長 | O(n) |
| exists | pub fn exists(path: &Path) -> bool | ファイル存在確認 | O(1) | O(1) |

各APIの詳細

1) SemanticMetadata（データ契約）
- 目的と責務
  - メタデータのスキーマ定義。フィールドはすべてpubで外部から読み取り可能。
- アルゴリズム
  - 該当なし（構造体定義）。
- 引数
  - 該当なし。
- 戻り値
  - 該当なし。
- 使用例
```rust
use semantic::metadata::SemanticMetadata;
let meta = SemanticMetadata {
    model_name: "AllMiniLML6V2".to_string(),
    dimension: 384,
    embedding_count: 1000,
    created_at: 1_735_689_600,
    updated_at: 1_735_689_600,
    version: 1,
};
```
- エッジケース
  - フィールド値の検証は行われない（dimension=0、model_name空等は通る）。

2) new
- 目的と責務
  - 現在版（CURRENT_VERSION=1）と現在UTC時刻で初期化してメタデータを作成。
- アルゴリズム（ステップ）
  - get_utc_timestampで時刻取得
  - 各フィールドに引数と時刻を設定
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | model_name | String | 埋め込みモデル名 |
  | dimension | usize | 埋め込み次元 |
  | embedding_count | usize | 初期埋め込み件数 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Self | 初期化済みメタデータ |
- 使用例
```rust
let metadata = SemanticMetadata::new("AllMiniLML6V2".into(), 384, 1000);
```
- エッジケース
  - model_nameが空文字でも受理される。
  - dimension=0でも受理される。
  - embedding_countが非常に大きい場合でも受理される（usizeに収まる範囲）。

3) update
- 目的と責務
  - 埋め込み件数を更新し、updated_atを現在時刻に更新。
- アルゴリズム（ステップ）
  - self.embedding_countに新件数を代入
  - get_utc_timestampで現在時刻を取得しself.updated_atに設定
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | embedding_count | usize | 新しい埋め込み件数 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例
```rust
let mut metadata = SemanticMetadata::new("TestModel".into(), 128, 100);
metadata.update(200);
```
- エッジケース
  - 値が減る更新も許容（整合性チェックなし）。
  - 非単調なupdated_at時刻（UTCソースの非単調性がある可能性）は理論上あり得るが通常想定外。

4) save
- 目的と責務
  - metadata.jsonに対してpretty JSONで保存。I/OとシリアライズエラーをSemanticSearchError::StorageErrorへ変換。
- アルゴリズム（ステップ）
  - path.join("metadata.json")で保存先パスを形成
  - serde_json::to_string_pretty(self)でJSON化
  - std::fs::writeでファイル書き込み
  - いずれか失敗時はStorageError（messageとsuggestion付き）を返す
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | path | &Path | ディレクトリパス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Result<(), SemanticSearchError> | 成否（StorageErrorにラップ） |
- 使用例
```rust
use std::path::Path;
let dir = Path::new("/tmp/semantic");
metadata.save(dir)?;
```
- エッジケース
  - ディレクトリが存在しない/権限不足→write失敗。
  - シリアライズ不可能（理論上本構造はSerialize導出済みなので通常発生しない）。
  - 併行書き込み時の競合（atomicではない）。

- 参照コード抜粋（<=20行のため全体引用）
```rust
pub fn save(&self, path: &Path) -> Result<(), SemanticSearchError> {
    let metadata_path = path.join("metadata.json");

    let json =
        serde_json::to_string_pretty(self).map_err(|e| SemanticSearchError::StorageError {
            message: format!("Failed to serialize metadata: {e}"),
            suggestion: "This is likely a bug in the code".to_string(),
        })?;

    std::fs::write(&metadata_path, json).map_err(|e| SemanticSearchError::StorageError {
        message: format!("Failed to write metadata: {e}"),
        suggestion: "Check disk space and file permissions".to_string(),
    })?;

    Ok(())
}
```

5) load
- 目的と責務
  - metadata.jsonを読み取りデシリアライズし、バージョンがCURRENT_VERSIONより新しい場合は互換性エラーにする。
- アルゴリズム（ステップ）
  - path.join("metadata.json")で読み込みパスを形成
  - std::fs::read_to_stringで文字列取得
  - serde_json::from_strでSemanticMetadataへ変換
  - version > CURRENT_VERSIONならStorageErrorを返す
  - それ以外はOk(metadata)
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | path | &Path | ディレクトリパス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Result<SemanticMetadata, SemanticSearchError> | 成否（StorageErrorにラップ） |
- 使用例
```rust
let loaded = SemanticMetadata::load(dir)?;
assert_eq!(loaded.version, 1);
```
- エッジケース
  - ファイル不存在→read_to_string失敗→StorageError。
  - JSON破損→from_str失敗→StorageError。
  - versionがCURRENT_VERSIONより新しい→StorageError（更新を促す）。
  - versionが古い→許容（移行処理は本ファイルにはない）。

- 参照コード抜粋（長いので要点のみ）
```rust
pub fn load(path: &Path) -> Result<Self, SemanticSearchError> {
    let metadata_path = path.join("metadata.json");
    let json = std::fs::read_to_string(&metadata_path).map_err(/* ... */)?;
    let metadata: Self = serde_json::from_str(&json).map_err(/* ... */)?;
    if metadata.version > Self::CURRENT_VERSION {
        return Err(SemanticSearchError::StorageError {
            message: format!(
                "Metadata version {} is newer than supported version {}",
                metadata.version,
                Self::CURRENT_VERSION
            ),
            suggestion: "Update the code to support the newer metadata format".to_string(),
        });
    }
    Ok(metadata)
}
```

6) exists
- 目的と責務
  - path配下にmetadata.jsonが存在するかを簡易チェック。
- アルゴリズム（ステップ）
  - path.join("metadata.json").exists()
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | path | &Path | ディレクトリパス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | bool | 存在する場合true |
- 使用例
```rust
if !SemanticMetadata::exists(dir) {
    // 初回セットアップ等
}
```
- 参照コード（<=20行）
```rust
pub fn exists(path: &Path) -> bool {
    path.join("metadata.json").exists()
}
```
- エッジケース
  - パスがディレクトリでない/シンボリックリンク等→existsはファイル存在判定のみで詳細は不問。

## Walkthrough & Data Flow

- 典型的フロー（保存）
  1. SemanticMetadata::newで初期作成（created_at/updated_atは同時刻、version=1）
  2. save(path)でJSONにシリアライズし、path/metadata.jsonへ書き出し
- 典型的フロー（読み込み）
  1. exists(path)で存在確認（任意）
  2. load(path)で読み込み
     - read_to_stringでファイル全文取得
     - from_strでデシリアライズ
     - version > CURRENT_VERSIONならStorageError
     - それ以外はメタデータを返す
- 更新フロー
  1. update(embedding_count)で件数とupdated_atのみ更新（created_atは不変）
  2. save(path)で再保存

このモジュールは自身でメタデータの整合性（モデル名/次元など）検証は行わず、単純なI/Oとフォーマット版チェックのみを担当する。

## Complexity & Performance

- new/update/existsはO(1)時間・O(1)空間。
- saveはO(n)時間/空間（n=JSON文字列長、実質小さい）。ディスクI/Oが支配的。
- loadはO(n)時間/空間（n=ファイル長）。読み取りとパースが支配的。
- ボトルネック
  - ファイルシステムI/O（SSD/HDD、権限、ネットワークFSの場合の遅延）。
- スケール限界
  - メタデータは小規模の固定サイズだが、ネットワークファイルシステムや高頻度更新ではI/Oボトルネック・競合が発生しうる。
- 実運用負荷要因
  - 高頻度のsave呼び出し（毎イベントで更新する設計は非推奨）
  - 併行プロセス/スレッドからの同時save/load

## Edge Cases, Bugs, and Security

セキュリティチェックリスト

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全性によりunsafeなし。整数はusize/u64/u32で、JSON→usizeへのデコード時にプラットフォーム依存のオーバーフローはserdeがエラー化する可能性あり（大きすぎる数値は失敗）。明示的ガードはなし。
- インジェクション
  - SQL/Command: 該当なし。
  - Path traversal: path.join("metadata.json")固定のため、ユーザ入力に依存するのはディレクトリpathのみ。joinの仕様上、固定ファイル名を使用するため traversalリスクは低い。
- 認証・認可
  - 権限チェック漏れ: OS依存。save時に権限不足エラーを返すのみ。明示的認可はなし。
  - セッション固定: 該当なし。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: ログ機能なし。エラーメッセージにはファイルパスやOSエラー文字列が含まれる可能性があるが、現状はResultに格納されるのみ。
- 並行性
  - Race condition: 複数スレッド/プロセスが同一metadata.jsonを同時に書くと競合。部分書き込み・破損の可能性。ロック・atomic書き込み未実装。
  - Deadlock: 該当なし。

Rust特有の観点

- 所有権
  - newでmodel_name: Stringは移動（new: 行番号不明）。
  - updateは&mut selfで内部更新（update: 行番号不明）。
  - save/loadは&self / -> Selfの所有権モデルで安全。
- 借用
  - &Path引数は読み取りのみ。可変借用なし。
- ライフタイム
  - 明示的ライフタイムは不要。返却値は所有型。
- unsafe境界
  - unsafeブロックは存在しない（本チャンクには現れない）。
- 並行性・非同期
  - 非同期なし。Send/Syncはメンバが標準型のため自動導出される（推測）。共有状態保護はなし（必要に応じて外部で同期化）。
- エラー設計
  - ResultでI/O/serdeエラーをSemanticSearchError::StorageErrorに変換。
  - panicはテスト内のunwrapのみ。ライブラリコード内でunwrap/expectは不使用。
  - エラー変換はmap_errで都度行っており、From/Intoの実装はない。

詳細エッジケース表

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ファイル不存在 | path=空ディレクトリ | StorageError (read失敗) | load内でmap_err | Covered |
| ディレクトリ不存在 | path=不在のパス | StorageError (write失敗) | save内でmap_err | Covered |
| 権限不足 | path=読み書き不可 | StorageError | save/loadでmap_err | Covered |
| 破損JSON | `"{ bad }"` | StorageError (parse失敗) | load内でmap_err | Covered |
| 将来版 | version=999 | StorageError（新しすぎる） | loadで版チェック | Covered |
| 古い版 | version=0 | 許容（読み込みOK） | loadでチェックなし（>のみ） | Allowed |
| dimension=0 | new(..., 0, ...) | 仕様次第だが現状は許容 | バリデーションなし | Not-Validated |
| model_name空 | new("", ...) | 許容 | バリデーションなし | Not-Validated |
| 極端な数値 | embedding_count=usize::MAX | 理論上許容、I/Oは正常 | バリデーションなし | Not-Validated |
| 併行書き込み | 複数save | 破損/競合の可能性 | ロック/atomicなし | Risk |

## Design & Architecture Suggestions

- 版管理強化
  - versionをメジャー/マイナーに分割し、マイナー前方互換を許可、メジャー差異で拒否などのポリシーを明確化。
  - 古い版からのマイグレーション関数（upgradeメソッド）を用意。
- 入力検証
  - new/update時にdimension > 0、model_name非空などのバリデーション追加。
  - embedding_countは非負・単調増加の保証が必要ならロジック追加。
- 併行性/耐障害性
  - atomic write: 一時ファイルに書き出し→fsync→renameで原子的更新。
  - ファイルロック（advisory lock）やプロセス間同期の導入。
- エラー設計/可観測性
  - エラーバリアントの詳細化（SerializeError, IoError, VersionError）で診断性向上。
  - ログ/トレース（tracingクレート）でpath・サイズ・所要時間を記録。
- API改善
  - save/load/existsのpath引数を impl AsRef<Path> にすることで使い勝手向上。
  - metadataファイル名の定数化と外部設定可能性（e.g., save_with_name）。
- 将来拡張
  - メタデータにモデルハッシュ・語彙/正規化設定などの互換性チェック項目追加。
  - 署名/チェックサム（e.g., SHA256）で破損検出強化。

## Testing Strategy (Unit/Integration) with Examples

既存テスト（本ファイルにあり）
- 保存/読込のラウンドトリップ
- updateで件数とupdated_atが更新される
- existsの挙動確認
- 版互換性（将来版でエラー）

追加推奨テスト
- 破損JSON
```rust
#[test]
fn load_corrupted_json() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    std::fs::write(temp_dir.path().join("metadata.json"), "{ bad }").unwrap();
    let err = SemanticMetadata::load(temp_dir.path()).unwrap_err();
    match err {
        SemanticSearchError::StorageError { .. } => {}
        _ => panic!("Expected StorageError"),
    }
}
```
- ディレクトリ不存在/権限エラー
```rust
#[test]
fn save_to_nonexistent_dir() {
    // あり得るが、通常TempDirは存在する。ここでは削除後に試す。
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();
    drop(temp_dir); // 破棄してディレクトリを消す
    let meta = SemanticMetadata::new("Test".into(), 10, 0);
    assert!(meta.save(&path).is_err());
}
```
- 極端な値
```rust
#[test]
fn extreme_values() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let meta = SemanticMetadata::new("X".into(), usize::MAX, usize::MAX);
    // 保存・読込が通るか（32bitでは失敗する可能性あるため、その場合はエラーでも良い）
    let _ = meta.save(temp_dir.path());
    let _ = SemanticMetadata::load(temp_dir.path());
}
```
- atomic writeの導入後（提案）には、同時書き込み耐性の統合テストを追加。

## Refactoring Plan & Best Practices

- ステップ1（安全な書き込み）
  - saveをatomic化（tmpファイル→書き込み→fsync→rename）。失敗時ロールバック。
- ステップ2（バリデーション）
  - new/updateに検証を追加（dimension>=1、model_name非空など）。失敗時は新しいErrorバリアント。
- ステップ3（エラー型の整備）
  - SemanticSearchErrorにVersionError, IoError, SerdeErrorなどを追加し、精密化。
- ステップ4（可観測性）
  - tracingでinfo/debugイベント（path, size, duration, version）を記録。
- ステップ5（API拡張）
  - AsRef<Path>対応、ファイル名の定数化/設定化、upgrade関数の導入。
- ステップ6（ドキュメント）
  - バージョン互換性ポリシー・エラー契約・I/O特性をドキュメント化。

## Observability (Logging, Metrics, Tracing)

- ログ（tracing）
  - save開始/終了（path, bytes, duration）
  - load開始/終了（path, bytes, duration, version）
  - エラー内容（原因・OSエラーコード）
- メトリクス
  - 保存/読込のカウンタ（成功/失敗）
  - 保存/読込のヒストグラム（レイテンシ）
- トレーシング
  - save/loadをspanで囲い、I/Oレイヤの詳細を可視化（特にネットワークFS上の遅延分析に有効）。
- ログのPII配慮
  - pathのフル表示を環境変数で制御可能にする。

## Risks & Unknowns

- get_utc_timestampの実装詳細（単調性/精度/タイムゾーン）: このチャンクには現れないため不明。非単調な時間が出ると更新時刻の比較ロジックに影響する可能性あり。
- SemanticSearchErrorのバリアント詳細: このチャンクには現れないため不明。StorageErrorの構造は使用方法から推測（message, suggestionフィールド）。
- 古いversionの扱い: 現状は読み込み許容だが、実際の互換性保証や移行の所在は不明（このチャンクには現れない）。
- Send/Syncの明示境界: 構造体メンバが標準型のみのため通常はSend/Syncだが、実際の利用環境での併行アクセス戦略は上位レイヤに依存。
- 利用箇所・整合性検証の責務分担: 他モジュールがmodel_name/dimensionの検証を担っている可能性があるが本チャンクには現れない。