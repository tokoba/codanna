# integration\test_typescript_alias_relationships.rs Review

## TL;DR

- 目的: **TypeScriptのインポート別名（alias）とインデックス上のmodule_pathの不一致**による解決失敗を示し、解決策（module_pathでの登録）を検証する統合テスト
- 主要API: **TypeScriptParser::find_imports**, **TypeScriptResolutionContext::add_symbol**, **TypeScriptResolutionContext::resolve**, **Symbol/SymbolId/FileId**
- 複雑箇所: インポートパス（例: "@/components/ui/button" や "./src/components/ui/button"）とインデックスの**module_path（例: "examples.typescript.react.src.components.ui.button"）のマッピング**の整合性
- 重大リスク: 文字列キーに依存した解決では**衝突/不一致**が起こりやすく、**別名・相対/絶対・tsconfigパス**等のバリエーションに弱い
- テスト戦略上の確認ポイント: デフォルトでは解決不能→**module_pathをキーに追加登録**することで解決可能（L90-L105）
- エラー要因: `unwrap`使用による**テスト時のpanic**可能性、**未使用import**（警告）、**解決アルゴリズムの詳細不明**
- 並行性/unsafe: **非同期・並行性なし**、**unsafe未使用**

## Overview & Purpose

本ファイルは、TypeScriptコードにおけるインポート別名（alias）と、インデックスに登録されたシンボルの`module_path`との関係をテストしています。

- `test_typescript_alias_resolution_for_relationships`（L7-L110）
  - 目的: インポート解析と解決コンテキストの現仕様では、**名前のみでの登録**では**module_path**や**強化済みインポートパス**による照合が失敗することを実証
  - 修正（THE FIX）: `module_path`でもシンボルを解決コンテキストに登録すると、**期待するmodule_pathで解決可能**になることを確認
- `test_typescript_import_to_module_path_mapping`（L113-L138）
  - 目的: **"./src/components/ui/button"**と**"examples.typescript.react.src.components.ui.button"**は直接一致しないことを示し、**マッピング処理の必要性**を明らかにする

このテストは、TypeScriptの`import { Button } from '@/components/ui/button'`という典型的なパスエイリアス（`@/*`→`src/*`のようなtsconfig paths）と、索引の`module_path`（プロジェクト構造由来）間にあるギャップを埋めるための設計上の示唆を与えます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_typescript_alias_resolution_for_relationships | private | インポート解析の結果と解決コンテキストの挙動を検証し、module_path登録の必要性を示す | Med |
| Function | test_typescript_import_to_module_path_mapping | private | 強化済みインポートパスとmodule_pathの非一致を明示 | Low |
| External Struct | TypeScriptParser | pub（外部） | TypeScriptコードからインポート情報の抽出 | 不明 |
| External Struct | TypeScriptResolutionContext | pub（外部） | シンボル解決（キー→SymbolId）を行う | 不明 |
| External Struct | Symbol | pub（外部） | インデックス上のシンボル表現（name, kind, file_id, range, module_path, visibility） | 不明 |
| External Struct | SymbolId | pub（外部） | シンボルを一意に識別するID | Low |
| External Struct | FileId | pub（外部） | ファイルを一意に識別するID | Low |
| External Struct | Range | pub（外部） | ソースコード上の範囲 | Low |
| External Enum | SymbolKind | pub（外部） | シンボル種別（ここではConstant） | Low |
| External Enum | Visibility | pub（外部） | 可視性（ここではPublic） | Low |
| External Enum | ScopeLevel | pub（外部） | 登録スコープ（Module/Global等） | Low |

### Dependencies & Interactions

- 内部依存（関数間の呼び出し）
  - なし（2つのテストは独立）
- 外部依存（使用クレート・モジュール）
  - 下表のとおり

