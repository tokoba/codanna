# resolution.rs Review

## TL;DR

- 目的: Rustのスコープ解決と継承（トレイト）解決を、旧API互換の形で再実装したモジュール。主要構成は**RustResolutionContext**（名前解決）と**RustTraitResolver**（メソッド解決）。
- 公開API: ResolutionScope/IneritanceResolverトrait実装＋補助メソッド群。中心は**resolve**（名前解決）、**resolve_method/resolve_method_trait**（メソッドの由来解決）。
- 重要修正: **resolve**における「完全修飾パスの直接解決」を追加（例: "crate::init::init_global_dirs"）。2分割パスの簡易解決もサポート。
- 複雑箇所: スコープスタックとローカルクリア、パス解決の分岐、トレイトと固有メソッドの優先順位、曖昧メソッドへの簡易警告。
- 重大リスク: exit_scopeのローカルクリア条件がコメントと挙動の不整合、ImportBinding/Importsが解決経路に未統合、unwrapの潜在的パニック、曖昧なトレイトメソッド選択の不確実性。
- Rust安全性: unsafeなし、所有権・借用は妥当。Option中心の戻り値設計だがエラー型不在のため診断性は限定的。
- 並行性: 内部可変HashMapを多用、Send/Sync前提なし。並行アクセス時の同期は未提供。

## Overview & Purpose

このファイルはRust特有の解決規則を実装する2つの主要コンポーネントを提供します。

- RustResolutionContext: スコープレベル（Local → Imported → Module → Crate）の優先順位でシンボルを解決。旧ResolutionContext API互換。useによるインポート、モジュール/クレート公開シンボル、ローカル変数を管理し、名前解決の中心である**resolve**を提供します。
- RustTraitResolver: Rustのトレイト実装と固有メソッドの優先順位に基づくメソッド解決。タイプが実装するトレイト、トレイトのメソッド、固有メソッドの対応をHashMapで管理し、**resolve_method**や**resolve_method_trait**を提供します。

旧モジュール（src/indexing/trait_resolver.rs、resolver.rs、resolution_context.rs）のロジックをRustに特化した形で移植し、API互換を保っています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | RustResolutionContext | pub | スコープ別シンボル解決（Local/Imported/Module/Crate）、インポート追跡、関係解決 | Med |
| Impl(Trait) | ResolutionScope for RustResolutionContext | pub（Trait経由） | 旧API互換のシンボル登録・解決・スコープ入退出・関係解決 | Med |
| Struct | RustTraitResolver | pub | トレイト実装管理、メソッド解決（固有優先） | Med |
| Impl(Trait) | InheritanceResolver for RustTraitResolver | pub（Trait経由） | 実装登録、サブタイプ判定、メソッド集合取得 | Med |
| Module | tests | private | 解決の回帰/修正確認（完全修飾パスのバグ／修正） | Low |

フィールド概要（抜粋）
- RustResolutionContext
  - local_scope/imported_symbols/module_symbols/crate_symbols: 各スコープの名前→SymbolIdマップ
  - scope_stack: Vec<ScopeType>で現在のネスト状態追跡
  - imports: Vec<(path, alias)>で生のuse記録
  - import_bindings: exposed_name→ImportBindingの詳細情報
- RustTraitResolver
  - type_to_traits: 型→(トレイト, FileId)の一覧
  - trait_methods: トレイト→メソッド一覧
  - type_method_to_trait: (型, メソッド)→トレイト の直接マッピング
  - inherent_methods: 型→固有メソッド一覧

### Dependencies & Interactions

- 内部依存
  - RustResolutionContext.resolve は local_scope → imported_symbols → module_symbols → crate_symbols の順で参照。パス含有時は各スコープに対し「完全修飾名」を直接検索し、さらに2分割パスの再帰解決を試みます。
  - resolve_relationship は RelationKind に応じて resolve を委譲（Calls/その他）し、Definesは直接解決またはNone。
  - RustTraitResolver.resolve_method は is_inherent_method を優先し、type_method_to_trait → trait_methods の順で走査。
