# resolution.rs Review

## TL;DR

- 目的: GDScript向けの名前解決スコープと単一継承ベースの継承解決を提供し、言語特有のクラス/モジュール/ローカル/インポート/グローバルの解決規則を実装する。
- 主な公開API: GdscriptResolutionContext（ResolutionScopeの実装）と GdscriptInheritanceResolver（InheritanceResolverの実装）。加えて両者の new コンストラクタ。
- コアロジック: スコープスタックに基づく段階的な名前解決（ローカル→クラス→モジュール→インポート→グローバル→ドット表記の順）。継承チェーンのDFSによるメソッド探索。
- 複雑箇所: Moduleスコープの取り扱い（クラス内での定義もModule側にエクスポートする挙動）、ドット表記の解決（"A.B"）の帰結、ブロックスコープの有無。
- 重大リスク: "A.B"の解決でBが見つからない場合にAを返す挙動、symbols_in_scopeの重複、GDScriptのブロックスコープ仕様との整合性が不明、resolve_relationshipで引数をほぼ無視。
- Rust安全性: unsafeなし、所有権/借用はシンプルで問題なし。エラーはOptionベースでパニックなし（テスト除く）。並行性は考慮外（非Sync/Send保証なし）。
- 性能: 典型操作はO(1)〜O(スコープ段数)、継承探索はO(継承深さ)。規模に対して十分軽量。

## Overview & Purpose

このファイルは、GodotのGDScriptの言語特性に合わせたスコープ解決と継承解決を提供する。具体的には:

- GdscriptResolutionContext: スコープの積み上げ（関数/ブロック/クラス/モジュール/グローバル/インポート）に基づく名前解決を提供し、ResolutionScopeトレイトを実装。
- GdscriptInheritanceResolver: 子→親の対応と型ごとのメソッド集合を保持し、InheritanceResolverトレイトを実装してメソッド解決、継承チェーン照会、サブタイプ判定を提供。

対象ドメインはパーサ/解析段階の軽量な名前解決であり、型の実体や神（Class）オブジェクトの詳細なメンバ表は持たず、シンボルIDや文字列名ベースで扱うのが特徴。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | GdscriptResolutionContext | pub | GDScriptの各種スコープ（ローカル/クラス/モジュール/グローバル/インポート）を追跡し、名前解決する | Med |
| Struct | GdscriptInheritanceResolver | pub | 単一（実装上は複数も許容）継承チェーン管理とメソッド探索 | Med |
| Trait Impl | impl ResolutionScope for GdscriptResolutionContext | pub | add_symbol/resolve/enter_scope/exit_scopeなど、言語固有スコープ規則の実装 | Med |
| Trait Impl | impl InheritanceResolver for GdscriptInheritanceResolver | pub | 継承追加/メソッド解決/チェーン/サブタイプ/メソッド集計の実装 | Low |
| Module | tests | private | 基本的なスコープ解決と継承解決の単体テスト | Low |

### Dependencies & Interactions

- 内部依存
  - GdscriptResolutionContext
    - current_local_scope_mut, current_class_scope_mut, resolve_in_locals, resolve_in_classes は resolve/add_symbol/symbols_in_scope 等から呼ばれる。
    - scope_stack により現在のスコープ種別を管理し、add_symbolの振る舞いを分岐。
  - GdscriptInheritanceResolver
    - resolve_method_recursive, collect_chain, gather_methods は各公開APIの内部で再帰的に利用。
- 外部依存（クレート/モジュール）

| 外部名 | 用途 |
|-------|------|
| crate::parsing::resolution::{ImportBinding, InheritanceResolver, ResolutionScope} | トレイトおよびインポート束縛データ |
| crate::parsing::{ScopeLevel, ScopeType} | スコープの粒度/種別の定義 |
| crate::{FileId, SymbolId} | ファイル/シンボル識別子 |
| std::collections::{HashMap, HashSet} | 各種マップ/集合 |

