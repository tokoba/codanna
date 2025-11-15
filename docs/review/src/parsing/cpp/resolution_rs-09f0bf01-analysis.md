# resolution.rs Review

## TL;DR

- 目的: C++の名前解決・スコープ管理と継承解決のためのコンテキストとリゾルバを提供
- 主要公開API: CppResolutionContext::{new, resolve, add_symbol, enter_scope/exit_scope, add_using_declaration, derives_from}, CppInheritanceResolver::{resolve_method, is_subtype, get_all_methods}
- 複雑箇所: resolveの優先順位、exit_scopeのローカルクリア条件、継承のDFS探索と循環検出
- 重大リスク: using directiveが解決経路に未反映、関数スコープ終了時に親がClassの場合にローカルがクリアされない可能性、継承種別(kind)が利用されない
- セキュリティ: unsafe未使用、I/Oなしでインジェクションリスク低、並行性未考慮
- テスト: 基本的な解決/継承/スコープ管理のユニットテストあり。複合ケース・クラス親子関係のスコープは未検証

## Overview & Purpose

このファイルは、C++固有の名前解決と継承解決のための2つの主要構造体を提供します。

- CppResolutionContext: C++のスコープ規則（ローカル、モジュール/ファイル、インポート、グローバル）に沿う名前解決、using宣言、include、継承グラフの管理を行います。ResolutionScopeトレイトを実装し、統一された解決インターフェースを提供します。
- CppInheritanceResolver: InheritanceResolverトレイトのC++版実装で、継承チェーン、メソッド解決、サブタイプ判定、メソッド集約をDFSで行います。

C++の複雑なスコープ・名前空間・クラス・多重継承を扱う設計の土台ですが、using directiveの反映、継承種別（public/protected/private/virtual）の扱いなど、詳細仕様の一部は未実装/簡略化されています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | CppResolutionContext | pub | スコープごとの記号表管理、名前解決、include/using追跡、継承グラフ保持 | Med |
| Impl (Trait) | ResolutionScope for CppResolutionContext | pub (実装により使用可能) | 統一インターフェースによる記号追加・解決・スコープ制御・インポート取り扱い | Med |
| Struct | CppInheritanceResolver | pub | 継承関係・型ごとのメソッド集合の管理 | Med |
| Impl (Trait) | InheritanceResolver for CppInheritanceResolver | pub (実装により使用可能) | メソッド解決、継承チェーン構築、サブタイプ判定、メソッド集約 | Med |
| Helper | build_inheritance_chain | private | 継承チェーンの再帰構築 | Low |
| Helper | is_subtype_recursive | private | サブタイプ判定の再帰探索 | Low |
| Helper | collect_all_methods | private | すべての継承メソッドの集約 | Med |

### Dependencies & Interactions

- 内部依存
  - CppResolutionContext
    - 名前解決時に using_declarations → local_scope → module_symbols → imported_symbols → global_symbols の順に参照
    - 継承関連 API（add_inheritance, derives_from, get_base_classes）は inheritance_graph を参照
  - CppInheritanceResolver
    - inheritance_map と type_methods を再帰DFSで走査
- 外部依存（クレート・モジュール）
  - crate::parsing::resolution::ImportBinding（インポートバインディング）
  - crate::parsing::{InheritanceResolver, ResolutionScope, ScopeLevel, ScopeType}（共通トレイトと定義）
  - crate::{FileId, SymbolId}（ID型）
  - std::collections::{HashMap, HashSet}
- 被依存推定
  - パーサのC++フロントエンド（抽出されたシンボルを add_symbol/add_import_symbol）
  - クロス言語解析レイヤ（ResolutionScope/InheritanceResolverのポリモーフィズム利用）
  - ドキュメント生成・ナビゲーション（symbols_in_scope、resolve）
  - 依存関係可視化（get_inheritance_chain）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CppResolutionContext::new | fn new(file_id: FileId) -> Self | 解決コンテキストを初期化 | O(1) | O(1) |