- 外部依存（モジュール/クレート）
  | 依存 | 用途 |
  |------|------|
  | crate::{FileId, SymbolId} | ID型管理 |
  | crate::parsing::{InheritanceResolver, ResolutionScope, ScopeLevel, ScopeType} | 旧API互換インターフェース |
  | crate::parsing::resolution::ImportBinding | インポートの公開名バインディング |
  | crate::RelationKind | 関係種別（Defines/Calls等） |
  | std::collections::HashMap | 内部テーブル管理 |
- 被依存推定
  - インデクシング/解析フェーズの名前解決・関係抽出（呼び出し、定義）
  - パーサ/アナライザが構築するファイル単位の解決コンテキスト
  - トレイト関連のコード参照解析（メソッドの由来追跡）

## API Surface (Public/Exported) and Data Contracts

API一覧

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| RustResolutionContext::new | fn new(file_id: FileId) -> Self | コンテキスト初期化 | O(1) | O(1) |
| RustResolutionContext::add_import | fn add_import(&mut self, path: String, alias: Option<String>) | use記録 | O(1) | O(1) |
| RustResolutionContext::add_local | fn add_local(&mut self, name: String, symbol_id: SymbolId) | ローカル登録 | O(1) | O(1) |
| RustResolutionContext::add_import_symbol | fn add_import_symbol(&mut self, name: String, symbol_id: SymbolId, _is_aliased: bool) | インポート済シンボル登録 | O(1) | O(1) |
| RustResolutionContext::add_module_symbol | fn add_module_symbol(&mut self, name: String, symbol_id: SymbolId) | モジュール登録 | O(1) | O(1) |
| RustResolutionContext::add_crate_symbol | fn add_crate_symbol(&mut self, name: String, symbol_id: SymbolId) | クレート公開登録 | O(1) | O(1) |
| RustResolutionContext::is_imported | fn is_imported(&self, name: &str) -> bool | インポート有無確認 | O(1) | O(1) |
| ResolutionScope::add_symbol | fn add_symbol(&mut self, name: String, symbol_id: SymbolId, scope_level: ScopeLevel) | スコープ別登録（互換API） | O(1) | O(1) |
| ResolutionScope::resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | 名前解決（Rust順序＋パス） | O(1)平均 | O(1) |
| ResolutionScope::clear_local_scope | fn clear_local_scope(&mut self) | ローカル消去 | O(n) | O(1) |
| ResolutionScope::enter_scope | fn enter_scope(&mut self, scope_type: ScopeType) | スコープ入 | O(1) | O(1) |
| ResolutionScope::exit_scope | fn exit_scope(&mut self) | スコープ出＋条件付きローカル消去 | O(1)+O(n) | O(1) |
| ResolutionScope::symbols_in_scope | fn symbols_in_scope(&self) -> Vec<(String, SymbolId, ScopeLevel)> | 可視シンボル一覧 | O(n) | O(n) |
| ResolutionScope::resolve_relationship | fn resolve_relationship(&self, _from_name: &str, to_name: &str, kind: RelationKind, _from_file: FileId) -> Option<SymbolId> | 関係解決（Calls/Defines等） | O(1)平均 | O(1) |
| ResolutionScope::populate_imports | fn populate_imports(&mut self, imports: &[crate::parsing::Import]) | useレコード取り込み | O(m) | O(m) |
| ResolutionScope::register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | インポートバインディング登録 | O(1) | O(1) |
| ResolutionScope::import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | バインディング取得 | O(1) | O(1) |
| RustTraitResolver::new | fn new() -> Self | 初期化 | O(1) | O(1) |
| InheritanceResolver::add_inheritance | fn add_inheritance(&mut self, child: String, parent: String, kind: &str) | 実装登録("implements") | O(1) | O(1) |
| InheritanceResolver::resolve_method | fn resolve_method(&self, type_name: &str, method_name: &str) -> Option<String> | メソッド由来解決 | O(t+m) | O(1) |
| InheritanceResolver::get_inheritance_chain | fn get_inheritance_chain(&self, type_name: &str) -> Vec<String> | 型→実装トレイト一覧 | O(t) | O(t) |
| InheritanceResolver::is_subtype | fn is_subtype(&self, child: &str, parent: &str) -> bool | トレイト実装有無 | O(t) | O(1) |
| InheritanceResolver::add_type_methods | fn add_type_methods(&mut self, type_name: String, methods: Vec<String>) | 固有メソッド登録 | O(k) | O(k) |
| InheritanceResolver::get_all_methods | fn get_all_methods(&self, type_name: &str) -> Vec<String> | 固有＋トレイトメソッド集約 | O(t+m) | O(t+m) |
| RustTraitResolver::add_trait_impl | fn add_trait_impl(&mut self, type_name: String, trait_name: String, file_id: FileId) | トレイト実装登録 | O(1) | O(1) |
| RustTraitResolver::add_trait_methods | fn add_trait_methods(&mut self, trait_name: String, methods: Vec<String>) | トレイトのメソッド登録 | O(k) | O(k) |
| RustTraitResolver::add_inherent_methods | fn add_inherent_methods(&mut self, type_name: String, methods: Vec<String>) | 固有メソッド登録 | O(k) | O(k) |
| RustTraitResolver::resolve_method_trait | fn resolve_method_trait(&self, type_name: &str, method_name: &str) -> Option<&str> | メソッド由来トレイト取得（固有ならNone） | O(t+m) | O(1) |

