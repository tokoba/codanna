# project_resolver\persist.rs Review

## TL;DR

- 目的: TypeScriptなどのプロジェクトリゾルバ向けに、**解決インデックス**（設定ファイルのSHA、ファイルパターン→設定ファイルの対応、ルール）をJSONで永続化・読み込みする仕組みを提供
- 主な公開API: **ResolutionIndex**（インメモリのインデックス管理）、**ResolutionPersistence**（ディスクへの保存/読み込み/削除）、定数**RESOLUTION_INDEX_VERSION**
- コアロジック: `ResolutionIndex::get_config_for_file`は、パターンの最長プレフィックス一致でファイル→設定ファイルを解決（現状は簡易なプレフィックス判定のみでglob未対応）
- エラー設計: I/OとJSONパースを`ResolutionError`へ適切に変換し、バージョン不一致を明示的にエラーにする
- 複雑箇所: プレフィックス切り詰めとソートによるパターン一致、永続化時のディレクトリ作成・JSONシリアライズ/デシリアライズ
- 重大リスク: `index_path(language_id)`の文字列結合仕様により、攻撃者制御の`language_id`で**パストラバーサル**が起き得る可能性（例: `"../evil"`）、パターンマッチが**OS間非互換**（区切り文字）で誤動作の懸念
- 並行性: インメモリ構造はロックなしで`&mut self`を要求、安全だが**多スレッド同時操作**や**同時ファイル書き込み**を考慮していない

## Overview & Purpose

このファイルは、プロジェクトのモジュール解決（例えばTypeScriptの`baseUrl`と`paths`）に必要な情報を「解決インデックス」として管理し、そのインデックスを`.codanna/index/resolvers/{language}_resolution.json`に永続化/ロードするための機能を提供します。機能の範囲は以下です。

- データモデル: `ResolutionIndex`にはスキーマバージョン、設定ファイルのSHA256、ファイルパターン→設定ファイルの対応、設定ファイルごとの解決ルールを保持
- インメモリ操作: SHA更新、マッピング追加、ルール設定、対象ファイルに対応する設定ファイルの検索
- 永続化: JSONへの保存、読み込み、削除（バージョン検証含む）

対象は主にTypeScriptですが、`language_id`で拡張可能な設計です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Const | RESOLUTION_INDEX_VERSION | pub | インデックススキーマのバージョン管理 | Low |
| Struct | ResolutionIndex | pub | 解決インデックス（バージョン、SHA、マッピング、ルール）を保持・操作 | Med |
| Struct | ResolutionRules | pub | 設定ファイルから抽出された`baseUrl`と`paths`のルール集合 | Low |
| Struct | ResolutionPersistence | pub | 解決インデックスのディスクへの保存/読み込み/削除 | Med |

### Dependencies & Interactions

- 内部依存
  - `super::{ResolutionError, ResolutionResult, Sha256Hash}`を使用（エラー型・Resultエイリアス・SHA型）。具体的な実装はこのチャンクには現れない。
  - `serde::{Serialize, Deserialize}`によるデータモデルの直列化/逆直列化
  - `serde_json`（このチャンク内で`serde_json::from_str`/`to_string_pretty`使用）
  - `std::fs`（`read_to_string`, `write`, `create_dir_all`, `remove_file`）
  - `std::collections::HashMap`, `std::path::{Path, PathBuf}`

- 外部依存（テストのみ）
  | クレート/モジュール | 用途 |
  |---------------------|------|
  | tempfile::TempDir | 一時ディレクトリ作成 |
  | crate::config::{LanguageConfig, Settings} | 言語設定取得 |
  | crate::parsing::typescript::tsconfig::read_tsconfig | TypeScriptのtsconfig読み込み |
  | crate::project_resolver::sha::compute_file_sha | ファイルのSHA256計算 |

