# plugin.rs Review

## TL;DR

- 目的: **plugin.json** を構造体で表現し、JSON読み込み・パース・厳格な検証を行う。相対パス仕様のセキュリティチェック（絶対/親ディレクトリ禁止）を含む。
- 主要公開API: **PluginManifest::from_file**, **from_json**, **validate**, **get_command_paths**, **get_agent_paths**, **PathSpec::to_vec**。
- 複雑箇所: **validate** の分岐（必須フィールド4件＋可変仕様の複数パス検証＋Hook/MCPのユニオン型）。Mermaid図で分岐整理。
- 重大リスク: 検証ルール「'./'必須」とデフォルトパス「commands/agents」（'./'なし）の不整合、Windowsのパス区切り非対応、パス検証が文字列ベース（正規化なし）である点。
- 不足API: **scripts** フィールドは検証されるが、**get_scripts_paths** の取得APIが存在しない。
- エラー/並行性: **unsafeなし**、エラーは**PluginResult**で伝播。非同期・並行処理はなし。外部エラー型（serde/IO）の変換詳細はこのチャンクでは不明。

## Overview & Purpose

このファイルは、プラグインディレクトリ配下の「.claude-plugin/plugin.json」を読み込み、Rust構造体（PluginManifest）へデシリアライズし、内容の妥当性を検証するためのコアロジックを提供する。特に、パス仕様（commands/agents/scripts、hooks、mcpServers）について、以下のセキュリティ・整合性ルールを適用する。

- 空文字の拒否（name, version, description, author.name）
- 相対パスのみ許容（絶対パス禁止）
- パス先頭に「./」必須（ユーザー指定のパス）
- 親ディレクトリ参照「..」禁止

加えて、デフォルトディレクトリ（commands, agents）を含めた実際の探索パスを返すAPIも提供する。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | PluginManifest | pub | plugin.jsonのスキーマ表現、読み込み・パース・検証・パス取得 | Med |
| Struct | PluginAuthor | pub | 著者情報のスキーマ表現 | Low |
| Enum | PathSpec | pub | パス指定（単一/複数）のユニオン型 | Low |
| Enum | HookSpec | pub | フック設定（パス/インライン）のユニオン型 | Low |
| Enum | McpServerSpec | pub | MCP設定（パス/インライン）のユニオン型 | Low |
| Impl | PluginManifest::from_file | pub | JSONファイル読み込み＋パース | Low |
| Impl | PluginManifest::from_json | pub | JSON文字列パース＋検証 | Med |
| Impl | PluginManifest::validate | pub | マニフェストの妥当性検証（必須フィールド・パス仕様） | Med |
| Impl | PluginManifest::get_command_paths | pub | デフォルト＋追加のコマンドパス一覧 | Low |
| Impl | PluginManifest::get_agent_paths | pub | デフォルト＋追加のエージェントパス一覧 | Low |
| Impl | PathSpec::to_vec | pub | ユニオン型をVec<String>へ変換 | Low |

### Dependencies & Interactions

- 内部依存
  - PluginManifest::from_file → from_json → validate
  - validate → validate_path_spec（private）→ validate_relative_path（private）
  - get_command_paths / get_agent_paths は検証済みフィールドを集約（ただしデフォルトディレクトリは検証対象外）
- 外部依存（表）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | super::error::PluginError | エラー型 | 仕様詳細は不明（このチャンクには現れない） |
  | super::error::PluginResult | 結果型 | おそらく Result<T, PluginError> |
  | serde::{Deserialize, Serialize} | JSONシリアライズ/デシリアライズ | 典型的 |
  | serde_json::{from_str, Value} | JSONパースとインライン設定の保持 | 典型的 |
  | std::fs::read_to_string | ファイル読み込み | I/O |
  | std::path::Path | 絶対パス判定 | OS依存の挙動注意 |
- 被依存推定
  - プラグインローダやレジストリがこのモジュールの**from_file**, **from_json**, **validate**を用いてマニフェストを構築・検証し、**get_*_paths**で探索ディレクトリを決定する可能性が高い。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PluginManifest::from_file | fn from_file(path: &Path) -> PluginResult<Self> | JSONファイルからマニフェストを構築 | O(n) | O(n) |
