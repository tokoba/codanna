# parsing\kotlin\resolution.rs Review

## TL;DR

- 目的: Kotlinのスコープ解決（ローカル/クラス/コンパニオン/モジュール/インポート/グローバル）と継承解決（単一継承＋複数インターフェース）を提供する。
- 主要公開API:
  - KotlinResolutionContext::new, set_expression_types、および ResolutionScope トレイト実装（add_symbol, resolve, enter_scope, exit_scope 等）
  - KotlinInheritanceResolver::new と InheritanceResolver トレイト実装（add_inheritance, resolve_method, get_inheritance_chain, is_subtype 等）
- 複雑箇所: resolve の探索順序と修飾名処理、作用域スタック管理、継承の再帰探索。
- 重大リスク:
  - クラス内で Module スコープ追加時に module_scope にも登録され、クラスメンバがファイルスコープへ漏洩するバグ。
  - 修飾名の解決が曖昧（head が解決された場合に tail を特定クラスに紐付けず、見つからない場合でも head を返す）。
  - companion_scopes に追加される経路が存在せず、実質未使用。
- Rust安全性: unsafe なし、所有権/借用は自然。unwrap は事前条件で安全。並行性は未対応で、マルチスレッド利用時はデータ競合のリスク。
- エラー設計: ほとんどが Option 返却で、詳細なエラー区別や診断が不足。ログは set_expression_types のみ。

## Overview & Purpose

このファイルは、Kotlin 言語特有のスコープ規則と継承モデルに合わせた解決コンテキストを提供する。主に以下を目的とする。

- KotlinResolutionContext: 名前解決のための作用域スタック（ローカル、クラス、コンパニオン、モジュール、インポート、グローバル）とインポート/式タイプ情報を保持し、ResolutionScope トレイトを実装。
- KotlinInheritanceResolver: クラス/インターフェースの継承関係とメソッド定義を保持し、再帰探索でメソッド解決・継承チェーン・サブタイプ判定を提供。

これにより、パーサや解析器は Kotlin ファイル内のシンボル参照やメソッド呼び出しを適切なスコープ・継承規則に基づいて解決できる。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | KotlinResolutionContext | pub | Kotlinのスコープスタック管理と名前解決、インポートと式タイプの提供 | Med |
| Impl (trait) | ResolutionScope for KotlinResolutionContext | pub (trait経由) | シンボル登録/解決、スコープ入退出、インポートバインディング | Med |
| Struct | KotlinInheritanceResolver | pub | 親関係と型メソッド集合の保持、再帰的継承探索 | Med |
| Impl (trait) | InheritanceResolver for KotlinInheritanceResolver | pub (trait経由) | 継承追加、メソッド解決、継承チェーン取得、サブタイプ判定 | Med |
| Module | tests | private | 単体テスト（基本ケース） | Low |

### Dependencies & Interactions

- 内部依存:
  - KotlinResolutionContext:
    - add_symbol は current_local_scope_mut / current_class_scope_mut と module_scope/import_scope/global_scope を更新。
    - resolve は resolve_in_locals/classes/companions → module_scope → import_scope → global_scope の順で探索し、修飾名（.）にも対応。
    - enter_scope/exit_scope は scope_stack と各スコープスタックの push/pop。
    - register_import_binding は import_scope と import_bindings を更新。
  - KotlinInheritanceResolver:
    - resolve_method は resolve_method_recursive（cycle 検出付き）で親を辿る。
    - get_inheritance_chain は collect_chain。
    - get_all_methods は gather_methods。

- 外部依存（このチャンクに現れる型/モジュールのみ記載）:

| 依存 | 用途 | 備考 |
|------|------|------|
| crate::parsing::resolution::ImportBinding | インポートバインディング格納/照会 | フィールド exposed_name, resolved_symbol を使用 |
| crate::parsing::resolution::ResolutionScope | トレイト実装 | スコープ操作・解決API |
| crate::parsing::resolution::InheritanceResolver | トレイト実装 | 継承探索API |
| crate::parsing::{ScopeLevel, ScopeType} | スコープ種別/レベル | 具体的バリアントはこのチャンクにない箇所もあり |
| crate::{FileId, SymbolId} | ファイル/シンボルID | 詳細不明 |
| std::collections::{HashMap, HashSet} | マップ/集合 | データ構造 |

- 被依存推定:
  - Kotlin パーサ/ASTウォーカー（シンボル登録とスコープ入退出）
  - 参照解決器/インデクサ（resolve 呼び出し）
  - 型解決フェーズ（resolve_expression_type）
  - インポート解析（register_import_binding）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| KotlinResolutionContext::new | pub fn new(file_id: FileId) -> Self | 新規解決コンテキスト生成 | O(1) | O(1) |
| KotlinResolutionContext::set_expression_types | pub fn set_expression_types(&mut self, entries: HashMap<String, String>) | 式→型マップを注入 | O(n) | O(n) |
| ResolutionScope::add_symbol | fn add_symbol(&mut self, name: String, symbol_id: SymbolId, scope_level: ScopeLevel) | シンボルを適切なスコープへ追加 | O(1)平均 | O(1) |
| ResolutionScope::resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | 名前解決（ローカル→クラス→コンパニオン→モジュール→インポート→グローバル→修飾名） | O(d) | O(1) |
| ResolutionScope::enter_scope | fn enter_scope(&mut self, scope_type: ScopeType) | スコープ開始（スタックpush） | O(1) | O(1) |
| ResolutionScope::exit_scope | fn exit_scope(&mut self) | スコープ終了（スタックpop） | O(1) | O(1) |
| ResolutionScope::clear_local_scope | fn clear_local_scope(&mut self) | 直近ローカルスコープのクリア | O(m) | O(1) |
| ResolutionScope::symbols_in_scope | fn symbols_in_scope(&self) -> Vec<(String, SymbolId, ScopeLevel)> | 可視シンボル列挙 | O(S) | O(S) |
| ResolutionScope::register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | インポートの登録 | O(1)平均 | O(1) |
| ResolutionScope::import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | 登録済みインポートの参照 | O(1)平均 | O(1) |
| ResolutionScope::resolve_relationship | fn resolve_relationship(&self, _from_name: &str, to_name: &str, _kind: crate::RelationKind, _from_file: FileId) -> Option<SymbolId> | 関係先の解決（委譲） | O(d) | O(1) |
| ResolutionScope::populate_imports | fn populate_imports(&mut self, _imports: &[crate::parsing::Import]) | インポートの事前展開（Kotlinは未使用） | O(1) | O(1) |
| ResolutionScope::resolve_expression_type | fn resolve_expression_type(&self, expr: &str) -> Option<String> | 式タイプの取得 | O(1)平均 | O(1) |
| KotlinInheritanceResolver::new | pub fn new() -> Self | 継承リゾルバ生成 | O(1) | O(1) |
| InheritanceResolver::add_inheritance | fn add_inheritance(&mut self, child: String, parent: String, _kind: &str) | 親関係を追加 | O(1)平均 | O(1) |
| InheritanceResolver::add_type_methods | fn add_type_methods(&mut self, type_name: String, methods: Vec<String>) | 型のメソッド群を登録 | O(k) | O(k) |
| InheritanceResolver::resolve_method | fn resolve_method(&self, type_name: &str, method_name: &str) -> Option<String> | 継承を辿ってメソッド定義型を解決 | O(A) | O(A) |
| InheritanceResolver::get_inheritance_chain | fn get_inheritance_chain(&self, type_name: &str) -> Vec<String> | 親チェーンの収集 | O(A) | O(A) |
| InheritanceResolver::get_all_methods | fn get_all_methods(&self, type_name: &str) -> Vec<String> | 祖先含む全メソッド名 | O(T) | O(T) |
| InheritanceResolver::is_subtype | fn is_subtype(&self, child: &str, parent: &str) -> bool | サブタイプ判定（再帰） | O(A) | O(A) |