| add_include | fn add_include(&mut self, header_path: String) | includeを記録 | O(1) | O(1) |
| add_using_directive | fn add_using_directive(&mut self, namespace: String) | using namespaceを記録 | O(1) | O(1) |
| add_using_declaration | fn add_using_declaration(&mut self, name: String, symbol_id: SymbolId) | using宣言を記録 | O(1) | O(1) |
| add_inheritance | fn add_inheritance(&mut self, derived: SymbolId, base: SymbolId) | 継承グラフ追加 | O(1) | O(1) |
| derives_from | fn derives_from(&self, derived: SymbolId, base: SymbolId) -> bool | 継承可否判定 | O(V+E) | O(V) |
| get_base_classes | fn get_base_classes(&self, class_id: SymbolId) -> Vec<SymbolId> | 直接の基底取得 | O(k) | O(k) |
| includes | fn includes(&self) -> &[String] | include一覧取得 | O(1) | O(1) |
| add_local | fn add_local(&mut self, name: String, symbol_id: SymbolId) | ローカル追加 | O(1) | O(1) |
| add_module_symbol | fn add_module_symbol(&mut self, name: String, symbol_id: SymbolId) | ファイルレベル追加 | O(1) | O(1) |
| add_import_symbol | fn add_import_symbol(&mut self, name: String, symbol_id: SymbolId) | インポート追加 | O(1) | O(1) |
| add_global_symbol | fn add_global_symbol(&mut self, name: String, symbol_id: SymbolId) | グローバル追加 | O(1) | O(1) |
| ResolutionScope::add_symbol | fn add_symbol(&mut self, name: String, id: SymbolId, lvl: ScopeLevel) | 汎用追加（委譲） | O(1) | O(1) |
| ResolutionScope::resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | 名前解決 | O(1) | O(1) |
| ResolutionScope::clear_local_scope | fn clear_local_scope(&mut self) | ローカルクリア | O(n) | - |
| ResolutionScope::enter_scope | fn enter_scope(&mut self, scope_type: ScopeType) | スコープ入場 | O(1) | O(1) |
| ResolutionScope::exit_scope | fn exit_scope(&mut self) | スコープ退出とローカルクリア条件 | O(1)+O(n) | - |
| ResolutionScope::symbols_in_scope | fn symbols_in_scope(&self) -> Vec<(String, SymbolId, ScopeLevel)> | 現在の記号一覧 | O(L+M+P+G) | O(L+M+P+G) |
| ResolutionScope::populate_imports | fn populate_imports(&mut self, imports: &[crate::parsing::Import]) | インポートの取り込み | O(k) | O(k) |
| ResolutionScope::register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | インポートバインディング登録 | O(1) | O(1) |
| ResolutionScope::import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | バインディング取得 | O(1)+clone | O(size) |
| ResolutionScope::as_any_mut | fn as_any_mut(&mut self) -> &mut dyn Any | ダイナミックダウンキャスト補助 | O(1) | O(1) |
| CppInheritanceResolver::new | fn new() -> Self | 継承リゾルバ初期化 | O(1) | O(1) |
| Default::default | fn default() -> Self | newの委譲 | O(1) | O(1) |
| InheritanceResolver::add_inheritance | fn add_inheritance(&mut self, child: String, parent: String, kind: &str) | 継承追加 | O(1) | O(1) |
| InheritanceResolver::resolve_method | fn resolve_method(&self, type_name: &str, method: &str) -> Option<String> | メソッド解決 | O(V+E) | O(V) |
| InheritanceResolver::get_inheritance_chain | fn get_inheritance_chain(&self, type_name: &str) -> Vec<String> | 継承チェーン取得 | O(V+E) | O(V) |
| InheritanceResolver::is_subtype | fn is_subtype(&self, child: &str, parent: &str) -> bool | サブタイプ判定 | O(V+E) | O(V) |
| InheritanceResolver::add_type_methods | fn add_type_methods(&mut self, type_name: String, methods: Vec<String>) | メソッド集合登録 | O(1) | O(m) |
| InheritanceResolver::get_all_methods | fn get_all_methods(&self, type_name: &str) -> Vec<String> | 全メソッド集約 | O(V+E+Σm) | O(Σm) |

以下、主要APIの詳細。

### CppResolutionContext::resolve

1. 目的と責務
   - 名前文字列に対して、C++の優先順位に従ってシンボルIDを返す。優先順位は、**using宣言 → ローカル → モジュール → インポート → グローバル**。

