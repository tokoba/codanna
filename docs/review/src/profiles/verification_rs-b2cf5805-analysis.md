# verification.rs Review

## TL;DR

- 目的: ロックファイルに記録されたプロファイルのファイル群の整合性（ハッシュ）を検証する（公開API: **verify_profile**, **verify_all_profiles**）。
- コアロジック: 相対パスをワークスペースへ結合し、全ファイルのハッシュを集約（calculate_integrity）してロックファイルの期待値と比較。
- 重大なリスク: 非UTF-8パスを`to_string_lossy()`で文字列化してハッシュ対象に渡すため、パス破損による誤検証の可能性。相対パスに`..`を含む場合のワークスペース外アクセス（パストラバーサル）未防止。
- エラー設計: `Option`→`Result`へ適切変換、`?`で委譲、特定エラー型（NotInstalled, IntegrityCheckFailed）を明示的に返却。
- 並行性: 同期・直列処理のみ。ファイル変更の競合（検証中の変更）への対処なし。
- 複雑箇所: レガシーロックファイル（integrity空）へのスキップ挙動、全プロファイル検証時の順次処理。
- セキュリティ: 標準出力への`println!`使用（ライブラリAPIとしてはログ設計不足）、パス検証・権限チェックの不足。

## Overview & Purpose

このモジュールは、ワークスペース配下の「.codanna/profiles.lock.json」に記録されたプロファイルに対応するファイル群の整合性を検証し、改変や欠落を検出するための機能を提供します。主な目的は、ロックファイルに保存された**期待ハッシュ（integrity）**と、実際に存在するファイルから算出した**実ハッシュ**を比較することです。

- verify_profile: 指定プロファイル名に対して検証を実行。
- verify_all_profiles: ロックファイル内の全プロファイルに対して検証を実行。
- 内部のコアロジックは verify_profile_entry に集約されています。

用途の文脈（コメント内の参照）として、プラグインシステム（src/plugins/mod.rs の verify_entry / verify_plugin / verify_all_plugins）との連携が示されていますが、詳細はこのチャンクには現れないため不明です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | verify_profile | pub | 単一プロファイルの整合性検証エントリポイント | Low |
| Function | verify_all_profiles | pub | 全プロファイルの一括整合性検証 | Low |
| Function | verify_profile_entry | private | 検証のコアロジック（ハッシュ算出と比較） | Low |
| Struct(外部) | ProfileLockfile | 外部依存 | ロックファイルのロード/保存/参照 | 不明 |
| Struct(外部) | ProfileLockEntry | 外部依存 | プロファイルのメタ情報とファイルリスト | 不明 |
| Function(外部) | calculate_integrity | 外部依存 | 複数ファイルのハッシュ集約値を算出 | 不明 |
| Type(外部) | ProfileResult / ProfileError | 外部依存 | エラー型と結果型（Resultエイリアス） | 不明 |

### Dependencies & Interactions

- 内部依存
  - verify_profile → ProfileLockfile::load → get_profile → verify_profile_entry
  - verify_all_profiles → ProfileLockfile::load → profiles.values() → verify_profile_entry
  - verify_profile_entry → calculate_integrity

- 外部依存（推定・表）
  | 依存対象 | 役割 | 備考 |
  |----------|------|------|
  | super::lockfile::{ProfileLockfile, ProfileLockEntry} | ロックファイルの入出力とプロファイル参照 | このチャンクには定義が現れない |
  | super::fsops::calculate_integrity | ファイル群の整合性ハッシュ計算 | 引数は`Vec<String>` |
  | super::error::{ProfileError, ProfileResult} | エラー型・Resultエイリアス | `?`で委譲 |
  | std::path::Path | パス操作 | ワークスペース基準で相対パス結合 |
  | 標準出力（println!） | ログ出力 | ライブラリAPIとしては粗い |

- 被依存推定
  - プラグインの検証機能から呼ばれる（コメントの参照: verify_entry, verify_plugin, verify_all_plugins）。詳細はこのチャンクには現れない。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| verify_profile | `pub fn verify_profile(workspace: &Path, profile_name: &str, verbose: bool) -> ProfileResult<()>` | 指定プロファイルの整合性検証 | O(S)（S=対象ファイル総バイト） | O(1)  |
