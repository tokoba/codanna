## source_resolver.rs Review

## TL;DR

- 目的: ProviderSource（Local/Github/Url）からプロファイルディレクトリへの解決を行い、必要に応じてGitリポジトリを一時クローンする。
- 公開API: ResolvedProfileSource（enum）、resolve_profile_source、ResolvedProfileSource::profile_dir、ResolvedProfileSource::commit。
- 複雑箇所: Gitクローン処理と一時ディレクトリ管理（TempDirのライフタイム）、構造チェックは「存在確認」のみで内容検証なし。
- 重大リスク: シンボリックリンクによるパス外への誘導、ネットワーク/認証失敗、URL不正、TempDir破棄による参照消失、ログ/観測情報不足。
- Rust安全性: unsafe未使用、所有権・借用は安全。TempDirのDropタイミングに依存するため、利用側でライフタイムに注意。
- パフォーマンス: ローカルはO(1) I/O、Gitはリポジトリサイズに比例してO(n)。キャッシュ/再利用がないため、頻繁な解決に不利。
- テスト: ローカルの成功/失敗、コミット取得なし、パス構築をカバー。ネットワーク必須テストはignore。

## Overview & Purpose

このモジュールは、プロバイダソース（ローカルパス、GitHub、任意Git URL）からプロファイルディレクトリを解決するためのコアロジックを提供する。ローカルソースの場合はディレクトリ存在確認、リモートソースの場合は一時ディレクトリにクローンを行い、その中に期待される構造「.codanna-profile/profiles/<profile_name>」が存在するかを検証する。クローンした場合は、コミットSHAを保持し、利用者が出力の再現性や追跡に活用できる。

このチャンクでは、ProviderSourceの全定義は現れないが、Local/Github/Urlの3バリアントを使用していることが確認できる（resolve_profile_source内のmatch）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | ResolvedProfileSource | pub | ローカル/一時Gitクローンのソース表現と派生情報（コミット、ディレクトリパスの生成） | Low |
| Fn | resolve_profile_source | pub | ProviderSourceを解決し、ResolvedProfileSourceを返すエントリポイント | Med |
| Method | ResolvedProfileSource::profile_dir | pub | プロファイル名からディレクトリパスを構築 | Low |
| Method | ResolvedProfileSource::commit | pub | Gitソース時のコミットSHA取得 | Low |
| Fn | resolve_local_source | private | ローカルプロバイダソースの存在検証とResolvedProfileSource生成 | Low |
| Fn | resolve_git_source | private | Gitクローンとプロファイルディレクトリ存在検証、ResolvedProfileSource生成 | Med |
| Fn | clone_repository | private | super::git::clone_repositoryへ委譲する薄いラッパー | Low |
| Mod | tests | private | ローカルパス/パス構築/コミットなしのテスト、ネットワーク依存テスト（ignore） | Low |

### Dependencies & Interactions

- 内部依存
  - resolve_profile_source → resolve_local_source / resolve_git_source
  - resolve_git_source → clone_repository → super::git::clone_repository
  - ResolvedProfileSource::profile_dir → TempDir::path（Gitの場合）
- 外部依存（このチャンクに現れるもの）
  - super::error::{ProfileError, ProfileResult}（エラー型とResultエイリアス）
  - super::provider_registry::ProviderSource（Local/Github/Urlバリアント）
  - super::git::clone_repository（Gitクローン実装への委譲）
  - std::path::{Path, PathBuf}
  - tempfile::{TempDir, tempdir}
  - std::fs（tests内のみ）
