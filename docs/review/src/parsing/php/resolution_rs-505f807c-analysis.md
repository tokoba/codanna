# parsing\php\resolution.rs Review

## TL;DR

- 目的: **PHPの名前解決と継承（extends/implements/trait use）**をRustで抽象化し、パーサ/解析器から利用可能にするコンテキストとリゾルバを提供
- 主要公開API: **PhpResolutionContext**（ResolutionScopeを実装）、**PhpInheritanceResolver**（InheritanceResolverを実装）
- コアロジック: `resolve`の名前解決順序（ローカル→クラス→名前空間→グローバル→`::`特例）、`resolve_method`のPHP的MRO（自身→traits（後勝ち）→親）
- 重大リスク:
  - クラス継承の**循環による無限再帰**（`resolve_method`/`is_subtype`で訪問検知がない）
  - **PHPの大文字小文字非依存**の未対応（HashMapキーが大小区別）
  - `Class::method`の解決が**メソッドの所属クラスを無視**して単純に`method`名を再解決
  - **PSR-4オートロード**はモジュール説明にあるが、このチャンクでは機能実装が見当たらない（不明）
- Rust安全性: **unsafe不使用**、所有権・借用は安全。並行性は**&mut自己参照前提**で同期なし、スレッド安全性は利用側に依存
- 追加のテスト必要: `use`エイリアス、`\`から始まるFQCN、`::`、トレイト優先度、継承循環、大小文字差異

## Overview & Purpose

このファイルは、PHPコード解析における**名前解決**と**継承・トレイト/インターフェイス**の解決を提供します。

- PhpResolutionContext: PHPのスコープモデル（ローカル、クラス、名前空間、グローバル）と`use`文・現在の名前空間を踏まえた**シンボル解決**を行う。
- PhpInheritanceResolver: クラスの**単一継承**、**複数インターフェイス実装**、**トレイト使用（優先度＝後勝ち）**に基づくメソッド解決やチェーン構築を行う。

このチャンクの説明文にはPSR-4が言及されていますが、**ファイル探索/マッピングの仕組みはこのコードには存在しません**（不明）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | PhpResolutionContext | pub | PHPの名前解決スコープ・`use`文・名前空間管理 | Med |
| impl Trait | ResolutionScope for PhpResolutionContext | pub（トレイト経由） | シンボル登録/解決/スコープ出入り/インポート管理 | High |
| fn | resolve_name(&self, name: &str) -> Option<String> | private | `use`/現在NS/`\`先頭を考慮してFQCNに正規化 | Med |
| Struct | PhpInheritanceResolver | pub | 継承/実装/トレイトの関係管理、MROに基づくメソッド解決 | Med |
| impl Trait | InheritanceResolver for PhpInheritanceResolver | pub（トレイト経由） | 継承追加、サブタイプ判定、メソッド解決、チェーン取得 | Med |
| Type alias | UseStatement | private | (alias, full_path) | Low |
| Type alias | NamespaceUses | private | alias→(alias, full_path) | Low |
| Default impl | Default for PhpInheritanceResolver | pub | new()の委譲 | Low |

### Dependencies & Interactions

- 内部依存
  - `PhpResolutionContext::resolve` → `resolve_name`（名前をFQCNへ正規化）
  - `populate_imports` → `add_use_statement`（Importから`use`文へ）
- 外部依存

| 外部 | 用途 | 備考 |
|-----|-----|-----|
| crate::parsing::resolution::ImportBinding | import_bindingsの登録/取得 | データ契約の詳細は不明 |
| crate::parsing::{InheritanceResolver, ResolutionScope, ScopeLevel, ScopeType} | トレイト実装・スコープ種別 | トレイト仕様の全容は不明 |
| crate::{FileId, SymbolId} | 識別子型 | 実体と整合性は不明 |
| std::collections::HashMap | 各種スコープ/関連づけ | 標準コンテナ |

- 被依存推定
  - PHPパーサ/ASTビルダーが、スコープ出入りやシンボル登録に`PhpResolutionContext`を利用
  - 型情報・メンバー抽出後、`PhpInheritanceResolver`に継承・トレイト情報とメソッド一覧を供給

## API Surface (Public/Exported) and Data Contracts

### API一覧

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PhpResolutionContext::new | fn new(file_id: FileId) -> Self | コンテキスト生成 | O(1) | O(1) |
| PhpResolutionContext::set_namespace | fn set_namespace(&mut self, namespace: String) | 現在NS設定 | O(1) | O(|namespace|) |
| PhpResolutionContext::add_use_statement | fn add_use_statement(&mut self, alias: Option<String>, full_path: String) | use文登録 | O(1) | O(|alias|+|full_path|) |
| ResolutionScope::add_symbol | fn add_symbol(&mut self, name: String, symbol_id: SymbolId, scope_level: ScopeLevel) | シンボル登録 | O(1) | O(|name|) |
| ResolutionScope::resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | 名前解決 | 平均O(1)、分岐 | O(1) |
| ResolutionScope::clear_local_scope | fn clear_local_scope(&mut self) | ローカル消去 | O(n_local) | O(1) |
| ResolutionScope::enter_scope | fn enter_scope(&mut self, scope_type: ScopeType) | スコープ開始 | O(n_clear) | O(1) |
| ResolutionScope::exit_scope | fn exit_scope(&mut self) | スコープ終了 | O(n_clear) | O(1) |
| ResolutionScope::symbols_in_scope | fn symbols_in_scope(&self) -> Vec<(String, SymbolId, ScopeLevel)> | スコープ内列挙 | O(n_all) | O(n_all) |
| ResolutionScope::populate_imports | fn populate_imports(&mut self, imports: &[crate::parsing::Import]) | Import→use変換 | O(m) | O(m) |
| ResolutionScope::register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | ImportBinding登録 | O(1) | O(|name|) |
| ResolutionScope::import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | ImportBinding取得 | O(1) | O(1) |
| PhpInheritanceResolver::default | fn default() -> Self | 既定初期化 | O(1) | O(1) |
| PhpInheritanceResolver::new | fn new() -> Self | 初期化 | O(1) | O(1) |
| PhpInheritanceResolver::add_class_extends | fn add_class_extends(&mut self, class: String, parent: String) | extends登録 | O(1) | O(|class|+|parent|) |
| PhpInheritanceResolver::add_class_implements | fn add_class_implements(&mut self, class: String, interfaces: Vec<String>) | implements登録 | O(k) | O(total) |
| PhpInheritanceResolver::add_class_uses | fn add_class_uses(&mut self, class: String, traits: Vec<String>) | trait使用登録 | O(k) | O(total) |
| PhpInheritanceResolver::add_interface_extends | fn add_interface_extends(&mut self, interface: String, parents: Vec<String>) | interface継承登録 | O(k) | O(total) |
| PhpInheritanceResolver::add_trait_methods | fn add_trait_methods(&mut self, trait_name: String, methods: Vec<String>) | トレイトのメソッド一覧登録 | O(k) | O(total) |
| InheritanceResolver::add_inheritance | fn add_inheritance(&mut self, child: String, parent: String, kind: &str) | 継承種別に応じた登録 | O(1)〜O(k) | O(1) |
| InheritanceResolver::resolve_method | fn resolve_method(&self, type_name: &str, method_name: &str) -> Option<String> | メソッド解決（MRO） | O(depth + traits) | O(1) |
| InheritanceResolver::get_inheritance_chain | fn get_inheritance_chain(&self, type_name: &str) -> Vec<String> | 継承・implements・traits列挙 | O(depth + I + T) | O(depth + I + T) |
| InheritanceResolver::is_subtype | fn is_subtype(&self, child: &str, parent: &str) -> bool | サブタイプ判定 | O(depth + I + T) | O(1) |
| InheritanceResolver::add_type_methods | fn add_type_methods(&mut self, type_name: String, methods: Vec<String>) | 型のメソッド一覧登録 | O(k) | O(total) |
| InheritanceResolver::get_all_methods | fn get_all_methods(&self, type_name: &str) -> Vec<String> | 自身/traits/親からメソッド集約 | O(depth + traits + total_methods) | O(unique_methods) |

注: ここでのTime/Spaceは概算。`resolve`は多数分岐を含みます。

### 主要API詳細

1) PhpResolutionContext（ResolutionScope::resolve）

- 目的と責務
  - **PHPの名前解決**を行う。優先順位は「ローカル→クラス→名前空間→グローバル」。`use`文と現在NS、FQCN（先頭`\`）を考慮。`::`を含む静的アクセスの特例あり。
- アルゴリズム
  1. `local_scope`を検索
  2. `class_scope`を検索
  3. `resolve_name(name)`でFQCNへ正規化し、`namespace_scope`→`global_scope`の順に検索
  4. 生の`name`で`namespace_scope`→`global_scope`を検索
  5. `name.contains("::")`なら
     - まず完全な`"A\\B::m"`として`namespace_scope`→`global_scope`を検索
     - 見つからなければ`"A\\B"`と`"m"`に分解
       - `self.resolve("A\\B")`で型の存在確認
       - 存在すれば`self.resolve("m")`でメソッド/定数名を再解決（所属の関連を見ない）
       - 存在しなければ`None`
- 引数

| 名前 | 型 | 説明 |
|------|----|------|
| name | &str | 解決対象の名前（FQCN/相対/`use`エイリアス/`::`含む可能性） |

- 戻り値

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 見つかればシンボルID、なければNone |

- 使用例
```rust
let mut ctx = PhpResolutionContext::new(file_id);
ctx.set_namespace("App\\Controllers".to_string);
ctx.add_use_statement(None, "App\\Services\\Auth".to_string()); // alias=Auth
// 登録（擬似）
ctx.add_symbol("index".to_string(), SymbolId(1), ScopeLevel::Local);
ctx.add_symbol("\\App\\Services\\Auth".to_string(), SymbolId(2), ScopeLevel::Package);

