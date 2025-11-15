# installer.rs Review

## TL;DR

- 目的: プロファイル（profile）が提供するファイルをワークスペースへインストールし、既存ファイルとの衝突を検出・回避（サイドカー生成）するためのコアロジックを提供
- 主な公開API: generate_sidecar_path, check_all_conflicts, check_profile_conflicts, ProfileInstaller::new, ProfileInstaller::install_files, ProfileInstaller::default
- 複雑箇所: サイドカー名生成（ドットファイルや拡張子の有無・複数ドットの扱い）、所有者判定とforceフラグによる分岐、インストールの原子性（部分的成功の可能性）
- 重大リスク: パス検証不足によるディレクトリトラバーサル（filesの相対パスに".."/絶対パスが含まれる場合）、TOCTTOU（存在チェック→コピーの間に状態変化）、サイドカー生成時のプロバイダ名未検証
- エラー設計: ProfileErrorにより詳細な衝突情報を返すが、install_filesはコピー途中で失敗すると部分的インストールが残る可能性あり（ロールバックなし）
- 並行性: 同期I/O中心で非同期・ロックなし。複数プロセス・スレッド同時実行時の競合（race condition）可能性あり
- 推奨: インストール前の包括的衝突チェック（check_all_conflicts）を必須化し、コピーはテンポラリ→アトミックrename方式で原子性を向上。files入力のパス正規化・検証を追加

## Overview & Purpose

このファイルは、プロファイルが提供するファイル群をワークスペースへ安全に配置するためのインストールロジックをまとめています。主目的は以下です。

- 既存ファイルとの衝突検出（所有者が同じなら上書き、異なる/不明ならforceでない限りエラー、forceならサイドカー作成）
- サイドカーのファイル名生成（拡張子やドットファイルに配慮）
- インストール成功・サイドカー作成の結果収集
- 事前検査（pre-flight）により「全部または何もなし」の方針を支援（ただし現状はI/O失敗時のロールバックなし）

プラグイン参照（src/plugins/mod.rs:871-940）は明示されていますが、このチャンクにはそのコードは現れないため詳細は不明です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Type Alias | InstallResult | pub | インストール結果のデータ契約（installed, sidecars） | Low |
| Function | generate_sidecar_path | pub | サイドカーのファイルパス生成（拡張子・ドットファイル対応） | Med |
| Function | check_all_conflicts | pub | すべてのファイルの衝突を収集して返す（包括的pre-flight） | Low |
| Function | check_profile_conflicts | pub | ファイル毎の衝突検査（最初の衝突で即エラー） | Low |
| Struct | ProfileInstaller | pub | インストールのエントリポイント（状態なし） | Low |
| Method | ProfileInstaller::new | pub | インストーラの生成 | Low |
| Method | ProfileInstaller::install_files | pub | ファイル群のインストールとサイドカー処理、結果収集 | Med |
| Trait Impl | ProfileInstaller::default | pub | 既定生成（newの委譲） | Low |

### Dependencies & Interactions

- 内部依存
  - install_files → generate_sidecar_path（サイドカー生成）
  - check_profile_conflicts → ProfileLockfile::load, find_file_owner
  - check_all_conflicts → ProfileLockfile::find_file_owner
- 外部依存（このモジュールから見える範囲）

| 依存 | 用途 | 備考 |
|------|------|------|
| super::error::{ProfileError, ProfileResult} | エラー型・結果型 | エラー詳細にFileConflict/MultipleFileConflictsなど |
| super::lockfile::ProfileLockfile | 所有者判定・lockfile読み込み | find_file_owner, load |
| std::path::{Path, PathBuf} | パス操作 | join, parent, file_nameなど |
| std::fs | ファイルI/O | create_dir_all, copy |

