# parsing/csharp/resolution.rs Review

## TL;DR

- 目的: C#のスコープ規則（ローカル→メンバー→名前空間→using→グローバル→修飾名）に従ってシンボルを解決するための文脈コンテキストと実装を提供
- 主要公開API: CSharpResolutionContext::new, add_using, add_import_symbol, add_symbol_with_context、および ResolutionScope トレイト実装（resolve, add_symbol, enter_scope, exit_scope 等）
- コアロジック: resolveは複数のスコープを定順で探索し、ドット区切りの修飾名とusingエイリアスも部分的に処理
- 重大なバグ: 「Type.Member」形式の解決が誤っており、メンバー名を裸で再解決してしまうため誤解決・シャドーイングが起きうる（resolve）
- 重大な欠落: symbols_in_scopeがmember_scopeを列挙に含めていないため、メンバーシンボルが「可視シンボル一覧」に出ない
- 設計上のギャップ: usingの静的インポート/型エイリアス、3階層以上の修飾名、クラス境界でのmember_scopeの寿命管理が未対応
- 安全性/並行性: unsafeなし、Option中心でパニック無し。マルチスレッド安全性は型境界次第（明示はなし）

## Overview & Purpose

このファイルは、C#特有の名前解決（スコープ・using・名前空間・修飾名）を行うための状態と振る舞いを提供します。CSharpResolutionContext は、各スコープ（ローカル、メンバー、名前空間、インポート、グローバル）用のシンボルテーブルを保持し、ResolutionScope トレイトの実装を通じて統一的なAPI（resolve他）を提供します。

解決順序はC#仕様に合わせて定義され、ローカル最優先、次にメンバー、名前空間、usingインポート、グローバルと進み、最後に修飾名の解決が行われます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | CSharpResolutionContext | pub | C#向けのスコープ別シンボルテーブルとusing情報の保持、解決ロジックの中核 | Medium |
| Impl | impl CSharpResolutionContext | crate内公開メソッド | 生成、using追加、インポート登録、スコープ文脈に応じたシンボル登録 | Low |
| Trait Impl | impl ResolutionScope for CSharpResolutionContext | 公開トレイトの実装 | 追加・解決・スコープ遷移・関係解決・インポートバインディング管理 | Medium |

フィールド概要:
- local_scope: ローカル（パラメータ・ローカル変数）
- member_scope: クラス/構造体メンバー
- namespace_symbols: 現在の名前空間スコープ
- imported_symbols: usingで可視になったシンボル
- global_symbols: グローバル
- scope_stack: スコープ遷移追跡（ScopeType）
- using_directives: (namespace, alias)のリスト
- import_bindings: ImportBinding（可視名→バインディング）索引用

### Dependencies & Interactions

- 内部依存
  - resolveは各スコープ用HashMapに対して順次検索を行い、必要に応じて自身を再帰呼び出し（Qualified名やエイリアス解決）。
  - exit_scopeはscope_stackをpopし、条件によりlocal_scopeをクリア。
  - symbols_in_scopeは現在の複数スコープの統合ビューを生成（ただしmember_scopeを含まないバグあり）。
  - register_import_binding/import_bindingはimport_bindingsを介してインポート情報を提供。

- 外部依存

| 依存 | 用途 | 備考 |
|------|------|------|
| crate::parsing::resolution::ImportBinding | インポートのバインディング情報保持 | get/cloneで値を返却 |
| crate::parsing::resolution::ResolutionScope | 共通解決インターフェイス | 本実装がトレイトを実装 |
| crate::parsing::{ScopeLevel, ScopeType} | スコープレベルとスコープ種別 | クリア判定・登録先切替 |
| crate::{FileId, SymbolId} | ファイルIDとシンボルID | キー/値として利用 |
| std::collections::HashMap | シンボルテーブル | O(1)平均探索 |
| std::any::Any | as_any_mutのダウンキャスト用 | トレイトオブジェクト補助 |

- 被依存推定
  - C#パーサ/アナライザでの名前解決フェーズ
  - クロスリファレンス/参照解決機能（RelationKindを使う依存グラフ構築）
  - IDE機能（Go to Definition, Symbol Highlight）

## API Surface (Public/Exported) and Data Contracts