| モジュール/型 | 用途 | 備考 |
|---------------|------|------|
| codanna::parsing::typescript::parser::TypeScriptParser | インポート抽出 | `find_imports`使用（L40-L44, L47-L51） |
| codanna::parsing::typescript::resolution::TypeScriptResolutionContext | シンボル解決 | `new`, `add_symbol`, `resolve`使用（L54-L63, L73-78, L93, L98-105） |
| codanna::{Symbol, SymbolId, SymbolKind, Visibility} | シンボル構築 | `Symbol::new`, `SymbolId::new`, 可視性/種別設定（L11-L20） |
| codanna::{FileId, Range} | ファイルIDと範囲設定 | `FileId::new`, `Range::new`（L16-L17, L39, L54） |
| codanna::parsing::{LanguageParser, ResolutionScope, ScopeLevel} | スコープ指定等 | `ScopeLevel`は使用（L63, L93）、他は未使用 |

- 被依存推定（このモジュールを使用する可能性のある箇所）
  - 統合テストスイート（integration tests）
  - TypeScript解析/解決の改善を検討する際の回帰テスト

## API Surface (Public/Exported) and Data Contracts

このファイル自身の公開APIはありません（テストのみ、exports=0）。以下は、テストで使用する外部API（codanna）のインタフェース状況を、コードから読み取れる範囲でまとめます。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| TypeScriptParser::new | 不明（unwrap使用） | パーサ生成 | 不明 | 不明 |
| TypeScriptParser::find_imports | `find_imports(code: &str, file_id: FileId) -> Vec<…>`（要素は`path`, `alias`, `is_glob`を持つ） | TypeScriptコードからインポート宣言を抽出 | 不明 | 不明 |
| TypeScriptResolutionContext::new | `new(file_id: FileId) -> …` | ファイル単位の解決コンテキスト生成 | 不明 | 不明 |
| TypeScriptResolutionContext::add_symbol | `add_symbol(key: String, id: SymbolId, scope: ScopeLevel)` | キー（名前やmodule_path）でシンボルIDを登録 | 不明 | 不明 |
| TypeScriptResolutionContext::resolve | `resolve(key: &str) -> Option<SymbolId>` | キーからシンボルIDを解決 | 不明 | 不明 |
| Symbol::new | `new(id: SymbolId, name: &str, kind: SymbolKind, file_id: FileId, range: Range) -> Symbol` | シンボルインスタンス生成 | O(1) | O(1) |
| SymbolId::new | `new(u64 or similar) -> Result/Option<SymbolId>`（unwrap使用から推測） | シンボルID生成 | O(1) | O(1) |
| FileId::new | `new(u64 or similar) -> Result/Option<FileId>`（unwrap使用から推測） | ファイルID生成 | O(1) | O(1) |

詳細説明（各API）

1) TypeScriptParser::find_imports
- 目的と責務
  - 与えられたTypeScriptコードから`import`文を解析し、パス・別名・グロブ指定などの情報を取り出す（L38-L44）
- アルゴリズム（ステップ分解）
  - 入力文字列をパース
  - `import`構文を収集
  - 各項目について`path`、`alias`（Option<String>）、`is_glob`（bool）を抽出
- 引数

  | 引数名 | 型 | 説明 |
  |--------|----|------|
  | code | &str | TypeScriptソースコード |
  | file_id | FileId | 解析対象ファイルID |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | Vec<…> | 各要素が`path`, `alias`, `is_glob`フィールドを持つインポート記述 |

- 使用例

  ```rust
  let mut parser = TypeScriptParser::new().unwrap();
  let file_id = FileId::new(2).unwrap();
  let imports = parser.find_imports(code, file_id);
  assert_eq!(imports.len(), 1);
  assert_eq!(imports[0].path, "@/components/ui/button");
  assert_eq!(imports[0].alias, Some("Button".to_string()));
  assert!(!imports[0].is_glob);
  ```

- エッジケース
  - 別名が存在しない（`alias == None`）
  - グロブインポート（`is_glob == true`）
  - 相対/絶対/tsconfig別名（`@/`）の混在
  - 不正な構文（パース失敗）→ このチャンクには現れない

2) TypeScriptResolutionContext::add_symbol
- 目的と責務
  - 与えられたキー（文字列）に`SymbolId`をマップする（L59-L63, L91-L94）
  - スコープ（`ScopeLevel::Module`/`Global`）指定により可視性/検索優先度を制御（詳細は不明）
- アルゴリズム（ステップ分解）
  - マップ（内部構造は不明）にキー→IDを登録
