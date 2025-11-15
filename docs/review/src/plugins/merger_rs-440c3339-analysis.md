## plugins\merger.rs Review

## TL;DR

- 目的: プロジェクトの`.mcp.json`にプラグイン提供の**MCPサーバー設定**を統合し、競合検出/削除/読み込みを行うユーティリティを提供
- 公開API: **merge_mcp_servers**, **check_mcp_conflicts**, **remove_mcp_servers**, **load_plugin_mcp_servers**, 構造体**McpMergeOutcome**
- 複雑箇所: 競合判定と**force**フラグの挙動、事前検査ヘルパー**allowed_keys**との不整合
- 重大リスク:
  - 競合検査では許可されるキーでも、実際のマージでは許可されない（API間の整合性欠如）
  - ファイル書き込みが非原子的でロックなし：**レースコンディション**/更新ロスの危険
  - `McpServerSpec::Path`での**絶対パス**利用により、プラグインディレクトリ外のファイル読み込み（パス逸脱）の可能性
  - **added_keys**が実際に「追加」されたかに関わらずプラグイン側キー一覧を返す命名の不一致
- Rust安全性/エラー/並行性: unsafeなし、所有権/借用は適切。I/OとJSON変換の**?**利用によりエラー伝播設計が要件（このチャンクには現れない）。並行性は未対処。

## Overview & Purpose

このファイルはプラグインインストール時に、プラグイン提供のMCPサーバー設定をプロジェクトの`.mcp.json`へ安全に統合するためのコアロジックを提供します。主な機能は以下の通りです。

- **merge_mcp_servers**: プラグインのサーバー設定を`.mcp.json`へ統合。競合があればエラーまたは上書き（force）し、必要ならファイルを更新。
- **check_mcp_conflicts**: ファイルを変更せずに**競合を検出**。一部キーは**allowed_keys**で例外扱い可能。
- **remove_mcp_servers**: 指定キーを`.mcp.json`の`mcpServers`から削除して保存。
- **load_plugin_mcp_servers**: プラグイン内のMCPサーバー仕様（**パス**または**インライン**）から`mcpServers`オブジェクトを取得。
- **McpMergeOutcome**: マージ結果（プラグイン側キー一覧、以前のファイル内容、存在有無）を表す。

用途は、プラグイン管理（インストール/アンインストール/検証）フローにおける**MCP設定の統合**と整合性維持です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | McpMergeOutcome | pub | マージ結果のメタ情報（追加キー、以前の内容、ファイル存在） | Low |
| Function | merge_mcp_servers | pub | プロジェクト`.mcp.json`へプラグインサーバー設定の統合 | Med |
| Function | check_mcp_conflicts | pub | プロジェクト`.mcp.json`とプラグイン設定の競合検査（非破壊） | Low |
| Function | remove_mcp_servers | pub | `.mcp.json`から指定キーの削除 | Low |
| Function | load_plugin_mcp_servers | pub | プラグイン仕様から`mcpServers`オブジェクト取得 | Low |

### Dependencies & Interactions

- 内部依存
  - 公開関数間の直接呼び出しは「なし」。各関数は**独立**して動作。
  - `McpMergeOutcome`は`merge_mcp_servers`の戻り値として利用。
- 外部依存（本チャンクに現れるもの）
  - super::error::{**PluginError**, **PluginResult**}（詳細はこのチャンクには現れない）
  - serde_json::{**Value**, **json**, from_str, to_string_pretty}
  - std::fs::{read_to_string, write}
  - std::path::**Path**
  - std::collections::**HashSet**
  - crate::plugins::plugin::**McpServerSpec**（詳細はこのチャンクには現れない）
  - testsのみ: **tempfile::tempdir**
