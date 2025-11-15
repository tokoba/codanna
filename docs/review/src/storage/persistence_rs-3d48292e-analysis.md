# storage\persistence.rs Review

## TL;DR

- 目的: **Tantivy専用の永続化**を担い、メタデータ管理・インデックス存在確認・セマンティック検索データの保存/ロードを行う
- 主要公開API: **new**, **save**, **load**, **load_with_settings**, **load_with_settings_lazy**, **exists**, **clear**
- 複雑箇所: **load_with_settings_lazy**（条件分岐多数、メタデータ・Tantivy・セマンティックデータの統合）、**clear**（Windowsロック対策のリトライ）
- Rust安全性: **unsafe未使用**、所有権/借用は妥当、**Arc<Settings>**の移動前にdebug抽出する所有権配慮あり
- エラー/並行性: I/O中心で**同期・ブロッキング**、**must_use属性**により結果の無視を防止、Windows権限/ロックを考慮した削除リトライあり
- 重大リスク: セマンティック検索の保存/ロードが**オプション扱い**でエラーを握りつぶす仕様、**update_project_registry**の失敗が**標準エラー出力のみ**でサイレントになる点
- パフォーマンス: パス復元やファイル削除は**O(n)**（パス数・ファイル数）でスケール、Tantivy/セマンティック処理のコストは*不明*（外部依存）

## Overview & Purpose

このモジュールは、プロジェクトのインデックスを**Tantivy**にのみ保存するシンプルな永続化層です。役割は以下の通りです。
- インデックスの**メタデータ管理**（記号数・ファイル数・データソース・インデックス済みパス）
- Tantivyインデックス（`tantivy/meta.json`）の**存在確認**と**ロード**
- セマンティック検索データ（`semantic/`以下）の**保存/ロード**（オプション）
- プロジェクトレジストリ（`~/.codanna`配下を前提）への**メタデータ更新**（権限によりスキップあり）

本ファイルは、プロジェクト全体の**インデックス保存と復元の入口**であり、実データの索引・検索は`SimpleIndexer`が担当します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | IndexPersistence | pub | ベースパス管理とインデックスの永続化/復元 | Med |
| Fn | new | pub | 永続化マネージャの生成 | Low |
| Fn | save | pub | メタデータ更新、Tantivy/セマンティック保存、レジストリ更新 | Med |
| Fn | load | pub | デフォルト設定でのロード | Low |
| Fn | load_with_settings | pub | 設定指定でのロード | Low |
| Fn | load_with_settings_lazy | pub | 設定+lazy指定でのロード、統合処理の中心 | High |
| Fn | exists | pub | Tantivyインデックスの存在確認 | Low |
| Fn | clear | pub | Tantivyインデックス削除（リトライあり） | Med |
| Fn | semantic_path | private | セマンティック検索ディレクトリパス生成 | Low |
| Fn | update_project_registry | private | プロジェクトレジストリへのメタデータ反映 | Med |

### Dependencies & Interactions

- 内部依存（呼び出し関係）
  - `save` → `semantic_path`, `update_project_registry`, `IndexMetadata::{load,new,update_counts,update_indexed_paths,save}`, `SimpleIndexer::{symbol_count,file_count,get_indexed_paths,settings,document_count,has_semantic_search,save_semantic_search}`, `crate::indexing::get_utc_timestamp`
  - `load_with_settings_lazy` → `semantic_path`, `IndexMetadata::load`, `SimpleIndexer::{with_settings,with_settings_lazy,symbol_count,file_count,load_semantic_search,add_indexed_path}`, `DataSource`
  - `load` → `load_with_settings`
  - `load_with_settings` → `load_with_settings_lazy`
  - `exists` → ファイル存在チェック
  - `clear` → ディレクトリ削除/作成、リトライ
  - `update_project_registry` → `crate::init::{local_dir_name,ProjectRegistry::load,save,find_project_by_id_mut}`