- 引数

  | 引数名 | 型 | 説明 |
  |--------|----|------|
  | key | String | シンボル解決キー（名前、module_pathなど） |
  | id | SymbolId | 登録対象のシンボルID |
  | scope | ScopeLevel | スコープ種別（Module/Globalなど） |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | () | 戻り値なし |

- 使用例

  ```rust
  // 名前で登録（Moduleスコープ）
  context.add_symbol(button_symbol.name.to_string(), button_id, ScopeLevel::Module);

  // module_pathで登録（Globalスコープ） — THE FIX
  if let Some(module_path) = &button_symbol.module_path {
      context.add_symbol(module_path.to_string(), button_id, ScopeLevel::Global);
  }
  ```

- エッジケース
  - キー重複（同一キーに複数ID）→ 実装不明
  - スコープ間競合（Module vs Global）→ 実装不明
  - 空文字キー→ 期待動作不明

3) TypeScriptResolutionContext::resolve
- 目的と責務
  - キー（名前、module_path、強化済みインポートパスなど）から`SymbolId`を検索し返す（L73-L78, L98-L105）
- アルゴリズム（ステップ分解）
  - マップからキー一致で検索
  - 見つかった場合`Some(SymbolId)`、見つからない場合`None`
- 引数

  | 引数名 | 型 | 説明 |
  |--------|----|------|
  | key | &str | 検索キー |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | Option<SymbolId> | 解決成功で`Some(id)`、失敗で`None` |

- 使用例

  ```rust
  let resolved_by_path = context.resolve("./src/components/ui/button");
  assert_eq!(resolved_by_path, None);

  let resolved_by_module = context.resolve("examples.typescript.react.src.components.ui.button");
  assert_eq!(resolved_by_module, None);

  // THE FIX 後
  let resolved_after_fix = context.resolve("examples.typescript.react.src.components.ui.button");
  assert_eq!(resolved_after_fix, Some(button_id));
  ```

- エッジケース
  - キーの正規化不足（大文字小文字、区切り、相対/絶対）→ 解決失敗
  - 複数候補一致（曖昧性）→ 実装不明

## Walkthrough & Data Flow

1. シンボル構築（L11-L20）
   - `SymbolId::new(42).unwrap()`でID生成
   - `Symbol::new(...)`で`Button`シンボル作成
   - `module_path`を設定（`examples.typescript.react.src.components.ui.button`）
   - `visibility = Public`

2. TypeScriptコードのインポート抽出（L36-L44）
   - `import { Button } from '@/components/ui/button';`
   - `find_imports`により`path="@"`形式の別名、`alias="Button"`、`is_glob=false`が得られる

3. 現状の解決コンテキスト構築と名前登録（L54-L63）
   - `TypeScriptResolutionContext::new(FileId::new(2).unwrap())`
   - `add_symbol("Button", button_id, ScopeLevel::Module)`

4. 解決試行（L69-L88）
   - 強化済みインポートパス（`"./src/components/ui/button"`）→ `None`（一致しない）
   - module_path（`"examples.typescript.react.src.components.ui.button"`）→ `None`（キー未登録）

5. 修正（THE FIX）適用（L90-L94）
   - `module_path`でも登録：`add_symbol(module_path, button_id, ScopeLevel::Global)`

6. 再解決（L96-L105）
   - module_pathでの解決が`Some(button_id)`となり成功

7. 別テスト（L113-L138）
   - 強化済みインポートパスとインデックスのmodule_pathが一致しないことを`assert_ne!`で確認
   - 解決コンテキストに**マッピング機能**が必要と指摘

データフロー要点
- 入力: TypeScriptコード文字列
- 中間: インポート記述（path/alias/is_glob）
- 解決キー: 初期は`name="Button"`のみ→ 修正後は`module_path`も登録
- 出力: `Option<SymbolId>`

## Complexity & Performance

- 本テストの処理は小規模で、時間・空間ともにほぼO(1)
- `find_imports`の内部、`resolve`の内部データ構造はこのチャンクには現れないため、**計算量は不明**
- 予想ボトルネック（一般論）
  - 大型プロジェクトではキー種別（名前、module_path、強化パス）が多様化し、**文字列正規化・ハッシュマップ登録数**が増える
  - I/Oやネットワークは本テストでは不使用

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価（このチャンクの範囲）

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし
  - 所有権/借用/ライフタイム: 標準的な所有権・借用のみで、問題なし（`module_path`参照→`to_string()`で所有権取得）
