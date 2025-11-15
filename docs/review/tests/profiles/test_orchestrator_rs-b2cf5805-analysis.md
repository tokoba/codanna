# profiles\test_orchestrator.rs Review

## TL;DR

- 目的: プロファイルのインストールオーケストレーションに関する統合テストを実行し、**ファイル配置**、**ロックファイル生成**、**重複インストール検出**、**強制更新**、**衝突時サイドカー作成**、**整合性ハッシュ計算**を検証する。
- 主要公開API: **install_profile**、**ProfileLockfile::load**、**ProfileLockfile::get_profile**、エラー型 **ProfileError::AlreadyInstalled**（テストで確認）。
- 複雑箇所: 同一ファイル名の競合時の挙動（強制でも上書きせず、**サイドカー**を作成する）、**SHA-256整合性**の計算・保存。
- 重大リスク: 3つの**Option引数**の意味が不明（このチャンクには現れない）、**manifestバリデーション**や**パス・トラバーサル**防止のテスト欠如、**同時実行**時の整合性/ロック未検証。
- パフォーマンス: ボトルネックは**ファイルI/O**と**ハッシュ計算**。ファイル数・サイズに比例して処理時間増加。
- セキュリティ: **パス・トラバーサル**、**原子的な書き込み**の欠如によるロックファイル破損の可能性、**権限エラー**や**競合**時の整合性未検証。

## Overview & Purpose

このファイルは、codanna プロジェクトの「プロファイルインストール」機能に対するテスト群であり、ワークスペースにプロファイルが正しくインストールされること、ロックファイルが生成・更新されること、重複インストール・強制再インストール・ファイル名衝突・整合性計算など主要なユースケースを検証する。各テストは一時ディレクトリ（tempdir）を用いて孤立環境を構築し、ローカルファイル操作を通じて期待動作をアサートする。

主な検証項目（テスト名:行番号不明）:
- test_install_profile_creates_structure: 生成物（ファイル/ロックファイル）とロックファイルへのプロフィール登録を確認。
- test_install_profile_not_found: 存在しないプロファイル指定時にエラー。
- test_install_profile_updates_manifest: 空filesでもロックファイルに名前/バージョンが記録される。
- test_install_profile_already_installed: 2回目の非forceインストールで AlreadyInstalled エラー。
- test_install_profile_with_force: force時にソース変更がワークスペースへ反映。
- test_install_profile_calculates_integrity: 整合性（SHA-256 64文字hex）が記録される。
- test_install_profile_conflict_creates_sidecar: 競合時にサイドカーファイル（例: CLAUDE.profile-b.md）を作成し、既存ファイルは保持。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_install_profile_creates_structure | test (非pub) | インストール後のファイル配置とロックファイル作成検証 | Low |
| Function | test_install_profile_not_found | test (非pub) | 存在しないプロファイル指定時のエラー検証 | Low |
| Function | test_install_profile_updates_manifest | test (非pub) | 空filesでもロックファイルにメタ情報が入ることを検証 | Low |
| Function | test_install_profile_already_installed | test (非pub) | 非forceでの重複インストール検出（AlreadyInstalled）検証 | Med |
| Function | test_install_profile_with_force | test (非pub) | force指定時の上書き挙動検証 | Med |
| Function | test_install_profile_calculates_integrity | test (非pub) | 整合性ハッシュ（SHA-256 hex 64文字）の計算・保存検証 | Med |
| Function | test_install_profile_conflict_creates_sidecar | test (非pub) | 同名ファイルの競合時にサイドカー作成と既存保持の検証 | Med |

### Dependencies & Interactions

- 内部依存: テスト関数同士の直接コールはなし。共通パターン（tempdir/セットアップ/インストール/検証）が繰り返し出現。
- 外部依存（使用モジュール/クレート）

| 依存 | 用途 | 備考 |
|-----|------|------|
| codanna::profiles::orchestrator::install_profile | コアAPI呼び出し | 戻り値は Result<_, ProfileError> と推定（テスト使用から）。引数の末尾3つの Option 型は不明 |
| codanna::profiles::lockfile::ProfileLockfile | ロックファイルのロードと照会 | load(&Path), get_profile(&str) 使用 |
| codanna::profiles::error::ProfileError | エラー型・パターンマッチ | AlreadyInstalled { name, version } をテストで使用 |
| std::fs | ファイル/ディレクトリ操作 | create_dir_all, write, read_to_string, exists |
| tempfile::tempdir | 一時ディレクトリ生成 | テストの隔離実行 |
| std::path（Path/PathBuf） | パス結合 | join を多用 |

