# profiles\lockfile.rs Review

## TL;DR

- 🎯 目的: インストール済みプロファイルをJSONロックファイルで管理するための**構造体**と**ユーティリティAPI**を提供
- 🔓 公開API: `ProfileLockfile::{new, load, save, is_installed, get_profile, add_profile, remove_profile, find_file_owner}` と `ProfileLockEntry`（全フィールドpub）
- 🧩 データ互換性: `integrity` は空文字、`commit`/`provider_id`/`source` は `Option` でデフォルトを許容し、古いロックファイルとの後方互換を確保
- ⚠️ 重大リスク: `save` が非アトミック書き込みで破損しうる、`load` のJSONエラーを一律 `InvalidManifest` に変換し原因情報が失われる、`find_file_owner` が文字列の不要な複製で低速
- 🔒 Rust安全性: `unsafe` なし、所有権・借用は適切。I/Oや並行性に関する**排他制御**は未実装でレース条件の可能性あり
- 📈 性能: HashMap操作は平均O(1)、`find_file_owner` は最悪 O(P×F)＋毎ループで `file_path.to_string()` する非効率
- 🧪 テスト指針: 不存在パスでのロード、保存と再ロード、後方互換フィールドのデシリアライズ、`find_file_owner` の一致/不一致、削除・更新の動作を網羅

## Overview & Purpose

このファイルは、プロファイル管理システムの「ロックファイル」を読み書き・更新するためのコアロジックを提供します。ロックファイルは、インストール済みプロファイルのメタデータ（バージョン、タイムスタンプ、インストールされたファイル一覧、整合性ハッシュ、供給元情報など）を保持するJSONです。

主な目的:
- プロファイルのインストール状態を永続化し、再現可能性を担保（例: 整合性チェック、供給元追跡）
- 後方互換性を意識したデータモデル（欠落フィールドのデフォルト付与）
- シンプルなAPIで、ロックファイルのロード/セーブ、クエリ、更新を提供

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | ProfileLockfile | pub | ロックファイル全体のモデルとCRUD（ロード/セーブ/クエリ/更新） | Low |
| Struct | ProfileLockEntry | pub | 個別プロファイルのメタデータ（名前、バージョン、ファイル、整合性、供給元） | Low |
| Impl Method | new | pub | 新規ロックファイルの生成 | Low |
| Impl Method | load | pub | JSONロックファイルの読み込み（なければ新規生成） | Low |
| Impl Method | save | pub | JSONロックファイルの保存（親ディレクトリ作成含む） | Low |
| Impl Method | is_installed | pub | プロファイルの存在チェック | Low |
| Impl Method | get_profile | pub | プロファイルの参照取得 | Low |
| Impl Method | add_profile | pub | プロファイルの追加/更新 | Low |
| Impl Method | remove_profile | pub | プロファイルの削除 | Low |
| Impl Method | find_file_owner | pub | ファイルの所有プロファイル検索 | Medium |

### Dependencies & Interactions

- 内部依存:
  - `ProfileLockfile` は `ProfileLockEntry` を `HashMap<String, ProfileLockEntry>` として保持
  - `load`/`save` は標準ライブラリのファイルI/Oと `serde_json` に依存
- 外部依存（クレート/モジュール）:

| モジュール/型 | 用途 |
|---------------|------|
| `super::error::{ProfileError, ProfileResult}` | ドメイン固有のエラー型とResultエイリアス |
| `super::provider_registry::ProviderSource` | 供給元の情報を表現（Git/ローカルなどの識別） |
| `serde::{Deserialize, Serialize}` | JSONシリアライズ/デシリアライズ |
| `std::collections::HashMap` | プロファイルエントリのインメモリ格納 |
| `std::path::Path` | パス表現と存在チェック |
| `std::fs` | ファイル読み書き、ディレクトリ作成 |