公開要素（推定: exports=5）:
- Struct: CSharpResolutionContext
- Methods (inherent, pub): new, add_using, add_import_symbol, add_symbol_with_context
- ResolutionScopeトレイト実装（外部からはトレイト経由で呼び出され得る）

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CSharpResolutionContext::new | fn new(file_id: FileId) -> Self | 空コンテキスト生成 | O(1) | O(1) |
| add_using | fn add_using(&mut self, namespace: String, alias: Option<String>) | usingディレクティブ追加 | O(1) | O(1) |
| add_import_symbol | fn add_import_symbol(&mut self, name: String, symbol_id: SymbolId, _is_type_only: bool) | usingで可視になったシンボル登録 | O(1) | O(1) |
| add_symbol_with_context | fn add_symbol_with_context(&mut self, name: String, symbol_id: SymbolId, scope_context: Option<&crate::symbol::ScopeContext>) | スコープ文脈に基づくシンボル登録 | O(1) | O(1) |
| as_any_mut | fn as_any_mut(&mut self) -> &mut dyn Any | ダウンキャスト補助 | O(1) | O(1) |
| add_symbol | fn add_symbol(&mut self, name: String, symbol_id: SymbolId, scope_level: ScopeLevel) | スコープレベルに応じた登録 | O(1) | O(1) |
| resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | 名称解決（C#順序・修飾名対応） | O(1)+O(k) | O(1) |
| clear_local_scope | fn clear_local_scope(&mut self) | ローカルスコープのクリア | O(n_local) | O(1) |
| enter_scope | fn enter_scope(&mut self, scope_type: ScopeType) | スコープ入場 | O(1) | O(1) |
| exit_scope | fn exit_scope(&mut self) | スコープ退出と必要に応じローカルクリア | O(1)+O(n_local) | O(1) |
| symbols_in_scope | fn symbols_in_scope(&self) -> Vec<(String, SymbolId, ScopeLevel)> | 可視シンボル一覧生成 | O(N) | O(N) |
| resolve_relationship | fn resolve_relationship(&self, _from_name: &str, to_name: &str, kind: crate::RelationKind, _from_file: FileId) -> Option<SymbolId> | 関係種別に応じたターゲット解決（実質resolve委譲） | O(1)+O(k) | O(1) |
| register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | ImportBinding登録 | O(1) | O(1) |
| import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | ImportBinding取得 | O(1) | O(1) |

k: using_directivesの件数、N: 合計シンボル件数（列挙対象）

以下、主要APIの詳細:

1) CSharpResolutionContext::new
- 目的と責務: 新規の解決コンテキスト生成
- アルゴリズム: フィールドを空のHashMap/Vecで初期化
- 引数:
  - file_id: FileId | ファイル識別子
- 戻り値:
  - Self | 初期化済みコンテキスト
- 使用例:
```rust
let mut ctx = CSharpResolutionContext::new(file_id);
```
- エッジケース:
  - 特になし（単純初期化）

2) add_using
- 目的と責務: usingディレクティブ（名前空間と任意のエイリアス）を記録
- アルゴリズム: using_directivesにpush
- 引数:

| 引数 | 型 | 意味 |
|------|----|------|
| namespace | String | 参照する名前空間名（例: "System.Text"） |
| alias | Option<String> | usingエイリアス（例: Some("Text")） |

- 戻り値: なし
- 使用例:
```rust
ctx.add_using("System.Text".to_string(), Some("Text".to_string()));
```
- エッジケース:
  - エイリアスが重複しても上書きや検出はしない

3) add_import_symbol
- 目的と責務: usingにより可視になったシンボルを登録
- アルゴリズム: imported_symbolsにinsert。_is_type_onlyはC#では無視
- 引数:

| 引数 | 型 | 意味 |
|------|----|------|
| name | String | 可視名 |
| symbol_id | SymbolId | 対応シンボルID |
| _is_type_only | bool | C#では意味なし |

- 戻り値: なし
- 使用例:
```rust
ctx.add_import_symbol("List".into(), sid(1), false);
```
- エッジケース:
  - 同名の上書き競合検知なし

4) add_symbol_with_context
- 目的と責務: ScopeContextに応じて適切なスコープへシンボルを登録
- アルゴリズム:
  - ScopeContext::Local/Parameter → local_scope
  - ScopeContext::ClassMember → member_scope
  - ScopeContext::Module → namespace_symbols
  - ScopeContext::Package → imported_symbols
  - ScopeContext::Global → global_symbols
  - None → local_scope
- 引数:

| 引数 | 型 | 意味 |
|------|----|------|
| name | String | シンボル名 |
| symbol_id | SymbolId | ID |
| scope_context | Option<&crate::symbol::ScopeContext> | 文脈 |

