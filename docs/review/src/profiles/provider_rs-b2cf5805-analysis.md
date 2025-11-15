# profiles\provider.rs Review

## TL;DR

- 目的: **プロバイダーマニフェスト**（provider.json）の読み込み・検証・検索、および**プロファイルソースの解決**を行う小さなコアモジュール。
- 主要公開API: **ProviderManifest::from_file**, **from_json**, **validate**, **get_profile**, **ProviderProfileSource::resolve**。
- 複雑箇所: **validate**の多段チェック（名前/オーナー/プロファイル配列/各プロファイル名）、**resolve**のGitHub/Git URL解決分岐。
- 重大リスク: **パス結合がString連結**のため、**パストラバーサル**や**非正規化**の懸念。Gitソースで**URL不在**を許容し、後段で失敗する可能性。**重複プロファイル名**や**requires未解決**の未検証。
- Rust安全性: **unsafe未使用**、所有権／借用はシンプル。I/Oは`std::fs::read_to_string`のみ。並行性なし。
- エラー設計: **ProfileResult/ ProfileError**でラップ（定義はこのチャンク外）。`validate`は構造妥当性のみを保障し、フィールド間の整合性は限定的。
- パフォーマンス: 全体として**O(n)**（n=プロファイル数 or JSONサイズ）で軽量。I/O以外のボトルネックなし。

## Overview & Purpose

このファイルは、プロバイダー（プロフィールのコンテナ）からのプロフィール発見のための**マニフェスト（JSON）**を扱います。主な責務は以下です。

- プロバイダーマニフェスト（`provider.json`）の構造体定義と**デシリアライズ**。
- 読み込んだマニフェストの**基本検証**（空文字や空配列のチェック）。
- プロファイルの名前による**検索**。
- プロファイルの**ソース解決**（リポジトリ内相対パス or Git/GitHubの外部ソース）。

このモジュールは、後段の「取得・展開」処理の前段にあたり、**入力の正規化とバリデーション**に焦点を置いています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | ProviderManifest | pub | プロバイダーマニフェストの表現と読み込み/検証/検索 | Med |
| Struct | ProviderOwner | pub | オーナー情報（名前/メール/URL） | Low |
| Struct | ProviderMetadata | pub | 任意メタデータ（namespace/settingsFile/profileRoot/version） | Low |
| Struct | ProviderProfile | pub | プロファイル項目（名前/ソース/説明/version/requires等） | Med |
| Enum | ProviderProfileSource | pub | プロファイルソース（相対パス or 詳細ディスクリプタ） | Med |
| Struct | ProviderProfileSourceDescriptor | pub | Git/GitHubソースの詳細（source/repo/url/path/subdir/ref） | Med |
| Enum | ResolvedProfileSource | pub | 取得準備済みソース（ProviderPath or Git） | Med |
| Fn | ProviderManifest::from_file | pub | JSONファイルから読み込み | Low |
| Fn | ProviderManifest::from_json | pub | JSON文字列から読み込み＋検証 | Med |
| Fn | ProviderManifest::validate | pub | マニフェスト整合性の基本検証 | Med |
| Fn | ProviderManifest::get_profile | pub | 名前検索 | Low |
| Fn | ProviderProfileSource::resolve | pub | ソース記述から取得用ソースへ解決 | Med |

### Dependencies & Interactions

- 内部依存
  - **ProviderManifest::from_file** → `from_json` → `validate`
  - **ProviderManifest::get_profile** → `self.profiles.iter().find(...)`
  - **ProviderProfileSource::resolve** → `ProviderProfileSourceDescriptor`を参照し、`ResolvedProfileSource`を生成

- 外部依存（クレート/モジュール）
  | 依存 | 用途 | 備考 |
  |------|-----|------|
  | serde | `Deserialize`, `Serialize`派生 | JSONマッピング |
  | serde_json | `from_str` | 文字列→構造体のデコード |
  | std::fs | `read_to_string` | ファイル読み込み |
  | std::path::Path | 引数型 | パス型 |
  | super::error::{ProfileError, ProfileResult} | エラー/結果型 | 実体はこのチャンク外（不明） |

- 被依存推定
  - このモジュールを呼び出す上位は、**プロフィール発見/取得**機能や**CLI/サービス起動**時のプリロード処理が想定されるが、具体箇所は不明（このチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ProviderManifest::from_file | `pub fn from_file(path: &Path) -> ProfileResult<Self>` | JSONファイルから読み込み・検証 | O(F) | O(F) |