- 外部依存（クレート/モジュール）
  | 依存 | 用途 |
  |------|------|
  | crate::storage::{DataSource, IndexMetadata} | メタデータ構造とデータソース識別 |
  | crate::{IndexError, IndexResult, Settings, SimpleIndexer} | エラー型・設定・インデクサ |
  | crate::indexing::get_utc_timestamp | タイムスタンプ生成 |
  | crate::init::{local_dir_name, ProjectRegistry} | プロジェクトレジストリI/O |
  | std::{fs, path::PathBuf, sync::Arc, thread, time, hint::black_box} | ファイル/同期ユーティリティ |
  | tempfile::TempDir（テスト） | テンポラリディレクトリ |

- 被依存推定
  - CLI/サービス層から**インデックスの保存・復元**を行う際に本モジュールを使用
  - 検索機能起動時の**初期化フェーズ**で`exists`→`load...`の組み合わせ
  - インデックス再作成・クリア機能で`clear`使用

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | `pub fn new(base_path: PathBuf) -> Self` | 永続化管理の初期化 | O(1) | O(1) |
| save | `pub fn save(&self, indexer: &SimpleIndexer) -> IndexResult<()>` | メタデータ/Tantivy/セマンティック保存、レジストリ更新 | O(p + m + s) | O(p) |
| load | `pub fn load(&self) -> IndexResult<SimpleIndexer>` | デフォルト設定でロード | O(1) + 外部不明 | O(1) |
| load_with_settings | `pub fn load_with_settings(&self, settings: Arc<Settings>, info: bool) -> IndexResult<SimpleIndexer>` | 設定指定でロード | O(1) + 外部不明 | O(1) |
| load_with_settings_lazy | `pub fn load_with_settings_lazy(&self, settings: Arc<Settings>, info: bool, skip_trait_resolver: bool) -> IndexResult<SimpleIndexer>` | 詳細ロード（Tantivy/セマンティック/パス復元） | O(p + s) | O(p) |
| exists | `pub fn exists(&self) -> bool` | Tantivyインデックス存在確認 | O(1) | O(1) |
| clear | `pub fn clear(&self) -> Result<(), std::io::Error>` | インデックス削除と再作成（リトライ） | O(f) | O(1) |

注:
- p: indexed_pathsの数
- m: メタデータ保存コスト（ファイルI/O）
- s: セマンティック保存/ロードのコスト（外部の実装に依存し*不明*）
- f: tantivyディレクトリ内ファイル数に線形

### 各APIの詳細

#### new

1. 目的と責務
   - ベースパス（インデックス永続化の基準ディレクトリ）を受け取り、`IndexPersistence`を構築。

2. アルゴリズム
   - 受け取った`PathBuf`を内包して`Self`を返す。

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | base_path | PathBuf | インデックス保存の基準ディレクトリ |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | IndexPersistence | 永続化マネージャ |

5. 使用例
   ```rust
   use std::path::PathBuf;
   let persistence = IndexPersistence::new(PathBuf::from("/path/to/index"));
   ```

6. エッジケース
   - 不正なパス: 生成時には検証しない（I/Oは他APIで実行）

#### save

1. 目的と責務
   - メタデータ（記号数・ファイル数・インデックス済みパス・データソース）を更新し保存
   - オプションでセマンティック検索データを保存
   - プロジェクトレジストリの最新化（失敗してもインデックス保存は成功させる）

2. アルゴリズム（関数: save、行番号: 不明）
   - `IndexMetadata::load(base_path)`に失敗したら`IndexMetadata::new()`で初期化
   - `update_counts(symbol_count, file_count)`を更新
   - `get_indexed_paths`でインデックス済みパスを収集し、`update_indexed_paths`へ反映（debug時はログ表示）
   - `data_source`を`DataSource::Tantivy{ path: base/tantivy, doc_count, timestamp }`で更新
   - `metadata.save(base_path)`書き出し
   - `update_project_registry(&metadata)`を試行（失敗は`eprintln!`のみ）
   - `has_semantic_search`なら`semantic_path`を作成して`save_semantic_search`を呼ぶ（失敗は`IndexError::General`）

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | &self | &IndexPersistence | マネージャ |
   | indexer | &SimpleIndexer | インデクサ（メタデータ・セマンティック取り出し元） |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | IndexResult<()> | 成功/失敗（I/O等のエラー時） |