// 解決
assert_eq!(ctx.resolve("index"), Some(SymbolId(1)));
assert_eq!(ctx.resolve("Auth"), Some(SymbolId(2))); // useによりFQCNへ
assert_eq!(ctx.resolve("\\App\\Services\\Auth"), Some(SymbolId(2))); // FQCN
```

- エッジケース
  - `\`始まりのFQCNはそのまま扱う
  - `use`エイリアスが衝突した場合、後勝ちで上書き
  - `A::m`でAが存在するがmが別スコープの同名シンボルに解決される可能性
  - 現在NS未設定時は相対名は生の文字列で探索される

2) PhpResolutionContext::add_use_statement

- 目的と責務
  - `use`文の登録。alias未指定時は**最後の名前片**をデフォルトエイリアスにする。
- アルゴリズム
  - aliasがSomeならそのキーで、Noneなら`full_path.rsplit('\\').next().unwrap_or(&full_path)`をキーにして登録
- 引数

| 名前 | 型 | 説明 |
|------|----|------|
| alias | Option<String> | エイリアス（省略時は末尾名） |
| full_path | String | FQCN（例: "App\\Services\\Auth"） |

- 戻り値

| 型 | 説明 |
|----|------|
| () | なし |

- 使用例
```rust
ctx.add_use_statement(None, "Vendor\\Pkg\\HttpClient".to_string()); // alias=HttpClient
ctx.add_use_statement(Some("Client".to_string()), "Vendor\\Pkg\\HttpClient".to_string()); // alias=Client
```

- エッジケース
  - `full_path`に`\\`を含まない場合、エイリアスは`full_path`全体
  - 同じキーの再登録は**上書き**

3) PhpInheritanceResolver（InheritanceResolver::resolve_method）

- 目的と責務
  - PHPのメソッド解決順序に従い、どの型（クラス/トレイト）がメソッドを提供するかを返す
- アルゴリズム
  1. 自身の`type_methods[type_name]`に`method_name`があれば`type_name`を返す
  2. `class_uses_traits[type_name]`の列挙を**後ろから**見て、`trait_methods[trait]`にあれば`trait`名を返す
  3. `class_extends[type_name]`があれば親に対して再帰
  4. 見つからなければ`None`
- 引数

| 名前 | 型 | 説明 |
|------|----|------|
| type_name | &str | 型名 |
| method_name | &str | メソッド名 |

- 戻り値

| 型 | 説明 |
|----|------|
| Option<String> | 実際にメソッドを提供する型名（クラスorトレイト） |

- 使用例
```rust
let mut inh = PhpInheritanceResolver::new();
inh.add_type_methods("Base".to_string(), vec!["foo".to_string()]);
inh.add_class_extends("Child".to_string(), "Base".to_string());
inh.add_class_uses("Child".to_string(), vec!["LogTrait".to_string()]);
inh.add_trait_methods("LogTrait".to_string(), vec!["log".to_string(), "foo".to_string()]); // トレイトがfooを上書き

