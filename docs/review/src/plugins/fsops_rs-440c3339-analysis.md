# plugins\fsops.rs Review

## TL;DR

- 目的: プラグインのファイルをプロジェクト配下の名前空間ディレクトリへ安全にコピー・削除し、整合性（SHA-256）を検証するユーティリティ。
- 主要公開API: **copy_plugin_files**, **copy_plugin_payload**, **remove_plugin_files**, **verify_file_integrity**, **calculate_integrity**。
- 複雑箇所: **calculate_dest_path** のカテゴリ分岐（commands/agents/hooks/scripts/plugins）と、**copy_plugin_payload** のフィルタリング（.git/既にコピー済み/カテゴリ除外）ロジック。
- 重大リスク:
  - パス正規化不足により「..」を含む入力で名前空間外へ書き出せる可能性（Path Traversal）。
  - symlinkをfs::copyが辿るプラットフォームで、外部ファイルをコピーしてしまう可能性。
  - 競合検出が TOCTOU（exists→copy）で非原子的、レースに弱い。
  - copy_plugin_payload 内の expect による潜在的 panic。
- 並行性: 非同期や共有可変状態はないが、ファイルシステム競合・レースの考慮が必要。
- エラー設計: PluginResult/PluginError へ委譲。walkdirエラーは io::Error::other へ変換、それ以外は ? で透過的に伝播。

## Overview & Purpose

このファイルは、プラグインのインストール時に必要なファイルシステム操作をまとめたヘルパー群です。主な責務は以下です。

- 名前空間化されたディレクトリ（.claude/commands|agents|hooks|scripts|plugins）へのファイルコピー。
- 競合検出（既存ファイルがある場合の所有者問い合わせ＋強制上書き制御）。
- プラグインが提供する全ペイロードの一括コピー（特定カテゴリは除外、.gitも除外）。
- コピーしたファイルの削除と空ディレクトリの簡易クリーンアップ。
- SHA-256 によるファイル/ファイル集合の整合性検証。

全体として、インストール/アンインストールに伴うディスクI/Oを同期的に処理します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | copy_plugin_files | pub | 指定リストをカテゴリ別の宛先へコピー＋競合検出 | Med |
| Function | copy_plugin_payload | pub | ディレクトリ全体をスキャンし、除外規則を適用して一括コピー | High |
| Function | remove_plugin_files | pub | コピー済みファイルの削除＋空親ディレクトリの削除 | Low |
| Function | calculate_dest_path | pub(crate) | カテゴリ名（commands/agents/hooks/scripts）に基づき宛先パスを組み立てる | Med |
| Function | verify_file_integrity | pub | 1ファイルのSHA-256を計算して期待値と比較 | Low |
| Function | calculate_integrity | pub | 複数ファイルのSHA-256を連結（改行区切り）で計算 | Low |
| Type | PluginError | 外部（super::error） | エラー種別（競合、I/O等） | 不明 |
| Type | PluginResult | 外部（super::error） | 結果型（Result<T, PluginError>） | 不明 |

### Dependencies & Interactions

- 内部依存
  - copy_plugin_files → calculate_dest_path を使用して宛先パスを決定。
  - その他関数間の直接呼び出しはなし。
- 外部依存（クレート/モジュール）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | std::fs / std::io / std::path | ファイルI/O、エラー、パス操作 | 同期I/O |
  | walkdir::WalkDir | ディレクトリ走査（再帰） | symlink既定は非追跡 |
  | sha2::{Digest, Sha256} | SHA-256 ハッシュ計算 | 検証・整合性 |
  | super::error::{PluginError, PluginResult} | エラーと結果型 | 実装詳細は不明 |
  | tempfile::tempdir（tests） | 一時ディレクトリ | テスト補助 |
- 被依存推定
  - プラグイン管理（Installer/Uninstaller）ロジックから呼び出される想定。
  - CLI コマンド（install/remove/verify）など。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| copy_plugin_files | pub fn copy_plugin_files(source_dir: &Path, dest_dir: &Path, plugin_name: &str, file_list: &[String], force: bool, conflict_owner: impl Fn(&Path) -> Option<String>) -> PluginResult<Vec<String>> | 明示リストに基づくカテゴリ別コピー＋競合検出 | O(n + Σfile_bytes) | O(n · path_len) |
