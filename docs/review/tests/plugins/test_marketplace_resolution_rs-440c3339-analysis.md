# plugins\test_marketplace_resolution.rs Review

## TL;DR

- 目的: Claude Pluginのマーケットプレイス定義からのプラグイン解決・インストール・更新判定を、Gitリポジトリを用いた擬似環境で検証するテスト群。
- 主要公開API: 呼び出し対象は外部の codanna::plugins の **add_plugin** と **update_plugin**。戻り値は Result（テストで .expect を使用）であることが推測可能。
- 複雑箇所: クロスプラットフォームな **file:// URL** 生成処理、マーケットプレイスの **pluginRoot** メタデータ解釈、外部Git **source object** の解決、ロックファイルの **updated_at** 比較による更新スキップ判定。
- 重大リスク: 並列テスト時のワークスペース衝突は原則回避されているが、外部状態（グローバル設定・環境変数）への依存がある場合の競合は未知。WindowsパスのURL化ミスによる取得失敗の可能性。
- Rust安全性: unsafe未使用、所有権・借用は単純。テストの大半が I/O 例外を **expect で即時パニック** させる設計。
- データ契約推測: .claude/commands 配下へのコマンドコピー、.codanna/plugins/lockfile.json の updated_at を用いた更新判定。
- パフォーマンス: Git初期化・ファイルコピーなど I/O が支配的。テストスケールでは問題なしだが、ファイル数増大時は線形に時間増加。

## Overview & Purpose

このファイルは、codanna プロジェクトのプラグイン管理（pluginsモジュール）に対する統合テストです。マーケットプレイスの `marketplace.json` と、各プラグインの `.claude-plugin/plugin.json` を用いたプラグイン解決の主要パスを検証します。

検証対象のシナリオは以下の通りです。
- marketplace メタデータ `pluginRoot` により、サブディレクトリ内にあるプラグインを解決してインストールできる。
- marketplace の `source` が **git source object** の場合に、外部Gitリポジトリをクローンしてインストールできる。
- 外部プラグインの更新で、ロックファイル上のコミットが一致する場合は「更新不要」として **updated_at** が変化しない。
- marketplaceが `strict: false` を指定する場合に、プラグイン側に `plugin.json` がなくても、marketplace記載の `commands` がコピーされる。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | with_temp_workspace | private | 一時ディレクトリでワークスペースを用意し、テストクロージャを実行 | Low |
| Function | load_workspace_settings | private | .codanna/settings.toml の作成・読み込みと Settings の初期化 | Low |
| Function | init_git_repo | private | 指定パスで Git リポジトリを初期化し、初回コミットを作成 | Med |
| Function | write_file | private | 親ディレクトリ作成とファイル書き込み | Low |
| Function | path_to_file_url | private | パスからクロスプラットフォームな file:// URL を生成 | Low |
| Function | create_marketplace_with_plugin_root | private | pluginRoot を含む marketplace とサブディレクトリプラグインを構築 | Med |
| Function | create_external_plugin_repo | private | 外部プラグイン用の Git リポジトリを構築 | Low |
| Test | install_uses_marketplace_plugin_root | #[test] | pluginRoot 経由のインストール検証 | Med |
| Test | install_supports_git_source_object | #[test] | git source object 経由インストール検証 | Med |
| Test | update_external_plugin_detects_up_to_date | #[test] | 更新不要判定（updated_at不変）検証 | Med |
| Test | install_allows_strict_false_without_plugin_manifest | #[test] | strict:false による marketplace 主導インストール検証 | Med |

### Dependencies & Interactions

- 内部依存
  - install_* / update_* テストは、with_temp_workspace → load_workspace_settings を用いて設定とワークスペースを用意。
  - marketplace・プラグイン生成は write_file と init_git_repo を利用。
  - 外部プラグインURL生成は path_to_file_url。
  - すべてのテストで最終的に plugins::add_plugin / plugins::update_plugin を呼び出し。

- 外部依存（推奨表）

