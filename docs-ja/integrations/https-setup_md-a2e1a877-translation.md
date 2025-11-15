Path: integrations\https-setup.md

```markdown
# 自己署名証明書を使用した MCP HTTPS サーバー

このガイドでは、特にローカル開発環境や本番環境への安全なデプロイ時に、Codanna の HTTPS MCP サーバーを自己署名証明書と共に使用する方法を説明します。

## 概要

HTTPS MCP サーバーは以下を提供します:
- **TLS/SSL 暗号化** による安全な通信
- **SSE (Server-Sent Events)** で Claude Code と互換性のあるトランスポート
- **OAuth2 認可フロー** による安全なアクセス制御
- **適切な X.509 属性を持つ自己署名証明書の生成**
- **Bearer トークン検証** による API セキュリティ

## 証明書の信頼性の課題

Claude Code は内部で Node.js を使用しており、Node.js はシステムの証明書ストアとは別に独自の証明書ストアを保持しています。そのため、オペレーティングシステム (macOS Keychain、Windows 証明書ストアなど) で証明書を信頼済みにしていても、Node.js はそれを認識しません。

自己署名証明書を持つ HTTPS サーバーに接続すると、以下の問題が発生します:
- Claude Code で `fetch failed` エラー
- `unable to verify the first certificate` エラー
- ブラウザでは証明書が信頼されているのに接続失敗

## 解決策: NODE_EXTRA_CA_CERTS

解決策は、`NODE_EXTRA_CA_CERTS` 環境変数を使用して Node.js に明示的に証明書を知らせることです。

## 手順

### 1. HTTPS サーバーを起動する

```bash
cargo run --all-features -- serve --https --watch
```

またはインストール済みの場合:
```bash
codanna serve --https --watch
```

初回実行時には以下を行います:
- 自己署名証明書を生成
- `~/Library/Application Support/codanna/certs/server.pem` (macOS) に保存
- 証明書詳細とフィンガープリントを表示

### 2. 証明書を標準的な場所にコピー

以下を実行し、生成された証明書をコピーします:

```bash
# SSL ディレクトリがない場合は作成
mkdir -p ~/.ssl

# 証明書をコピー
cp ~/Library/Application\ Support/codanna/certs/server.pem ~/.ssl/codanna-ca.pem
```

### 3. プロジェクトで MCP を設定

プロジェクトルートの `.mcp.json` に以下を追加:

```json
{
  "mcpServers": {
    "codanna-https": {
      "type": "sse",
      "url": "https://127.0.0.1:8443/mcp/sse"
    }
  }
}
```

### 4. 証明書を信頼して Claude Code を起動

```bash
NODE_EXTRA_CA_CERTS=~/.ssl/codanna-ca.pem claude
```

### 5. 接続を確認

Claude Code で `/mcp` コマンドを使用して接続状況を確認します。以下のように表示されれば成功です:

```
codanna-https  ✔ connected
```

## 代替セットアップ方法

### 方法 1: シェルエイリアス

`~/.bashrc` または `~/.zshrc` に追加:

```bash
alias claude-secure='NODE_EXTRA_CA_CERTS=~/.ssl/codanna-ca.pem claude'
```

使用例:
```bash
claude-secure
```

### 方法 2: システム全体での信頼 (macOS)

システム全体で信頼させる場合 (ただし Node.js では依然として NODE_EXTRA_CA_CERTS が必要):

```bash
sudo security add-trusted-cert -d -r trustRoot \
  -k /Library/Keychains/System.keychain \
  ~/Library/Application\ Support/codanna/certs/server.pem