- 被依存推定（このモジュールを利用し得る箇所）:
  - プロファイルのインストーラ/アンインストーラ
  - CLIの「インストール済み一覧表示」機能
  - 整合性検証ルーチン（ハッシュの検証）
  - プロバイダ管理（供給元追跡・再解決）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| ProfileLockfile::new | `pub fn new() -> Self` | 空のロックファイル生成 | O(1) | O(1) |
| ProfileLockfile::load | `pub fn load(path: &Path) -> ProfileResult<Self>` | ディスクから読み込み（なければ新規） | O(n) | O(n) |
| ProfileLockfile::save | `pub fn save(&self, path: &Path) -> ProfileResult<()>` | ディスクへ保存（親ディレクトリ作成） | O(n) | O(n) |
| ProfileLockfile::is_installed | `pub fn is_installed(&self, name: &str) -> bool` | プロファイル存在判定 | O(1)平均 | O(1) |
| ProfileLockfile::get_profile | `pub fn get_profile(&self, name: &str) -> Option<&ProfileLockEntry>` | プロファイル参照取得 | O(1)平均 | O(1) |
| ProfileLockfile::add_profile | `pub fn add_profile(&mut self, entry: ProfileLockEntry)` | 追加/更新 | O(1)平均 | O(1)＋キー長 |
| ProfileLockfile::remove_profile | `pub fn remove_profile(&mut self, name: &str) -> Option<ProfileLockEntry>` | 削除 | O(1)平均 | O(1) |
| ProfileLockfile::find_file_owner | `pub fn find_file_owner(&self, file_path: &str) -> Option<&str>` | ファイル所有者検索 | O(P×F) | O(1) |

nはファイルサイズ（バイト）、Pはプロファイル数、Fは各プロファイルのファイル数。`add_profile` は `entry.name.clone()` によりキー複製コストがキー長に依存。

### データ契約（JSON）

- ProfileLockfile
  - version: String（例: "1.0.0"。`new()`では"1.0.0"固定）
  - profiles: { [name: String]: ProfileLockEntry }
- ProfileLockEntry
  - name: String
  - version: String
  - installed_at: String（タイムスタンプ）
  - files: Vec<String>（インストールされたファイルのパス）
  - integrity: String（SHA-256の想定。`#[serde(default)]` により欠落時は空文字）
  - commit: Option<String>（Gitソース時のコミット。欠落時None）
  - provider_id: Option<String>（供給元ID。欠落時None）
  - source: Option<ProviderSource>（供給元詳細。欠落時None）

後方互換性:
- `integrity` は空文字、`commit`/`provider_id`/`source` は `None` で復元されるため、古いロックファイルの読み込みに対応。

---

以下、各APIの詳細です（根拠: 関数名、行番号は本チャンクに行番号がないため不明）。

#### ProfileLockfile::new
1. 目的と責務
   - 新しいロックファイルを**デフォルトバージョン"1.0.0"**と空の `profiles` で作成
2. アルゴリズム
   - `version` に固定の文字列を設定
   - `profiles` に空の `HashMap` を割り当て
3. 引数
   - なし
4. 戻り値
   - `Self`（初期化済みの `ProfileLockfile`）
5. 使用例
```rust
let mut lockfile = ProfileLockfile::new();
assert!(!lockfile.is_installed("foo"));
```
6. エッジケース
   - なし（純粋な初期化）

#### ProfileLockfile::load
1. 目的と責務
   - 指定パスからロックファイルを読み込み、存在しない場合は新規作成
2. アルゴリズム
   - `path.exists()` で存在確認
   - なければ `Ok(Self::new())`
   - あれば `std::fs::read_to_string(path)?` で読み込み
   - `serde_json::from_str(&content)` でデシリアライズ
   - デシリアライズ失敗時、`ProfileError::InvalidManifest { reason: "Lockfile is corrupted" }` に変換
3. 引数

| 名前 | 型 | 意味 | 制約 |
|------|----|------|------|
| path | &Path | ロックファイルのパス | 読取可能であること |

4. 戻り値

| 型 | 意味 |
|----|------|
| ProfileResult<Self> | 成功時 `ProfileLockfile`、失敗時ドメインエラー |