- 被依存推定（このモジュールを使用する可能性のある箇所）
  - TypeScriptのパス解決器（resolver）やウォッチャーが、設定変更時の再ビルド判定やファイル→設定ファイルの解決に利用
  - CLIやデーモンがインデックスの読み書きを介して起動時間短縮

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| RESOLUTION_INDEX_VERSION | `pub const RESOLUTION_INDEX_VERSION: &str` | インデックススキーマのバージョン文字列 | O(1) | O(1) |
| ResolutionIndex::new | `pub fn new() -> Self` | 空のインデックス生成 | O(1) | O(1) |
| ResolutionIndex::needs_rebuild | `pub fn needs_rebuild(&self, config_path: &Path, current_sha: &Sha256Hash) -> bool` | 設定ファイルのSHA差分で再構築要否判定 | O(1) ハッシュ参照 | O(1) |
| ResolutionIndex::update_sha | `pub fn update_sha(&mut self, config_path: &Path, sha: &Sha256Hash)` | 設定ファイルのSHA登録/更新 | O(1) 平均（HashMap） | O(1) |
| ResolutionIndex::add_mapping | `pub fn add_mapping(&mut self, pattern: &str, config_path: &Path)` | ファイルパターン→設定ファイル対応を追加 | O(1) 平均（HashMap） | O(1) |
| ResolutionIndex::set_rules | `pub fn set_rules(&mut self, config_path: &Path, rules: ResolutionRules)` | 設定ファイルに対応する解決ルールを設定 | O(1) 平均（HashMap） | O(1) |
| ResolutionIndex::get_config_for_file | `pub fn get_config_for_file(&self, file_path: &Path) -> Option<&PathBuf>` | 対象ファイルの設定ファイルを最長プレフィックス一致で検索 | O(m + k log k) | O(k) |
| ResolutionRules（構造体） | `pub struct ResolutionRules { pub base_url: Option<String>, pub paths: HashMap<String, Vec<String>> }` | 設定ファイルから抽出されたルールのデータ契約 | N/A | N/A |
| ResolutionPersistence::new | `pub fn new(codanna_dir: &Path) -> Self` | 永続化マネージャ生成（ベースディレクトリ設定） | O(1) | O(1) |
| ResolutionPersistence::load | `pub fn load(&self, language_id: &str) -> ResolutionResult<ResolutionIndex>` | JSONからインデックスを読み込み（存在しなければ新規） | O(n) 読み込み/パース | O(n) |
| ResolutionPersistence::save | `pub fn save(&self, language_id: &str, index: &ResolutionIndex) -> ResolutionResult<()>` | インデックスをJSONで保存 | O(n) シリアライズ/書き込み | O(n) |
| ResolutionPersistence::clear | `pub fn clear(&self, language_id: &str) -> ResolutionResult<()>` | インデックスファイル削除 | O(1)〜O(n) | O(1) |

詳細説明:

1) ResolutionIndex::new
- 目的と責務: 空のインデックスを現在のスキーマバージョンで初期化
- アルゴリズム:
  1. `version`にRESOLUTION_INDEX_VERSIONを設定
  2. `hashes`, `mappings`, `rules`を空の`HashMap`で初期化
- 引数: なし
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | ResolutionIndex | 新規インデックス |
- 使用例:
  ```rust
  let mut index = ResolutionIndex::new();
  ```
- エッジケース:
  - 特になし（定数と空のコレクションのみ）

2) ResolutionIndex::needs_rebuild
- 目的と責務: 指定設定ファイルのSHAが一致するか確認して、再ビルドが必要か判定
- アルゴリズム:
  1. `hashes`から`config_path`のSHA文字列を取得
  2. `current_sha.as_str()`と比較
  3. 既存エントリがない、または非一致なら`true`
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | config_path | &Path | 設定ファイルパス |
  | current_sha | &Sha256Hash | 現在のSHA |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | bool | 再ビルド要否 |
- 使用例:
  ```rust
  let sha = compute_file_sha(config_path)?;
  if index.needs_rebuild(config_path, &sha) {
      // 再インデックス処理
  }
  ```
- エッジケース:
  - ハッシュ未登録: `true`を返す
  - `current_sha.as_str()`が空文字: 不一致として`true`（安全ではあるが意味的検証は上位で必要）

3) ResolutionIndex::update_sha
- 目的と責務: 指定設定ファイルのSHAをインデックスに保存
- アルゴリズム:
  1. `hashes.insert(config_path.to_path_buf(), sha.as_str().to_string())`
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | config_path | &Path | 設定ファイルパス |
  | sha | &Sha256Hash | SHA値 |