5. 使用例
   ```rust
   let persistence = IndexPersistence::new(settings.index_path.clone());
   persistence.save(&indexer)?;
   ```

6. エッジケース
   - メタデータの読み込み失敗 → 新規メタデータ生成
   - レジストリ更新失敗 → 標準エラー出力に警告、処理継続
   - セマンティック保存失敗 → 例外化（IndexError::General）
   - `document_count()`がErr → `unwrap_or(0)`で0にフォールバック

#### load

1. 目的と責務
   - デフォルト設定でインデクサをロード（本質は`load_with_settings`委譲）

2. アルゴリズム（関数: load、行番号: 不明）
   - `Settings::default()`を`Arc`に包み、`info=false`で`load_with_settings`に委譲

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | &self | &IndexPersistence | マネージャ |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | IndexResult<SimpleIndexer> | ロード結果 |

5. 使用例
   ```rust
   let indexer = persistence.load()?;
   ```

6. エッジケース
   - Tantivyメタが無い場合はNotFoundエラー（`IndexError::FileRead`）

#### load_with_settings

1. 目的と責務
   - 設定とinfoフラグ指定でロード。lazy有無は`false`固定。`load_with_settings_lazy`へ委譲。

2. アルゴリズム（関数: load_with_settings、行番号: 不明）
   - `load_with_settings_lazy(settings, info, false)`を呼び出し

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | settings | Arc<Settings> | ロード用設定 |
   | info | bool | ロード時の情報表示有無 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | IndexResult<SimpleIndexer> | ロード結果 |

5. 使用例
   ```rust
   let indexer = persistence.load_with_settings(Arc::new(settings), true)?;
   ```

6. エッジケース
   - `settings`の所有権は`SimpleIndexer`へ移動

#### load_with_settings_lazy

1. 目的と責務
   - Tantivyインデックス/メタデータ/セマンティックデータ/インデックス済みパスの復元を行い、`SimpleIndexer`を初期化

2. アルゴリズム（関数: load_with_settings_lazy、行番号: 不明）
   - `IndexMetadata::load(base_path)`を試み、`Option<IndexMetadata>`へ（失敗は無視）
   - `tantivy/meta.json`の存在を確認
     - 存在しなければ`IndexError::FileRead(NotFound)`を返す
     - 存在すれば、`settings.debug`を先に取り出してから`SimpleIndexer::with_settings`または`with_settings_lazy`でインデクサ生成（`skip_trait_resolver`分岐）
   - `metadata`があれば、`info`時に`DataSource`種別とドキュメント数を表示。`symbol_count`/`file_count`はインデクサからの最新値を表示
   - セマンティック検索のロードを**常に試行**（存在チェックは行わない）
     - `load_semantic_search(semantic_path, info)`の結果で分岐: `Ok(true)`/`Ok(false)`/`Err(e)`（`Err`でも継続し、警告ログのみ）
   - `metadata.indexed_paths`があれば`indexer.add_indexed_path`で復元（失敗はdebug時のみログ）
   - `Ok(indexer)`を返す

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | settings | Arc<Settings> | ロード用設定（`SimpleIndexer`に移動） |
   | info | bool | ロード時の情報表示有無 |
   | skip_trait_resolver | bool | lazy初期化モード選択（互換性目的） |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | IndexResult<SimpleIndexer> | 初期化されたインデクサ |

5. 使用例
   ```rust
   let indexer = persistence.load_with_settings_lazy(Arc::new(settings), true, false)?;
   ```

6. エッジケース
   - Tantivyメタ無し → NotFoundで失敗
   - セマンティックロード失敗 → 警告ログのみで継続（オプション機能）
   - `indexed_paths`復元時に個別エラー → debug時のみログ、継続
   - `settings.debug`を移動前に抽出しないと借用エラーになるため、先に抽出済み（所有権配慮）

#### exists

1. 目的と責務
   - `base_path/tantivy/meta.json`の存在確認

