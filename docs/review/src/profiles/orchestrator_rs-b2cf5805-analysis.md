# orchestrator.rs Review

## TL;DR

- 目的: ワークスペースに対するプロファイルのインストールを、衝突解決とロールバック付きで**擬似アトミック**に実行する高レベルオーケストレーション
- 主要公開API: **install_profile**（プロファイルのインストール・更新・ロールバック・ロックファイル/設定更新）
- 複雑箇所: 事前衝突チェック→ファイルコピー→整合性ハッシュ→ロックファイル/設定の二段更新とロールバックの整合性
- 重大リスク:
  - ロックファイル/設定保存失敗時のロールバックが「既存バックアップ有り」の場合しか発動せず、初回インストールで**中途半端な状態**が残る恐れ
  - `current_timestamp` のISO8601算出が簡易近似で、日付が正確でない可能性（閏年・月日計算の誤り）
  - `profile_name` の `..` 含みなどによる**パストラバーサル**リスク（joinのみで正規化/拒否なし）
  - 複数プロセス/スレッドからの同時実行に対する**ファイルロック不在**による競合・破損の可能性

## Overview & Purpose

このファイルは、プロファイルインストールの高レベルな手続きを提供します。主な責務は以下の通りです。

- プロファイルマニフェストの読み込みと検証
- 既存インストールのチェック（強制フラグに応じたバックアップ取得）
- ファイル衝突の事前一括検証（失敗時は一切変更しない）
- ファイルのインストール（必要に応じたサイドカー作成）
- インストール済ファイルの整合性ハッシュ計算
- ロックファイル更新とプロジェクト設定（ProfilesConfig）の更新
- 途中失敗時のロールバック

外部の具体的なファイルコピーや整合性計算は他モジュール（installer/fsops）に委譲し、ここでは順序制御とロールバックの整合性を担います。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | install_profile | pub | プロファイルのインストール全体をオーケストレーション（事前検証、コピー、ハッシュ、ロック/設定更新、ロールバック） | High |
| Function | current_timestamp | private | 現在時刻を簡易ISO8601文字列に整形 | Low |

### Dependencies & Interactions

- 内部依存（このモジュール内の呼び出し関係）
  - install_profile → current_timestamp（ロックエントリの `installed_at` 生成）
- 外部依存（他モジュール/標準ライブラリ）
  - 他モジュール

    | モジュール | シンボル/関数 | 用途 |
    |-----------|---------------|------|
    | super::error | ProfileError, ProfileResult | エラー種別と結果型 |
    | super::fsops | ProfileBackup, backup_profile, calculate_integrity, collect_all_files, restore_profile | バックアップ、整合性計算、ファイル収集、復元 |
    | super::installer | ProfileInstaller, check_all_conflicts | ファイルインストール、衝突の事前検証 |
    | super::lockfile | ProfileLockfile, ProfileLockEntry | ロックファイルの入出力、エントリ作成 |
    | super::manifest | ProfileManifest | プロファイル定義の読み込み |
    | super::project | ProfilesConfig | チーム/プロジェクトのプロファイル設定管理 |
    | super::provider_registry | ProviderSource | インストールソースのメタ情報 |
    | std::path | Path | パス結合・確認 |

  - 標準ライブラリ
    - `std::time::{SystemTime, UNIX_EPOCH}`（`current_timestamp`）

- 被依存推定（このモジュールを利用しそうな箇所）
  - CLIやプラグイン実行パス（コメントにある Plugin reference）から呼び出される高レベルエントリポイント（正確な呼び出し元はこのチャンクには現れない）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| install_profile | pub fn install_profile(profile_name: &str, profiles_dir: &Path, workspace: &Path, force: bool, commit: Option<String>, provider_id: Option<&str>, source: Option<ProviderSource>) -> ProfileResult<()> | プロファイルのインストールを一括実行し、ロールバックとロック/設定更新まで行う | O(n + B) | O(n) |