以下、主要APIの詳細（抜粋）

1) RustResolutionContext.resolve
- 目的と責務
  - Rustの解決順序（Local → Imported → Module → Crate）で名字を探索。
  - 名前に"::"を含む場合、完全修飾名での直接探索、2分割パスの簡易解決（Type::methodやmodule::function）を試行。
- アルゴリズム（主要ステップ）
  1. 各スコープのHashMapからnameを順に検索。
  2. nameに"::"がある場合、各スコープで完全修飾名として検索。
  3. parts.len()==2なら、左辺（型/モジュール）をresolveし成功なら右辺（メソッド/関数）もresolve。
  4. 見つからなければNone。
- 引数
  | 名前 | 型 | 意味 |
  |------|----|------|
  | name | &str | 解決対象名（素名または"::"含むパス） |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | Option<SymbolId> | 見つかればSome(id)、なければNone |
- 使用例
  ```rust
  let mut ctx = RustResolutionContext::new(FileId::new(1).unwrap());
  let sym = SymbolId::new(42).unwrap();
  ctx.add_symbol("init_global_dirs".into(), sym, ScopeLevel::Global);
  ctx.add_symbol("crate::init::init_global_dirs".into(), sym, ScopeLevel::Global);
  assert_eq!(ctx.resolve("init_global_dirs"), Some(sym));
  assert_eq!(ctx.resolve("crate::init::init_global_dirs"), Some(sym));
  ```
- エッジケース
  - nameが複数パーツ（3個以上）: 完全修飾名の直接検索のみ。2分割以外の部分解決は未対応。
  - 左辺が型/モジュールと判定できない場合: 自動判定はなく、左辺の存在確認が成功するかに依存。
  - インポートバインディング未統合: import_bindingsやimportsはresolveで未使用。

2) ResolutionScope.add_symbol
- 目的と責務
  - ScopeLevelに応じて対象HashMapへ登録（Local→local_scope、Module→module_symbols、Package→imported_symbols、Global→crate_symbols）。
- 引数
  | 名前 | 型 | 意味 |
  |------|----|------|
  | name | String | 登録名（素名またはパス） |
  | symbol_id | SymbolId | 識別子 |
  | scope_level | ScopeLevel | 対象スコープ |
- 戻り値: なし
- 使用例
  ```rust
  ctx.add_symbol("foo".to_string(), sym, ScopeLevel::Local);
  ```
- エッジケース
  - 同名上書き: 既存エントリを上書き。

3) ResolutionScope.resolve_relationship
- 目的と責務
  - 関係種別（Defines/Calls等）に応じて名前解決を委譲。Callsは単純/修飾名ともにresolveを呼ぶ。Definesはメソッド定義の区別（トレイトvs固有）を未実装で暫定Noneを返す可能性。