2. アルゴリズム（関数: exists、行番号: 不明）
   - `tantivy_path.join("meta.json").exists()`で真偽判定

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | &self | &IndexPersistence | マネージャ |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | bool | インデックスの存在可否 |

5. 使用例
   ```rust
   if !persistence.exists() { /* 初期化処理 */ }
   ```

6. エッジケース
   - ファイルシステムの一時的な不整合 → 返り値は瞬間の状態のみ

#### clear

1. 目的と責務
   - `base_path/tantivy`ディレクトリを削除し、空の同名ディレクトリを再作成。Windowsのファイルロックに対処。

2. アルゴリズム（関数: clear、行番号: 不明）
   - `tantivy_path.exists()`なら削除処理へ
   - 最大3回の`remove_dir_all`リトライ
     - Windowsで`PermissionDenied`なら200ms待機+`black_box()`呼び出し
     - それ以外の失敗は100ms待機して再試行
     - 成功でループ脱出、失敗でエラー返却
   - `create_dir_all(tantivy_path)`で空ディレクトリ再作成
   - Windowsなら再作成後100ms待機
   - `Ok(())`

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | &self | &IndexPersistence | マネージャ |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Result<(), std::io::Error> | 成功/失敗（I/O） |

5. 使用例
   ```rust
   persistence.clear()?; // tantivyをクリアして再初期化
   ```

6. エッジケース
   - Windowsの権限拒否/ロック → リトライ処理で緩和
   - 他プロセスが同時アクセス → レースで失敗する可能性（適切にエラーへ）

### Data Contracts

- IndexMetadata（このチャンクでは構造不明）
  - 使用フィールド/メソッド（関数: save, load_with_settings_lazy など、行番号: 不明）
    - `symbol_count`, `file_count`, `last_modified`（`update_project_registry`で使用）
    - `indexed_paths: Option<Vec<PathBuf>>`（復元対象）
    - `data_source: DataSource`（Tantivy/Fresh）
    - `load`, `new`, `update_counts`, `update_indexed_paths`, `save`
- DataSource
  - `Tantivy { path: PathBuf, doc_count: u32, timestamp: ... }`を使用
  - `Fresh`（情報用分岐で表示のみ）

不明点:
- IndexMetadata/Settings/SimpleIndexerの内部構造と正確なフィールド型はこのチャンクには現れない

## Walkthrough & Data Flow

- 保存フロー（save）
  - 入力: `&SimpleIndexer`
  - ステップ
    1. メタデータ読み込み or 新規生成
    2. 記号数・ファイル数・インデックス済みパスの更新
    3. データソースを`Tantivy`に設定（パス・doc_count・timestamp）
    4. メタデータ保存
    5. プロジェクトレジストリ更新（失敗はログのみ）
    6. セマンティック検索データの保存（オプション）
  - 出力: 成功/失敗（IndexResult<()>）

- ロードフロー（load_with_settings_lazy）
  - 入力: `Arc<Settings>`, `info`, `skip_trait_resolver`
  - ステップ
    1. メタデータをOptionとして読み込み
    2. Tantivy存在チェック
    3. 設定からdebug抽出→インデクサ生成（lazy/非lazy）
    4. info/debug表示（データソース・最新カウント）
    5. セマンティック検索ロード試行（オプション、失敗は警告のみ）
    6. インデックス済みパスの復元
    7. インデクサ返却

### Mermaid: load_with_settings_lazyの主要分岐

```mermaid
flowchart TD
    A[Start] --> B[Load IndexMetadata (Option)]
    B --> C{tantivy/meta.json exists?}
    C -- No --> Z[Err(NotFound: FileRead)]
    C -- Yes --> D[Extract settings.debug]
    D --> E{skip_trait_resolver?}
    E -- Yes --> F[SimpleIndexer::with_settings_lazy]
    E -- No --> G[SimpleIndexer::with_settings]
    F --> H[info/debug: print DataSource and fresh counts]
    G --> H
    H --> I[semantic_path = base/semantic]
    I --> J[Try load_semantic_search(path, info)]
    J -- Ok(true) --> K[Semantic loaded]
    J -- Ok(false) --> L[No semantic data]
    J -- Err(e) --> M[Warn and continue]
    K --> N[Restore indexed_paths from metadata]
    L --> N
    M --> N
    N --> Y[Ok(indexer)]
```