- 被依存推定（このモジュールを使用しそうな箇所）
  - プロファイルローダ/実行計画生成モジュール（profile.jsonの読み込み）
  - CLIやサーバーサイドのプロファイル選択/適用ロジック
  - キャッシュ管理やダウンロード管理の上位層

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| ResolvedProfileSource（enum） | pub enum ResolvedProfileSource { Local { path: PathBuf }, Git { temp_dir: TempDir, commit: String } } | プロファイルソースの表現（ローカル or Gitクローン） | N/A | N/A |
| resolve_profile_source | pub fn resolve_profile_source(provider_source: &ProviderSource, profile_name: &str) -> ProfileResult<ResolvedProfileSource> | ProviderSourceをResolvedProfileSourceに解決 | ローカル: O(1) I/O / Git: O(n)（n=リポジトリサイズ+ネットワーク） | ローカル: O(1) / Git: O(n)（クローン先ディスク） |
| ResolvedProfileSource::profile_dir | pub fn profile_dir(&self, profile_name: &str) -> PathBuf | プロファイル名のディレクトリパスを返す | O(k)（パス結合） | O(k)（PathBuf） |
| ResolvedProfileSource::commit | pub fn commit(&self) -> Option<&str> | Gitソース時のコミットSHAをOptionで返す | O(1) | O(1) |

### ResolvedProfileSource（Data Contract）

- Local { path: PathBuf }
  - 意味: 「.codanna-profile/profiles」へのベースパス
  - profile_dir(profile_name) は path.join(profile_name)
- Git { temp_dir: TempDir, commit: String }
  - 意味: 一時ディレクトリにクローン済み。commitはクローン時のHEADや指定リファレンスのSHA。
  - profile_dir(profile_name) は temp_dir.path().join(".codanna-profile/profiles").join(profile_name)
  - TempDirはDrop時にディレクトリが削除されるため、利用側はライフタイムに注意。

重要な主張の根拠（行番号）はこのチャンクでは不明（関数定義は当該ファイル内）。

---

### resolve_profile_source

1. 目的と責務
   - ProviderSourceを解決し、ローカルまたはGitクローン済みのResolvedProfileSourceを返す。
   - ローカル時は対象プロファイルの存在を検証、Git時はクローン後に存在を検証。

2. アルゴリズム（ステップ分解）
   - match provider_source:
     - Local { path } → resolve_local_source(path, profile_name)
     - Github { repo } → "https://github.com/{repo}.git" を生成 → resolve_git_source(url, profile_name)
     - Url { url } → resolve_git_source(url, profile_name)
   - 返り値はProfileResult<ResolvedProfileSource>

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| provider_source | &ProviderSource | はい | プロバイダソース（Local/Github/Url） |
| profile_name | &str | はい | 対象プロファイル名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<ResolvedProfileSource> | 成功時ResolvedProfileSource、失敗時ProfileError |

5. 使用例
```rust
use profiles::source_resolver::{resolve_profile_source, ResolvedProfileSource};
use profiles::provider_registry::ProviderSource;

let source = ProviderSource::Local { path: "/opt/providers/my-provider".into() };
let resolved = resolve_profile_source(&source, "my-profile")?;
let dir = resolved.profile_dir("my-profile");
// dir == "/opt/providers/my-provider/.codanna-profile/profiles/my-profile"
```

6. エッジケース
- ProviderSource::Github で存在しないrepoを指定
- ProviderSource::Url に不正なURL/認証が必要なURL
- profile_name に空文字や存在しないディレクトリ

---

### ResolvedProfileSource::profile_dir

1. 目的と責務
   - ResolvedProfileSourceからプロファイルディレクトリのフルパスを構築。

2. アルゴリズム
   - match self:
     - Local { path } → path.join(profile_name)
     - Git { temp_dir } → temp_dir.path().join(".codanna-profile").join("profiles").join(profile_name)

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| profile_name | &str | はい | プロファイル名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| PathBuf | プロファイルディレクトリのパス |

5. 使用例
```rust
let dir = resolved.profile_dir("my-profile");
assert!(dir.ends_with("my-profile"));
```

6. エッジケース
- Local で path が存在しない/削除済み
- Git で TempDir がDropされ、返されたPathBufが無効になる利用側バグ

---

### ResolvedProfileSource::commit

1. 目的と責務
   - Gitソースの場合のみコミットSHAを返す。ローカルはNone。

2. アルゴリズム
   - match self:
     - Git { commit } → Some(&commit)
     - Local → None

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| なし | なし | - | - |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<&str> | Gitの場合Some(SHA)、ローカルはNone |