- 引数
  | 名前 | 型 | 意味 |
  |------|----|------|
  | _from_name | &str | 呼び元（未使用） |
  | to_name | &str | 対象名 |
  | kind | RelationKind | 関係種別 |
  | _from_file | FileId | 呼び元ファイル（未使用） |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | Option<SymbolId> | 見つかればSome |
- 使用例
  ```rust
  let id = ctx.resolve_relationship("caller", "module::func", RelationKind::Calls, file_id);
  ```
- エッジケース
  - Defines: トレイトメソッドの解決は未対応でNoneを返しうる。

4) RustTraitResolver.resolve_method
- 目的と責務
  - 型とメソッド名に対し、まず固有メソッドを優先、なければ直接マッピング、最後に型が実装するトレイト群のメソッド一覧を探索し由来（型またはトレイト名）を返す。
- アルゴリズム
  1. is_inherent_method(type_name, method_name)ならSome(type_name)。
  2. type_method_to_traitに一致があればそのトレイト名。
  3. type_to_traits[type_name]を走査し trait_methods[trait] にmethodがあるか確認。
- 引数
  | 名前 | 型 | 意味 |
  |------|----|------|
  | type_name | &str | 型名 |
  | method_name | &str | メソッド名 |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | Option<String> | 由来（型名またはトレイト名） |
- 使用例
  ```rust
  let mut r = RustTraitResolver::new();
  r.add_trait_methods("Display".into(), vec!["fmt".into()]);
  r.add_trait_impl("Point".into(), "Display".into(), FileId::new(1).unwrap());
  assert_eq!(r.resolve_method("Point","fmt"), Some("Display".into()));
  ```
- エッジケース
  - 複数トレイトが同名メソッド: 最初に見つかったトレイトが返る（曖昧性は本来エラー）。診断はresolve_method_trait側でeprintln!警告。

5) RustTraitResolver.resolve_method_trait
- 目的と責務
  - メソッドの由来トレイト名を返す（固有メソッドならNone）。
- 引数/戻り値は上記に準じる（戻り値はOption<&str>）。
- 使用例
  ```rust
  assert_eq!(r.resolve_method_trait("Point","fmt"), Some("Display"));
  ```
- エッジケース
  - 複数トレイトで一致: 最初のトレイトを返し、警告ログを出力。

データ契約（ImportBinding）
- キー: binding.exposed_name（公開される名前）でimport_bindingsに登録・取得。
- 目的: インポートの別名/公開名に関する詳細を保持。
- 注意: 現状resolveはimport_bindingsを参照しないため、名前解決とバインディング情報は未統合。

## Walkthrough & Data Flow

典型的な処理フロー
1. ファイル単位でRustResolutionContextを作成（new）。
2. パーサ/インデクサが、useに関する情報をpopulate_imports、register_import_bindingで登録。解決済みインポートはadd_import_symbolへ。
3. 関数/ブロック/モジュールへ入るたびにenter_scope。ローカル変数・パラメータはadd_localまたはadd_symbol(ScopeLevel::Local)で追加。
4. モジュール/クレート公開シンボルはadd_module_symbol/add_crate_symbol（またはadd_symbol）で登録。
5. 名前/呼び出し/定義関係を解析し、resolveまたはresolve_relationshipでSymbolIdを取得。
6. スコープを抜ける際にexit_scopeを呼び、必要ならローカルをクリア。

Mermaidフローチャート（resolveの主分岐）

```mermaid
flowchart TD
  A[resolve(name)] --> B{Localに存在?}
  B -- Yes --> R1[Some(id)]
  B -- No --> C{Importedに存在?}
  C -- Yes --> R2[Some(id)]
  C -- No --> D{Moduleに存在?}
  D -- Yes --> R3[Some(id)]
  D -- No --> E{Crateに存在?}
  E -- Yes --> R4[Some(id)]
  E -- No --> F{nameに"::"を含む?}
  F -- No --> G[None]
  F -- Yes --> H{完全修飾名がImported/Module/Crateに存在?}
  H -- Yes --> R5[Some(id)]
  H -- No --> I{parts.len()==2?}
  I -- No --> G
  I -- Yes --> J{左辺(type/module)がresolve成功?}
  J -- No --> G
  J -- Yes --> K[右辺(method/func)をresolve]
  K --> L[結果を返す]
```