注:
- d: スコープ深さ（ローカル/クラス/コンパニオンスタック総数）
- S: 現在可視なシンボル総数
- A: 祖先数（継承階層のノード数）
- T: 祖先までのメソッド総数
- 行番号は不明のため、関数名のみで根拠を示します。

### 各APIの詳細説明

1) KotlinResolutionContext::new
- 目的と責務: 新たなファイルIDに紐づく解決コンテキストを初期化し、グローバルスコープで開始する。
- アルゴリズム: 各スコープ用の空コンテナを用意し、scope_stack に Global を積む。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | file_id | FileId | 対象ファイルのID |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Self | 初期化済みコンテキスト |
- 使用例:
  ```rust
  let mut ctx = KotlinResolutionContext::new(FileId(1));
  ```
- エッジケース:
  - なし（初期化のみ）。

2) KotlinResolutionContext::set_expression_types
- 目的と責務: パーサが推定した式タイプマッピングを解決コンテキストに注入。
- アルゴリズム: デバッグが有効なら eprintln で件数をログ出力し、expression_types を置き換える。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | entries | HashMap<String, String> | 式文字列→型名のマップ |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例:
  ```rust
  ctx.set_expression_types(HashMap::from([("this".to_string(), "MyClass".to_string())]));
  ```
- エッジケース:
  - 大量登録時のメモリ使用増加。

3) ResolutionScope::add_symbol（KotlinResolutionContext）
- 目的と責務: 指定スコープレベルに応じてシンボルを適切なマップへ追加。
- アルゴリズム（要約）:
  - Local: 直近ローカルスコープの HashMap に insert。
  - Module: scope_stack の先頭が Class なら class_scopes の末尾に insert、その後 module_scope.entry(name).or_insert(symbol_id)。
  - Package: import_scope に insert。
  - Global: global_scope に insert、module_scope.entry(name).or_insert(symbol_id)。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | name | String | シンボル名 |
  | symbol_id | SymbolId | シンボルID |
  | scope_level | ScopeLevel | 追加先のスコープレベル |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例:
  ```rust
  ctx.add_symbol("topLevel".to_string(), SymbolId(1), ScopeLevel::Module);
  ```
- エッジケース:
  - クラス内Module追加が module_scope にも入るためメンバ漏洩（バグ、詳細は下記参照）。

4) ResolutionScope::resolve（KotlinResolutionContext）
- 目的と責務: 名前をスコープ優先順に解決し、該当 SymbolId を返す。
- アルゴリズム（主要分岐）:
  - 順に探索: ローカル → クラス → コンパニオン → モジュール → インポート → グローバル。
  - 修飾名 "head.tail" は head を resolve 後、tail をクラススコープで検索（ただし head に紐付かないグローバルなクラススコープ検索という制限あり）。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | name | &str | 解決対象名（修飾名対応） |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Option<SymbolId> | 見つかれば Some(id)、なければ None |
- 使用例:
  ```rust
  assert_eq!(ctx.resolve("topLevel"), Some(SymbolId(1)));
  ```
- エッジケース:
  - 修飾名 tail 未検出時に head を返してしまう動作（不正確）。
  - companion_scopes が未実装のため、コンパニオン解決が常に未命中。

5) ResolutionScope::enter_scope / exit_scope（KotlinResolutionContext）
- 目的と責務: 作用域の開始/終了。スタック管理と対応スコープの push/pop。
- アルゴリズム:
  - enter_scope: Function/Block はローカルを push、Class は class_scopes と companion_scopes を push。
  - exit_scope: 直近 scope_type に従い pop。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | scope_type | ScopeType | 開始/終了対象のスコープ種別 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例:
  ```rust
  ctx.enter_scope(ScopeType::Class);
  ctx.exit_scope();
  ```
