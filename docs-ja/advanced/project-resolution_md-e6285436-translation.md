Path: advanced\project-resolution.md

```markdown
# プロジェクト固有のパス解決

Codanna はプロジェクトの設定ファイルを理解し、インポートを正しく解決します。

## TypeScript

`tsconfig.json` を読み取り、パスエイリアスを解決します。

### 設定

```toml
# .codanna/settings.toml
[languages.typescript]
enabled = true
config_files = [
    "tsconfig.json",
    "packages/web/tsconfig.json"  # For monorepos
]
```

### 動作概要

TypeScript コードで `@app/utils` をインポートすると、Codanna は `tsconfig.json` のパスマッピングを利用して実際のファイル位置（`src/app/utils`）へ解決します。これはモノレポ内のモジュール間でも機能します。

**プロセス:**
1. Codanna がプロジェクトの設定ファイル（`tsconfig.json`）を読み取る  
2. パスエイリアス、`baseUrl` などの解決ルールを抽出  
3. それらを `.codanna/index/resolvers/` に保存  
4. インデックス作成時にこれらのルールを用いてインポートを正確に解決  

### 例

以下の `tsconfig.json` がある場合:

```json
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@app/*": ["src/app/*"],
      "@utils/*": ["src/utils/*"],
      "~/components/*": ["components/*"]
    }
  }
}
```

Codanna は次のように解決します:  
- `@app/main` → `src/app/main`  
- `@utils/config` → `src/utils/config`  
- `~/components/Button` → `components/Button`

### モノレポ対応

複数の `tsconfig.json` があるモノレポの場合:

```toml
[languages.typescript]
config_files = [
    "tsconfig.json",
    "packages/web/tsconfig.json",
    "packages/api/tsconfig.json"
]
```

各設定ファイルのパスマッピングは、そのスコープ内のファイルに適用されます。

## 今後の対応予定

### Python
`pyproject.toml` を用いたプロジェクト固有のインポート解決:
- パッケージ探索
- 名前空間パッケージ
- 編集可能インストール

### Go
`go.mod` を使ったモジュール解決:
- モジュールパス解決
- 置換ディレクティブ
- ローカルモジュール参照

### その他の言語
必要に応じて言語固有のインポート解決を提供します。

## 利点

- **正確なインポート解決** - プロジェクトのルールに従います  
- **モジュール間ナビゲーション** - モノレポでも機能  
- **パスエイリアス対応** - `@app/*`, `~/utils/*` などのパターンをサポート  
- **手動設定不要** - 既存のプロジェクト設定を読み込みます  

## トラブルシューティング

### インポートが解決されない

設定ファイルが列挙されていることを確認してください:
```bash
codanna config | grep config_files
```

`tsconfig.json` 内のパスが正しいか確認します。

### モノレポの問題

すべての関連する `tsconfig.json` が settings.toml に列挙されていることを確認してください。

### 変更後の再インデックス

パスエイリアスを変更した後は再インデックスしてください:
```bash
codanna index . --force --progress
```

## 参照

- [Configuration](../user-guide/configuration.md) - 設定ガイドの完全版  
- [First Index](../getting-started/first-index.md) - 最初のインデックスを作成する
```