# storage\metadata.rs Review

## TL;DR

- 目的: インデックスの状態（バージョン、データソース、シンボル数、ファイル数、最終更新時刻、索引対象ディレクトリ）をJSONで保存・復元するための軽量メタデータ層。
- 公開API: IndexMetadata(new/update_counts/update_indexed_paths/save/load/display_source) と DataSource(enum)。保存は index.meta にJSON書き出し、読み込みは存在しなければデフォルト新規。
- コアロジック: serde_jsonでのシリアライズ/デシリアライズと、標準FSによる読み書き。失敗は crate::IndexError に変換。
- 複雑箇所: 低い。主にI/OエラーとJSONパースエラーの取り扱い。indexed_pathsはOptionでスキップシリアライズされる点に注意。
- 重大リスク:
  - 競合: 複数プロセス/スレッドからの並行saveでレース、破損の可能性。原子的書き込み/ロック未実装。
  - 型: symbol_count/file_countがu32固定で将来オーバーフロー/桁あふれの懸念。DataSourceのdoc_countはu64で一貫性注意。
  - エラー粒度: serdeエラーがGeneralに集約され、エラー可観測性がやや低い（パス情報欠如）。
  - パス: indexed_pathsは「正規化済み」を期待するが関数内で正規化はしない（呼び出し側責務）。

## Overview & Purpose

このモジュールは、インデックスの状態を表すメタデータを保持・更新・永続化するための単純な層を提供します。主な用途は:

- 新規作成時や既存インデックス読み込み時の状態表現
- インデックスのデータソース（Tantivyからロード/新規作成）の記録
- シンボル数、ファイル数、最終更新時刻の追跡
- どのディレクトリをインデックスしたか（設定の変更検知用）を保持
- JSONファイル index.meta への保存・読み込み

このファイル単独で、インデックス本体の操作はせず、状態の記録と通知（display_sourceでの標準エラー出力）に焦点を当てています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | IndexMetadata | pub | インデックスのメタ情報（バージョン、データソース、カウント、タイムスタンプ、パス群）を保持 | Low |
| Enum | DataSource | pub | インデックスデータの由来（Tantivy由来 or 新規）を表す | Low |
| Impl Function | IndexMetadata::new | pub | デフォルトメタデータ生成 | Low |
| Impl Function | IndexMetadata::update_counts | pub | シンボル/ファイル数と更新時刻を更新 | Low |
| Impl Function | IndexMetadata::update_indexed_paths | pub | 索引用ディレクトリ群と更新時刻を更新 | Low |
| Impl Function | IndexMetadata::save | pub | JSONにシリアライズし index.meta へ書き出し | Low |
| Impl Function | IndexMetadata::load | pub | index.meta から読み込み、なければデフォルト | Low |
| Impl Function | IndexMetadata::display_source | pub | データソースとカウントをユーザーへ表示（stderr） | Low |

小要素:
- IndexMetadata: Debug, Clone, Serialize, Deserialize を派生。Default 実装あり（version=1、Fresh、カウント0、last_modified=現在、indexed_paths=None）。
- DataSource::Tantivy は path: PathBuf, doc_count: u64, timestamp: u64 を保持。Fresh は新規。

### Dependencies & Interactions

- 内部依存（関数/構造体間）
  - IndexMetadata::default/new → crate::indexing::get_utc_timestamp() を呼出して last_modified を設定
  - update_counts/update_indexed_paths → get_utc_timestamp() で last_modified 更新
  - save/load → serde_json と std::fs を使用して永続化/復元
  - display_source → self.data_source と self.symbol_count/file_count を参照し eprintln! で表示

- 外部依存（推測も含む）

| 依存 | 用途 | 備考 |
|------|------|------|
| crate::IndexResult | 返り値のResult型 | 具体定義は不明 |
| crate::IndexError | エラー変換 | FileRead/FileWrite/General を使用（他は不明） |
| crate::indexing::get_utc_timestamp | タイムスタンプ取得 | 秒/ミリ秒単位などは不明 |
| serde::{Serialize, Deserialize} | JSONシリアライズ/デシリアライズ | PathBufの扱いはserde実装に依存 |
| serde_json | JSON変換 | Pretty出力 |
| std::fs | ファイル読み書き | 原子的書き込みは未使用 |
| std::path::{Path, PathBuf} | パス表現 | 非UTF-8扱いは不明 |

