```markdown
# Installation
# インストール

Detailed installation instructions for all platforms.
すべてのプラットフォーム向けの詳細なインストール手順です。

## Requirements
## 必要条件

- Rust 1.75+ (for development)
- ~150MB for model storage (downloaded on first use)
- A few MB for index storage (varies by codebase size)
- Rust 1.75+（開発用）
- モデル保存用に約150MB（初回使用時にダウンロード）
- インデックス保存用に数MB（コードベースのサイズにより変動）

## Install from Crates.io
## Crates.io からインストール

```bash
cargo install codanna --all-features
```

## System Dependencies
## システム依存関係

### Linux (Ubuntu/Debian)
```bash
sudo apt update && sudo apt install pkg-config libssl-dev
```

### Linux (CentOS/RHEL)
```bash
sudo yum install pkgconfig openssl-devel
```

### Linux (Fedora)
```bash
sudo dnf install pkgconfig openssl-devel
```

### macOS
No additional dependencies required.
追加の依存関係は不要です。

## Verify Installation
## インストールの確認

After installation, verify Codanna is working:
インストール後、Codanna が動作することを確認します。

```bash
# Check version
codanna --version

# Initialize configuration
codanna init

# Test MCP connection (for AI assistant integration)
codanna mcp-test
```

## Build from Source
## ソースからビルド

If you prefer to build from source:
ソースからビルドしたい場合:

```bash
# Clone the repository
git clone https://github.com/anthropics/codanna.git
cd codanna

# Build with all features
cargo build --release --all-features

# Binary will be at target/release/codanna
```

## Development Setup
## 開発環境セットアップ

For development:
開発する場合:

```bash
# Build the project
cargo build --release

# Run tests
cargo test

# Build and run in development mode
cargo run -- <command>
```

## Troubleshooting
## トラブルシューティング

### Linux: Missing pkg-config
If you see errors about pkg-config, install the system dependencies listed above for your distribution.
pkg-config に関するエラーが表示された場合は、上記のディストリビューション別システム依存関係をインストールしてください。

### Model Download
The embedding model (~150MB) downloads automatically on first use. Ensure you have a stable internet connection for the initial download.
埋め込みモデル（約150MB）は初回使用時に自動でダウンロードされます。最初のダウンロード時に安定したインターネット接続があることを確認してください。

## Next Steps
## 次のステップ

- Continue with [First Index](first-index.md) to create your first code index
- See [Configuration](../user-guide/configuration.md) for customization options
- Set up [Integrations](../integrations/) with your AI assistant
- 最初のコードインデックスを作成するには [First Index](first-index.md) を参照
- カスタマイズオプションについては [Configuration](../user-guide/configuration.md) を参照
- AI アシスタントとの統合は [Integrations](../integrations/) を設定
```