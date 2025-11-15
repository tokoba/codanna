# Windows互換性問題と修正ガイド

**プロジェクト**: Codanna v0.7.0
**調査日**: 2025-11-15
**対象**: Windows プラットフォームでの使用における互換性問題

## 目次

1. [はじめに](#はじめに)
2. [現在の状態](#現在の状態)
3. [問題点の詳細](#問題点の詳細)
4. [修正方法](#修正方法)
5. [推奨される実装順序](#推奨される実装順序)
6. [テスト戦略](#テスト戦略)
7. [参考資料](#参考資料)

---

## はじめに

Codannaは、AIアシスタント向けのコードインテリジェンスツールで、MCP (Model Context Protocol) サーバーとして動作します。Rustで実装されており、複数のプログラミング言語（Rust、Python、TypeScript、Kotlin、Go、PHP、C、C++、C#、GDScript）のコード解析をサポートしています。

本ドキュメントでは、Windows環境での使用時に発生する可能性のある互換性問題を特定し、それぞれの問題に対する修正方法を提供します。

**READMEの現状表明**:
```markdown
## Current Status
- Windows support is experimental
```

---

## 現在の状態

### プラットフォーム固有のコード

プロジェクトには既にいくつかのプラットフォーム固有の実装が存在しており、基本的なWindows対応は行われています：

**実装済みの対応箇所**:
- シンボリックリンク作成の分岐 (`src/init.rs:165-184`)
- ファイルロック時のリトライロジック (`src/storage/persistence.rs:233-267`)
- パス区切り文字の正規化 (複数箇所で `replace('\\', '/')`)
- テストでのプラットフォーム固有処理 (`tests/integration/test_settings_init_integration.rs`)

**使用されている主要な依存関係**:
```toml
git2 = { version = "0.20.2", features = ["vendored-openssl"] }  # ✓ Windows対応済み
notify = "8.2.0"                                                # ✓ クロスプラットフォーム
walkdir = "2.5.0"                                               # ✓ クロスプラットフォーム
ignore = "0.4.23"                                               # ✓ クロスプラットフォーム
fastembed = "5.2.0"                                             # ✓ クロスプラットフォーム
dirs = "6.0.0"                                                  # ✓ クロスプラットフォーム
```

---

## 問題点の詳細

### 問題 1: シンボリックリンクの作成 - 権限要件

**深刻度**: 🔴 高
**影響範囲**: 初期化プロセス、モデルキャッシュ
**ファイル**: `src/init.rs:120-187`

#### 問題の説明

Windowsでは、シンボリンクの作成にデフォルトで管理者権限またはDeveloper Modeの有効化が必要です。これにより、一般ユーザーが `codanna init` を実行した際にエラーが発生する可能性があります。

#### 現在のコード

```rust
// src/init.rs:165-184
#[cfg(unix)]
{
    std::os::unix::fs::symlink(&global_models, &local_cache)?;
    println!(
        "Created symlink: {} -> {}",
        local_cache.display(),
        global_models.display()
    );
}

#[cfg(windows)]
{
    // Windows requires different handling
    std::os::windows::fs::symlink_dir(&global_models, &local_cache)?;
    println!(
        "Created symlink: {} -> {}",
        local_cache.display(),
        global_models.display()
    );
}
```

#### 問題点

1. **権限エラー**: 管理者権限なしでは `symlink_dir` が失敗する
2. **エラーハンドリング不足**: 失敗時の代替手段がない
3. **ユーザー体験**: 一般ユーザーには使用が困難

#### 実際のエラー例

```
Error: Os { code: 1314, kind: Uncategorized, message: "A required privilege is not held by the client." }
```

#### 修正方法

**オプション A: ディレクトリジャンクションの使用（推奨）**

Windowsのディレクトリジャンクションはシンボリックリンクと似ていますが、管理者権限が不要です。

```rust
// src/init.rs に追加
#[cfg(windows)]
fn create_directory_junction(target: &Path, link: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::symlink_dir;
    use std::process::Command;

    // First try symlink_dir (works on Developer Mode)
    if let Ok(_) = symlink_dir(target, link) {
        return Ok(());
    }

    // Fallback to junction using mklink /J
    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J",
               &link.to_string_lossy(),
               &target.to_string_lossy()])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to create junction"
        ))
    }
}
```

**オプション B: シンボリックリンクなしで直接使用（より安全）**

```rust
// src/init.rs:120-187 を以下に置き換え
pub fn create_fastembed_symlink() -> Result<(), std::io::Error> {
    let local_cache = PathBuf::from(fastembed_cache_name());
    let global_models = models_dir();

    // Check if symlink/junction already exists and is correct
    if local_cache.exists() {
        if local_cache.is_symlink() {
            let target = std::fs::read_link(&local_cache)?;
            if target == global_models {
                println!(
                    "Cache link already exists: {} -> {}",
                    local_cache.display(),
                    global_models.display()
                );
                return Ok(());
            }
            // Remove incorrect symlink
            std::fs::remove_file(&local_cache)?;
        } else if local_cache.is_dir() {
            // Real directory exists, don't delete user data
            eprintln!(
                "Warning: {} exists and is not a symlink",
                local_cache.display()
            );
            eprintln!("         Models will be downloaded locally");
            return Ok(());
        }
    }

    // Try to create symlink/junction
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&global_models, &local_cache)?;
        println!(
            "Created symlink: {} -> {}",
            local_cache.display(),
            global_models.display()
        );
        return Ok(());
    }

    #[cfg(windows)]
    {
        // Try multiple strategies on Windows

        // Strategy 1: Try symlink_dir (works with Developer Mode)
        if std::os::windows::fs::symlink_dir(&global_models, &local_cache).is_ok() {
            println!(
                "Created symlink: {} -> {}",
                local_cache.display(),
                global_models.display()
            );
            return Ok(());
        }

        // Strategy 2: Try junction via mklink command
        if let Ok(output) = std::process::Command::new("cmd")
            .args([
                "/C", "mklink", "/J",
                &local_cache.to_string_lossy(),
                &global_models.to_string_lossy(),
            ])
            .output()
        {
            if output.status.success() {
                println!(
                    "Created junction: {} -> {}",
                    local_cache.display(),
                    global_models.display()
                );
                return Ok(());
            }
        }

        // Strategy 3: Fall back to informing user
        eprintln!("Note: Could not create cache link (requires elevated privileges or Developer Mode)");
        eprintln!("      You can enable Developer Mode in Windows Settings:");
        eprintln!("      Settings > Update & Security > For developers > Developer Mode");
        eprintln!("      Or run as administrator once to create the cache link.");
        eprintln!("      Models will work without the link, but will use more disk space.");

        // Don't fail - the application can work without symlinks
        return Ok(());
    }
}
```

**オプション C: FastEmbed 5.0+ の `with_cache_dir()` APIを活用**

既にコメントで言及されていますが、完全に実装されていません。

```rust
// src/vector/embedding.rs または該当ファイルで
use fastembed::{EmbeddingModel, InitOptions};

// グローバルモデルディレクトリを直接指定
let model = EmbeddingModel::try_new(
    InitOptions::new(ModelInfo::default())
        .with_cache_dir(crate::init::models_dir())
)?;
```

この方法により、シンボリックリンクが不要になります。

---

### 問題 2: ファイルロックとアクセス権限

**深刻度**: 🟡 中
**影響範囲**: インデックス削除、ファイル操作
**ファイル**: `src/storage/persistence.rs:219-270`

#### 問題の説明

Windowsでは、ファイルロックの動作がUnixと異なり、以下の状況でファイルの削除が失敗する可能性があります：

1. **アンチウイルスソフトウェア**: ファイルをスキャン中にロック
2. **インデクサー**: Windows Searchなどがファイルをインデックス中
3. **ファイルハンドルの遅延クローズ**: Rustのドロップタイミングとの相互作用

#### 現在のコード

```rust
// src/storage/persistence.rs:222-270
pub fn clear(&self) -> Result<(), std::io::Error> {
    let tantivy_path = self.base_path.join("tantivy");
    if tantivy_path.exists() {
        // On Windows, we may need multiple attempts due to file locking
        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 3;

        loop {
            match std::fs::remove_dir_all(&tantivy_path) {
                Ok(()) => break,
                Err(e) if attempts < MAX_ATTEMPTS => {
                    attempts += 1;

                    // Retry logic for file locking issues
                    #[cfg(windows)]
                    {
                        // Windows-specific: Check for permission denied (code 5)
                        if e.kind() == std::io::ErrorKind::PermissionDenied {
                            eprintln!(
                                "Attempt {attempts}/{MAX_ATTEMPTS}: Windows permission denied ({e}), retrying after delay..."
                            );

                            // Force garbage collection to release any handles
                            std::hint::black_box(());

                            // Brief delay to allow file handles to close
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            continue;
                        }
                    }

                    // On non-Windows or non-permission errors, log and retry with delay
                    eprintln!(
                        "Attempt {attempts}/{MAX_ATTEMPTS}: Failed to remove directory ({e}), retrying..."
                    );
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        // Recreate the empty tantivy directory after clearing
        std::fs::create_dir_all(&tantivy_path)?;

        // On Windows, add extra delay after recreating directory to ensure filesystem is ready
        #[cfg(windows)]
        {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    Ok(())
}
```

#### 分析

既存のコードは良好な対応を実装していますが、以下の改善が可能です：

#### 改善案

```rust
// src/storage/persistence.rs の clear() メソッドを強化
pub fn clear(&self) -> Result<(), std::io::Error> {
    let tantivy_path = self.base_path.join("tantivy");
    if !tantivy_path.exists() {
        return Ok(());
    }

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 5; // 3→5に増加

    // Windows固有: より長い初期遅延
    #[cfg(windows)]
    {
        // ファイルハンドルが完全にクローズされるまで待機
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    loop {
        match std::fs::remove_dir_all(&tantivy_path) {
            Ok(()) => break,
            Err(e) if attempts < MAX_ATTEMPTS => {
                attempts += 1;

                let retry_delay = match e.kind() {
                    std::io::ErrorKind::PermissionDenied => {
                        #[cfg(windows)]
                        {
                            eprintln!(
                                "Attempt {}/{}: Access denied (antivirus or file lock?), retrying...",
                                attempts, MAX_ATTEMPTS
                            );
                            // 長めの遅延（指数バックオフ）
                            std::time::Duration::from_millis(100 * (1 << attempts))
                        }
                        #[cfg(not(windows))]
                        {
                            std::time::Duration::from_millis(100)
                        }
                    }
                    _ => {
                        eprintln!(
                            "Attempt {}/{}: Failed to remove directory ({}), retrying...",
                            attempts, MAX_ATTEMPTS, e
                        );
                        std::time::Duration::from_millis(100 * attempts as u64)
                    }
                };

                std::thread::sleep(retry_delay);
                continue;
            }
            Err(e) => {
                // 最終的なエラーメッセージを詳細化
                return Err(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to remove directory after {} attempts: {}{}",
                        MAX_ATTEMPTS,
                        e,
                        if cfg!(windows) {
                            "\nSuggestion: Close any programs accessing the index, or temporarily disable antivirus"
                        } else {
                            ""
                        }
                    ),
                ));
            }
        }
    }

    // ディレクトリを再作成
    std::fs::create_dir_all(&tantivy_path)?;

    #[cfg(windows)]
    {
        // ファイルシステムが準備完了するまで待機
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    Ok(())
}
```

**追加の安全策: ファイルハンドルの明示的クローズ**

```rust
// Tantivy のインデックスライターを使用している箇所で
impl Drop for SimpleIndexer {
    fn drop(&mut self) {
        // 明示的にリソースをクリーンアップ
        if let Some(writer) = self.writer.take() {
            let _ = writer.commit();
            // Windows: 追加の遅延
            #[cfg(windows)]
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
```

---

### 問題 3: パス区切り文字の扱い

**深刻度**: 🟡 中
**影響範囲**: プラグイン、プロファイル、パス表示
**ファイル**: 複数

#### 問題の説明

Windowsでは `\` をパス区切り文字として使用しますが、多くのツールやライブラリは `/` も受け入れます。しかし、パスの表示や比較時に一貫性がないと問題が発生する可能性があります。

#### 現在の対応状況

すでに多くの箇所で正規化が実装されています：

```rust
// src/plugins/fsops.rs:39
let dest_str = dest_path.to_string_lossy().replace('\\', "/");

// src/plugins/fsops.rs:74
let normalized = relative.to_string_lossy().replace('\\', "/");

// src/plugins/fsops.rs:99
let dest_str = dest_path.to_string_lossy().replace('\\', "/");

// src/profiles/fsops.rs:64
let normalized = relative.to_string_lossy().replace('\\', "/");

// src/profiles/fsops.rs:143
.replace('\\', "/");
```

#### 問題点

1. **一貫性の欠如**: 一部の箇所でのみ正規化が行われている
2. **パス比較の問題**: 正規化されていないパス同士の比較が失敗する可能性
3. **ユーザー表示**: エラーメッセージでのパス表示が不統一

#### 修正方法

**ヘルパー関数の導入**

```rust
// src/lib.rs または src/utils.rs に追加
/// Normalize path separators to forward slashes for cross-platform consistency
///
/// This is particularly important for:
/// - Path storage in configuration files
/// - Path comparison across platforms
/// - Path display to users
pub fn normalize_path_separators(path: impl AsRef<Path>) -> String {
    path.as_ref()
        .to_string_lossy()
        .replace('\\', "/")
}

/// Normalize a PathBuf to use forward slashes
pub fn normalize_pathbuf(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// Compare two paths for equality, handling path separator differences
pub fn paths_equal(a: &Path, b: &Path) -> bool {
    // Canonicalize if possible, otherwise compare normalized strings
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => normalize_path_separators(a) == normalize_path_separators(b),
    }
}
```

**使用例**

```rust
// src/plugins/fsops.rs の修正例
use crate::normalize_path_separators;

pub fn copy_plugin_files(...) -> PluginResult<Vec<String>> {
    let mut copied_files = Vec::new();

    for file_path in file_list {
        let source_path = source_dir.join(file_path);
        let dest_path = calculate_dest_path(dest_dir, plugin_name, file_path);

        // ... (コピー処理)

        // 統一されたヘルパーを使用
        copied_files.push(normalize_path_separators(&dest_path));
    }

    Ok(copied_files)
}
```

---

### 問題 4: Git操作 - SSH認証とパス

**深刻度**: 🟡 中
**影響範囲**: プロファイルとプラグインのGitリポジトリからのクローン
**ファイル**: `src/profiles/git.rs`, `src/plugins/resolver.rs`

#### 問題の説明

Windowsでは、SSH認証の設定がUnixと異なる場合があります：

1. **SSH鍵の場所**: `~/.ssh/` vs `%USERPROFILE%\.ssh\`
2. **SSH-Agent**: Windowsでは別途設定が必要
3. **Git認証情報**: Windows Credential Managerとの統合

#### 現在のコード

```rust
// src/profiles/git.rs:79-107
fn credential_callback(
    _url: &str,
    username_from_url: Option<&str>,
    allowed_types: CredentialType,
) -> Result<Cred, git2::Error> {
    // Try SSH key from agent first
    if allowed_types.is_ssh_key() {
        if let Ok(cred) = Cred::ssh_key_from_agent(username_from_url.unwrap_or("git")) {
            return Ok(cred);
        }
    }

    // Try default credentials (netrc, etc.)
    if let Ok(cred) = Cred::default() {
        return Ok(cred);
    }

    // Try username/password from environment
    if allowed_types.is_user_pass_plaintext() {
        if let (Ok(username), Ok(password)) =
            (std::env::var("GIT_USERNAME"), std::env::var("GIT_PASSWORD"))
        {
            return Cred::userpass_plaintext(&username, &password);
        }
    }

    Err(git2::Error::from_str("no credentials available"))
}
```

#### 良い点

- `git2` crateの `vendored-openssl` featureを使用しているため、OpenSSLの依存関係が解決されている
- クレデンシャルのフォールバックメカニズムがある

#### 改善案

```rust
// src/profiles/git.rs の credential_callback を強化
fn credential_callback(
    url: &str,
    username_from_url: Option<&str>,
    allowed_types: CredentialType,
) -> Result<Cred, git2::Error> {
    let username = username_from_url.unwrap_or("git");

    // Strategy 1: SSH key from agent
    if allowed_types.is_ssh_key() {
        if let Ok(cred) = Cred::ssh_key_from_agent(username) {
            return Ok(cred);
        }

        // Strategy 2: Try default SSH key locations
        #[cfg(windows)]
        {
            if let Ok(home) = std::env::var("USERPROFILE") {
                let ssh_dir = PathBuf::from(home).join(".ssh");

                // Try common key files
                for key_name in &["id_rsa", "id_ed25519", "id_ecdsa"] {
                    let private_key = ssh_dir.join(key_name);
                    let public_key = ssh_dir.join(format!("{}.pub", key_name));

                    if private_key.exists() {
                        if let Ok(cred) = Cred::ssh_key(
                            username,
                            Some(&public_key),
                            &private_key,
                            None,
                        ) {
                            return Ok(cred);
                        }
                    }
                }
            }
        }

        #[cfg(unix)]
        {
            if let Ok(home) = std::env::var("HOME") {
                let ssh_dir = PathBuf::from(home).join(".ssh");

                for key_name in &["id_rsa", "id_ed25519", "id_ecdsa"] {
                    let private_key = ssh_dir.join(key_name);
                    let public_key = ssh_dir.join(format!("{}.pub", key_name));

                    if private_key.exists() {
                        if let Ok(cred) = Cred::ssh_key(
                            username,
                            Some(&public_key),
                            &private_key,
                            None,
                        ) {
                            return Ok(cred);
                        }
                    }
                }
            }
        }
    }

    // Strategy 3: Default credentials (netrc, credential manager, etc.)
    if let Ok(cred) = Cred::default() {
        return Ok(cred);
    }

    // Strategy 4: Username/password from environment
    if allowed_types.is_user_pass_plaintext() {
        if let (Ok(username), Ok(password)) =
            (std::env::var("GIT_USERNAME"), std::env::var("GIT_PASSWORD"))
        {
            return Cred::userpass_plaintext(&username, &password);
        }
    }

    // Strategy 5: Credential helper (Windows Credential Manager, etc.)
    #[cfg(windows)]
    {
        if allowed_types.is_user_pass_plaintext() {
            // libgit2 should automatically use Windows Credential Manager via Cred::default()
            // but we've already tried that above
        }
    }

    Err(git2::Error::from_str(
        "No credentials available. Please set up SSH keys or configure Git credentials."
    ))
}
```

**Windows用の追加ドキュメント**

READMEまたはドキュメントに追加：

```markdown
### Windows での Git 認証設定

Codanna がGitリポジトリから profiles/plugins をクローンする際、以下の認証方法を試行します：

1. **SSH Agent** (推奨)
   ```powershell
   # OpenSSH Authentication Agent サービスを有効化
   Start-Service ssh-agent
   Set-Service ssh-agent -StartupType Automatic

   # SSH鍵を追加
   ssh-add ~\.ssh\id_rsa
   ```

2. **SSH鍵ファイル**
   - `%USERPROFILE%\.ssh\id_rsa`
   - `%USERPROFILE%\.ssh\id_ed25519`
   - `%USERPROFILE%\.ssh\id_ecdsa`

3. **Git Credential Manager**
   ```powershell
   # Git Credential Manager のインストール（Git for Windowsに含まれる）
   git config --global credential.helper manager-core
   ```

4. **環境変数** (最後の手段)
   ```powershell
   $env:GIT_USERNAME="your-username"
   $env:GIT_PASSWORD="your-token"
   ```
```

---

### 問題 5: ファイルウォッチャー - プラットフォーム固有の動作

**深刻度**: 🟢 低
**影響範囲**: ファイル変更の自動検出
**ファイル**: `src/indexing/fs_watcher.rs`

#### 問題の説明

`notify` crateは各プラットフォームで異なるバックエンドを使用します：
- **Windows**: ReadDirectoryChangesW API
- **Linux**: inotify
- **macOS**: FSEvents

これにより、イベントのタイミングや種類が異なる場合があります。

#### 現在のコード分析

```rust
// src/indexing/fs_watcher.rs:95-99
let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
    // Send events to our async channel
    // We use blocking_send because this callback is sync
    let _ = tx.blocking_send(res);
})
```

`notify` crateの `recommended_watcher` は各プラットフォームに最適なバックエンドを自動選択するため、基本的には問題ありません。

#### 潜在的な問題

1. **イベントの重複**: Windowsでは一部のファイル操作で複数のイベントが発生する
2. **パス形式**: Windowsではバックスラッシュでパスが返される可能性
3. **大文字小文字**: Windowsはcase-insensitiveだが、パス比較がcase-sensitive

#### 改善案

```rust
// src/indexing/fs_watcher.rs の watch() メソッドを強化
pub async fn watch(mut self) -> IndexResult<()> {
    // ... (既存のセットアップコード)

    // Convert paths to absolute paths in HashSet for efficient lookup
    // The notify crate gives us absolute paths, but our index stores relative paths
    let mut indexed_set: HashSet<PathBuf> = indexed_paths
        .into_iter()
        .map(|p| {
            let absolute = if p.is_absolute() {
                p
            } else {
                workspace_root.join(&p)
            };

            // Windowsでパスを正規化
            #[cfg(windows)]
            {
                // Canonicalize to get consistent case
                absolute.canonicalize().unwrap_or(absolute)
            }
            #[cfg(not(windows))]
            {
                absolute
            }
        })
        .collect();

    // ... (イベントハンドリングループ)
    loop {
        // ...
        tokio::select! {
            Some(res) = self.event_rx.recv() => {
                match res {
                    Ok(event) => {
                        // Handle different event types for indexed files
                        for path in &event.paths {
                            // Windowsでパスを正規化
                            #[cfg(windows)]
                            let normalized_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                            #[cfg(not(windows))]
                            let normalized_path = path.clone();

                            if indexed_set.contains(&normalized_path) {
                                match event.kind {
                                    EventKind::Modify(_) => {
                                        // Windows: 短時間に複数のModifyイベントが来る可能性があるため
                                        // debounceが重要
                                        pending_changes.insert(normalized_path.clone(), Instant::now());
                                    }
                                    // ... (その他のイベントタイプ)
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Windows固有のエラーに対する詳細なメッセージ
                        #[cfg(windows)]
                        {
                            eprintln!("File watch error: {}", e);
                            eprintln!("Note: This may occur if the watched directory was moved or if");
                            eprintln!("      antivirus software is blocking file system notifications.");
                        }
                        #[cfg(not(windows))]
                        {
                            eprintln!("File watch error: {e}");
                        }
                    }
                }
            }
            // ...
        }
    }

    Ok(())
}
```

---

### 問題 6: 環境変数とホームディレクトリ

**深刻度**: 🟢 低（既に対応済み）
**影響範囲**: グローバル設定ディレクトリ
**ファイル**: `src/init.rs:35-43`

#### 現在のコード

```rust
// src/init.rs:35-43
pub fn global_dir() -> PathBuf {
    GLOBAL_DIR
        .get_or_init(|| {
            dirs::home_dir()
                .expect("Failed to determine home directory")
                .join(GLOBAL_DIR_NAME)
        })
        .clone()
}
```

#### 分析

✅ **既に適切に対応されています**

`dirs` crateは各プラットフォームで適切なディレクトリを返します：
- **Windows**: `%USERPROFILE%` (例: `C:\Users\username`)
- **Unix**: `$HOME` (例: `/home/username`)

#### 推奨事項

現在の実装で問題ありませんが、エラーメッセージを改善できます：

```rust
pub fn global_dir() -> PathBuf {
    GLOBAL_DIR
        .get_or_init(|| {
            dirs::home_dir()
                .unwrap_or_else(|| {
                    eprintln!("Warning: Could not determine home directory");
                    eprintln!("Using current directory for global config");
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                })
                .join(GLOBAL_DIR_NAME)
        })
        .clone()
}
```

---

### 問題 7: コンパイルとビルドの依存関係

**深刻度**: 🟡 中
**影響範囲**: ビルドプロセス
**ファイル**: `Cargo.toml`

#### 問題の説明

Windows でのビルド時に必要な依存関係やツールが不足している可能性があります。

#### 現在の依存関係

```toml
git2 = { version = "0.20.2", features = ["vendored-openssl"] }  # ✓ Good!
```

`vendored-openssl` feature により、OpenSSLのシステム依存がなくなり、Windows でのビルドが容易になっています。

#### 確認事項

**必要なツール**:
1. **Rust toolchain**: `rustup` でインストール
2. **C/C++ compiler**:
   - Visual Studio Build Tools (推奨)
   - または MinGW-w64
3. **CMake**: 一部のネイティブ依存関係のビルドに必要

#### 推奨: ビルド手順のドキュメント化

`README.md` に Windows セクションを追加：

```markdown
### Windows でのビルド

#### 前提条件

1. **Rust のインストール**
   ```powershell
   # https://rustup.rs/ から rustup をインストール
   rustup-init.exe
   ```

2. **Visual Studio Build Tools のインストール** (推奨)
   - [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) をダウンロード
   - "Desktop development with C++" workload を選択してインストール

   または

   **MinGW-w64 のインストール** (代替)
   ```powershell
   # Chocolatey を使用
   choco install mingw

   # または MSYS2
   # https://www.msys2.org/ からインストール
   ```

3. **CMake のインストール** (オプション、一部の依存関係で必要)
   ```powershell
   choco install cmake
   ```

#### ビルド

```powershell
# すべてのフィーチャーを有効にしてビルド
cargo build --release --all-features

# HTTPサーバー機能を除外（より軽量）
cargo build --release
```

#### トラブルシューティング

**問題: "link.exe が見つかりません"**
- Visual Studio Build Tools がインストールされていることを確認
- 「x64 Native Tools Command Prompt for VS 2022」から cargo を実行

**問題: OpenSSL 関連のエラー**
- `vendored-openssl` feature が有効になっていることを確認（デフォルトで有効）
- または `OPENSSL_DIR` 環境変数を設定

**問題: git2 のビルドエラー**
```powershell
# libgit2 のビルドに失敗する場合
$env:LIBGIT2_SYS_USE_PKG_CONFIG = "0"
cargo build --release
```
```

---

### 問題 8: テストの互換性

**深刻度**: 🟢 低
**影響範囲**: テストスイート
**ファイル**: 複数のテストファイル

#### 問題の説明

一部のテストがUnix固有の動作に依存している可能性があります。

#### 現在の対応状況

いくつかのテストは既にプラットフォーム固有の処理を実装しています：

```rust
// tests/integration/test_settings_init_integration.rs:31-46
#[cfg(unix)]
{
    std::os::unix::fs::symlink(&models_dir, &cache_path)
        .or({
            Ok::<(), std::io::Error>(())
        })
        .expect("Should handle symlink creation");
}

#[cfg(windows)]
{
    std::os::windows::fs::symlink_dir(&models_dir, &cache_path)
        .or(Ok::<(), std::io::Error>(()))
        .expect("Should handle symlink creation");
}
```

#### 推奨事項

**プラットフォーム固有のテストスキップ**

```rust
// Windowsで失敗する可能性のあるテストをスキップ
#[test]
#[cfg(unix)]  // Unix でのみ実行
fn test_symlink_creation() {
    // ...
}

// または条件付きでスキップ
#[test]
fn test_file_permissions() {
    if cfg!(windows) {
        println!("Skipping on Windows - different permission model");
        return;
    }
    // ... テストコード
}
```

**テストヘルパーの追加**

```rust
// tests/common/mod.rs
#[cfg(windows)]
pub fn create_test_link(target: &Path, link: &Path) -> std::io::Result<()> {
    // Try symlink first, fall back to junction
    std::os::windows::fs::symlink_dir(target, link)
        .or_else(|_| {
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J",
                       &link.to_string_lossy(),
                       &target.to_string_lossy()])
                .output()
                .and_then(|output| {
                    if output.status.success() {
                        Ok(())
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Failed to create link"
                        ))
                    }
                })
        })
}

#[cfg(unix)]
pub fn create_test_link(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}
```

---

## 修正方法

### 推奨される実装順序

優先度と実装の複雑さに基づいた推奨順序：

#### フェーズ 1: 重要な修正（即座に実施）

1. **シンボリックリンクのフォールバック改善** (問題1)
   - 影響: 高
   - 複雑さ: 中
   - 工数: 2-4時間
   - ファイル: `src/init.rs`

2. **ファイルロックリトライの強化** (問題2)
   - 影響: 中
   - 複雑さ: 低
   - 工数: 1-2時間
   - ファイル: `src/storage/persistence.rs`

#### フェーズ 2: パス処理の統一（1週間以内）

3. **パス正規化ヘルパーの導入** (問題3)
   - 影響: 中
   - 複雑さ: 低
   - 工数: 2-3時間
   - ファイル: 新規 `src/path_utils.rs`、既存の複数ファイル

4. **Git認証の強化** (問題4)
   - 影響: 中
   - 複雑さ: 中
   - 工数: 3-4時間
   - ファイル: `src/profiles/git.rs`

#### フェーズ 3: ポリッシュとドキュメント（2週間以内）

5. **ファイルウォッチャーの改善** (問題5)
   - 影響: 低
   - 複雑さ: 低
   - 工数: 1-2時間
   - ファイル: `src/indexing/fs_watcher.rs`

6. **Windows ビルド手順のドキュメント化** (問題7)
   - 影響: 中
   - 複雑さ: 低
   - 工数: 1-2時間
   - ファイル: `README.md`、新規 `docs/windows-setup.md`

7. **テストの改善** (問題8)
   - 影響: 低
   - 複雑さ: 中
   - 工数: 3-5時間
   - ファイル: `tests/` 配下の複数ファイル

#### フェーズ 4: 検証とリリース

8. **Windows での包括的テスト**
   - 工数: 4-8時間
   - 内容:
     - 初期化プロセス
     - インデックス作成
     - プラグイン/プロファイルのインストール
     - ファイルウォッチング
     - MCPサーバーモード

9. **ドキュメントの更新**
   - README の "Windows support is experimental" を更新
   - 既知の制限事項の明記

---

## テスト戦略

### 手動テストチェックリスト

Windows 10/11 環境で以下をテスト：

#### 基本機能

- [ ] `codanna init` の実行
  - [ ] 管理者権限なし
  - [ ] 管理者権限あり
  - [ ] Developer Mode 有効
  - [ ] Developer Mode 無効

- [ ] `codanna index <dir>` でインデックス作成
  - [ ] 小規模プロジェクト（< 100ファイル）
  - [ ] 中規模プロジェクト（100-1000ファイル）
  - [ ] 大規模プロジェクト（> 1000ファイル）

- [ ] `codanna serve` でMCPサーバー起動
  - [ ] stdio モード
  - [ ] http モード
  - [ ] ファイルウォッチング有効

#### プラグイン/プロファイル

- [ ] プロファイルのインストール
  - [ ] HTTPSリポジトリから
  - [ ] SSHリポジトリから（Git認証あり）
  - [ ] ローカルパスから

- [ ] プラグインのインストール
  - [ ] 競合なし
  - [ ] 既存ファイルとの競合

#### エッジケース

- [ ] スペースを含むパス
- [ ] 日本語を含むパス（`C:\Users\ユーザー名\...`）
- [ ] 長いパス（> 260文字）
- [ ] ネットワークドライブ
- [ ] OneDrive 同期フォルダ
- [ ] アンチウイルスが有効な環境

### 自動テストの追加

```rust
// tests/windows_integration.rs (新規作成)
#![cfg(windows)]

use codanna::*;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_init_without_admin() {
    // 管理者権限なしでの初期化をテスト
    let temp = TempDir::new().unwrap();
    let result = init_in_directory(temp.path());

    // エラーにならないこと（フォールバックが機能）
    assert!(result.is_ok());
}

#[test]
fn test_path_with_spaces() {
    let temp = TempDir::new().unwrap();
    let project_path = temp.path().join("my project");
    std::fs::create_dir(&project_path).unwrap();

    // スペース含むパスでインデックスが作成できること
    let result = create_index(&project_path);
    assert!(result.is_ok());
}

#[test]
fn test_long_path() {
    // 長いパス（> 260文字）でも動作することを確認
    // Windows の MAX_PATH 制限への対応
    let temp = TempDir::new().unwrap();
    let mut long_path = temp.path().to_path_buf();

    // 長いパスを生成
    for i in 0..20 {
        long_path = long_path.join(format!("very_long_directory_name_{}", i));
    }

    std::fs::create_dir_all(&long_path).ok();

    if long_path.to_string_lossy().len() > 260 {
        // パスが260文字を超える場合のみテスト
        let result = create_index(&long_path);
        // 失敗する可能性があるが、少なくともパニックしないこと
        if let Err(e) = result {
            println!("Long path not supported: {}", e);
        }
    }
}

#[test]
fn test_antivirus_file_lock_retry() {
    // ファイルロックのリトライロジックをテスト
    let temp = TempDir::new().unwrap();
    let index_path = temp.path().join("index");

    // インデックスを作成
    create_test_index(&index_path).unwrap();

    // ファイルをロック
    let lock_file = index_path.join("tantivy").join("meta.json");
    let _file_handle = std::fs::File::open(&lock_file).unwrap();

    // 削除を試みる（リトライが機能するはず）
    let result = clear_index(&index_path);

    // ファイルハンドルを閉じた後は成功するはず
    drop(_file_handle);
    std::thread::sleep(std::time::Duration::from_millis(100));
}
```

### CI/CD での Windows テスト

GitHub Actions ワークフローに Windows テストを追加：

```yaml
# .github/workflows/windows-test.yml (新規作成)
name: Windows Tests

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]

jobs:
  test-windows:
    runs-on: windows-latest

    steps:
    - uses: actions/checkout@v3

    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
        profile: minimal
        override: true

    - name: Build
      run: cargo build --release --all-features

    - name: Run tests
      run: cargo test --all-features

    - name: Run Windows-specific tests
      run: cargo test --test windows_integration

    - name: Test init without admin
      run: |
        cargo run -- init
        cargo run -- index examples/rust

    - name: Test with spaces in path
      shell: powershell
      run: |
        New-Item -ItemType Directory -Path "test project" -Force
        cargo run -- init --force
        cargo run -- index "test project"
```

---

## 参考資料

### Rust でのWindows プログラミング

- [The Rust Programming Language - OS-Specific Functionality](https://doc.rust-lang.org/std/os/windows/)
- [Cargo Book - Platform Specific Dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#platform-specific-dependencies)

### 使用している Crates のドキュメント

- [git2-rs - Git bindings for Rust](https://docs.rs/git2/)
- [notify - Cross-platform file system notification](https://docs.rs/notify/)
- [walkdir - Recursive directory traversal](https://docs.rs/walkdir/)
- [dirs - Platform-specific directories](https://docs.rs/dirs/)

### Windows 固有の問題

- [Windows Symbolic Links](https://learn.microsoft.com/en-us/windows/win32/fileio/symbolic-links)
- [Windows Developer Mode](https://learn.microsoft.com/en-us/windows/apps/get-started/enable-your-device-for-development)
- [Long Path Support in Windows](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation)

---

## まとめ

Codanna プロジェクトは既にいくつかのWindows 互換性対応を実装していますが、以下の改善により、Windowsでの使用体験を大幅に向上させることができます：

### 重要な改善点

1. ✅ **シンボリックリンクのフォールバック** - 管理者権限なしでも動作
2. ✅ **ファイルロックのリトライ強化** - アンチウイルスとの共存
3. ✅ **パス処理の統一** - クロスプラットフォームでの一貫性
4. ✅ **Git認証の改善** - Windows Credential Manager の活用
5. ✅ **ドキュメントの充実** - Windows ユーザー向けのガイド

### 期待される成果

これらの改善により、README の以下の記述を更新できます：

```markdown
## Current Status
- Production ready for supported languages on all platforms
- Windows: Fully supported with detailed setup documentation
```

### 次のステップ

1. フェーズ1の修正を実装（シンボリックリンク、ファイルロック）
2. Windows 環境でテスト
3. フィードバックに基づいて調整
4. 残りのフェーズを実装
5. READMEとドキュメントを更新
6. リリースノートに Windows サポート改善を記載

---

**Document Version**: 1.0
**Last Updated**: 2025-11-15
**Contributors**: Claude Code Analysis