- 戻り値: なし
- 使用例:
```rust
// 仮: Classのメンバーとして登録
ctx.add_symbol_with_context("Count".into(), sid(2), Some(&crate::symbol::ScopeContext::ClassMember));
```
- エッジケース:
  - 不明なScopeContext分岐はこのチャンクには現れない

5) resolve（ResolutionScopeトレイト）
- 目的と責務: C#の解決順序に沿ってnameをSymbolIdに解決
- アルゴリズム（簡略）:
  1. local_scope → member_scope → namespace_symbols → imported_symbols → global_symbols の順に探索
  2. '.'を含む場合: 完全修飾名としてimported/namespace/globalから直接一致探索
  3. 2パート（Type.Member）の場合:
     - 左辺type_nameが解決できれば、なぜかmember_nameを裸で再解決（バグ）
     - usingエイリアス一致時は namespace + '.' + member_name を再解決
- 引数:

| 引数 | 型 | 意味 |
|------|----|------|
| name | &str | 解決対象名 |

- 戻り値:

| 型 | 意味 |
|----|------|
| Option<SymbolId> | 成功時Some、失敗時None |

- 使用例:
```rust
assert!(ctx.resolve("List").is_some());
assert!(ctx.resolve("System.Text.StringBuilder").is_some()); // 完全修飾名が登録されていれば
```
- エッジケース:
  - "A.B.C"の3パート以上の分解解決は未対応（直接登録がないと解決不可）
  - Type.Memberの処理が誤っており誤解決の可能性あり（後述）

6) その他トレイトAPI
- add_symbol: ScopeLevelで挿入先を切替（Local/Module/Package/Global）
- enter_scope/exit_scope: scope_stack管理。exit時、topがNone/Module/Globalならlocal_scopeをクリア
- clear_local_scope: ローカルを空に
- symbols_in_scope: 現在可視のシンボル一覧を返すがmember_scopeが含まれない（欠落）
- resolve_relationship: RelationKindに依らず実質resolve委譲
- register_import_binding / import_binding: import_bindingsを登録・取得

## Walkthrough & Data Flow

1. 準備
   - CSharpResolutionContext::newで空の各スコープテーブルとusing_directives/stackを生成。

2. シンボルの投入
   - add_symbol_with_contextやadd_symbolで、適切なテーブル（local/member/namespace/imported/global）にinsert。
   - add_usingでusing_directivesに(namespace, alias)をpush。
   - register_import_bindingでImportBindingを登録（import_bindingで取得可能）。

3. 解決フロー（resolve）
   - 直線探索: local → member → namespace → imported → global の順でHashMapをlookup。
   - 修飾名:
     - まず完全一致でimported/namespace/globalを探索。
     - "Type.Member" の形なら左辺Typeが解決可能か確認。可能なら「member_nameを裸で再解決」する（バグ）。次に、usingエイリアスが左辺に合致する場合は「aliasが指すnamespace + '.' + member」を再解決。

4. スコープ遷移
   - enter_scopeでscope_stackにpush。
   - exit_scopeでpop後、stackが空 or 先頭がModule/Globalならlocal_scopeをクリア。これによりメソッド/ブロック脱出時にローカルが掃除される想定。

5. 一覧取得
   - symbols_in_scopeでローカル、インポート、名前空間、グローバルのシンボルを列挙（メンバーが欠落）。

### Mermaid Flowchart: resolveの主要分岐

```mermaid
flowchart TD
  A[resolve(name)] --> B{local_scopeに存在?}
  B -- Yes --> Z1[返す]
  B -- No --> C{member_scopeに存在?}
  C -- Yes --> Z1
  C -- No --> D{namespace_symbolsに存在?}
  D -- Yes --> Z1
  D -- No --> E{imported_symbolsに存在?}
  E -- Yes --> Z1
  E -- No --> F{global_symbolsに存在?}
  F -- Yes --> Z1
  F -- No --> G{nameに'.'を含む?}
  G -- No --> Z2[None]
  G -- Yes --> H{完全修飾名でimported / namespace / globalに存在?}
  H -- Yes --> Z1
  H -- No --> I{nameを'.'で分割し長さ==2?}
  I -- No --> Z2
  I -- Yes --> J[parts[0]=type, parts[1]=member]
  J --> K{typeがresolve可能?}
  K -- Yes --> L[member名を裸でresolve（バグ）]
  K -- No --> M{usingエイリアスalias==type?}
  M -- Yes --> N[qualified = namespace+'.'+member; resolve(qualified)]
  M -- No --> Z2
```