| クレート/モジュール | 用途 |
|---------------------|------|
| codanna::Settings | ワークスペース設定の読み込みと初期化 |
| codanna::plugins | プラグインの追加・更新操作（テスト対象API） |
| git2 | リポジトリ初期化・インデックス追加・コミット作成 |
| tempfile | 一時ディレクトリの生成 |
| serde_json | ロックファイルJSONの読み込み・比較 |

- 被依存推定
  - このテストファイルは、codanna のプラグイン管理（pluginsモジュール）の機能保証のためのみ利用され、他のモジュールから直接呼び出されることはない。

## API Surface (Public/Exported) and Data Contracts

注: このファイル自体に公開APIはありません。ここでは「このファイルから呼び出す外部公開API」を対象に記載します。シグネチャはテストの呼び出しからの推定であり、厳密な定義はこのチャンクには現れません。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| plugins::add_plugin | 不明（呼び出しは &Settings, &str, &str, Option<?>, bool, bool） | marketplace定義に基づきプラグインをインストール | 不明（*推測*: O(F)） | 不明（*推測*: O(F)） |
| plugins::update_plugin | 不明（呼び出しは &Settings, &str, Option<?>, bool, bool） | 既存プラグインを外部ソースの最新状態へ更新 | 不明（*推測*: O(F+Δ)） | 不明 |

Fはコピー対象ファイル数。推測と明記。

### plugins::add_plugin

1) 目的と責務
- marketplaceリポジトリURLとプラグイン名から、プラグイン定義を解決し、プラグインのコマンド等をワークスペースの `.claude/commands/<plugin>/...` へ配置する。
- pluginRootメタデータや source object（git）に対応。

2) アルゴリズム（推測、テストからのステップ）
- marketplaceリポジトリから `.claude-plugin/marketplace.json` を読む。
- 対象プラグインのエントリを見つけ、sourceを解決。
  - `source: "./<dir>"` の場合、pluginRoot を前置してプラグインディレクトリを解決。
  - `source: {source: "git", url: "<file://...>"}`
    の場合、外部リポジトリを取得（クローン）してプラグイン内容を使用。
- プラグインの `commands` は、プラグインの manifest（plugin.json）または marketplace の commands（strict:falseの場合）から得てコピー。
- ロックファイルを更新（内容はこのチャンクには現れない）。

3) 引数（推測）

| 引数名 | 型 | 役割 |
|-------|----|------|
| settings | &Settings | ワークスペースルートやデバッグ設定 |
| repo_url | &str | marketplaceリポジトリURL（file:// またはローカルパスの文字列） |
| plugin_name | &str | インストール対象プラグイン名 |
| rev_or_tag | Option<?> | バージョン/リビジョン指定（不明） |
| flag1 | bool | 動作フラグ（不明） |
| flag2 | bool | 動作フラグ（不明） |

4) 戻り値（推測）

| 型 | 意味 |
|----|------|
| Result<..., ...> | 成功/失敗の結果。テストでは .expect("install succeeds") を使用。 |

5) 使用例

```rust
plugins::add_plugin(
    &settings,
    &repo_url,
    "codanna-cc-plugin",
    None,
    false,
    false,
).expect("install succeeds");
```

6) エッジケース（テストから読み取れる期待）
- pluginRoot が指定されている場合にサブディレクトリを正しく解決。
- プラグイン側に plugin.json が無いが strict:false の場合、marketplaceの commands が優先。
- git source object の URL が file:// 形式で与えられた場合に取得可能。

### plugins::update_plugin

1) 目的と責務
- 既存インストール済みプラグインについて、外部ソースの最新状態に更新。
- 更新確認のため、ロックファイル（.codanna/plugins/lockfile.json）の **updated_at** を用いて「変更なし」を検出する（テストの期待）。

2) アルゴリズム（推測）
- 現在のインストール状態（コミット）と外部ソース状態を比較。
- 差分がなければ更新スキップし、updated_at は不変。
- 差分があれば取得・コピー・ロックファイル更新。

3) 引数（推測）

