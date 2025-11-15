# profiles\manifest.rs Review

## TL;DR

- 目的: **プロファイルのマニフェスト（profile.json）を読み込み・検証するためのデータ構造とAPI**を提供
- 公開API: **ProfileManifest**（構造体）、**provider_name(&self) -> &str**、**from_file(&Path) -> ProfileResult<Self>**、**from_json(&str) -> ProfileResult<Self>**
- コアロジック: JSONからのデシリアライズ後に**空文字列のファイルパスを除去**し、**name/versionの空チェック**で検証
- 複雑箇所: シンプルだが、**serdeのデフォルト挙動（#[serde(default)]）**と**エラー変換（?演算子）**が前提となる点が暗黙的
- 重大リスク: **パス検証の欠如によるパストラバーサル/任意ファイル読み込み**、**versionフォーマット未検証**、**未知フィールド取り扱いの不明確さ**
- Rust安全性: **unsafe不使用**、**借用寿命は安全**、Errorsは**ProfileResult/ProfileError**を使用（詳細は他チャンクに依存）
- 並行性: 明示的な**非同期/並行処理なし**。構造体は**Send/Sync**に問題ない推定（String/Vecのみ）

## Overview & Purpose

このファイルは、プロバイダレポジトリ内の `profiles/{name}/profile.json` に配置されるプロファイルのマニフェストを読み込み、パースし、基本的な検証を行うための**データモデル**と**ユーティリティ関数**を提供します。主な用途は以下の通りです。

- JSONファイルから**ProfileManifest**のインスタンスを読み込む（from_file）
- JSON文字列から**ProfileManifest**をパースする（from_json）
- `provider` が未指定の場合に**プロファイル名をプロバイダ名として返す**（provider_name）
- マニフェストの**必須フィールド（name、version）チェック**（validate）

このチャンクは**I/O、デシリアライズ、軽微な検証**のみを扱います。より高度な検証（セマンティックバージョンの検証、ファイルパスの正当性、未知フィールドの扱いなど）は実装されていません。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | ProfileManifest | pub | マニフェストデータモデル（name, version, provider, files） | Low |
| Function | ProfileManifest::provider_name | pub | provider未指定時のフォールバック（name）提供 | Low |
| Function | ProfileManifest::from_file | pub | JSONファイル読み込み→from_jsonへ委譲 | Low |
| Function | ProfileManifest::from_json | pub | JSON文字列パース→ファイルパスの空除去→validate | Low |
| Function | ProfileManifest::validate | private | name/version 非空チェック | Low |

内部の重要なコード根拠（行番号不明のため関数名のみ併記）:
- 空ファイルパス除去は from_json で `manifest.files.retain(|f| !f.is_empty())` にて実施（from_json: 行番号不明）
- 必須フィールドの検証は validate にて `is_empty()` チェックで Err(ProfileError::InvalidManifest) を返す（validate: 行番号不明）
- provider のフォールバックは provider_name にて `self.provider.as_deref().unwrap_or(&self.name)`（provider_name: 行番号不明）

### Dependencies & Interactions

- 内部依存
  - from_file → from_json（I/O結果をパース）
  - from_json → serde_json::from_str（デシリアライズ）
  - from_json → validate（検証）
  - provider_name（独立）

- 外部依存（クレート/モジュール）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | super::error::{ProfileError, ProfileResult} | エラー型・結果型 | 具体定義は他チャンク。`?`によりio/jsonエラー変換が必要（不明） |
  | serde::{Deserialize, Serialize} | 構造体のシリアライズ/デシリアライズ | #[derive] |
  | serde_json | JSONパース | from_jsonで使用 |
  | std::fs::read_to_string | ファイル読み込み | from_fileで使用 |
  | std::path::Path | パス型 | from_file引数 |

