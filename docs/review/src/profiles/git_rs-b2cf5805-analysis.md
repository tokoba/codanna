# profiles\git.rs Review

## TL;DR

- 目的: libgit2 を用いて Git リポジトリからプロフィールを取得するための基本操作（クローン、参照解決、検証、コミットSHA取得）を提供する。
- 主要公開API: clone_repository, get_commit_sha, resolve_reference, validate_repository（いずれも ProfileResult を返す）。
- 複雑箇所: clone_repository の分岐（ローカル判定、浅いクローン、ブランチ/タグ/コミットの指定とチェックアウト、プロキシ・認証コールバック設定）。
- 重大リスク: checkout_reference がタグ参照の際に HEAD を適切に設定しない（タグ Object を Commit に peel していない）。また、エラーメッセージに repo_url を含めるため、URL に埋め込まれた資格情報漏えいの可能性。
- エラー設計: 多くは ProfileError::GitOperationFailed にラップされるが、validate_repository はエラーメッセージの文字列部分一致で ProviderNotFound を返すため脆弱。create_detached の ? による型変換は From 実装に依存。
- Rust安全性: unsafe 不使用。所有権・借用はシンプルで健全。並行性は使っていないが、libgit2 のコールバックは内部スレッドから呼ばれうる点に留意。
- パフォーマンス: ネットワーク/I/O主導。resolve_reference はリモート参照列挙 O(R)。clone はダウンロード対象のサイズに比例。

## Overview & Purpose

このファイルは、プロフィール取得のために Git リポジトリを操作するユーティリティ群を提供する。libgit2（git2 クレート）を用いて以下を実装する。

- リポジトリの浅いクローン（ブランチ/タグ/コミット指定対応）
- リポジトリの現在のコミット SHA を取得
- クローンなしでリモート参照（ブランチ/タグ/完全参照名）をコミット SHA に解決
- リポジトリ URL の妥当性検証（接続可能か）

いずれの関数も ProfileError/ProfileResult（super::error由来）を用いたエラー伝播を行う。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | clone_repository | pub | リポジトリを浅くクローンし指定参照にチェックアウト、コミットSHA返却 | High |
| Function | credential_callback | private | 認証コールバック（SSHエージェント、デフォルト、ユーザ/パス） | Med |
| Function | checkout_reference | private | 参照（ブランチ/タグ/コミット）をチェックアウトし HEAD 設定 | Med |
| Function | get_commit_sha | pub | リポジトリの HEAD コミット SHA を取得 | Low |
| Function | resolve_reference | pub | リモート参照を列挙して指定参照のコミット SHA を返す | Med |
| Function | validate_repository | pub | リモートへ接続できるか検証し、存在しない場合に ProviderNotFound を返す | Low |

### Dependencies & Interactions

- 内部依存
  - clone_repository → credential_callback（認証）/ checkout_reference（参照チェックアウト）/ get_commit_sha（SHA取得）
  - resolve_reference → credential_callback
  - validate_repository → credential_callback
- 外部依存（git2 クレート中心）

| 依存 | 用途 |
|------|------|
| git2::Repository | リポジトリ操作（open, checkout_head, revparse_single など） |
| git2::build::RepoBuilder | クローン構築（fetchオプション、branch指定） |
| git2::RemoteCallbacks | 認証や進捗のコールバック設定 |
| git2::FetchOptions | 浅いクローン設定、タグダウンロード、リモートコールバック |
| git2::ProxyOptions | プロキシ設定（auto） |
| git2::Cred, CredentialType | 資格情報管理（SSH, default, ユーザ/パス） |
| git2::Remote | 接続/参照列挙（ls-remote 相当） |

- 被依存推定
  - プロフィールのソースを Git から取得する上位モジュール（profiles サブシステムやリゾルバ）が、clone_repository/resolve_reference/validate_repository/get_commit_sha を使用する可能性が高い。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| clone_repository | fn clone_repository(repo_url: &str, target_dir: &Path, git_ref: Option<&str>) -> ProfileResult<String> | リポジトリを浅くクローンして参照をチェックアウトし、コミットSHAを返す | O(D)（ダウンロード量） | O(S)（リポジトリ作業コピー） |
| get_commit_sha | fn get_commit_sha(repo_dir: &Path) -> ProfileResult<String> | リポジトリの HEAD のコミット SHA を返す | O(1)（HEAD→commit 参照） | O(1) |
| resolve_reference | fn resolve_reference(repo_url: &str, git_ref: &str) -> ProfileResult<String> | クローンなしで指定参照（ブランチ/タグ/完全名）をコミット SHA に解決 | O(R)（リモート参照数） | O(1) |
| validate_repository | fn validate_repository(repo_url: &str) -> ProfileResult<()> | リポジトリURLが接続可能か検証 | O(1)（接続試行） | O(1) |