- エッジケース:
  - 未対応の ScopeType（このチャンクに現れない場合あり）。

6) ResolutionScope::symbols_in_scope（KotlinResolutionContext）
- 目的と責務: 直近のローカル/クラス/コンパニオンとモジュール/インポート/グローバルに存在するシンボル一覧を返す。
- 複雑度: O(S) で全マップをなめる。

7) ResolutionScope::register_import_binding / import_binding（KotlinResolutionContext）
- 目的と責務: インポートの登録と照会。resolved_symbol がある場合は import_scope に名前→ID を登録し、binding 全体も保存。
- リスク: 同名 import を insert で上書きするため、後勝ち（衝突時の方針は明記なし）。

8) ResolutionScope::resolve_expression_type（KotlinResolutionContext）
- 目的と責務: 事前登録された式タイプを返す。
- 返却: Option<String>。

9) InheritanceResolver::*（KotlinInheritanceResolver）
- add_inheritance: 子→親を登録。
- add_type_methods: 型にメソッド名集合を登録（HashSet）。
- resolve_method: visited（HashSet）で循環検出しつつ、先に自身の型メソッド→親を再帰的に探索。
- get_inheritance_chain: 親を辿ってチェーンを収集。
- get_all_methods: 自身＋親のメソッド集合を再帰収集。
- is_subtype: 親を辿る再帰で一致を確認、循環検出。

### データ契約（ImportBinding の利用）
- ImportBinding（外部型）のフィールド利用:
  - binding.exposed_name（String）: インポートで公開される名前。
  - binding.resolved_symbol（Option<SymbolId>）: 解決済みシンボルID。Some の場合 import_scope に登録。
- Clone: import_binding(&self) は cloned() を返しているため、ImportBinding は Clone 実装が前提（このチャンクでは型定義不明）。

## Walkthrough & Data Flow

- 典型フロー（クラスメンバ追加を含む）:
  ```rust
  let mut ctx = KotlinResolutionContext::new(FileId(1));
  // トップレベルに登録
  ctx.add_symbol("topLevel".to_string(), SymbolId(1), ScopeLevel::Module);

  // クラススコープへ入る
  ctx.enter_scope(ScopeType::Class);
  // クラスメンバを「Module」レベルで追加（設計上メンバ扱い）
  ctx.add_symbol("member".to_string(), SymbolId(2), ScopeLevel::Module);

  // 解決
  assert_eq!(ctx.resolve("topLevel"), Some(SymbolId(1)));
  assert_eq!(ctx.resolve("member"), Some(SymbolId(2)));

  // クラススコープから出る
  ctx.exit_scope();

  // 期待: member は可視でない（クラス外）
  // 現実: module_scope に漏れていれば Some(SymbolId(2)) になり得る（バグ）
  ```
  - 根拠: add_symbol と resolve の実装（関数名:行番号不明）。

- インポートフロー:
  ```rust
  let binding = ImportBinding { /* exposed_name/resolved_symbol 等 */ /* 実体は不明 */ };
  ctx.register_import_binding(binding);
  // ctx.import_binding("Name") で照会、ctx.resolve("Name") でも解決可能
  ```

- 継承フロー:
  ```rust
  let mut resolver = KotlinInheritanceResolver::new();
  resolver.add_inheritance("Child".to_string(), "Parent".to_string(), "extends");
  resolver.add_type_methods("Parent".to_string(), vec!["m".to_string()]);
  assert_eq!(resolver.resolve_method("Child", "m"), Some("Parent".to_string()));
  assert!(resolver.is_subtype("Child", "Parent"));
  ```

### Mermaid（resolve の主要分岐）

