```markdown
# プロファイルシステム

Codanna のプロファイルを使うと、チームは再利用可能な設定、フック、コマンドをパッケージ化できます。プロファイルは **プロバイダー**（git リポジトリまたはローカルフォルダー）によって配布され、各ワークスペースにインストールされます。レジストリは `~/.codanna` に保存されます。

---

## 基本概念

| 用語 | 説明 |
|------|------|
| **プロバイダー** | プロファイルのソース（GitHub 省略記法、git URL、ローカルパス） |
| **プロファイル** | マニフェスト、フック、オプションの MCP エージェントを含むバンドル |
| **グローバルレジストリ** | `~/.codanna/providers.json` に保存され、登録済みプロバイダーを管理 |
| **ワークスペースインストール** | プロジェクトの `.codanna/profiles.lock.json` にインストール済みプロファイルを記録 |

---

## 典型的なワークフロー

1. **プロバイダーを登録する**
   ```bash
   codanna profile provider add bartolli/codanna-profiles
   ```
2. **利用可能なプロファイルをプレビュー**
   ```bash
   codanna profile list --verbose
   ```
3. **現在のワークスペースにインストール**
   ```bash
   codanna profile install claude
   ```
4. **インストール済みプロファイルを確認**
   ```bash
   codanna profile status
   ```
5. **プロジェクトの進行に合わせて更新・検証**
   ```bash
   codanna profile update claude
   codanna profile verify claude
   ```

---

## プロバイダーのソース

| 種類 | 形式 | 例 |
|------|------|----|
| GitHub 省略記法 | `owner/repo` | `bartolli/codanna-profiles` |
| Git URL | `https://...` | `https://github.com/bartolli/codanna-profiles` |
| ローカルパス | 絶対または相対 | `/Users/name/my-profiles` |

### プロバイダーを登録

```bash
codanna profile provider add bartolli/codanna-profiles
codanna profile provider add https://github.com/org/profiles.git
codanna profile provider add ./my-profiles
```

### プロバイダーの削除 / 確認

```bash
codanna profile provider remove codanna-profiles
codanna profile provider list --verbose
```

---

## プロファイルのインストールと管理

| コマンド | 目的 | フラグ |
|----------|------|--------|
| `codanna profile install <name>` | プロファイルをワークスペースにインストール | `--force` |
| `codanna profile update <name>` | インストール済みプロファイルを更新 | `--force` |
| `codanna profile remove <name>` | プロファイルをアンインストール | `--verbose` |
| `codanna profile list` | プロバイダーからプロファイルを一覧表示 | `--verbose`, `--json` |
| `codanna profile status` | インストール済みプロファイルを表示 | `--verbose` |
| `codanna profile sync` | チーム設定からインストール | `--force` |
| `codanna profile verify [<name>]` | 整合性をチェック | `--all`, `--verbose` |

例:
```bash
codanna profile install claude
codanna profile update claude --force
codanna profile remove claude --verbose
codanna profile sync --force
codanna profile verify --all --verbose
```

---

## プロファイル構成

プロバイダーは次のレイアウトに従います:

```
.codanna-profile/
├── provider.json          # プロバイダーのメタデータ
└── profiles/
    └── profile-name/
        ├── profile.json   # マニフェスト（フック、プロンプト、要件）
        ├── .claude/       # Claude Code の指示 / アセット
        └── CLAUDE.md      # オプションのドキュメント
```

---

## 保存場所

| 位置 | 目的 |
|------|------|
| `~/.codanna/providers.json` | グローバルプロバイダーレジストリ |
| `~/.codanna/profiles/` | キャッシュされたプロバイダークローン |
| `<workspace>/.codanna/profiles.lock.json` | プロジェクト用にインストールされたプロファイル |

---

## ヒント

- インストール前に内容を確認したい場合は `--verbose` を使用してください。
- `codanna profile sync` はオンボーディングに最適です。リポジトリにロックファイルをコミットし、チームメンバーは `sync` を実行して環境を一致させます。
- CI で `profile verify` を実行して、ワークスペースが古くなったり改ざんされたフックを使用していないか確認しましょう。
- プロバイダーは複数のプロファイル（例: `backend`, `frontend`, `ops` など）をホストでき、チームは自由に組み合わせて利用できます。

CLI コマンドの構文については、[CLI リファレンス](../user-guide/cli-reference.md#profile-system)を参照してください。
```