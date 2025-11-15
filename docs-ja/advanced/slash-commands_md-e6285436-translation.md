Path: advanced\slash-commands.md

```markdown
# スラッシュコマンド

Codanna はプラグインシステムを通じて Claude 用のカスタムスラッシュコマンドを提供します。

## プラグインで利用可能

スラッシュコマンドは現在プラグインとして配布されています。インテリジェントなコード探索ワークフローにアクセスするには、core プラグインをインストールしてください:

```bash
codanna plugin add https://github.com/bartolli/codanna-plugins.git codanna
```

## 含まれるコマンド

| コマンド | 説明 |
|---------|-------------|
| `/symbol <name>` | 完全なコンテキストでシンボルを検索・解析 |
| `/x-ray <query>` | 関係マッピング付きの深いセマンティック検索 |

## 仕組み

これらのコマンドは内部で Codanna の MCP ツールを使用しますが、包括的な解析と自動レポート生成を備えたガイド付きワークフローを提供します。

### `/symbol` コマンド

特定のシンボルを検索・解析します:
- シンボルを正確に検索
- 完全なコンテキストとドキュメント
- 関係マッピング
- 使用状況解析

### `/x-ray` コマンド

完全なコンテキストでの深いセマンティック検索:
- 自然言語クエリ
- コードのセマンティック理解
- 関係追跡
- 影響分析

## カスタムコマンドの作成

独自のスラッシュコマンドをプラグインとして作成できます。作成と配布の詳細は [Plugin Documentation](../plugins/) を参照してください。

## 参照

- [Plugin System](../plugins/) - プラグインのインストールと作成
- [MCP Tools](../user-guide/mcp-tools.md) - コマンドが使用する基盤ツール
- [Agent Guidance](../integrations/agent-guidance.md) - コマンドが AI アシスタントを案内する方法
```