- 被依存推定
  - GDScriptパーサ/ASTウォーカーによるシンボル登録（add_symbol, enter_scope/exit_scope）
  - クロスリファレンス/ジャンプ（resolve, resolve_relationship）
  - 型支援/補完（InheritanceResolverのresolve_method, get_all_methods など）

## API Surface (Public/Exported) and Data Contracts

### API一覧

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| GdscriptResolutionContext::new | fn new(file_id: FileId) -> Self | 新しい解決コンテキスト生成 | O(1) | O(1) |
| ResolutionScope::as_any_mut | fn as_any_mut(&mut self) -> &mut dyn Any | ダウンキャスト用アクセサ | O(1) | O(1) |
| ResolutionScope::add_symbol | fn add_symbol(&mut self, name: String, symbol_id: SymbolId, scope_level: ScopeLevel) | シンボル登録 | O(1) | O(1) |
| ResolutionScope::resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | 名前解決 | O(d) | O(1) |
| ResolutionScope::clear_local_scope | fn clear_local_scope(&mut self) | 最内ローカルスコープのクリア | O(k) 但しkは最内スコープサイズ | O(1) |
| ResolutionScope::enter_scope | fn enter_scope(&mut self, scope_type: ScopeType) | スコープ入場 | O(1) | O(1) |
| ResolutionScope::exit_scope | fn exit_scope(&mut self) | スコープ退出 | O(1) | O(1) |
| ResolutionScope::symbols_in_scope | fn symbols_in_scope(&self) -> Vec<(String, SymbolId, ScopeLevel)> | 可視シンボル列挙 | O(N) | O(N) |
| ResolutionScope::resolve_relationship | fn resolve_relationship(&self, _from_name: &str, to_name: &str, _kind: crate::RelationKind, _from_file: FileId) -> Option<SymbolId> | 関連解決（簡易） | O(d) | O(1) |
| ResolutionScope::populate_imports | fn populate_imports(&mut self, _imports: &[crate::parsing::Import]) | GDScriptインポートの取り込み（NOP） | O(1) | O(1) |
| ResolutionScope::register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | インポート束縛の登録 | O(1) | O(1) |
| ResolutionScope::import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | インポート束縛の取得 | O(1) | O(1) |
| GdscriptInheritanceResolver::new | fn new() -> Self | 継承リゾルバ生成 | O(1) | O(1) |
| InheritanceResolver::add_inheritance | fn add_inheritance(&mut self, child: String, parent: String, _kind: &str) | 継承関係の追加 | O(1) | O(1) |
| InheritanceResolver::resolve_method | fn resolve_method(&self, type_name: &str, method: &str) -> Option<String> | メソッド定義元型の探索 | O(h) | O(h) |
| InheritanceResolver::get_inheritance_chain | fn get_inheritance_chain(&self, type_name: &str) -> Vec<String> | 親チェーン列挙 | O(h) | O(h) |
| InheritanceResolver::is_subtype | fn is_subtype(&self, child: &str, parent: &str) -> bool | サブタイプ判定 | O(h) | O(h) |
| InheritanceResolver::add_type_methods | fn add_type_methods(&mut self, type_name: String, methods: Vec<String>) | 型直下メソッドの登録 | O(m) | O(m) |
| InheritanceResolver::get_all_methods | fn get_all_methods(&self, type_name: &str) -> Vec<String> | 継承込みの全メソッド集合 | O(h + M) | O(M) |

注:
- d = スコープ段数（localスコープ数 + classスコープ数 + 定数時間の他マップ）
- h = 継承深さ
- N = 現在可視な全シンボル数
- M = 集合化されるメソッド総数。HashSetで重複排除。

以下、主要APIの詳細。

### GdscriptResolutionContext::new

1) 目的と責務
- 新規ファイルに対する解決コンテキストの初期化。グローバルスコープを1フレームとして積む。

2) アルゴリズム
- フィールドをデフォルト初期化し、scope_stackにGlobalを積む。

3) 引数
| 名前 | 型 | 意味 |
|------|----|------|
| file_id | FileId | 対象ファイルID |