- 記法:
  - n: インストール対象ファイル数
  - B: 対象ファイル群の総バイト数（ハッシュ計算支配）
  - Spaceは主にファイルリスト保持やメタデータ構築に比例

### install_profile

1) 目的と責務
- 指定プロファイルを `profiles_dir/{profile_name}` から読み込み、`workspace` にインストールする。
- 事前の衝突検査、必要ならバックアップとロールバック、整合性計算、ロックファイル・プロジェクト設定の更新を行う。
- `--force` 相当の `force` 指定時はアップグレード/サイドカーでの衝突解決を許容。

2) アルゴリズム（主ステップ）
- ロックファイルの読み込み
- マニフェスト（`profile.json`）の存在確認と読み込み
- 既存インストール検出と `force` に応じたエラーまたはバックアップ取得
- 対象ファイルの決定（明示列挙 or ディレクトリ全走査）
- 事前衝突検査（いずれかに問題あれば即時中止）
- ファイルインストール実行（サイドカー生成の可能性あり）
- 整合性ハッシュ計算（失敗時ロールバック）
- ロックファイル更新（失敗時ロールバック）
- ProfilesConfig 更新（失敗時ロールバック）

3) 引数

| 引数 | 型 | 説明 |
|------|----|------|
| profile_name | &str | インストールするプロファイル名（パス結合に使用、未正規化） |
| profiles_dir | &Path | プロファイル定義ルートディレクトリ |
| workspace | &Path | インストール先ワークスペース |
| force | bool | 既存インストール上書き/サイドカー許容の可否 |
| commit | Option<String> | 任意のGitコミットSHA等のメタ情報 |
| provider_id | Option<&str> | チーム設定用のプロバイダID |
| source | Option<ProviderSource> | 供給元情報（種別はこのチャンクには現れない） |

4) 戻り値

| 戻り値 | 型 | 説明 |
|--------|----|------|
| 成功 | () | すべての更新が完了 |
| 失敗 | ProfileError | 途中の検証・コピー・保存等でのエラー（詳細は他モジュールのエラー含む） |

5) 使用例

```rust
use std::path::Path;
use crate::profiles::orchestrator::install_profile;
use crate::profiles::provider_registry::ProviderSource;

fn example() -> Result<(), crate::profiles::error::ProfileError> {
    let profiles_dir = Path::new("./profiles");
    let workspace = Path::new("./my-workspace");

    install_profile(
        "webapp",
        profiles_dir,
        workspace,
        /* force */ false,
        /* commit */ Some("abc123def".to_string()),
        /* provider_id */ Some("team-a"),
        /* source */ None, // または Some(ProviderSource::Git { ... }) 等（このチャンクには定義なし）
    )
}
```

6) エッジケース
- profile.json が存在しない／無効
- 既にインストール済かつ `force=false`
- ファイル衝突（同一/異なるプロファイル、unknown owner）
- インストール途中のI/Oエラー
- 整合性計算失敗
- ロックファイル保存失敗
- ProfilesConfig 保存失敗
- 初回インストール時に失敗し、バックアップが存在せずロールバックできないケース

### Data Contracts（このファイルから分かる範囲）

- ProfileLockEntry（フィールドはこのファイルでの初期化から推定）
  - name: String
  - version: String（ProfileManifest 由来）
  - installed_at: String（ISO8601想定だが実装は簡易）
  - files: Vec<String>（ワークスペースからの相対パス想定）
  - integrity: String（calculate_integrity のハッシュ）
  - commit: Option<String>
  - provider_id: Option<String>
  - source: Option<ProviderSource>

- ProfileManifest（このチャンクでは）
  - from_file(&Path) -> Result<Self, ...>
  - version: String
  - files: Vec<String>（空ならディレクトリ走査で補完）
  - provider_name() -> Option<&str> あるいは &str（戻り値はこのチャンクには現れないが、`provider_name` は installer への引数で使用）

