# parsers\gdscript\test_resolution.rs Review

## TL;DR

- 目的: GDScript向けのシンボル解決コンテキストと継承リゾルバの基本動作をテストし、スコープの出入りとメソッド解決が正しく機能するか検証。
- 主要API（外部）: GdscriptResolutionContext（new, add_symbol, resolve, enter_scope, exit_scope）、GdscriptBehavior（create_resolution_context）、GdscriptInheritanceResolver（add_inheritance, add_type_methods, is_subtype, resolve_method, get_all_methods）。
- 複雑箇所: スコープ階層（Module→Class→Function）の解決優先とスコープレベル指定の整合性、継承チェーンのメソッド解決。
- 重大リスク: unwrapによるパニック、add_symbolのScopeLevelと現在スコープの矛盾（例：ClassスコープでModuleレベルの追加）、スコープ過剰exitの未検証。
- パフォーマンス推定: 名前解決はハッシュマップなら平均O(1)、継承チェーン探索はO(depth)。
- 不明点: 内部実装（データ構造・アルゴリズム）、スレッド安全性、エラー型詳細、行番号はこのチャンクでは不明。

## Overview & Purpose

このファイルはRustのユニットテストで、GDScript用パーサコンポーネントの「名前解決コンテキスト」と「継承リゾルバ」の基本的な振る舞いを検証します。

- test_gdscript_resolution_context_basic: スコープの入れ子（Module→Class→Function）とシンボルの追加・解決・スコープ退出時の可視性を確認。
- test_gdscript_behavior_produces_context: LanguageBehavior実装（GdscriptBehavior）が正しく解決コンテキストを生成することを確認。
- test_gdscript_inheritance_resolver: 継承リゾルバのサブタイプ判定、メソッド探索、メソッド集合の集約挙動を確認。

本ファイル自体は公開APIを定義せず、外部のcodannaクレートのAPIを使用してテストする役割です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function (test) | test_gdscript_resolution_context_basic | private | スコープ遷移とシンボル解決の検証 | Med |
| Function (test) | test_gdscript_behavior_produces_context | private | Behaviorからのコンテキスト生成の検証 | Low |
| Function (test) | test_gdscript_inheritance_resolver | private | 継承チェーンとメソッド解決の検証 | Med |

### Dependencies & Interactions

- 内部依存（このファイル内の呼び出し関係）
  - なし（各テストは独立）

- 外部依存（使用クレート・モジュール）
  | モジュール | 対象API/型 | 目的 |
  |-----------|------------|------|
  | codanna::parsing::gdscript | GdscriptBehavior, GdscriptInheritanceResolver, GdscriptResolutionContext | GDScript言語向けの振る舞い・継承解決・名前解決コンテキスト |
  | codanna::parsing | InheritanceResolver, LanguageBehavior, ResolutionScope, ScopeLevel, ScopeType | 共通インタフェースとスコープのモデル |
  | codanna | FileId, SymbolId | ファイルID・シンボルIDの型 |

- 被依存推定（このモジュールを使用する可能性のある箇所）
  - テストランナー（cargo test）
  - GDScriptパーサ全体の回帰テストスイート
  - 名前解決と継承ロジックのリファクタリング時の検証

## API Surface (Public/Exported) and Data Contracts

このファイル自身に公開APIはありません。以下はテストで使用された外部APIの一覧（推定シグネチャを含む）です。