| PluginManifest::from_json | fn from_json(json: &str) -> PluginResult<Self> | JSON文字列から構築し検証 | O(n) | O(n) |
| PluginManifest::validate | fn validate(&self) -> PluginResult<()> | 必須項目・パス仕様の検証 | O(k) | O(1) |
| PluginManifest::get_command_paths | fn get_command_paths(&self) -> Vec<String> | コマンド探索パス一覧を返す | O(k) | O(k) |
| PluginManifest::get_agent_paths | fn get_agent_paths(&self) -> Vec<String> | エージェント探索パス一覧を返す | O(k) | O(k) |
| PathSpec::to_vec | fn to_vec(&self) -> Vec<String> | 単一/複数パスをベクタ化 | O(k) | O(k) |

kはパス数、nはJSON文字列長。SpaceのO(n)はパース後の文字列保持による。

データ契約（plugin.jsonの主フィールド）:
- 必須: name, version, description, author.name
- 任意: repository, license, keywords[], commands(PathSpec), agents(PathSpec), scripts(PathSpec), hooks(HookSpec), mcpServers(McpServerSpec)
- PathSpec: "Single(String)" または "Multiple(Vec<String>)"
- HookSpec/McpServerSpec: "Path(String)" または "Inline(serde_json::Value)"
- パスの検証ポリシー（ユーザー指定分）:
  - 空文字不可
  - 絶対パス不可
  - "./"で始まる必要あり
  - ".."含有不可
- デフォルト探索ディレクトリ: "commands", "agents"（注意: "./"なし）

### PluginManifest::from_file

1. 目的と責務
   - 指定パスからJSONファイルを読み込み、**from_json**でパースと検証を行う。
2. アルゴリズム
   - read_to_stringでファイル読み込み
   - from_jsonに委譲
3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | path | &Path | plugin.jsonのファイルパス |
4. 戻り値
   | 型 | 説明 |
   |----|------|
   | PluginResult<PluginManifest> | 成功時はマニフェスト、失敗時はPluginError |
5. 使用例
   ```rust
   use std::path::Path;
   let path = Path::new("./.claude-plugin/plugin.json");
   let manifest = PluginManifest::from_file(path)?;
   ```
6. エッジケース
   - ファイルが存在しない/権限なし → IOエラーでErr
   - 読み込んだJSONが不正 → serde_jsonエラーでErr
   - 検証失敗 → PluginError::InvalidPluginManifestでErr

### PluginManifest::from_json

1. 目的と責務
   - JSON文字列から構造体へデシリアライズし、**validate**で整合性チェック。
2. アルゴリズム
   - serde_json::from_str
   - validate呼び出し
3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | json | &str | マニフェストJSON文字列 |
4. 戻り値
   | 型 | 説明 |
   |----|------|
   | PluginResult<PluginManifest> | 成功時は検証済みマニフェスト |
5. 使用例
   ```rust
   let json = r#"{
     "name": "test-plugin",
     "version": "1.0.0",
     "description": "A test plugin",
     "author": { "name": "Author" }
   }"#;
   let manifest = PluginManifest::from_json(json)?;
   ```
6. エッジケース
   - 必須フィールドが空 → InvalidPluginManifest
   - パス仕様がルール違反 → InvalidPluginManifest

### PluginManifest::validate

1. 目的と責務
   - マニフェストの必須フィールドとパス仕様の整合性を検証し、安全な相対パスのみを許可。
2. アルゴリズム
   - name/version/description/author.nameの空文字チェック
   - commands/agents/scriptsのPathSpecを**validate_path_spec**
   - hooksがPathの場合**validate_relative_path**
   - mcpServersがPathの場合**validate_relative_path**
3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &PluginManifest | 対象マニフェスト |
4. 戻り値
   | 型 | 説明 |
   |----|------|
   | PluginResult<()> | 検証成功ならOk、失敗ならErr |
5. 使用例
   ```rust
   let manifest = PluginManifest::from_json(json)?;
   manifest.validate()?; // 再検証（from_json内で既に実施）
   ```
6. エッジケース
   - Multiple([]) の空配列 → Err
   - "./"で始まらない相対パス → Err
   - ".."を含む → Err
   - 絶対パス（Unix/Windows） → Err

### PluginManifest::get_command_paths

1. 目的と責務
   - デフォルト "commands" とマニフェスト指定の追加パスを返す。
2. アルゴリズム
   - ベクタに "commands" を追加
   - commandsがSingleならpush、Multipleならextend
3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &PluginManifest | |
4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Vec<String> | コマンド探索パス一覧 |
5. 使用例
   ```rust
   let paths = manifest.get_command_paths();
   for p in paths { println!("{}", p); }
   ```
6. エッジケース
   - デフォルト "commands" は"./"を持たないため、後段のファイル分解ロジックで扱い統一が必要（仕様不整合の可能性）。

### PluginManifest::get_agent_paths