- 被依存推定
  - プラグイン管理/インストールロジック、CLIやUIの設定適用処理から呼び出される可能性あり（このチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| merge_mcp_servers | fn merge_mcp_servers(project_mcp_path: &Path, plugin_servers: &Value, force: bool) -> PluginResult<McpMergeOutcome> | プラグインのMCPサーバー設定を`.mcp.json`へ統合 | O(n + k) | O(n) |
| check_mcp_conflicts | fn check_mcp_conflicts(project_mcp_path: &Path, plugin_servers: &Value, force: bool, allowed_keys: &HashSet<String>) -> PluginResult<()> | 競合の事前検査（非破壊） | O(n + k) | O(n) |
| remove_mcp_servers | fn remove_mcp_servers(project_mcp_path: &Path, server_keys: &[String]) -> PluginResult<()> | 指定キー削除 | O(n + r) | O(n) |
| load_plugin_mcp_servers | fn load_plugin_mcp_servers(plugin_dir: &Path, mcp_spec: &crate::plugins::plugin::McpServerSpec) -> PluginResult<Value> | プラグイン仕様から`mcpServers`を抽出 | O(n) | O(n) |
| Data Contract | struct McpMergeOutcome { added_keys: Vec<String>, previous_content: Option<String>, file_existed: bool } | マージ結果のメタ情報 | - | - |

n=対象JSONファイルの文字列長、k=プラグイン側のサーバーキー数、r=削除対象キー数。

### merge_mcp_servers

1) 目的と責務
- `.mcp.json`を読み込み、`mcpServers`を**オブジェクトとして**確保し、プラグインのサーバー設定をキー単位で統合。
- 競合時は**force**がfalseならエラー（PluginError::McpServerConflict）、trueなら上書き。
- 変更があったときのみファイルを書き戻す。
- マージ対象キー一覧（added_keys）と変更前内容（previous_content）を返す。

2) アルゴリズム（主なステップ）
- project_mcp_pathの存在確認（行番号:不明）
- JSON文字列を読み込み→Valueへパース。存在しない場合は`{"mcpServers": {}}`の初期値（行番号:不明）
- ルートがオブジェクトか検査し、`mcpServers`エントリをオブジェクトとして確保（行番号:不明）
- plugin_serversがオブジェクトならキーごとに:
  - 既存値と完全一致ならスキップ
  - 不一致かつ**force=false**ならMcpServerConflict
  - それ以外は設定を挿入（上書き可）、変更フラグを立てる
- 変更ありならpretty JSONで書き戻し
- McpMergeOutcomeを返す（added_keysはプラグイン側キー一覧）

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| project_mcp_path | &Path | プロジェクトの`.mcp.json`へのパス |
| plugin_servers | &Value | プラグインが提供する`mcpServers`オブジェクト（JSON） |
| force | bool | 競合時に上書きするか |

4) 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<McpMergeOutcome> | 成功時はマージ結果、失敗時はPluginError |

5) 使用例
```rust
use serde_json::json;
use std::path::Path;

let path = Path::new("/path/to/.mcp.json");
let plugin_servers = json!({
    "my-server": { "command": "npx", "args": ["my-server"] }
});
let outcome = merge_mcp_servers(path, &plugin_servers, /*force=*/ false)?;
assert!(outcome.file_existed || outcome.previous_content.is_none());
```

6) エッジケース
- plugin_serversがオブジェクトでない場合は何もしない（変更なし）
- `.mcp.json`が不正構造（ルートがオブジェクトでない/`mcpServers`がオブジェクトでない）→InvalidPluginManifest
- 同値（完全一致）なら**変更なし**で保存も行わない
- 競合時に**force=false**→McpServerConflict

### check_mcp_conflicts

1) 目的と責務
- `.mcp.json`を変更せずに、プラグインのキーが既存と競合するかを検査。
- **allowed_keys**に含まれるキーは、force=falseでも例外として許容。