| API名 | シグネチャ（推定/使用から推論） | 目的 | Time | Space |
|-------|----------------------------------|------|------|-------|
| GdscriptResolutionContext::new | fn new(file_id: FileId) -> Self | ファイル単位の解決コンテキスト生成 | O(1) | O(1) |
| GdscriptResolutionContext::add_symbol | fn add_symbol(name: String, id: SymbolId, level: ScopeLevel) | シンボル登録 | O(1) | O(1) |
| GdscriptResolutionContext::resolve | fn resolve(name: &str) -> Option<SymbolId> | 名前からID解決 | 平均O(1) | O(1) |
| GdscriptResolutionContext::enter_scope | fn enter_scope(scope: ScopeType) | 新たなスコープに入る | O(1) | O(1) |
| GdscriptResolutionContext::exit_scope | fn exit_scope() | 現在スコープを出る | O(1) | O(1) |
| GdscriptBehavior::new | fn new() -> Self | 言語振る舞いの初期化 | O(1) | O(1) |
| GdscriptBehavior::create_resolution_context | fn create_resolution_context(file_id: FileId) -> GdscriptResolutionContext | コンテキスト生成ファクトリ | O(1) | O(1) |
| GdscriptInheritanceResolver::new | fn new() -> Self | 継承リゾルバ初期化 | O(1) | O(1) |
| GdscriptInheritanceResolver::add_inheritance | fn add_inheritance(child: String, parent: String, kind: &str) | 継承関係登録 | O(1) | O(1) |
| GdscriptInheritanceResolver::add_type_methods | fn add_type_methods(ty: String, methods: Vec<String>) | 型にメソッド集合を追加 | O(m) | O(m) |
| GdscriptInheritanceResolver::is_subtype | fn is_subtype(child: &str, parent: &str) -> bool | サブタイプ判定 | O(depth) | O(1) |
| GdscriptInheritanceResolver::resolve_method | fn resolve_method(ty: &str, method: &str) -> Option<String> | メソッドが定義される型の解決 | O(depth) | O(1) |
| GdscriptInheritanceResolver::get_all_methods | fn get_all_methods(ty: &str) -> Vec<String> | 継承チェーン上の全メソッド集約 | O(total_m) | O(total_m) |
| FileId::new | fn new(u64) -> Option<FileId>/Result | FileId生成 | O(1) | O(1) |
| SymbolId::new | fn new(u64) -> Option<SymbolId>/Result | SymbolId生成 | O(1) | O(1) |
| ScopeType::Class | const/associated | クラススコープ種別 | O(1) | O(1) |
| ScopeType::function | fn function() -> ScopeType | 関数スコープ種別 | O(1) | O(1) |
| ScopeLevel::{Module, Local} | enum variants | シンボルのスコープレベル指定 | O(1) | O(1) |

詳細（主なAPI）:

1) GdscriptResolutionContext::add_symbol
- 目的と責務: 指定名・ID・レベルでシンボル表に登録し、後のresolveに利用可能にする。
- アルゴリズム（推定）:
  1. 現在スコープスタックと引数levelを参照。
  2. 対応するシンボルテーブル（例：HashMap<String, SymbolId>）に挿入。
  3. 同名が存在する場合は上書きまたはシャドウイングルール適用（不明）。
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | name | String | シンボル名 |
  | id | SymbolId | シンボルID |
  | level | ScopeLevel | 登録先のスコープレベル |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | ()/Result | エラーは不明（テストでは戻り値未使用） |
- 使用例:
  ```rust
  let move_id = SymbolId::new(12).unwrap();
  context.add_symbol("move".into(), move_id, ScopeLevel::Module);
  assert_eq!(context.resolve("move"), Some(move_id));
  ```
- エッジケース:
  - 同名の重複登録時の挙動（上書きかエラーか）: 不明
  - levelと現在スコープの不整合（例：Class内でModule登録）: 仕様不明、テストでは許容

2) GdscriptResolutionContext::resolve
- 目的と責務: 現在のスコープ可視性に従って名前からSymbolIdを返す。
- アルゴリズム（推定）:
  1. 最内スコープから外側へ順に探索。
  2. 各レベルのシンボルテーブルで一致名を検索。
  3. 見つかれば返す。なければNone。
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | name | &str | 検索名 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Option<SymbolId> | 解決成功ならSome、失敗ならNone |
- 使用例:
  ```rust
  assert_eq!(context.resolve("Player"), Some(player_id));
  assert!(context.resolve("temp").is_none());
  ```
- エッジケース:
  - シャドウイング時の優先順位: 最内スコープ優先が一般的だが不明
  - 大小文字の扱い（ケースセンシティブ）: 不明

3) GdscriptResolutionContext::{enter_scope, exit_scope}
- 目的と責務: スコープスタックの管理。
- アルゴリズム（推定）:
  - enter_scope: 現在スコープスタックにScopeTypeをpush。
  - exit_scope: スタックからpop。空時のpop挙動は不明。
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | scope（enter） | ScopeType | 新規に入るスコープ種別 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | ()/Result | 失敗時のエラーは不明 |
- 使用例:
  ```rust
  context.enter_scope(ScopeType::Class);
  context.enter_scope(ScopeType::function());
  context.exit_scope(); // function
  context.exit_scope(); // class
  ```
- エッジケース:
  - スタックアンダーフロー（過剰exit）: 未テスト
  - 未対応ScopeTypeの扱い: 不明