- ProfileLockfile（このチャンクでは）
  - load(&Path) -> Result<Self, ...>
  - get_profile(&str) -> Option<...>（.version 使用）
  - add_profile(ProfileLockEntry)
  - remove_profile(&str)
  - save(&Path) -> Result<(), ...>

- ProfilesConfig（このチャンクでは）
  - load(&Path) -> Result<Self, ...>
  - add_profile(&str)
  - save(&Path) -> Result<(), ...>

- installer / fsops（このチャンクでは）
  - installer::check_all_conflicts(...)
  - ProfileInstaller::new().install_files(...) -> Result<(Vec<String>, Vec<(String, String)>), ...>
  - backup_profile(...) -> Result<ProfileBackup, ...>
  - restore_profile(&ProfileBackup) -> Result<(), ...>
  - collect_all_files(&Path) -> Result<Vec<String>, ...>
  - calculate_integrity(&[String]) -> Result<String, ...>

不明な点はこのチャンクには現れない。

## Walkthrough & Data Flow

- ステップ
  1) ロックファイル読み込み
  2) `profiles_dir/{profile_name}/profile.json` 存在確認と読み込み
  3) 既存プロファイル確認。`force=false` なら AlreadyInstalled エラー、`force=true` ならバックアップ取得
  4) 対象ファイル決定（マニフェスト列挙 or ディレクトリ全収集）
  5) 事前衝突検査（全ファイル分を一挙チェック）
  6) ファイルコピー実行（サイドカー生成の可能性）
  7) インストール済ファイルの絶対パス化→整合性ハッシュ計算
  8) ロックファイル更新/保存（失敗時：エントリ削除 + 可能なら復元）
  9) ProfilesConfig 更新/保存（失敗時：ロックファイル巻き戻し + 可能なら復元）

- Mermaid フローチャート

```mermaid
flowchart TD
  A[Start install_profile] --> B[Load ProfileLockfile]
  B --> C[Resolve profile_dir/profile.json]
  C -->|missing| E1[Err InvalidManifest]
  C -->|exists| D[Load ProfileManifest]
  D --> E{lockfile.get_profile?}
  E -->|Some & force=false| E2[Err AlreadyInstalled]
  E -->|Some & force=true| F[backup_profile]
  E -->|None| G[No backup]
  F --> G
  G --> H[Select files_to_install (manifest.files or collect_all_files)]
  H --> I[installer::check_all_conflicts]
  I -->|Err| E3[Err Conflict]
  I -->|Ok| J[installer.install_files]
  J -->|Err| R1{Has backup?}
  R1 -->|Yes| R2[restore_profile] --> E4[Err InstallFailed]
  R1 -->|No| E4
  J -->|Ok(installed, sidecars)| K{sidecars.is_empty?}
  K -->|No| K1[Warn: sidecars summary] --> L[calculate_integrity]
  K -->|Yes| L
  L -->|Err| R3{Has backup?}
  R3 -->|Yes| R4[restore_profile] --> E5[Err Integrity]
  R3 -->|No| E5
  L -->|Ok| M[Build ProfileLockEntry]
  M --> N[lockfile.add_profile + save]
  N -->|Err| R5{Has backup?}
  R5 -->|Yes| R6[restore_profile] --> E6[Err LockfileSave]
  R5 -->|No| E6
  N -->|Ok| O[Load ProfilesConfig]
  O --> P[profiles_config.add_profile + save]
  P -->|Err| Q[lockfile.remove_profile + save; restore if backup] --> E7[Err ConfigSave]
  P -->|Ok| Z[Ok]
```

上記の図は本ファイルの install_profile 関数の主要分岐を示す。

- 重要な抜粋（事前衝突チェック→コピー→整合性→ロック/設定保存）