1. 目的と責務
   - デフォルト "agents" とマニフェスト指定の追加パスを返す。
2. アルゴリズム
   - ベクタに "agents" を追加
   - agentsがSingleならpush、Multipleならextend
3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &PluginManifest | |
4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Vec<String> | エージェント探索パス一覧 |
5. 使用例
   ```rust
   let paths = manifest.get_agent_paths();
   ```
6. エッジケース
   - デフォルト "agents" は"./"を持たない（上記同様の不整合）。

### PathSpec::to_vec

1. 目的と責務
   - Single/MultipleをVec<String>へ統一。
2. アルゴリズム
   - Singleなら1要素Vec、Multipleならクローン
3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &PathSpec | |
4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Vec<String> | パス一覧 |
5. 使用例
   ```rust
   let spec = PathSpec::Multiple(vec!["./a".into(), "./b".into()]);
   let paths = spec.to_vec();
   ```
6. エッジケース
   - 空のMultipleはvalidateで弾かれるべき（to_vec自体は空Vecを返す）。

## Walkthrough & Data Flow

- 入力源
  - JSONファイル（from_file）または文字列（from_json）
- 処理フロー
  1. 読み込み（I/O）
  2. デシリアライズ（serde_json）
  3. 検証（必須フィールド→パス仕様）
  4. 利用側は get_command_paths / get_agent_paths で探索パスを取得

以下、validateの分岐を図示（条件数が多いためMermaidを使用）:

```mermaid
flowchart TD
  A[validate開始] --> B{nameが空?}
  B -- Yes --> E1[Err: name empty]
  B -- No --> C{versionが空?}
  C -- Yes --> E2[Err: version empty]
  C -- No --> D{descriptionが空?}
  D -- Yes --> E3[Err: description empty]
  D -- No --> F{author.nameが空?}
  F -- Yes --> E4[Err: author.name empty]
  F -- No --> G{commandsあり?}
  G -- Yes --> G1[validate_path_spec(commands)]
  G -- No --> H{agentsあり?}
  G1 --> H
  H -- Yes --> H1[validate_path_spec(agents)]
  H -- No --> I{scriptsあり?}
  H1 --> I
  I -- Yes --> I1[validate_path_spec(scripts)]
  I -- No --> J{hooksがPath?}
  I1 --> J
  J -- Yes --> J1[validate_relative_path(hooks)]
  J -- No --> K{mcpServersがPath?}
  J1 --> K
  K -- Yes --> K1[validate_relative_path(mcpServers)]
  K -- No --> Z[Ok]
  K1 --> Z
```

上記の図は`validate`関数の主要分岐を示す（行番号不明。このチャンクには行番号情報がない）。

## Complexity & Performance

- from_file
  - 時間: O(n)（ファイル読み込み＋パース）
  - 空間: O(n)（文字列保持＋構造体）
  - ボトルネック: I/OとJSONパース。大規模JSONではパースコスト増。
- from_json
  - 時間: O(n)
  - 空間: O(n)
- validate
  - 時間: O(k + m)（k=PathSpec内の文字列数合計、m=条件分岐数固定）
  - 空間: O(1)
- get_*_paths / PathSpec::to_vec
  - 時間: O(k)
  - 空間: O(k)（出力ベクタ）

実運用負荷要因:
- I/O（ネットワークではなくローカルFS前提）
- JSONのフィールド数・文字列長に比例したパース/検証コスト
- パス検証は文字列操作中心で軽量

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし。Rust安全な標準APIのみ使用、unsafeなし。
- インジェクション
  - SQL/Command: 該当なし。
  - Path traversal: 対策あり（絶対パス禁止、'./'必須、'..'禁止）。ただし文字列ベースで正規化しないため、区切り文字や表記揺れに注意（Windowsの".\"など）。
- 認証・認可
  - 該当なし（構成読み込みのみ）。
- 秘密情報
  - Hard-coded secrets: 該当なし。
  - Log leakage: ログ出力なし。
- 並行性
  - Race condition / Deadlock: 該当なし（非同期なし）。

Rust特有の観点（詳細チェックリスト）:
- 所有権
  - 文字列は構造体に所有され、APIは &self / 値返し。ムーブの危険はない（関数名:行番号不明）。
- 借用
  - 引数は &Path, &str を受け取り、内部で文字列を所有化。可変借用はなし。
- ライフタイム
  - 明示的ライフタイム不要。Serdeで所有型へデコード。
- unsafe境界
  - 使用箇所なし。