4) GdscriptInheritanceResolver
- 目的と責務: 継承関係と型メソッドの管理・解決。
- 主なメソッドと使用例:
  ```rust
  let mut resolver = GdscriptInheritanceResolver::new();
  resolver.add_inheritance("Player".into(), "CharacterBody2D".into(), "extends");
  resolver.add_inheritance("CharacterBody2D".into(), "Node2D".into(), "extends");
  resolver.add_type_methods("CharacterBody2D".into(), vec!["physics_process".into(), "move_and_slide".into()]);
  resolver.add_type_methods("Player".into(), vec!["jump".into()]);

  assert!(resolver.is_subtype("Player", "Node2D"));
  assert_eq!(resolver.resolve_method("Player", "move_and_slide"), Some("CharacterBody2D".into()));
  let mut methods = resolver.get_all_methods("Player");
  methods.sort();
  assert_eq!(methods, vec!["jump", "move_and_slide", "physics_process"]);
  ```
- エッジケース:
  - 複数継承や循環継承の扱い: 不明
  - 同名メソッドのオーバーライド優先順位: 不明
  - kind引数（"extends"）のバリデーションと意味拡張: 不明

5) GdscriptBehavior::create_resolution_context
- 目的と責務: 言語設定に従うコンテキスト生成。
- 使用例:
  ```rust
  let behavior = GdscriptBehavior::new();
  let mut context = behavior.create_resolution_context(file_id);
  context.add_symbol("helper".into(), helper_id, ScopeLevel::Module);
  assert_eq!(context.resolve("helper"), Some(helper_id));
  ```

データ契約（推定）:
- SymbolId, FileIdは不変の識別子型。newはResult/Optionを返し、unwrapでパニック可能。
- ScopeLevel/ScopeTypeは列挙型で、解決時の可視性とスコープスタック管理の鍵。

根拠（関数名:行番号）: このチャンクでは行番号が不明のため「行番号: 不明」。関数名はテスト関数内の使用に基づく。

## Walkthrough & Data Flow

各テストの処理フロー（高レベル）:

- test_gdscript_resolution_context_basic（行番号: 不明）
  1. FileIdを生成し、GdscriptResolutionContextを初期化。
  2. Moduleレベルに"Player"を追加し、resolveで一致確認。
  3. Classスコープに入る。
  4. Class内だがModuleレベルとして"move"を追加し、resolve確認。
  5. functionスコープに入る。
  6. Localレベルに"temp"を追加し、resolve確認。
  7. functionスコープをexitし、"temp"が解決不可になることを確認。
  8. classスコープをexitし、Moduleレベルの"Player"と"move"が解決可能であることを確認。

- test_gdscript_behavior_produces_context（行番号: 不明）
  1. GdscriptBehaviorを初期化し、file_idから解決コンテキストを生成。
  2. Moduleレベルに"helper"を追加して解決確認。

- test_gdscript_inheritance_resolver（行番号: 不明）
  1. 継承チェーン Player→CharacterBody2D→Node2D を登録。
  2. それぞれの型にメソッド集合を登録。
  3. PlayerがNode2Dのサブタイプであることを確認。
  4. Playerの"move_and_slide"がCharacterBody2D由来で解決されることを確認。
  5. Playerの全メソッド集合が継承分を含めて集約されることを確認。

Mermaid（状態遷移図）
```mermaid
stateDiagram-v2
  [*] --> Module
  Module --> Class: enter_scope(Class)
  Class --> Function: enter_scope(function)
  Function --> Class: exit_scope()
  Class --> Module: exit_scope()

  Module: 可視: Player, move
  Class: 可視: Player, move, (同スコープ定義があればクラス内)
  Function: 可視: Player, move, temp(Local)
```
上記の図は`test_gdscript_resolution_context_basic`関数の主要なスコープ遷移を示す（行番号: 不明）。

## Complexity & Performance

- 名前解決（resolve）: ハッシュマップを使用している場合、平均時間O(1)、最悪O(n)（衝突/線形探索）。スコープ階層がn段の場合、内側から外側への探索でO(n)の係数が乗る可能性。
- シンボル追加（add_symbol）: O(1)（平均）。メモリはシンボル数に比例。
- 継承判定（is_subtype）・メソッド解決（resolve_method）: 継承チェーン長をdepthとするとO(depth)。get_all_methodsは総メソッド数total_mに比例。
- ボトルネック:
  - 深いスコープネストや巨大なグローバルシンボル表。
  - 継承の深さが大きい場合の逐次探索。