| 引数名 | 型 | 役割 |
|-------|----|------|
| settings | &Settings | ワークスペース設定 |
| plugin_name | &str | 対象プラグイン名 |
| rev_or_tag | Option<?> | バージョン/リビジョン指定（不明） |
| flag1 | bool | 動作フラグ（不明） |
| flag2 | bool | 動作フラグ（不明） |

4) 戻り値（推測）

| 型 | 意味 |
|----|------|
| Result<..., ...> | 成功/失敗 |

5) 使用例

```rust
plugins::update_plugin(&settings, "external-plugin", None, false, false)
    .expect("update succeeds");
```

6) エッジケース
- 外部ソースコミット一致時は更新不要で updated_at 不変。

## Walkthrough & Data Flow

各テストの入出力フローを要約します（行番号はこのチャンクでは不明）。

- install_uses_marketplace_plugin_root
  1. with_temp_workspace で一時ワークスペース作成。
  2. load_workspace_settings で Settings 初期化（workspace_root 設定、debug=true）。
  3. create_marketplace_with_plugin_root で marketplace リポジトリを作成:
     - `.claude-plugin/marketplace.json` に `"metadata": {"pluginRoot": "./plugins"}` を記述。
     - `plugins/codanna-cc-plugin/.claude-plugin/plugin.json` と `commands/ask.md` を作成。
     - init_git_repo で初回コミット。
  4. plugins::add_plugin を呼び出し。
  5. 成功後、`.claude/commands/codanna-cc-plugin/ask.md` が存在することを assert。

- install_supports_git_source_object
  1. with_temp_workspace → load_workspace_settings。
  2. marketplace_repo と `.claude-plugin` を作成。
  3. create_external_plugin_repo で外部プラグインリポジトリ構築。
  4. path_to_file_url で外部リポジトリパスを file:// URL に変換。
  5. marketplace_json を生成し、プラグインの `source` を `{source:"git", url:"file://..."}` として記述。
  6. marketplace_repo を init_git_repo。
  7. plugins::add_plugin 実行。
  8. `.claude/commands/external-plugin/external.md` の存在を assert。

- update_external_plugin_detects_up_to_date
  1. with_temp_workspace → load_workspace_settings。
  2. marketplace_repo を作成し、外部プラグインの git source を記述→ init_git_repo。
  3. plugins::add_plugin でインストール。
  4. `.codanna/plugins/lockfile.json` を読み取り、`updated_at` を before に保持。
  5. plugins::update_plugin を実行。
  6. ロックファイルを再読み込みして `updated_at` を after に保持。
  7. before == after を assert（更新不要時、タイムスタンプ不変）。

- install_allows_strict_false_without_plugin_manifest
  1. with_temp_workspace → load_workspace_settings。
  2. marketplace_repo を作り、`plugins/loose-plugin/commands/loose.md` のみ作成（プラグイン側 plugin.json は無し）。
  3. marketplace のプラグイン定義に `strict:false` と `commands` を記述。
  4. init_git_repo。
  5. plugins::add_plugin 実行。
  6. `.claude/commands/loose-plugin/loose.md` の存在を assert。

補助関数のデータフロー:
- path_to_file_url は OS 依存のパス表記を `/` に統一し、先頭スラッシュ有無で `file://` と `file:///` を切り替えます。

```rust
fn path_to_file_url(path: &Path) -> String {
    let path_str = path.display().to_string().replace('\\', "/");

    if path_str.starts_with('/') {
        format!("file://{path_str}")
    } else {
        format!("file:///{path_str}")
    }
}
```

## Complexity & Performance

- 補助関数
  - with_temp_workspace: O(1) 時間／空間（TempDirの作成）
  - load_workspace_settings: O(1)〜O(n設定ファイルサイズ)（I/O主体）
  - write_file: O(n内容サイズ)（I/O）
  - init_git_repo: O(F)（ステージング対象ファイル数Fに線形、コミット作成）
  - path_to_file_url: O(L)（パス文字列長）

- 外部API（推測）
  - add_plugin: O(F)（コピー対象ファイル数、クローンの場合はネットワーク/ディスクI/Oが支配的）
  - update_plugin: O(F+Δ)（差分確認のためのメタデータ比較＋必要な更新）