- 戻り値: なし
- 使用例:
  ```rust
  index.update_sha(config_path, &sha);
  ```
- エッジケース:
  - 既存エントリ上書き: そのまま新値を保存（履歴は保持しない）

4) ResolutionIndex::add_mapping
- 目的と責務: ファイルパターン→設定ファイルの対応を追加
- アルゴリズム:
  1. `mappings.insert(pattern.to_string(), config_path.to_path_buf())`
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | pattern | &str | ファイルパターン（例: "src/**/*.ts"） |
  | config_path | &Path | 対応する設定ファイル |
- 戻り値: なし
- 使用例:
  ```rust
  index.add_mapping("examples/typescript/src/**/*.ts", ts_config_path);
  ```
- エッジケース:
  - 同一パターン重複登録: 上書きされる

5) ResolutionIndex::set_rules
- 目的と責務: 設定ファイルに紐づく解決ルール（`baseUrl`, `paths`）を設定
- アルゴリズム:
  1. `rules.insert(config_path.to_path_buf(), rules)`
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | config_path | &Path | 設定ファイルパス |
  | rules | ResolutionRules | ルール |
- 戻り値: なし
- 使用例:
  ```rust
  index.set_rules(ts_config_path, ResolutionRules { base_url, paths });
  ```
- エッジケース:
  - 上書き: 既存ルールを更新

6) ResolutionIndex::get_config_for_file
- 目的と責務: 対象ファイルに最も適合する設定ファイルを検索
- アルゴリズム（このファイルの関数本体参照; 行番号はこのチャンクでは不明）:
  1. `file_path.to_str()`でUTF-8文字列化。失敗時は`None`を返す
  2. `mappings`を走査し、簡易な「プレフィックス一致」で候補を抽出
     - パターン末尾の`"**/*.ts"`、`"**/*.tsx"`、末尾`'/'`を取り除いたプレフィックスを用いる
  3. 候補をパターン文字列長で降順ソート（最長マッチ優先）
  4. 先頭の候補の`config`を返す（`Option<&PathBuf>`）
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | file_path | &Path | 対象ファイル |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Option<&PathBuf> | 該当する設定ファイル（なければNone） |
- 使用例:
  ```rust
  let cfg = index.get_config_for_file(Path::new("examples/typescript/src/main.ts"));
  if let Some(ts_config) = cfg {
      // ts_configを使用して解決
  }
  ```
- エッジケース:
  - 非UTF-8パス: `None`を返す
  - 複数マッチ: 最長プレフィックスが選択される
  - Windows区切り（'\\'）とパターンが'/'の不一致: 誤判定の可能性あり（後述）

7) ResolutionRules（データ契約）
- 目的と責務: TypeScriptの`tsconfig`から抽出した解決ルールの保持
- フィールド:
  | 名 | 型 | 説明 |
  |----|----|------|
  | base_url | Option<String> | `compilerOptions.baseUrl` |
  | paths | HashMap<String, Vec<String>> | `compilerOptions.paths` |
- 使用例:
  ```rust
  let rules = ResolutionRules { base_url: Some("src".to_string()), paths: aliases };
  index.set_rules(config_path, rules);
  ```
- エッジケース:
  - `base_url`が`None`: ベースURLなしの解決

8) ResolutionPersistence::new
- 目的と責務: ベースディレクトリ（`.codanna/index/resolvers`）を設定した永続化マネージャを生成
- アルゴリズム:
  1. `base_dir = codanna_dir.join("index").join("resolvers")`
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | codanna_dir | &Path | `.codanna`ディレクトリへのパス |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | ResolutionPersistence | マネージャ |
- 使用例:
  ```rust
  let persistence = ResolutionPersistence::new(Path::new(".codanna"));
  ```
- エッジケース:
  - 特になし（生成のみ）

9) ResolutionPersistence::load
- 目的と責務: 指定言語のインデックスファイルを読み込み、なければ新規生成
- アルゴリズム（関数本体参照; 行番号はこのチャンクでは不明）:
  1. `index_path(language_id)`でファイルパス生成
  2. 存在しなければ`ResolutionIndex::new()`を返す
  3. 存在すれば`fs::read_to_string`で内容読込
  4. `serde_json::from_str`で`ResolutionIndex`へパース
  5. `version`が`RESOLUTION_INDEX_VERSION`と一致するか検証
  6. 一致すれば返却。非一致なら`ResolutionError::ParseError`でエラー
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | language_id | &str | 言語ID（例: "typescript"） |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | ResolutionResult<ResolutionIndex> | 読み込み結果 |
- 使用例:
  ```rust
  let loaded = persistence.load("typescript")?;
  ```