上記の図はresolve関数（行番号不明）の主要分岐を示す。

該当コード抜粋（バグ箇所の文脈）:
```rust
// 6. Handle qualified names (Namespace.Type or Type.Member)
if name.contains('.') {
    // First try to resolve the full qualified name
    if let Some(&id) = self.imported_symbols.get(name) { return Some(id); }
    if let Some(&id) = self.namespace_symbols.get(name) { return Some(id); }
    if let Some(&id) = self.global_symbols.get(name) { return Some(id); }

    // Try to resolve as Type.Member
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() == 2 {
        let type_name = parts[0];
        let member_name = parts[1];

        // Check if we have the type in our scope
        if self.resolve(type_name).is_some() {
            // Type exists, try to resolve the member
            return self.resolve(member_name); // ← 裸のメンバー名を再解決（衝突・誤解決の可能性）
        }

        // Check using aliases
        for (namespace, alias) in &self.using_directives {
            if let Some(alias_name) = alias {
                if alias_name == type_name {
                    let qualified_name = format!("{namespace}.{member_name}");
                    return self.resolve(&qualified_name);
                }
            }
        }
    }
}
```

## Complexity & Performance

- 単発解決（resolve）
  - 時間計算量: 平均O(1)のHashMap探索を最大5回＋「完全修飾名」の3回＋エイリアス走査O(k)（k = using_directives数）。最悪 O(k) 程度。
  - 空間計算量: 追加のデータ構造はほぼ定数。Vec分割など微小。
- シンボル追加（add_*）
  - 時間: O(1)（HashMap insert）
  - 空間: O(1)増
- 一覧生成（symbols_in_scope）
  - 時間: O(N)（登録済みシンボル総数）
  - 空間: O(N)（Vecにコピー）
- スケール限界/ボトルネック
  - using_directivesが多い場合、修飾名の別名解決が線形に増加
  - 再帰resolveを多用しており、深い修飾名やエイリアスチェーンには弱い
  - キャッシュ（メモ化）が無い

I/O/ネットワーク/DB: 関与なし（純粋なメモリ内処理）

## Edge Cases, Bugs, and Security

- バグ/仕様欠落の主な指摘（根拠: resolve/exit_scope/symbols_in_scope; 行番号不明）
  - Type.Member解決バグ: 左辺Typeが存在するだけでmember_nameを裸で再解決してしまい、同名ローカル等に誤マッチする危険
  - 3階層以上の修飾名未対応: "A.B.C"の分割再解決がない
  - symbols_in_scopeにmember_scopeが含まれない: ユーザ視点の可視一覧に欠落
  - クラス/名前空間境界でのmember_scope/namespace_symbolsの寿命管理がない: スコープ離脱時のクリア方針がlocal_scopeのみに限られ、漏れ込みの可能性
  - using静的インポートや型エイリアスの表現不足: C#の多様なusing形態を網羅していない
  - add_import_symbolの_is_type_only無視はコメントで明示されるが、将来互換性に注意
  - resolve_relationshipがRelationKindをほぼ無視: 種別特化ロジック不在

- セキュリティチェックリスト
  - メモリ安全性: unsafe未使用、標準コレクションのみでUse-after-free/Buffer overflowの懸念なし
  - Integer overflow: なし
  - インジェクション（SQL/Command/Path traversal）: 対象外（内部解決のみ）
  - 認証・認可: 対象外
  - 秘密情報: ハードコード秘密やログ漏えいなし
  - 並行性: 共有可変状態があるため、外部で同一インスタンスに並行アクセスすればデータ競合余地あり（Sync/Send境界は型次第）

- Rust特有の観点
  - 所有権/借用: &self / &mut self の通常APIで整合。ライフタイムパラメータなし
  - unsafe境界: なし
  - Send/Sync: フィールド型（SymbolId, FileId, ImportBinding等）に依存して自動導出される可能性。明示境界はこのチャンクには現れない
  - 非同期/await: 非該当
  - エラー設計: Optionで失敗表現。詳細な理由や診断情報は失われる（Result採用で改善余地）