4) 戻り値
| 型 | 意味 |
|----|------|
| Self | 新規コンテキスト |

5) 使用例
```rust
let ctx = GdscriptResolutionContext::new(FileId::new(1).unwrap());
```

6) エッジケース
- 特になし。

### ResolutionScope::add_symbol（GdscriptResolutionContextによる実装）

1) 目的と責務
- 指定のスコープレベルに従ってシンボルを登録する。
- Moduleレベルはクラス内部ではクラスメンバとしても取り扱う（かつmodule_scopeにもor_insertで配置）。

2) アルゴリズム（簡略）
- Local: 最内ローカルスコープに挿入。
- Module: scope_stackのトップがClassならclass_scopesの最内にも挿入。その後module_scopeにor_insert（未登録時のみ）。
- Package: import_scopeに挿入。
- Global: global_scopeに挿入し、module_scopeにもor_insert。

3) 引数
| 名前 | 型 | 意味 |
|------|----|------|
| name | String | シンボル名 |
| symbol_id | SymbolId | シンボルID |
| scope_level | ScopeLevel | 追加先スコープの粒度 |

4) 戻り値
| 型 | 意味 |
|----|------|
| () | なし |

5) 使用例
```rust
context.add_symbol("Player".into(), player_id, ScopeLevel::Module);
```

6) エッジケース
- クラス内部でModule登録するとmodule_scopeにも追加され得るため、重複/二重可視化に注意（期待仕様ならOK）。

該当コード抜粋:
```rust
match scope_level {
    ScopeLevel::Local => {
        self.current_local_scope_mut().insert(name, symbol_id);
    }
    ScopeLevel::Module => {
        if matches!(self.scope_stack.last(), Some(ScopeType::Class)) {
            if let Some(scope) = self.current_class_scope_mut() {
                scope.insert(name.clone(), symbol_id);
            }
        }
        self.module_scope.entry(name).or_insert(symbol_id);
    }
    /* ... 省略 ... */
}
```

### ResolutionScope::resolve（GdscriptResolutionContextによる実装）

1) 目的と責務
- 名前をローカル→クラス→モジュール→インポート→グローバルの優先順位で解決。
- "A.B" のようなドット表記の軽量サポート。

2) アルゴリズム（ステップ）
- local_scopes（内側から外側へ）検索。
- class_scopes（内側から外側へ）検索。
- module_scope検索。
- import_scope検索。
- global_scope検索。
- "head.tail" に分割可能なら head を resolve したうえで、tail をクラススコープで探す。見つかればtailを返す。見つからなければheadの解決結果を返す。
- 見つからなければNone。

3) 引数
| 名前 | 型 | 意味 |
|------|----|------|
| name | &str | 参照名（必要なら "A.B" 形式） |

4) 戻り値
| 型 | 意味 |
|----|------|
| Option<SymbolId> | 見つかればID、なければNone |

5) 使用例
```rust
let id = context.resolve("Player");
let method = context.resolve("move");
let maybe = context.resolve("Player.move");
```

6) エッジケース
- "A.B" でBが見つからない場合にAを返す現在の仕様に注意（期待仕様に応じて変更検討）。

該当コード抜粋（分岐部のみ）:
```rust
/* ... 前半のローカル/クラス/モジュール/インポート/グローバル探索は省略 ... */

// Handle qualified access like "Player.move"
if let Some((head, tail)) = name.split_once('.') {
    // Prefer resolving the head, then attempt tail within class scope
    if let Some(class_id) = self.resolve(head) {
        // If the head resolves to the current class, search class scope for the member
        if let Some(id) = self.resolve_in_classes(tail) {
            return Some(id);
        }
        return Some(class_id);
    }
}
```

### ResolutionScope::enter_scope / exit_scope / clear_local_scope / symbols_in_scope など