上記の図は`resolve`関数（行番号はこのチャンクでは不明）の主要分岐を示す。

関連コード抜粋

```rust
fn resolve(&self, name: &str) -> Option<SymbolId> {
    if let Some(&id) = self.local_scope.get(name) { return Some(id); }
    if let Some(&id) = self.imported_symbols.get(name) { return Some(id); }
    if let Some(&id) = self.module_symbols.get(name) { return Some(id); }
    if let Some(&id) = self.crate_symbols.get(name) { return Some(id); }

    if name.contains("::") {
        if let Some(&id) = self.imported_symbols.get(name) { return Some(id); }
        if let Some(&id) = self.module_symbols.get(name) { return Some(id); }
        if let Some(&id) = self.crate_symbols.get(name) { return Some(id); }

        let parts: Vec<&str> = name.split("::").collect();
        if parts.len() == 2 {
            let type_or_module = parts[0];
            let method_or_func = parts[1];
            if self.resolve(type_or_module).is_some() {
                return self.resolve(method_or_func);
            }
        }
        return None;
    }
    None
}
```

## Complexity & Performance

- HashMapベースの解決・登録は平均的に**O(1)**時間／**O(1)**空間（エントリ単位）。
- symbols_in_scopeの集約はエントリ数nに対し**O(n)**時間／**O(n)**空間。
- resolveのパス処理では
  - 完全修飾名の直接検索は**O(1)**（平均）。
  - 2分割パス分解・再帰（split/resolve）は分解が**O(k)**（kは文字列長）、resolveは**O(1)**（平均）。
- RustTraitResolverのget_all_methods/resolve_methodは型が実装するトレイト数t、各トレイトのメソッド数mに対して**O(t+m)**。
  - all_methodsの重複排除にVec.containsを使うため最悪**O(n^2)**的要素があり、方法によってはボトルネック。改善はHashSet導入が有効。
- スケール限界
  - 非階層的なパス解決（3パーツ以上）の部分解決は未対応。大規模モジュール階層では完全修飾登録が必要。
  - imports/import_bindingsをresolveで使用していないため、useツリーの展開が多い場合に解決漏れが起こり得る。

実運用負荷要因
- I/O/ネットワーク/DBは本モジュールでは非対象。CPU/メモリ負荷はエントリ数とメソッド集合サイズに比例。
- ログ（eprintln!）は少量で無視可能だが、大量曖昧性時にSTDERR出力でノイズ化。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト
- メモリ安全性: unsafe不使用。Buffer overflow/Use-after-free/Integer overflowの懸念なし。
- インジェクション: SQL/Command/Path traversalは対象外。名前解決は純粋なメモリ内構造。
- 認証・認可: 当該モジュールの責務外。
- 秘密情報: Hard-coded secretsなし。ログに秘密情報は出力しない想定。
- 並行性: 内部可変構造の共有にロックなし。並行使用時はRace conditionの懸念あり（本設計は単スレッド前提）。

Rust特有の観点（安全性）
- 所有権・借用: 文字列は所有（String）で保持し、参照（&str）で受ける一般的パターン。戻り値Option<&str>（resolve_method_trait）はselfの生存に紐づく参照で安全。
- ライフタイム: 明示的ライフタイムは不要。コンパイラ推論に依存。
- unsafe境界: なし。
- エラー設計: Option中心で診断性は限定的。unwrap使用箇所あり（下記）。

不具合/懸念点（根拠: 関数名、行番号は不明）
1. exit_scopeのローカルクリア条件の不整合
   - コメント「関数スコープを抜けるとクリア」とあるが、実装は「self.scope_stack.pop()した後のlastがNone/Module/Globalならクリア」。
   - 関数スコープ（例えば関数→モジュールへ戻る）ではクリアされるが、ブロックスコープを出るケースなどで期待動作が曖昧。より明確に「関数スコープを出るときのみクリア」へ調整が必要。