2. アルゴリズム（行番号不明）
   - using_declarations.get(name) を最初に確認
   - local_scope.get(name)
   - module_symbols.get(name)
   - imported_symbols.get(name)
   - global_symbols.get(name)
   - いずれもなければ None

3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | name | &str | 解決対象の識別子 |

4. 戻り値
   | 型 | 意味 |
   |----|------|
   | Option<SymbolId> | 見つかった場合はSome(SymbolId)、それ以外はNone |

5. 使用例
   ```rust
   let mut ctx = CppResolutionContext::new(FileId::new(1).unwrap());
   let id = SymbolId::new(42).unwrap();
   ctx.add_symbol("Foo".to_string(), id, ScopeLevel::Module);
   assert_eq!(ctx.resolve("Foo"), Some(id));
   assert_eq!(ctx.resolve("Bar"), None);
   ```

6. エッジケース
   - using宣言とローカルが同名: using宣言を優先
   - using directive（using namespace ...）は現状解決ロジックに未反映
   - 同名が複数スコープに存在: 優先順位に従い最初にヒットしたものを返す

### ResolutionScope::exit_scope（CppResolutionContext）

1. 目的と責務
   - スコープスタックから一段戻り、必要に応じてローカルスコープをクリア

2. アルゴリズム（行番号不明）
   - scope_stack.pop()
   - 直後の scope_stack.last() が None | Module | Global | Namespace の場合、clear_local_scope()

3. 引数/戻り値
   - 引数なし／戻り値なし

4. 使用例
   ```rust
   ctx.enter_scope(ScopeType::Function { hoisting: false });
   ctx.add_symbol("x".to_string(), SymbolId::new(1).unwrap(), ScopeLevel::Local);
   ctx.exit_scope();
   assert_eq!(ctx.resolve("x"), None);
   ```

5. エッジケース
   - 関数の親がClassの場合、last() == Some(Class)のためローカルクリアされない可能性（バグの疑い）
   - ネストしたブロックスコープからの退出ではローカル維持（関数内のブロック変数の扱い要件次第）

### CppResolutionContext::derives_from

1. 目的と責務
   - シンボルIDで表されるクラス間の継承関係（直接/推移）判定

2. アルゴリズム
   - BFS: to_checkにderivedから開始
   - visitedで循環防止
   - current == base なら true
   - inheritance_graph[current] の基底を to_check に追加

3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | derived | SymbolId | 派生クラス |
   | base | SymbolId | 基底クラス |

4. 戻り値
   | 型 | 意味 |
   |----|------|
   | bool | 継承関係が存在すればtrue |

5. 使用例
   ```rust
   ctx.add_inheritance(derived_id, base_id);
   assert!(ctx.derives_from(derived_id, base_id));
   ```

6. エッジケース
   - 循環継承: visitedで停止
   - 複数継承: すべての基底を探索（順序は格納順）

### CppInheritanceResolver::resolve_method

1. 目的と責務
   - 型名とメソッド名から、実際にメソッドを提供する型（thisか基底）を解決

2. アルゴリズム
   - type_methods[type_name] にメソッドがあれば type_name を返す
   - それ以外は inheritance_map[type_name] の親を宣言順で DFS
   - 親で provider が見つかればそれを返す
   - 見つからなければ None

3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | type_name | &str | 型名 |
   | method | &str | メソッド名 |

4. 戻り値
   | 型 | 意味 |
   |----|------|
   | Option<String> | 提供元型名（見つからなければNone） |

5. 使用例
   ```rust
   let mut r = CppInheritanceResolver::new();
   r.add_type_methods("Base".to_string(), vec!["foo".to_string()]);
   r.add_inheritance("Derived".to_string(), "Base".to_string(), "public");
   assert_eq!(r.resolve_method("Derived", "foo"), Some("Base".to_string()));
   ```

6. エッジケース
   - 多重継承: 宣言順に探索（曖昧性解決は未対応）
   - オーバーライド/隠蔽: コメント通り未厳密（現状は最初に見つかった親を返す）
   - 仮想継承: kind未使用のため未対応

### ResolutionScope::add_symbol（委譲）