- Send/Sync
  - 明示的境界は導入なし。この構造は一般にSend/Syncだが、本チャンクでは宣言なし（不明）。
- 非同期/await
  - 非同期処理なし。キャンセル対応なし。
- エラー設計
  - すべてResultで返す。panic誘発のunwrap/expectは本体コードに無し（テストにはunwrapあり）。
  - 外部エラー（io::Error, serde_json::Error）の変換は?演算子で委譲。PluginErrorへのFrom/Into実装詳細は不明。

エッジケース一覧（詳細化）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空のname | `"name": ""` | Err(InvalidPluginManifest) | validate | 対応済み |
| 空のversion | `"version": ""` | Err(InvalidPluginManifest) | validate | 対応済み |
| 空のdescription | `"description": ""` | Err(InvalidPluginManifest) | validate | 対応済み |
| 空のauthor.name | `"author": {"name": ""}` | Err(InvalidPluginManifest) | validate | 対応済み |
| commandsが配列だが空 | `"commands": []` | Err(InvalidPluginManifest) | validate_path_spec | 対応済み（テスト未） |
| パスが"./"で始まらない | `"commands": "custom-commands"` | Err(InvalidPluginManifest) | validate_relative_path | 対応済み（テストあり） |
| 絶対パス | `"commands": "/abs"` or `"C:\\abs"` | Err(InvalidPluginManifest) | validate_relative_path | 対応済み（テスト未） |
| ".."含有 | `"commands": "./../escape"` | Err(InvalidPluginManifest) | validate_relative_path | 対応済み（テストあり） |
| 正規化必要な相対 | `"./a/../b"` | Err(InvalidPluginManifest) | validate_relative_path | 対応済み（文字列判定で検出） |
| Windows相対 | `".\\agents"` | 想定はErr（'./'限定） | validate_relative_path | 仕様上非対応 |
| hooks/mcpServers Inline | `"hooks": {...}` | Ok（構造未検証） | validate | 許容 |

既知/潜在バグ・仕様不整合:
- デフォルトの"commands"/"agents"は**"./"を付けていない**一方、ユーザー指定は"./"必須。パス形式が混在しうる。
- Windowsの相対パス表記（".\"）を許容しておらず、クロスプラットフォーム互換に課題。
- パス検証が文字列ベースであり、`Path::components()`による正規化/親ディレクトリ検出（ParentDir）を使っていないため、表記揺れや余計なセグメント（"./"の多重等）への耐性が低い。
- scriptsは検証対象だが、取得API（get_scripts_paths）が欠落。

## Design & Architecture Suggestions

- パス検証の正規化
  - `std::path::Path::new(path).components()`でParentDirを検出し、文字列包含より堅牢に。
  - `is_relative()`で相対判定し、`starts_with("./")`という表現依存のルールをOS非依存に再設計。
- 形式統一
  - デフォルトディレクトリにも"./"を付与するか、逆にユーザー指定で"./"強制を撤廃し、相対であればOKに緩和。内部的には`root.join(path)`で統一取扱い。
- 機能補完
  - **get_scripts_paths** を追加してscriptsも対称性を保つ。
  - Hook/MCPのInline構造に対する軽微なスキーマ検証（キー存在チェック）を検討。
- スキーマ厳格化
  - `#[serde(deny_unknown_fields)]` の導入でtypoや不要フィールドを検出。
  - `license` と `repository` の形式バリデーション（SPDX識別子、URLパース）。
- エラーメッセージ
  - 現在はreason文字列のみ。フィールド名・値のスナップショット等、診断向上。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本経路をカバー。追加すべきテスト:

- PathSpecの空配列
  ```rust
  #[test]
  fn test_reject_empty_commands_array() {
      let json = r#"{
          "name": "p", "version": "1.0.0", "description": "d",
          "author": {"name": "a"},
          "commands": []
      }"#;
      let result = PluginManifest::from_json(json);
      assert!(matches!(result, Err(PluginError::InvalidPluginManifest { .. })));
  }
  ```
- 絶対パス（Unix/Windows）
  ```rust
  #[test]
  fn test_reject_absolute_paths() {
      let unix = r#"{
          "name":"p","version":"1.0.0","description":"d","author":{"name":"a"},
          "agents": "/etc/agents"
      }"#;
      assert!(matches!(
          PluginManifest::from_json(unix),
          Err(PluginError::InvalidPluginManifest { .. })
      ));

      // Windows風（実環境差に注意）
      let windows = r#"{
          "name":"p","version":"1.0.0","description":"d","author":{"name":"a"},
          "agents": "C:\\agents"
      }"#;
      assert!(matches!(
          PluginManifest::from_json(windows),
          Err(PluginError::InvalidPluginManifest { .. })
      ));
  }
  ```