- 被依存推定（このモジュールを使いそうな箇所）
  - インデックスの初期化・起動時処理（メタデータ読み込み）
  - インデックス更新処理（カウント/パス更新 → 保存）
  - Tantivyインデックス読み込み処理（DataSourceの設定、display_sourceによる通知）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| IndexMetadata::new | pub fn new() -> Self | デフォルトメタデータの生成 | O(1) | O(1) |
| IndexMetadata::update_counts | pub fn update_counts(&mut self, symbol_count: u32, file_count: u32) | カウントの更新と最終更新時刻の更新 | O(1) | O(1) |
| IndexMetadata::update_indexed_paths | pub fn update_indexed_paths(&mut self, paths: Vec<PathBuf>) | 索引対象パスの設定と最終更新時刻の更新 | O(1) | O(1) |
| IndexMetadata::save | pub fn save(&self, base_path: &Path) -> IndexResult<()> | JSON保存（base_path/index.meta） | O(n) | O(n) |
| IndexMetadata::load | pub fn load(base_path: &Path) -> IndexResult<Self> | JSON読み込み（なければデフォルト） | O(n) | O(n) |
| IndexMetadata::display_source | pub fn display_source(&self) | データソースとカウントの表示（stderr） | O(1) | O(1) |
| DataSource | pub enum DataSource { Tantivy{path: PathBuf, doc_count: u64, timestamp: u64}, Fresh } | データソースの状態表現 | - | - |
| IndexMetadata（型） | pub struct IndexMetadata { ... } | メタデータのデータ契約 | - | - |

注: nはJSONコンテンツ長（数百バイト程度を想定）。

詳細:

1) 目的と責務
- new: デフォルト状態（Fresh、カウント0、現在時刻）を生成
- update_counts: symbol_count/file_countを設定し、last_modifiedを最新化
- update_indexed_paths: 索引対象ディレクトリ群を設定（呼び出し側で正規化される前提）、last_modified更新
- save: 自身をJSONにprettyシリアライズして base_path/index.meta に書き出す
- load: base_path/index.meta を読みJSONから復元。存在しなければ new() を返す
- display_source: DataSourceに応じたメッセージと、シンボル数/ファイル数を eprintln! する

2) アルゴリズム（ステップ）
- save:
  - metadata_path = base_path.join("index.meta")
  - serde_json::to_string_pretty(self)
  - fs::write(metadata_path, json)
- load:
  - metadata_path = base_path.join("index.meta")
  - 存在チェック: なければ Ok(new())
  - fs::read_to_string(metadata_path)
  - serde_json::from_str(&json)

3) 引数

| 関数 | 引数 | 型 | 必須 | 説明 |
|------|------|----|------|------|
| update_counts | symbol_count | u32 | 必須 | シンボル数 |
| update_counts | file_count | u32 | 必須 | ファイル数 |
| update_indexed_paths | paths | Vec<PathBuf> | 必須 | 索引対象ディレクトリ群（呼び出し側で正規化済みを想定） |
| save | base_path | &Path | 必須 | index.meta を置くルートディレクトリ |
| load | base_path | &Path | 必須 | index.meta を探索するルートディレクトリ |

4) 戻り値

| 関数 | 戻り値 | 説明 |
|------|--------|------|
| new | Self | デフォルトメタデータ |
| update_counts | () | なし |
| update_indexed_paths | () | なし |
| save | IndexResult<()> | 保存成功/失敗 |
| load | IndexResult<Self> | 復元成功/失敗 |
| display_source | () | なし |

5) 使用例

```rust
use std::path::PathBuf;
use storage::metadata::{IndexMetadata, DataSource};

fn demo(base: &std::path::Path) -> crate::IndexResult<()> {
    // 新規作成
    let mut meta = IndexMetadata::new();
    // データソースを設定（例: Tantivyロード後）
    meta.data_source = DataSource::Tantivy {
        path: base.join("tantivy_index"),
        doc_count: 1234,
        timestamp: crate::indexing::get_utc_timestamp(), // 行番号不明
    };

    // カウント更新
    meta.update_counts(42, 10);

    // 索引対象パス（呼び出し側で正規化を推奨）
    let paths = vec![PathBuf::from("/repo/src"), PathBuf::from("/repo/lib")];
    meta.update_indexed_paths(paths);

    // 保存
    meta.save(base)?;

    // 復元
    let loaded = IndexMetadata::load(base)?;
    loaded.display_source();

    Ok(())
}
```