- 被依存推定: このテストは「プロファイル・オーケストレータ」機能を検証しているため、CLI（例: codanna コマンド）、他の管理ツール、IDE連携等が install_profile を利用する可能性が高い（このチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

本ファイル自体に公開APIはないが、テストが利用している公開APIとデータ契約を優先して整理する。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| install_profile | fn install_profile(name: &str, profiles_dir: &Path, workspace: &Path, force: bool, opt_a: Option<T1>, opt_b: Option<T2>, opt_c: Option<T3>) -> Result<(), ProfileError> | プロファイルをワークスペースへインストールし、ロックファイル更新 | O(F + Σsize + L) | O(Σbuffer) |
| ProfileLockfile::load | fn load(path: &Path) -> Result<ProfileLockfile, E> | ロックファイルの読み込み | O(L) | O(L) |
| ProfileLockfile::get_profile | fn get_profile(name: &str) -> Option<ProfileEntry> | ロックファイルからプロファイルエントリ取得 | O(1)〜O(P)（実装次第） | O(1) |
| ProfileError::AlreadyInstalled | enum variant AlreadyInstalled { name: String, version: String } | 既にインストール済みであることを示すエラー | - | - |

注:
- T1/T2/T3 はこのチャンクでは不明（Option引数の意味・型が現れない）。
- L はロックファイルのサイズ/エントリ数、F はファイル数。

### install_profile

1) 目的と責務
- 指定されたプロファイル名に対応するプロファイルディレクトリから manifest（profile.json）とファイル群を読み込み、ワークスペースにファイルを配置し、ロックファイル（.codanna/profiles.lock.json）を更新する。
- 既に同プロファイルがインストール済みの場合は **force** が false なら **ProfileError::AlreadyInstalled** を返す。
- 同名ファイルの競合時は **force** でも既存を保持し、新規はサイドカー（例: filename.profile-name.ext）に出力する（test_install_profile_conflict_creates_sidecar:行番号不明）。
- 整合性（SHA-256）を計算し、ロックファイルへ保存する（test_install_profile_calculates_integrity:行番号不明）。

2) アルゴリズム（テストからの推測）
- プロファイルディレクトリ（profiles_dir/name/）を探索し、manifest（profile.json）を読み取る。
- manifestの name/version/files を解釈。
- ロックファイルを workspace/.codanna/profiles.lock.json に作成または更新。
- 既インストール判定:
  - 既にロックファイルに同名エントリがある場合、force=falseなら ProfileError::AlreadyInstalled。
- ファイルコピー:
  - files に列挙された各ファイルについて、workspace直下へコピー。
  - 競合（既に存在し、別プロファイルのもの）時は、既存を保持し、インストール対象はサイドカー名（例: CLAUDE.profile-b.md）で出力。
- 整合性計算:
  - コピーしたコンテンツの SHA-256 を計算し、hex 64文字でロックファイルに保存。
- 終了。Result<(), ProfileError> を返す（test使用からの推測）。

3) 引数

| 引数 | 型 | 説明 |
|------|----|------|
| name | &str | プロファイル名 |
| profiles_dir | &Path | プロファイル群の格納ディレクトリ |
| workspace | &Path | インストール先ワークスペース |
| force | bool | 強制再インストール/競合時のポリシー |
| opt_a | Option<T1> | 不明（このチャンクには現れない） |
| opt_b | Option<T2> | 不明（このチャンクには現れない） |
| opt_c | Option<T3> | 不明（このチャンクには現れない） |

4) 戻り値

| 返り値 | 型 | 説明 |
|--------|----|------|
| Ok | ()（推測） | インストール成功 |
| Err | ProfileError | 失敗（例: AlreadyInstalled, NotFound, InvalidManifest などは推測） |

5) 使用例

```rust
use std::fs;
use tempfile::tempdir;
use codanna::profiles::orchestrator::install_profile;

let temp = tempdir()?;
let profiles_dir = temp.path().join("profiles");
let workspace = temp.path().join("workspace");
fs::create_dir_all(&profiles_dir)?;
fs::create_dir_all(&workspace)?;

// プロファイル作成（manifestやファイル書き込みは省略）
install_profile("claude", &profiles_dir, &workspace, false, None, None, None)?;
```

