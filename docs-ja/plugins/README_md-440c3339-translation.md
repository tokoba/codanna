[ドキュメント](../README.md) / **プラグイン**

---

# プラグイン

Codanna のプラグインはプロジェクト単位で管理されます。グローバルではなくプロジェクトディレクトリ内の `.claude/` にインストールされるため、プロジェクトごとに異なるプラグインバージョンを保持できます。

## codanna-cc プラグイン

Claude Code の `/plugin` コマンド、または codanna の CLI から利用可能です。

**Claude Code 経由:**
```bash
# Codanna マーケットプレイスを追加
/plugin marketplace add bartolli/codanna-plugins

# プラグインをインストール
/plugin install codanna-cc@codanna-plugins
```

**Codanna CLI 経由:**
```bash
codanna plugin add https://github.com/bartolli/codanna-plugins.git codanna
codanna plugin add https://github.com/bartolli/codanna-plugins.git codanna --ref v1.2.0  # 特定バージョン
```

CLI 方法ならバージョン管理が可能で、プロジェクトごとに異なるタグをインストールできます。

### トークン効率の高いワークフロー

このプラグインには JSON 出力を解析してトークン消費を抑える Node.js スクリプトが含まれています。例は [codanna-plugins](https://github.com/bartolli/codanna-plugins) を参照してください。

**例: Node.js ラッパーとパイプ処理**
```bash
# Node スクリプトが JSON の解析と整形を担当
node .claude/scripts/codanna/context-provider.js find "error handling" --limit=3

# 出力にはフォローアップ用の symbol_id が含まれる
# 1. IndexError (Enum) [symbol_id:205]
#    使用例: node .claude/scripts/codanna/context-provider.js calls symbol_id:205
```

この方式により、結果を AI アシスタントへ渡す前に前処理することでトークン使用量を削減します。

## クイックスタート

**コアプラグインをインストール**

```bash
codanna plugin add https://github.com/bartolli/codanna-plugins.git codanna
```

**プラグインを更新**

```bash
codanna plugin update my-plugin
```

**プラグインを削除**

```bash
codanna plugin remove my-plugin
```

**インストール済みプラグインを一覧表示**

```bash
codanna plugin list --verbose
```

**プラグインの整合性を検証**

```bash
codanna plugin verify my-plugin
```

**プラグイン追加時の処理**

プラグインを追加すると codanna は以下を行います:

1. マーケットプレイスリポジトリを一時ディレクトリへクローン
2. プラグインマニフェスト (.claude-plugin/plugin.json) を検証
3. 既存プラグインとのファイル競合をチェック
4. コンポーネントファイルを名前空間付きディレクトリへコピー  
   - Commands → .claude/commands/<plugin>/  
   - Agents → .claude/agents/<plugin>/  
   - Hooks → .claude/hooks/<plugin>/  
   - Scripts → .claude/scripts/<plugin>/  
   - その他 → .claude/plugins/<plugin>/  

5. MCP サーバー設定を .mcp.json にマージ
6. すべてのインストール済みファイルの整合性チェックサム (SHA-256) を計算
7. ロックファイル (.codanna/plugins/lockfile.json) を更新

## 高度なオプション

**特定 Git リファレンス (ブランチ/タグ/コミット) をインストール**

```bash
codanna plugin add https://github.com/user/marketplace.git my-plugin --ref v1.2.0
```

**強制インストール (競合を上書き)**

```bash
codanna plugin add https://github.com/user/marketplace.git my-plugin --force
```

**事前確認 (インストールせず変更点を確認)**

```bash
codanna plugin add https://github.com/user/marketplace.git my-plugin --dry-run
```

ロールバック保護: いずれかのステップが失敗した場合、codanna は自動で  
- 部分的にコピーされたファイルを削除  
- 旧バージョンのプラグインを復元 (更新時)  
- MCP 設定を復元  
- ディレクトリをクリーンアップ  

**プラグインの更新**

Git のコミット SHA 比較で変更を検出します:

**最新コミットへ更新**

```bash
codanna plugin update my-plugin
```

**特定リファレンスへ更新**

```bash
codanna plugin update my-plugin --ref main
```

**強制再インストール (コミットチェックを無視)**

```bash
codanna plugin update my-plugin --force
```

**更新プロセス:**

1. Git リポジトリからリモートのコミット SHA を取得
2. インストール済みコミットと比較  
   - 同一コミット + 検証成功 → "Already up to date"  
   - 同一コミット + 検証失敗 → 再インストール  
   - 異なるコミット → 更新  

3. 既存プラグインをバックアップ
4. 古いバージョンを完全にアンインストール
5. 新バージョンをインストール
6. インストール失敗時はバックアップへロールバック

## プラグインの削除

**安全な削除と完全クリーンアップ**

```bash
codanna plugin remove my-plugin
```

**強制削除 (安全チェックをスキップ)**

```bash
codanna plugin remove my-plugin --force
```

**削除をプレビュー**

```bash
codanna plugin remove my-plugin --dry-run
```

クリーンアップ処理:

1. 追跡対象ファイルをファイルシステムから削除
2. .mcp.json から MCP サーバーエントリを削除
3. プラグインディレクトリ (.claude/plugins/<plugin>/ など) をクリーンアップ
4. ロックファイルからプラグインエントリを削除

プラグイン格納構造

```text
.claude/
├── commands/<plugin>/ # スラッシュコマンド
├── agents/<plugin>/   # カスタムエージェント
├── hooks/<plugin>/    # イベントフック
├── scripts/<plugin>/  # ユーティリティスクリプト
└── plugins/<plugin>/  # 追加ファイル

.codanna/
└── plugins/
└── lockfile.json      # 整合性チェックサム付きインストール記録
```

**ロックファイル構造**

ロックファイル (.codanna/plugins/lockfile.json) はインストール済みプラグインを追跡します:

```json
{
  "version": "1.0.0",
  "plugins": {
    "my-plugin": {
      "name": "my-plugin",
      "version": "1.0.0",
      "commit": "abc123def456...",
      "marketplace_url": "https://github.com/user/marketplace.git",
      "installed_at": "2025-10-17T13:58:03Z",
      "updated_at": "2025-10-17T14:00:00Z",
      "integrity": "sha256:...",
      "files": [".claude/commands/my-plugin/command.md"],
      "mcp_keys": ["my-plugin-server"],
      "source": {
        "type": "marketplace_path",
        "relative": "plugins/my-plugin"
      }
    }
  }
}
```

## MCP サーバー統合

プラグインは MCP サーバーを提供でき、プロジェクトの .mcp.json にマージされます。

インストール前:

```json
{
  "mcpServers": {
    "existing-server": { "command": "cmd" }
  }
}
```

プラグインに MCP サーバーが含まれる場合のインストール後:

```json
{
  "mcpServers": {
    "existing-server": { "command": "cmd" },
    "my-plugin-server": {
      "command": "node",
      "args": ["server.js"]
    }
  }
}
```

競合処理: MCP サーバーキーが既に存在する場合、--force を使わない限りインストールは失敗します (--force で既存エントリを上書き)。

## 検証と整合性

いつでもプラグインの整合性を検証できます:

### 特定プラグインを検証

```bash
codanna plugin verify my-plugin --verbose
```

検証内容:

1. 追跡ファイルがすべて存在するか
2. ファイル内容が SHA-256 チェックサムと一致するか
3. MCP サーバーキーが .mcp.json に存在するか

検証失敗は改ざんや破損を示します。`--force` 付きで再インストールしてください。

## プラグイン一覧

### 基本一覧

```bash
codanna plugin list
```

### 詳細情報付き

```bash
codanna plugin list --verbose
```

### スクリプト用 JSON 出力

```bash
codanna plugin list --json
```

詳細出力には以下が含まれます:

- プラグイン名とバージョン
- インストール・更新日時
- Git コミット SHA
- マーケットプレイス URL
- インストール済みファイル数
- MCP サーバーキー

コマンドリファレンス

| コマンド                                   | 説明                         | フラグ                         |
| ------------------------------------------ | ---------------------------- | ------------------------------ |
| codanna plugin add <marketplace> <plugin>  | マーケットプレイスから追加   | --ref, --force, --dry-run      |
| codanna plugin remove <plugin>             | プラグインを削除             | --force, --dry-run             |
| codanna plugin update <plugin>             | 最新バージョンへ更新         | --ref, --force, --dry-run      |
| codanna plugin list                        | インストール済みを一覧表示   | --verbose, --json              |
| codanna plugin verify <plugin>             | プラグインの整合性を検証     | --verbose                      |

共通フラグ:

- --ref <ref>: Git ブランチ、タグ、コミット SHA を指定
- --force: 競合や安全チェックを無視して実行
- --dry-run: 実行せず変更点をプレビュー
- --verbose: 詳細情報を表示
- --json: JSON 形式で出力 (スクリプト用)

安全機能

1. トランザクション方式のインストール: 失敗時は自動ロールバック
2. ファイル競合検出: 他プラグインのファイル上書きを防止 (--force で無効化可)
3. 整合性検証: SHA-256 チェックサムで改ざんや破損を検出
4. バックアップと復元: 更新前に旧バージョンをバックアップ
5. MCP 競合検出: 重複する MCP サーバーキーを防止 (--force で上書き可)
6. 名前空間ディレクトリ: 各プラグインを専用サブディレクトリで分離

エラー処理

よくあるエラーと対処方法:

| エラー                 | 原因                             | 解決策                                                     |
| ---------------------- | -------------------------------- | ---------------------------------------------------------- |
| AlreadyInstalled       | 既にプラグインが存在             | --force を付けて再インストール                             |
| FileConflict           | 他プラグインのファイルと競合     | 競合の所有者を確認し、--force で上書き                     |
| McpServerConflict      | MCP キーが既に存在               | サーバー名を変更するか --force を使用                      |
| IntegrityCheckFailed   | ファイルの改ざん/破損            | `codanna plugin update <name> --force` で再インストール    |
| PluginNotFound         | マーケットプレイスに存在しない   | プラグイン名と URL を確認                                  |
| InvalidPluginManifest  | マニフェストの検証に失敗         | プラグイン作者に連絡し修正を依頼                           |

プラグインの作成

独自プラグインを作るには:

1. 以下の構成で Git リポジトリを作成:

```text
   my-plugin/
   ├── .claude-plugin/
   │ └── plugin.json # 必須マニフェスト
   ├── commands/     # 任意: スラッシュコマンド
   ├── agents/       # 任意: カスタムエージェント
   ├── hooks/        # 任意: イベントフック
   └── scripts/      # 任意: ユーティリティスクリプト
```

2. マニフェスト (.claude-plugin/plugin.json) を定義:

```json
{
    "name": "my-plugin",
    "version": "1.0.0",
    "description": "このプラグインの機能",
    "author": { "name": "Your Name" },
    "commands": "./commands",
    "agents": "./agents"
}
```

3. マーケットプレイスマニフェスト (.claude-plugin/marketplace.json) を作成:

```json
{
    "name": "My Marketplace",
    "version": "1.0.0",
    "plugins": [
    {
        "name": "my-plugin",
        "description": "Plugin description",
        "source": {
        "type": "marketplace_path",
        "relative": "."
        }
    }
    ]
}
```

4. Git に公開し、リポジトリ URL を共有

ユーザーは以下でインストールできます:

```bash
codanna plugin add https://github.com/you/my-plugin.git my-plugin
```

---

[ドキュメントへ戻る](../README.md)