- エッジケース:
  - JSON破損: `ParseError`
  - バージョン不一致: `ParseError`
  - I/O失敗: `IoError`（パスと原因文字列含む）

10) ResolutionPersistence::save
- 目的と責務: インデックスをJSON形式で保存（ディレクトリがなければ作成）
- アルゴリズム:
  1. `fs::create_dir_all(&self.base_dir)`
  2. `serde_json::to_string_pretty(index)`で整形JSON化
  3. `fs::write(&path, content)`
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | language_id | &str | 言語ID |
  | index | &ResolutionIndex | 保存対象 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | ResolutionResult<()> | 成否 |
- 使用例:
  ```rust
  persistence.save("typescript", &index)?;
  ```
- エッジケース:
  - ディレクトリ作成失敗/書き込み失敗: `IoError`
  - シリアライズ失敗: `ParseError`

11) ResolutionPersistence::clear
- 目的と責務: インデックスファイルを削除（存在しない場合は何もしない）
- アルゴリズム:
  1. `index_path(language_id)`でパス算出
  2. `path.exists()`なら`fs::remove_file(&path)`
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | language_id | &str | 言語ID |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | ResolutionResult<()> | 成否 |
- 使用例:
  ```rust
  persistence.clear("typescript")?;
  ```
- エッジケース:
  - 削除失敗: `IoError`

データ契約（ResolutionIndex）:
- フィールド:
  | 名 | 型 | 説明 |
  |----|----|------|
  | version | String | スキーマバージョン（RESOLUTION_INDEX_VERSIONと一致が必要） |
  | hashes | HashMap<PathBuf, String> | 設定ファイルのSHA256文字列 |
  | mappings | HashMap<String, PathBuf> | ファイルパターン→設定ファイル |
  | rules | HashMap<PathBuf, ResolutionRules> | 設定ファイル→解決ルール |

## Walkthrough & Data Flow

典型的な利用フロー（テストに準拠）:

1. 設定ファイルのSHA計算
   - `compute_file_sha(config_path)`でSHA256を計算（このチャンクには現れない）
   - `index.needs_rebuild(config_path, &sha)`で差分確認
   - 差分があれば`index.update_sha(config_path, &sha)`で更新

2. tsconfigの読み込みとルール設定
   - `read_tsconfig(config_path)`で`baseUrl`/`paths`を取得（このチャンクには現れない）
   - `index.set_rules(config_path, ResolutionRules { base_url, paths })`

3. ファイルパターンの登録
   - `index.add_mapping("examples/typescript/src/**/*.ts", config_path)`

4. ファイル解決
   - `index.get_config_for_file(Path::new(".../src/main.ts"))`で該当設定ファイルを特定
   - 最長プレフィックス一致で選択

5. 永続化
   - `persistence.save("typescript", &index)`でJSON保存
   - 次回起動時に`persistence.load("typescript")`で読み込み、バージョン確認

6. クリア
   - 不要時は`persistence.clear("typescript")`でファイル削除

I/Oフロー:
- 読み込み: `index_path(language_id)` → `fs::read_to_string` → `serde_json::from_str` → バージョン検証
- 保存: `fs::create_dir_all` → `serde_json::to_string_pretty` → `fs::write`
- 削除: `fs::remove_file`

## Complexity & Performance

- ResolutionIndex 操作
  - `needs_rebuild`: O(1)（HashMapのキー検索）
  - `update_sha`/`add_mapping`/`set_rules`: 平均O(1)
  - `get_config_for_file`:
    - マッチ候補抽出: O(m)（m=登録パターン数）
    - ソート: O(k log k)（k=マッチ候補数、最悪m）
    - 合計: O(m + k log k) ≒ O(m log m)
    - 追加メモリ: O(k)