- enter_scope: Function/Blockでローカルスコープを積む。Classでクラススコープを積む。scope_stackにpush。
- exit_scope: scope_stackのトップをpopし、対応するスコープ（Local or Class）もpop。
- clear_local_scope: 最内ローカルスコープの中身をclear（スコープそのものは維持）。
- symbols_in_scope: 現在の最内ローカル、最内クラス、モジュール、インポート、グローバルの順で列挙（重複ありうる）。

使用例:
```rust
context.enter_scope(ScopeType::Class);
context.add_symbol("move".into(), move_id, ScopeLevel::Module);
assert_eq!(context.resolve("move"), Some(move_id));
context.exit_scope(); // Class
```

### ResolutionScope::register_import_binding / import_binding

- register_import_binding: ImportBindingにシンボルが解決済みなら import_scope にも挿入し、binding を内部保持。
- import_binding: 以前に登録したbindingを取得（Clone返却）。

使用例:
```rust
context.register_import_binding(binding);
if let Some(b) = context.import_binding("Foo") {
    // ...
}
```

### GdscriptInheritanceResolver と InheritanceResolver 実装

- add_inheritance: 親を child → Vec<parent> にpush。
- resolve_method: DFSで method を定義しているもっとも近い祖先型名を返す。
- get_inheritance_chain: 祖先を列挙（重複防止のvisitedあり、順序はDFS準拠）。
- is_subtype: get_inheritance_chainに parent が含まれるか。
- add_type_methods: 型にメソッド名集合を追加。
- get_all_methods: 祖先を含めてメソッド集合を集約（HashSetで重複排除、順序未定）。

使用例（テスト相当）:
```rust
let mut r = GdscriptInheritanceResolver::new();
r.add_inheritance("Player".into(), "CharacterBody2D".into(), "extends");
r.add_inheritance("CharacterBody2D".into(), "Node2D".into(), "extends");
r.add_type_methods("CharacterBody2D".into(), vec!["physics_process".into()]);
r.add_type_methods("Player".into(), vec!["jump".into()]);
assert!(r.is_subtype("Player", "Node2D"));
assert_eq!(r.resolve_method("Player", "physics_process"), Some("CharacterBody2D".into()));
```

## Walkthrough & Data Flow

- 解決順序（優先度）: 
  1) ローカル最内 → 外側
  2) クラス最内 → 外側
  3) モジュール
  4) インポート
  5) グローバル
  6) ドット表記（"A.B"）: A解決後にBをクラススコープで探索。見つからなければAを返す挙動。

- データ構造:
  - local_scopes: Vec<HashMap<name, SymbolId>>
  - class_scopes: Vec<HashMap<name, SymbolId>>
  - module_scope/import_scope/global_scope: HashMap<name, SymbolId>
  - import_bindings: HashMap<exposed_name, ImportBinding>
  - parents: HashMap<child, Vec<parent>>
  - type_methods: HashMap<type, HashSet<method>>

以下は resolve の主要分岐のフローチャート。

```mermaid
flowchart TD
    A[resolve(name)] --> B{ローカルに存在?}
    B -- Yes --> R1[SymbolIdを返す]
    B -- No --> C{クラスに存在?}
    C -- Yes --> R2[SymbolIdを返す]
    C -- No --> D{モジュールに存在?}
    D -- Yes --> R3[SymbolIdを返す]
    D -- No --> E{インポートに存在?}
    E -- Yes --> R4[SymbolIdを返す]
    E -- No --> F{グローバルに存在?}
    F -- Yes --> R5[SymbolIdを返す]
    F -- No --> G{"nameに'.'が含まれる?"}
    G -- No --> N[None]
    G -- Yes --> H[head, tailに分割]
    H --> I{headをresolve可能?}
    I -- No --> N2[None]
    I -- Yes --> J{tailがクラススコープに存在?}
    J -- Yes --> R6[tailのSymbolIdを返す]
    J -- No --> R7[headのSymbolIdを返す]
```

上記の図は`resolve`関数の主要分岐を示す（行番号: 不明。このチャンクには行番号メタは提供されない）。

## Complexity & Performance