| verify_all_profiles | `pub fn verify_all_profiles(workspace: &Path, verbose: bool) -> ProfileResult<()>` | ロックファイル内の全プロファイルの整合性検証 | O(ΣS_i)（全プロファイルの総バイト） | O(1) |

詳細（各API）

1) verify_profile
- 目的と責務
  - 指定されたプロファイル名がロックファイルに存在するか確認し、そのファイル群の整合性を検証する。
- アルゴリズム（ステップ分解）
  1. `workspace/.codanna/profiles.lock.json` をロード（ProfileLockfile::load）。
  2. `get_profile(profile_name)`で該当エントリを取得。なければ`ProfileError::NotInstalled`。
  3. `verify_profile_entry(workspace, entry, verbose)`を呼び出し。
- 引数
  | 名前 | 型 | 意味 |
  |------|----|------|
  | workspace | &Path | ワークスペースのルートディレクトリ |
  | profile_name | &str | 検証対象プロファイル名 |
  | verbose | bool | 詳細ログ出力フラグ |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | ProfileResult<()> | 成功は`Ok(())`、失敗は`ProfileError` |
- 使用例
  ```rust
  use std::path::Path;
  use profiles::verification::verify_profile;

  let workspace = Path::new("/path/to/workspace");
  let result = verify_profile(workspace, "analytics", true);
  if let Err(e) = result {
      eprintln!("Verification failed: {e}");
  }
  ```
- エッジケース
  - プロファイルがロックファイルに存在しない → `NotInstalled`
  - ロックファイルが読み込めない → エラー委譲（詳細型は外部）
  - レガシーロックファイル（integrity空） → 検証スキップ

2) verify_all_profiles
- 目的と責務
  - ロックファイルに登録された全プロファイルの整合性を順次検証する。
- アルゴリズム（ステップ分解）
  1. ロックファイルをロード。
  2. プロファイル数が0なら`verbose`時に「No profiles installed」を表示して終了。
  3. `profiles.values()`で全エントリを反復し、`verify_profile_entry`を呼ぶ。
  4. すべて成功なら「All profiles verified successfully」を表示。
- 引数
  | 名前 | 型 | 意味 |
  |------|----|------|
  | workspace | &Path | ワークスペースのルート |
  | verbose | bool | 詳細ログ出力フラグ |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | ProfileResult<()> | いずれかが失敗したら最初の失敗でErr |
- 使用例
  ```rust
  use std::path::Path;
  use profiles::verification::verify_all_profiles;

  let workspace = Path::new("/path/to/workspace");
  verify_all_profiles(workspace, false)?;
  ```
- エッジケース
  - プロファイルが空 → スキップして`Ok(())`
  - 途中でひとつでも失敗 → その時点でErrを返す

## Walkthrough & Data Flow

処理の主な流れは verify_profile_entry にあります。

- 入力: workspace（&Path）、entry（&ProfileLockEntry）、verbose（bool）
- 手順
  1. verboseなら、プロファイル名・保存済みintegrityを表示。
  2. integrityが空文字ならレガシー扱いで警告表示の上、検証スキップ（成功扱い）。
  3. entry.files（相対パスのVec<String>）をworkspaceに結合し、絶対パス風の文字列（to_string_lossy）へ変換。
  4. `calculate_integrity(&absolute_files)`を呼び、実ハッシュを得る。
  5. 実ハッシュと保存済みハッシュを比較。異なれば`IntegrityCheckFailed`を返す。
  6. verboseなら詳細、そうでなければ成功メッセージを表示して終了。

Mermaidフローチャート（条件分岐が4つ以上）
```mermaid
flowchart TD
    A[Start: verify_profile_entry] --> B{verbose?}
    B -- yes --> B1[print 'Verifying' + stored integrity]
    B -- no --> C
    B1 --> C{entry.integrity is empty?}
    C -- yes --> C1[print legacy warning]
    C1 --> Z[Return Ok()]
    C -- no --> D[Build absolute_files from workspace + entry.files]
    D --> E[actual = calculate_integrity(absolute_files)]
    E --> F{actual != entry.integrity?}
    F -- yes --> F1[Return Err(IntegrityCheckFailed)]
    F -- no --> G{verbose?}
    G -- yes --> G1[print calculated integrity + OK]
    G -- no --> G2[print 'Profile verified']
    G1 --> Z
    G2 --> Z
```
上記の図は`verify_profile_entry`関数（行番号: 不明）の主要分岐を示す。