- 被依存推定（このモジュールを利用しそうな箇所）
  - プロファイル管理の上位層（CLIコマンド、プラグイン管理）
  - プロビジョニング/セットアップフェーズ（例: init, upgrade）
  - 複数プロバイダ（provider）が生成するファイルを統合するレイヤ

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| InstallResult | pub type InstallResult = (Vec<String>, Vec<(String, String)>) | インストール結果（設置済み相対パス一覧、サイドカーの対応表） | O(n)（保持要素数） | O(n) |
| generate_sidecar_path | pub fn generate_sidecar_path(original: &Path, provider: &str) -> PathBuf | 衝突時に作るサイドカーのファイルパス生成 | O(L)（ファイル名長） | O(L) |
| check_all_conflicts | pub fn check_all_conflicts(workspace: &Path, files: &[String], profile_name: &str, lockfile: &ProfileLockfile, force: bool) -> ProfileResult<()> | 全ファイルの衝突を収集して複数エラーで返す | O(n) + 依存（find） | O(1) |
| check_profile_conflicts | pub fn check_profile_conflicts(workspace: &Path, profile_name: &str, files: &[String], force: bool) -> ProfileResult<()> | 最初の衝突で即エラーする軽量チェック | O(n) + lockfileロード | O(1) |
| ProfileInstaller::new | pub fn new() -> Self | インストーラ生成 | O(1) | O(1) |
| ProfileInstaller::install_files | pub fn install_files(&self, source_dir: &Path, dest_dir: &Path, files: &[String], profile_name: &str, provider_name: &str, lockfile: &ProfileLockfile, force: bool) -> ProfileResult<InstallResult> | ファイル群のコピー、サイドカー生成、結果収集 | O(n + Σcopy(bytes)) | O(n) |
| ProfileInstaller::default | fn default() -> Self（Default実装） | 既定生成（newに委譲） | O(1) | O(1) |

以下、各APIの詳細。

### generate_sidecar_path

1) 目的と責務
- 既存ファイルと衝突した際に、元のパスと同ディレクトリに「{stem}.{provider}{ext}」形式のサイドカーを生成する。
- ドットファイルや拡張子の有無を考慮する。

2) アルゴリズム（ステップ分解）
- original.file_name()を取得（文字列化できなければ"file"を使用）
- 親ディレクトリを保持
- 以下の条件でsidecar_nameを決定
  - 先頭が'.'かつ'.'が1つだけ → "{file_name}.{provider}"
  - それ以外で'.'を含む → 最初の'.'位置でsplitし「{stem}.{provider}{ext}」
  - '.'を含まない → "{file_name}.{provider}"
- 親ディレクトリがあればjoin、なければそのままPathBufにする

3) 引数

| 引数 | 型 | 意味 |
|-----|----|------|
| original | &Path | サイドカー元のファイルパス |
| provider | &str | プロバイダ名（サイドカー名に埋め込む） |

4) 戻り値

| 型 | 意味 |
|----|------|
| PathBuf | 生成したサイドカーのパス |

5) 使用例

```rust
use std::path::Path;
let original = Path::new("/project/.gitignore");
let sidecar = generate_sidecar_path(original, "codanna");
assert_eq!(sidecar.to_string_lossy(), "/project/.gitignore.codanna");
```

6) エッジケース
- file_nameが非UTF-8 → "file"にフォールバック
- ".env.local"のように複数ドットのドットファイル → ".codanna.env.local"（期待と異なる可能性、詳細は後述）
- providerに不正文字（スラッシュ等）が含まれる場合 → 無効なファイル名になる可能性

コード（短いため全体引用。行番号はこのチャンクに提供されないため省略）:

```rust
pub fn generate_sidecar_path(original: &Path, provider: &str) -> PathBuf {
    let file_name = original
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    let parent = original.parent();

    let sidecar_name =
        if file_name.starts_with('.') && file_name.chars().filter(|&c| c == '.').count() == 1 {
            format!("{file_name}.{provider}")
        } else if let Some(dot_pos) = file_name.find('.') {
            let (stem, ext) = file_name.split_at(dot_pos);
            format!("{stem}.{provider}{ext}")
        } else {
            format!("{file_name}.{provider}")
        };

    if let Some(p) = parent {
        p.join(sidecar_name)
    } else {
        PathBuf::from(sidecar_name)
    }
}
```

### check_all_conflicts

1) 目的と責務
- インストール対象の全ファイルについて、既存ファイルがある場合の所有者を照会し、衝突をすべて収集して返す。
- atomicに「全部または何もなし」に近づけるためのpre-flight用。