1. 目的と責務
   - ScopeLevelに応じて適切なテーブルへシンボルを格納

2. アルゴリズム
   - Local→local_scope、Module→module_symbols、Package→imported_symbols、Global→global_symbols

3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | name | String | 識別子名 |
   | symbol_id | SymbolId | シンボルID |
   | scope_level | ScopeLevel | スコープレベル |

4. 戻り値
   - なし

5. 使用例
   ```rust
   ctx.add_symbol("Foo".to_string(), id, ScopeLevel::Module);
   ```

6. エッジケース
   - 同名上書き: HashMapにより上書きされる（過去値喪失）

### InheritanceResolver::get_all_methods

1. 目的と責務
   - 型に直接定義されたメソッドと継承された全メソッドの集合を返す

2. アルゴリズム
   - visitedで循環防止
   - 自身のmethodsを追加
   - 親すべてを再帰探索し追加（隠蔽の厳密制御は未対応）

3. 引数/戻り値
   - 引数: type_name: &str
   - 戻り値: Vec<String>（ユニーク化済み集合からのイテレータ収集）

5. 使用例
   ```rust
   let methods = r.get_all_methods("Derived");
   ```

6. エッジケース
   - 同名メソッドの隠蔽/オーバーライド: 現状は併存（厳密なC++規則未実装）

## Walkthrough & Data Flow

- 記号表の流れ
  - 追加経路: add_symbol（ScopeLevelに応じて）/add_local/add_module_symbol/add_import_symbol/add_global_symbol
  - 参照経路: resolveが using_declarations → local_scope → module_symbols → imported_symbols → global_symbols の順で探索
  - スコープ管理: enter_scopeでpush、exit_scopeでpop。pop後のスタックトップに応じてローカルクリア。

- 継承データフロー
  - CppResolutionContextの継承（SymbolIdベース）は derives_from でBFS探索
  - CppInheritanceResolverの継承（Stringベース）は resolve_method/get_inheritance_chain/is_subtype/get_all_methods でDFSまたは集合探索

- インポート
  - populate_importsは Import.path を includes に保存
  - register_import_binding/import_binding は ImportBinding を exposed_nameキーで保存/取得
  - resolve は import_bindings を参照しないため、インポート名→シンボル解決の実経路は imported_symbols のみに依存

### Mermaid Flowchart（resolveの主要分岐）

```mermaid
flowchart TD
    A[resolve(name)] --> B{using_declarationsにnameはあるか?}
    B -- Yes --> R1[Return using_declarations[name]]
    B -- No --> C{local_scopeにnameはあるか?}
    C -- Yes --> R2[Return local_scope[name]]
    C -- No --> D{module_symbolsにnameはあるか?}
    D -- Yes --> R3[Return module_symbols[name]]
    D -- No --> E{imported_symbolsにnameはあるか?}
    E -- Yes --> R4[Return imported_symbols[name]]
    E -- No --> F{global_symbolsにnameはあるか?}
    F -- Yes --> R5[Return global_symbols[name]]
    F -- No --> R6[Return None]
```

上記の図は`resolve`関数（行番号不明）の主要分岐を示す。

## Complexity & Performance

- 名前解決
  - 時間計算量: 平均O(1)（固定個数のHashMap探索）
  - 空間計算量: スコープごとのシンボル数に線形
  - ボトルネック: symbols_in_scopeは全スコープ合算でO(N)＋nameのcloneコスト。clear_local_scopeはO(L)（ローカル数）。
- 継承解決
  - 時間計算量: DFS/BFSベースでO(V+E)（V=型数、E=継承辺数）
  - 空間計算量: visited集合や結果収集でO(V)〜O(Σメソッド)
- スケール限界
  - 大きなコードベースでの頻繁な resolve は比較的安定（HashMap）
  - 継承が深い場合の get_all_methods/resolve_method はDFSの再帰コスト増