## Complexity & Performance

- verify_profile
  - 時間計算量: O(S)（S=対象ファイルの総バイト数。ハッシュ計算が支配的）
  - 空間計算量: O(1)（ストリーム的に読み出せば定数だが、実実装は外部関数次第。不明要素あり）
  - ボトルネック: ディスクI/Oとハッシュ計算。ファイル数・サイズが増えるほど遅延。
- verify_all_profiles
  - 時間計算量: O(ΣS_i)（全プロファイルの合計バイト）
  - 空間計算量: O(1)
- 実運用負荷要因
  - I/O: 多数・大容量ファイルの読み込み。
  - CPU: ハッシュ計算。
  - 競合: 検証中のファイル更新による結果不安定性。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト（このチャンクに現れる設計・コードの範囲に限る）

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全機構内でunsafe未使用。該当なし。
  - 非UTF-8パスを`to_string_lossy()`へ変換しているため、別種のロジックバグの可能性（下記参照）。
- インジェクション
  - SQL/Command: 該当なし。
  - Path traversal: `workspace.join(rel)`で`rel`が`..`等を含む場合にワークスペース外へのアクセスが可能。検証処理とはいえ、意図しない領域の読み取りリスクがある（防御未実装）。
- 認証・認可
  - 該当なし（ローカルファイル検証のみ）。
- 秘密情報
  - 期待ハッシュ・実ハッシュを`IntegrityCheckFailed`エラーに含める。ハッシュそのものは秘密情報ではないが、ログ出力層で漏れうる可能性（設計次第）。このチャンクにはログポリシーは現れない。
- 並行性
  - Race condition: 検証中のファイル変更に対する不整合可能性（トランザクション的整合は担保なし）。
  - Deadlock: 該当なし（同期直列処理）。
  - Send/Sync: スレッド共有前提なし。該当なし。

詳細エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| プロファイル未インストール | "nonexistent" | Err(NotInstalled) | `verify_profile`で`ok_or_else` | OK |
| レガシーintegrity空 | integrity="" | 警告してスキップ（Ok） | `verify_profile_entry`で空判定 | OK |
| ファイル改変 | 内容変更 | Err(IntegrityCheckFailed) | 実ハッシュ比較 | OK |
| ファイル欠落 | 削除 | Err(IntegrityCheckFailed)（calculate_integrityが失敗または不一致） | 例外委譲 | OK |
| 権限不足 | 読み取り不可 | Err(...) | `calculate_integrity`へ委譲 | OK（型詳細不明） |
| 非UTF-8パス | OsStrに非UTF-8含む | 正常検証 | `to_string_lossy`が置換文字により誤パス化 | 未対応（改善要） |
| パストラバーサル | "../secret" | ワークスペース外は拒否 | joinのみで許容されうる | 未対応（改善要） |
| 検証中の変更 | 書換ながら検証 | 一貫した結果 | 排他なしで不安定 | 未対応 |
| プロファイルなし（verify_all） | profiles空 | Ok（必要なら情報ログ） | 実装済み | OK |
| 大容量/多数ファイル | 数万ファイル | 許容時間内で完了 | 直列で遅い可能性 | 潜在課題 |

根拠（関数名:行番号）
- レガシースキップ: `verify_profile_entry` integrity空判定（行番号:不明）
- パス変換: `verify_profile_entry`で`workspace.join(rel).to_string_lossy()`（行番号:不明）
- 例外委譲とエラー型: `verify_profile`, `verify_all_profiles` 内の`?`と`ProfileError`（行番号:不明）

## Design & Architecture Suggestions

- パスの取り扱い改善
  - calculate_integrityの引数を`impl AsRef<Path>`や`&[PathBuf]`に変更し、非UTF-8パスのロスレス処理を可能にする。
  - 検証前に`canonicalize()`して、ワークスペース配下であることをチェック（パストラバーサル対策）。
- ロギング
  - `println!`ではなく、`log`クレート（またはトレース統合）を利用し、ライブラリ層でメッセージレベル管理（info/warn/error）を行う。