| ProviderManifest::from_json | `pub fn from_json(json: &str) -> ProfileResult<Self>` | JSON文字列から読み込み・検証 | O(J) | O(J) |
| ProviderManifest::validate | `pub fn validate(&self) -> ProfileResult<()>` | マニフェストの基本検証 | O(P) | O(1) |
| ProviderManifest::get_profile | `pub fn get_profile(&self, name: &str) -> Option<&ProviderProfile>` | 名前によるプロファイル検索 | O(P) | O(1) |
| ProviderProfileSource::resolve | `pub fn resolve(&self, provider_root: Option<&str>) -> ResolvedProfileSource` | ソース記述から取得用ソースへ正規化 | O(1) | O(1) |

ここで、F=ファイルサイズ、J=JSON文字列サイズ、P=プロファイル数。

### ProviderManifest::from_file

1. 目的と責務
   - ファイルパスから**JSON文字列**を読み込み、`from_json`でデコード・検証して`ProviderManifest`を返す。

2. アルゴリズム
   - `std::fs::read_to_string(path)`で読み込み。
   - `Self::from_json(&content)`を呼ぶ。
   - 結果を返す。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | path | `&Path` | プロバイダーマニフェストのファイルパス |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | `ProfileResult<Self>` | 成功で`ProviderManifest`、失敗で`ProfileError`（このチャンク外定義） |

5. 使用例
   ```rust
   use std::path::Path;
   use profiles::provider::ProviderManifest; // 実パスはプロジェクト構成に依存（不明）

   let path = Path::new(".codanna-profile/provider.json");
   let manifest = ProviderManifest::from_file(path)?;
   println!("Provider: {}", manifest.name);
   ```

6. エッジケース
   - ファイルが存在しない、権限がない、文字コード不正 → I/Oエラーが返る。
   - JSONが不正 → `serde_json`由来のエラー。
   - 構造的に不正（空nameなど） → `validate`由来の`ProfileError::InvalidManifest`。

### ProviderManifest::from_json

1. 目的と責務
   - JSON文字列から**デコード**し、**検証**を通した安全な`ProviderManifest`を返す。

2. アルゴリズム
   - `serde_json::from_str(json)`で構造体へデコード。
   - `manifest.validate()?`を呼び検証。
   - `Ok(manifest)`を返す。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | json | `&str` | プロバイダーマニフェストのJSON文字列 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | `ProfileResult<Self>` | 成功で`ProviderManifest`、失敗でエラー |

5. 使用例
   ```rust
   let json = r#"{"name":"example","owner":{"name":"Org"},"profiles":[{"name":"base","source":"./base"}]}"#;
   let manifest = ProviderManifest::from_json(json)?;
   ```

6. エッジケース
   - JSONフィールド名の誤り・型不一致 → デコード失敗。
   - 必須フィールドの欠落、空配列 → `validate`でエラー。

### ProviderManifest::validate

1. 目的と責務
   - マニフェストの**最低限の整合性**チェック（空文字や空配列の拒否、各プロフィール名の空文字拒否）。

2. アルゴリズム（主要ステップ）
   - `self.name.is_empty()`ならエラー。
   - `self.owner.name.is_empty()`ならエラー。
   - `self.profiles.is_empty()`ならエラー。
   - 各`profile.name.is_empty()`ならエラー。
   - 成功なら`Ok(())`。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | self | `&Self` | 対象マニフェスト |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | `ProfileResult<()>` | 成功は`Ok(())`、失敗は`ProfileError::InvalidManifest`等 |

5. 使用例
   ```rust
   let manifest = ProviderManifest::from_json(json)?;
   manifest.validate()?; // 再検証も可能
   ```

6. エッジケース
   - `profiles`内に**重複名**がある → 現状は許容（要改善）。
   - `requires`が未解決（存在しないプロファイルを参照） → 現状は許容（要改善）。
   - Gitソース記述の整合性（repo/url） → 現状は未検証（要改善）。

### ProviderManifest::get_profile

1. 目的と責務
   - 名前でプロファイルを**線形探索**し返す。

