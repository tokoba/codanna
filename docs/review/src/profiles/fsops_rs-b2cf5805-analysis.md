# fsops.rs Review

## TL;DR

- 目的: プロファイル（プラグイン）インストールに関する**ファイルシステム処理**（収集・コピー・削除・整合性ハッシュ計算・バックアップ/ロールバック）を提供
- 主要公開API: **calculate_integrity**, **collect_all_files**, **remove_profile_files**, **copy_profile_files**, **backup_profile**, **restore_profile**, **ProfileBackup**
- 複雑箇所: **collect_all_files**のフィルタリング分岐（.git除外、manifest除外）、**copy_profile_files**の競合検知と強制上書きフロー
- 重大リスク: 入力ファイルリストの未検証により**パストラバーサル（../）**が発生し得る、**TOCTOU**（存在確認→コピー）による競合レース、**バックアップの全メモリ展開**によるメモリ圧迫
- Rust安全性: **unsafeなし**、基本は所有権/借用に従った安全なI/O。ただし大容量ファイル読み込み（fs::read）に伴うメモリ消費は注意
- テスト: 単体テストが充実（整合性ハッシュ、コピー、削除、バックアップ/リストア）。追加すべきは**../混入**、シンボリックリンク、競合の同時アクセス等
- パフォーマンス: I/O主導。整合性ハッシュとバックアップの**バイト列全読み**がボトルネック。ストリーミング化を推奨

## Overview & Purpose

このモジュールは、プロファイル（プラグイン）のインストール/アンインストール/ロールバックのための**ファイルシステム操作**をまとめています。ディレクトリ内のファイル収集、実ファイルのコピー、不要ファイルの削除、整合性ハッシュの計算、既存インストールのバックアップと復元を1箇所に集約し、上位ロジック（例: プラグイン管理フロー）から再利用可能にしています。

- エラーは**ProfileError / ProfileResult**（super::error）によりモジュール境界で統一
- ロックファイルエントリ**ProfileLockEntry**（super::lockfile）と連携して、バックアップと復元で追跡されたファイルを取り扱う
- 外部依存は**sha2**（ハッシュ計算）と**walkdir**（再帰ディレクトリ走査）

用途の想定フロー（典型例）:
1. collect_all_filesでプロファイルパッケージからコピー対象の相対パス一覧を生成
2. copy_profile_filesでワークスペースへコピー（競合検知/force上書き）
3. calculate_integrityでインストールされた内容の**整合性ハッシュ**を記録
4. アップデート前にbackup_profileで既存状態のバックアップを作成し、失敗時restore_profileで復元
5. remove_profile_filesで不要ファイルを削除

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| fn | calculate_integrity | pub | 複数ファイルの内容からSHA-256整合性ハッシュを生成 | Low |
| fn | collect_all_files | pub | ディレクトリから再帰的にファイルリスト（相対パス）を収集（.gitとmanifest除外） | Med |
| fn | remove_profile_files | pub | 指定ファイル群の安全な削除と空ディレクトリのクリーンアップ | Low |
| fn | copy_profile_files | pub | 競合検知付きファイルコピー（force上書き、所有者問い合わせ） | Med |
| struct | ProfileBackup | pub | ロックファイルエントリと、対象ファイルの内容バックアップを保持 | Low |
| fn | backup_profile | pub | ロックファイル記録に基づいてワークスペースの実ファイルをメモリに退避 | Med |
| fn | restore_profile | pub | バックアップから元のファイル群を復元（親ディレクトリ作成） | Low |

### Dependencies & Interactions

- 内部依存
  - backup_profile → ProfileBackupを作成（restore_profileの入力）
  - restore_profile → ProfileBackupのfilesを走査して復元
  - copy_profile_files → conflict_owner（呼び出し元提供の関数/クロージャ）を呼び出し
  - calculate_integrity / collect_all_files / remove_profile_files → 独立ユーティリティ
- 外部依存（クレート/モジュール）