2) アルゴリズム
- conflictsベクタを初期化
- filesを走査し、dest_path.exists()ならlockfile.find_file_ownerで所有者判定
  - owner==profile_name → 継続
  - owner!=profile_name → forceでなければconflictsに追加
  - None（不明） → forceでなければconflictsに追加
- conflictsが空でなければMultipleFileConflictsエラーを返す

3) 引数

| 引数 | 型 | 意味 |
|-----|----|------|
| workspace | &Path | ワークスペースルート |
| files | &[String] | 相対ファイルパス群 |
| profile_name | &str | 現在のプロファイル名 |
| lockfile | &ProfileLockfile | 既存所有者の参照元 |
| force | bool | 強制モード（trueならサイドカー作成許容） |

4) 戻り値

| 型 | 意味 |
|----|------|
| ProfileResult<()> | 成功()またはMultipleFileConflictsエラー |

5) 使用例

```rust
let lockfile = ProfileLockfile::load(workspace.join(".codanna/profiles.lock.json").as_path())?;
check_all_conflicts(&workspace, &files, "my-profile", &lockfile, false)?;
```

6) エッジケース
- filesが空 → Ok
- すべて既存なし → Ok
- 所有者が混在 → 複数件をまとめて返す

### check_profile_conflicts

1) 目的と責務
- 与えられたfilesについて、既存ファイルの所有者を調べ、最初の衝突を即返す軽量チェック。
- lockfileは内部でロードする。

2) アルゴリズム
- lockfile_pathをworkspace/.codanna/profiles.lock.jsonに固定
- lockfile.load
- filesを走査してdest.exists()なら所有者判定
  - 異なる所有者かつ!force → 即FileConflict
  - 所有者不明かつ!force → 即FileConflict
  - 同所有者またはforce → 継続
- 最後まで衝突なしならOk

3) 引数

| 引数 | 型 | 意味 |
|-----|----|------|
| workspace | &Path | ワークスペースルート |
| profile_name | &str | 現在のプロファイル名 |
| files | &[String] | 相対ファイルパス群 |
| force | bool | 強制モード |

4) 戻り値

| 型 | 意味 |
|----|------|
| ProfileResult<()> | 成功()またはFileConflictエラー |

5) 使用例

```rust
check_profile_conflicts(&workspace, "my-profile", &files, false)?;
```

6) エッジケース
- lockfileが存在しない/読み込み失敗 → エラー伝播
- filesが空 → Ok

### ProfileInstaller::new

1) 目的と責務
- 無状態のインストーラを生成

2) アルゴリズム
- Selfを返すのみ

3) 引数

| 引数 | 型 | 意味 |
|-----|----|------|
| なし | - | なし |

4) 戻り値

| 型 | 意味 |
|----|------|
| ProfileInstaller | 新しいインスタンス |

5) 使用例

```rust
let installer = ProfileInstaller::new();
```

6) エッジケース
- なし

### ProfileInstaller::install_files

1) 目的と責務
- source_dirからdest_dirへfilesをコピーする。
- 既存ファイルの所有者が同じなら上書き、異なる/不明ならforce=trueでサイドカー作成、force=falseならエラー。
- インストール済みファイルとサイドカーの対応を返す。

2) アルゴリズム
- installed, sidecarsを初期化
- filesを走査しsource存在を確認（なければスキップ）
- dest存在時、lockfile.find_file_ownerで所有者判断
  - owner==profile_name → 通常コピー
  - owner!=profile_name → force? trueならサイドカー、falseならエラー
  - owner==None → force? trueならサイドカー、falseならエラー
- use_sidecarならgenerate_sidecar_pathで最終パス決定
- 親ディレクトリ作成（create_dir_all）
- コピー（std::fs::copy）
- sidecarの場合はsidecarsに(意図したパス, 実際のサイドカー相対パス)を追加し、installedへ相対サイドカー名を追加。通常コピーなら意図したパスをinstalledへ追加
- 最後に(installed, sidecars)を返す

3) 引数