6) エッジケース
- プロファイルが存在しない場合: Err（test_install_profile_not_found:行番号不明）
- 既インストールかつ force=false: Err(AlreadyInstalled)（test_install_profile_already_installed:行番号不明）
- force=true: ファイル更新（test_install_profile_with_force:行番号不明）
- 同名ファイルの競合時: サイドカー作成（test_install_profile_conflict_creates_sidecar:行番号不明）
- filesが空: ロックファイルのみ更新（test_install_profile_updates_manifest:行番号不明）
- 整合性ハッシュ（64文字hex）記録（test_install_profile_calculates_integrity:行番号不明）

### ProfileLockfile::load

1) 目的と責務
- ワークスペースのロックファイルを読み込んでメモリ上の構造体へ変換。

2) アルゴリズム（推測）
- JSONをパースし、エントリ辞書/配列を構築。

3) 引数

| 引数 | 型 | 説明 |
|------|----|------|
| path | &Path | ロックファイルの場所（workspace/.codanna/profiles.lock.json） |

4) 戻り値

| 返り値 | 型 | 説明 |
|--------|----|------|
| Ok | ProfileLockfile | ロード成功 |
| Err | E（不明） | パース/IO失敗 |

5) 使用例

```rust
use codanna::profiles::lockfile::ProfileLockfile;

let lockfile_path = workspace.join(".codanna/profiles.lock.json");
let lockfile = ProfileLockfile::load(&lockfile_path)?;
```

6) エッジケース
- ロックファイルが存在しない/壊れている: Err（このチャンクには現れない）

### ProfileLockfile::get_profile

1) 目的と責務
- 指定名のプロファイルエントリを返す。

2) アルゴリズム（推測）
- 名前キーで検索し、存在すれば返す。

3) 引数

| 引数 | 型 | 説明 |
|------|----|------|
| name | &str | プロファイル名 |

4) 戻り値

| 返り値 | 型 | 説明 |
|--------|----|------|
| Some | ProfileEntry | name, version, integrity など（integrityは64文字hexを確認済み） |
| None | - | 未登録 |

5) 使用例

```rust
let entry = lockfile.get_profile("claude").unwrap();
assert_eq!(entry.name, "claude");
assert_eq!(entry.version, "1.0.0");
// 整合性はSHA-256 hexの64文字
assert!(!entry.integrity.is_empty());
assert_eq!(entry.integrity.len(), 64);
```

6) エッジケース
- 未登録: None（test_install_profile_creates_structure:行番号不明にて is_some() の検証あり）

### ProfileError::AlreadyInstalled

1) 目的と責務
- 非forceで既に同プロファイルがインストール済みであることを通知。

2) データ契約
- フィールド: name: String, version: String（テストで一致検証）

3) 使用例

```rust
use codanna::profiles::error::ProfileError;

let result = install_profile("claude", &profiles_dir, &workspace, false, None, None, None);
match result {
    Err(ProfileError::AlreadyInstalled { name, version }) => {
        assert_eq!(name, "claude");
        assert_eq!(version, "1.0.0");
    }
    _ => panic!("Expected AlreadyInstalled error"),
}
```

## Walkthrough & Data Flow

- 一般フロー（test_install_profile_creates_structure:行番号不明を基に推測）
  - tempdirで隔離環境を作成。
  - profiles/<name>/ に manifest（profile.json）とファイルを用意。
  - workspace を作成。
  - install_profile を実行。
  - workspaceにファイルがコピーされる。
  - workspace/.codanna/profiles.lock.json が生成され、プロファイルエントリ（name, version, integrityなど）が追加される。

- エラーケース（test_install_profile_not_found:行番号不明）
  - profilesディレクトリに該当プロファイルがない場合、Err となる。

- 再インストール（test_install_profile_already_installed:行番号不明）
  - 1度インストール後、非forceで再度インストールすると AlreadyInstalled の Err を返す。

- 強制更新（test_install_profile_with_force:行番号不明）
  - ソースファイル更新後、force=true で再インストールすると workspace のファイル内容が更新される。

- 整合性記録（test_install_profile_calculates_integrity:行番号不明）
  - ロックファイルのエントリに 64文字の SHA-256 hex を保持。

- 競合解決（test_install_profile_conflict_creates_sidecar:行番号不明）
  - 異なるプロファイルが同名ファイルを配布する場合、先インストールの内容は保持され、後インストール分は「filename.profile-name.ext」のサイドカーに出力される。

