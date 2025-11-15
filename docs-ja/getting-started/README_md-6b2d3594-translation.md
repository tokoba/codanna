```markdown
[Documentation](../README.md) / **Getting Started**

---

# はじめに

Codanna へようこそ！ここから基本を学びましょう。

## このセクションについて

1. **[Quick Start](quick-start.md)** - 数分で始める
2. **[Installation](installation.md)** - システム要件とプラットフォーム別セットアップ
3. **[First Index](first-index.md)** - 最初の検索用インデックスを構築

## クイックプレビュー

```bash
# Install
cargo install codanna --all-features

# Setup
codanna init

# Index your code
codanna index src --progress

# Ask real questions
codanna mcp semantic_search_docs query:"where do we resolve symbol references" limit:3
```

## 次のステップ

- [User Guide](../user-guide/) - 完全な CLI リファレンス
- [Integrations](../integrations/) - Claude、Codex などのクライアントと接続
- [Advanced](../advanced/) - Unix 哲学とパワーフィーチャー

[Back to Documentation](../README.md)
```