- GdscriptResolutionContext
  - add_symbol: 平均O(1)（HashMap）。Module時にclass_scopeにも挿入するため、定数コストが僅増。
  - resolve: O(L + C + 1) ただしL=ローカルフレーム数、C=クラスフレーム数。ドット表記時に head の再帰resolveが1回起き得るため、最大+O(L+C)。
  - enter_scope/exit_scope: O(1)
  - clear_local_scope: O(k) ただしk=最内ローカルスコープのサイズ
  - symbols_in_scope: O(N) ただしN=すべての可視シンボル総数（重複あり）
- GdscriptInheritanceResolver
  - add_inheritance/add_type_methods: O(1)〜O(m)
  - resolve_method: O(h)（深さ優先でvisitedあり）
  - get_inheritance_chain: O(h)
  - is_subtype: O(h)
  - get_all_methods: O(h + M)（祖先探索 + 集合結合）

ボトルネック:
- 極端に多段なスコープ/継承深さで線形に劣化。
- symbols_in_scope は全件列挙のため、大規模スクリプトで高コストになり得る。
- HashMap/HashSet使用に起因するメモリフットプリント（小〜中規模では問題低）。

実運用負荷:
- 解析フェーズ専用でI/O/ネットワークなし。CPU/メモリのみの負荷。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト:
- メモリ安全性: unsafeなし。バッファオーバーフロー/Use-after-free/整数オーバーフローの懸念は低い。
- インジェクション: SQL/コマンド/パストラバーサルは扱わないため該当なし。
- 認証・認可: コンパイル時解析ユーティリティであり該当なし。
- 秘密情報: ハードコード秘密/ログ漏洩なし。
- 並行性: 明示的な同期なし。マルチスレッド下で共有するなら外部で同期が必要（Send/Sync保証については明記なし）。

Rust特有の観点（詳細チェックリスト）:
- 所有権: HashMap/HashSetに所有権を移動して格納。current_local_scope_mutでlast_mut().unwrap()を呼ぶ前に空ならpushするため安全（関数: current_local_scope_mut）。
- 借用: &mut self を適切に使用。長期借用は行わず、関数内に限定。
- ライフタイム: 明示的パラメータ不要。参照を外に返さず、Option<SymbolId>等のコピー可能値を返す。
- unsafe境界: なし。
- 並行性・非同期: 非Sync/非Sendを示すコードはないが、スレッド共有は想定していない。awaitもキャンセル制御もなし。
- エラー設計: Option/ResultのうちOption中心。unwrap/expectはテストコード内のみ使用。エラー変換はなし。

エッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ドット表記でメンバ未解決 | "Player.unknown" | None（または厳密な仕様次第） | head解決後、tail不在ならheadのIDを返す | 要検討/改善余地 |
| 未定義名の解決 | "NotDefined" | None | すべてのスコープ不一致でNone | OK |
| ローカルとクラスで同名 | ローカルx、クラスx | ローカル優先でx解決 | resolveはローカル→クラス順 | OK |
| クラスとモジュールで同名 | クラスm、モジュールm | クラス優先 | resolveはクラス→モジュール順 | OK |
| importとmoduleの衝突 | import名i、module名i | 仕様に応じてどちらか | module優先（importはmoduleの後） | 仕様要確認 |
| symbols_in_scopeの重複 | クラス"move" + moduleにも"move" | 一意集合が望ましい | 2回pushされ得る | 改善推奨 |
| ブロックスコープ | if内var x、ブロック外参照 | GDScript仕様では関数スコープが一般的 | Blockで新たなローカルフレームを積む | 仕様適合性が不明 |
| 継承ループ | A→B、B→A | ループ検出して停止 | visitedで停止 | OK |
| 複数継承 | A→B, A→C | 明確なMROが必要 | Vec<String>でDFS順になる | 仕様上GDScriptは単一継承、想定外入力の結果順は不定 |