```

## OAuth 認可フロー

HTTPS サーバーには完全な OAuth2 実装が含まれています:

1. **ディスカバリー**: `/.well-known/oauth-authorization-server`
2. **クライアント登録**: `/oauth/register`
3. **認可**: `/oauth/authorize`
4. **トークン交換**: `/oauth/token`

このフローは、サーバーに接続する際に Claude Code により自動的に処理されます。

## トラブルシューティング

### "fetch failed" エラー

**問題**: Claude Code で接続時に "fetch failed" と表示される。

**解決**: `NODE_EXTRA_CA_CERTS` を付けて Claude Code を実行しているか確認:
```bash
NODE_EXTRA_CA_CERTS=~/.ssl/codanna-ca.pem claude
```

### 証明書が既に存在する

**問題**: サーバーが証明書が既に存在すると表示するが、再生成したい。

**解決**: 既存の証明書を削除:
```bash
rm -rf ~/Library/Application\ Support/codanna/certs/
```

その後サーバーを再起動して新しい証明書を生成。

### 401 Unauthorized

**問題**: サーバーが 401 エラーを返す。

**解決**: OAuth フローは自動処理されるはずです。401 エラーが出る場合:
1. サーバーログで Bearer トークン検証メッセージを確認
2. `.mcp.json` で SSE トランスポートタイプになっているか確認
3. Claude Code で `/mcp` コマンドを使い再接続

### ブラウザでは接続できるが Claude Code では失敗する

**問題**: ブラウザで `https://127.0.0.1:8443/health` にアクセスできても Claude Code では失敗する。

**解決**: ブラウザはシステム証明書ストアを使用しますが、Node.js はそうではありません。必ず `NODE_EXTRA_CA_CERTS` を使用してください。

## セキュリティ考慮事項

### 開発環境向け

自己署名証明書はローカル開発環境で許容されます。`NODE_EXTRA_CA_CERTS` を使用する方法は、特定の証明書のみを信頼するため安全です。

### 本番環境向け

本番環境では以下の代替案を検討してください:

1. **Let's Encrypt**: certbot で無料の正規証明書を取得
2. **リバースプロキシ**: 有効な証明書を持つ nginx/caddy をサーバーの前段に配置
3. **クラウドプロバイダー**: AWS、GCP、Azure のマネージド証明書を利用
4. **企業 CA**: 組織内部の認証局を使用

### 絶対にやってはいけないこと

**本番環境で `NODE_TLS_REJECT_UNAUTHORIZED=0` を使用しないでください。**  
これはすべての証明書検証を無効化し、重大なセキュリティリスクとなります。

## 実装の詳細

HTTPS サーバー (`src/mcp/https_server.rs`) は以下を提供します:

- **証明書生成**: `rcgen` クレートを使用し、適切な X.509 属性を設定
- **TLS 設定**: `rustls` と `axum-server` により実装
- **ローカル IP 検出**: 証明書 SANs にローカルネットワーク IP を自動で追加
- **証明書の永続化**: サーバー再起動時に証明書を再利用
- **Bearer トークン検証**: API セキュリティ用のミドルウェア
- **OAuth2 エンドポイント**: 完全な認可コードフローを実装

## プラットフォーム別の注意事項

### macOS
証明書保存場所: `~/Library/Application Support/codanna/certs/`

### Linux
証明書保存場所: `~/.config/codanna/certs/`

### Windows
証明書保存場所: `%APPDATA%\codanna\certs\`

注意: Windows では NODE_EXTRA_CA_CERTS のパスにスラッシュを使用:
```cmd
set NODE_EXTRA_CA_CERTS=C:/Users/username/.ssl/codanna-ca.pem
claude
```

## 今後の改善予定

証明書信頼をより簡単にする方法を検討中です:

1. **自動信頼設定**: すべてを自動で行う `codanna trust-cert` コマンド
2. **証明書バンドル**: Claude Code が自動検出できる形式で CA 証明書を同梱
3. **プラットフォーム統合**: システム証明書ストアとのより良い統合
4. **ドキュメント**: 証明書問題検出時のアプリ内ガイダンス

## 参考文献

- [Node.js TLS ドキュメント](https://nodejs.org/api/tls.html#tlscreatesecurecontextoptions)
- [Claude Code MCP ドキュメント](https://docs.anthropic.com/en/docs/claude-code/mcp)
- [Model Context Protocol 仕様](https://modelcontextprotocol.io)
- [GitHub Issue #2899](https://github.com/anthropics/claude-code/issues/2899) - 自己署名証明書サポート

## まとめ

自己署名証明書には `NODE_EXTRA_CA_CERTS` を用いた追加設定が必要ですが、ローカルまたは限定された環境で HTTPS MCP サーバーを安全に運用する方法として有効です。  
最良の開発体験のため、`NODE_EXTRA_CA_CERTS` を自動で設定して Claude Code を起動するシェルエイリアスやラッパースクリプトの作成を推奨します。
```