2. アルゴリズム
   - `self.profiles.iter().find(|p| p.name == name)`

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | name | `&str` | 検索するプロファイル名 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | `Option<&ProviderProfile>` | 見つかれば参照Some、なければNone |

5. 使用例
   ```rust
   if let Some(p) = manifest.get_profile("codanna") {
       println!("source: {:?}", p.source);
   }
   ```

6. エッジケース
   - 大文字小文字差異は**厳密一致**。`"Codanna"`と`"codanna"`は異なる。

### ProviderProfileSource::resolve

1. 目的と責務
   - ユーザ提供のソース記述（Path or Descriptor）を**取得用の解決済み表現**に変換。

2. アルゴリズム
   - `Path(String)`の場合:
     - `provider_root.map(|root| format!("{root}/{path}")).unwrap_or_else(|| path.clone())`
     - `ResolvedProfileSource::ProviderPath{ relative }`を返す。
   - `Descriptor(desc)`の場合:
     - `desc.source == "github"`なら`repo`から`https://github.com/{repo}.git`を生成（なければ`""`）。
     - それ以外は`desc.url.clone().unwrap_or_default()`を使用（なければ`""`）。
     - `subdir`は`desc.subdir.clone().or_else(|| desc.path.clone())`。
     - `ResolvedProfileSource::Git{ url, git_ref: desc.git_ref.clone(), subdir }`を返す。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | self | `&Self` | ソース記述 |
   | provider_root | `Option<&str>` | プロバイダリポジトリ内のルート（相対解決用、任意） |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | `ResolvedProfileSource` | 取得準備済みのソース（ProviderPath or Git） |

5. 使用例
   ```rust
   // 相対パス
   let src = ProviderProfileSource::Path("./profiles/codanna".into());
   let resolved = src.resolve(Some("profiles"));
   // => ProviderPath { relative: "profiles/./profiles/codanna" }

   // GitHubディスクリプタ
   let desc = ProviderProfileSourceDescriptor {
       source: "github".into(),
       repo: Some("codanna/profiles".into()),
       url: None,
       path: None,
       subdir: Some("profiles/codanna".into()),
       git_ref: Some("main".into()),
   };
   let resolved = ProviderProfileSource::Descriptor(desc).resolve(None);
   // => Git { url: "https://github.com/codanna/profiles.git", git_ref: Some("main"), subdir: Some("profiles/codanna") }
   ```

6. エッジケース
   - `source == "github"`かつ`repo == None` → `url`が空文字。後続フェッチで失敗する可能性。
   - `source != "github"`かつ`url == None` → 同様に`url`が空文字。
   - `subdir`未指定かつ`path`未指定 → `subdir`は`None`。
   - `provider_root`と`path`のString連結により`".."`を含む**パストラバーサル**の懸念（要対策）。

## Walkthrough & Data Flow

- ファイル読み込みフロー
  1. ユーザ（上位コード）が`ProviderManifest::from_file`へ`Path`を渡す。
  2. ファイル内容を文字列として取得（I/O）。
  3. `from_json`でデコード、`validate`で基本検証。
  4. 成功した`ProviderManifest`を返却。

- プロファイル検索フロー
  1. `get_profile(name)`で`Vec<ProviderProfile>`を線形探索。
  2. 該当要素の参照を返す。

- ソース解決フロー
  1. `ProviderProfile.source`が`Path`なら、`provider_root`と連結して`ProviderPath{relative}`。
  2. `Descriptor`なら、`source`種別により`url`組み立て（GitHubかURL）。
  3. `git_ref`と`subdir/path`を組み合わせて`Git{...}`で返す。

### Mermaid: validateの主要分岐

```mermaid
flowchart TD
    A[Start validate] --> B{self.name.is_empty()?}
    B -- Yes --> E[Err(InvalidManifest: Provider name cannot be empty)]
    B -- No --> C{self.owner.name.is_empty()?}
    C -- Yes --> F[Err(InvalidManifest: Provider owner name cannot be empty)]
    C -- No --> D{self.profiles.is_empty()?}
    D -- Yes --> G[Err(InvalidManifest: Provider must contain at least one profile)]
    D -- No --> H[for profile in profiles]
    H --> I{profile.name.is_empty()?}
    I -- Yes --> J[Err(InvalidManifest: Profile name cannot be empty)]
    I -- No --> K[Next profile or End]
    K --> L[Ok(())]
```