- 被依存推定
  - プロファイル管理ロジック（インストーラ、バンドラー）
  - CLI/サービス層で、プロファイルのロード/検証時に使用

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ProfileManifest | struct ProfileManifest { pub name: String, pub version: String, #[serde(default)] pub provider: Option<String>, #[serde(default)] pub files: Vec<String> } | マニフェストデータ保持 | O(1) | O(n)（フィールドサイズに依存） |
| provider_name | pub fn provider_name(&self) -> &str | provider未指定時はnameを返す | O(1) | O(1) |
| from_file | pub fn from_file(path: &Path) -> ProfileResult<Self> | JSONファイルを読み込み、from_jsonに委譲 | O(n)（ファイル長） | O(n) |
| from_json | pub fn from_json(json: &str) -> ProfileResult<Self> | JSON文字列をパース、空ファイル除去、検証 | O(n)（JSON長）+O(m)（files長） | O(n) |

データ契約（構造体フィールドの意味と制約）
- name: 必須・非空。validateで空はエラー
- version: 必須・非空。validateで空はエラー。フォーマット制約はこのチャンクでは未検証
- provider: オプション。未指定時は None。provider_nameがnameを返すフォールバックあり
- files: 省略可能（#[serde(default)]）。未指定なら空Vec。from_jsonで空文字列は削除

詳細説明

1) ProfileManifest（構造体）
- 目的と責務
  - プロファイルの基本メタデータを保持
- アルゴリズム
  - 該当なし（データ構造）
- 引数
  - コンストラクタは定義なし（serde経由で生成）
- 戻り値
  - 該当なし
- 使用例
```rust
use profiles::manifest::ProfileManifest;

let manifest = ProfileManifest {
    name: "my-profile".into(),
    version: "1.2.3".into(),
    provider: None,
    files: vec!["bin/tool".into(), "config/settings.toml".into()],
};
```
- エッジケース
  - name/version が空だと validateでエラー
  - provider 未指定でも provider_name が name を返す

2) provider_name(&self) -> &str
- 目的と責務
  - provider未指定時のフォールバックロジック
- アルゴリズム（ステップ）
  - Option<String> を as_deref で Option<&str> に変換
  - unwrap_or(&self.name) でフォールバック
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | self | &Self | マニフェスト参照 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | &str | providerがSomeならその値、Noneならname |
- 使用例
```rust
let m = ProfileManifest { 
    name: "profileA".into(), 
    version: "0.1.0".into(), 
    provider: None, 
    files: vec![]
};
assert_eq!(m.provider_name(), "profileA");
```
- エッジケース
  - provider: Some("") の場合、空文字列を返す（フォールバックしない）

3) from_file(&Path) -> ProfileResult<Self>
- 目的と責務
  - ファイルからJSON文字列を読み込み、from_jsonに委譲
- アルゴリズム（ステップ）
  - std::fs::read_to_string(path) でUTF-8文字列を読み込み
  - Self::from_json(&content) を呼び出し
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | path | &Path | JSONファイルへのパス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | ProfileResult<Self> | 成功時はProfileManifest、失敗時はProfileError |
- 使用例
```rust
use std::path::Path;
let manifest = ProfileManifest::from_file(Path::new("profiles/my/profile.json"))?;
```
- エッジケース
  - 非UTF-8ファイル→読み込みエラー
  - ファイル不存在→I/Oエラー
  - JSON不正→from_jsonでエラー

4) from_json(&str) -> ProfileResult<Self>
- 目的と責務
  - JSON文字列からのパース、追加の正規化（空ファイルの除去）、検証
- アルゴリズム（ステップ）
  - serde_json::from_str(json) でデシリアライズ
  - files.retain(|f| !f.is_empty()) で空文字列の削除
  - validate() で name/version 非空チェック
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | json | &str | JSON文字列 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | ProfileResult<Self> | 成功時は正規化済みManifest、失敗時はProfileError |
- 使用例
```rust
let json = r#"{
  "name": "core",
  "version": "1.0.0",
  "files": ["bin/core", ""]
}"#;
let manifest = ProfileManifest::from_json(json)?;
assert_eq!(manifest.files, vec!["bin/core"]);
```
- エッジケース
  - filesが未指定→空Vecに（#[serde(default)]）
  - provider未指定→None、provider_nameはnameを返す
  - JSONに未知フィールドが含まれる場合の挙動は不明（このチャンクでは未制御）
  - filesに空文字列→除去される

内部API（非公開）
- validate(&self) -> ProfileResult<()>
  - name/version が空なら InvalidManifest を返す
  - 他の検証なし

## Walkthrough & Data Flow

- from_file
  - 入力: Path
  - 処理: read_to_stringでファイルを読み込み → from_json に委譲
  - 出力: ProfileManifest またはエラー
- from_json
  - 入力: JSON文字列
  - 処理:
    1) serde_json::from_strで構造体にデシリアライズ
    2) filesベクタから空文字列要素を削除
    3) validateで name/version の空チェック
  - 出力: 正規化・検証済み ProfileManifest またはエラー