```mermaid
flowchart TD
  A[resolve(name)] --> B{Localsにある?}
  B -- はい --> R1[SymbolIdを返す]
  B -- いいえ --> C{Classにある?}
  C -- はい --> R2[SymbolIdを返す]
  C -- いいえ --> D{Companionにある?}
  D -- はい --> R3[SymbolIdを返す]
  D -- いいえ --> E{Moduleにある?}
  E -- はい --> R4[SymbolIdを返す]
  E -- いいえ --> F{Importにある?}
  F -- はい --> R5[SymbolIdを返す]
  F -- いいえ --> G{Globalにある?}
  G -- はい --> R6[SymbolIdを返す]
  G -- いいえ --> H{nameに'.'含む?}
  H -- いいえ --> N[Noneを返す]
  H -- はい --> I[headをresolve]
  I --> J{headが解決できる?}
  J -- いいえ --> N
  J -- はい --> K{tailがClassスコープにある?}
  K -- はい --> R7[tailのSymbolIdを返す]
  K -- いいえ --> R8[headのSymbolIdを返す]
```

上記の図は`resolve`関数の主要分岐を示す（行番号: 不明）。

## Complexity & Performance

- KotlinResolutionContext
  - add_symbol: 平均 O(1)。衝突時の entry.or_insert は既存保持。
  - resolve: O(d + 1) 程度。d はローカル/クラス/コンパニオンのスタック深さ。各マップは平均 O(1)。修飾名処理では head の解決コストが加算。
  - symbols_in_scope: O(S)。全マップの合計サイズに比例。
  - メモリ: 各スコープごとに HashMap を保持。式タイプやインポートが増えると線形増加。

- KotlinInheritanceResolver
  - resolve_method: O(A)（祖先数）。各親を一度しか訪れない visited による循環検出。
  - get_inheritance_chain: O(A)。
  - get_all_methods: O(T)（祖先のメソッド総数）。
  - is_subtype: O(A)。

- スケール限界/ボトルネック:
  - 非常に深いネストや大量シンボルで resolve が線形に悪化。
  - 修飾名の tail 探索が正規のクラス階層に紐付かないため、曖昧な探索による誤命中・不整合の可能性（正しく紐付けるためには head に対応するクラススコープを特定する必要あり）。
  - インポート衝突時の方針未定義。

- 実運用負荷要因:
  - I/O/ネットワーク/DB は不使用（純メモリ内）。ただし set_expression_types の eprintln は標準エラー出力を行う。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| クラスメンバ漏洩 | クラス内で ScopeLevel::Module 追加 | クラス外では見えない | add_symbol（Class中に module_scope.entry(...).or_insert(... )） | バグ |
| 修飾名の不整合 | "Outer.Inner" で Inner が未登録 | None かエラー（少なくとも head だけ返さない） | resolve（tail 未発見時に head の SymbolId を返す） | バグ |
| Companion未実装 | コンパニオンにメンバ追加 | companion_scopes に登録・解決される | enter_scope は Companion用のpushのみ、追加経路なし | 欠落 |
| インポート衝突 | 同 exposed_name を複数 register_import_binding | 衝突検出/警告か名前解決規則に従った選択 | insert により単純上書き | 設計不足 |
| ローカルクリア誤用 | clear_local_scope 多用 | スコープ退出時は pop、クリアは副作用に注意 | clear は最終ローカルのみに作用 | 注意事項 |
| 循環継承 | Child→Parent→Child | 無限ループ回避 | visited による検出（resolve_method_recursive / collect_chain / gather_methods / is_subtype_recursive） | 対応済み |
| シンボルシャドウ | ローカル名がグローバル名と同じ | ローカルが優先される | resolve の順序で対応 | 対応済み |
| 修飾名での無関係一致 | head が別クラスを指し、tail が現在のクラススコープに存在 | head に紐づくメンバのみ探索 | resolve_in_classes(tail) はグローバルなクラススタック探索 | バグ |
| ログ漏えい | set_expression_types の eprintln | 過度な情報出力を避ける | 件数と FileId のみ | 低リスク |