assert_eq!(inh.resolve_method("Child", "log"), Some("LogTrait".to_string()));
assert_eq!(inh.resolve_method("Child", "foo"), Some("LogTrait".to_string())); // 後勝ち
```

- エッジケース
  - 継承が循環していると無限再帰の可能性（訪問検知なし）
  - インターフェイスのメソッドは通常抽象のためこのロジックでは考慮外（このチャンクではインターフェイスに対するメソッド格納はなし）

4) PhpInheritanceResolver::get_all_methods

- 目的と責務
  - 自身、使用トレイト、親クラスから**重複排除**しつつメソッド一覧を収集
- アルゴリズム
  - 自身→traits→親（再帰）と追加。`seen`で重複排除
- 引数/戻り値
  - 引数: `type_name: &str`
  - 戻り値: `Vec<String>`
- 使用例
```rust
let methods = inh.get_all_methods("Child");
assert!(methods.contains(&"foo".to_string()));
assert!(methods.contains(&"log".to_string()));
```
- エッジケース
  - 親連鎖の循環で無限再帰の可能性（訪問検知はこの関数内にはない）

データ契約（主要マップの意味）
- `local_scope`/`class_scope`/`namespace_scope`/`global_scope`: 文字列キーで`SymbolId`を保持。キーは**FQCNの場合と生名の場合が混在**し得る。
- `use_statements`: alias→(alias, full_path)。解決は**エイリアスの一致**または冒頭片一致+連結。
- 継承関連:
  - `class_extends`: 子→親（単一）
  - `class_implements`: クラス→インターフェイス群
  - `class_uses_traits`: クラス→使用トレイト群（リストの末尾が最も強い）
  - `type_methods`: 型→メソッド名配列
  - `trait_methods`: トレイト→メソッド名配列

## Walkthrough & Data Flow

典型的なフロー（名前解決）
1. `PhpResolutionContext::new(file_id)`で作成
2. `set_namespace("App\\Controllers")`
3. `populate_imports(&[Import{alias: None, path: "App\\Services\\Auth"}])` → `add_use_statement`でalias=Authを登録
4. `enter_scope(ScopeType::Class)`→クラススコープ初期化
5. `add_symbol("\\App\\Services\\Auth".to_string(), sid, ScopeLevel::Package)`（例：名前空間スコープに型登録）
6. `resolve("Auth")`→`resolve_name`で`Auth`→`"App\\Services\\Auth"`に正規化→`namespace_scope`ヒット

典型的なフロー（メソッド解決）
1. `PhpInheritanceResolver::new()`
2. クラス・トレイト・メソッドを各`add_*`で登録
3. 実行時に`resolve_method("Child", "m")`を呼び、順序に従い提供元を決定

Mermaid（名前解決の主要分岐）
```mermaid
flowchart TD
  A[resolve(name)] --> B{local_scope.contains(name)?}
  B -- Yes --> R1[return local id]
  B -- No --> C{class_scope.contains(name)?}
  C -- Yes --> R2[return class id]
  C -- No --> D[full_name = resolve_name(name)]
  D --> E{namespace_scope.contains(full_name)?}
  E -- Yes --> R3[return ns id]
  E -- No --> F{global_scope.contains(full_name)?}
  F -- Yes --> R4[return global id]
  F -- No --> G{namespace_scope.contains(name)?}
  G -- Yes --> R5[return ns id(raw)]
  G -- No --> H{global_scope.contains(name)?}
  H -- Yes --> R6[return global id(raw)]
  H -- No --> I{name.contains("::")?}
  I -- No --> R7[return None]
  I -- Yes --> J{ns/global contains full 'A\\B::m'?}
  J -- Yes --> R8[return id]
  J -- No --> K[parts = split("::")]
  K --> L{len(parts)==2?}
  L -- No --> R7
  L -- Yes --> M{resolve(parts[0]).is_some()?}
  M -- Yes --> N[return resolve(parts[1])]
  M -- No --> R7