- Windows相対表記の扱い
  ```rust
  #[test]
  fn test_windows_relative_not_allowed() {
      let json = r#"{
          "name":"p","version":"1.0.0","description":"d","author":{"name":"a"},
          "commands": ".\\cmds"
      }"#;
      // 現仕様では'./'必須のためErr
      assert!(matches!(
          PluginManifest::from_json(json),
          Err(PluginError::InvalidPluginManifest { .. })
      ));
  }
  ```
- hooks/mcpServersのInline/Path
  ```rust
  #[test]
  fn test_hooks_and_mcp_path_and_inline() {
      let inline = r#"{
        "name":"p","version":"1.0.0","description":"d","author":{"name":"a"},
        "hooks": {"onLoad": []},
        "mcpServers": {"servers": []}
      }"#;
      assert!(PluginManifest::from_json(inline).is_ok());

      let path = r#"{
        "name":"p","version":"1.0.0","description":"d","author":{"name":"a"},
        "hooks": "./hooks.json",
        "mcpServers": "./.mcp.json"
      }"#;
      assert!(PluginManifest::from_json(path).is_ok());
  }
  ```
- scriptsの取得APIが追加された場合のテスト（将来拡張）
- deny_unknown_fields導入時の未知フィールド検出テスト

インテグレーションテスト:
- 実ファイル読み込み（from_file）で権限エラー、ファイル欠如、妥当なJSONの成功パス。

## Complexity & Performance

- Big-Oは前述の通り。大半が軽量な文字列検証とserdeのパースコスト。
- スケール限界:
  - 非常に大きなJSONや大量のパス指定では、パースとVecの構築に比例したコスト増。
- ボトルネック:
  - I/O（from_file）、serdeのパース。これ以外は軽微。
- 実運用負荷:
  - 多数のプラグインを起動時にまとめて読み込む場合、I/O並列化やキャッシュを検討。

## Edge Cases, Bugs, and Security

（このセクションは上記のテーブルと重複するため、要点のみ）
- 仕様不整合: デフォルトパスに'./'なし、ユーザー指定は'./'必須。
- OS依存: Windows区切りやドライブ文字の扱い。現在の仕様はUnix前提。
- 正規化不足: Path::components() を用いた親ディレクトリ検出を推奨。

## Design & Architecture Suggestions

（前述の提案を再掲＋補足）
- 相対パス許容のポリシーを**OS非依存**に再設計し、内部で**正規化**して安全性を担保。
- 文字列ルールではなく、**Path APIベース**の検証に移行。
- **get_scripts_paths** の追加。
- **deny_unknown_fields** 導入でスキーマ厳格化。
- ライブラリレベルで**error kind**を詳細化（io/serde/validationの区別）。

## Testing Strategy (Unit/Integration) with Examples

（上記に具体例あり）

## Refactoring Plan & Best Practices

- ステップ1: validate_relative_pathをPathベースに書き換え
  - `Path::is_relative()`で相対確認
  - `Path::components()`で`Component::ParentDir`検出時にErr
  - OS依存のセパレータを許容（"./"強制撤廃または内部正規化で統一）
- ステップ2: デフォルトパス形式の統一
  - "commands"/"agents" を内部的に正規化して同一規約に揃える
- ステップ3: scripts向けの取得API追加
- ステップ4: スキーマ厳格化（deny_unknown_fields）と追加バリデーション（URL, SPDX）
- ベストプラクティス
  - エラー型の粒度を上げ、ユーザー向けメッセージとログ向け詳細を分離
  - ユニットテストでOS差分ケースを網羅（cfg(target_os)利用）

## Observability (Logging, Metrics, Tracing)

- ログ: 検証失敗時にフィールド名・原因をログ（レベル: warn）。現在はErr返却のみ。
- メトリクス: 検証失敗件数、原因カテゴリ（必須フィールド/パス違反/JSONパース）。
- トレーシング: from_file→from_json→validateのスパンを付与してI/O/パース/検証時間を可視化。

## Risks & Unknowns

- PluginError / PluginResultの詳細（外部エラーのマッピング）がこのチャンクには現れないため不明。
- hooks/mcpServersのInline構造の具体的スキーマは不明。
- クロスプラットフォーム動作（Windowsパス、UTF-8以外のファイルエンコーディング）に関する仕様が不明。
- 大規模運用時のキャッシュ戦略や再読み込みポリシーは不明。