| 依存 | 用途 | 備考 |
|-----|------|------|
| sha2::{Digest, Sha256} | 整合性ハッシュ計算 | バイナリ内容をハッシュ化、区切りに改行 |
| walkdir::WalkDir | 再帰ディレクトリ走査 | .gitとmanifest除外判定に使用 |
| std::fs / std::path | ファイル読み書き・ディレクトリ操作・パス結合 | すべて同期I/O |
| super::error::{ProfileError, ProfileResult} | エラー型/結果型 | IoError等へ変換 |
| super::lockfile::ProfileLockEntry | 設定/ロック情報 | backup対象ファイル一覧の基礎 |

- 被依存推定
  - プロファイルインストール/更新/削除を行う上位モジュール（例: plugins/mod.rs）からの利用
  - CLIやサービス層からの「適用」「ロールバック」コマンド実装

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| calculate_integrity | `pub fn calculate_integrity(file_paths: &[String]) -> ProfileResult<String>` | 複数ファイル内容の連結（改行区切り）をSHA-256でハッシュ化 | O(Σサイズ) | O(最大ファイルサイズ) |
| collect_all_files | `pub fn collect_all_files(profile_dir: &Path) -> ProfileResult<Vec<String>>` | ディレクトリ配下のファイル相対パス一覧を収集（.git/manifest除外） | O(項目数) | O(ファイル数) |
| remove_profile_files | `pub fn remove_profile_files(file_list: &[String]) -> ProfileResult<()>` | ファイル削除と空親ディレクトリ削除 | O(n) | O(1) |
| copy_profile_files | `pub fn copy_profile_files(source_dir: &Path, dest_dir: &Path, file_list: &[String], force: bool, conflict_owner: impl Fn(&Path) -> Option<String>) -> ProfileResult<Vec<String>>` | 競合検知付きのコピー。成功したファイルの絶対パス一覧を返す | O(n + Σサイズ) | O(n) |
| ProfileBackup | `#[derive(Debug, Clone)] pub struct ProfileBackup { pub entry: ProfileLockEntry, pub files: Vec<(PathBuf, Vec<u8>)> }` | バックアップデータコンテナ | - | O(Σサイズ) |
| backup_profile | `pub fn backup_profile(workspace: &Path, entry: &ProfileLockEntry) -> ProfileResult<ProfileBackup>` | ロックファイル記録に基づき実ファイルを読み込みメモリ退避 | O(n + Σサイズ) | O(Σサイズ) |
| restore_profile | `pub fn restore_profile(backup: &ProfileBackup) -> ProfileResult<()>` | バックアップから書き戻し（親ディレクトリ作成） | O(n + Σサイズ) | O(1) |

以下、各APIの詳細。

### calculate_integrity

1) 目的と責務
- 指定されたファイル群の内容を順序通りにハッシュ化し、**整合性検証**用のSHA-256十六進文字列を返す
- 存在しないファイルは*スキップ*

2) アルゴリズム（ステップ分解）
- Sha256ハッシャを初期化
- file_pathsを順序通りに反復
  - Path.exists()ならfs::readで内容取得（全読み）
  - hasher.update(内容) → hasher.update(b"\n")で区切りを追加
- finalizeし、hex文字列に整形

3) 引数

| 名称 | 型 | 説明 |
|------|----|------|
| file_paths | &[String] | 絶対/相対を含む可能性。順序はハッシュに影響 |

4) 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<String> | 64桁のhex文字列（SHA-256）。I/O失敗はErr |

5) 使用例
```rust
use std::fs;
use std::path::Path;

let files = vec!["a.txt".into(), "b.txt".into()];
fs::write(Path::new("a.txt"), "A")?;
fs::write(Path::new("b.txt"), "B")?;
let integrity = calculate_integrity(&files)?;
println!("{}", integrity); // 64桁のハッシュ
```

6) エッジケース
- 空の入力: 空ベクトル→ハッシュはSha256("")に改行なしの結果
- 不存在ファイル: スキップ（テストあり）
- 大容量ファイル: fs::readの全読みでメモリ圧迫
- 順序の違い: ハッシュが変わる（テストあり）
- バイナリファイル: 問題なし（生バイトをハッシュ）