5. 使用例
```rust
use std::path::Path;
let path = Path::new("profiles.lock.json");
let lockfile = ProfileLockfile::load(path)?; // なければ空のロックファイル
```
6. エッジケース
   - ファイルが存在しない: 新規生成して成功
   - ファイルは存在するが読み込み失敗（権限/エンコードなど）: `?` でI/Oエラーが伝播（具体型は `ProfileResult` 次第）
   - JSON破損: `InvalidManifest` に変換され原因詳細は失われる

#### ProfileLockfile::save
1. 目的と責務
   - ロックファイルを整形済みJSONで保存。親ディレクトリが無ければ作成
2. アルゴリズム
   - `path.parent()` から親ディレクトリを取得し `create_dir_all()` で作成
   - `serde_json::to_string_pretty(self)?` で整形JSONへ
   - `std::fs::write(path, json)?` で保存
3. 引数

| 名前 | 型 | 意味 | 制約 |
|------|----|------|------|
| path | &Path | 保存先パス | 書込可能であること |

4. 戻り値

| 型 | 意味 |
|----|------|
| ProfileResult<()> | 成否のみ |

5. 使用例
```rust
let path = Path::new("profiles/lockfile.json");
let lockfile = ProfileLockfile::new();
lockfile.save(path)?;
```
6. エッジケース
   - 親ディレクトリがない: 作成される
   - 書込み失敗（権限/ディスク容量）: I/Oエラーが伝播
   - 同時書込み: 破損する可能性（アトミックではない）

#### ProfileLockfile::is_installed
1. 目的と責務
   - 名前でプロファイル存在を判定
2. アルゴリズム
   - `HashMap::contains_key(name)` でチェック
3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| name | &str | プロファイル名 |

4. 戻り値

| 型 | 意味 |
|----|------|
| bool | 存在すれば true |

5. 使用例
```rust
assert!(!lockfile.is_installed("foo"));
```
6. エッジケース
   - 大文字/小文字の差異は区別される（文字列比較）

#### ProfileLockfile::get_profile
1. 目的と責務
   - プロファイルの参照を取得
2. アルゴリズム
   - `HashMap::get(name)` を返す
3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| name | &str | プロファイル名 |

4. 戻り値

| 型 | 意味 |
|----|------|
| Option<&ProfileLockEntry> | 参照（存在しなければ None） |

5. 使用例
```rust
if let Some(entry) = lockfile.get_profile("foo") {
    println!("version = {}", entry.version);
}
```
6. エッジケース
   - 戻り値は借用参照のため、`lockfile` のミュータブル操作後には参照が無効化される可能性あり（Rustの借用規則上、安全に防止される）

#### ProfileLockfile::add_profile
1. 目的と責務
   - プロファイルエントリを追加または更新
2. アルゴリズム
   - `entry.name.clone()` をキーに `HashMap::insert(key, entry)` を実行
3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| entry | ProfileLockEntry | 追加/更新するエントリ |

4. 戻り値
   - なし
5. 使用例
```rust
let entry = ProfileLockEntry {
    name: "foo".into(),
    version: "1.2.3".into(),
    installed_at: "2024-10-01T12:34:56Z".into(),
    files: vec!["bin/foo".into()],
    integrity: "".into(),
    commit: None,
    provider_id: None,
    source: None,
};
let mut lockfile = ProfileLockfile::new();
lockfile.add_profile(entry);
assert!(lockfile.is_installed("foo"));
```
6. エッジケース
   - 同名プロファイルが既に存在する: 上書き
   - `name` が空文字: 設計上の想定次第（現実装は受け付ける）

#### ProfileLockfile::remove_profile
1. 目的と責務
   - プロファイルエントリを削除
2. アルゴリズム
   - `HashMap::remove(name)` を返す
3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| name | &str | 削除対象プロファイル名 |

4. 戻り値

| 型 | 意味 |
|----|------|
| Option<ProfileLockEntry> | 削除されたエントリ（存在しなければ None） |