ボトルネック:
- Git操作（libgit2）およびファイルコピーのI/O。
- テストではローカル file:// URL と小規模リポジトリのため、実行時間は短い。

スケール限界（推測）:
- 大規模プラグインの大量ファイルコピー。
- ネットワーク越しのGitソース（このファイルでは file:// のみ）。

実運用負荷要因:
- ネットワークレイテンシ・クローンサイズ。
- ロックファイルの更新頻度と競合（このチャンクでは不明）。

## Edge Cases, Bugs, and Security

セキュリティチェックリストに基づく評価:
- メモリ安全性: unsafe未使用。所有権・借用はシンプルで、Use-after-free/Buffer overflow の懸念なし。
- インジェクション: このファイル内でSQL/Command/Path traversal の外部入力はなし。file:// URL 生成は安全な文字列操作のみ。
- 認証・認可: 対象外（テスト用ローカルI/O、外部APIに委譲）。
- 秘密情報: ハードコードされた秘密はなし。「Test/test@example.com」はダミー。
- 並行性: 各テストは独立した TempDir を用いるためワークスペース衝突は原則なし。ただし plugins モジュールがグローバル状態を持つなら未知の競合リスクあり。

Rust特有の観点:
- 所有権: &Path 参照を渡す設計でムーブは限定的。String を返す箇所は所有権移動のみ（例: create_marketplace_with_plugin_root の戻り値）。
- 借用: 可変借用は限定的（Settings の可変フィールド設定）。借用期間は関数スコープ内に閉じる。
- ライフタイム: 明示的ライフタイムは不要。
- unsafe境界: なし。
- 並行性・非同期: 非同期コード・Send/Sync の検討は不要（このチャンクでは同期I/Oのみ）。
- エラー設計: 多用される `.expect(...)` により失敗時は即時 panic（テストとしては妥当）。本番コードでは Result の伝播が望ましい。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| file:// URLのスラッシュ数 | Windowsパス "C:\repo" | "file:///C:/repo" | path_to_file_url | 対応済み |
| 先頭が「/」のパス | "/home/user/repo" | "file:///home/user/repo" ではなく "file:///..."に相当（実装は "file://"+パス） | path_to_file_url | 実装仕様通り（コメントに3スラッシュ要件） |
| plugin.jsonが存在しない | strict:false で marketplaceの commands 指定 | インストール成功しコマンドコピー | install_allows_strict_false_without_plugin_manifest | 対応済み |
| 外部プラグインの更新不要 | 同一コミット | updateで updated_at 不変 | update_external_plugin_detects_up_to_date | 対応済み |
| marketplaceの pluginRoot | "./plugins" 指定 | サブディレクトリ解決・コピー成功 | install_uses_marketplace_plugin_root | 対応済み |
| ロックファイル欠如 | lockfile.json が無い | テスト内では生成済み前提 | このチャンクには現れない | 不明 |

潜在的バグ:
- path_to_file_url の Unix ルートパス処理はコメント上は「3スラッシュ」と明記されていますが、実装は `file://{path_str}`（2スラッシュ+先頭スラッシュ）となり、結果的に "file:///..." 相当の文字列になります。仕様上は期待通りですが、特殊なパス（UNC, ネットワークドライブ）については未検証。
- テストは .expect に依存しており、失敗原因の粒度が粗い（どのステップで失敗かの追跡が難しい）。

## Design & Architecture Suggestions

- path_to_file_url の責務分離
  - OSごとのパスルート（Windowsドライブレター、UNC、Unix ルート）へ対応した **専用ユーティリティ** としてテストを追加する。
- テストユーティリティの再利用化
  - write_file, init_git_repo, load_workspace_settings 等を `tests/common.rs` のようなモジュールに集約し、重複を削減。
- 明確なデータ契約のドキュメント化
  - `.claude/commands/<plugin>/...` と `.codanna/plugins/lockfile.json` の仕様をドキュメント化し、plugins モジュールとの整合性を検証しやすくする。
- フラグ引数の型安全化
  - add_plugin / update_plugin の末尾2つの bool は意味が不明なため、オプション構造体にするなど **型で意味を伝える** 設計へ。