根拠: calculate_integrity（行番号不明、このチャンクには行番号情報がない）

### collect_all_files

1) 目的と責務
- 指定ディレクトリ以下のすべての**ファイル相対パス**を収集
- **ディレクトリ・.git配下・profile.json**は除外

2) アルゴリズム
- WalkDirで再帰走査
- entryがディレクトリならcontinue
- profile_dirに対する相対パスへstrip_prefix
- コンポーネントに「.git」を含むなら除外
- 正規化（'\' → '/'）後、"profile.json"は除外
- 残りをVecにpush

3) 引数

| 名称 | 型 | 説明 |
|------|----|------|
| profile_dir | &Path | 収集の起点ディレクトリ |

4) 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<Vec<String>> | 相対パス文字列（'/'区切り）一覧 |

5) 使用例
```rust
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

let root = Path::new("profile");
fs::create_dir_all(root.join(".git"))?;
fs::write(root.join("file.txt"), "data")?;
fs::write(root.join("profile.json"), "{}")?;
let files = collect_all_files(root)?;
assert_eq!(files, vec!["file.txt"]);
```

6) エッジケース
- シンボリックリンク: WalkDirの設定デフォルト依存（本コードにリンク追跡指定なし）
- Windowsパス区切り: 正規化で'/'に統一
- .git検出: コンポーネント比較で隠しディレクトリも除外
- strip_prefix前提: entryがprofile_dir配下であることを期待し、expectでpanicする可能性はあるがwalkdirは配下を返す想定
- 読み取り権限なしファイル: WalkDirのErrorをIoError(other(e))へ変換

根拠: collect_all_files（行番号不明）

Mermaid（主要分岐）
```mermaid
flowchart TD
  A[WalkDirでentry取得] --> B{entry.is_dir()?}
  B -- Yes --> A
  B -- No --> C[relative = path.strip_prefix(profile_dir)]
  C --> D{relativeに .git を含む?}
  D -- Yes --> A
  D -- No --> E[normalized = relative.to_string_lossy().replace('\\','/')]
  E --> F{normalized == "profile.json"?}
  F -- Yes --> A
  F -- No --> G[files.push(normalized)]
  G --> A
```
上記の図は`collect_all_files`関数（行番号不明）の主要分岐を示す。

### remove_profile_files

1) 目的と責務
- 指定されたファイルを**存在チェック後に削除**
- 親ディレクトリが空なら削除（失敗は無視）

2) アルゴリズム
- file_listを反復
  - Path.exists()ならremove_file
  - parent()がSomeならremove_dir（失敗は無視）

3) 引数

| 名称 | 型 | 説明 |
|------|----|------|
| file_list | &[String] | 絶対/相対パスの文字列一覧 |

4) 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<()> | 例外はI/Oエラーのみ返す（存在しないファイルは無視） |

5) 使用例
```rust
let files = vec!["tmp/a.txt".into(), "tmp/b.txt".into()];
remove_profile_files(&files)?;
```

6) エッジケース
- 不存在ファイル: スキップ（テストあり）
- 相対パスの親ディレクトリ: 空でなければremove_dirは失敗→無視
- 絶対パス: 想定外の外部ディレクトリ削除を試みる恐れ（設計上のガードが必要）

根拠: remove_profile_files（行番号不明）

### copy_profile_files

1) 目的と責務
- source_dirからdest_dirへfile_listに基づき**コピー**
- **競合検知**（destに既存でforce=false）→**FileConflict**エラー
- 成功したファイルの**絶対パス（'/'区切り）**一覧を返す

2) アルゴリズム
- file_list反復
  - source_path = source_dir.join(file_path)
  - dest_path = dest_dir.join(file_path)
  - source_path.exists()でなければcontinue
  - dest_path.exists() && !forceならconflict_owner(dest_path)でowner取得→Err(FileConflict)
  - parentがあればcreate_dir_all
  - fs::copy(source_path, dest_path)
  - canonicalizeに成功すれば絶対パスへ、失敗時はdest_pathをそのまま文字列化。'\'→'/'で正規化してpush

