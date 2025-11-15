# provider_registry.rs Review

## TL;DR

- 目的は、ホームディレクトリ配下の JSON（~/.codanna/providers.json）でグローバルなプロバイダレジストリを管理すること（登録、取得、検索、保存/読み込み）。
- 主要公開APIは、ProviderRegistry::{new,load,save,add_provider,remove_provider,get_provider,find_provider_for_profile,find_provider_with_id,list_all_profiles} と ProviderSource::{parse,from_*}、RegisteredProvider::{git_url,is_local,local_path}。
- コアロジックは JSON シリアライズ/デシリアライズ、プロファイル集合の検索、GitHub/URL/ローカルのソース判別（parse）に集約。
- 重大リスクは current_timestamp の暦計算が不正確である点、parse のヒューリスティック誤判定（相対パスや Windows パス、ssh スキームなど）、ファイル保存の非アトミック性と並行アクセス時の破損可能性。
- エラー設計は ProfileResult に委譲しているが、panic（SystemTime::duration_since の expect）混在があり改善余地。
- 並行性はない（同期 I/O のみ）。プロセス間競合対策やロック・アトミック書き込みは未実装。
- 互換性（version フィールド）の移行処理が現状不在で、将来のレジストリフォーマット変更時に問題化する可能性。

## Overview & Purpose

このファイルは、アプリケーション全体で共有されるプロバイダレジストリの**データモデル**と**操作関数**を提供します。目的は以下の通りです。

- JSON（~/.codanna/providers.json）にプロバイダ情報を保存/読み込みする。
- ProviderManifest からレジストリへ登録（プロファイル情報を抽出しキャッシュ）。
- プロバイダやプロファイル名からの検索、一覧取得を可能にする。
- プロバイダのソース（GitHub/URL/Local）を文字列から判別する。

このモジュールは、他の機能（プロバイダのフェッチ/更新、プロファイルの実行など）のための**基盤となるメタデータ管理**層です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | ProviderRegistry | pub | レジストリ全体（version, providers）を保持し、保存/読み込み・CRUD・検索を提供 | Med |
| Struct | RegisteredProvider | pub | 単一プロバイダ（name, source, namespace, profiles, last_updated）の情報を保持 | Low |
| Enum | ProviderSource | pub | プロバイダのソース（Github/Url/Local）を表現し、文字列からのパースを提供 | Med |
| Struct | ProfileInfo | pub | プロファイルのキャッシュ情報（version, description） | Low |
| Func | current_timestamp | private | ISO風タイムスタンプ生成（簡易） | Low |
| Tests | mod tests | private | ユニットテスト群（保存/読み込み、追加/削除、検索、パース、URL生成） | Med |

### Dependencies & Interactions

- 内部依存
  - ProviderRegistry::add_provider → current_timestamp を使用（last_updated 設定）。
  - ProviderRegistry::* → serde_json によるシリアライズ/デシリアライズ。
  - RegisteredProvider::git_url → ProviderSource に依存（Github なら URL 形成）。
  - ProviderSource::parse → 文字列判定ロジック。

- 外部依存（推奨表）

| クレート/モジュール | 用途 |
|--------------------|------|
| serde, serde_json  | JSONシリアライズ/デシリアライズ |
| std::collections::HashMap | プロバイダ/プロファイルのマップ管理 |
| std::fs, std::path::Path | ファイルI/O、パス管理 |
| std::time::SystemTime | タイムスタンプ生成 |
| super::error::ProfileResult | エラー型（詳細はこのチャンクには現れない） |
| super::provider::ProviderManifest 他 | マニフェスト構造（詳細はこのチャンクには現れない） |

- 被依存推定
  - CLI やサービス層で、プロバイダ一覧表示、プロファイル検索、レジストリ更新（add/remove/save/load）を行うモジュール。
  - プロファイル実行やダウンロード機能が RegisteredProvider の source を参照する可能性。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ProviderRegistry::new | fn new() -> Self | 空のレジストリ生成 | O(1) | O(1) |