エッジケース詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| Type.Memberの誤解決 | "List.Count" かつ "Count"がローカルにも存在 | 型ListのメンバーCountを解決 | member_nameを裸でresolve | 問題 |
| 3階層以上の修飾 | "A.B.C" | 段階的にA→A.B→A.B.Cと解決 | 完全一致のみ | 欠落 |
| usingエイリアス（型対象） | using X = System.Collections.Generic.List<int>; "X.Count" | Xの型メンバーを解決 | namespace前提で"namespace.member"を構築 | 不正確 |
| symbols_in_scopeにメンバー不在 | クラスメンバーのみ存在 | メンバーも列挙 | member_scope未列挙 | 問題 |
| スコープ離脱のクリア範囲 | クラスや名前空間を離脱 | memberやnamespaceシンボルのクリア | localのみクリア | 設計不足 |
| 別名重複 | using A=NS1; using A=NS2; | 警告/上書き方針 | 末尾優先で重複許容 | 仕様未定 |
| 同名シンボルの影響 | ローカルとグローバル同名 | ローカル優先で解決 | 仕様通り | OK |
| 関係解決の粒度 | kind=Calls/Implements等 | 種別に応じた特化 | 実質resolve委譲 | 粗い |

## Design & Architecture Suggestions

- Type.Member解決をスコープに依存しない形で階層的に処理
  - ドットで分割し、セグメント毎に名前空間→型→メンバーと段階的に探索する関数（try_resolve_qualified）を新設
  - 左辺Typeの検証後、右辺memberはそのTypeのメンバーテーブルで解決すべき（裸再解決を廃止）
- usingの表現拡張
  - エイリアスが「名前空間/型/静的インポート」のいずれかを持つ列挙型に
  - using static のサポート（静的メンバーを直接可視化）
- スコープ寿命管理の明確化
  - class/namespaceに入退場するenter/exitを受けてmember_scope/namespace_symbolsの切替やスタック化（Mapスタック）
- 一覧APIの整合性
  - symbols_in_scopeにmember_scopeを含め、整合したビューを提供
  - 望むなら表示制御（フィルタ）を引数で指定
- キャッシュ/メモ化
  - 同一名前の繰返し解決に対する短期キャッシュ
- エラー診断
  - Option→Resultへ変更し、未解決理由（未登録/曖昧/アクセス不可など）を通知可能に
- 関係解決の粒度
  - RelationKindに応じた限定スコープ/優先順位を設定（Implementsなら型空間のみ等）

## Testing Strategy (Unit/Integration) with Examples

目的: 正しい解決順序、修飾名/エイリアス、スコープ遷移、競合時の優先順位、バグ回帰テストを網羅。

- ユニットテスト例（擬似コード注意: SymbolIdの生成方法はこのチャンクには現れないためダミー関数を使用）

```rust
fn sid(_n: u32) -> SymbolId {
    // このチャンクにはSymbolIdの作り方が不明のため、適宜実装に合わせて置換してください
    unimplemented!("テスト環境でSymbolIdを生成できるヘルパを使用");
}

#[test]
fn resolves_in_csharp_order() {
    let file_id = /* ... */ unimplemented!();
    let mut ctx = CSharpResolutionContext::new(file_id);

    ctx.add_symbol("GlobalX".into(), sid(1), ScopeLevel::Global);
    ctx.add_symbol("NsX".into(), sid(2), ScopeLevel::Module);
    ctx.add_symbol("ImportedX".into(), sid(3), ScopeLevel::Package);
    ctx.add_symbol("LocalX".into(), sid(4), ScopeLevel::Local);

    // ローカルが最優先
    assert_eq!(ctx.resolve("LocalX").is_some(), true);
    // 次にメンバー（今回は未設定）
    assert_eq!(ctx.resolve("NsX").is_some(), true);
    assert_eq!(ctx.resolve("ImportedX").is_some(), true);
    assert_eq!(ctx.resolve("GlobalX").is_some(), true);
}

#[test]
fn symbols_in_scope_includes_all_but_member_bug() {
    let file_id = unimplemented!();
    let mut ctx = CSharpResolutionContext::new(file_id);

    // メンバー登録
    ctx.add_symbol_with_context("M".into(), sid(10), Some(&crate::symbol::ScopeContext::ClassMember));
    // 可視一覧にメンバーが出ない現状を確認（バグの回帰テスト）
    let listed: Vec<_> = ctx.symbols_in_scope().into_iter().map(|(n, _, _)| n).collect();
    assert!(!listed.contains(&"M".to_string()));
}

#[test]
fn qualified_name_resolution_behavior() {
    let file_id = unimplemented!();
    let mut ctx = CSharpResolutionContext::new(file_id);

    // 完全修飾名を直接登録した場合のみ成功
    ctx.add_symbol("System.Text.StringBuilder".into(), sid(100), ScopeLevel::Module);
    assert!(ctx.resolve("System.Text.StringBuilder").is_some());

    // "A.B.C" の分解解決は未サポート（登録していないと失敗）
    assert!(ctx.resolve("Foo.Bar.Baz").is_none());
}

#[test]
fn using_alias_resolution_for_namespace_member() {
    let file_id = unimplemented!();
    let mut ctx = CSharpResolutionContext::new(file_id);

    ctx.add_using("MyCompany.Utils".into(), Some("U".into()));
    ctx.add_symbol("MyCompany.Utils.Helper".into(), sid(20), ScopeLevel::Module);

    // "U.Helper" を "MyCompany.Utils.Helper" にマップできること
    assert!(ctx.resolve("U.Helper").is_some());
}

#[test]
fn type_member_bug_regression() {
    let file_id = unimplemented!();
    let mut ctx = CSharpResolutionContext::new(file_id);

    // 型 List を登録（名前空間や型表現の詳細は不明なため直接登録）
    ctx.add_symbol("List".into(), sid(30), ScopeLevel::Module);
    // ローカルに "Count" を登録（本来は List.Count が優先されるべき）
    ctx.add_symbol("Count".into(), sid(31), ScopeLevel::Local);

    // 現実装では "List.Count" がローカル "Count" に誤って解決されうる
    assert!(ctx.resolve("List.Count").is_some()); // 誤動作の可視化
}
```