- インジェクション
  - SQL / Command / Path traversal: 直接該当なし（ただし、将来的な**パス正規化**は慎重に）
- 認証・認可
  - 権限チェック/セッション固定: 該当なし
- 秘密情報
  - ハードコード秘密/ログ漏洩: 該当なし（`println!`は情報メッセージのみ）
- 並行性
  - Race condition / Deadlock: 該当なし（単一スレッドのテスト）

詳細エッジケース（本テストの観点）

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| パス形式不一致（強化パス vs module_path） | "./src/components/ui/button" vs "examples.typescript.react.src.components.ui.button" | マッピングにより解決可能 | module_pathでの登録により解決（L90-L105） | 一部対応（THE FIX） |
| 別名なしのインポート | `import Button from '...'` | 名前/デフォルト名で解決 | このチャンクには現れない | 不明 |
| グロブインポート | `import * as UI from '...'` | `UI.Button`などで解決可能 | `is_glob`ありのケース未対応 | 不明 |
| tsconfig paths（@エイリアス） | `@/components/...` | `src/...`へ正規化 | 文字列マッピング必要と指摘（L132-L137） | 未実装 |
| キー衝突（同名異パス） | name="Button"が複数 | スコープ/優先ルールで一意化 | ScopeLevelにより制御可能性あり | 実装不明 |
| 未使用import警告 | `LanguageParser`, `ResolutionScope` | 警告回避 | 削除/`allow(unused_imports)` | 未対応 |

- panic箇所
  - `unwrap`使用（L11, L16, L36, L39, L54）: エラー発生時はpanic。テストでは許容されるが、**原因特定のため`expect`でメッセージを付ける**ことを推奨

## Design & Architecture Suggestions

- キー正規化レイヤの導入
  - **インポートパス→module_path**への正規化関数を一元化（tsconfigの`paths`や`baseUrl`対応、`@`→`src`、相対→絶対、拡張子/インデックスファイルの解決）
- 複数キー解決の統合
  - `add_symbol`を拡張し、**name**と**module_path**の双方で登録（またはキー種別付きで登録）できるAPIを提供
  - 例: `add_symbol_with_keys(id, &[Key::Name("Button"), Key::ModulePath("examples...")])`
- スコープ戦略
  - `ScopeLevel::Module`と`ScopeLevel::Global`の優先順位・検索順を定義/文書化し、**衝突時の解決ルール**を確立
- 型安全なキー
  - 文字列ベースではなく、**型付けされたキー**（`enum Key { Name(String), ModulePath(String), ImportPath(String) }`）を使い、**曖昧性/衝突**を低減
- tsconfig/プロジェクト構造の取り込み
  - 解析時に**プロジェクトルート/パスエイリアス設定**を解決コンテキストへ反映
- 失敗時の診断
  - 解決失敗時に**候補キー**や**正規化結果**を提示できる診断ログ/デバッグAPIを提供

## Testing Strategy (Unit/Integration) with Examples