- 永続化
  - `load`: 読み込み/パースのO(n)（n=ファイルサイズ）。バージョンチェックはO(1)
  - `save`: シリアライズ/書き込みのO(n)（n=インデックスのJSONサイズ）
  - `clear`: O(1)〜O(n)（OS依存、ファイルサイズ/メタデータ）

ボトルネックとスケール限界:
- 大量のパターン登録（mが大）で`get_config_for_file`がオーバーヘッド。グロブを導入するなら前処理（トライ木や`globset`）で高速化可能
- ファイルサイズが大きい場合のJSON読み書きはI/Oバウンド。圧縮や差分保存、バイナリフォーマット（例えば`bincode`）検討余地
- パターン判定が文字列ベースの`starts_with`で、OS区切りや正規化不足による誤判定/再計算あり

実運用負荷要因:
- ファイルシステムI/O（ディスク速度、ファイルロック）
- 設定数増加によるメモリ利用（HashMapのキー/値にPathBuf/Strings）
- 複数言語/複数設定の同時操作（並行書き込み時の競合）

## Edge Cases, Bugs, and Security

エッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 非UTF-8パス | `OsStr`に非UTF-8含む | `get_config_for_file`はNone | `to_str()?`でNoneを返す | OK |
| パターン重複/競合 | `"src/**/*.ts"`と`"src/app/**/*.ts"`の両方登録 | 最長一致の`"src/app/**/*.ts"`が選ばれる | 長さ降順ソート | OK |
| Windows区切り | `"src\\main.ts"` vs `"src/**/*.ts"` | 正確な一致 | `starts_with`で'/'前提なので不一致の可能性 | 要改善 |
| glob未対応 | `"src/**/*.ts"`における`**`や`*`の意味 | 正確なglob一致 | 末尾トリム＋プレフィックス一致のみ | 欠落 |
| バージョン不一致 | JSONの`version="0.9"` | エラーを返す | `ParseError`で明示 | OK |
| インデックス未存在 | ファイルが存在しない | 新規`ResolutionIndex`返却 | `if !path.exists()`で新規 | OK |
| JSON破損 | 不正な形式 | パースエラーで失敗 | `ParseError`で明示 | OK |
| ディレクトリ作成失敗 | 権限なし | `IoError`で失敗 | `create_dir_all`の`map_err` | OK |
| 部分書き込み/クラッシュ | 書き込み中断 | 原子的保存で整合性維持 | 直接`fs::write`のみ | リスク |
| パストラバーサル | `language_id="../evil"` | ベースディレクトリ外への書き込み拒否 | `join(format!("{language_id}_resolution.json"))` | リスク |

セキュリティチェックリスト:

- メモリ安全性
  - Buffer overflow: Rustの標準APIと`String`/`HashMap`のみでunsafeなし、問題なし
  - Use-after-free: 所有権・借用に従っており発生しない
  - Integer overflow: 長さ比較に`i32`へキャスト（`-(pattern.len() as i32)`）あり。極端に長いパターン文字列で`i32`に収まらない場合はオーバーフローの懸念。ただし`usize`→`i32`変換で極大値は不適切。ソートキーは`isize`や逆順比較を使うべき（詳細は後述）
- インジェクション
  - SQL/Command: 該当なし
  - Path traversal: `language_id`が外部入力である場合、`index_path`の`format!`と`join`によりベースディレクトリ外へ到達可能なパスが生成され得る。防御が必要
- 認証・認可
  - 本コードには認可/認証機構はない。保存先アクセス権はOSに依存
- 秘密情報
  - ハードコード秘密情報: 該当なし
  - ログ漏えい: ログなし
- 並行性
  - Race condition: 同一言語IDに対し、別スレッドから同時`save`/`clear`が走ると競合可能。ファイルロックや原子的更新が望ましい
  - Deadlock: 該当なし（ロック未使用）

Rust特有の観点:

- 所有権
  - `update_sha`/`add_mapping`/`set_rules`で`PathBuf`を`to_path_buf()`し、`HashMap`の所有とする。借用は返さないため有効
- 借用
  - `get_config_for_file`は`Option<&PathBuf>`を返すため、インデックスのライフタイムに紐づく参照を安全に提供
- ライフタイム
  - 明示的ライフタイムは不要。参照は`&self`スコープに限定される