| 引数 | 型 | 意味 |
|-----|----|------|
| &self | ProfileInstaller | インスタンス |
| source_dir | &Path | 供給元ルート |
| dest_dir | &Path | 配置先ルート |
| files | &[String] | 相対ファイルパス群 |
| profile_name | &str | 現在のプロファイル名 |
| provider_name | &str | サイドカー名に付与するプロバイダ |
| lockfile | &ProfileLockfile | 所有者判定用 |
| force | bool | 強制モード |

4) 戻り値

| 型 | 意味 |
|----|------|
| ProfileResult<InstallResult> | 成功時(installed, sidecars)、失敗時エラー |

5) 使用例

```rust
let installer = ProfileInstaller::new();
let lockfile = ProfileLockfile::load(dest_dir.join(".codanna/profiles.lock.json").as_path())?;
let files = vec!["README.md".into(), ".gitignore".into()];
let (installed, sidecars) = installer.install_files(
    &source_dir,
    &dest_dir,
    &files,
    "my-profile",
    "codanna",
    &lockfile,
    true, // forceでサイドカーを許容
)?;
```

6) エッジケース
- sourceに存在しないファイルはスキップされる（静かに無視）
- destにファイルがなく新規 → 通常コピー
- destに存在し所有者が同じ → 上書き
- destに存在し所有者が異なる/不明で!force → FileConflictエラー
- destに存在し所有者が異なる/不明でforce → サイドカー作成
- ディレクトリが存在しない → create_dir_allで作成

抜粋（長いため重要部分のみ。行番号はこのチャンクに提供されないため省略）:

```rust
pub fn install_files(
    &self,
    source_dir: &Path,
    dest_dir: &Path,
    files: &[String],
    profile_name: &str,
    provider_name: &str,
    lockfile: &ProfileLockfile,
    force: bool,
) -> ProfileResult<InstallResult> {
    let mut installed = Vec::new();
    let mut sidecars = Vec::new();

    for file_path in files {
        let source_path = source_dir.join(file_path);
        if !source_path.exists() {
            continue;
        }
        let dest_path = dest_dir.join(file_path);

        let use_sidecar = if dest_path.exists() {
            match lockfile.find_file_owner(file_path) {
                Some(owner) if owner == profile_name => false,
                Some(owner) => {
                    if !force {
                        return Err(ProfileError::FileConflict {
                            path: file_path.clone(),
                            owner: owner.to_string(),
                        });
                    }
                    true
                }
                None => {
                    if !force {
                        return Err(ProfileError::FileConflict {
                            path: file_path.clone(),
                            owner: "unknown".to_string(),
                        });
                    }
                    true
                }
            }
        } else {
            false
        };

        let (final_path, relative_path) = if use_sidecar {
            let sidecar_path = generate_sidecar_path(&dest_path, provider_name);
            let sidecar_relative = generate_sidecar_path(Path::new(file_path), provider_name);
            (sidecar_path, sidecar_relative.to_string_lossy().to_string())
        } else {
            (dest_path.clone(), file_path.clone())
        };

        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::copy(&source_path, &final_path)?;

        if use_sidecar {
            sidecars.push((file_path.clone(), relative_path.clone()));
            installed.push(relative_path);
        } else {
            installed.push(file_path.clone());
        }
    }

    Ok((installed, sidecars))
}
```

### ProfileInstaller::default

1) 目的と責務
- Defaultトレイトの実装。newに委譲。

2) アルゴリズム
- Self::new()呼び出し

3) 引数/戻り値
- 引数なし、ProfileInstallerを返す

4) 使用例

```rust
let installer: ProfileInstaller = Default::default();
```

5) エッジケース
- なし

## Walkthrough & Data Flow

- 典型的なフロー（非forceモード）
  1. check_all_conflictsで全ファイルの衝突を収集し、問題があればMultipleFileConflictsで中断
  2. 問題なければinstall_filesでコピー
  3. 既存ファイルの所有者が同じなら上書き
  4. 所有者が異なる/不明ならFileConflictで即中断（部分的インストールの可能性）