詳細説明:

1) clone_repository
- 目的と責務
  - 参照（ブランチ名/タグ名/コミットSHA）が与えられればそれをチェックアウト。リモートの場合は浅いクローン（depth=1）を行う。最終的なチェックアウト状態のコミット SHA を返す。
- アルゴリズム（ステップ分解）
  1. repo_url がローカル（file:// またはパスが存在）か判定。
  2. target_dir の親ディレクトリを作成（存在しない場合）。
  3. target_dir が存在すれば削除（クリーンな作業ディレクトリを確保）。
  4. RemoteCallbacks に credential_callback を設定。
  5. FetchOptions を構築（ローカルでなければ depth=1、タグは AutotagOption::All、proxy auto）。
  6. RepoBuilder に FetchOptions を設定し、git_ref が Some なら branch 設定。
  7. clone を実行。失敗時は ProfileError::GitOperationFailed。
  8. git_ref が Some なら checkout_reference、None なら repo.checkout_head(None)。
  9. get_commit_sha(target_dir) でコミット SHA を取得して返す。
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| repo_url | &str | 必須 | Git リポジトリ URL（file://, ローカルパス, ssh, https 等） |
| target_dir | &Path | 必須 | クローン先ディレクトリ |
| git_ref | Option<&str> | 任意 | ブランチ名/タグ名/コミットSHA、または完全参照名 |

- 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<String> | 成功時はコミット SHA（文字列）、失敗時は ProfileError |

- 使用例
```rust
use std::path::Path;
let sha = clone_repository(
    "https://github.com/example/project.git",
    Path::new("/tmp/project"),
    Some("v1.2.3"), // タグ名
)?;
// sha は v1.2.3 のコミットSHA
```
- エッジケース
  - repo_url が認証を要する場合（credential_callback で対応）
  - git_ref がタグやコミットSHAの場合の HEAD 設定（現在の実装に不備あり、後述）
  - target_dir が既に存在する場合は削除されるため、誤指定による破壊的操作に注意

2) get_commit_sha
- 目的と責務
  - 指定ディレクトリのリポジトリを開き、HEAD を peel してコミット SHA を返す。
- アルゴリズム
  1. Repository::open(repo_dir)
  2. repo.head()
  3. head.peel_to_commit()
  4. commit.id().to_string() を返す
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| repo_dir | &Path | 必須 | Git リポジトリのルートディレクトリ |

- 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<String> | 成功時は HEAD のコミット SHA、失敗時は ProfileError |

- 使用例
```rust
let sha = get_commit_sha(Path::new("/tmp/project"))?;
```
- エッジケース
  - HEAD が不正（未初期化/剥がせない）な場合にエラー

3) resolve_reference
- 目的と責務
  - クローンせずにリモートへ接続し、参照名（完全名, ブランチ名, タグ名）をコミット SHA に解決する。
- アルゴリズム
  1. RemoteCallbacks に credential_callback を設定。
  2. git2::Remote::create_detached(repo_url) で一時的リモート作成。
  3. remote.connect_auth(Direction::Fetch, Some(callbacks), None)
  4. remote.list() で参照一覧取得。
  5. name が git_ref そのもの、refs/heads/git_ref、refs/tags/git_ref のいずれかに一致すればその OID を返す。
  6. 見つからなければ ProfileError::GitOperationFailed を返す。
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| repo_url | &str | 必須 | Git リポジトリ URL |
| git_ref | &str | 必須 | 参照名（完全名、ブランチ名、タグ名） |

- 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<String> | 成功時はコミット SHA、失敗時は ProfileError |

- 使用例
```rust
let sha = resolve_reference("https://github.com/example/project.git", "main")?;
```
- エッジケース
  - 参照名の曖昧性（同名タグとブランチが存在する場合、完全参照名を推奨）
  - プロキシ設定が適用されていない（fetch_opts ではなく connect_auth での適用がない）

4) validate_repository
- 目的と責務
  - リポジトリ URL がフェッチ方向で接続可能かを検証する。存在しない場合は ProviderNotFound を返す。