2. Imports/import_bindingsとresolveの未統合
   - add_import/populate_imports/register_import_bindingで情報を蓄積するが、resolveはimported_symbolsのみを参照。import_bindings/完全修飾path展開が反映されないため、別名やグロブインポートのケースで解決漏れの可能性。
3. unwrapによる潜在的パニック
   - add_inheritanceでFileId::new(1).unwrap()（ダミーID）。FileId::newが失敗しうる仕様ならpanicのリスク。testsでもFileId/SymbolIdのnewでunwrapあり。
4. resolveの2分割パス簡易解決の誤解決リスク
   - 左辺のresolve成功→右辺もresolveという戦略は型メソッド解決としては緩く、別スコープの同名シンボルへの誤マッピング可能性あり。
5. 曖昧なトレイトメソッドの扱い
   - resolve_method_traitで複数トレイトに同名メソッドがある場合、最初のものを返し、eprintln!で警告のみ。本来は明示的な曖昧性解消を要求すべき。

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 完全修飾パスの直接解決 | "crate::init::init_global_dirs" | 登録済ならSome(SymbolId) | resolveで各スコープに対し直接検索 | OK |
| 2分割パス(Type::method) | "Point::fmt" | 型が可視かつfmtが解決可能ならSome | 左辺resolve成功→右辺resolve | 条件付きOK（誤解決余地） |
| 複数パーツパス(3+) | "a::b::c::d" | 階層解決または直接登録が必要 | 直接登録のみ/部分解決なし | 制限あり |
| インポート別名 | use foo as bar; "bar" | bar→fooへの解決 | import_bindings未参照 | 要修正 |
| ローカルのクリアタイミング | 関数から退出 | ローカルクリア | exit_scopeでModule/Global/None時クリア | 要確認 |
| 曖昧なトレイトメソッド | 型が2トレイトを実装、両方に"f" | エラーか明示的解決要求 | 最初のトレイト返却＋警告 | 要改善 |
| unwrapパニック | FileId::new失敗 | エラー伝播 | unwrapでpanic | 要修正 |

## Design & Architecture Suggestions

- resolveのパス解決強化
  - 完全修飾名の木構造（階層）を導入し、"a::b::c"を段階的に辿る仕組みを追加。
  - 2分割以外のパスにも拡張可能な一般化アルゴリズムへ。
- Imports/ImportBinding統合
  - import_bindingsをresolveに組み込み、別名・公開名の正確な解決を実現。
  - グロブ（use a::b::*）やリネーム対応を含める設計。
- スコープスタックの意味づけ強化
  - ScopeTypeにFunction/Blockなどの識別を導入し、exit_scopeのローカルクリア条件を「Functionから出る時のみ」に明確化。
- トレイトメソッドの曖昧性処理
  - resolve_method/resolve_method_traitにError型（Ambiguity）を導入し、呼出側で明示的な解決要求を促す。
- データ構造最適化
  - get_all_methodsの重複排除にHashSetを使用して**O(n)**でユニーク化。
- 例外・エラー設計
  - unwrap除去、Result型で安全に伝播。
- API整合性
  - add_import_symbol/_is_aliasedのフラグを活用または削除。無使用パラメータは削減。

## Testing Strategy (Unit/Integration) with Examples

推奨ユニットテスト
- 完全修飾名の直接解決
  ```rust
  #[test]
  fn resolves_fully_qualified_names() {
      let mut ctx = RustResolutionContext::new(FileId::new(1).unwrap());
      let id = SymbolId::new(100).unwrap();
      ctx.add_symbol("crate::m::f".into(), id, ScopeLevel::Global);
      assert_eq!(ctx.resolve("crate::m::f"), Some(id));
  }
  ```
- 2分割パスの簡易解決（型→メソッド）
  ```rust
  #[test]
  fn resolves_two_part_type_method() {
      let mut ctx = RustResolutionContext::new(FileId::new(1).unwrap());
      let id_t = SymbolId::new(1).unwrap();
      let id_m = SymbolId::new(2).unwrap();
      ctx.add_symbol("Point".into(), id_t, ScopeLevel::Module);
      ctx.add_symbol("fmt".into(), id_m, ScopeLevel::Module);
      assert_eq!(ctx.resolve("Point::fmt"), Some(id_m));
  }
  ```