3) 引数

| 名称 | 型 | 説明 |
|------|----|------|
| source_dir | &Path | コピー元のルート |
| dest_dir | &Path | コピー先のルート |
| file_list | &[String] | 相対パス想定（joinで結合） |
| force | bool | trueなら既存を上書き |
| conflict_owner | impl Fn(&Path) -> Option<String> | 競合時に所有者識別文字列を返す関数 |

4) 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<Vec<String>> | 正常にコピーされたファイルの絶対パス（'/'区切り） |

5) 使用例
```rust
let files = vec!["subdir/test.txt".into()];
let copied = copy_profile_files(
    Path::new("pkg"),
    Path::new("workspace"),
    &files,
    false,
    |_| None, // 所有者不明なら "unknown"
)?;
for p in copied {
    println!("copied: {}", p);
}
```

6) エッジケース
- sourceに存在しない: スキップ（テストあり）
- 競合（force=false）: Err(FileConflict { path, owner })（テストあり）
- パスに".."を含む: joinによりdest_dirから外へ出る可能性。ガードが必要（設計上の課題）
- 親ディレクトリがない: create_dir_allで作成
- canonicalize失敗: 返却パスは非正規化（ただしコピー後なので存在するはず）

根拠: copy_profile_files（行番号不明）

### ProfileBackup

1) 目的と責務
- **ロックファイルエントリ**と**ファイル内容（絶対パス, バイト列）**を束ねたバックアップコンテナ
- ロールバック時の復元ソース

2) フィールド

| 名称 | 型 | 説明 |
|------|----|------|
| entry | ProfileLockEntry | バックアップ対象のメタ情報（名前/バージョン/ファイル一覧等） |
| files | Vec<(PathBuf, Vec<u8>)> | バックアップした絶対パスと内容 |

根拠: ProfileBackup（行番号不明）

### backup_profile

1) 目的と責務
- **workspace**基準で**entry.files**の相対パスを絶対化し、存在するファイルのみ**fs::read**で内容をメモリ退避
- **ProfileBackup**を返す

2) アルゴリズム
- entry.filesを反復
  - absolute = workspace.join(relative)
  - absolute.exists()ならfs::readでVec<u8>取得しpush
- ProfileBackup { entry: entry.clone(), files } を返す

3) 引数

| 名称 | 型 | 説明 |
|------|----|------|
| workspace | &Path | バックアップの基準ディレクトリ |
| entry | &ProfileLockEntry | ロックファイルエントリ（ファイル一覧含む） |

4) 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<ProfileBackup> | メモリ上バックアップ |

5) 使用例
```rust
use super::lockfile::ProfileLockEntry;
let entry = ProfileLockEntry {
  name: "p".into(), version: "1.0".into(), installed_at: "2025-01-11".into(),
  files: vec!["a.txt".into()], integrity: "abc".into(),
  commit: None, provider_id: None, source: None,
};
let backup = backup_profile(Path::new("workspace"), &entry)?;
```

6) エッジケース
- 不存在ファイル: スキップ（テストあり）
- 大容量/多数ファイル: filesベクトルが**総サイズ分メモリ**を使用
- 読み取り権限なし: I/Oエラーへ

根拠: backup_profile（行番号不明）

### restore_profile

1) 目的と責務
- **ProfileBackup**に含まれるファイル群を**元の絶対パスへ書き戻し**
- 必要に応じて親ディレクトリをcreate_dir_all

2) アルゴリズム
- backup.files反復
  - parentがSomeならcreate_dir_all
  - fs::write(path, data)

3) 引数

| 名称 | 型 | 説明 |
|------|----|------|
| backup | &ProfileBackup | 復元対象（絶対パスとバイト列） |

4) 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<()> | I/O失敗はErr |

5) 使用例
```rust
let backup = /* 事前に取得 */;
restore_profile(&backup)?;
```