2) アルゴリズム
- `.mcp.json`が存在しなければOK
- 読み込み→JSON→`mcpServers`をオブジェクトとして取得
- plugin_serversキーごとに、既存に含まれ、かつ`force=false`かつ`allowed_keys`に含まれないなら競合エラー

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| project_mcp_path | &Path | `.mcp.json`パス |
| plugin_servers | &Value | プラグインの`mcpServers` |
| force | bool | 競合許容フラグ |
| allowed_keys | &HashSet<String> | 競合を許容するキー集合 |

4) 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<()> | 成功時はOk(()); 失敗時はPluginError |

5) 使用例
```rust
use serde_json::json;
use std::collections::HashSet;

let allowed = HashSet::from(["codanna".to_string()]);
let plugin_servers = json!({ "codanna": { "command": "replacement" } });
check_mcp_conflicts(Path::new("./.mcp.json"), &plugin_servers, false, &allowed)?;
```

6) エッジケース
- `.mcp.json`が不正構造→InvalidPluginManifest
- plugin_serversがオブジェクトでない→何も検査せずOK

⚠ 重要: このAPIで許容されたキーでも、実際のマージ**merge_mcp_servers**では**allowed_keys**の概念がないためエラーとなる可能性あり（設計不整合）。

### remove_mcp_servers

1) 目的と責務
- `.mcp.json`の`mcpServers`から指定キーを削除し、ファイルを保存。

2) アルゴリズム
- ファイルが存在しなければ何もしない
- 読み込み→JSON→ルート/`mcpServers`がオブジェクトの場合のみ削除
- 常に保存（削除がなくても保存する実装）

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| project_mcp_path | &Path | `.mcp.json` |
| server_keys | &[String] | 削除するサーバーキー一覧 |

4) 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<()> | 成功/失敗 |

5) 使用例
```rust
let keys = vec!["obsolete".to_string(), "legacy".to_string()];
remove_mcp_servers(Path::new("./.mcp.json"), &keys)?;
```

6) エッジケース
- ルートや`mcpServers`がオブジェクトでない→削除処理はスキップだがエラーにならない
- 指定キーが存在しない→変更なしだが保存は実施

### load_plugin_mcp_servers

1) 目的と責務
- プラグイン仕様（**McpServerSpec**）に応じて`mcpServers`オブジェクトを取得。

2) アルゴリズム
- `McpServerSpec::Path(path)`:
  - `plugin_dir.join(path)`でファイルパス作成→読み込み→JSON
  - ルートに`mcpServers`があればそれを返し、なければ空オブジェクト`{}`を返す
- `McpServerSpec::Inline(value)`:
  - 値をそのまま返す

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| plugin_dir | &Path | プラグイン基準ディレクトリ |
| mcp_spec | &McpServerSpec | パスまたはインライン定義 |

4) 戻り値

| 型 | 説明 |
|----|------|
| PluginResult<Value> | `mcpServers`オブジェクト（なければ空オブジェクト） |

5) 使用例
```rust
use crate::plugins::plugin::McpServerSpec;
use serde_json::json;

let value = json!({ "mcpServers": { "s": { "command": "x" } } });
let servers = load_plugin_mcp_servers(Path::new("./plugin"), &McpServerSpec::Inline(value))?;
assert!(servers.is_object());
```

6) エッジケース
- パス指定でJSONが不正→serde_json::Error伝播
- 絶対パス指定による`join`の挙動でプラグインディレクトリ外を参照しうる（セキュリティ注意）

### Data Contract: McpMergeOutcome

- フィールド
  - **added_keys**: プラグイン側が提供したサーバーキー一覧（実際に追加/変更されたかに依らず）
  - **previous_content**: 既存`.mcp.json`の文字列（存在しない場合None）
  - **file_existed**: `.mcp.json`の存在有無
- 注意
  - 名前と意味の不一致: "added_keys"は実際の「追加」ではなく「提供キー一覧」。改善余地あり。

## Walkthrough & Data Flow