| copy_plugin_payload | pub fn copy_plugin_payload(source_dir: &Path, dest_dir: &Path, plugin_name: &str, force: bool, conflict_owner: impl Fn(&Path) -> Option<String>, already_copied: &[String]) -> PluginResult<Vec<String>> | ディレクトリ走査＋除外規則適用して一括コピー | O(m + Σfile_bytes) | O(k + path_len) |
| remove_plugin_files | pub fn remove_plugin_files(file_list: &[String]) -> PluginResult<()> | コピー済みファイル削除＋空親ディレクトリ削除 | O(n) + I/O | O(1) |
| verify_file_integrity | pub fn verify_file_integrity(file_path: &Path, expected_checksum: &str) -> PluginResult<bool> | 単一ファイルのSHA-256と期待値の比較 | O(file_bytes) | O(file_bytes) |
| calculate_integrity | pub fn calculate_integrity(file_paths: &[String]) -> PluginResult<String> | 複数ファイル連結によるSHA-256算出 | O(Σfile_bytes) | O(max_file_bytes) |

Data Contracts（入力/出力の前提）
- source_dir, dest_dir: 実在するディレクトリパスを想定。存在しない場合は create_dir_all で途中生成されることあり。
- plugin_name: ディレクトリ名として使用。OSのパスに許されない文字は未検証。
- file_list: 相対パス（例: "commands/foo.md"）。"./"で始まる場合はトリムされる。".."/絶対パスは未防御。
- conflict_owner: 競合ファイルの所有者表示を決定するコールバック。Noneの場合は "unknown"。
- already_copied: 文字列の正規化は "/" 区切り前提。OS差異と大小文字は未正規化。

以下、各APIの詳細。

### copy_plugin_files