- スコープクリアの挙動
  ```rust
  #[test]
  fn clears_locals_on_function_exit() {
      let mut ctx = RustResolutionContext::new(FileId::new(1).unwrap());
      let id = SymbolId::new(1).unwrap();
      ctx.enter_scope(ScopeType::Module);
      ctx.enter_scope(ScopeType::Function);
      ctx.add_symbol("x".into(), id, ScopeLevel::Local);
      assert_eq!(ctx.resolve("x"), Some(id));
      ctx.exit_scope(); // exit function
      // 期待: ローカル消去（現在はModule/Global/None判定に依存）
      assert_eq!(ctx.resolve("x"), None);
  }
  ```
- ImportBindingの取得
  ```rust
  #[test]
  fn import_binding_roundtrip() {
      let mut ctx = RustResolutionContext::new(FileId::new(1).unwrap());
      let b = ImportBinding { exposed_name: "bar".into(), /* 他フィールドは不明 */ };
      ctx.register_import_binding(b.clone());
      assert_eq!(ctx.import_binding("bar"), Some(b));
  }
  ```
- トレイトメソッド解決と曖昧性警告
  ```rust
  #[test]
  fn trait_method_resolution_and_ambiguity() {
      let mut r = RustTraitResolver::new();
      r.add_trait_methods("T1".into(), vec!["m".into()]);
      r.add_trait_methods("T2".into(), vec!["m".into()]);
      r.add_trait_impl("X".into(), "T1".into(), FileId::new(1).unwrap());
      r.add_trait_impl("X".into(), "T2".into(), FileId::new(1).unwrap());
      // 由来は曖昧。現実のRustではエラーだが、本実装は最初のトレイトを返し警告。
      let trait_opt = r.resolve_method_trait("X", "m");
      assert!(trait_opt.is_some());
  }
  ```

インテグレーション（例）
- パーサ→コンテキスト構築→インポート取り込み→名前解決→関係解決の一連のフローで、別名インポートや完全修飾パスの混在を検証。

## Refactoring Plan & Best Practices

- スコープ管理
  - ScopeTypeに関数/ブロック区別を導入、exit_scopeのローカルクリアを明確化。
- パス解決
  - 階層Map（Trieやネストマップ）導入で"::"分解を逐次解決。3パーツ以上も対応。
- インポート統合
  - import_bindingsをresolveで反映。別名・グロブ・再エクスポート対応。
- データ構造
  - get_all_methods重複排除にHashSetを使用。
- エラー処理
  - unwrap除去、Result<_, Error>採用。曖昧性や未解決に対し明示的エラー型。
- ログ/観測性
  - eprintln!/println!をtracing/logクレートへ置換。レベル、フィールド化。
- API整理
  - 未使用/冗長なパラメータ（_is_aliased）や補助構造の整理。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 重要イベント（スコープ入退、登録、未解決、曖昧性）を**info/warn**レベルで記録。
  - **resolve**には「探索段階・命中スコープ・失敗理由」のデバッグログをオプションで出力。
- メトリクス
  - 解決成功率、未解決率、曖昧性発生数、HashMapサイズ（スコープ別）をカウンタ/ゲージでトラック。
- トレーシング
  - ファイルID/スコープスタック/名前をspanに含め、串刺しで追跡可能に。

## Risks & Unknowns

- FileId::new/SymbolId::newの失敗条件は「不明」。unwrap使用に伴うパニックリスクの有無は型実装依存。
- ImportBindingの構造詳細は「不明」。resolveとの統合要件も「不明」。
- ScopeTypeのバリエーション（Function/Block等）の有無は「不明」。exit_scopeの正当性検証に必要。
- RelationKindの全列挙は「不明」。Defines以外の関係の特別扱い必要性の判断が限定的。
- 旧API（ResolutionScope/InheritanceResolver）の外部契約仕様の細部は「不明」。互換性要件の網羅には追加情報が必要。