```
上記の図は`resolve`関数の主要分岐を示す（行番号: 不明、関数名で特定）。

## Complexity & Performance

- PhpResolutionContext::resolve
  - 時間計算量: 平均O(1)（HashMap検索複数回 + 短い文字列操作）。`::`分岐時は最大で**2回の再帰的呼出**（`resolve(class)`と`resolve(member)`）。
  - 空間計算量: O(1) 追加分岐のみ（返却用）。
  - ボトルネック: 多数のHashMapを順番に検索。巨大スコープ（特に`namespace_scope`/`global_scope`）でのキャッシュ・FQCN正規化の冪等性に注意。
- PhpInheritanceResolver::resolve_method
  - 時間: O(depth + traits)（継承深さ + 使用トレイト数）。循環時は無限。
  - 空間: O(1)
- PhpInheritanceResolver::get_all_methods
  - 時間: O(depth + traits + total_methods)
  - 空間: O(unique_methods)（`seen`集合）

スケール限界・実運用要因
- 大規模コードベースでは`namespace_scope`/`global_scope`のサイズが肥大化し、**キー文字列の正規化/比較コスト**が支配的に。
- PSR-4に基づく**オンデマンドロード**や**インデックス**がないため、未知シンボルは常に検索失敗で終了（このチャンクではオートロードは不明）。

## Edge Cases, Bugs, and Security

### 詳細エッジケース表

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| FQCN先頭バックスラッシュ | "\\App\\X\\Y" | そのままグローバル扱いで検索 | resolve_nameでSome(name) | OK |
| use未指定の相対名 | "Auth" | 現在NSに相対解決 | resolve_nameでns付与 | OK |
| useエイリアス衝突 | alias="Auth"を二重登録 | 後勝ち上書き（警告が望ましい） | 上書き | リスク（警告なし） |
| 一部修飾名 | "Auth\\Login" | useの先頭片一致で連結 | resolve_nameでpos検索→連結 | OK |
| "::"完全修飾 | "App\\S\\A::m" | 完全文字列キーで検索 | まず完全キーで検索 | OK |
| "::"分割後メソッド解決 | "A::m"でA存在 | Aのメンバmに限定して解決 | 実装は`resolve("m")`で所属無視 | バグ可能性 |
| クラス継承循環 | A extends B、B extends A | 安全に停止 | resolve_method/is_subtypeはvisitedなし | バグ（無限再帰） |
| 大小文字差 | "Auth" vs "auth" | PHP的には等価に扱う | 文字列キーは大小区別 | リスク（仕様不一致） |
| current_class未使用 | なし | 仕様通り利用 | フィールドのみ存在 | 未使用（不明） |
| インターフェイスのメソッド | interfaceに抽象メソッド | 解決は抽象扱い | type_methodsに登録なければ未考慮 | 仕様準拠か不明 |

### セキュリティチェックリスト

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 文字列/HashMap操作のみで、**unsafe不使用**。該当なし。
- インジェクション
  - SQL/Command/Path traversal: 本ロジックは解析データのみ扱い、外部実行なし。該当なし。
- 認証・認可
  - 権限チェック漏れ/セッション固定: パーサ内部コンポーネントで認可文脈なし。該当なし。
- 秘密情報
  - Hard-coded secrets / Log leakage: ログも秘密も扱わない。該当なし。
- 並行性
  - Race condition / Deadlock: **&mut自己参照前提**。マルチスレッド共有で同期がなければ競合の可能性。Send/Syncは型に依存（不明）。

## Design & Architecture Suggestions

- 名前正規化の一貫化
  - すべての登録キーを**FQCNで統一**し、`resolve`は常にFQCN化してから検索する方針にすると分岐が簡略化。
- `Class::member`解決の改善
  - クラス存在確認後は、メンバ検索を**そのクラスのスコープで限定**または`(class, member)`二要素キー等に変更。
- 継承・トレイトの循環検出
  - `resolve_method`/`is_subtype`/`get_all_methods`に**visited集合**を導入し、循環時の停止を保証。
- PHP大小文字非依存対応
  - キーを**正規化（例: lowercase）**して保存/検索するか、**ケースインセンシティブ比較**を導入。
- 不要フィールド整理
  - `current_class`はこのチャンクでは未使用。意図がなければ削除か、**スコープ管理に活用**（enter_scope(Class)で設定等）。
- `use`エイリアス衝突の可視化
  - 上書き時に**警告ロギング**や重複検知を追加。
- PSR-4
  - 説明にあるPSR-4の**具体実装（パス→名前空間マップ、オートロードフック）**を別モジュールで提供（このチャンクでは不明）。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト（名前解決）
  - エイリアス/現在NS/`\`始まり/FQCN/`::`の各ケース

```rust
#[test]
fn resolves_with_use_and_namespace() {
    let file_id = FileId(1);
    let mut ctx = PhpResolutionContext::new(file_id);
    ctx.set_namespace("App\\Controllers".to_string());
    ctx.add_use_statement(None, "App\\Services\\Auth".to_string()); // alias=Auth

    // 登録（FQCN）
    let sid_auth = SymbolId(42);
    ctx.add_symbol("\\App\\Services\\Auth".to_string(), sid_auth, ScopeLevel::Package);

    assert_eq!(ctx.resolve("Auth"), Some(sid_auth));                // use alias
    assert_eq!(ctx.resolve("\\App\\Services\\Auth"), Some(sid_auth)); // FQCN
    assert_eq!(ctx.resolve("App\\Services\\Auth"), None);           // 相対名→NS付与→未登録ならNone
}