## Testing Strategy (Unit/Integration) with Examples

現状の統合テストは主要シナリオを網羅しています。追加を推奨するテスト:

- Windows/Unixでの file:// URL 生成の厳密テスト
```rust
#[test]
fn file_url_windows_drive_letter() {
    // 仮想的に "C:\repo" を Path にした場合の期待
    let p = PathBuf::from("C:\\repo");
    assert_eq!(path_to_file_url(&p), "file:///C:/repo");
}

#[test]
fn file_url_unix_root() {
    let p = PathBuf::from("/home/user/repo");
    assert_eq!(path_to_file_url(&p), "file:///home/user/repo"); // 実装仕様の再確認
}
```

- update_plugin の更新ありシナリオ
  - 外部プラグインリポジトリに新規コミット（ファイル追加）を行った後に update を実行し、updated_at が変化することを検証。
```rust
#[test]
fn update_external_plugin_applies_changes() {
    with_temp_workspace(|workspace| {
        let settings = load_workspace_settings(workspace);
        // セットアップ（省略）
        /* ... 省略 ... */

        // 変更を外部プラグインにコミット
        write_file(&plugin_repo.join("commands/new.md"), "# New");
        init_git_repo(&plugin_repo); // 新規コミット
        plugins::update_plugin(&settings, "external-plugin", None, false, false)
            .expect("update succeeds");

        let lockfile_path = workspace.join(".codanna/plugins/lockfile.json");
        let lf: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&lockfile_path).unwrap()).unwrap();

        assert!(workspace.join(".claude/commands/external-plugin/new.md").exists());
        // updated_at の検証（厳密な比較は環境依存、存在チェック等）
        assert!(lf["plugins"]["external-plugin"]["updated_at"].is_string());
    });
}
```

- strict:true の挙動
  - プラグイン側の plugin.json が必要なケースの検証（このチャンクには現れないため期待は不明）。

## Refactoring Plan & Best Practices

- 重複コード整理
  - marketplace JSON の生成をヘルパー化し、パラメータ化（name, plugin entries, metadata）。
  - `create_external_plugin_repo` と `create_marketplace_with_plugin_root` の共通部分（plugin.json/commands/コミット）を抽出。

- エラーの可観測性向上
  - `.expect("...")` のメッセージをもう少し具体化する（「read lockfile」→パス付与など）。
  - テスト失敗時に状態（作業ディレクトリ、生成ファイル一覧）を出力するユーティリティの導入。

- フラグの自己記述性
  - add_plugin / update_plugin の末尾 bool をビルダーパターンやオプション構造体に変更し、テスト側でフィールド名を明示。

## Observability (Logging, Metrics, Tracing)

- Settings.debug = true をテストで有効化しているため、plugins モジュールがログを出していれば診断に有益。標準出力/エラーログの収集をテストヘルパで行うと、失敗時の調査が容易。
- ロックファイルの更新時刻やコミットハッシュなど、更新判定の根拠をログに含めると良い（このチャンクには現れない）。

## Risks & Unknowns

- 不明点
  - plugins::add_plugin / update_plugin の厳密なシグネチャ・戻り値・挙動。
  - ロックファイルのスキーマ（plugins.external-plugin.updated_at 以外のフィールド）。
  - strict:true の厳密な意味付けとエラーハンドリング。
  - ネットワーク越しGit（file:// 以外）のサポート状況。

- リスク
  - OS間差異による file:// URL 生成の不整合（UNCパス等）は未検証。
  - テスト並列実行時、環境依存のグローバル状態があると衝突する可能性（このチャンクには現れない）。
  - libgit2 の挙動に依存するため、環境差によるテスト不安定性（ファイルロック、パーミッション等）。

以上の通り、このファイルはプラグイン解決・インストールと更新判定の主要パスを、ローカルGit環境と file:// URL を用いて再現・検証するための実用的な統合テストです。個々のユーティリティは簡潔かつ安全に実装されており、さらなる堅牢性・可観測性・再利用性の向上が可能です。