指摘すべきバグ/リスク:
- ドット表記のフォールバック: tailが見つからない場合にheadを返すのは誤解を招く。少なくとも「headがクラスであり、tailがそのクラスに存在する場合のみ成功」とすべき。現実装は head が何であっても（クラスでなくても）headを返す可能性があるため、誤解決のリスクがある（関数: resolve）。
- symbols_in_scopeの重複: クラススコープとモジュールスコープに同一名が存在し、両方pushされる。ツール利用側で重複排除が必要。
- resolve_relationship が from_name/kind を利用せず to_nameの解決のみを返す簡略実装。関係種別に応じた解決の拡張が望ましい。
- ブロックスコープの扱い: GDScriptの仕様が関数スコープ中心であれば、Blockで新フレームを積むのは挙動差の可能性あり（この点は仕様確認が必要）。

## Design & Architecture Suggestions

- ドット表記の厳密化
  - 仕様例: "A.B" は「Aがクラス（またはクラスインスタンス）を表すシンボルである」かつ「そのメンバBが存在する」場合にのみ成功。そうでなければNone。現在の「B不在でもAを返却」は避ける。
  - 可能なら SymbolId → クラス（型）メタへのマッピングを保持し、対象クラスのメンバ表でBを解決する。
- 重複の抑制
  - symbols_in_scopeはVec生成前にHashSetで重複排除するか、（name, id）のキーで一意化。
- スコープフレームの型付け
  - local/class/moduleの3種類を一元管理するFrame構造を導入すると読みやすく拡張も容易。
- ブロックスコープの仕様確認
  - GDScript 仕様次第で Block での新ローカルフレーム積みを撤回/変更（例えば関数フレーム1つに集約し、宣言位置情報で整合性管理）。
- 継承の単一性を前提に
  - add_inheritance で既に親がいる場合は上書き/警告など、単一継承前提をコードにも反映。
- APIドキュメント強化
  - resolve の探索順、ドット表記の解釈、ModuleレベルがClassにも反映されるポリシーを明記。

## Testing Strategy (Unit/Integration) with Examples

拡充すべきテスト例:

1) ドット表記のエッジ
```rust
#[test]
fn test_qualified_resolution_tail_missing() {
    let file_id = FileId::new(1).unwrap();
    let mut ctx = GdscriptResolutionContext::new(file_id);
    let a_id = SymbolId::new(1).unwrap();
    ctx.add_symbol("A".into(), a_id, ScopeLevel::Module);

    // 期待: None（または仕様に合わせる）。現実装は Some(a_id) になる。
    assert_eq!(ctx.resolve("A.missing"), None); // 仕様変更後の期待
}
```

2) import vs module の優先度
```rust
#[test]
fn test_import_vs_module_precedence() {
    let file_id = FileId::new(1).unwrap();
    let mut ctx = GdscriptResolutionContext::new(file_id);
    let mod_id = SymbolId::new(2).unwrap();
    ctx.add_symbol("X".into(), mod_id, ScopeLevel::Module);

    // import登録
    let mut binding = ImportBinding {
        exposed_name: "X".into(),
        // ... 他フィールドは不明のため省略/モック
        resolved_symbol: Some(SymbolId::new(3).unwrap()),
    };
    ctx.register_import_binding(binding);

    // 現実装: module優先でmod_idが返る。仕様に応じて検証。
    assert_eq!(ctx.resolve("X"), Some(mod_id));
}
```

3) ブロックスコープの動作（仕様確認用）
```rust
#[test]
fn test_block_scope_behavior() {
    let file_id = FileId::new(1).unwrap();
    let mut ctx = GdscriptResolutionContext::new(file_id);
    ctx.enter_scope(ScopeType::function());

    ctx.enter_scope(ScopeType::Block);
    let v = SymbolId::new(11).unwrap();
    ctx.add_symbol("v".into(), v, ScopeLevel::Local);
    assert_eq!(ctx.resolve("v"), Some(v));
    ctx.exit_scope(); // Block

    // GDScriptが関数スコープならここでもSome(v)が期待される
    assert_eq!(ctx.resolve("v"), Some(v)); // 仕様に合わせて調整
}
```