セキュリティチェックリスト:
- メモリ安全性: unsafe 不使用。unwrap は current_local_scope_mut 内で is_empty チェック後に push 済みのため安全。Buffer overflow / Use-after-free / Integer overflow の懸念なし。
- インジェクション: SQL/Command/Path なし。インジェクション面のリスクなし。
- 認証・認可: この層では扱わない。権限チェックは該当なし。
- 秘密情報: ハードコードされた秘密情報なし。ログは件数/ファイルIDのみで低リスク。
- 並行性: Mutex/RwLock 等なし。複数スレッドから同一インスタンスを操作すると Race condition の可能性。Send/Sync 境界は型に依存（本チャンクでは不明）。

Rust特有の観点:
- 所有権: &mut self を用いたミュータブル操作で整合的。current_local_scope_mut の unwrap は事前に push 済みで安全（関数名:行番号不明）。
- 借用/ライフタイム: HashMap への参照は短期借用。明示的ライフタイム不要。
- unsafe 境界: なし。
- 並行性/非同期: 非同期/await なし。共有状態保護なし。キャンセル等の概念なし。
- エラー設計: Result より Option を多用。詳細なエラー種別・診断は不足。panic は unwrap のみで安全条件付き。
- エラー変換: From/Into の実装なし。

## Design & Architecture Suggestions

- クラスメンバのスコープ扱い修正:
  - ScopeLevel::Module が「クラス内ならクラスメンバ、クラス外ならファイルスコープ」を意味しているが、クラス内で module_scope.entry(...).or_insert(...) を行わないよう分岐を変更。
  - 例:
    ```rust
    // 擬似修正案
    if matches!(self.scope_stack.last(), Some(ScopeType::Class)) {
        if let Some(scope) = self.current_class_scope_mut() {
            scope.insert(name, symbol_id);
        }
        // module_scope への登録はしない
    } else {
        self.module_scope.entry(name).or_insert(symbol_id);
    }
    ```
- 修飾名解決の厳密化:
  - head をクラス（型）として確定したうえで、その「特定クラス」に属するメンバ/ネストクラス/コンパニオンのみを探索する設計へ。
  - 現状の resolve_in_classes(tail) は全クラススコープスタックを見るため、head と無関係な一致があり得る。
- Companion の取り扱い:
  - ScopeType に Companion が存在するなら enter_scope/exit_scope と add_symbol に Companion 経路を追加。
  - コンパニオン専用の登録 API（または ScopeLevel 拡張）を用意する。
- インポート衝突ポリシー:
  - register_import_binding で既存バインディングがある場合は警告ログや衝突戦略（後勝ち/先勝ち/エラー）を明示。
- エラー設計強化:
  - resolve の失敗理由（未定義、曖昧、スコープ外など）を区別する Result<SymbolId, ResolveError> へ拡張を検討。
- 型安全な修飾名:
  - 修飾名を構造体（QualifiedName { head, tail }) として扱い、パース済みの構造を受け取る API を追加。

## Testing Strategy (Unit/Integration) with Examples

- クラスメンバ漏洩の再現テスト（現行バグ確認）:
  ```rust
  #[test]
  fn test_member_leak() {
    let mut ctx = KotlinResolutionContext::new(FileId(1));
    ctx.enter_scope(ScopeType::Class);
    ctx.add_symbol("member".to_string(), SymbolId(2), ScopeLevel::Module);
    ctx.exit_scope();
    // 期待: None。現状: Some(SymbolId(2)) になり得る
    assert_eq!(ctx.resolve("member"), None);
  }
  ```
- 修飾名の厳密化テスト（設計改善後を想定、現状は失敗が望ましい）:
  ```rust
  #[test]
  fn test_qualified_resolution_tail_missing() {
    let mut ctx = KotlinResolutionContext::new(FileId(1));
    // head のみ存在、tail は未登録
    ctx.add_symbol("Outer".to_string(), SymbolId(10), ScopeLevel::Module);
    // 現状は Some(SymbolId(10)) を返すが、望ましくは None
    assert_eq!(ctx.resolve("Outer.Inner"), None);
  }
  ```