上記の図は`ProviderManifest::validate`関数（行番号不明：このチャンクには明示的行番号がない）の主要分岐を示す。

## Complexity & Performance

- from_file: 時間O(F)、空間O(F)（ファイルサイズFに比例）。I/O依存。
- from_json: 時間O(J)、空間O(J)（JSONサイズJに比例）。`serde_json`のデコード性能に依存。
- validate: 時間O(P)（P=プロファイル数）、空間O(1)。
- get_profile: 時間O(P)、空間O(1)。
- resolve: 時間O(1)、空間O(1)。

ボトルネック:
- 実運用では**I/O（ファイル読み込み）**と**JSONデコード**が支配的。プロファイル数が非常に多い場合は`get_profile`の線形探索がコストになるため、頻繁検索には**インデックス化**（HashMap化）を検討。

スケール限界:
- 数千〜万件のプロファイルを単一マニフェストで扱う場合、初期デコードとバリデーションは線形で伸びる。現行設計は**十分軽量**だが、検索性能要件次第で調整必要。

## Edge Cases, Bugs, and Security

- 機能安全性（メモリ/所有権）
  - すべて**安全なRust**で記述、`unsafe`未使用。所有権/借用は単純（`&self`、`clone`）。
  - 大容量ファイル読み込み時のメモリ使用量増大に注意。

- インジェクション/パストラバーサル
  - ⚠️ `ProviderProfileSource::Path`で`provider_root`と`path`を**String連結**しており、`"../"`を含む相対パスによる**ディレクトリ外参照**（パストラバーサル）の懸念。`Path::join` + ルート内検証が望ましい。

- 認証・認可
  - 本モジュールでは**認証/認可**未扱い（このチャンクには現れない）。

- 秘密情報
  - **ハードコード秘密**なし。ログ出力もなし。

- 並行性
  - 非同期/並行処理は未使用。共有可変状態なし。レース/デッドロックの懸念なし。

- 入力妥当性
  - ⚠️ `ProviderProfileSource::Descriptor`で`source == "github"`かつ`repo == None`、または`source != "github"`かつ`url == None`の場合、`url`が空文字で**後続処理が失敗**する可能性。`validate`で検出すべき。

- 整合性
  - ⚠️ **重複プロファイル名**未検出。
  - ⚠️ `requires`の参照先が**存在しない**場合も未検出。

### エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空プロバイダー名 | `"name": ""` | Err(InvalidManifest) | validate | 検出済み |
| 空オーナー名 | `"owner": {"name": ""}` | Err(InvalidManifest) | validate | 検出済み |
| プロファイル配列空 | `"profiles": []` | Err(InvalidManifest) | validate | 検出済み |
| 空プロファイル名 | `{"name": ""}` | Err(InvalidManifest) | validate | 検出済み |
| 重複プロファイル名 | `"profiles": [{"name":"a"}, {"name":"a"}]` | Err(InvalidManifest) | ? | 未対応 |
| requires未解決 | `"requires": ["missing"]` | Err(InvalidManifest) | ? | 未対応 |
| GitHubでrepo欠落 | `{"source":"github","repo":null}` | Err(InvalidManifest) or 明示的エラー | resolve→空URL | 未対応 |
| Gitでurl欠落 | `{"source":"git","url":null}` | Err(InvalidManifest) or 明示的エラー | resolve→空URL | 未対応 |
| パストラバーサル | `Path("../outside")` | ルート外拒否 | String連結 | 未対応 |
| 異常文字列 | 制御文字/不正UTF-8 | デコード失敗 | serde_json | 一部対応 |

（? はこのチャンクには現れない／未実装）

根拠（関数名:行番号不明）

- `validate`が空チェックを行う。
- `resolve`が空URLを許容するロジックを含む。

## Design & Architecture Suggestions

- **Source種別をEnum化**: `ProviderProfileSourceDescriptor.source`の文字列（"github"/"git"）を`enum SourceType { Github, Git }`へ。型安全性と分岐の明確化。
- **詳細バリデーション**:
  - Githubなら**repo必須**、Gitなら**url必須**を`validate`で強制。
  - `subdir`と`path`の意味を統一または片方に集約し、優先順位を仕様化。
  - **重複プロファイル名**の検出。
  - **requires参照整合性**（存在確認、循環依存検出は必要なら別層で）。