🧪 これらのフローはすべてテストで観測された結果を根拠にしているが、内部実装の分岐・詳細はこのチャンクには現れないため推測を含む。

## Complexity & Performance

- install_profile
  - 時間計算量: O(F + Σsize + L)
    - F: ファイル数（メタデータ/コピー）
    - Σsize: コピー・ハッシュ計算対象の総バイト数
    - L: ロックファイルの読み書き/パース
  - 空間計算量: O(Σbuffer)（コピー・ハッシュのためのバッファ）
- 主なボトルネック
  - ファイルI/O（大量/大容量ファイル）
  - SHA-256計算のCPU負荷（大容量ファイル）
  - ロックファイルの多エントリ時のパース/シリアライズ
- スケール限界・運用要因
  - ネットワークI/OやDBは関与せずローカルFS中心（このチャンクには現れない）
  - 大量プロファイル・大容量ファイルで所要時間増加
  - 同時実行時のロックファイル更新競合（このチャンクには現れない）

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| プロファイル未存在 | name="nonexistent" | Err(NotFound系) | テストで Err を確認 | テスト済み |
| 既インストール・非force | 2回目のインストール、force=false | Err(AlreadyInstalled) | テストでパターンマッチ検証 | テスト済み |
| 強制更新 | 2回目、force=true | ファイルが更新される | テストで内容変更を確認 | テスト済み |
| 競合（同名ファイル） | A→B（force=true） | 既存保持＋Bはサイドカー出力 | テストでファイル保持＋サイドカー作成確認 | テスト済み |
| filesが空 | manifest.files=[] | ロックファイルにname/versionが記録 | テストで確認 | テスト済み |
| manifest欠落/不正JSON | 不正なprofile.json | Err(InvalidManifest系) | このチャンクには現れない | 未テスト |
| filesに存在しないファイル | "missing.txt" | Err(FileNotFound系) | このチャンクには現れない | 未テスト |
| パス・トラバーサル | "../evil" | 拒否（sandbox化） | このチャンクには現れない | 未テスト |
| ロックファイル破損 | 壊れたJSON | 自動修復orErr | このチャンクには現れない | 未テスト |
| サイドカー名の衝突 | 既に .profile-b.md が存在 | 一意な命名 or 上書きポリシー | このチャンクには現れない | 未テスト |
| 大容量ファイル | >1GB | ストリーミング/chunking | このチャンクには現れない | 未テスト |
| 非ASCII名前 | "プロファイル.md" | 正常処理（UTF-8） | このチャンクには現れない | 未テスト |
| Windows/Unix差異 | 改行/パス区切り | クロスプラ対応 | このチャンクには現れない | 未テスト |
| 同時実行レース | 並行install_profile | ロック/原子的更新 | このチャンクには現れない | 未テスト |

🔐 セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: テストコードは標準ライブラリ利用のみで unsafe なし。現時点で問題なし（このチャンクには現れない）。
- インジェクション
  - SQL/Command: 該当なし。
  - Path traversal: manifest.files の相対パスに対する検査が未確認。防止が必要（未テスト）。
- 認証・認可
  - ローカル操作のみで該当なし。権限（ファイルアクセス権）エラーはハンドリング必要（未テスト）。
- 秘密情報
  - ハードコード秘密情報なし。ログ出力時の内容漏洩に注意（このチャンクには現れない）。
- 並行性
  - Race condition / Deadlock: ロックファイル更新の同時実行レース未検証。ファイルロックや原子的な書き込みが望ましい（未テスト）。

Rust特有の観点
- 所有権/借用: install_profile へ &Path を借用で渡しており、ライフタイムはテスト内で完結（各test_fn:行番号不明）。問題なし。
- unsafe境界: 本テストコードに unsafe は存在しない（ファイル全体）。
- 並行性・非同期: 非同期/Send/Sync 境界はこのチャンクには現れない。
- エラー設計: unwrap/expect をテストで使用しているが、失敗時にテストが明確に落ちる性質上妥当。実運用コードでは適切なエラーハンドリングを推奨。

## Design & Architecture Suggestions

- 設定オブジェクト導入
  - install_profile の末尾3つの Option 引数の意味が不明。**構成体（Config struct）**にまとめ、明示的フィールド名・デフォルト値を持たせることで可読性と型安全性を向上。