```rust
// 事前衝突チェック（失敗なら一切書き込まない）
installer::check_all_conflicts(workspace, &files_to_install, profile_name, &lockfile, force)?;

// コピー実行（失敗時、バックアップがあれば復元）
let (installed_files, sidecars) = match installer.install_files(...) {
    Ok(result) => result,
    Err(e) => { if let Some(b) = backup { let _ = restore_profile(&b); } return Err(e); }
};

// 整合性計算（失敗時、バックアップがあれば復元）
let integrity = match calculate_integrity(&absolute_files) {
    Ok(hash) => hash,
    Err(e) => { if let Some(b) = backup { let _ = restore_profile(&b); } return Err(e); }
};

// ロックファイル保存（失敗時、エントリ削除＆バックアップ復元の試み）
lockfile.add_profile(entry);
if let Err(e) = lockfile.save(&lockfile_path) {
    lockfile.remove_profile(profile_name);
    if let Some(b) = backup { let _ = restore_profile(&b); }
    return Err(e);
}

// ProfilesConfig 保存（失敗時、ロックファイル巻き戻し＆バックアップ復元の試み）
if let Err(e) = profiles_config.save(&profiles_config_path) {
    lockfile.remove_profile(profile_name);
    let _ = lockfile.save(&lockfile_path);
    if let Some(b) = backup { let _ = restore_profile(&b); }
    return Err(e);
}
```

## Complexity & Performance

- 計算量
  - 事前衝突検査: O(n)
  - ファイルコピー: O(n + I/O)
  - 整合性ハッシュ: O(B)（B=ファイル群の総バイト数）
  - ロック/設定の保存: O(n)（シリアライズと書き込み）
  - 総合: 時間 O(n + B)、空間 O(n)（ファイルリストやメタ保持）
- ボトルネック
  - 大規模プロファイルでのハッシュ計算（Bが大）とI/O
  - 複数段のファイルシステム書き込み（コピー、ロックファイル、設定）
- スケール限界
  - 単一スレッド・同期I/Oのため大容量/多数ファイルで遅延
  - 競合なしのアトミシティは「事前検証」に依存し、実システム障害（途中I/O失敗）は完全に原子ではない
- 実運用負荷要因
  - ストレージ速度
  - ウイルススキャン・ファイル監視による遅延
  - 複数プロセスからの同時操作（ロック不在）

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| プロファイル未存在 | profile_name="x", profiles_dir="..." | InvalidManifest エラー | `!manifest_path.exists()` で Err | OK |
| 既にインストール済・force=false | 同名プロファイル | AlreadyInstalled エラー | `get_profile` かつ `!force` | OK |
| 既にインストール済・force=true | 同名プロファイル | バックアップ取得後に続行 | `backup_profile` 呼び出し | OK |
| manifest.files 空 | files=[] | ディレクトリ全走査 | `collect_all_files` | OK |
| 衝突あり・force=false | 既存ファイル他プロファイル所有等 | エラーで中止、ファイル無変更 | `check_all_conflicts` | OK |
| 衝突あり・force=true | 同 | サイドカー作成し続行 | `install_files` 結果の sidecars | OK |
| コピー途中の失敗 | I/Oエラー等 | 変更を全て巻き戻し | バックアップがあるときのみ `restore_profile` | 初回インストールで不十分 |
| 整合性計算失敗 | 読み取り不可等 | ロールバック | バックアップがあるときのみ実施 | 初回インストールで不十分 |
| ロックファイル保存失敗 | 書き込み不可等 | エントリ削除＋復元 | バックアップがあるときのみ復元 | 初回インストールで不十分 |
| ProfilesConfig保存失敗 | 同上 | ロックファイル巻戻し＋復元 | バックアップがあるときのみ復元 | 初回インストールで不十分 |
| パストラバーサル | profile_name="../evil" | 拒否/正規化 | joinのみで未防御 | 要対策 |