- **パス安全化**:
  - `Path::new(provider_root).join(path)`へ変更。
  - `canonicalize`または**ルート外検出**（前処理で`starts_with`判定）でパストラバーサル防止。
- **エラー詳細化**:
  - `InvalidManifest`に**フィールド名/原因**を含める（既にreasonあり、発生箇所の識別を拡張）。
  - `Descriptor`の不整合は**早期エラー**にする（空URLを返さない）。
- **検索高速化**:
  - `HashMap<String, ProviderProfile>`のビューを構築（必要ならキャッシュ）し、`get_profile`をO(1)に。

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト
  - 正常系のJSONパース/メタデータ確認。
  - バリデーション（空name/空profiles）エラー。
  - `get_profile`存在/非存在。
  - `resolve`のPath/GitHubケース。

- 追加すべきユニットテスト
  1. 重複プロファイル名検出（改善後）
  2. requires未解決の検出（改善後）
  3. Descriptorの不整合（GitHubでrepoなし、Gitでurlなし）
  4. パストラバーサル（`Path("../...")`）の拒否
  5. `provider_root`が空文字/末尾スラッシュ/Windowsパスの取り扱い
  6. `path`と`subdir`の競合（両方指定）時の優先ロジック

- テスト例（Descriptor不整合の検出を想定）
  ```rust
  #[test]
  fn test_validate_descriptor_requires_url_or_repo() {
      // 改善後: validateがDescriptor整合性を確認する前提
      let json = r#"{
          "name": "prov",
          "owner": {"name": "Org"},
          "profiles": [
              {"name": "p1", "source": {"source": "github", "repo": null}}
          ]
      }"#;
      let res = ProviderManifest::from_json(json);
      assert!(res.is_err(), "githubはrepo必須");
  }
  ```

- テスト例（パス安全結合を想定）
  ```rust
  #[test]
  fn test_resolve_path_prevents_traversal() {
      // 改善後: Path::join + ルート外検出を行う前提
      let src = ProviderProfileSource::Path("../outside".into());
      // 期待: エラーまたはルート内に制限
      // 現状コードではProviderPathでそのまま出るため、改善が必要
  }
  ```

## Refactoring Plan & Best Practices

- 段階的リファクタリング
  1. **型の強化**: `SourceType` enum導入、`ProviderProfileSourceDescriptor`のフィールドを`Option`から必須化（種別ごと）。
  2. **validate拡張**: 重複名、requires整合性、Descriptor必須項目チェック。
  3. **パス結合の安全化**: `Path::join`に変更し、ルート外検出ロジックを追加。
  4. **APIの明確化**: `resolve`が失敗可能なケースを`Result<ResolvedProfileSource, ProfileError>`へ変更し、空URL生成を廃止。
  5. **検索性能向上**: マニフェストロード時にインデックス化（オプション）。

- ベストプラクティス
  - **入力は早期にバリデーション**し、無効値をシステムに入れない。
  - **文字列より型で表現**（URL/Repo/Refなど）。
  - **テスト駆動**で仕様化（path vs subdir、sourceのルール）。

## Observability (Logging, Metrics, Tracing)

- 現状: ログ/メトリクス/トレースなし。
- 推奨:
  - **詳細ログ**: validate失敗時にフィールド名やインデックスを含める（ただし、このモジュールでは返却エラー詳細で十分、ログは上位層で）。
  - **メトリクス**: マニフェストサイズ、プロファイル数、validate失敗率。
  - **トレース**: 大規模システムではマニフェスト読み込みのスパンを追加。

## Risks & Unknowns

- Unknowns
  - `ProfileError`, `ProfileResult`の詳細設計（このチャンクには現れない）。
  - `ResolvedProfileSource`の後続処理（Gitクローン/抽出の実装やエラー方針）。
  - `metadata.profile_root`の利用箇所と意味（このチャンクには現れない）。

- Risks
  - **空URL**を許容する`resolve`により、後段で**遅延失敗**が発生し、エラー原因の特定が困難。
  - **パストラバーサル**による不正アクセス（ルート外ファイル）リスク。
  - **requires未検証**のため、依存関係解決時にランタイム失敗が発生する可能性。

- 緩和策
  - `validate`で構造/整合性の厳密チェック。
  - `resolve`のエラー化（Result化）とURL/サブディレクトリの厳密検証。
  - パスの正規化とルート制約。