- 統合テスト
  - 複数のファイル/名前空間/クラスを跨ぐ解決（RelationKindも含む）
  - スコープ遷移（enter_scope/exit_scope）とローカルのクリア検証

- プロパティテスト
  - ランダムな名前空間/using/スコープ挿入でresolveがパニックしないこと
  - 同名衝突時の優先順位が仕様通りであること

## Refactoring Plan & Best Practices

- Qualified名の統一的処理
  - try_resolve_qualified(segments: &[&str]) → Option<SymbolId> を導入し、任意長のセグメントを段階解決
  - Type.MemberはTypeのメンバー空間に限定して探索（裸の再解決をやめる）
- メンバー/名前空間の寿命管理
  - scope_stackに応じてmember_scope/namespace_symbolsをスタック化し、enter/exitでpush/popする構造に
- 一覧APIの一貫性
  - symbols_in_scopeにmember_scopeを含める。必要なら引数で列挙範囲指定（例: include_members: bool）
- usingモデル拡張
  - enum UsingTarget { Namespace(String), Type(SymbolId), Static(SymbolId) } のように表現し、解決ロジックを分岐
- エラー改善
  - Result<SymbolId, ResolveError> で曖昧名/見つからない/修飾不足/アクセス不可を区別
- パフォーマンス
  - 短期メモ化（LRU）でresolveのヒットを高速化
- APIドキュメンテーション
  - 重要な前提（完全修飾名の直接登録が必要など）をdocコメントに明記

## Observability (Logging, Metrics, Tracing)

- ログ
  - 解決失敗時にトレースレベルで「探索順序」「各スコープヒット/ミス」「using適用状況」を記録（ビルドフラグで切替）
- メトリクス
  - resolve呼出回数、平均探索深度、エイリアス使用率、未解決率
- トレーシング
  - resolveスパンを作り、修飾名分解と各段階の結果をイベントで記録
- デバッグフック
  - 現在のスコープスタックと各テーブルのダンプ機能（テスト限定）

## Risks & Unknowns

- Unknowns
  - SymbolId/FileId/ScopeType/ScopeContext/ImportBindingの詳細仕様はこのチャンクには現れない
  - using staticや型エイリアスの要件/仕様の厳密さ
  - クラス境界をどのようにenter/exitしているか（外部呼び出し規約）
- Risks
  - 誤ったType.Member解決により、IDE機能（Go to Definition等）で誤ジャンプ
  - member_scope未列挙によるユーザインターフェイスの不整合
  - 大規模usingのあるプロジェクトでのresolveの漸増コスト
  - 将来TypeScript等との共通基盤に寄せる際の_is_type_only引数の混乱

以上の改善で、正確性と拡張性、デバッグ容易性、性能のバランスが向上します。