- セキュリティチェックリスト
  - メモリ安全性: unsafe未使用。Rustの範囲で概ね安全。
  - インジェクション:
    - パストラバーサル: `profiles_dir.join(profile_name)` に入力検証がなく、`..` やセパレータ混入で意図外パスにアクセスし得る。対策必須。
    - その他（SQL/Command）: 該当なし。
  - 認証・認可: 本関数内では該当なし（ファイルシステム権限はOS任せ）。
  - 秘密情報:
    - ハードコード秘密なし。
    - `eprintln!` によるログ漏えいは低リスクだが、ワークスペースパスやファイル名が出力される可能性はある。必要ならサニタイズ/レベル制御。
  - 並行性:
    - ロックファイル/設定ファイルへの排他制御なし。複数プロセス/スレッドからの同時実行で競合・破損のリスク。
    - 途中失敗時の復元はベストエフォートで、完全アトミックではない。
  - 例外/パニック:
    - `current_timestamp` 内の `expect("Time went backwards")` は理論上パニック可能。システム時間の後退を許容しない設計だが、より堅牢なハンドリングが望ましい。

- Rust特有の観点（詳細）
  - 所有権:
    - `commit: Option<String>` は `entry` にムーブ。`provider_id: Option<&str>` は `map(String::from)` で所有権を新規取得し安全。
  - 借用/ライフタイム:
    - `lockfile` は可変で保持し、必要時に参照/更新。外部への参照を返さないためライフタイム問題は顕在化しない。
  - unsafe境界: このチャンクには現れない（unsafe未使用）。
  - 並行性/非同期:
    - 同期関数。`Send/Sync` 境界は不要。共有状態はファイルシステムのみで、アプリ内ロックはなし。
  - エラー設計:
    - `ProfileResult<()>` を返却。途中で `Err(e)` を返しロールバックを試みる設計。
    - `unwrap/expect` は `current_timestamp` のみで使用（改善余地あり）。

## Design & Architecture Suggestions

- アトミック性強化
  - ステージングディレクトリに展開→最後にリネームでカットオーバー（同一ファイルシステム上での原子的 `rename` を活用）
  - 変更ログ（ジャーナル）と**RAIIトランザクション**オブジェクト（Dropで自動ロールバック）を導入
  - 初回インストールでもロールバック可能な「一時バックアップ（作業前のスナップショット）」を作る
- 排他制御
  - ロックファイル/ProfilesConfigの**ファイルロック**（advisory lock）導入で同時実行を禁止
- 入力検証
  - `profile_name` の正規表現・正規化（`..`, 絶対パス、セパレータ混入の拒否）
  - `profiles_dir`/`workspace` の `canonicalize` と安全なパス前提のチェック
- 時刻処理
  - `current_timestamp` は暦計算を自前で行わず、`time`/`chrono` クレートや `SystemTime` → `DateTime<Utc>` 変換を使用
  - システム時間の後退に対する堅牢処理（fallback/警告）
- ログ/監視（詳細は Observability 参照）
  - `eprintln!` から構造化ログ（`tracing` など）へ
- エラーとロールバックの一元化
  - すべてのフェーズで「影響を与えた変更の記録」を残し、失敗時に一箇所で巻き戻し
- サイドカー命名規則の仕様化
  - `stem.provider.ext` の記述はあるが、実装の正確性/衝突時の再サイドカー規則を仕様化（このチャンクには現れない）

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト観点
  - プロファイル未存在 → InvalidManifest
  - 既存インストール + `force=false` → AlreadyInstalled
  - `manifest.files=[]` → `collect_all_files` が呼ばれること
  - 事前衝突検査エラー → ファイル未変更
  - `install_files` 失敗 → バックアップがあるときは `restore_profile` 呼び出し
  - 整合性計算失敗 → 同上
  - ロックファイル保存失敗 → エントリ削除＋復元
  - ProfilesConfig 保存失敗 → ロック巻戻し＋復元
  - 初回インストール失敗（バックアップなし） → 中間ファイル残留しうることの検知と今後の改善テスト
  - `profile_name` に `..` を含む場合の拒否（改善後テスト）

- 統合テスト観点
  - 一時ディレクトリ上に模擬 `profiles_dir`/`workspace` を作成
  - ファイル衝突（同一/異プロファイル）で `force` 有無を切替
  - 大量ファイル・大容量ファイルでのハッシュ計算
  - ロックファイル/設定ファイルの同時アクセス（別プロセス擬似）での挙動