- 追加ユニットテスト案
  - デフォルトインポート
    ```rust
    #[test]
    fn resolves_default_import_with_module_path_mapping() {
        let code = r#"import Button from '@/components/ui/button';"#;
        let mut parser = TypeScriptParser::new().unwrap();
        let file_id = FileId::new(3).unwrap();
        let imports = parser.find_imports(code, file_id);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].alias, None); // 仮：defaultならaliasなし

        let mut context = TypeScriptResolutionContext::new(file_id);
        // name登録だけでは失敗
        assert_eq!(context.resolve("./src/components/ui/button"), None);

        // module_path登録で成功
        context.add_symbol(
            "examples.typescript.react.src.components.ui.button".to_string(),
            SymbolId::new(100).unwrap(),
            ScopeLevel::Global,
        );
        assert_eq!(
            context.resolve("examples.typescript.react.src.components.ui.button"),
            Some(SymbolId::new(100).unwrap())
        );
    }
    ```
  - グロブインポート
    ```rust
    #[test]
    fn resolves_glob_import_member_with_module_path() {
        let code = r#"import * as UI from '@/components/ui/button';"#;
        let mut parser = TypeScriptParser::new().unwrap();
        let file_id = FileId::new(4).unwrap();
        let imports = parser.find_imports(code, file_id);
        assert_eq!(imports.len(), 1);
        assert!(imports[0].is_glob);

        let mut context = TypeScriptResolutionContext::new(file_id);
        // Buttonのmodule_pathを登録
        let button_id = SymbolId::new(200).unwrap();
        context.add_symbol(
            "examples.typescript.react.src.components.ui.button".to_string(),
            button_id,
            ScopeLevel::Global,
        );
        // 具体的なメンバー解決仕様はこのチャンクには現れないため、ここでは存在確認のみ
        assert_eq!(
            context.resolve("examples.typescript.react.src.components.ui.button"),
            Some(button_id)
        );
    }
    ```
  - tsconfigパスエイリアスの正規化
    ```rust
    #[test]
    fn normalizes_tsconfig_paths_alias_to_module_path() {
        let alias_import = "@/components/ui/button";
        let normalized = "./src/components/ui/button"; // 例：正規化関数の期待値
        assert_ne!(alias_import, "examples.typescript.react.src.components.ui.button");

        // 正規化関数 → module_path関数 が必要（このチャンクには現れない）
        // let module_path = to_module_path(normalized);
        // assert_eq!(module_path, "examples.typescript.react.src.components.ui.button");
    }
    ```

- 統合テスト案
  - 実プロジェクトディレクトリ構造を用意し、`@/*`→`src/*`のマッピングを含む**tsconfig.json**を読み込み
  - `find_imports`→正規化→`add_symbol`→`resolve`までの一連の動作を検証

## Refactoring Plan & Best Practices

- `unwrap`の見直し
  - テストでも`expect("context message")`に置換し、失敗時の診断性を向上
- 未使用importの削除
  - `LanguageParser`, `ResolutionScope`は未使用のため削除（ビルド警告低減）
- APIの拡張（利便性向上）
  - `add_symbol_by_module_path(id, module_path: &str, scope: ScopeLevel)`のショートカットを追加
  - 型付きキー導入で**曖昧性低減**
- 正規化ユーティリティの導入
  - `normalize_import_path(code_file_dir, raw_path) -> CanonicalPath`
  - `canonical_to_module_path(project_root, canonical_path) -> ModulePath`
- テストデータの共通化
  - サンプルプロジェクトパス・モジュール名を**定数**や**ヘルパー**に集約

## Observability (Logging, Metrics, Tracing)

- ログ
  - 現状は`println!`（L8, L22-25, L41-44, L57-58, L72-78, L92, L97-99, L107-109, L114, L126-127, L137）
  - 推奨: `log`または`tracing`クレートを使用し、**レベル別（info/debug/warn）**の出力と**構造化フィールド**（キー、scope、file_id）を付加
- メトリクス
  - `resolve`成功/失敗回数、正規化に要する時間、登録キー数などを計測
- トレーシング
  - 複数キー・スコープを跨ぐ解決の**スパン**を設定して可観測性を高める

## Risks & Unknowns

- 不明点
  - `TypeScriptParser::find_imports`の内部仕様（tsconfig対応、パス正規化の有無）
  - `TypeScriptResolutionContext`の内部構造（ハッシュマップ/トライ等）、`ScopeLevel`の詳細な意味論
  - 複数キー登録時の優先順位・衝突解決戦略
- リスク
  - 文字列キーのままでは**曖昧性・衝突・整合性**の問題が発生しやすい
  - tsconfigやモノレポ構成の**複雑なパス解決**に未対応だと、スケール時に解決率が低下
  - `unwrap`による**テスト早期中断**で根本原因の分析が困難

以上の通り、本テストは「名前のみの登録ではTypeScriptの実運用パターン（別名/エイリアス/強化パス）に対して不十分で、**module_pathでの登録**や**パス正規化**が必要」という知見を明確に示しています。設計面では、キー種別の型安全化・正規化レイヤ・スコープ戦略の確立が、解決の正確性と拡張性を大きく向上させます。