- 実運用負荷要因
  - I/O/ネットワーク/DBは本モジュールに登場せず「該当なし」
  - 大規模プロジェクトでは imported_symbols/global_symbols が巨大になり解決時のハッシュ衝突によるスループットに影響し得る

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| using directive未反映 | using namespace std; resolve("vector") | namespaceの導入により解決成功 | resolveはusing_directivesを参照しない | 未対応 |
| 関数退出時のローカルクリア（親Class） | enter_scope(Function) → exit_scope（親がClass） | 関数ローカルはクリアされるべき | last()がClassの場合クリア条件外 | BUG疑い |
| 多重継承の曖昧性 | A:foo, B:foo, Derived: A,B | 規則に基づく解決/曖昧性検出 | 宣言順DFS、曖昧性検出なし | 未対応 |
| 仮想継承 | virtual public Base | 仮想継承のダイヤモンド解決 | kindを保存するが未使用 | 未対応 |
| 循環継承 | A→B→A | ループ防止で安全終了 | visitedで防止 | OK |
| import_bindingの解決連携 | register_import_binding後 resolve("Name") | バインディングに基づき解決 | resolveはimport_bindings非参照 | 未対応 |
| symbols_in_scopeの性能 | 大量のシンボル | 線形だが許容 | すべてcloneで収集 | OK |
| クリアタイミング | 関数内ブロック退出 | ブロックスコープのローカルのみクリア | 関数判定のみでブロック考慮なし | 仕様不明 |

セキュリティチェックリスト
- メモリ安全性: 
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全なコレクションのみ使用。unsafeなし。問題の兆候なし。
- インジェクション:
  - SQL/Command/Path traversal: I/O未実施、パスは文字列として保持のみ。現時点「該当なし」。
- 認証・認可:
  - 権限チェック漏れ/セッション固定: 本モジュール範囲外。「該当なし」。
- 秘密情報:
  - ハードコード秘密/ログ漏えい: 秘密取り扱いなし。「該当なし」。
- 並行性:
  - Race condition / Deadlock: 共有可変状態（HashMap）があり得るが同期なし。本モジュール単体は非同期・並行未考慮。多スレッドで共有するならMutex/RwLock等が必要。

Rust特有の観点（詳細）
- 所有権: 文字列やシンボルIDをHashMapへmove（例: add_symbol:行番号不明）。返却はコピー/クローンで安全。
- 借用: resolveは&selfと&strのみの不変借用。includesは&[String]を返しライフタイムは&selfに束縛。
- ライフタイム: 明示的ライフタイム不要。返却参照は&selfに準拠。
- unsafe境界: unsafeブロック「該当なし」。
- Send/Sync: 明示境界なし。複数スレッドで共有する場合は外部で同期必要。
- await境界/キャンセル: 非同期処理なし。
- エラー設計: resolveはOptionを返す。Result未使用。テストでunwrap使用（外部FileId::new/SymbolId::new前提）。

## Design & Architecture Suggestions

- using directiveの反映
  - resolveにusing_directivesを組み込み、名前空間修飾なし参照の探索を行う（例: std::vector → vector）。
- スコープ退出時のローカルクリア条件改善
  - exit_scopeで「退出対象がFunction」であることを判定しクリアする（現在は「退出後の親種別」で推測しているためClass親で取りこぼし）。
- インポートバインディングの統合
  - import_bindingsをresolve経路に組み込み、パス→シンボルのバインディング解決を可能にする。
- 継承kindの活用
  - public/protected/private、virtualを解決ロジックに反映（アクセス可能性、ダイヤモンド継承の共通基底共有）。
- メソッド解決の厳密化
  - オーバーライド/隠蔽/曖昧性検出、名前修飾（スコープ解決）を考慮。
- パフォーマンス
  - symbols_in_scopeの返却をイテレータ提供にし、cloneを必要最小化。
  - 大規模プロジェクト向けに名前→完全修飾名の索引を導入。
- 観測性
  - resolveヒット/ミス、exit_scopeクリア実行をログ化。テスト時のトレース容易化。

## Testing Strategy (Unit/Integration) with Examples

既存テスト
- test_cpp_resolution_basic: モジュールレベル解決の基本
- test_using_declarations: using宣言の優先順位
- test_inheritance_tracking: SymbolIdベース継承の推移性
- test_scope_management: 関数入退場時のローカルクリア