- アルゴリズム
  1. RemoteCallbacks に credential_callback を設定。
  2. git2::Remote::create_detached(repo_url)
  3. connect_auth(Direction::Fetch, Some(callbacks), None) を試行。
  4. 失敗時にエラーメッセージ文字列を分析し、"not found"/"does not exist" を含めば ProviderNotFound、それ以外は GitOperationFailed。
  5. remote.disconnect() を試みてから Ok(())。
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| repo_url | &str | 必須 | Git リポジトリ URL |

- 戻り値

| 型 | 説明 |
|----|------|
| ProfileResult<()> | 成功時は Unit、失敗時は ProfileError |

- 使用例
```rust
validate_repository("https://github.com/example/project.git")?;
```
- エッジケース
  - エラーメッセージの言語や内容に依存する文字列判定の脆弱性

## Walkthrough & Data Flow

- clone_repository の主なデータフロー
  - 入力: repo_url (&str), target_dir (&Path), git_ref (Option<&str>)
  - 副作用: target_dir の作成/削除、ネットワークアクセス、チェックアウト
  - 出力: 現在の作業ディレクトリ状態に対応するコミット SHA（String）

- 認証・ネットワーク
  - RemoteCallbacks に credential_callback を設定し、SSH鍵（エージェント優先）→デフォルト資格情報→環境変数 GIT_USERNAME/GIT_PASSWORD の順で試行。
  - ProxyOptions::auto() を FetchOptions に適用（clone）。resolve_reference/validate_repository では ProxyOptions が未適用。

- HEAD/チェックアウト
  - git_ref が指定されれば checkout_reference により revparse_single(reference) の結果を checkout_tree し、必要に応じて HEAD を設定。指定なしの場合 checkout_head(None)。

- resolve_reference のフロー
  - リモート接続→参照一覧→簡易マッチ（完全名/ブランチ/タグ）→OID→文字列化して返却。

- get_commit_sha のフロー
  - repo_dir を open→head→peel_to_commit→id→文字列化。

- validate_repository のフロー
  - リモート接続試行→失敗時に文字列マッチで ProviderNotFound へ変換→disconnect→Ok。

Mermaid（条件分岐が4つ以上の clone_repository の主要分岐）:
```mermaid
flowchart TD
  A[Start: repo_url, target_dir, git_ref] --> B{is_local?}
  B -->|Yes| C[FetchOptions: depth not set]
  B -->|No| D[FetchOptions: depth=1]
  C --> E[ProxyOptions::auto]
  D --> E
  E --> F[RemoteCallbacks: credential_callback]
  F --> G[RepoBuilder: set fetch_opts]
  G --> H{git_ref is Some?}
  H -->|Yes| I[builder.branch(reference)]
  H -->|No| J[skip branch]
  I --> K[builder.clone(repo_url, target_dir)]
  J --> K
  K --> L{clone ok?}
  L -->|No| M[Err(GitOperationFailed)]
  L -->|Yes| N{git_ref is Some?}
  N -->|Yes| O[checkout_reference(repo, reference)]
  N -->|No| P[repo.checkout_head(None)]
  O --> Q[get_commit_sha(target_dir)]
  P --> Q
  Q --> R[Return SHA]
```
上記の図は clone_repository 関数の主要分岐（行番号: 不明。このチャンクには行番号が含まれていません）を示す。

## Complexity & Performance

- clone_repository
  - 時間: O(D)、D は取得対象のオブジェクトサイズ合計（ブランチ/タグ/コミットにより差異）。ネットワーク/ディスク I/O支配。
  - 空間: O(S)、S はワークディレクトリ（チェックアウト後のファイル）＋ .git データのサイズ。
  - ボトルネック: ネットワークレイテンシ、帯域、プロキシ、SSHハンドシェイク。Windows での削除/チェックアウトはロックによる遅延あり。
- get_commit_sha
  - 時間: O(1)。HEAD→commit の解決は定数時間相当。
  - 空間: O(1)。
- resolve_reference
  - 時間: O(R)、R はリモートの参照本数（remote.list() 列挙）。
  - 空間: O(1)（参照のメタデータのみ）。
- validate_repository
  - 時間: O(1)（接続試行）。ただしネットワーク往復。
  - 空間: O(1)。

スケール限界:
- 非同期でないため高並列には不向き。大量同時クローン/参照解決ではスレッドブロッキングが発生。
- resolve_reference は参照数が多いリポジトリ（モノレポ）で遅くなり得る。