- 検証の一貫性
  - ファイル更新との競合に対して、タイムスタンプ/サイズの事前取得と一貫したスナップショット的読み出しが理想。難しければ「検証開始〜完了の間に変更が検出された場合のリトライ」などの戦略を導入。
- 並列化
  - 多数ファイルのハッシュ計算をスレッドプールで並列化（I/OとCPUのバランス）。ただし順序・決定性維持のため集約順序には注意。
- エラー表現
  - `IntegrityCheckFailed`に差分（どのファイルが不一致/欠落か）を含めるとUX向上。現在は集約ハッシュ不一致のみ。

## Testing Strategy (Unit/Integration) with Examples

既存テストは以下を網羅
- 成功ケース（整合性一致）
- 改変による失敗
- 欠落による失敗
- 全プロファイル検証の成功
- レガシーintegrity空のスキップ
- 未インストールのエラー

追加を推奨するテスト
- 非UTF-8パス
  ```rust
  // OS依存だが、非UTF-8バイトを含むファイル名を作成し、検証が誤って失敗/成功しないか確認。
  // calculate_integrityがString前提のため、現状は作成自体困難。API変更後に追加。
  ```
- パストラバーサルの防止
  ```rust
  // lockfileのentry.filesに "../outside.txt" を含め、検証関数が拒否することを確認（防止ロジック追加後）
  ```
- 権限不足
  ```rust
  // 対象ファイルの読み取り権限を外し、calculate_integrityがErrを返し、それが委譲されることを確認。
  ```
- 競合変更
  ```rust
  // 別スレッドで検証中にファイルを書き換え、結果の安定性や再試行戦略（導入後）を検証。
  ```
- verboseの出力
  ```rust
  // capture出力（例えば`assert_cmd`や`duct`等活用）で、verbose=true時のメッセージを検証。
  ```
- 空ロックファイル（verify_all）
  ```rust
  // profiles.is_empty() 時にOkを返し、副作用出力がverboseのみであることを確認。
  ```

## Refactoring Plan & Best Practices

- calculate_integrityのインターフェース変更
  - `fn calculate_integrity(paths: &[PathBuf]) -> ProfileResult<String>`へ変更。
  - 既存呼び出しを`entry.files.iter().map(|rel| workspace.join(rel)).collect::<Vec<_>>()`に置換。
- パス検証
  - `let abs = workspace.join(rel).canonicalize()?;`
  - `abs.starts_with(workspace.canonicalize()?)`を検査し、逸脱時はErr(PathOutsideWorkspace)。
- ログ抽象化
  - `println!`を廃し、`log::info!`, `log::warn!`等へ置換。verboseフラグに依存せずログレベルで制御。
- 詳細エラー
  - `IntegrityCheckFailed`にフィールド追加：`mismatched_files: Vec<PathBuf>`, `missing_files: Vec<PathBuf>`など。
- 並列ハッシュ
  - `rayon`等で並列化。ただし順序決定性はソート＋安定集約で確保。
- API一貫性
  - `verify_all_profiles`の成功メッセージはverboseに寄せる。ライブラリ側で標準出力に直接出さない。

## Observability (Logging, Metrics, Tracing)

- ログ
  - レベル: info（開始/成功）、warn（レガシースキップ）、error（不一致/欠落）。
  - 構造化ログ（プロファイル名、ファイル数、経過時間、ハッシュ値の短縮表示）。
- メトリクス
  - 検証時間（ヒストグラム）
  - 検証対象ファイル数
  - 成功/失敗カウント
- トレーシング
  - スパン: `verify_profile`、`verify_profile_entry`
  - 属性: `profile_name`, `file_count`

## Risks & Unknowns

- 外部型の詳細不明
  - ProfileLockfile/Entryの内部構造や`calculate_integrity`の具体的実装はこのチャンクには現れないため不明。
- プラグイン連携
  - コメントの参照先（src/plugins/mod.rs）との具体的な制御フローは不明。戻り値・ログ方針の整合要確認。
- パスの文字コード
  - 現状`to_string_lossy`依存。非UTF-8環境での挙動は不明。
- ロックファイルのバージョニング
  - レガシー判定はintegrity空のみ。将来のフォーマット変更時の互換層は不明。
- 変更競合
  - 検証中の変更に対するポリシー（検出・再試行・失敗扱い）の定義は不明。