- Companion 登録/解決テスト（API追加後）:
  ```rust
  // Companion スコープへの追加と解決の成功を検証（このチャンクには現れないため擬似例）
  ```
- インポート衝突テスト:
  ```rust
  #[test]
  fn test_import_conflict() {
    let mut ctx = KotlinResolutionContext::new(FileId(1));
    let mut b1 = ImportBinding { exposed_name: "X".to_string(), resolved_symbol: Some(SymbolId(100)) /* 他不明 */ };
    let mut b2 = ImportBinding { exposed_name: "X".to_string(), resolved_symbol: Some(SymbolId(200)) /* 他不明 */ };
    ctx.register_import_binding(b1);
    ctx.register_import_binding(b2);
    // 後勝ちになる（要仕様化）
    assert_eq!(ctx.resolve("X"), Some(SymbolId(200)));
  }
  ```
- 継承循環テスト（循環検出）:
  ```rust
  #[test]
  fn test_inheritance_cycle_detection() {
    let mut r = KotlinInheritanceResolver::new();
    r.add_inheritance("A".to_string(), "B".to_string(), "extends");
    r.add_inheritance("B".to_string(), "A".to_string(), "extends");
    assert_eq!(r.resolve_method("A", "m"), None); // m 未登録、かつ循環を安全に抜ける
    assert!(!r.is_subtype("A", "C"));
  }
  ```

## Refactoring Plan & Best Practices

- スコープ登録の明確化:
  - add_symbol を ScopeType と ScopeLevel の組み合わせで厳密に分ける。クラス内のメンバは module_scope に触らない。
- 修飾名処理の分離:
  - resolve_qualified_name(head, tail) を独立関数にし、head に紐づくクラススコープを特定してから tail を探索。
- Companion 取り扱いの追加:
  - ScopeType::Companion（このチャンクでは不明）に対応した enter/exit/add_symbol の分岐を追加。
- インポート衝突ポリシーの実装:
  - register_import_binding 内で既存衝突時の動作（警告/拒否/上書き）を設定可能に。
- エラー型導入:
  - ResolveError（NotFound, Ambiguous, ScopeLeak など）を定義して診断を容易に。
- 監査ログ拡充:
  - add_symbol/resolve にデバッグログフックを追加（グローバルフラグに連動）。

## Observability (Logging, Metrics, Tracing)

- 現状:
  - set_expression_types のみ eprintln による件数ログ（グローバルデバッグフラグで制御）。
- 提案:
  - 名前解決失敗時の軽量ログ（デバッグ時のみ）。
  - インポート衝突時の警告ログ。
  - メトリクス例:
    - 解決成功/失敗件数、平均解決深さ d、継承探索深さ A。
  - Tracing:
    - resolve の分岐通過（ローカル/クラス/インポート/グローバル/修飾名）を span で可視化（このチャンク外の infra 依存）。

## Risks & Unknowns

- 外部型の詳細不明:
  - ImportBinding, ScopeType, ScopeLevel, RelationKind, FileId, SymbolId の仕様（このチャンクには現れない）。
- コンパニオンのモデル:
  - Companion スコープの想定 API（追加方法）が存在しないため、本当の利用方法が不明。
- クラス階層とスコープの対応付け:
  - 複数のネストクラス/複数クラスの同名メンバに対する選択規則が未定義。
- 並行利用:
  - スレッドセーフ設計か不明。現在の &mut self ベース API はシングルスレッド前提。

以上により、現在の実装は基本的な解決/継承探索を提供するが、Kotlin の細部（コンパニオン、厳密な修飾名、クラスメンバの漏洩防止）に関しては改善が必要。適切なリファクタとテストの追加により、正確性と拡張性を高められる。