実運用負荷要因:
- ネットワーク・プロキシ設定の差異。
- 認証手段の可用性（SSHエージェント、環境変数）。
- 大規模リポジトリの浅いクローンでも初回はオブジェクト解決に時間。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| タグ参照で HEAD を適切に設定 | git_ref="v1.0" | タグの指すコミットに detached HEAD を設定 | checkout_reference は obj.as_commit() が Some の場合のみ set_head_detached を実行 | バグの可能性（タグ Object では HEAD 未設定） |
| 誤った target_dir 指定によるディレクトリ削除 | target_dir="/important" | 安全な削除防止、または確認 | target_dir.exists() なら remove_dir_all 実行 | リスク（破壊的操作） |
| ブランチ名がローカルに存在しない | git_ref="main"（clone後にローカルブランチ未生成） | 適切に detached HEAD または origin/main に設定 | find_branch(reference, Local) に依存 | 挙動不明（ローカルブランチ存在しない場合の分岐はあるが revparse 成功条件次第） |
| URL に認証情報が含まれる | https://user:pass@host/repo.git | ログに資格情報を出力しない | エラーメッセージに repo_url を含めて GitOperationFailed を生成 | セキュリティリスク（資格情報漏えい） |
| validate_repository のエラー分類の脆弱性 | 非存在URL / ネットワーク障害 | ProviderNotFound と GitOperationFailed を安定的に区別 | "not found"/"does not exist" の文字列判定 | 脆弱（ロケールや文言に依存） |
| プロキシ設定の不整合 | 企業内プロキシ環境 | すべてのネットワーク操作でプロキシ適用 | clone の FetchOptions のみ ProxyOptions::auto | 改善余地（resolve/validate では未適用） |
| 資格情報環境変数の未設定 | GIT_USERNAME/GIT_PASSWORD 未設定 | 他の認証手段へフォールバック | SSH→default→環境変数 の順 | 問題なし（最後に Err） |
| create_detached の ? によるエラー変換 | validate_repository 内 | ProfileError へ正しく変換 | 直接 ? を使用 | super::error で From<git2::Error> 実装が必須（このチャンクには現れない） |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全なAPIのみ使用、unsafeなし。該当なし。
- インジェクション
  - SQL/Command/Path traversal: 外部コマンド未使用。Path traversal はユーザ入力 target_dir による remove_dir_all の破壊的影響に注意（検証/サンドボックス推奨）。
- 認証・認可
  - 権限チェック漏れ: 該当なし（ローカルファイル権限は OS に依存）。
  - セッション固定: 該当なし。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: エラーメッセージに repo_url を含めるため、URL 埋め込み資格情報漏えいの可能性。
- 並行性
  - Race condition / Deadlock: 該当なし（同期コード）。libgit2 コールバックが並行に呼ばれても credential_callback はステートレス。
  - Send/Sync: このファイルではスレッド越し共有なし。型の Send/Sync 要件は不明（このチャンクには現れない）。

Rust特有の観点:
- 所有権: &str, &Path の借用のみ。値の移動はエラーや String の返却（clone_repository, get_commit_sha, resolve_reference）。
- 借用: 可変借用は FetchOptions/ProxyOptions/RemoteCallbacks の設定に限定。スコープが短く安全。
- ライフタイム: 明示的パラメータ不要。
- unsafe境界: unsafe ブロックなし。安全性根拠不要。
- 並行性・非同期: async/await 未使用。await 境界なし。キャンセル対応なし。
- エラー設計: Result 使用。panic（unwrap/expect）なし。From/Into によるエラー変換の一部が必要（validate_repository 先頭の create_detached 呼び出し）。

## Design & Architecture Suggestions

- checkout_reference の HEAD 設定改善
  - タグ名や完全参照がタグ Object を返す場合、obj.peel_to_commit()（もしくは obj.peel(git2::ObjectType::Commit)）してその id で set_head_detached を行う。コメント「For tags or specific SHAs, detached HEAD」にコードを合わせる。
- 破壊的削除の安全化
  - clone_repository の target_dir.remove_dir_all 前にガード（例: 空である/プロジェクトルート配下のみ許可/設定で上書き許可要否）。誤操作抑止。
- プロキシ設定の一貫性
  - resolve_reference / validate_repository の connect_auth にも ProxyOptions を適用する（libgit2 の transport/proxy 設定に合わせるヘルパーを導入）。
- エラー分類の堅牢化
  - validate_repository で文字列判定ではなく git2::ErrorClass/Code に基づく判定へ移行（このチャンクには具体コードがないため詳細は不明）。少なくとも URL をエラーメッセージに丸ごと含めない設計へ。