- 実運用負荷要因:
  - 多ファイル解析時のコンテキスト数。
  - 大規模プロジェクトでのメソッド集合の集約コスト。
  - I/O・ネットワーク・DBはこのテストでは関与しない。

不明点:
- 実際の内部データ構造（HashMapか木構造か）
- キャッシュ戦略やインデックス化の有無

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性:
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全であり、このテストコードでは該当なし。
  - unwrapによるpanic: FileId::new, SymbolId::newに対するunwrapが失敗時にpanic。テストでは許容だが本番では要注意。
- インジェクション:
  - SQL/Command/Path traversal: 該当なし。
- 認証・認可:
  - 権限チェック漏れ / セッション固定: 該当なし。
- 秘密情報:
  - ハードコード秘密 / ログ漏洩: 該当なし。
- 並行性:
  - レースコンディション / デッドロック: 該当なし（非同期・並行処理を行っていない）。

Rust特有の観点:
- 所有権: 文字列nameは"Player".into()等で新規所有権を移動してadd_symbolに渡している（test関数内、行番号: 不明）。SymbolIdはCopy/Cloneか参照渡し不明だが、Option<SymbolId>を返すresolveの使用からCopy可能性あり（推定、不明）。
- 借用: resolveは&strで借用引数。返り値はOption<SymbolId>で所有権の問題なし。
- ライフタイム: 明示的ライフタイムは使用されていない。APIは所有型（String）またはCopy可能型で設計されている推定。
- unsafe境界: unsafeブロックはこのファイルには出現しない。
- 並行性・非同期: Send/Sync境界は不明。テストは単一スレッド前提。
- エラー設計:
  - Result vs Option: resolveはOptionで「見つからない」を表現。newはunwrapを要求するResult/Option返却（詳細不明）。
  - panic箇所: unwrap使用多数。テストでは妥当。
  - エラー変換: From/Intoなどの実装はこのチャンクには現れない。

エッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 未解決名 | "unknown" | Noneを返す | resolve使用 | 動作確認済み（tempの例で間接的に） |
| 同名シャドウイング | "x"がModuleとLocalに存在 | Localを優先して解決 | 仕様不明 | 未テスト |
| スコープアンダーフロー | exit_scopeが過剰に呼ばれる | エラー/無視/パニックのいずれか | 不明 | 未テスト |
| スコープ不整合登録 | Class内でModuleレベル登録 | Moduleに入るかエラーか | 本テストはModuleに入ると仮定 | テストで確認済み |
| 循環継承 | A→B→A | is_subtypeが無限ループしない | 不明 | 未テスト |
| メソッド重複 | 親と子に同名メソッド | 子を優先/両方保持 | 不明 | 未テスト |
| 大文字小文字 | "Move" vs "move" | 言語仕様通りの比較 | 不明 | 未テスト |
| 空文字シンボル | "" | エラーまたは無視 | 不明 | 未テスト |

注: 「実装」「状態」の列はこのチャンクでは不明な点を明示。

## Design & Architecture Suggestions

- スコープレベルの自動付与:
  - add_symbolが現在のスコープコンテキストから適切なScopeLevelを推定できるAPIを提供すると、Class内でModule指定などの不整合を防げます（例: add_local_symbol, add_class_memberのような明確な関数）。
- スコープ管理の安全化:
  - exit_scopeがアンダーフローした場合のResult返却やpanic防止。
  - enter_scope/exit_scopeのバランスをAssert/Debugログで検証。
- 継承リゾルバの仕様明確化:
  - 複数継承・インタフェース・ミックスイン対応の有無を明確にし、resolve_methodの優先順位（最も近い祖先を優先）を仕様化。
- APIの戻り値の型設計:
  - add_symbolは重複時の挙動をResultで返すなど、エラー指向設計を検討。
- 命名と一貫性:
  - ScopeType::function() は他は定数風なので、Functionに合わせた命名一貫性を検討。

## Testing Strategy (Unit/Integration) with Examples

拡張すべきテスト観点:

- シャドウイング優先順位（Local > Class > Module）:
  ```rust
  #[test]
  fn test_shadowing_priority() {
      let file_id = FileId::new(100).unwrap();
      let mut ctx = GdscriptResolutionContext::new(file_id);

      let id_mod = SymbolId::new(1).unwrap();
      ctx.add_symbol("x".into(), id_mod, ScopeLevel::Module);

      ctx.enter_scope(ScopeType::Class);
      let id_class = SymbolId::new(2).unwrap();
      ctx.add_symbol("x".into(), id_class, ScopeLevel::Local); // クラスレベル相当なら別APIが望ましいが仮にLocal

      ctx.enter_scope(ScopeType::function());
      let id_local = SymbolId::new(3).unwrap();
      ctx.add_symbol("x".into(), id_local, ScopeLevel::Local);

      assert_eq!(ctx.resolve("x"), Some(id_local));
      ctx.exit_scope();
      assert_eq!(ctx.resolve("x"), Some(id_class));
      ctx.exit_scope();
      assert_eq!(ctx.resolve("x"), Some(id_mod));
  }
  ```
- スコープアンダーフロー:
  ```rust
  #[test]
  fn test_scope_underflow() {
      let file_id = FileId::new(101).unwrap();
      let mut ctx = GdscriptResolutionContext::new(file_id);
      // 1回余分にexitして安全に失敗/エラーを返すか確認
      ctx.exit_scope(); // 仕様不明、ここでpanicしない設計が望ましい
  }
  ```
- 循環継承と検出:
  ```rust
  #[test]
  fn test_cyclic_inheritance_detection() {
      let mut resolver = GdscriptInheritanceResolver::new();
      resolver.add_inheritance("A".into(), "B".into(), "extends");
      resolver.add_inheritance("B".into(), "A".into(), "extends");
      // 期待: is_subtypeが無限ループせずfalse/エラーとなる
      assert!(!resolver.is_subtype("A", "A")); // 仕様に応じて調整
  }
  ```
- メソッドオーバーライド優先:
  ```rust
  #[test]
  fn test_method_override_priority() {
      let mut resolver = GdscriptInheritanceResolver::new();
      resolver.add_inheritance("Child".into(), "Parent".into(), "extends");
      resolver.add_type_methods("Parent".into(), vec!["m".into()]);
      resolver.add_type_methods("Child".into(), vec!["m".into()]);
      // 期待: resolve_method("Child","m")が"Child"を返す
      assert_eq!(resolver.resolve_method("Child", "m"), Some("Child".into()));
  }
  ```

- プロパティベーステスト（proptest）導入:
  - ランダムなスコープ入れ子とシンボル挿入・削除に対し、resolveが期待通りに動く不変条件を検証。

## Refactoring Plan & Best Practices

- unwrapの置換:
  - テストでも`expect("FileId::new failed")`などの明示メッセージで原因特定を容易に。
- テストヘルパーの導入:
  - コンテキスト初期化やシンボル追加の定型処理を関数化して重複排除。
- 命名改善:
  - テスト名をより仕様駆動に（例: `resolves_module_symbols_across_class_and_function_scopes`）。
- スコープレベル指定の明確化:
  - add_symbolで現在スコープに紐づくAPIの追加（例: add_local, add_class_member）を用いて誤用防止。
- テストの独立性と順序非依存:
  - 既に独立だが、将来的にグローバル状態が導入される場合は`#[serial]`等の制御（必要時）。

## Observability (Logging, Metrics, Tracing)

- ロギング:
  - enter_scope/exit_scope時にスコープスタックの内容をDEBUGログ。
  - add_symbol時にスコープレベルと重複有無をログ。
- メトリクス:
  - resolve呼び出し回数、成功/失敗比率。
  - シンボル総数、スコープ深さの分布。
- トレーシング:
  - 継承チェーンの探索パスをspanで可視化（resolve_methodの経路）。

テストでは通常不要だが、実運用の解析時に有用。

## Risks & Unknowns

- 不明点:
  - データ構造・アルゴリズム詳細（HashMapか、スコープごとの多層マップか）。
  - エラー時の戻り値仕様（add_symbol/exit_scopeのResult有無）。
  - 継承の種類（単一継承のみか、インターフェース/ミックスイン対応か、kindの語彙）。
  - メソッド解決のオーバーライド戦略。
  - 行番号情報はこのチャンクには現れないため、厳密な位置の根拠提示は不可。

- リスク:
  - unwrapによるテストの脆弱性（外部API変更時の予期せぬパニック）。
  - スコープ指定の誤用がプロダクションコードに波及する可能性。
  - 継承探索が循環や長いチェーンで性能低下/無限ループ（未検出時）する可能性。

以上により、このテストは基本動作を有効にカバーしているものの、スコープの整合性・エッジケース・継承の複雑性について追加の検証が望まれます。