- merge_mcp_serversの主要データフロー
  - 入力: `project_mcp_path`（ファイルパス）, `plugin_servers`（JSON）, `force`
  - 出力: `McpMergeOutcome`
  - 処理:
    - 既存ファイル有無チェック→読み込み（Optional）
    - JSON Value化→ルートオブジェクトの検査→`mcpServers`の確保
    - プラグインキー走査→一致ならスキップ、不一致なら`force`に従い挿入/エラー
    - 変更あれば**pretty**で保存
    - 結果を返却

```mermaid
flowchart TD
  A[Start] --> B{.mcp.json exists?}
  B -- No --> C[project_mcp = {"mcpServers": {}}]
  B -- Yes --> D[read_to_string + from_str(Value)]
  D --> E{root is object?}
  E -- No --> X[Err(InvalidPluginManifest)]
  E -- Yes --> F[ensure "mcpServers" object]
  F --> G{plugin_servers is object?}
  G -- No --> O[changed=false; write? no]
  G -- Yes --> H[for each (key,value)]
  H --> I{servers_obj has key?}
  I -- No --> J[insert; changed=true]
  I -- Yes --> K{existing == value?}
  K -- Yes --> L[continue]
  K -- No --> M{force?}
  M -- No --> Y[Err(McpServerConflict)]
  M -- Yes --> J
  L --> N{loop end}
  J --> N
  O --> P{changed?}
  N --> P
  P -- Yes --> Q[to_string_pretty + write]
  P -- No --> R[no write]
  Q --> S[Return Outcome]
  R --> S
```

上記の図は`merge_mcp_servers`関数（行番号:不明）の主要分岐を示す。

## Complexity & Performance

- merge_mcp_servers
  - 時間: O(n + k)（ファイル読み書き＋JSONパース＋キーループ）
  - 空間: O(n)（JSON Value保持）
  - ボトルネック: ファイルI/Oとシリアライズ。大規模`mcpServers`では比較・挿入が増える。
- check_mcp_conflicts
  - 時間: O(n + k)
  - 空間: O(n)
  - ボトルネック: I/O＋パース。allowed_keysはHashSetでO(1)照会。
- remove_mcp_servers
  - 時間: O(n + r)
  - 空間: O(n)
  - ボトルネック: 不必要な再保存（削除なしでも保存）によりI/O増加。
- load_plugin_mcp_servers
  - 時間: O(n)
  - 空間: O(n)
  - ボトルネック: パス指定時のI/O。`mcpServers`抽出は軽微。

実運用負荷要因:
- 頻繁なプラグインインストール/更新で**同時アクセス**があるとレース/更新ロスが発生しやすい。
- ネットワーク/DBは使用していないためI/Oはローカルファイルのみ。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| `.mcp.json`不存在 | パス未作成 | 新規生成して統合 | 初期値`{"mcpServers": {}}`で対応 | OK |
| ルートが非オブジェクト | `[]`や`"str"` | エラー | InvalidPluginManifest | OK |
| `mcpServers`が非オブジェクト | `"mcpServers": []` | エラー（merge/check） | merge/checkはInvalidPluginManifest、removeは無視 | 要仕様確認 |
| 完全一致の設定 | 既存と同一 | 変更なし/保存なし | 比較してスキップ | OK |
| 競合（force=false） | 既存と不一致 | エラー | McpServerConflict | OK |
| 競合（force=true） | 既存と不一致 | 上書き保存 | insertで上書き | OK |
| plugin_serversが非オブジェクト | 数値等 | 無視 or エラー | 無視（変更なし） | 要仕様確認 |
| 削除対象キーなし | 空配列 | 保存不要 | でも保存する | 要改善（無駄I/O） |
| 絶対パスのmcp_spec | `/etc/passwd`等 | プラグイン内に限定 | joinの挙動で外部参照の危険 | 要修正 |
| added_keysの語義 | 同値で変更なし | 変更されたキーのみ | プラグイン提供キー全てを返す | 要修正（命名/意味） |