5. 使用例
```rust
if let Some(sha) = resolved.commit() {
    println!("Resolved from commit {}", sha);
} else {
    println!("Local provider source");
}
```

6. エッジケース
- Gitクローン失敗でResolvedProfileSourceが生成されないためNone/Someの整合性は保たれる

---

## Walkthrough & Data Flow

- ローカルソース（resolve_local_source）
  - 入力: path: &str, profile_name: &str
  - base_path = Path::new(path)
  - profiles_path = base_path.join(".codanna-profile").join("profiles")
  - profile_path = profiles_path.join(profile_name)
  - profile_path.exists() を確認し、存在しなければ ProfileError::ProfileNotFoundInProvider を返す
  - 成功なら ResolvedProfileSource::Local { path: profiles_path } を返す

- Gitソース（resolve_git_source）
  - 入力: url: &str, profile_name: &str
  - temp_dir = tempfile::tempdir() で一時ディレクトリ生成
  - commit = clone_repository(url, temp_dir.path(), None)? （super::git に委譲）
  - temp_dir.path().join(".codanna-profile").join("profiles").join(profile_name).exists() を確認
  - 存在しなければ ProfileError::ProfileNotFoundInProvider
  - 成功なら ResolvedProfileSource::Git { temp_dir, commit } を返す
  - 注意: TempDirはResolvedProfileSourceがDropされるとディレクトリが削除される

- resolve_profile_source
  - ProviderSourceをmatchし、Local/Github/Urlをそれぞれ resolve_local_source / resolve_git_source に振り分ける
  - Github は "https://github.com/{repo}.git" を生成してGitフローへ

このチャンクは分岐が3つで状態遷移も少ないため、Mermaid図の使用基準を満たさない（条件分岐4つ以上ではない）。

## Complexity & Performance

- resolve_profile_source
  - ローカル: 時間 O(1)（パス結合と存在確認のI/O）、空間 O(1)
  - Git: 時間 O(n)（n=リポジトリサイズ＋ネットワーク遅延）、空間 O(n)（クローンしたファイル群）
  - ボトルネック: Gitクローン（ネットワーク、ストレージI/O）、構造検証が浅いため不必要なクローンを防ぐ仕組みがない
- ResolvedProfileSource::profile_dir: 時間 O(k)、空間 O(k)（k=パス長）
- スケール限界: 多数のGitソースを連続解決すると、帯域/ディスクの消費が急増。キャッシュや再利用がないためスループット低下。

実運用負荷要因:
- I/O: フォルダ存在確認は軽微だが、Gitは重い。
- ネットワーク: GitHub/外部Gitのレート制限/認証/接続エラー。
- ストレージ: TempDirに全リポジトリをクローン。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（標準APIとPath操作のみ、unsafe未使用）
  - TempDirのDropによるパス無効化には利用側の注意が必要（Use-after-free的な論理バグ誘発の恐れ）。このモジュールはPathBufを返すのみで、保持はしない。
- インジェクション
  - SQL/Command/Path traversal: コマンド実行は行わずgit2想定のAPI委譲のため、シェルインジェクションの可能性は低い。ただし「このチャンクにはsuper::gitの実装が現れない」ため最終安全性は不明。
  - Path traversal（シンボリックリンク）: ローカル/クローンしたリポジトリ内で profiles/<profile_name> がシンボリックリンクの場合、上位コードがresolve後にファイルを読み込む際に外部へのアクセスを誘導される恐れ。対策としてrealpath/canonicalizeでルート以下に収まることを検証すべき。
- 認証・認可
  - 権限チェック漏れ: 不明（このチャンクでは認証トークン等の取扱いはない）。
  - セッション固定: 該当なし。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: ログは未実装。失敗時の詳細なURL/パスがエラーに含まれうるが、現状ProfileErrorに含めるのはprofile/provider文字列のみ。