- provider_name
  - 入力: &self
  - 処理: providerがSomeならその文字列、Noneならname
  - 出力: &str

この処理は直線的で、分岐は validate 内の2箇所（name空、version空）のみです。Mermaid図の使用基準（分岐4以上など）に該当しないため図は省略します。

## Complexity & Performance

- from_file
  - 時間計算量: O(n)（ファイルサイズnの読み込み） + O(n)（JSONパース）
  - 空間計算量: O(n)（文字列内容、構造体フィールド）
  - ボトルネック: ファイルI/OとJSONパース
- from_json
  - 時間計算量: O(n)（JSONパース） + O(m)（filesの要素数mのフィルタ）
  - 空間計算量: O(n)
- provider_name
  - 時間/空間: O(1)

実運用負荷要因
- 大きなJSON・大量のfiles配列はパースとフィルタでCPU/メモリ負荷増
- ネットワーク/DBは不使用。本チャンクは純粋にファイルI/Oとメモリ操作

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価

- メモリ安全性
  - unsafe未使用。所有権/借用は明快（provider_nameは&selfの借用のみ）。Buffer overflow/Use-after-free/Integer overflowの懸念は無し
- インジェクション
  - SQL/Commandインジェクション該当なし
  - Path traversal: from_fileは与えられたPathをそのまま読むため、呼び出し元がユーザ入力を直接渡すと任意ファイル読み込みのリスクあり。対策として**ディレクトリ制限/正規化/拒否リスト（..）**が必要
- 認証・認可
  - 未実装。必要に応じて呼び出し側で権限チェック
- 秘密情報
  - ログ出力なし。Hard-coded secretsなし。エラー詳細に機微情報が含まれる可能性は低いが、上位でログに出す際は注意
- 並行性
  - 共有状態なし。Race/Deadlockの懸念なし
- エラー設計
  - `?`によりI/O/JSONエラーがProfileErrorへ変換される前提（この前提の詳細は他チャンクに依存）。`InvalidManifest`が検証エラーに使われる

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空name | `{"name":"", "version":"1"}` | Err(InvalidManifest) | validate | 対応済み |
| 空version | `{"name":"p", "version":""}` | Err(InvalidManifest) | validate | 対応済み |
| provider未指定 | `{"name":"p","version":"1"}` | provider_name()は"name"返却 | provider_name | 対応済み |
| files未指定 | `{"name":"p","version":"1"}` | filesは空Vec | #[serde(default)] | 対応済み |
| filesに空文字 | `{"name":"p","version":"1","files":["", "a"]}` | 空要素を除去し["a"] | from_json retain | 対応済み |
| 非UTF-8ファイル | バイナリ | Err(I/O) | from_file | 対応済み |
| ファイル不存在 | 不正パス | Err(I/O) | from_file | 対応済み |
| JSON不正 | `{` | Err(JSON) | from_json | 対応済み |
| 未知フィールド | `{"name":"p","version":"1","x":1}` | 挙動不明（エラーになる可能性） | serde派生 | 不明 |
| 絶対/親参照パス | `files:["../../etc/passwd"]` | 許可/拒否のポリシー不明 | 未検証 | 要対策 |

Rust特有の観点（詳細チェック）

- 所有権
  - from_jsonは新規所有のProfileManifestを返す。移動/クローンの問題なし（関数: from_json）
- 借用
  - provider_nameは&selfから&strを返し、selfの寿命に束縛される。可変借用なし（関数: provider_name）
- ライフタイム
  - 明示的ライフタイム不要。返り値はselfに束縛
- unsafe境界
  - unsafeブロック無し
- Send/Sync
  - String/Vec<String>のみで、標準的にはSend+Syncを満たす。特殊な非Sync要素なし
- await境界/非同期
  - 非同期未使用。同期I/O
- キャンセル
  - 該当なし
- エラー設計
  - Resultの使用。panicにつながるunwrap/expect未使用。Error変換（From/Into）は他チャンク実装に依存（不明）

## Design & Architecture Suggestions

- 入力検証強化
  - **ファイルパス検証**: `files` 内の値が相対パスで、`..` や絶対パスを含まないことをチェック（正規化・拒否）
  - **versionのセマンティック検証**: `semver` 的な形式を検証
  - **providerの形式検証**: 空文字や不正文字を拒否