| ProviderRegistry::load | fn load(path: &Path) -> ProfileResult<Self> | JSONからロード。無ければ新規 | O(|file|) | O(#providers + #profiles) |
| ProviderRegistry::save | fn save(&self, path: &Path) -> ProfileResult<()> | レジストリをJSON保存 | O(#providers + #profiles) | O(#providers + #profiles) |
| ProviderRegistry::add_provider | fn add_provider(&mut self, provider_id: String, manifest: &ProviderManifest, source: ProviderSource) | マニフェストをレジストリへ登録 | O(#manifest.profiles) | O(#manifest.profiles) |
| ProviderRegistry::remove_provider | fn remove_provider(&mut self, provider_id: &str) -> bool | 指定IDの削除 | O(1)平均 | O(1) |
| ProviderRegistry::get_provider | fn get_provider(&self, provider_id: &str) -> Option<&RegisteredProvider> | IDで取得 | O(1)平均 | O(1) |
| ProviderRegistry::find_provider_for_profile | fn find_provider_for_profile(&self, profile_name: &str) -> Option<&RegisteredProvider> | プロファイルを含むプロバイダ検索 | O(#providers) | O(1) |
| ProviderRegistry::find_provider_with_id | fn find_provider_with_id(&self, profile_name: &str) -> Option<(&str, &RegisteredProvider)> | 上記に加えIDも返す | O(#providers) | O(1) |
| ProviderRegistry::list_all_profiles | fn list_all_profiles(&self) -> Vec<(String, String, &ProfileInfo)> | 全プロファイル列挙 | O(Σ #profiles) | O(Σ #profiles) |
| RegisteredProvider::git_url | fn git_url(&self) -> Option<String> | GitHub/URL から Git 取得URL生成 | O(1) | O(1) |
| RegisteredProvider::is_local | fn is_local(&self) -> bool | ローカル判定 | O(1) | O(1) |
| RegisteredProvider::local_path | fn local_path(&self) -> Option<&str> | ローカルパス取得 | O(1) | O(1) |
| ProviderSource::from_github_shorthand | fn from_github_shorthand(repo: &str) -> Self | owner/repo から Github 構築 | O(1) | O(1) |
| ProviderSource::from_git_url | fn from_git_url(url: &str) -> Self | Git URL から Url 構築 | O(1) | O(1) |
| ProviderSource::from_local_path | fn from_local_path(path: &str) -> Self | ローカルパスから Local 構築 | O(1) | O(1) |
| ProviderSource::parse | fn parse(source: &str) -> Self | 文字列を Github/Url/Local に識別 | O(1) | O(1) |

データ契約（JSONフォーマット、Serialize/Deserialize）
- ProviderRegistry: { "version": u32, "providers": { provider_id: RegisteredProvider } }
- RegisteredProvider: { "name": String, "source": ProviderSource, "namespace"?: String, "profiles": { profile_name: ProfileInfo }, "last_updated"?: String }
- ProviderSource: タグ付き列挙（serde(tag="type", rename_all="lowercase")）
  - {"type":"github","repo":"owner/repo"}
  - {"type":"url","url":"https://..."} または {"type":"url","url":"git@..."}
  - {"type":"local","path":"./path"}
- ProfileInfo: { "version": String, "description"?: String }

例（JSON）
```json
{
  "version": 1,
  "providers": {
    "test-provider": {
      "name": "claude",
      "source": { "type": "github", "repo": "codanna/claude-provider" },
      "namespace": ".claude",
      "profiles": {
        "codanna": { "version": "1.0.0", "description": "Test profile" }
      },
      "last_updated": "2024-09-01T12:34:56Z"
    }
  }
}
```

以下、各APIの詳細。

### ProviderRegistry::new

1. 目的と責務
   - 空のレジストリ（version=1、providers=空）を生成。

2. アルゴリズム
   - 構造体初期化のみ。

3. 引数
| 名前 | 型 | 説明 |
|------|----|------|
| なし | - | - |

4. 戻り値
| 型 | 説明 |
|----|------|
| ProviderRegistry | 初期化済み |

5. 使用例
```rust
let registry = ProviderRegistry::new();
```

6. エッジケース
- 特になし。

### ProviderRegistry::load

1. 目的と責務
   - 指定パスから JSON を読み込み、レジストリを返す。ファイルが無ければ新規を返す。

2. アルゴリズム
   - Path.exists() → 無い場合 Self::new() を返す。
   - read_to_string → serde_json::from_str でデコード。

3. 引数
| 名前 | 型 | 説明 |
|------|----|------|
| path | &Path | レジストリファイルパス |

4. 戻り値
| 型 | 説明 |
|----|------|
| ProfileResult<ProviderRegistry> | 成功時レジストリ、失敗時エラー |

5. 使用例
```rust
let reg = ProviderRegistry::load(Path::new("/tmp/providers.json"))?;
```

6. エッジケース
- ファイル破損/無効な JSON → デコード失敗（エラー伝播）。
- version 不一致時の移行処理は未実装（このチャンクには現れない）。

### ProviderRegistry::save

1. 目的と責務
   - レジストリを JSON としてファイルへ保存。

2. アルゴリズム
   - 親ディレクトリ create_dir_all。
   - serde_json::to_string_pretty → std::fs::write。

3. 引数
| 名前 | 型 | 説明 |
|------|----|------|
| path | &Path | 保存先ファイルパス |

4. 戻り値
| 型 | 説明 |
|----|------|
| ProfileResult<()> | 成功時 (), 失敗時エラー |

5. 使用例
```rust
registry.save(Path::new("/tmp/providers.json"))?;
```

6. エッジケース
- 並行書き込み・クラッシュ時の無原子書き込みリスク。
- パーミッション不足でエラー。

### ProviderRegistry::add_provider

1. 目的と責務
   - ProviderManifest から RegisteredProvider を構築し、指定 ID で登録。

2. アルゴリズム
   - manifest.profiles を map して ProfileInfo キャッシュ化。
   - namespace は manifest.metadata.namespace を Option チェーンで取得。
   - last_updated を current_timestamp() で設定。
   - HashMap::insert により provider_id で格納（既存があれば上書き）。

3. 引数
| 名前 | 型 | 説明 |
|------|----|------|
| provider_id | String | レジストリキー |
| manifest | &ProviderManifest | プロバイダのマニフェスト |
| source | ProviderSource | プロバイダソース |

4. 戻り値
| 型 | 説明 |
|----|------|
| なし | 上書きの有無は返さない |

5. 使用例
```rust
registry.add_provider(
    "test-provider".to_string(),
    &manifest,
    ProviderSource::from_github_shorthand("org/repo"),
);
```

6. エッジケース
- provider_id 重複→サイレント上書き。
- manifest.profile の version が None → "unknown" を格納（曖昧）。

### ProviderRegistry::remove_provider

1. 目的と責務
   - 指定 ID を削除。

2. アルゴリズム
   - HashMap::remove の結果から bool を返す。

3. 引数
| 名前 | 型 | 説明 |
|------|----|------|
| provider_id | &str | レジストリキー |

4. 戻り値
| 型 | 説明 |
|----|------|
| bool | 削除成功なら true |

5. 使用例
```rust
let ok = registry.remove_provider("test-provider");
```

6. エッジケース
- 存在しない ID → false。

### ProviderRegistry::get_provider

1. 目的と責務
   - ID から参照取得。

2. アルゴリズム
   - HashMap::get。

3. 引数/戻り値
| 引数 | 型 | 説明 |
|------|----|------|
| provider_id | &str | レジストリキー |

| 戻り値 | 型 | 説明 |
|--------|----|------|
| Option<&RegisteredProvider> | 存在すれば参照 |

5. 使用例
```rust
if let Some(p) = registry.get_provider("test") { /* ... */ }
```

6. エッジケース
- なし。

### ProviderRegistry::find_provider_for_profile

1. 目的と責務
   - プロファイル名を含むプロバイダを検索。

2. アルゴリズム
   - providers.values().find(|p| p.profiles.contains_key(profile_name))

3. 引数/戻り値
| 引数 | 型 |
|------|----|
| profile_name | &str |

| 戻り値 | 型 |
|--------|----|
| Option<&RegisteredProvider> | |

5. 使用例
```rust
let p = registry.find_provider_for_profile("codanna");
```

6. エッジケース
- 複数プロバイダが同名プロファイルを持つ場合、最初に見つかったもののみ返す。

### ProviderRegistry::find_provider_with_id

1. 目的と責務
   - 上記に加えプロバイダIDも返す。

2. アルゴリズム
   - providers.iter().find(...).map(|(id,p)| (id.as_str(), p))

3. 引数/戻り値
| 引数 | 型 |
|------|----|
| profile_name | &str |

| 戻り値 | 型 |
|--------|----|
| Option<(&str, &RegisteredProvider)> | |

5. 使用例
```rust
if let Some((id, p)) = registry.find_provider_with_id("codanna") { /* ... */ }
```

6. エッジケース
- 同上（曖昧性）。

### ProviderRegistry::list_all_profiles

1. 目的と責務
   - 全プロファイルの一覧（provider_id, profile_name, &ProfileInfo）を返す。

2. アルゴリズム
   - providers を走査し profiles をフラットに push。

3. 引数/戻り値
| 引数 | 型 |
|------|----|
| なし | - |

| 戻り値 | 型 |
|--------|----|
| Vec<(String, String, &ProfileInfo)> | クローンした ID/名前と参照 |

5. 使用例
```rust
for (pid, pname, pinfo) in registry.list_all_profiles() {
    println!("{pid}/{pname}: {}", pinfo.version);
}
```

6. エッジケース
- ベクタが大きくなる（総プロファイル数に比例）。

### RegisteredProvider::git_url

1. 目的と責務
   - ProviderSource から git URL を得る（Githubは https://github.com/{repo}.git）。

2. アルゴリズム
   - match source: Github→format、Url→クローン、Local→None。

3. 引数/戻り値
| 引数 | 型 |
|------|----|
| &self | RegisteredProvider |

| 戻り値 | 型 |
|--------|----|
| Option<String> | |

5. 使用例
```rust
let url = provider.git_url();
```

6. エッジケース
- Url には非 Git スキームでもそのまま返すため、後段でエラー化する可能性。

### RegisteredProvider::is_local

- 責務: Local 判定。
- 使用例:
```rust
if provider.is_local() { /* ... */ }
```

### RegisteredProvider::local_path

- 責務: Local の path を返す。
- 使用例:
```rust
if let Some(path) = provider.local_path() { /* ... */ }
```

### ProviderSource::{from_github_shorthand, from_git_url, from_local_path}

- いずれも文字列を受け取り各バリアントを構築。
- 使用例:
```rust
let g = ProviderSource::from_github_shorthand("org/repo");
let u = ProviderSource::from_git_url("https://example.com/repo.git");
let l = ProviderSource::from_local_path("./repo");
```

### ProviderSource::parse

1. 目的と責務
   - 文字列から Github/Url/Local をヒューリスティックに判別。

2. アルゴリズム（簡略ステップ）
   - 先頭が "http://", "https://", "git@" → Url
   - それ以外で "/" を含み、先頭が "." でも "/" でもない → Github shorthand
   - 上記以外 → Local

3. 引数/戻り値
| 引数 | 型 |
|------|----|
| source | &str |

| 戻り値 | 型 |
|--------|----|
| ProviderSource | |

5. 使用例
```rust
let s = ProviderSource::parse("codanna/claude-provider"); // Github
```

6. エッジケース
- "ssh://host/repo" → Url と認識されない（現状 Local）
- "src/module"（相対パス）→ Github と誤認（先頭が '.' ではないため）
- Windows パス "C:\\repo" → "/" が無いので Local と誤認/意図不明
- 先頭が "/" の絶対パスは Local として認識されるが、"~/repo" は Github と誤認の可能性

## Walkthrough & Data Flow

- レジストリ初期化
  - new() で空構造を用意。
- 読み込み
  - load(path): ファイル存在チェック → 読み込み → JSON デコード → ProviderRegistry 生成。
- 追加
  - add_provider(id, manifest, source): マニフェストから ProfileInfo を生成し、RegisteredProvider を作って providers に挿入。last_updated を current_timestamp に設定。
- 検索
  - get_provider(id): 直接参照。
  - find_provider_for_profile(name): HashMap の values を走査し、profiles.contains_key(name)で一致検索。
  - find_provider_with_id(name): iter() で (id, provider) を走査し同様に一致検索。
- 一覧
  - list_all_profiles(): 各プロバイダの profiles を平坦化。
- 保存
  - save(path): JSON 文字列化（pretty）→ 親ディレクトリ作成 → 書き込み。

このフローで**永続化**と**検索**の双方を提供します。I/O 以外はすべてメモリ上の HashMap 操作です。

## Complexity & Performance

- new: O(1)
- load: O(|file|) 読み込み＋デコード。メモリは O(#providers + #profiles)。
- save: O(#providers + #profiles) のシリアライズと書き込み。
- add_provider: O(#manifest.profiles) の map 処理。
- remove/get: 平均 O(1)（HashMap）。
- find_provider_*: O(#providers)（各プロバイダの profiles.contains_key は O(1)）。
- list_all_profiles: O(Σ #profiles)。

ボトルネック/スケール限界
- providers/profiles が増えると save/load の JSON（単一ファイル）肥大化による I/O 負荷・メモリ使用増大。
- find_provider_* は線形走査なので、プロバイダ数が多いと遅延が増加（必要なら索引構造を検討）。
- ファイル保存は非アトミックで競合時に破損リスク。

実運用負荷要因
- ディスク I/O（読み込み/書き込み）。
- JSON パース/生成コスト（pretty で余分な文字挿入）。

## Edge Cases, Bugs, and Security

セキュリティチェックリストに沿って評価します。

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 直接的な不安全コードは無し（unsafe 不使用、行番号:不明）。HashMap/文字列操作は安全な Rust 標準APIのみ。
  - current_timestamp の暦計算は簡易であり、整数演算におけるオーバーフローは現実的に発生しないが、暦の正確性が欠如（後述）。

- インジェクション
  - SQL/Command/Path traversal: このモジュールはコマンド実行や DB アクセスをしない。Path は save/load のみ。ユーザ入力に基づくパスであっても create_dir_all/write はそのパスへ書くため、任意ファイル上書きのリスク（権限内）あり。適切な保存先制限が望ましい（このチャンクには現れない）。

- 認証・認可
  - なし（レジストリ操作はローカルファイル）。

- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: ログ機構なし。例外詳細は ProfileResult 経由で返る前提（このチャンクには現れない）。

- 並行性
  - Race condition / Deadlock: ファイルアクセス時のプロセス間競合に対するロックなし。save が非アトミックで、中断/同時書き込みで破損可能性。

- panic
  - current_timestamp が SystemTime::duration_since(UNIX_EPOCH).expect("Time went backwards") により panic の可能性あり。稀だが本番コードとしてはエラー伝播が望ましい。

詳細なエッジケース表

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ファイル不存在 | path="/tmp/none.json" | 新規レジストリ返却 | load | OK |
| ファイル破損JSON | path が壊れたJSON | エラー返却 | load | OK（エラー伝播） |
| ディレクトリ作成失敗 | 権限不足 | エラー返却 | save | OK（エラー伝播） |
| 非アトミック書き込み | 同時書き込み | 破損防止 | save | NG（ロック/アトミック無し） |
| provider_id 重複 | "test" を再追加 | 方針に沿った上書き/拒否 | add_provider | 上書き（通知なし） |
| プロファイル version 欠如 | version=None | Optionで表現か既定値 | add_provider | "unknown"（曖昧） |
| プロファイル名重複（複数プロバイダ） | profile="codanna" が複数に存在 | 明示的選択かエラー | find_provider_* | 最初の一致のみ（曖昧） |
| parseの誤判定（相対パス） | "src/module" | Local扱い | parse | Github扱い（誤判定） |
| parseのスキーム非対応 | "ssh://host/repo" | Url扱い | parse | Local扱い（誤判定） |
| Windowsパス | "C:\\repo" | Local扱い | parse | "/"判定のみで不正確 |
| タイムスタンプ暦不正確 | 任意 | 正しいISO8601 | current_timestamp | 簡易計算（不正確） |
| panic回避 | SystemTime逆行 | エラー返却 | current_timestamp | panicの可能性 |

## Design & Architecture Suggestions

- バージョン管理と移行
  - ProviderRegistry.version を読み込み時に検査し、旧フォーマットからのマイグレーションロジックを導入。
- 保存の堅牢化
  - アトミック書き込み（テンポラリファイル → rename）とファイルロック（advisory）で同時書き込み/破損を防止。
- 解析ロジックの改善
  - ProviderSource::parse は URL スキームの包括的判定（http(s), ssh, file, git+ssh）を導入。可能なら url クレートや正規表現で厳密化。
  - 相対パス判定を強化（"." 始まり以外でも、パス区切りや存在確認を考慮）。
  - Windows パス（バックスラッシュ）の取り扱い。
- エラー設計の調整
  - current_timestamp の panic をやめ、Result<String, Error> を返すか、fail-safe の値を返す。
  - add_provider の上書き可否を返す（Option<RegisteredProvider> の previous を返却）ことで呼び出し側が把握可能。
- データモデリング
  - ProfileInfo.version を Option<String> にし、「unknown」固定文字列を避ける。
  - find_provider_* の曖昧性解消のため、プロファイル識別に provider_id と profile_name のペアを前提にする API を追加。
- インデックス化
  - プロファイル名→provider_id の補助マップを維持して検索を O(1) に。
- 設定の保存先制約
  - save はホームディレクトリ配下のみ等の制約を設け、任意パス上書きを回避（このチャンクにはポリシーが現れない）。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本機能をカバー（new, save/load, add/remove, find_provider_for_profile, parse, git_url）。追加を推奨：

- エラー系
  - 破損 JSON 読み込みの失敗確認。
  - 権限不足で save が失敗するケース（Unixのディレクトリに書き込み不可の場所で）。
- 競合/アトミック
  - 複数スレッド/プロセスで同時 save を模擬（可能なら integration）。
- parse 拡張
  - "ssh://", "file://", "git+ssh://"、Windows パス "C:\\repo"、相対パス "src/module" の判定。
- 上書き挙動
  - 同一 provider_id を add_provider した際、以前のエントリが消えることの検証と周知。

例: 破損 JSON 読み込み
```rust
use std::fs;
use tempfile::tempdir;

#[test]
fn test_load_invalid_json() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("providers.json");
    fs::write(&path, "{ invalid json").unwrap();
    let res = ProviderRegistry::load(&path);
    assert!(res.is_err());
}
```

例: parse の追加ケース
```rust
#[test]
fn test_parse_ssh_url() {
    let source = ProviderSource::parse("ssh://example.com/repo.git");
    // 現仕様では Local になる可能性。将来は Url と認識されるべき。
    match source {
        ProviderSource::Url { .. } => {},
        _ => panic!("Should detect ssh:// as Url"),
    }
}
```

例: 上書き挙動
```rust
#[test]
fn test_add_provider_overwrite() {
    let mut reg = ProviderRegistry::new();
    let manifest = /* ... 省略 ... */;
    reg.add_provider("id".to_string(), &manifest, ProviderSource::from_local_path("./a"));
    reg.add_provider("id".to_string(), &manifest, ProviderSource::from_local_path("./b"));
    let p = reg.get_provider("id").unwrap();
    assert_eq!(p.local_path(), Some("./b"));
}
```

## Refactoring Plan & Best Practices

- タイムスタンプ
  - chrono/time クレートを使用し、UTC の ISO 8601（RFC3339）正確な形式を生成。panic を除去。
- パース改善
  - URL スキームの網羅、OS 依存パス区切り対応、正規表現ベースの厳密判定。
- API 戻り値の充実
  - add_provider: Option<RegisteredProvider>（上書き前の値）を返す。
  - find_provider_*: 複数一致時の制御（優先度ルールやエラー）。
- 保存の安全化
  - 一時ファイル + fs::rename によるアトミック保存。必要なら file-lock（flock/WindowsのCreateFile）導入。
- データ構造
  - 二次インデックス（profile_name → provider_id）。
  - ProfileInfo.version を Option に。
- バージョン互換性
  - load 時に version 検査とマイグレーション。
- ドキュメント
  - JSON スキーマの明文化（例: serdeの tag フォーマット）。

## Observability (Logging, Metrics, Tracing)

- ログ
  - load/save の開始/成功/失敗ログ。provider 追加/削除ログ（ID/件数）。
- メトリクス
  - プロバイダ数、プロファイル総数、保存/読み込みレイテンシ。
- トレーシング
  - 重要 I/O 操作に span を付与。呼び出し側で trace-context を渡す API を検討（このチャンクには非同期/トレーシングは現れない）。
- エラー詳細
  - ProfileResult にエラー分類（I/O, Serde, Validation）を持たせ、ログと連携。

## Risks & Unknowns

- 現仕様の parse ヒューリスティックは誤判定リスクが高い（相対パス、Windows、ssh スキーム）。
- current_timestamp の暦計算は近似であり、**日付が誤る**可能性が高い。ログや監査に誤情報が残る。
- ファイル競合時の破損（非アトミック保存、ロックなし）。
- バージョン移行の不在により、将来の JSON フォーマット変更時に互換性崩壊の可能性。
- ProfileResult の実装詳細が不明（このチャンクには現れない）。エラー分類/メッセージ一貫性は外部に依存。
- 複数プロバイダが同名プロファイルを持つ場合の曖昧性（検索 API は最初の一致のみ返す）。

以上を踏まえ、公開APIとコアロジックは簡潔で扱いやすい設計だが、I/O の堅牢化、解析の厳密化、エラー/時間処理の改善を行うことで実運用に耐える品質へと強化できます。