セキュリティチェックリスト:
- メモリ安全性: 低リスク。バッファ操作なし、unsafeなし。所有権/借用は**&Path**や**&Value**で安全に扱い、必要箇所でcloneを使用。
- インジェクション:
  - SQL/Command/Path Traversal: **Path Traversalの可能性**あり（絶対パスを`join`で許容しうる）。Commandは値として保持するだけで実行なし。
- 認証・認可: 該当なし（このチャンクには現れない）
- 秘密情報:
  - **previous_content**を返すため、呼び出し側がログ出力すると機密漏洩の可能性。関数自体はログしない。
- 並行性:
  - **Race condition**: 読み込み→編集→書き込みの三段階でロックなし。並行操作で上書きロス/整合性崩壊の危険。
  - **Atomicity**: `std::fs::write`は原子的ではなく、途中失敗時に破損可能。テンポラリ＋`rename`の原子的置換が望ましい。
- Rust特有の観点:
  - 所有権: `servers_obj.insert(key.clone(), value.clone())`で所有権移動なし、cloneにより安全（行番号:不明）
  - 借用/ライフタイム: 明示的ライフタイム不要、短期借用のみ
  - unsafe境界: なし
  - Send/Sync: グローバル共有状態なし。I/Oは同期ブロッキング。
  - 非同期/await: 使用なし
  - エラー設計: `?`で`std::fs`/`serde_json`エラーを伝播。`PluginError`への変換実装はこのチャンクには現れない。panicは使用なし。

## Design & Architecture Suggestions

- API整合性
  - **allowed_keys**の概念を`merge_mcp_servers`にも導入し、`check_mcp_conflicts`との挙動を統一する。例: `merge_mcp_servers(..., force, allowed_keys: Option<&HashSet<String>>)`.
- 書き込みの原子性とロック
  - テンポラリファイルに書き出し後、**rename**による原子的置換を行う。
  - プロセス内ロック（例: `parking_lot::Mutex`）やファイルロック（例: `fs2::FileExt::lock_exclusive`）で**排他制御**を導入。
- パス安全性
  - `McpServerSpec::Path`で**絶対パス拒否**、`..`コンポーネントの正規化/拒否を行い、プラグインディレクトリ外参照を禁止。
- 追加キーの語義
  - **added_keys**を「追加/変更されたキー」に変更するか、フィールド名を**provided_keys**へリネームして意味を明確化。
- エラー詳細化
  - InvalidPluginManifestの**reason**に文脈（どのフィールドが不正か）を含める。
- 無駄な保存抑制
  - `remove_mcp_servers`で変更がない場合は保存をスキップ。
- バリデーション
  - plugin_serversが非オブジェクトの場合は**エラー**にするか、警告を返す仕組みを追加。

## Testing Strategy (Unit/Integration) with Examples

- 既存テストは基本パス/競合/許容キー/同値保存なしをカバー。以下を追加すると良い。

1) allowed_keysとmergeの整合性（要変更後）
```rust
#[test]
fn test_merge_respects_allowed_keys() -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashSet;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(".mcp.json");
    std::fs::write(&path, r#"{ "mcpServers": { "codanna": { "command": "old" } } }"#)?;

    let plugin_servers = serde_json::json!({ "codanna": { "command": "new" } });
    let allowed = HashSet::from(["codanna".to_string()]);
    // 仮: merge_mcp_serversにallowed_keysを導入したとする
    let outcome = merge_mcp_servers_with_allowed(&path, &plugin_servers, false, Some(&allowed))?;
    assert!(outcome.added_keys.contains(&"codanna".to_string()));
    Ok(())
}
```