1. 目的と責務
   - 指定された file_list を、カテゴリに応じて .claude/*/plugin_name 配下へコピーする。
   - 競合検出（destが存在し、force=false の場合に PluginError::FileConflict を返す）。
   - コピーされた宛先パス（"/" 区切りへ正規化済み）を Vec<String> で返す。

2. アルゴリズム（ステップ）
   - file_list を順に処理。
   - calculate_dest_path(dest_dir, plugin_name, file_path) で宛先決定。
   - dest_path.exists() && !force なら conflict_owner(dest_path) を呼び出し、FileConflictエラー。
   - 親ディレクトリを create_dir_all。
   - std::fs::copy(source_path → dest_path) 実行。
   - 宛先パスを "/" 区切りへ変換し、出力に追加。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| source_dir | &Path | ソースルート |
| dest_dir | &Path | 宛先ルート |
| plugin_name | &str | 名前空間ディレクトリ名 |
| file_list | &[String] | 相対ファイルパスのリスト |
| force | bool | 競合時に上書き許可 |
| conflict_owner | impl Fn(&Path) -> Option<String> | 競合ファイルの所有者を返すコールバック |

4. 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<Vec<String>> | コピーされた宛先パス（"/"区切り）のリスト |

5. 使用例
```rust
use std::path::Path;
use plugins::fsops::copy_plugin_files;

let copied = copy_plugin_files(
    Path::new("/src/plugin"),
    Path::new("/project"),
    "my-plugin",
    &vec![
        "commands/run.md".to_string(),
        "agents/bot.yaml".to_string(),
    ],
    false,
    |_| None, // 所有者不明扱い
)?;
// copied: ["/project/.claude/commands/my-plugin/run.md", "/project/.claude/agents/my-plugin/bot.yaml"]
```

6. エッジケース
- file_list に存在しないファイルが含まれる（std::fs::copy がエラー）。
- 宛先親ディレクトリの作成に失敗（権限不足）。
- file_path に ".." を含む（名前空間逸脱のリスク、現状未防御）。
- Windows バックslashを "/" に置換して返却するため、後続削除との不一致の懸念は薄いがOS差異は留意。

### copy_plugin_payload

1. 目的と責務
   - source_dir 配下の全ファイルを走査し、以下を除外して .claude/plugins/plugin_name 配下へコピー。
     - ディレクトリ（ファイルのみ対象）
     - ".git" を含むパス
     - already_copied に含まれるパス
     - "commands/", "agents/", "hooks/", "scripts/" で始まるパス（カテゴリ別コピー対象は除外）

2. アルゴリズム（ステップ）
   - already_copied を HashSet へ（O(k)）。
   - WalkDir::new(source_dir) で再帰走査。
   - 例外時は PluginError::IoError(io::Error::other(e)) に変換。
   - ディレクトリはスキップ。
   - relative = entry.path().strip_prefix(source_dir) を取得（失敗時は expect で panic）。
   - relative に ".git" を含むならスキップ。
   - normalized = relative を "/" 区切りへ正規化。
   - normalized が already/カテゴリ開始ならスキップ。
   - dest_path = dest_dir.join(".claude/plugins").join(plugin_name).join(relative)。
   - 競合検出（exists && !force → FileConflict）。
   - 親を create_dir_all。
   - fs::copy(entry.path(), dest_path)。
   - 宛先の "/" 区切り文字列を結果に追加。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| source_dir | &Path | ソースルート |
| dest_dir | &Path | 宛先ルート |
| plugin_name | &str | 名前空間ディレクトリ名 |
| force | bool | 競合時に上書き許可 |
| conflict_owner | impl Fn(&Path) -> Option<String> | 競合所有者問い合わせ |
| already_copied | &[String] | 既にコピー済み（"/"区切り前提）の相対パス集合 |

4. 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<Vec<String>> | コピーされた宛先パス（"/"区切り）のリスト |

5. 使用例
```rust
use std::path::Path;
use plugins::fsops::copy_plugin_payload;

let copied = copy_plugin_payload(
    Path::new("/src/plugin"),
    Path::new("/project"),
    "my-plugin",
    true,      // 強制上書き
    |_| None,
    &vec!["README.md".to_string()], // 既にコピー済み扱い
)?;
```

6. エッジケース
- symlink を含む場合、プラットフォームによって fs::copy がリンク先をコピーしうる（外部ファイル漏洩のリスク）。
- strip_prefix の expect による panic（極端なFSケースで source 外のパスが混入した場合）。
- ".git" スキップはコンポーネント一致で行うが、大小文字/代替名には非対応。
- already_copied の正規化不一致（バックスラッシュ等）で誤判定。

7. Mermaid（主要分岐）
```mermaid
flowchart TD
  A[WalkDir entry] --> B{is_dir?}
  B -- yes --> A
  B -- no --> C{relative under source?}
  C -- no --> P[Panic (expect)]  %% 注意: 現状panic
  C -- yes --> D{contains ".git"?}
  D -- yes --> A
  D -- no --> E[normalize to "/"]
  E --> F{already_copied contains?}
  F -- yes --> A
  F -- no --> G{starts with commands/agents/hooks/scripts?}
  G -- yes --> A
  G -- no --> H{dest exists AND !force?}
  H -- yes --> I[Err(FileConflict)]
  H -- no --> J[create_dir_all(parent)]
  J --> K[fs::copy]
  K --> L[push normalized dest to output]
  L --> A
```
上記の図は `copy_plugin_payload` 関数（行番号不明）の主要分岐を示す。

### remove_plugin_files

1. 目的と責務
   - 渡されたファイルパスを削除し、親ディレクトリの削除を試みる（空の場合のみ成功）。

2. アルゴリズム（ステップ）
   - 文字列パスから Path を生成。
   - path.exists() なら remove_file。
   - 親ディレクトリがあれば remove_dir を試みる（失敗は無視）。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| file_list | &[String] | 削除対象の絶対（またはプロジェクト相対）パス |

4. 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<()> | 正常完了時は Ok(()) |

5. 使用例
```rust
use plugins::fsops::remove_plugin_files;

remove_plugin_files(&vec![
  "/project/.claude/commands/my-plugin/run.md".to_string()
])?;
```

6. エッジケース
- path.exists() を満たさない場合は何もしない。
- 親ディレクトリにファイルが残っている場合、remove_dir は失敗するが無視される。
- 権限不足で削除失敗（エラーとして返される）。

### verify_file_integrity

1. 目的と責務
   - 単一ファイルのSHA-256を計算し、期待チェックサムと一致するかを返す。

2. アルゴリズム（ステップ）
   - fs::read(file_path) で全内容をメモリへ読み込み。
   - Sha256::new → update(content) → finalize。
   - 16進文字列へフォーマットし、expected と比較。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| file_path | &Path | 対象ファイル |
| expected_checksum | &str | 期待SHA-256（hex） |

4. 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<bool> | 一致すれば true、不一致は false |

5. 使用例
```rust
use std::path::Path;
use plugins::fsops::verify_file_integrity;

let ok = verify_file_integrity(Path::new("README.md"),
                               "e3b0c442...")?;
```

6. エッジケース
- 大容量ファイルでメモリ使用量が増える（fs::readは全読み込み）。
- ファイル非存在や権限問題はエラー。

### calculate_integrity

1. 目的と責務
   - 複数ファイルの内容を順にハッシュへ投入し、ファイル間に改行を挿入して全体チェックサムを生成。

2. アルゴリズム（ステップ）
   - Sha256::new。
   - file_paths を順に、存在チェックして fs::read → hasher.update(content) → hasher.update(b"\n")。
   - finalize して hex 文字列に。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| file_paths | &[String] | 対象ファイル群のパス |

4. 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<String> | 集合チェックサム（hex） |

5. 使用例
```rust
use plugins::fsops::calculate_integrity;

let sum = calculate_integrity(&vec![
  "/project/.claude/commands/my-plugin/run.md".to_string(),
  "/project/.claude/agents/my-plugin/bot.yaml".to_string(),
])?;
```

6. エッジケース
- 非存在ファイルはスキップ（存在チェック後に読み込み）。結果は既存ファイルのみで計算される。
- 巨大ファイルが連続すると一時的なメモリ使用が増える。

## Walkthrough & Data Flow

- copy_plugin_files
  - 入力: source_dir + file_list。
  - 処理: calculate_dest_path → 競合検出 → create_dir_all → fs::copy。
  - 出力: 宛先のパス文字列（"/" 区切り）。
  - データフローは直線的で、1ファイルにつき独立処理。

- copy_plugin_payload
  - 入力: source_dir 全走査。
  - フィルタリング: is_dir, ".git" 含有, already_copied に含まれる, 特定カテゴリ開始で除外。
  - 宛先: .claude/plugins/plugin_name/relative。
  - 競合検出 → 親作成 → コピー → 出力リストに追加。

- calculate_dest_path のカテゴリ分岐（Mermaid）
```mermaid
flowchart TD
  A[file_path] --> B{starts with "commands"?}
  B -- yes --> C[strip "commands" and join dest/.claude/commands/plugin_name/relative]
  B -- no --> D{starts with "agents"?}
  D -- yes --> E[dest/.claude/agents/...]
  D -- no --> F{starts with "hooks"?}
  F -- yes --> G[dest/.claude/hooks/...]
  F -- no --> H{starts with "scripts"?}
  H -- yes --> I[dest/.claude/scripts/...]
  H -- no --> J[dest/.claude/plugins/plugin_name/path]
```
上記の図は `calculate_dest_path` 関数（行番号不明）の主要分岐を示す。

## Complexity & Performance

- 時間計算量
  - copy_plugin_files: O(n + Σfile_bytes)。nは file_list 長。I/O支配。
  - copy_plugin_payload: O(m + Σfile_bytes)。mは WalkDir のエントリ数。
  - remove_plugin_files: O(n)＋I/O。
  - verify_file_integrity: O(file_bytes)。
  - calculate_integrity: O(Σfile_bytes)。

- 空間計算量
  - copy_plugin_files: 宛先リスト分 O(n · path_len)。
  - copy_plugin_payload: HashSet 作成 O(k)（already_copied）。ファイル単位で一時メモリ使用。
  - verify_file_integrity: fs::read により O(file_bytes)。
  - calculate_integrity: O(max_file_bytes)。

- ボトルネック・スケール限界
  - 大容量ファイルでメモリ消費（verify/calculate）は増加。ストリーミングハッシュへ改善余地。
  - WalkDir の全探索はファイル数が多いと時間がかかる（除外ルールを早期適用してもI/Oは発生）。

- 実運用負荷要因
  - ディスクI/O、権限、ファイルロック、アンチウイルスの干渉など。
  - Windows/Unixでのパス区切りと symlink の扱い差異。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価
- メモリ安全性
  - unsafe 未使用。所有権/借用は標準的で安全。
  - fs::read で大容量ファイルの読み込みによりメモリ圧迫の可能性（DoS的側面）。
- インジェクション
  - Path Traversal: file_list/relative に ".." コンポーネントが含まれる場合、dest_dir 配下の名前空間を逸脱して書き込み可能。現状防御なし（copy_plugin_files/calculate_dest_path/copy_plugin_payload の join がそのまま受け入れる）。
  - コマンド/SQLインジェクションは対象外。
- 認証・認可
  - 権限チェックは OS に委任。操作前の明示的な認可はなし。
- 秘密情報
  - ハードコード秘密はなし。
  - ログ出力がないため漏洩はないが監査性も低い。
- 並行性
  - TOCTOU: exists→copy が原子的でないため、他プロセスが間に介入すると競合検出がすり抜ける可能性。
  - 複数並行インストール時の競合処理は未設計。

詳細エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| パスに「..」 | "commands/../../secrets.txt" | 名前空間外へは書き込まない（拒否） | join によりそのまま適用 | 問題あり |
| 絶対パス混入 | "/etc/passwd" | 拒否 | trim_start_matches("./")のみ | 問題あり |
| symlink | source_dir に外部を指すリンク | リンクを辿らない/拒否 | fs::copy が辿る場合あり | 問題あり |
| 競合レース | 他プロセスが直後に作成 | 原子的に失敗 | exists→copy の非原子的 | 問題あり |
| strip_prefix panic | 異常エントリ | エラーとして返す | expect により panic | 問題あり |
| already_copied 正規化 | "\\" 区切り | 同値として扱う | "/" 文字列のみ考慮 | 要改善 |
| 大容量ファイル検証 | 10GB | ストリーミングで検証 | fs::read 全読み込み | 非効率 |
| .git 除外 | "src/.git/config" | 除外 | コンポーネント等価のみ | 妥当だが限定的 |
| 権限不足 | 宛先不可 | エラー | ? で伝播 | 妥当 |
| 不存在ソース | file_listに誤り | エラー | fs::copy が失敗 | 妥当 |

Rust特有の観点（詳細チェックリスト）
- 所有権
  - 関数引数は不変借用（&Path, &str, &[String]）。クロージャ conflict_owner は Fn で不変呼び出し（行番号不明）。
- 借用
  - 可変借用はなし。Vec<String> への push のみ自己所有。
- ライフタイム
  - 明示的ライフタイムなしで十分。Path/strの借用範囲は関数内に限定。
- unsafe境界
  - unsafe ブロックなし。
- 並行性・非同期
  - Send/Sync に関する境界指定なし（同期関数）。共有状態なし。
  - await 境界なし。キャンセル対応なし。
- エラー設計
  - Result による伝播。"?" を多用。copy_plugin_payload の strip_prefix で expect 使用は改善余地。
  - io::Error から PluginError への変換の妥当性は super::error 実装に依存（不明）。

## Design & Architecture Suggestions

- Path Traversal対策
  - safe_join(dest_root, relative) を導入し、relative.components() を検査して **".." や RootDir** を拒否。
  - normalize: "./" や重複区切りを正規化。
- symlinkの扱い
  - WalkDir に対して follow_links(false) を明示、entry.file_type().is_symlink() をチェックして拒否。
  - fs::copy の前にメタデータでシンボリックリンク判定してスキップ。
- 競合の原子性
  - 宛先を OpenOptions::new().write(true).create_new(true) で作成後、コピーは一時ファイル + 原子的 rename を採用。
  - 上書き（force=true）のときも一時ファイル→置き換えにする。
- panic除去
  - strip_prefix(...).ok_or_else(|| PluginError::...) に置換してエラーで返す。
- 正規化一貫性
  - already_copied の取り扱いは OS 区切り（std::path）で行い、比較は Path 正規化に基づく。
- ストリーミングハッシュ
  - verify/calculate は BufReader + hasher.update(chunk) のストリーミング処理へ変更。
- ログ・監査
  - 成功/失敗/スキップ理由を INFO/DEBUG/ERROR で記録。衝突の所有者もログ出力。

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト
  - calculate_dest_path のカテゴリ分岐。
  - copy→remove の往復（tempdir使用）。

- 追加すべきユニットテスト
  - Path Traversal 拒否（"commands/../../x" が Err を返すこと）。
  - symlink スキップ（シンボリックリンクを作成し、コピー対象から除外）。
  - conflict_owner の動作（存在ファイルで "owner" が返されること）。
  - force=false/true の分岐（競合時に Err/成功）。
  - already_copied の除外判定（区切り違いのケースも）。
  - ".git" 除外（深いパスに含まれる場合）。
  - verify_file_integrity の一致/不一致、巨大ファイルのメモリ使用抑制（ストリーミング変更後）。

- サンプル（Path Traversal 防止の期待テスト）
```rust
#[test]
fn test_path_traversal_rejected() {
    use std::fs;
    use tempfile::tempdir;

    let tmp = tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(src.join("commands")).unwrap();
    fs::write(src.join("commands/ok.md"), "content").unwrap();

    // 悪意あるパス
    let files = vec!["commands/../../evil.txt".to_string()];

    // 改修後の calculate_dest_path が Err を返す前提
    let result = copy_plugin_files(&src, &dest, "p", &files, false, |_| None);
    assert!(result.is_err(), "path traversal should be rejected");
}
```

- 統合テスト
  - copy_plugin_payload で大量ファイル＋除外規則が期待通りに働くか。
  - Windows/Unix の区切り差異をCIマトリクスで検証。

## Refactoring Plan & Best Practices

1. calculate_dest_path の防御的設計
   - Path Componentsを使ってカテゴリ名を厳密一致→残りは相対のみ許可、".." を拒否。
2. fs::copy のラップ
   - copy_atomic(src, dst, overwrite: bool) を新設。create_new + 書き込み + rename。
3. symlink対策
   - is_symlink を検知して Err またはスキップするポリシーを決める。
4. エラーの一貫性
   - expect を排除し、PluginError に正規化。FileConflict 時に全件報告（まとめて）したいなら収集戦略を導入。
5. ハッシュ計算のストリーミング化
   - BufReader + chunking で大容量に耐える。
6. 共有ユーティリティ
   - 正規化（to_unix_style）やフィルタ（is_excluded_path）を共通化。

## Observability (Logging, Metrics, Tracing)

- ログ
  - レベル: INFO（開始/完了/件数）、DEBUG（スキップ理由、除外規則）、ERROR（失敗詳細）。
  - コンテキスト: plugin_name, source/dest, ファイル数、失敗パス。
- メトリクス
  - コピー成功/失敗件数、競合件数、スキップ件数、総バイト数、所要時間。
- トレーシング
  - インストール操作IDを紐付け、各ファイル処理をspanで囲む。

## Risks & Unknowns

- PluginError/PluginResult の詳細仕様・変換 From 実装はこのチャンクには現れない（不明）。
- fs::copy の symlink挙動はプラットフォーム依存。明示的対策が必要。
- ".git" 除外の要件（大小文字、ワークツリー以外）は仕様不明。
- plugin_name に使用できる文字種（OSごとの制約）は仕様不明。
- 競合検出の期待動作（一部だけ失敗時のロールバックなど）は不明。

以上を踏まえ、現行実装はシンプルで分かりやすい一方、パス安全性と原子性、symlink対策の改善が重要です。改善により、より堅牢で安全なプラグインインストール基盤へと進化できます。