- 不変条件の明文化
  - `#[serde(deny_unknown_fields)]` の導入で未知フィールドを拒否（必要なら）
  - validateを**公開**して、上位が手動作成した構造体にも適用可能に
- API拡張
  - `TryFrom<&str> for ProfileManifest` 実装でfrom_jsonと同等のエルゴノミクス
  - `load_from_dir(&Path)` のようなヘルパーを追加し、固定パス `profiles/{name}/profile.json` を組み立てて読み込む
- エラー詳細
  - InvalidManifestの理由に**どのフィールドが不正か**を含める（複数エラー収集も検討）
- セキュリティ
  - 呼び出し側で**ベースディレクトリの制限**（沙箱化）を義務化
  - `files` のパスには**正規化（canonicalize不可の場合は独自検証）**を適用

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト観点
  - provider_nameのフォールバック
  - from_jsonの空filesフィルタ
  - validateのname/version空チェック
  - from_fileのI/Oエラー伝播

- テスト例（ユニット）
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn provider_name_falls_back_to_name() {
        let m = ProfileManifest {
            name: "core".into(),
            version: "1.0.0".into(),
            provider: None,
            files: vec![],
        };
        assert_eq!(m.provider_name(), "core");
    }

    #[test]
    fn from_json_filters_empty_files() {
        let json = r#"{
            "name": "core",
            "version": "1.0.0",
            "files": ["bin/core", ""]
        }"#;
        let m = ProfileManifest::from_json(json).unwrap();
        assert_eq!(m.files, vec!["bin/core"]);
    }

    #[test]
    fn validate_errors_on_empty_fields() {
        let bad_json1 = r#"{"name": "", "version": "1"}"#;
        assert!(ProfileManifest::from_json(bad_json1).is_err());

        let bad_json2 = r#"{"name": "p", "version": ""}"#;
        assert!(ProfileManifest::from_json(bad_json2).is_err());
    }

    #[test]
    fn from_file_propagates_io_error() {
        // 存在しないパス
        let res = ProfileManifest::from_file(Path::new("no/such/file.json"));
        assert!(res.is_err());
    }
}
```

- 統合テスト観点
  - 実際の `profiles/{name}/profile.json` ファイルを準備し、読み込みから検証まで通す
  - `files` のパスを使う上位処理（インストールフェーズ）と結合して安全性確認

## Refactoring Plan & Best Practices

- validateの拡張
  - version形式（例: `X.Y.Z`）をチェック
  - provider空文字拒否
  - filesの各要素に**ホワイトリスト**（相対パス、`/`禁止、`..`禁止など）
- API改善
  - `pub fn validate(&self) -> ProfileResult<()>` にして外部からも呼べるようにする
  - `impl TryFrom<&str> for ProfileManifest` 実装で自然なパース
- エラーハンドリング
  - エラー型に**詳細なコンテキスト**（フィールド名、値）を付加
- serde戦略
  - `#[serde(deny_unknown_fields)]` の追加でマニフェストのスキーマを厳格化（必要に応じ）
- ドキュメント
  - マニフェストの**スキーマ仕様**（必須/任意、制約、例）をREADMEへ明記

## Observability (Logging, Metrics, Tracing)

- 現状ログ/メトリクスなし
- 推奨
  - **tracing** クレートで
    - from_file開始/終了、エラー時の`path`タグを記録
    - from_json失敗時の**要約**（フィールド名と原因）を記録（実データは必要最小限）
  - メトリクス
    - パース成功/失敗カウント
    - validate失敗の種類別カウント
  - トレース
    - 上位処理で「プロファイル読み込み→検証→インストール」までのスパンを関連付け

## Risks & Unknowns

- ProfileError/ProfileResultの詳細不明
  - `?`でio/jsonエラーが変換可能な設計である前提（このチャンクでは定義なし）
- serdeの未知フィールド取り扱い
  - デフォルトでは**未知フィールドでエラー**になる可能性があるが、このチャンクでは方針不明
- filesの安全性
  - 絶対パスや親ディレクトリ参照、シンボリックリンクの扱いは未定義
- バージョン表記の意味
  - セマンティックバージョンである前提かどうか不明
- ローカライズ/国際化
  - エラーメッセージは英語固定。多言語対応方針不明

以上の点を踏まえ、現状は**最小限の読み込み・検証**として堅実ですが、運用環境では**パス安全性**と**スキーマ厳格化**の強化が推奨されます。