- 原子的な更新
  - ロックファイル更新・ファイルコピーは**一時ファイル→rename**による原子的操作にすることで、異常終了時の破損を防止。
- 競合ポリシーの明文化
  - サイドカー命名規則（filename.profile-name.ext）を仕様化し、衝突時の挙動（連番付与など）を定義。
- 整合性/改ざん検出
  - ロックファイルへ各ファイルの**ハッシュ詳細（ファイルごとの hash）**も記録し、変更検出・検証を容易に。
- スキーマバージョニング
  - manifest（profile.json）/lockfile に**schema_version**を導入し、後方互換性管理。
- 監査ログ
  - 何をコピーし、何を更新/スキップ/サイドカー出力したかを**構造化ログ**で記録。
- バリデーション
  - manifest の**必須フィールド**と**filesの正当性（相対パス、ルート越え禁止）**を厳格にチェック。

## Testing Strategy (Unit/Integration) with Examples

- 追加ユニット/統合テスト案
  - manifest不正/欠落、filesに不存在ファイル、パス・トラバーサル、サイドカー衝突、ロックファイル破損、同時実行、巨大ファイルなど。

例1: 不正manifest

```rust
#[test]
fn test_install_profile_invalid_manifest() {
    use std::fs;
    use tempfile::tempdir;
    use codanna::profiles::orchestrator::install_profile;

    let temp = tempdir().unwrap();
    let profiles_dir = temp.path().join("profiles");
    let p = profiles_dir.join("bad");
    fs::create_dir_all(&p).unwrap();
    fs::write(p.join("profile.json"), "{ invalid json ").unwrap();

    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let result = install_profile("bad", &profiles_dir, &workspace, false, None, None, None);
    assert!(result.is_err(), "invalid manifest should error");
}
```

例2: filesに不存在ファイル

```rust
#[test]
fn test_install_profile_missing_file_in_manifest() {
    use std::fs;
    use tempfile::tempdir;
    use codanna::profiles::orchestrator::install_profile;

    let temp = tempdir().unwrap();
    let profiles_dir = temp.path().join("profiles");
    let p = profiles_dir.join("x");
    fs::create_dir_all(&p).unwrap();
    fs::write(
        p.join("profile.json"),
        r#"{"name":"x","version":"1.0.0","files":["MISSING.txt"]}"#,
    ).unwrap();

    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let result = install_profile("x", &profiles_dir, &workspace, false, None, None, None);
    assert!(result.is_err(), "missing file should cause error");
}
```

例3: パス・トラバーサル防止

```rust
#[test]
fn test_install_profile_prevents_path_traversal() {
    use std::fs;
    use tempfile::tempdir;
    use codanna::profiles::orchestrator::install_profile;

    let temp = tempdir().unwrap();
    let profiles_dir = temp.path().join("profiles");
    let p = profiles_dir.join("safe");
    fs::create_dir_all(&p).unwrap();
    fs::write(
        p.join("profile.json"),
        r#"{"name":"safe","version":"1.0.0","files":["../outside.txt"]}"#,
    ).unwrap();

    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let result = install_profile("safe", &profiles_dir, &workspace, false, None, None, None);
    assert!(result.is_err(), "path traversal should be rejected");
}
```

例4: サイドカー衝突時の一意化

```rust
#[test]
fn test_sidecar_name_collision_is_resolved() {
    use std::fs;
    use tempfile::tempdir;
    use codanna::profiles::orchestrator::install_profile;

    let temp = tempdir().unwrap();
    let profiles_dir = temp.path().join("profiles");

    // A
    let a = profiles_dir.join("A");
    fs::create_dir_all(&a).unwrap();
    fs::write(a.join("profile.json"), r#"{"name":"A","version":"1.0.0","files":["F.md"]}"#).unwrap();
    fs::write(a.join("F.md"), "A").unwrap();

    // B
    let b = profiles_dir.join("B");
    fs::create_dir_all(&b).unwrap();
    fs::write(b.join("profile.json"), r#"{"name":"B","version":"1.0.0","files":["F.md"]}"#).unwrap();
    fs::write(b.join("F.md"), "B").unwrap();

    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    install_profile("A", &profiles_dir, &workspace, false, None, None, None).unwrap();
    // 既にサイドカー名を作成済みにして衝突させる
    fs::write(workspace.join("F.B.md"), "existing").unwrap();

    install_profile("B", &profiles_dir, &workspace, true, None, None, None).unwrap();

    // 一意化された名前で出力されたか（具体名は実装次第）
    assert!(workspace.read_dir().unwrap().any(|e| {
        let p = e.unwrap().path();
        p.file_name().unwrap().to_string_lossy().starts_with("F.B")
    }));
}
```