上記の図は`load_with_settings_lazy`関数の主要分岐を示す（行番号: 不明）。

### Mermaid: clearのリトライロジック

```mermaid
flowchart TD
    A[Start] --> B{tantivy_path exists?}
    B -- No --> H[create_dir_all(tantivy_path) if needed] --> I[Ok]
    B -- Yes --> C[attempts=0]
    C --> D[remove_dir_all(tantivy_path)]
    D -- Ok --> E[create_dir_all(tantivy_path)]
    E --> F{windows?}
    F -- Yes --> G[sleep 100ms]
    F -- No --> I[Ok]
    G --> I
    D -- Err(e) --> J{attempts < 3?}
    J -- No --> K[return Err(e)]
    J -- Yes --> L{windows && PermissionDenied?}
    L -- Yes --> M[log + black_box() + sleep 200ms]
    L -- No --> N[log + sleep 100ms]
    M --> O[attempts+=1] --> D
    N --> O --> D
```

上記の図は`clear`関数の主要分岐を示す（行番号: 不明）。

## Complexity & Performance

- save
  - 時間: O(p + m + s)（p=インデックス済みパス数、m=メタ保存I/O、s=セマンティック保存）
  - 空間: O(p)（パスベクトル）
  - ボトルネック: ファイルI/O（メタ保存/セマンティック保存）、`get_indexed_paths`のコピー
- load_with_settings_lazy
  - 時間: O(p + s)（パス復元/セマンティックロード）
  - 空間: O(p)
  - ボトルネック: ファイル存在チェック、セマンティックロード（外部実装依存）
- exists
  - 時間/空間: O(1)
- clear
  - 時間: O(f)（ディレクトリ内ファイル数）
  - 空間: O(1)
  - ボトルネック: Windowsでのロック/権限、複数回の削除試行
- スケール限界と運用
  - インデックス済みパスが増えた場合の**復元ループ**が増加
  - セマンティック検索データのサイズ増大時の保存/ロード時間の増加（詳細は*不明*）
  - ネットワーク/DBは本モジュールでは不使用（ローカルFSのみ）

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| Tantivy不在 | base/tantivy/meta.json無し | NotFoundエラー | `load_with_settings_lazy`でErr返却 | 良好 |
| メタデータ読み込み失敗 | index.meta破損 | 新規メタデータ生成 | `unwrap_or_else(IndexMetadata::new)` | 良好 |
| セマンティック無し | semantic/無 | ログ（debug/info）だけで継続 | `Ok(false)`扱い | 良好 |
| セマンティック保存失敗 | パーミッション拒否 | エラーで失敗 | `IndexError::General`に変換 | 良好 |
| レジストリ更新失敗 | ~/.codanna書込不可 | インデックス保存は成功、警告表示 | `eprintln!`で告知し継続 | 良好（設計意図） |
| indexed_paths復元失敗 | 無効パス含む | 個別エラーはログのみ、継続 | debug時`eprintln!` | 良好 |
| Windows削除失敗 | PermissionDenied | リトライ後成功/失敗通知 | リトライ/遅延あり | 良好 |
| 大量パス | indexed_pathsが非常に多い | 線形で処理時間増加 | ベクトル複製→ループ | 要監視 |
| document_count取得失敗 | `document_count()`がErr | 0にフォールバック | `unwrap_or(0)` | 良好 |
| Settings所有権 | Arcの移動 | 移動前にdebug抽出 | 先取りでbool取得 | 良好 |

- バグ/改善候補
  - セマンティックロード失敗を**警告のみにする**仕様は、運用要件によっては見過ごされる恐れ（再試行や明示的な状態通知を検討）
  - `save`で`update_project_registry`失敗時に**戻り値へ反映しない**ため、上位レイヤでの一括エラー監視が困難（イベント/メトリクスで補完が望ましい）
  - `clear`後に`tantivy`を再作成するが、**サブディレクトリ構成**の再初期化は*不明*（上位が対応）