- 並行性
  - Race condition / Deadlock: 並行処理なし。TempDirはスレッド安全な操作だが、外部で同一TempDirを共有する設計は避けるべき。
  - Send/Sync: ResolvedProfileSource::GitがTempDirを保持するため、構造体のSend/SyncはTempDirの実装に依存（通常TempDirはSendだがSyncではない可能性あり）。このチャンクでは並行使用前提のコードはない。

Rust特有の観点（このチャンクの範囲）:
- 所有権: TempDirはResolvedProfileSource::Gitに所有され、Drop時にクリーンアップされる。PathBufは所有型で安全。
- 借用/ライフタイム: &str引数の借用は関数スコープ内のみ。commit()はOption<&str>を返すが、内部Stringへの借用でselfのライフタイムに束縛され安全。
- unsafe境界: unsafe未使用。
- エラー設計: ProfileResult/ProfileErrorで明示的に失敗を返す。unwrap/expectはtestsのみ。

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ローカル: providerパスなし | path="/no/such/dir" | Err(ProfileNotFound) or より適切な「ProviderNotFound」 | profile_path.exists()のみ | このチャンクでは不存在時にProfileNotFoundを返す設計 |
| ローカル: .codanna-profile/profilesが無い | path="/opt/providers/a" | Err(ProfileNotFoundInProvider) | 末端profile_path.exists()判定 | 明示チェックなしだが結果的にErr |
| ローカル: profile_nameが空 | profile_name="" | Err(InvalidProfileName) | 入力検証なし | 未対応 |
| ローカル: profiles/<name>がファイル | profiles/test-profileがファイル | Err or 無視 | exists()のみ | ディレクトリ種別未検証 |
| Git: URL不正 | url="not a url" | Err(gitエラー) | clone_repository委譲 | super::git次第（このチャンクでは不明） |
| Git: ネットワーク不可 | 任意URL | Err(gitエラー) | 委譲 | 不明 |
| Git: 認証必要 | プライベートrepo | Err or 認証処理 | 委譲 | このチャンクでは不明 |
| Git: profiles/<name>なし | 任意URL | Err(ProfileNotFoundInProvider) | 明示実装あり | 対応済み |
| Git: TempDir早期Drop | 利用側が所有権をDrop | その後のpath無効 | 設計上の注意 | 利用側のバグリスク |
| シンボリックリンク誘導 | profiles/<name>が外部リンク | canonicalizeで検知すべき | 未実装 | セキュリティリスク |

## Design & Architecture Suggestions

- 入力検証強化
  - profile_nameの空文字/不正文字を拒否。
  - ローカル: profiles/<name>がディレクトリであることを確認（is_dir）。
- Gitオプションの拡張
  - ProviderSourceにブランチ/タグ/コミット指定を追加し、resolve_git_sourceでgit_refを渡す（現状None固定）。
  - シャロークローン（depth=1）や必要ディレクトリのみのスパースチェックアウトでパフォーマンス改善。
- キャッシュ・再利用
  - 同一URL+refのクローンをローカルキャッシュして再利用。一時ディレクトリではなくキャッシュディレクトリを採用し、TempDir不要のオプションも提供。
- 路径安全性
  - canonicalizeで実体パスを確認し、プロバイダルート配下に収まっていることを検証。シンボリックリンクを辿らないオプションの導入。
- エラーの粒度
  - ProviderNotFound（ローカル基底ディレクトリがない）とProfileNotFoundを区別。
  - Gitクローン失敗時にURL、ref、詳細原因（DNS、認証、権限）を含むが、機密情報を含めない安全なメッセージを整備。
- API設計
  - resolve_local_source/resolve_git_sourceの公開範囲は必要に応じてpub(crate)など検討。
  - 引数にAsRef<Path>/AsRef<str>を採用して柔軟性を向上。
- ライフタイム管理
  - ResolvedProfileSource::GitのTempDirを外部利用がしやすい形に（明示的cleanupメソッド、永続クローンオプション）。

## Testing Strategy (Unit/Integration) with Examples

既存テスト（このチャンク）
- ローカル成功（ディレクトリ作成＋profile.json設置）
- ローカル失敗（プロファイルが存在しない）
- GitHub（ignore）: ネットワーク依存のためスキップ
- commit()がNone（ローカル）
- profile_dirのパス構築