- 典型的なフロー（forceモード）
  1. check_all_conflictsはスキップ可能（エラーは返さない）だが推奨は事前確認
  2. install_filesは衝突時サイドカーを作成し、通常ファイルはそのまま設置

Mermaidフローチャート（install_filesの主要分岐を示す。行番号はこのチャンクに提供されないため省略）:

```mermaid
flowchart TD
  A[Start install_files] --> B{for each file_path}
  B --> C[Compute source_path, dest_path]
  C --> D{source_path exists?}
  D -- No --> B
  D -- Yes --> E{dest_path exists?}
  E -- No --> F[use_sidecar=false]
  E -- Yes --> G{find_file_owner(file_path)}
  G -- owner==profile --> H[use_sidecar=false]
  G -- owner!=profile --> I{force?}
  I -- No --> J[Error: FileConflict]
  I -- Yes --> K[use_sidecar=true]
  G -- None --> L{force?}
  L -- No --> J
  L -- Yes --> K
  F --> M[Decide final_path=dest_path]
  H --> M
  K --> N[final_path=generate_sidecar_path(dest)]
  M --> O[create_dir_all(parent)]
  N --> O
  O --> P[copy(source -> final)]
  P --> Q{use_sidecar?}
  Q -- Yes --> R[sidecars push (intended, relative)]
  Q -- No --> S[installed push file_path]
  R --> T[installed push relative]
  S --> T
  T --> B
  B --> U[Return (installed, sidecars)]
```

## Complexity & Performance

- generate_sidecar_path
  - 時間: O(L)（ファイル名長に比例）
  - 空間: O(L)
- check_all_conflicts / check_profile_conflicts
  - 時間: O(n)（nはfiles数）+ lockfile照会コスト
    - find_file_ownerの計算量は不明（このチャンクには現れない）。ハッシュマップ想定ならO(1)平均。
  - 空間: O(1)（check_all_conflictsは衝突数に比例して一時増加）
- install_files
  - 時間: O(n + Σbytes_copy)（ファイル数とコピーサイズ）
  - 空間: O(n)（結果ベクタ）
- ボトルネック
  - ディスクI/O（create_dir_all, copy）
  - 大量ファイルや巨大ファイルのコピーコスト
  - 複数プロセス/スレッドによる同時操作の競合（ロックなし）
- スケール限界
  - filesが非常に多い場合メモリ負荷（installed/sidecarsの格納）とI/Oレイテンシ
  - ネットワークファイルシステム（NFS等）ではTOCTTOUやI/O失敗率増加の可能性

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性
  - Rust標準の安全APIのみ使用。unsafeなし（このチャンクにはunsafe境界は現れない）。
  - 所有権/借用は適切。to_stringで所有を複製してエラー情報に保持。
- インジェクション
  - コマンド/SQLなし。パス名組み立てにユーザ入力（files, provider_name）が影響。
  - パス トラバーサルの可能性: filesが"../"や絶対パスを含むとdest_dirから外へ出てしまう恐れ。検証・正規化なし。重大。
- 認証・認可
  - なし（ファイルI/Oのみ）。権限チェックはOS依存。書き込み不可でエラーになる。
- 秘密情報
  - ハードコードされた秘密はなし。ログ出力もないため漏えいはないが、今後ログを追加する際はパスや所有者情報の扱いに注意。
- 並行性
  - ロックなし。別プロセスやスレッドが同じファイルを操作する可能性に対して競合（race condition）。
  - TOCTTOU: existsチェック後に他者がファイルを変更/削除する可能性。
- エラー設計
  - I/O失敗時に?で即時中断。部分的にコピー済みのファイルをロールバックしないため、原子性が不十分。