- unsafe境界
  - unsafeブロックなし
- 並行性・非同期
  - `Send/Sync`: `ResolutionIndex`は`HashMap<PathBuf, ..>`の集合で、型としては`Send/Sync`を満たすが、メソッドが`&mut self`を必要とするため並行更新は不可。スレッド共有時は外部で同期化が必要
  - await境界/キャンセル: 非同期処理なし
- エラー設計
  - `ResolutionResult`（型エイリアス想定）を一貫して使用
  - `IoError`と`ParseError`へ`map_err`で変換し、原因文字列を保持
  - `unwrap`/`expect`はテストコードにのみ存在。ライブラリ側はパニックなし

## Design & Architecture Suggestions

- 正確なパターンマッチングの導入
  - 現状の`starts_with`＋末尾トリムは簡易で誤判定が多い。*globset*クレートや*ignore*クレートを用いて**本格的なglob一致**を実装する
  - OS間のパス区切り（'/' vs '\\'）を**標準化**（`Path`ベース比較、`components()`）して一致判定を行う
- ソートの安全化
  - `matches.sort_by_key(|(pattern, _)| -(pattern.len() as i32));`は`usize→i32`変換が危険。`sort_by(|a,b| b.0.len().cmp(&a.0.len()))`などに置換
- パストラバーサル防止
  - `index_path(language_id)`で`language_id`を検証し、英数字と`-`/`_`など**限定されたホワイトリスト**のみ許可
  - もしくは`sanitize_filename`系の処理を導入
- 原子的な書き込み
  - `save`は`fs::write`で直接書くのではなく、`tmp`ファイルに書いて`rename`する**原子的更新**でクラッシュ/中断耐性を上げる
- バージョン移行（マイグレーション）
  - 将来のスキーマ変更に備え、バージョン不一致時に**移行ロジック**を提供（現状はエラーのみ）
- APIの拡張
  - 逆引きAPI（設定ファイル→マッチングパターン一覧）
  - マッピング削除/更新API
  - `get_rules_for_config(&Path)`の提供
- Canonicalizeの導入
  - `config_path`と`file_path`は`fs::canonicalize`で正規化して保存/比較し、相対パスの揺れを排除
- 設定ファイルの存在検証
  - `load`後に`hashes`/`rules`にある`PathBuf`が実在するか確認し、警告やクリーンアップ

## Testing Strategy (Unit/Integration) with Examples

既存テストは実ファイルに依存する統合的な流れを確認しています。補完すべき単体テスト・エッジテスト:

- ユニットテスト: `needs_rebuild`の基本動作
  ```rust
  #[test]
  fn test_needs_rebuild_simple() {
      let mut index = ResolutionIndex::new();
      let path = PathBuf::from("tsconfig.json");
      // ダミーのSha256Hash（このチャンクには型詳細不明）
      let sha1 = Sha256Hash::from_str("abc"); // 仮: 実際のAPIは不明
      let sha2 = Sha256Hash::from_str("def"); // 仮

      // 未登録はtrue
      assert!(index.needs_rebuild(&path, &sha1));
      // 更新後はfalse
      index.update_sha(&path, &sha1);
      assert!(!index.needs_rebuild(&path, &sha1));
      // 異なるSHAでtrue
      assert!(index.needs_rebuild(&path, &sha2));
  }
  ```
  ※ Sha256Hashの生成APIはこのチャンクには現れないため擬似コード（不明）。

- ユニットテスト: パターン最長一致
  ```rust
  #[test]
  fn test_get_config_longest_prefix() {
      let mut index = ResolutionIndex::new();
      let cfg1 = PathBuf::from("/configs/base.json");
      let cfg2 = PathBuf::from("/configs/app.json");
      index.add_mapping("src/**/*.ts", &cfg1);
      index.add_mapping("src/app/**/*.ts", &cfg2);

      let sel = index.get_config_for_file(Path::new("src/app/main.ts")).unwrap();
      assert_eq!(sel, &cfg2);
  }
  ```

- エッジテスト: 非UTF-8パス
  - OS依存で`OsStr`から`Path`生成し`to_str`が失敗するケースを模擬。期待値は`None`