追加推奨テスト
- ローカル: profiles/<name>がファイルの場合
- ローカル: base_pathが存在しない場合のエラー（区別）
- ローカル: profile_nameが空文字
- Git: 不正URL/到達不可/認証必要リポジトリでの失敗とメッセージ検証
- Git: 指定refのサポート（super::git側実装がある場合）
- セキュリティ: シンボリックリンクを用いた外部誘導を検出するcanonicalize検証（導入後）

テスト例（ローカル: ディレクトリ種別検証を追加した場合の例）
```rust
#[test]
fn test_local_profile_is_not_dir() {
    let temp = tempfile::tempdir().unwrap();
    let provider_dir = temp.path().join("my-provider");
    let profiles_dir = provider_dir.join(".codanna-profile/profiles");
    std::fs::create_dir_all(&profiles_dir).unwrap();

    // プロファイル名の場所にファイルを置く
    let file_path = profiles_dir.join("not-a-dir");
    std::fs::write(&file_path, "dummy").unwrap();

    let source = ProviderSource::Local {
        path: provider_dir.to_string_lossy().to_string(),
    };
    let result = resolve_profile_source(&source, "not-a-dir");
    assert!(result.is_err());
    // 実装後は、ProfileError::InvalidProfileDirなど期待
}
```

テスト例（Git: 不正URL）
```rust
#[test]
fn test_git_invalid_url() {
    let source = ProviderSource::Url { url: "not a url".into() };
    let result = resolve_profile_source(&source, "any");
    assert!(result.is_err());
    // super::gitのエラー型がProfileErrorに適切にラップされることを検証
}
```

## Refactoring Plan & Best Practices

- 署名の一般化: resolve_local_source/resolve_git_sourceの引数をAsRef<Path>/AsRef<str>に。
- ディレクトリ検証強化: exists()ではなくis_dir()。必要に応じて必須ファイル（profile.json）存在確認。
- エラー型の拡充: ProviderNotFound/InvalidProfileName/InvalidProfileDirなど。
- Git関連の拡張: git_refの受け取り、シャロークローン/スパースチェックアウト設定、タイムアウト。
- キャッシュ層導入: URL+refでキー化、クローン済みリポジトリ再利用。
- APIドキュメント/例: 仕様（.codanna-profile/profiles/<name>）の明文化。
- ロギング導入と構造化エラー（thiserror + anyhow）との整合。
- セキュリティ強化: canonicalizeベースのパス検証、シンボリックリンク対策。

## Observability (Logging, Metrics, Tracing)

- ログ
  - resolve_profile_source開始/終了、分岐（Local/Git）のログ
  - Gitクローン開始/終了、URL、ref（機密情報は出さない）
  - エラー時の原因・再試行可能性
- メトリクス
  - クローン所要時間、サイズ、成功率/失敗率
  - ローカル/リモート解決比率
- トレーシング
  - リクエストID/トレースIDに紐付けたスパン（clone、existsチェック）
  - コミットSHAと関連付けて後続処理の再現性を追跡

このチャンクには実装なし。導入にはlog/tracingクレートとメトリクス収集基盤（Prometheus等）の連携が必要。

## Risks & Unknowns

- super::git::clone_repositoryの詳細が不明
  - 認証/タイムアウト/リトライ/シャロークローン/スパースチェックアウト対応の有無はこのチャンクには現れない。
  - エラー型の詳細とProfileErrorへの変換方針も不明。
- ProviderSourceの完全定義が不明
  - ここではLocal/Github/Urlのみを使用しているが、拡張有無は不明。
- TempDirのSend/Sync性はTempDir実装に依存
  - 並行処理環境での扱いは、このチャンクでは考慮なし。
- プロファイル構造の仕様詳細が不明
  - 必須ファイル（profile.json等）の検証は未実装。必要要件は別モジュールで定義されている可能性。

以上により、Gitクローンの堅牢性とパス安全性を高める改修、キャッシュ/観測性/エラー粒度の改善が推奨される。