#[test]
fn resolves_static_member_safely() {
    let mut ctx = PhpResolutionContext::new(FileId(1));
    // 擬似登録
    ctx.add_symbol("\\Lib\\Math".to_string(), SymbolId(1), ScopeLevel::Global);
    ctx.add_symbol("sum".to_string(), SymbolId(2), ScopeLevel::Module); // 仮: クラススコープ登録

    // 現状仕様では所属無視でmethod名解決
    assert_eq!(ctx.resolve("Lib\\Math::sum"), Some(SymbolId(2)));
}
```

- 単体テスト（継承/トレイト）
  - 後勝ちトレイト、親へのフォールバック、循環検出（将来改善用）

```rust
#[test]
fn trait_override_order() {
    let mut inh = PhpInheritanceResolver::new();
    inh.add_type_methods("Base".to_string(), vec!["foo".to_string()]);
    inh.add_class_extends("Child".to_string(), "Base".to_string());
    inh.add_class_uses("Child".to_string(), vec!["T1".to_string(), "T2".to_string()]);
    inh.add_trait_methods("T1".to_string(), vec!["foo".to_string()]);
    inh.add_trait_methods("T2".to_string(), vec!["bar".to_string(), "foo".to_string()]);

    // 後勝ち（T2が優先）
    assert_eq!(inh.resolve_method("Child", "foo"), Some("T2".to_string()));
    // 自身にない→トレイトから
    assert_eq!(inh.resolve_method("Child", "bar"), Some("T2".to_string()));
    // 親から
    assert_eq!(inh.resolve_method("Child", "BaseOnly"), None); // 未登録
}
```

- 境界テスト
  - エイリアス衝突、大小文字差、`::`でメンバ錯誤解決の検出（期待動作の確認/改善後のテスト計画）

## Refactoring Plan & Best Practices

1. 名前キーの正規化
   - 登録時/解決時に**FQCN化＋ケース正規化**（lowercase）を行い、一貫性を保証。
2. `resolve`の分岐簡略化
   - 検索順序を関数化し、`HashMap`集合を配列化して共通ループで検索。`::`処理は専用関数に分離。
3. クラスメンバの関連解決
   - `Class::member`は`(class, member)`をキーにするか、クラス別のメンバテーブルを導入して**所属を強制**。
4. 循環検出の導入
   - `resolve_method`/`is_subtype`/`get_all_methods`で**visited集合**を使い、無限再帰防止。
5. ロギング/トレースの追加（次章参照）
6. 未使用フィールドの削除または活用（`current_class`）
7. 将来拡張: PSR-4の統合（このチャンクでは不明）を別モジュールに、ここから依存注入する形に。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - `resolve`で分岐ごとの結果（どのスコープでヒットしたか、`::`処理のパス）を**debugログ**で出力
  - `add_use_statement`で**エイリアス衝突警告**をwarnログ
- メトリクス
  - 解決ヒット率（local/class/ns/global）、`::`処理の成功/失敗回数、`use`登録数
- トレーシング
  - `resolve`にspanを入れ、name/FQCN/スコープ順をタグ化
  - 継承解決にも`resolve_method`のspan（trait優先/親フォールバック）を付与

## Risks & Unknowns

- 仕様不一致
  - **PHPの大小文字非依存**未対応。これにより実コードとの不一致が発生し得る。
- 循環
  - 継承循環で`resolve_method`/`is_subtype`/`get_all_methods`が**無限再帰**しうる。
- PSR-4
  - 説明にはあるが、このチャンクでは**具体的なオートロード/マッピング機能は不明**。
- 依存型の詳細不明
  - `SymbolId`/`ImportBinding`/トレイト仕様の詳細がこのチャンクにないため、**完全な契約は不明**。
- スレッド安全性
  - `HashMap`を可変参照で扱うため、**並行アクセスは外側での同期が前提**。Send/Syncは型に依存（不明）。
- current_class
  - フィールドは存在するが、このチャンクでは**参照箇所がなく意図不明**。利用設計は不明。