- OS差異テスト: Windows区切り vs UNIX区切り
  - `"src\\main.ts"`（Windows想定）では現行実装が誤判定する可能性を検証。対策実装後は成功を確認

- セキュリティテスト: `language_id`のパストラバーサル
  ```rust
  #[test]
  fn test_language_id_traversal_risk() {
      let tmp = tempfile::TempDir::new().unwrap();
      let persistence = ResolutionPersistence::new(tmp.path());
      let index = ResolutionIndex::new();
      // 攻撃的ID（実際の対策実装後はエラーにすべき）
      let res = persistence.save("../evil", &index);
      // 現行は成功する可能性あり -> テストで検知し、修正を促す
      assert!(res.is_ok(), "現行では通るが、修正後はErrにすべき");
  }
  ```

- 永続化耐性: 原子的保存のテスト（対策実装後）

- 大量データテスト: パターン1万件で`get_config_for_file`の性能測定

既存の統合テストは、実ファイル存在チェックでスキップする分岐を含むため、CI環境での再現性確保のために**モックファイル/仮のtsconfig**を用いた固定テストに置換すると良いです。

## Refactoring Plan & Best Practices

- パターン一致改善
  - `get_config_for_file`を`globset::GlobSet`に置換し、事前コンパイルしたパターン集合を`ResolutionIndex`に保持
  - ソートではなく、マッチした`Glob`の**具体性（ワイルドカードの少なさ、長さなど）**で優先順位付け
- 文字列長ソートの修正
  - `sort_by_key`の負値ハックをやめ、`sort_by(|a,b| b.0.len().cmp(&a.0.len()))`
- ファイルパスの正規化
  - 登録時・検索時とも`canonicalize`と`components()`を用いた正規化を行う
- エラー型の詳細化
  - `ParseError`に**バージョン不一致**を別Variantとして分離（例: `VersionMismatch { expected, found }`）
  - `IoError`の`cause`を`std::io::Error`として保持（現在は`to_string()`で詳細が失われる）
- 永続化の原子的更新
  - `.tmp`ファイルに書き、`rename`。Windows/UNIX両対応の**安全な更新**を実装
- セキュアな`language_id`取り扱い
  - サニタイズ関数を導入（英数と`-_`のみ）、不正は`Err`
- APIの拡張とドキュメント化
  - モジュールレベルのドキュメントコメントに**使用例**と**設計意図**を追記
  - DataContractの**バージョン管理策**（将来フィールド追加の方針）を明示

## Observability (Logging, Metrics, Tracing)

- ロギング
  - `load`時: ファイルパス、読み込み成功/失敗、バージョン不一致の警告
  - `save`時: 書き込み先、サイズ、所要時間
  - `clear`時: 削除の成功/失敗
- メトリクス
  - インデックスサイズ（`hashes`/`mappings`/`rules`の件数）
  - 解決クエリ数とヒット率（`get_config_for_file`の成功/失敗）
  - I/Oレイテンシ（読み書きの時間）
- トレーシング
  - 設定ファイルSHA計算→インデックス更新→永続化までのスパンを**スパン**で計測
- 機密情報
  - パス情報はログに出るため、ユーザー環境依存情報の露出に配慮（デバッグレベルのみに）

## Risks & Unknowns

- Unknowns
  - `Sha256Hash`型の詳細なAPI（このチャンクには現れない）
  - `ResolutionError`/`ResolutionResult`の具体的な定義（Variant/型エイリアス）や`IoError`構造
  - 設定ファイル（tsconfigなど）の想定規模と複雑性
  - `language_id`の入力元（ユーザー/内部固定）により、パストラバーサルの深刻度が変わる

- Risks
  - パストラバーサルによるディスク書き込み先の逸脱
  - Windows/UNIX間のパス表現差異による解決誤り
  - glob未対応により**意図しないマッチ**や**ミスマッチ**
  - 非原子的な書き込みによる**ファイル破損**や**部分書き込み**
  - 文字列長ソートの`i32`キャストによる潜在的オーバーフローバグ（非常に長いパターンで理論上あり得る）

以上により、公開APIは明確で安全性もRustの規範に則っていますが、ファイルパスの取り扱いとパターンマッチングの正確性・セキュリティ・耐障害性の面で改善余地があります。