6) エッジケース
- 深いディレクトリ: 自動作成（テストあり）
- 権限不足: 書き込み失敗
- 既存ファイル上書き: 常に上書き（変更が失われる）

根拠: restore_profile（行番号不明）

## Walkthrough & Data Flow

典型的なインストール＆ロールバックフロー:

1. プロファイルパッケージから対象ファイルの一覧取得
   - collect_all_files(profile_dir) → Vec<String>（相対パス）
2. ワークスペースへのコピー
   - copy_profile_files(source_dir, dest_dir, &files, force, conflict_owner) → 成功した絶対パス一覧
   - 競合時はFileConflictエラー（force=false）
3. 整合性記録
   - calculate_integrity(絶対パスを文字列化した一覧、または相対パスを絶対化して連結）→ 64桁ハッシュ
4. アップデート前バックアップ
   - backup_profile(workspace, &lock_entry) → ProfileBackup（全バイトをメモリ保持）
5. 失敗時のロールバック
   - restore_profile(&backup) → 元のファイル内容に戻す
6. アンインストール
   - remove_profile_files(&files) → 削除と空ディレクトリ掃除

データの流れと契約:
- ファイルパスは**相対**で取り扱う想定（copy/backupでjoin）。相対でない場合も動作するが、セキュリティ上のチェックが必要
- ProfileLockEntry.filesはバックアップ/復元の**ソース・オブ・トゥルース**（このチャンク内では構造詳細は不明）

## Complexity & Performance

- calculate_integrity: 時間 O(Σファイルサイズ)、空間 O(最大ファイルサイズ)。全読みのため大容量でメモリ使用高。
- collect_all_files: 時間 O(エントリ数)。空間 O(収集ファイル数)。WalkDirオーバーヘッドあり。
- remove_profile_files: 時間 O(n)。空間 O(1)。
- copy_profile_files: 時間 O(n + Σファイルサイズ)。空間 O(n)（返却用）。fs::copyはOSのストリームを利用しメモリ効率は相対的に良好。
- backup_profile: 時間 O(n + Σファイルサイズ)。空間 O(Σファイルサイズ)。メモリ消費がボトルネック。
- restore_profile: 時間 O(n + Σファイルサイズ)。空間 O(1)。

ボトルネックとスケール限界:
- バックアップが**メモリ全量保持**のため、数GB規模では不可。ストリーミング/一時ディスク利用が望ましい
- 整合性計算で**fs::read**を使っており、巨大ファイルは負荷高。BufReaderでチャンク読みしてハッシュ更新へ改善可能
- I/O集中でスループットはストレージ性能に依存。ネットワーク/DBは未使用（このチャンクには現れない）

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全な標準APIのみ使用、unsafeなし→問題の兆候なし
  - 大容量ファイルの全読み（fs::read）により**メモリ枯渇**のリスク（calculate_integrity, backup_profile）あり
- インジェクション
  - SQL/Command: 該当なし（このチャンクには現れない）
  - Path traversal: copy_profile_filesでfile_listに「..」が含まれる場合、`dest_dir.join(file_path)`が**外部ディレクトリへ到達**し得る。remove_profile_filesでも攻撃的な絶対パスが渡されると想定外削除の危険
- 認証・認可
  - 権限チェック漏れ: OSファイル権限に依存。アプリ層の認可は本モジュールでは**不明**
  - セッション固定: 該当なし（このチャンクには現れない）
- 秘密情報
  - Hard-coded secrets: なし
  - Log leakage: ログ出力自体がなく、漏洩は**不明**
- 並行性
  - Race condition: copy_profile_filesで`exists()`チェック後に他プロセスが変更する**TOCTOU**の可能性。整合性確保のためには**ロックファイル**やOSロックにより保護が必要
  - Deadlock: 該当なし
- その他
  - Windows/UNIXパス差異: 返却パス文字列で'/'に正規化。上位が**OS依存パス**を必要とする場合の不整合に注意
  - canonicalize: ネットワークパス/シンボリックリンクの扱いは**不明**

詳細エッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空ファイルリスト（ハッシュ） | [] | 空連結のSHA-256 | fs::readなし→finalizeのみ | OK |
| 不存在ファイル（ハッシュ） | ["exists.txt","missing.txt"] | missingはスキップし成功 | existsのみ読込 | OK（テストあり） |
| 大容量バックアップ | 数GB | 成功するがメモリ使用増大 | 全読込→Vec<u8>保持 | リスク |
| ..混入のコピー | ["../../etc/passwd"] | 拒否（エラー） | joinのみ | 脆弱（要ガード） |
| .git除外 | ".git/config" | 収集から除外 | components検査 | OK |
| manifest除外 | "profile.json" | 収集から除外 | 文字列比較 | OK |
| 競合検知 | dest存在, force=false | Err(FileConflict) | owner取得, Err返却 | OK（テストあり） |
| TOCTOU | exists→copy間に変更 | 競合回避 | 非対応 | リスク |
| 親ディレクトリ削除 | 存在しない/非空 | 削除しない/失敗無視 | remove_dirの失敗無視 | OK（安全側） |

Rust特有の観点:

- 所有権
  - すべてのバイト列は関数スコープ内で所有。戻り値ProfileBackup.filesはVec<u8>を**所有**し、restoreで再利用可能
- 借用/ライフタイム
  - &Path / &ProfileLockEntry の**不変借用**のみで、ライフタイム衝突なし。明示的ライフタイム不要
- unsafe境界
  - **unsafeブロックなし**（このチャンクには現れない）
- 並行性/非同期
  - Send/Sync: 戻り値のデータ構造は標準コンテナで、一般にSend/Sync。I/Oは同期APIで**ブロッキング**
  - 共有状態: グローバル共有なし、データ競合の心配は低い
  - await境界/キャンセル: 非同期未使用（このチャンクには現れない）
- エラー設計
  - Result中心。Optionは存在チェックなどで適切に使用
  - panic可能性: collect_all_filesの`expect("walkdir entry...")`は設計前提。WalkDirがprofile_dir配下のみ返す前提が破れるとpanic
  - エラー変換: WalkDirのErrorを`std::io::Error::other(e)`へラップ。詳細喪失の懸念あり

## Design & Architecture Suggestions

- パス安全性の強化（重要）
  - **相対パスのみ**受け入れる型（例: `RelativePath`）を導入し、".."や絶対パスを**バリデーションして拒否**
  - `dest_dir.join(file_path)`後に**canonicalize**し、`starts_with(dest_dir.canonicalize()?)`で**逸脱検出**
  - remove_profile_filesも同様に**ワークスペース境界内**に限定
- ハッシュ/バックアップのストリーミング
  - calculate_integrityは`BufReader`で**チャンク読み**し`hasher.update`、メモリ負荷低減
  - backup_profileはメモリ保持ではなく**一時ディスク**または**差分用の圧縮アーカイブ**に退避
- 競合処理の堅牢化
  - **ファイルロック**（OSロック）や**原子的な置換**（temp書き→`rename`）を採用
  - 競合情報の詳細（ownerのソース）を拡充し、ユーザーフィードバック改善
- エラーの情報保持
  - WalkDirエラーを独自型へ詳細保持（パス/原因）し、`IoError::Other`へ単純ラップしない
- 返却パスのプラットフォーム適合
  - 返却の絶対パスを**OsString/PathBuf**のまま返すAPI追加、UI層で表示整形を行う

## Testing Strategy (Unit/Integration) with Examples

現状テストの網羅:
- calculate_integrity: 単数/複数/順序差/欠落ファイル（OK）
- remove_profile_files: 存在/欠落パス（OK）
- copy_profile_files: 単一/サブディレクトリ/欠落スキップ/競合検知とforce上書き（OK）
- backup/restore: 通常/欠落/深いディレクトリ/ラウンドトリップ（OK）

追加すべきテスト（例）:
- **パストラバーサル**拒否（想定修正後）
```rust
#[test]
fn test_copy_profile_files_reject_traversal() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("src");
    let dest = temp.path().join("dst");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("safe.txt"), "x").unwrap();

    // 「../」を含む入力は拒否（修正後のバリデーション想定）
    let files = vec!["../outside.txt".to_string()];
    // ここでは仮にProfileError::InvalidPathを想定（このチャンクには現れない）
    let result = copy_profile_files(&source, &dest, &files, false, |_| None);
    assert!(result.is_err());
}
```

- **シンボリックリンク**の扱い
```rust
#[test]
fn test_collect_all_files_symlink() {
    // OSによりsymlink作成APIが異なるため条件付き（詳細は不明）
    // シンボリックリンクがファイル扱いになるか、リンク先を辿るかを確認
    // このチャンクにはリンク追跡のポリシーが現れないため、期待動作は不明
}
```

- **大容量ファイル**ハッシュ（ストリーミング化後）
```rust
#[test]
fn test_calculate_integrity_large_file() {
    // 100MB程度のファイルを生成し、メモリ逼迫がないことを確認
    // ストリーミング実装後に有効
}
```

- **TOCTOU**検証（擬似）
```rust
#[test]
fn test_copy_profile_files_toctou() {
    // destが存在チェック後に別スレッドで作成されるケースを模擬
    // 現実には同期I/Oでの再現は難しく、integrationで検証
}
```

- **Windowsパス文字列**返却の正規化確認
```rust
#[test]
fn test_copy_profile_files_path_normalization() {
    // Windows環境で'\'が'/'へ正規化されることを確認
}
```

## Refactoring Plan & Best Practices

- calculate_integrityの改善
  - `std::fs::File` + `std::io::BufReader`でチャンク読み、`hasher.update(buf)`を繰り返す
  - 区切り文字は**ハッシュ衝突防止**のためファイル境界メタデータ（例: 長さ）も組み込むことを検討
- copy_profile_filesの安全化
  - **入力パス検証**: `Path::new(file).is_relative()`かつ`components`に**ParentDir**がないことをチェック
  - `dest_path.canonicalize()`後に`starts_with(dest_dir.canonicalize()?)`で境界内確認
  - 書き込みは**一時ファイル**へ出力→`rename`で原子的置換
- remove_profile_filesの安全化
  - 削除対象を**workspace配下**に限定するAPI設計（workspace引数追加）
  - 親ディレクトリ削除は**ルート越え**を回避するガード（例: workspace.starts_with）
- バックアップのスケーラビリティ
  - メモリではなく**一時アーカイブ**（zip/tar）への退避、または**差分方式**を採用
- エラー詳細の保持
  - WalkDirのErrorを**独自エラー**へ包含し、パスと原因の**構造化**を提供
- 監査ログの導入
  - どのファイルが作成/削除/上書きされたかを**構造化ログ**として出力

## Observability (Logging, Metrics, Tracing)

- ロギング
  - **INFO**: コピー開始/終了、削除件数、バックアップ件数
  - **WARN**: 競合検知、スキップ（不在ファイル）、権限不足
  - **ERROR**: I/O失敗、検証失敗（../混入など）
- メトリクス
  - カウンタ: コピー成功/失敗、削除成功/失敗
  - ヒストグラム: ファイルサイズ分布、処理時間
- トレーシング
  - 1インストール操作に**トレースID**付与、各API呼び出しをspanとして関連付け

このチャンクのコードにはログ/メトリクス/トレーシングの実装は**現れない**ため、提案のみ。

## Risks & Unknowns

- ProfileError / ProfileResultの詳細（バリアント/表示）は**不明**
- ProfileLockEntryの完全仕様（filesの性質、相対/絶対保証）は**不明**
- conflict_ownerの上位実装（所有者の決定ロジック）は**不明**
- シンボリックリンクや特殊ファイル（FIFO/デバイス）の扱いは**不明**
- OS依存の差（Windows/POSIXのpath/canonicalize挙動）に関するポリシーは**不明**