2) 絶対パスの拒否
```rust
#[test]
fn test_load_plugin_mcp_servers_rejects_absolute_path() {
    use crate::plugins::plugin::McpServerSpec;
    let dir = tempfile::tempdir().unwrap();
    let spec = McpServerSpec::Path(std::path::PathBuf::from("/etc/passwd"));
    let res = load_plugin_mcp_servers_safe(dir.path(), &spec); // 安全版
    assert!(matches!(res, Err(PluginError::InvalidPluginManifest { .. })));
}
```

3) removeの無変更保存抑制
```rust
#[test]
fn test_remove_noop_does_not_write() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(".mcp.json");
    let initial = r#"{ "mcpServers": { "a": { "command": "x" } } }"#;
    std::fs::write(&path, initial)?;
    let metadata_before = std::fs::metadata(&path)?.modified()?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    remove_mcp_servers_no_write_on_noop(&path, &[])?; // 改善版
    let metadata_after = std::fs::metadata(&path)?.modified()?;
    assert_eq!(metadata_before, metadata_after);
    Ok(())
}
```

4) 破損JSONのエラー伝播
```rust
#[test]
fn test_invalid_json_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".mcp.json");
    std::fs::write(&path, "{ invalid json }").unwrap();
    let plugin_servers = serde_json::json!({});
    let res = merge_mcp_servers(&path, &plugin_servers, true);
    assert!(res.is_err());
}
```

5) 並行アクセステスト（擬似）
- 2スレッドで同時マージを試み、**ロック導入後**に整合性維持を確認（このチャンクには行わないが方針示す）

## Refactoring Plan & Best Practices

- ステップ1: `merge_mcp_servers`に`allowed_keys: Option<&HashSet<String>>`を追加し、`check_mcp_conflicts`と整合。
- ステップ2: ファイルI/Oをヘルパー化（read_json, write_json_atomic）。`write_json_atomic`は`NamedTempFile`→`persist`/`rename`。
- ステップ3: パス検証ユーティリティ（reject absolute, normalize, ensure inside plugin_dir）。
- ステップ4: `remove_mcp_servers`で変更検知後のみ保存するよう修正。
- ステップ5: `McpMergeOutcome`のフィールド命名/意味整合（added vs provided）。
- ステップ6: エラー理由の詳細化と単体テスト拡充。

ベストプラクティス:
- **原子的書き込み**、**排他制御**、**入力バリデーション**の徹底
- JSONスキーマ検証（可能なら）で`mcpServers`構造保証
- ログに機密を出さない（previous_contentの取り扱い注意）

## Observability (Logging, Metrics, Tracing)

- ログ方針
  - レベル: info（開始/完了）、warn（競合/非標準構造）、error（I/O/JSONエラー）
  - フィールド: path, keys, changed, conflict_keys, force, allowed_keys_size
- 例（tracing想定）
```rust
use tracing::{info, warn, error};

info!(path=?project_mcp_path, "MCP merge start");
if !project_mcp_path.exists() {
    info!(path=?project_mcp_path, "MCP file not found; will create");
}
warn!(conflict_key=%key, force=%force, "MCP server conflict");
error!(error=?e, path=?project_mcp_path, "Failed to read .mcp.json");
info!(changed=%changed, keys=?owned_keys, "MCP merge completed");
```
- メトリクス
  - 成功/失敗回数、競合検出数、書き込みの有無、処理時間
- トレーシング
  - インストール/アンインストールフローのスパンに紐づけ、`.mcp.json`操作を子スパンで記録

## Risks & Unknowns

- `PluginError`/`PluginResult`の具体的な実装/エラー変換（このチャンクには現れない）
- `McpServerSpec`の詳細仕様（絶対パスの許容/拒否、正規化ルール）は不明
- `.mcp.json`の正式スキーマは不明（`mcpServers`内の許容フィールド/型など）
- 並行アクセス要件（複数プロセスが同時に操作する想定か）は不明
- 「added_keys」の期待仕様は不明（現在はプラグイン提供キー一覧を返すが、命名と乖離）

以上の点から、**API整合性**と**ファイル操作の安全性**を優先して改善することを推奨します。