6) エッジケース
- index.meta が存在しない → load は新規デフォルトを返す
- 読み込み失敗（権限/ロック/パス不正）→ FileRead エラー
- JSONパース失敗（破損/不整合）→ General("Failed to parse metadata: ...")
- indexed_paths が未設定 → シリアライズ時に省略（skip_serializing_if）。デシリアライズ時は None
- パスの非UTF-8問題は serde_json の仕様に依存（このチャンクでは挙動不明）
- 大きすぎるカウント値（u32の上限超）を他処理が持っている場合、呼び出し側での変換に注意

データ契約（構造体/enum）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub version: u32,
    pub data_source: DataSource,
    pub symbol_count: u32,
    pub file_count: u32,
    pub last_modified: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed_paths: Option<Vec<PathBuf>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    Tantivy { path: PathBuf, doc_count: u64, timestamp: u64 },
    Fresh,
}
```

- version: スキーマバージョン（現在1）
- last_modified: get_utc_timestamp() の返すUTC時刻（単位は不明）
- indexed_paths: None時はシリアライズ省略、復元時は default

## Walkthrough & Data Flow

- 新規作成フロー
  1) IndexMetadata::new()（Default）で初期化（data_source=Fresh, counts=0, last_modified=現在）
  2) 必要に応じ DataSource を設定（例: Tantivyからロード後）
  3) update_counts / update_indexed_paths で更新・last_modified更新
  4) save(base) で base/index.meta に書き出し

- 読み込みフロー
  1) load(base) が base/index.meta の存在を判定
  2) なければ Ok(new()) を返す（Fresh）
  3) あれば read_to_string → serde_json::from_str で復元
  4) display_source でユーザー通知も可能

根拠: 上記はそれぞれ save / load / new / update_counts / update_indexed_paths / display_source の本文に一致（関数名:行番号不明）。

注: Mermaid図の使用基準（分岐4つ以上/状態3以上/アクター3以上）に達しないため図は省略。

## Complexity & Performance

- シリアライズ/デシリアライズ:
  - 時間: O(n)（n = JSON文字列長。フィールド数が少ないため小さい）
  - 空間: O(n)（saveでは文字列を一旦メモリへ構築）
- ファイルI/O:
  - fs::write/fs::read_to_string はファイルサイズに線形（ここでは数KB想定）
- ボトルネック:
  - 現状はほぼなし。ディスクI/Oが支配的であるがファイルサイズは小さい。
- スケール限界:
  - indexed_paths が非常に多い/長い場合、JSONサイズが増加し読み書きが遅延・メモリ使用増。
- 実運用負荷要因:
  - ネットワーク/DBは不使用。ストレージI/Oのみ。
  - 並行アクセス時のロック未実装によるリトライ/失敗が起こりうる。

最適化余地:
- saveで to_string_pretty → fs::write の二段階を、BufWriter + serde_json::to_writer_pretty でストリーミング化し一時メモリを削減。
- PrettyではなくコンパクトJSONにしてディスク/I/O量を削減（可読性とトレードオフ）。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性:
  - unsafe未使用。所有権/借用は標準的かつ安全（可変借用は &mut self のみ）。
  - 整数オーバーフロー: symbol_count/file_count が u32。呼び出し側でより大きな値を保持していると、変換時に問題の可能性（このモジュール内ではキャストなし）。
- インジェクション:
  - SQL/Command/Path traversal: base_path.join("index.meta") への書き出しのみ。base_path自体が攻撃者制御なら任意場所に作成されうるが、joinで固定ファイル名を付与しており traversal ではない。アプリ側のパス検証が必要。
- 認証・認可:
  - 機能なし。アプリケーション層での保護が必要。
- 秘密情報:
  - ハードコード秘密なし。eprintlnへの出力で機微情報漏えいの可能性は低いが、パス情報は表示されうる（DataSource::Tantivy.path）。
- 並行性:
  - レース条件: 複数プロセス/スレッドが同時に save すると、書き込み途中の破損/クラッシュに伴う部分書き込みの可能性。
  - デッドロック: 非該当。
  - ファイルロック/原子的リネーム未実装。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| index.meta 不在 | base_pathにファイル無し | デフォルトを返す（Fresh） | loadでexistsチェックしてnew() | 実装済 |
| 読み取り権限なし | パーミッション拒否 | FileReadエラー | fs::read_to_string→IndexError::FileRead | 実装済 |
| JSON破損 | 不正JSON文字列 | パースエラー戻し | serde_json::from_str→General | 実装済（エラー粒度粗い） |
| 書き込み権限なし | 不可ディレクトリ | FileWriteエラー | fs::write→IndexError::FileWrite | 実装済 |
| ディスク満杯 | 書き込み途中失敗 | FileWriteエラー | 同上 | 実装済 |
| 非UTF-8パス | PathBufに非UTF要素 | 保存/復元の可否 | serdeのPathBuf実装に依存 | 不明 |
| indexed_paths未設定 | None | シリアライズ省略 | #[serde(skip_serializing_if)] | 実装済 |
| カウントの桁あふれ | 2^32以上のシンボル数 | エラー/切り詰め | APIがu32のため受け取れない | 設計注意 |
| 並行書き込み | 2プロセスでsave | 原子的セーフティ | 未サポート（競合の可能性） | 要対応 |
| タイムスタンプ単位 | 秒/ミリ秒 | 一貫した比較 | get_utc_timestampの仕様に依存 | 不明 |

Rust特有の観点（本ファイルに関して）:
- 所有権/借用: update_indexed_paths(paths: Vec<PathBuf>) はムーブで受け取り Option に格納（再割当なし、O(1)）。可変借用は &mut self のスコープのみ。
- unsafe境界: なし。
- Send/Sync: 型はPathBufとプリミティブの集合で、基本的にSend可能。Syncは内部可変性なしのため問題になりにくい（明示境界は付与していない）。
- await境界/非同期: 非同期未使用。ブロッキングI/Oである点は設計上の把握が必要。
- エラー設計: IndexError::FileRead/FileWriteとGeneralへの集約。unwrap/expect 不使用。From/Intoによるエラー変換の実装はこのチャンクには現れない。

## Design & Architecture Suggestions

- 原子的書き込みとロック
  - save: 一時ファイル（index.meta.tmp）に書き出し、fsync後に原子的renameで置換。競合/破損耐性を高める。
  - マルチプロセス/スレッドを想定するなら、OSファイルロック（flock等）やadvisory lockを導入。
- エラー型の精緻化
  - serdeエラーを General ではなく MetadataSerialize/Deserialize 等の専用バリアントに。parse失敗時に metadata_path を含める。
  - エラーに version, data_source などの文脈を付加してデバッグ容易に。
- バージョニング/移行
  - CURRENT_VERSION 定数化。serdeで旧バージョンからの移行ロジック（custom deserializer）や upgrade() メソッドの導入。
- APIの明確化
  - set_data_source(...) ヘルパー、from_tantivy(path, doc_count, timestamp) コンストラクタで不変条件を明示。
  - update_indexed_paths 内での正規化（canonicalize）をオプションで提供、もしくは「常に呼び出し側で正規化する」契約をドキュメントに明記。
- I/Oとフォーマット
  - to_writer_pretty + BufWriter でメモリ使用を削減。必要なら非Prettyに切替可能に。
  - JSON Schema相当の検証（versionや必須フィールドのバリデーション）を追加。
- 表示とロギング
  - eprintln! ではなく log/tracing でレベル付き出力。ユーザー向け表示はUI層に委譲。

## Testing Strategy (Unit/Integration) with Examples

推奨ユニットテスト（tempfileを活用）:

1) 新規ロード（ファイル無し）
- 概要: 空ディレクトリで load → default
```rust
#[test]
fn load_returns_default_when_missing() -> crate::IndexResult<()> {
    let dir = tempfile::tempdir().unwrap();
    let meta = IndexMetadata::load(dir.path())?;
    assert_eq!(meta.version, 1);
    match meta.data_source { DataSource::Fresh => {}, _ => panic!("expected Fresh") }
    assert_eq!(meta.symbol_count, 0);
    assert_eq!(meta.file_count, 0);
    Ok(())
}
```

2) 保存→復元のラウンドトリップ
```rust
#[test]
fn save_and_load_roundtrip() -> crate::IndexResult<()> {
    let dir = tempfile::tempdir().unwrap();
    let mut meta = IndexMetadata::new();
    meta.data_source = DataSource::Tantivy {
        path: dir.path().join("idx"),
        doc_count: 7,
        timestamp: crate::indexing::get_utc_timestamp(), // 行番号不明
    };
    meta.update_counts(42, 3);
    meta.update_indexed_paths(vec![dir.path().join("src")]);
    meta.save(dir.path())?;

    let loaded = IndexMetadata::load(dir.path())?;
    assert_eq!(loaded.version, meta.version);
    assert_eq!(loaded.symbol_count, 42);
    assert_eq!(loaded.file_count, 3);
    match loaded.data_source {
        DataSource::Tantivy { doc_count, .. } => assert_eq!(doc_count, 7),
        _ => panic!("expected Tantivy"),
    }
    assert!(loaded.indexed_paths.as_ref().unwrap()[0].ends_with("src"));
    Ok(())
}
```

3) 破損JSONでのエラー
```rust
#[test]
fn load_fails_on_invalid_json() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.meta"), "{not-json}").unwrap();
    let err = IndexMetadata::load(dir.path()).unwrap_err();
    // 具体エラー型のアサートはIndexErrorの定義に依存（このチャンクには現れない）
}
```

4) indexed_paths シリアライズ省略の確認（None→省略、Some→保存）
```rust
#[test]
fn indexed_paths_skipped_when_none() -> crate::IndexResult<()> {
    let dir = tempfile::tempdir().unwrap();
    let meta = IndexMetadata::new(); // indexed_paths=None
    meta.save(dir.path())?;
    let s = std::fs::read_to_string(dir.path().join("index.meta"))?;
    assert!(!s.contains("indexed_paths"));
    Ok(())
}
```

5) 並行書き込み（ベストエフォート）
- 2スレッドでsaveを同時実行し破損が生じないかを観測。ただし現実には原子的書き込みが必要。現状は不安定テストとなるため、将来の修正後に導入推奨。

プロパティテスト（任意）:
- 任意のDataSource/カウント/パスを生成して serialize→deserialize の同値性検証（非UTF-8パスは環境依存で注意）。

## Refactoring Plan & Best Practices

- I/O安全性強化
  - serde_json::to_writer_pretty + tempfile → rename で原子的保存
  - 例外発生時に一時ファイルを確実に削除
- エラー型拡充
  - IndexError に MetadataSerialize/Deserialize を追加し path を保持
  - anyhowではなくthiserrorで人間可読メッセージとプログラム的識別を両立
- バージョン管理
  - const CURRENT_VERSION: u32 = 1; を導入。load時に古いversionなら migrate() 実行
- APIの明確化・利便性
  - fn set_data_source_tantivy(path: PathBuf, doc_count: u64, timestamp: u64)
  - fn touch(&mut self) で last_modified のみ更新
  - update_indexed_paths に canonicalize の有無を引数やFeatureで選択化
- 構造の堅牢化
  - indexed_paths が None と Some(vec![]) の意味の違いを整理（「未収集」と「空集合」の区別を明記）
  - versionに応じて必須/任意フィールドの制約を定義

## Observability (Logging, Metrics, Tracing)

- ロギング
  - eprintln! ではなく tracing::info!/warn!/error! を使用し、環境に応じて出力制御
  - save/load 成功/失敗、ファイルパス、バイト数、所要時間をログ
- メトリクス
  - 保存/読み込み回数、失敗回数、再試行回数、JSONサイズ（bytes）
- トレーシング
  - save/load を span で囲み、I/Oレイテンシ計測
- ユーザー通知
  - display_source の役割をロギングに移譲し、UI層での表示可否を制御

## Risks & Unknowns

- crate::IndexResult / IndexError / indexing::get_utc_timestamp の詳細は不明（このチャンクには現れない）。タイムスタンプの単位/エポック、エラー型の全バリアントは確認が必要。
- serde_json における PathBuf の非UTF-8取扱いは環境依存で不明。クロスプラットフォーム性の要求次第では独自エンコードが必要。
- DataSource::Tantivy.doc_count と IndexMetadata.symbol_count の意味的整合性（片方はドキュメント数、片方はシンボル数）に注意。ユーザーへの表示/解釈の混乱を避ける措置（ラベルやドキュメント強化）が必要。
- 複数プロセス/スレッドでの同時アクセス想定が不明。要件に応じてロック/原子的更新/リトライポリシーが必要。
- バージョンアップ時の移行戦略（versionフィールドの扱い、後方互換要件）が未定義。