- セキュリティチェックリスト
  - メモリ安全性: **Buffer overflow / Use-after-free / Integer overflow**の可能性は低い（標準ライブラリと安全なRustのみ）
  - インジェクション: **SQL/Command/Path traversal**は直接なし。パスは`base_path`準拠で構築。外部入力の妥当性検証は*上位に依存*。
  - 認証・認可: この層では未実装。**ファイルシステム権限**に依存（失敗時はエラー/警告）
  - 秘密情報: **ハードコードされた秘密情報無し**。ログにパスが出るが機微情報は*不明*。
  - 並行性: **Race condition/Deadlock**はこのモジュール単体では無し。ただし他プロセスが同ディレクトリ操作すると**削除/作成**で競合の可能性あり。
    - `clear`のリトライは競合緩和策だが完全ではない。

### Rust特有の観点（詳細チェック）

- 所有権
  - `load_with_settings_lazy`: `settings`（`Arc<Settings>`）は`SimpleIndexer::with_settings{_lazy}`へ**移動**。移動前に`let debug = settings.debug;`で**コピー**（関数: load_with_settings_lazy、行番号: 不明）。
  - `save`: `indexer.get_indexed_paths().iter().cloned().collect()`で**所有権の新規ベクトル**を作成。

- 借用
  - すべてのメソッドが`&self`参照。内部フィールドの変更は無し（`IndexPersistence`は不変）。

- ライフタイム
  - 明示的ライフタイムは不要。`Arc`と所有権移動のみ。

- unsafe境界
  - **unsafe未使用**。`std::hint::black_box()`は安全関数。

- 並行性・非同期
  - **同期処理のみ**。`thread::sleep`による待機あり。
  - `Send/Sync`境界は型定義に現れない（`IndexPersistence`は`PathBuf`のみを保持し、通常`Send`/`Sync`では問題ないが公式保証は*不明*）。

- await境界/キャンセル
  - 非同期未使用のため該当無し。

- エラー設計
  - `IndexResult`（`Result<_, IndexError>`）を使用。I/Oや一般エラーを適切にラップ。
  - `unwrap_or(0)`は仕様上許容（ドキュメント数の情報目的）。
  - `update_project_registry`失敗は**ログのみ**で継続というポリシーを明示。
  - `From/Into`のエラー変換詳細はこのチャンクには現れない。

## Design & Architecture Suggestions

- セマンティック検索のエラー扱い
  - 現状は**オプション**として失敗時も継続。運用要件次第では、ロード結果に**状態フラグ**（例: `SemanticStatus::{Loaded,NotFound,Failed}`）を返し、上位が判断可能にすると良い。
- レジストリ更新の堅牢化
  - `save`の戻り値に**副次エラー情報**（例: `SaveOutcome { index_saved: bool, registry_updated: bool }`）を含める、または**イベントログ/メトリクス**で明示。
- ログの統一
  - `eprintln!`ではなく**構造化ロガー**（env_logger/tracing）を使用し、**debug/info/warn**レベルを統一。
- 設定の受け渡し
  - `load()`が`Settings::default()`固定なのは上位の期待とズレる可能性。**base_path由来の設定**を内部で補完するか、明示的にドキュメント化。
- クリア操作の拡張
  - `clear`は`tantivy`のみ対象。セマンティック検索ディレクトリも**オプションで削除**できるAPIを追加検討。
- メタデータ一貫性
  - `document_count`に0フォールバックする仕様を**メタデータ整合性監査**で補完（例: カウントの差分警告）。

## Testing Strategy (Unit/Integration) with Examples

既存テスト:
- `test_save_and_load`: メタデータ保存の存在確認（Tantivyディレクトリを事前作成）
- `test_exists`: Tantivyメタの存在/非存在
- `test_semantic_paths`: セマンティックパスとメタファイルの検知