5. 使用例
```rust
let removed = lockfile.remove_profile("foo");
assert!(removed.is_some());
```
6. エッジケース
   - 存在しない名前: `None`

#### ProfileLockfile::find_file_owner
1. 目的と責務
   - 指定ファイルパスを所有するプロファイル名を検索
2. アルゴリズム
   - `profiles` を走査し、各 `entry.files.contains(&file_path.to_string())` で一致チェック
   - 一致したらプロファイル名を `Some(name)` で返す
3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| file_path | &str | 検索対象のファイルパス |

4. 戻り値

| 型 | 意味 |
|----|------|
| Option<&str> | 所有者プロファイル名（存在しなければ None） |

5. 使用例
```rust
let owner = lockfile.find_file_owner("bin/foo");
assert_eq!(owner, Some("foo"));
```
6. エッジケース
   - 複数プロファイルに同一ファイルが listed（通常は想定外）: 最初に見つかったものを返す
   - パスの正規化/区切り文字差（Windows vs POSIX）: 現実装は文字列一致のみ
   - 性能: P×F の線形探索に加え、毎ループで `file_path.to_string()` するため余分な割り当てあり（改善余地）

## Walkthrough & Data Flow

- Load（読み込み）
  - 入力: `Path`
  - 分岐: 存在しない→空のロックファイルを返す。存在する→読み込み→JSONデコード→`ProfileLockfile`
  - 出力: `ProfileLockfile` インスタンス
- Save（保存）
  - 入力: `&ProfileLockfile`, `Path`
  - 親ディレクトリ作成→JSONエンコード（pretty）→ファイル書き込み
  - 出力: 成否
- Update/Query
  - `add_profile`: `HashMap` に挿入（上書き可）
  - `remove_profile`: `HashMap` から削除して返す
  - `is_installed`/`get_profile`: `HashMap` に対するクエリ
  - `find_file_owner`: `HashMap` 全走査→`Vec` 内検索

データはメモリ内では `HashMap<String, ProfileLockEntry>` として保持され、ディスクとの同期は `load`/`save` を明示的に呼ぶ設計。

## Complexity & Performance

- `new`: O(1) 時間・空間
- `load`: O(n) 時間（読み込み＋デコード）、O(n) 空間（デコードした構造体）
- `save`: O(n) 時間（エンコード＋書き込み）
- `is_installed`/`get_profile`/`add_profile`/`remove_profile`: 平均 O(1)、最悪（ハッシュ衝突）O(P)
- `find_file_owner`: O(P×F) 時間。さらに毎イテレーションで `file_path.to_string()` するため O(P×(|file_path| + F)) 相当の割り当てコスト

ボトルネック:
- 大規模な `profiles` と `files` の場合の `find_file_owner` 線形探索
- 非アトミックな `save` による再試行時のI/O負荷と破損リスク

スケール限界:
- 1つのロックファイルに大量のファイルを持つプロファイルが増えると検索の遅延が顕著
- 単一ファイルでの並行読み書きは未対処

実運用負荷要因:
- ディスクI/O（JSONの全量読み書き）
- JSON整形の計算コスト（`to_string_pretty`）

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:
- メモリ安全性: Rustの安全域のみ。Buffer overflow / Use-after-free / Integer overflow の懸念は現実装では低い
- インジェクション: SQL/Command なし。Pathは外部入力の可能性ありだがロックファイル自体はデータファイル。パス・トラバーサルは「どのパスを渡すか」の利用側の責務
- 認証・認可: なし。ファイルI/Oに対する権限チェックはOSに委譲
- 秘密情報: Hard-coded secrets なし。`integrity` はハッシュ値であり秘密ではない。ログ漏洩なし（ログ出力が存在しない）
- 並行性: ファイルの同時アクセス時の Race condition/部分書込み/破損の可能性（ファイルロック/アトミック書き込み未実装）