- テスト例（疑似）

```rust
#[test]
fn install_fails_without_force_when_already_installed() {
    use std::path::Path;
    let tmp = tempfile::tempdir().unwrap();
    let profiles_dir = tmp.path().join("profiles");
    let workspace = tmp.path().join("ws");
    std::fs::create_dir_all(&profiles_dir).unwrap();
    std::fs::create_dir_all(&workspace).unwrap();

    // profiles_dir/myprof/profile.json を用意
    let prof_dir = profiles_dir.join("myprof");
    std::fs::create_dir_all(&prof_dir).unwrap();
    std::fs::write(prof_dir.join("profile.json"), r#"{"version":"1.0.0","files":["a.txt"]}"#).unwrap();
    // ワークスペースとロックファイル初期状態を用意（このチャンクではロックフォーマット不明のため擬似）

    // 初回インストール（成功想定）
    let r1 = super::install_profile("myprof", &profiles_dir, &workspace, false, None, None, None);
    assert!(r1.is_ok());

    // 再インストール（force=false）で失敗想定
    let r2 = super::install_profile("myprof", &profiles_dir, &workspace, false, None, None, None);
    assert!(r2.is_err());
}
```

このチャンクには外部型の具体仕様が現れないため、実際にはテスト用のモック/スタブが必要。

## Refactoring Plan & Best Practices

- 取引（Transaction）オブジェクト導入
  - 事前計画（Plan）→ 実行（Apply）→ コミット（Commit）→ 失敗時は Dropでロールバック
  - 影響範囲（コピーしたファイルリスト、作成/上書き情報）を記録
- パス検証ユーティリティ
  - `sanitize_profile_name(profile_name) -> Result<ValidName, Error>` を導入し、`..`, '/', '\\' 等を拒否
- タイムスタンプ
  - `current_timestamp()` を標準/外部ライブラリで正確に実装し、パニック排除
- エラーの粒度向上
  - Lockfile保存失敗、Config保存失敗、整合性計算失敗などの識別可能なエラーコード/メッセージ
- ログ強化
  - `tracing` による span で各フェーズを囲み、サイドカー数・ファイル数・バイト数などを記録
- 並行実行の安全化
  - ファイルロック/アプリ内ミューテックス（プロセス間はファイルロック）と再試行戦略

## Observability (Logging, Metrics, Tracing)

- ログ（構造化）
  - level=INFO: 開始/成功、プロフィール名、ファイル数、サイドカー数
  - level=WARN: 衝突発生、サイドカー生成一覧
  - level=ERROR: 各フェーズの失敗理由（install, integrity, lockfile, config）
  - install_id（UUID）を発行し全ログに紐付け
- メトリクス
  - counter: installs_total, installs_failed_total
  - histogram: install_duration_seconds, files_count, total_bytes
  - gauge: sidecars_count
- トレーシング
  - spans: preflight, copy, hash, lockfile_update, config_update, rollback
  - エラー時は span にエラータグと原因チェーンを付与

## Risks & Unknowns

- Unknowns（このチャンクには現れない）
  - `ProfileManifest`, `ProfileInstaller`, `ProfileLockfile`, `ProfilesConfig`, `ProviderSource` の詳細仕様
  - `check_all_conflicts` と `install_files` の正確な衝突解決実装
  - `calculate_integrity` のハッシュアルゴリズム/フォーマット
- リスク
  - 初回インストール失敗時のロールバック未実装によりファイル残骸が生じる
  - パストラバーサル（profile_name 未検証）
  - 並行実行によるロック/設定ファイル破損
  - `current_timestamp` の日付計算が不正確（閏年/各月日数未考慮）で**データ契約（installed_atのISO8601）違反の可能性**
  - OS/FS依存（リネーム/上書きの原子性、シンボリックリンクの扱い、権限エラー）