追加推奨テスト
- 親がClassの関数退出でローカルがクリアされることの検証（現状バグ検知用）
- using directive反映のテスト（実装後）
- import_bindings連携のテスト（register_import_binding→resolve経路）
- 多重継承の曖昧性ケース（A,B両方が同名メソッドを提供）
- 循環継承の安全性（get_inheritance_chain / get_all_methodsが停止すること）
- symbols_in_scopeの内容とスコープレベルの正しさ

例（親Classの関数退出クリア）
```rust
#[test]
fn test_exit_scope_clears_locals_when_parent_is_class() {
    let file_id = FileId::new(1).unwrap();
    let mut ctx = CppResolutionContext::new(file_id);
    let local = SymbolId::new(10).unwrap();
    // 疑似的にClassスコープに入る
    ctx.enter_scope(ScopeType::Class);
    // 関数に入る
    ctx.enter_scope(ScopeType::Function { hoisting: false });
    ctx.add_symbol("x".to_string(), local, ScopeLevel::Local);
    // 関数を出る
    ctx.exit_scope();
    // 期待: ローカルはクリアされる
    assert_eq!(ctx.resolve("x"), None);
}
```

例（import_binding連携・実装後）
```rust
#[test]
fn test_import_binding_resolve() {
    let file_id = FileId::new(1).unwrap();
    let mut ctx = CppResolutionContext::new(file_id);
    let sym = SymbolId::new(99).unwrap();
    let binding = ImportBinding {
        exposed_name: "vector".to_string(),
        // ... 他フィールドは実定義に合わせる
    };
    ctx.register_import_binding(binding);
    // 実装後: resolveがimport_bindingsも考慮
    // assert_eq!(ctx.resolve("vector"), Some(sym));
}
```

## Refactoring Plan & Best Practices

- exit_scopeのロジック修正
  1. enter_scope/exit_scopeで対象スコープ型を追跡（popの戻り値でFunctionを直接判定）
  2. Function退出時は常にclear_local_scopeする
- using directive対応
  1. using_directivesから対象namespaceを展開し imported_symbols へ反映するか、resolve側でプレフィックス探索
- インポート統合
  1. populate_imports→register_import_binding→add_import_symbol の連携規約を整備
  2. import_bindingsをresolveで参照
- 継承kindの取り扱い
  1. kindに応じて探索の可視性/順序を調整
  2. virtualのダイヤモンド解決を導入（共通基底重複防止）
- APIのドキュメントと契約
  1. resolveの優先順位と陰影（hiding）仕様を明文化
  2. symbols_in_scopeの返却の安定順序（必要なら名称整列）

ベストプラクティス
- 返却の不要なcloneを避ける（Iterator返却）
- HashMapキーにStringではなくCow<str>やArc<str>で共有削減を検討
- 大文字小文字や名前修飾の規則参照を統一ユーティリティに分離

## Observability (Logging, Metrics, Tracing)

- ログ追加ポイント
  - resolve: どのスコープでヒットしたか（debug）
  - exit_scope: クリアの実施有無・スコープ型（debug）
  - add_symbol: 追加入力の重複警告（trace）
- メトリクス
  - resolve呼び出し数、成功率、平均探索スコープ数（ヒット位置）
  - 継承探索の平均深さ
- トレーシング
  - スコープスタックのpush/popイベント

例（簡易ログの追加例・擬似）
```rust
// 例: resolve内で
// log::debug!("Resolving '{}': checked using_declarations => {:?}", name, using_hit);
// log::debug!("Result: {:?}", result_symbol_id);
```

## Risks & Unknowns

- Unknown
  - ImportBindingの完全な構造と利用規約（このチャンクには詳細不明）
  - ScopeTypeの全バリアント（Classの存在は仮定、行番号不明）
  - FileId/SymbolIdの内部仕様（Copy/Clone特性など）
- リスク
  - using directive未反映によりC++ソースの一般的解決ケースに対応不足
  - exit_scopeのクリア条件ロジックによるローカルリーク
  - 継承kind未使用によるアクセス制御/仮想継承の誤解決
  - 大規模プロジェクトでのsymbols_in_scopeのcloneコスト
- 対策
  - 設計改善提案の実装
  - テスト拡充（クラス親の関数退出、using directive、多重/仮想継承）
  - ドキュメント化と契約整備（優先順位、隠蔽、アクセス可視性）