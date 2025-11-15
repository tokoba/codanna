Path: integrations\claude-desktop.md

```markdown
# Claude Desktop 統合

Claude Desktop アプリケーションで Codanna を設定します。

## 設定

Claude Desktop では、異なる場所から実行されるため `--config` フラグが必要です。

(Mac の場合) `~/Library/Application Support/Claude/claude_desktop_config.json` に設定します:

```json
{
  "mcpServers": {
    "codanna": {
      "command": "codanna",
      "args": ["--config", "/absolute/path/to/your/project/.codanna/settings.toml", "serve", "--watch"]
    }
  }
}
```

`/absolute/path/to/your/project/` を実際のプロジェクトのパスに置き換えてください。

## なぜ --config フラグが必要か？

Claude Desktop はプロジェクトとは異なる作業ディレクトリから実行されるため、プロジェクトの設定ファイルへの絶対パスが必要です。

## 特長

- Claude Code と同じ機能
- `--watch` によるファイル監視
- stdio トランスポート

## 検証

設定後:
1. Claude Desktop を再起動します  
2. プロジェクトディレクトリで次を実行します:
   ```bash
   codanna mcp-test
   ```

## 複数プロジェクト

複数のプロジェクトで使用する場合は、次の方法があります:
1. プロジェクトごとに異なる設定ファイルを使用する  
2. プロジェクトを切り替える際に `claude_desktop_config.json` のパスを更新する  

## トラブルシューティング

- 相対パスではなく絶対パスを使用してください  
- 指定したパスに `.codanna/settings.toml` が存在することを確認してください  
- Codanna がシステムの PATH に登録されているか確認してください  

## 関連情報

- [MCP ツールリファレンス](../user-guide/mcp-tools.md)
- [設定](../user-guide/configuration.md)
- [Claude Code 統合](claude-code.md)
```