4) 継承ループ/複数継承の健全性
```rust
#[test]
fn test_inheritance_cycle_and_multi() {
    let mut r = GdscriptInheritanceResolver::new();
    r.add_inheritance("A".into(), "B".into(), "extends");
    r.add_inheritance("B".into(), "A".into(), "extends");
    assert!(r.get_inheritance_chain("A").contains(&"B".into()));
    assert!(r.get_inheritance_chain("B").contains(&"A".into())); // visitedにより停止するが順序は不定

    // 複数継承を入れても落ちないこと
    r.add_inheritance("C".into(), "D".into(), "extends");
    r.add_inheritance("C".into(), "E".into(), "extends");
    let _ = r.get_inheritance_chain("C");
}
```

5) symbols_in_scopeの重複
```rust
#[test]
fn test_symbols_in_scope_duplicates() {
    let file_id = FileId::new(1).unwrap();
    let mut ctx = GdscriptResolutionContext::new(file_id);
    ctx.enter_scope(ScopeType::Class);
    let mid = SymbolId::new(5).unwrap();
    ctx.add_symbol("m".into(), mid, ScopeLevel::Module);
    let all = ctx.symbols_in_scope();
    // "m" が複数回含まれる可能性
    assert!(all.iter().filter(|(n,_,_)| n == "m").count() >= 1);
}
```

6) メソッド集約の順序非決定性
```rust
#[test]
fn test_all_methods_order_is_unspecified() {
    let mut r = GdscriptInheritanceResolver::new();
    r.add_type_methods("A".into(), vec!["a".into(), "b".into()]);
    let v = r.get_all_methods("A");
    // 並び順は未定。集合比較を推奨。
    assert!(v.contains(&"a".into()) && v.contains(&"b".into()));
}
```

## Refactoring Plan & Best Practices

- resolveのドット表記処理を関数分離し、厳密な型文脈を導入（例: resolve_qualified(head_id, member_name)）。head がクラス/インスタンスであることを検証。
- symbols_in_scopeの重複排除（HashSetで一意化）と出力順の安定化。
- Blockスコープの挙動を仕様確認の上で是正（関数スコープ一本化など）。必要に応じてenter_scope(ScopeType::Block)では新規フレームを作らず、clear_local_scope等のフックを併用。
- InheritanceResolverに単一継承制約を反映（既存の親がある場合の扱いを定義）。
- resolve_relationship の拡張: kind（RelationKind）や from_name/from_file を解釈して、同一ファイル/異ファイル、メンバ/型/モジュール関係を分岐。

## Observability (Logging, Metrics, Tracing)

- ロギング:
  - add_symbol/resolve/register_import_binding/enter_scope/exit_scopeでdebugログを追加。重複定義や上書き時はwarn。
- トレーシング:
  - tracingクレートのinstrument属性を resolve/resolve_method に付与し、名前/結果/探索深さをspanに記録。
- メトリクス:
  - 解決成功/失敗件数、スコープ深さの最大値、継承探索の最大深さ、symbols_in_scopeの生成サイズなどをカウンタ/ヒストグラムで計測。

## Risks & Unknowns

- GDScriptのブロックスコープ仕様: 現実装のBlockフレーム生成が仕様と一致するか不明。要仕様確認。
- importとmoduleの優先度: 現実装はmodule優先。言語仕様/ユーザー期待と一致するか要確認。
- "A.B"のフォールバック仕様: B不在時にAを返す挙動は、補完/ナビゲーションで誤解を招く可能性。期待仕様の確認と修正が必要。
- SymbolId → 型/クラスメタのマッピングがこのレイヤーにはないため、ドット表記の正確なメンバ解決が困難。上位レイヤの設計/連携が鍵。
- 並行実行: 現コンテキストはスレッドセーフ設計ではない（外部同期が必要）。解析を並行化する場合、適切な同期またはスレッドローカル化が必要。