詳細なエッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空files | [] | Ok(())または(0,0)結果 | 各関数でループせずOk | OK |
| source不存在 | files=["A.txt"]（source_dirにA.txtなし） | スキップ（副作用なし） | install_filesはcontinue | OK |
| dest不存在 | files=["A.txt"] | 通常コピー | use_sidecar=false | OK |
| 同一所有者 | 既存のA.txt所有者==profile | 上書き許容 | use_sidecar=false | OK |
| 異なる所有者, !force | 既存のA.txt所有者!=profile | エラー（上書き禁止） | FileConflict返却 | OK |
| 不明所有者, !force | lockfileに未登録 | エラー | FileConflict返却 | OK |
| 異なる/不明所有者, force | 任意 | サイドカー生成 | generate_sidecar_path使用 | OK |
| ドットファイル（拡張なし） | ".gitignore" | ".gitignore.provider" | 対応済み | OK |
| 複数ドットのドットファイル | ".env.local" | ".env.provider.local"が望ましい | ".codanna.env.local"になる | 要検討 |
| providerに不正文字 | provider="/"など | 無効なファイル名 | 未検証 | 要修正 |
| パストラバーサル | files=["../outside.txt"] | dest_dir外へ書き込み禁止 | joinのみで許容される | 要修正 |
| 競合中の変更 | exists→copyの間に他者が変更 | 耐性が必要（アトミック化） | 非対応 | 要改善 |
| ロールバック | コピー途中でI/Oエラー | 部分インストールのクリーンアップ | 非対応 | 要改善 |

## Design & Architecture Suggestions

- サイドカー命名の改善
  - ".env.local"のような複数ドットのドットファイルは、期待的には".env.provider.local"が自然。現在は".codanna.env.local"となるため、ルール見直しを提案。
  - ルール案: 先頭が'.'の場合は先頭ドットを保持し、次のドット前までをstemとみなす（例: ".env.local" → stem=".env", ext=".local" → ".env.provider.local"）。
- パス検証・正規化
  - files要素に対して、絶対パス拒否、".."を含むセグメントの拒否、dest_dir配下に収まることを保証するガードを追加。
- 原子性の向上
  - 現状はI/O失敗で部分的インストールが残る。対策として:
    - 一時ファイル（dest_path.tmp）にコピーし、最後にアトミックrenameで置き換える
    - サイドカーも同様の手順
    - 失敗時は一時ファイルを確実に削除
- 事前チェックの統合
  - check_profile_conflictsとcheck_all_conflictsの方針を統一。原則としてcheck_all_conflictsを使い、UI層で複数衝突の提示を行う。
- ロックファイルの扱い
  - install後にlockfileの更新（所有者記録）が必要であれば、トランザクション的な更新手順を別コンポーネントで定義（このチャンクには現れないため不明）。
- provider名の検証
  - 英数字と一部の安全な記号のみ許可（例: '-', '_'）。無効文字は置換。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト
  - generate_sidecar_path
    - ".gitignore" → ".gitignore.codanna"
    - "docker-compose.yml" → "docker-compose.codanna.yml"
    - ".env.local" → 現仕様の".codanna.env.local"（期待との乖離を確認）
    - 非UTF-8ファイル名（OS依存）→ "file.provider"
  - check_all_conflicts
    - 複数衝突を収集するケース
    - force=trueで衝突がエラーにならないこと
  - check_profile_conflicts
    - 最初の衝突で即エラー
  - install_files
    - 新規コピー、上書き、サイドカー生成、source不存在のスキップ

- 統合テスト
  - tempfileやtempdirを使い、一時ディレクトリ上で実際のファイルI/Oを検証
  - lockfileのモック/実体でfind_file_ownerの分岐を制御

例（概略、テストフレームワークは標準の#[test]）:

```rust
#[test]
fn test_generate_sidecar_path_variants() {
    use std::path::Path;

    assert_eq!(
        generate_sidecar_path(Path::new("CLAUDE.md"), "codanna").to_string_lossy(),
        "CLAUDE.codanna.md"
    );
    assert_eq!(
        generate_sidecar_path(Path::new(".gitignore"), "codanna").to_string_lossy(),
        ".gitignore.codanna"
    );
    assert_eq!(
        generate_sidecar_path(Path::new("docker-compose.yml"), "codanna").to_string_lossy(),
        "docker-compose.codanna.yml"
    );
    // 現仕様の挙動
    assert_eq!(
        generate_sidecar_path(Path::new(".env.local"), "codanna").to_string_lossy(),
        ".codanna.env.local"
    );
}

#[test]
fn test_check_all_conflicts_collects_multiple() {
    // lockfileをスタブして find_file_owner の戻りを制御
    // このチャンクにはProfileLockfileの実装が現れないため、擬似的なモック例を示す
    struct StubLockfile;
    impl StubLockfile {
        fn find_file_owner(&self, path: &str) -> Option<&str> {
            match path {
                "a.txt" => Some("other"),
                "b.txt" => None,
                _ => None,
            }
        }
    }
    // 実際にはProfileLockfileを用いる
    // ... 省略 ...
}

#[test]
fn test_install_files_sidecar_and_overwrite() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    use std::path::PathBuf;
    let tmp = tempfile::tempdir()?;
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&src)?;
    fs::create_dir_all(&dst)?;
    fs::write(src.join("README.md"), "hello")?;
    fs::write(dst.join("README.md"), "old")?;

    // lockfileのfind_file_ownerが"other"を返すようにスタブ（実際はProfileLockfile）
    // ... 省略 ...

    let installer = ProfileInstaller::new();
    let files = vec!["README.md".into()];
    // force=trueでサイドカー発生
    // let (installed, sidecars) = installer.install_files(&src, &dst, &files, "my", "codanna", &lockfile, true)?;
    // アサーション:
    // - dst/README.codanna.md が存在
    // - installedに"README.codanna.md"を含む
    // - sidecarsに("README.md", "README.codanna.md")を含む
    Ok(())
}
```

- 失敗時ロールバックのテスト
  - 故意にコピー途中で失敗させ、部分インストールが残るかを確認。改善後はテンポラリ→renameで原子性を検証。

## Refactoring Plan & Best Practices

- パスの安全性を強化（高優先度）
  - filesの各要素に対して:
    - Path::is_absolute拒否
    - コンポーネントに".."が含まれないことを検証
    - dest_dir.join(path).canonicalize()がdest_dir配下であることを確認（シンボリックリンク考慮）
- サイドカー名生成のルール改善（中優先度）
  - 複数ドットのドットファイルに対し、stem=".env" ext=".local"などに分割するロジックへ変更
- 原子性の向上（高優先度）
  - copyの代わりに一時ファイルに書き込み→rename（posix/NTFSのアトミック仕様に依存）
  - 失敗時クリーンアップ
- 事前チェック統合（中優先度）
  - check_all_conflictsを標準フローに組み込み、install_files前に必ず実行
- provider名の検証（中優先度）
  - 無効文字の拒否・正規化（英数字、'-', '_'のみ許可）
- エラー詳細の充実（低〜中）
  - install_filesのI/Oエラー時に、どのファイルで失敗したかを明確化し、部分的成果のリストを返す（オプション）

## Observability (Logging, Metrics, Tracing)

- ロギング
  - レベル: debug（開始/終了、ファイル数）、info（サイドカー発生）、warn（未知所有者）、error（I/O失敗）
  - メッセージ例: "install start: n files", "sidecar created: intended=..., sidecar=..."
- メトリクス
  - カウンタ: installed_files_count, sidecar_files_count, conflicts_detected
  - ヒストグラム: copy_duration, file_size
- トレーシング
  - インストール1件をスパンとして、各ファイルコピーを子スパンに。属性にpath, size, sidecarフラグ、owner状態など。
- 注意
  - ログに機密情報（ファイル内容）は含めない。パス/所有者の最小限の情報に限定。

## Risks & Unknowns

- Unknowns
  - ProfileLockfileの内部構造とfind_file_ownerの計算量/返却型詳細（このチャンクには現れない）
  - プラグイン側の衝突検査実装（src/plugins/mod.rs:871-940）の仕様差異
  - lockfileの更新タイミング（install後に所有者登録をするか）
- Risks
  - パス トラバーサルによりdest_dir外へ書き込む危険
  - TOCTTOUにより意図しない上書き/失敗
  - 複数スレッド/プロセスの同時実行時に競合（ロックなし）
  - provider名の不正でファイル作成に失敗
  - ".env.local"の扱いが期待と異なる可能性によるユーザ混乱

以上を踏まえ、まずはパス検証と原子的コピー（テンポラリ→rename）を導入することで、安全性と堅牢性を大きく改善できます。