追加推奨テスト:
- update_project_registryの動作
  - `.project-id`無し時に**noop**で成功すること
  - `.project-id`有り＆レジストリ存在 → **値更新**されること
  - レジストリ書き込み不可 → **警告ログ**のみで`save`が成功すること

- load_with_settings_lazyの分岐網羅
  - Tantivyメタ無し → NotFound
  - セマンティック`Ok(true)/Ok(false)/Err`の3種
  - `indexed_paths`復元失敗時の**ログ**確認（debug有効時）

- clearのリトライ
  - 疑似的に`remove_dir_all`を失敗させ、**最大試行数**到達時のエラー返却を確認（プラットフォーム依存のためモック/抽象化が必要）

- 大量indexed_pathsのパフォーマンス
  - 大量のパスを設定し、**復元時間**が線形で増えることを確認（境界テスト）

例: レジストリ更新の統合テスト（疑似モック前提、擬似コード）

```rust
// このチャンクにはレジストリの詳細がないため擬似コード例
#[test]
fn test_update_project_registry_success_and_failure() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let persistence = IndexPersistence::new(temp_dir.path().to_path_buf());

    // 準備: tantivyディレクトリとmeta
    std::fs::create_dir_all(temp_dir.path().join("tantivy")).unwrap();
    std::fs::write(temp_dir.path().join("tantivy").join("meta.json"), "{}").unwrap();

    // SettingsとIndexerのセットアップ（詳細は不明）
    let settings = Settings { index_path: temp_dir.path().to_path_buf(), ..Settings::default() };
    let indexer = SimpleIndexer::with_settings(Arc::new(settings));

    // save実行（レジストリ書き込み失敗をシミュレートするにはProjectRegistryをモック化する必要あり）
    let result = persistence.save(&indexer);
    assert!(result.is_ok());
}
```

## Refactoring Plan & Best Practices

- ロギングの抽象化
  - `eprintln!`から`tracing`へ移行し、**レベル管理**と**可観測性**を強化（Metrics/Spanとの統合）
- エラー詳細の伝搬
  - `save`のレジストリ更新失敗を戻り値に**付随情報**として返し、上位が対処可に
- API分割
  - `load_with_settings_lazy`は責務が多いので、以下に分割するとテスト容易
    - `ensure_tantivy_exists`
    - `create_indexer(settings, skip_trait_resolver)`
    - `restore_from_metadata(indexer, metadata, info, debug)`
    - `load_semantic(indexer, path, info, debug)`
- Windows固有処理の抽象化
  - 削除リトライを**プラットフォーム抽象**に切り出し、ユニットテスト可能に
- セマンティック検索の状態管理
  - ロード結果を**型で表現**し上位へ伝えることで運用可視化

## Observability (Logging, Metrics, Tracing)

- 現状
  - 標準エラー出力で**DEBUG/Warning/Note**を出すのみ
  - ログレベルや構造化情報はなし

- 提案
  - **tracing**でイベントログ（例: `index.save`, `index.load.semantic`, `registry.update`）
  - メトリクス
    - 保存/ロード時間（ヒストグラム）
    - セマンティックデータの有無/失敗回数
    - クリアのリトライ回数
  - トレース
    - `save`/`load_with_settings_lazy`にSpanを張り、外部呼び出し（`SimpleIndexer`）の所要時間を可視化

## Risks & Unknowns

- Unknowns
  - `SimpleIndexer`/`Settings`/`IndexMetadata`の詳細仕様（初期化・ロード戦略・スレッド安全性）
  - セマンティック検索のデータ形式・サイズ・I/O特性
  - `crate::init::ProjectRegistry`の実装詳細（フォーマット・ロック戦略）
  - タイムスタンプ型（`get_utc_timestamp()`の型）

- Risks
  - セマンティックデータに依存する機能が**黙って非活性**になる可能性（Optional扱い）
  - マルチプロセス/マルチスレッド環境での**ディレクトリ操作の競合**
  - メタデータと実インデックスの**ドキュメント数不整合**が見えるが自動修正はしない（情報表示のみ）
  - `load()`がデフォルト設定を使用することによる**配置パス不一致**のリスク（上位と合意が必要）