詳細なエッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ロックファイルなし | pathが存在しない | 新規ロックファイル返却 | `load` で `Ok(Self::new())` | 対応済み |
| JSON破損 | `{ invalid json }` | 明確なエラー返却 | `InvalidManifest` に変換 | 対応済み（原因詳細は失われる） |
| I/O権限なし | 読取不可/書込不可 | エラー返却 | `?` で伝播 | 対応済み |
| 親ディレクトリなし | `profiles/lock.json` | 親を作成して保存 | `create_dir_all` | 対応済み |
| 同時書込み | 複数プロセス/スレッドが `save` | 整合性維持 | 非アトミック書込み | 未対応（リスク） |
| 大量データ検索 | PやFが大 | 妥当な性能 | 線形探索＋余分な文字列割当 | 性能リスク |
| パスの正規化差 | `\` vs `/` | 正しく一致判定 | 文字列一致のみ | 未対応 |
| 空名プロファイル | `name = ""` | 拒否/検証 | 受理される | 未検証 |
| 重複ファイル | 同一ファイルが複数Entryに存在 | 明確なポリシー | 先に見つかったもの返却 | 未定義仕様 |

## Design & Architecture Suggestions

- 書込みのアトミック化: 一時ファイルに書いてから `rename`（POSIXではアトミック）することで破損リスクを低減
- ファイルロック: OSロック（例: `flock`/`fs2::FileExt`）やアプリ内ミューテックスで並行アクセス制御
- エラー詳細保持: `serde_json::Error` を `InvalidManifest` に包む際、`source` として内包し診断性を向上
- 検索性能改善: `find_file_owner` を以下のように修正
  - `file_path.to_string()` の毎回生成を避ける（事前に一度だけ `String` 化、または `iter().any(|f| f == file_path)` で &str比較）
  - 逆引きインデックス（`HashMap<String, String>`: file_path → profile_name）を維持する
- バージョン管理: `version` フィールドの意味づけ（スキーマ互換性のチェック/マイグレーション）
- 入力検証: `ProfileLockEntry.name` の空文字禁止などバリデーション追加
- パス正規化: OS差異を吸収する正規化（区切り文字、ケース、相対/絶対の扱い）

## Testing Strategy (Unit/Integration) with Examples

対象領域はファイルI/Oとデータモデルのシリアライズ/デシリアライズ。推奨テスト:

- ロード（存在しないパス）
```rust
#[test]
fn load_returns_new_when_path_missing() {
    use std::path::Path;
    let path = Path::new("nonexistent_dir/lock.json");
    let lf = ProfileLockfile::load(path).unwrap();
    assert_eq!(lf.version, "1.0.0");
    assert!(lf.profiles.is_empty());
}
```

- セーブ＆ロード往復
```rust
#[test]
fn save_and_reload_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("lock.json");

    let mut lf = ProfileLockfile::new();
    lf.add_profile(ProfileLockEntry {
        name: "foo".into(),
        version: "1.0.0".into(),
        installed_at: "2024-10-01T00:00:00Z".into(),
        files: vec!["bin/foo".into()],
        integrity: "".into(),
        commit: None,
        provider_id: Some("provider-x".into()),
        source: None,
    });
    lf.save(&path).unwrap();

    let lf2 = ProfileLockfile::load(&path).unwrap();
    assert!(lf2.is_installed("foo"));
    assert_eq!(lf2.get_profile("foo").unwrap().files, vec!["bin/foo"]);
}
```

- 後方互換フィールド（欠落時のデフォルト）
```rust
#[test]
fn deserialize_old_lockfile_defaults() {
    let json = r#"{
        "version": "1.0.0",
        "profiles": {
            "foo": {
                "name": "foo",
                "version": "1.0.0",
                "installed_at": "2024-10-01T00:00:00Z",
                "files": ["bin/foo"]
            }
        }
    }"#;
    let lf: ProfileLockfile = serde_json::from_str(json).unwrap();
    let e = lf.get_profile("foo").unwrap();
    assert_eq!(e.integrity, ""); // #[serde(default)]
    assert!(e.commit.is_none());
    assert!(e.provider_id.is_none());
    assert!(e.source.is_none());
}
```

- 所有者検索（一致/不一致）
```rust
#[test]
fn find_file_owner_works() {
    let mut lf = ProfileLockfile::new();
    lf.add_profile(ProfileLockEntry {
        name: "foo".into(),
        version: "1.0.0".into(),
        installed_at: "ts".into(),
        files: vec!["bin/foo".into(), "share/doc.txt".into()],
        integrity: "".into(),
        commit: None,
        provider_id: None,
        source: None,
    });
    assert_eq!(lf.find_file_owner("bin/foo"), Some("foo"));
    assert_eq!(lf.find_file_owner("bin/bar"), None);
}
```

- 削除・更新
```rust
#[test]
fn add_update_remove_profile() {
    let mut lf = ProfileLockfile::new();
    lf.add_profile(ProfileLockEntry {
        name: "foo".into(),
        version: "1.0.0".into(),
        installed_at: "ts".into(),
        files: vec![],
        integrity: "".into(),
        commit: None,
        provider_id: None,
        source: None,
    });
    assert!(lf.is_installed("foo"));
    let removed = lf.remove_profile("foo");
    assert!(removed.is_some());
    assert!(!lf.is_installed("foo"));
}
```

- 破損JSONのエラー（型は不明だが、`InvalidManifest` が返ることを期待）
```rust
#[test]
fn load_invalid_json_returns_invalid_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("lock.json");
    std::fs::write(&path, "{ invalid json }").unwrap();
    let err = ProfileLockfile::load(&path).unwrap_err();
    // 具象型は不明のため、文字列化などで検証する
    let msg = format!("{:?}", err);
    assert!(msg.contains("InvalidManifest"));
}
```

## Refactoring Plan & Best Practices

- `find_file_owner` の文字列割当削減
```rust
pub fn find_file_owner(&self, file_path: &str) -> Option<&str> {
    for (name, entry) in &self.profiles {
        if entry.files.iter().any(|f| f == file_path) {
            return Some(name);
        }
    }
    None
}
```
- アトミック保存
  - `let tmp = path.with_extension("json.tmp");`
  - `write(tmp, json)` → `rename(tmp, path)`
- エラー表現の改善
  - `serde_json::from_str(&content).map_err(|e| ProfileError::InvalidManifest { reason: format!("Lockfile is corrupted: {e}") })`
- バリデーションAPI追加
  - `fn validate(&self) -> ProfileResult<()>`（名前、重複ファイル、空フィールド、バージョン整合性など）
- 逆引きインデックスの維持（大量検索対策）
  - `HashMap<String, String>` を内部キャッシュ（同期責務を明確化）
- フィールドの意味づけ
  - `installed_at` の形式（ISO 8601）をドキュメント化しバリデーション

## Observability (Logging, Metrics, Tracing)

- ログ（tracing）
  - `load`/`save` の開始/成功/失敗ログ（`debug`/`info`/`error`）
  - `InvalidManifest` の詳細を `error` で出力
- メトリクス
  - ロックファイルサイズ（バイト）
  - プロファイル数、ファイル数合計
  - 保存/読み込みのレイテンシ
- トレーシング
  - `#[instrument]`（関数境界でのスパン）
  - 失敗時に `error` フィールドとして `reason` を記録

例:
```rust
#[tracing::instrument(level = "info", skip(self))]
pub fn save(&self, path: &Path) -> ProfileResult<()> {
    // ...
}
```

## Risks & Unknowns

- `ProfileResult`/`ProfileError` の具体的なエラーハンドリングポリシーは不明（このチャンクには現れない）
- `ProviderSource` のバリアントや序列は不明（このチャンクには現れない）
- `version` フィールドの運用（互換性チェック/マイグレーションフロー）は不明
- 並行アクセスの要件（単一プロセス内のみ想定か、複数プロセス/スレッド想定か）は不明
- パス表記の標準化（OS差/相対・絶対・ケース感度）の運用ルールは不明

以上の不明点は設計全体のコンテキスト次第で方針が変わるため、利用側のガイドライン整備が望まれます。