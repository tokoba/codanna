Path: integrations\http-server.md

```markdown
# HTTP/HTTPS サーバー

リアルタイムのファイル監視を備えた永続的サーバー。

## HTTP サーバー

OAuth 認証付きで実行:

```bash
# HTTP server with OAuth authentication (requires http-server feature)
codanna serve --http --watch
```

## HTTPS サーバー

TLS 暗号化で実行:

```bash
# HTTPS server with TLS encryption (requires https-server feature)
codanna serve --https --watch
```

## 設定

### MCP クライアント設定

`.mcp.json` に設定:
```json
{
  "mcpServers": {
    "codanna-sse": {
      "type": "sse",
      "url": "http://127.0.0.1:8080/mcp/sse"
    }
  }
}
```

HTTPS を使用する場合:
```json
{
  "mcpServers": {
    "codanna-sse": {
      "type": "sse",
      "url": "https://127.0.0.1:8080/mcp/sse"
    }
  }
}
```

### カスタムバインドアドレス

```bash
# Bind to custom address and port
codanna serve --http --bind 0.0.0.0:3000

# Bind to all interfaces on port 8080
codanna serve --http --bind 0.0.0.0:8080
```

## 機能

- 永続的なサーバープロセス
- 複数クライアント対応
- `--watch` によるリアルタイムファイル監視
- OAuth 認証 (HTTP)
- TLS 暗号化 (HTTPS)

## 高度なセットアップ

自己署名証明書を使用した詳細な HTTPS セットアップについては、[HTTPS Setup Guide](https-setup.md) を参照してください。

## 利点

- 複数のクライアントが同一サーバーに接続可能
- クライアントセッション間でサーバーが持続
- 集中的なインデックス管理
- 適切な設定によりネットワーク経由でアクセス可能

## セキュリティ上の注意

- HTTP モードでは OAuth による認証を提供
- HTTPS モードでは TLS による暗号化を提供
- デフォルトでは localhost のみ (127.0.0.1) にバインド
- 0.0.0.0 (全インターフェース) にバインドする場合は注意

## 参考

- [Serve コマンド](../user-guide/cli-reference.md#codanna-serve)
- [MCP ツール](../user-guide/mcp-tools.md)
- [Agent Guidance](agent-guidance.md)
```