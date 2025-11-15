Path: contributing\README.md

```markdown
[Documentation](../README.md) / **コントリビューション**

---

# コントリビューション

コントリビューション歓迎！このセクションでは開発環境の設定とガイドラインについて説明します。

## 本セクションの内容

- **[Development](development.md)** - 開発環境のセットアップ
- **[Adding Languages](adding-languages.md)** - 新しい言語パーサーの追加方法
- **[Testing](testing.md)** - テスト基盤とガイドライン

## コントリビュータ向けクイックスタート

1. リポジトリをフォークする  
2. フォークしたリポジトリをクローンする  
3. プロジェクトをビルド:
   ```bash
   cargo build --release
   ```
4. テストを実行:
   ```bash
   cargo test
   ```

## 開発用コマンド

```bash
# Build the project
cargo build --release

# Run tests
cargo test

# Build and run in development mode
cargo run -- <command>
```

## ガイドライン

詳細なコントリビューションガイドラインについては、ルートの [CONTRIBUTING.md](../../CONTRIBUTING.md) を参照してください。

## 言語サポートの追加

新しい言語をサポートに追加する際は:
1. パーサートレイトを実装する  
2. 必要に応じて言語固有の解決処理を追加する  
3. 包括的なテストを追加する  
4. ドキュメントを更新する  

## 次のステップ

- まずはメインの [CONTRIBUTING.md](../../CONTRIBUTING.md) を読む  
- 内部構造を理解するために [Architecture](../architecture/) を参照  
- 使用パターンを理解するために [User Guide](../user-guide/) を確認  

[ドキュメントへ戻る](../README.md)
```