例5: 同時実行（レース耐性）

```rust
#[test]
fn test_install_profile_concurrent_race() {
    use std::{fs, thread};
    use tempfile::tempdir;
    use codanna::profiles::orchestrator::install_profile;

    let temp = tempdir().unwrap();
    let profiles_dir = temp.path().join("profiles");
    let p = profiles_dir.join("c");
    fs::create_dir_all(&p).unwrap();
    fs::write(p.join("profile.json"), r#"{"name":"c","version":"1.0.0","files":["X.md"]}"#).unwrap();
    fs::write(p.join("X.md"), "X").unwrap();
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let handles: Vec<_> = (0..4).map(|_| {
        let profiles_dir = profiles_dir.clone();
        let workspace = workspace.clone();
        thread::spawn(move || {
            let _ = install_profile("c", &profiles_dir, &workspace, true, None, None, None);
        })
    }).collect();

    for h in handles { let _ = h.join(); }

    // lockfileが破損していないかの検証（loadが成功すること）
    let lockfile_path = workspace.join(".codanna/profiles.lock.json");
    assert!(lockfile_path.exists());
    let _ = codanna::profiles::lockfile::ProfileLockfile::load(&lockfile_path).unwrap();
}
```

## Refactoring Plan & Best Practices

- テストの重複排除
  - プロファイル/ワークスペース作成をヘルパー関数に集約し、可読性を向上。

例: ヘルパー

```rust
use std::fs;
use std::path::Path;

fn write_manifest(dir: &Path, name: &str, version: &str, files: &[&str]) {
    let json = format!(
        r#"{{"name":"{name}","version":"{version}","files":[{}]}}"#,
        files.iter().map(|f| format!(r#""{}""#, f)).collect::<Vec<_>>().join(",")
    );
    fs::write(dir.join("profile.json"), json).unwrap();
}

fn create_profile(root: &Path, name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    write_manifest(&dir, name, "1.0.0", &files.iter().map(|(f, _)| *f).collect::<Vec<_>>());
    for (file, content) in files {
        fs::write(dir.join(file), content).unwrap();
    }
    dir
}
```

- アサーションの明確化
  - unwrap() は expect("理由") に変更し、失敗時の原因を明示。
- 定数化
  - ロックファイルパス（.codanna/profiles.lock.json）を定数で管理。
- パラメータの型安全
  - install_profile の Option群は型の不明確さが可読性を低下させるため型名を公開し、テストで具体値も検証。

## Observability (Logging, Metrics, Tracing)

- ログ
  - install_profile の主要イベント（manifest読取、既存判定、コピー、サイドカー出力、ロックファイル更新、整合性計算）を **structured logging** で記録。
- メトリクス
  - コピーしたファイル数/バイト数、整合性計算時間、衝突回数、再インストール回数などをカウンタ/ヒストグラムで収集（このチャンクには現れない）。
- トレーシング
  - spanでプロファイル名・workspace・処理フェーズを紐付け、問題発生時に追跡容易に。
- テストでのログ検証
  - tracing_subscriber を使い、期待ログパターンをキャプチャして検証（実装はこのチャンクには現れない）。

## Risks & Unknowns

- 不明点
  - install_profile の末尾3つの Option 引数の**型と意味**（このチャンクには現れない）。
  - manifest/lockfile の**詳細スキーマ**（このチャンクには現れない）。
  - **エラー種類**（AlreadyInstalled 以外の具体バリアント）（このチャンクには現れない）。
  - **同時実行**時のロック戦略・原子的更新の有無（このチャンクには現れない）。
  - **サイドカー命名**の厳密ルール（このチャンクには現れない）。
- リスク
  - パス・トラバーサルによる外部ファイルアクセス。
  - ロックファイル書き込み中断による破損。
  - 大量/大容量ファイル処理時のパフォーマンス劣化。
  - 異常系（権限/ディスク満杯/IO障害）での部分的インストールに伴う整合性欠如。

以上のとおり、本テストファイルは install_profile の主要挙動に対する有用な統合検証を提供しているが、入力バリデーション、エラーバリアント多様性、同時実行、原子的更新、セキュリティ観点などについてのテスト強化余地がある。