- 資格情報取り扱いの安全化
  - エラーメッセージやログに repo_url を出さない、あるいは資格情報部分をマスキングするヘルパー関数を導入。
- 共通設定ヘルパー
  - RemoteCallbacks, FetchOptions, ProxyOptions の構築をユーティリティ関数に抽出し、resolve/validate/clone で共通利用。DRYと一貫性向上。
- 参照解決の汎用化
  - resolve_reference のマッチングを拡張し、優先順位や曖昧一致回避（完全参照指定推奨）を明示化。必要なら "origin/" 接頭辞の扱いも追加。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト（ローカル環境で完結）
  - credential_callback
    - allowed_types を SSHのみ/デフォルトのみ/ユーザパスのみで試験。環境変数 GIT_USERNAME/GIT_PASSWORD の有無で分岐確認。
  - checkout_reference
    - ローカル bare+workdir のテストリポジトリを作成し、ブランチ名/タグ名/コミットSHA でチェックアウトを試験。タグの peel と HEAD 設定が正しく行われるか（要修正後）。
- インテグレーションテスト（ローカル file://）
  - clone_repository
    - 事前にローカルリポジトリを作成（別ディレクトリ）。file:// URL でクローンし、git_ref=None で checkout_head、git_ref=Some("branch") / Some("tag") / Some(commit_sha) の各パスを検証。
```rust
#[test]
fn test_clone_repository_local_tag() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;
    // ローカルにリポジトリを作成しコミットとタグを付与（省略）
    /* ... 省略 ... */
    let sha = clone_repository("file:///path/to/repo", Path::new("/tmp/clone"), Some("v1.0"))?;
    let got = get_commit_sha(Path::new("/tmp/clone"))?;
    assert_eq!(sha, got);
    Ok(())
}
```
  - resolve_reference
    - file:// のリモートに対して list が可能かを検証（libgit2 の制約上、必要に応じてローカルで裸リポジトリを参照）。
- ネガティブテスト
  - validate_repository
    - 存在しない URL に対して ProviderNotFound が返るか。ネットワーク障害（モック/環境依存）時の分類を確認。
  - clone_repository
    - 誤った target_dir を指定した場合のエラー伝播と破壊的削除ガード（改善後）。

テストの注意:
- ネットワーク依存のテストは flaky になりやすい。file:// とローカル repos を使い、必要最小限にする。
- Windows/UNIX ファイルロック差異に配慮。

## Refactoring Plan & Best Practices

- 関数抽出
  - build_fetch_options(is_local: bool) -> FetchOptions
  - build_remote_callbacks() -> RemoteCallbacks
  - apply_proxy_to_connect(...) または connect_with_proxy(remote, callbacks) ヘルパー
- checkout_reference の修正
  - obj.peel_to_commit() を用いてタグ・注釈付きタグを正しく扱い、必ず HEAD を設定する。
- エラーメッセージ整形
  - repo_url の資格情報部分をマスクしてから ProfileError を生成。例: https://user:****@host/repo.git
- 破壊的操作のガード
  - target_dir の削除前に安全チェック関数を導入（例: is_safe_target_dir(target_dir)）。
- 一貫したプロキシ適用
  - resolve_reference と validate_repository にも ProxyOptions を適用。
- ドキュメントと例
  - API ごとの動作と前提（浅いクローンの制限、タグ/ブランチ扱い）を README/Doc コメントに明記。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 成功/失敗時の操作名・対象（資格情報マスク済みURL）・所要時間を記録。
  - RemoteCallbacks に transfer_progress を設定しダウンロード進捗をログ/メトリクスへ。
- メトリクス
  - clone 所要時間、ダウンロードバイト数、参照列挙数（resolve_reference）を計測。
- トレース
  - tracing クレートを用いて各ステップ（親作成、削除、clone、checkout、HEAD取得、remote接続）に span を設定。

## Risks & Unknowns

- Unknowns（このチャンクには現れない）
  - super::error の ProfileError/Result の定義詳細と From<git2::Error> 実装の有無。
  - 実際の上位モジュールからの使用パターン（例: 並列実行、タイムアウト）。
- リスク
  - タグ参照時の HEAD 未設定による状態不一致（checkout_tree と HEAD の乖離）。
  - repo_url をメッセージに埋め込むことによる秘密情報漏えい。
  - validate_repository の文字列依存の分類が誤る可能性（ロケール/文言差異）。
  - 破壊的な remove_dir_all による誤削除。
  - プロキシ適用の不一致が原因の接続失敗（resolve/validate）。