# parsing/go/resolution.rs Review

## TL;DR

- Goのスコープ/解決と暗黙的インターフェース実装を扱う**3つの柱**: GoResolutionContext（スコープ/インポート/パッケージ解決）、TypeRegistry（型/ジェネリクス解決）、GoInheritanceResolver（インターフェース実装・埋め込み）。
- 主要公開APIは、スコープ登録・名前解決（ResolutionScope::resolve）、モジュール/ベンダ/相対インポート解決、go.mod解析、型レジストリのジェネリクス/組み込み型、インターフェース実装判定。
- 複雑箇所は、名前解決順序（ローカル→パッケージ→インポート→ドット付き名）、相対/ベンダインポート解決、go.mod探索、暗黙実装の発見。
- 重大リスク: 空インターフェースの実装判定が誤り（常にfalse）、discover_implementationsが未登録structを走査しない、ドット付き名のfallback解決が誤スコープ、resolve_symbol_in_packageの末尾一致で誤マッチ、exit_scopeのローカルスコープクリア条件が粗い。
- Rust安全性は概ね安全（unsafe未使用）だが、Optionベースのエラー消失が多い。パフォーマンス面ではベンダ解決の全走査やVec重複チェックのO(n^2)が課題。
- テストは豊富（ユニット）だが、上記バグをカバーするケースは未網羅。go.modの多様な構文、ベンダ解決、ドット付き名の厳密解決などの追加が有用。

## Overview & Purpose

このファイルはGo固有のスコープ/名前解決、およびGoの構造的型付け（インターフェースの暗黙実装）をRustで実装しています。

- GoResolutionContext: ローカル/パッケージ/インポートスコープの管理、import収集、パッケージ内/他パッケージのシンボル解決、相対/ベンダ/モジュールパス解決、go.mod解析と置換対応。
- TypeRegistry: 組み込み型の初期化、ユーザ定義型登録、ジェネリック型パラメータのスコープ管理と解決、インターフェース実装候補探索/判定。
- GoInheritanceResolver: インターフェース埋め込み、structのインターフェース実装、メソッド集合の集約/解決、型の継承チェーン/サブタイプ判定。

これらはcrate::parsingのResolutionScope/InheritanceResolverトレイトに適合し、上位のパーサ/インデクサ（DocumentIndex）と連携して、IDE/コードインテリジェンス向けの解決を提供します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | GoModInfo | pub | go.modのメタ情報格納（module/go/require/replace） | Low |
| Struct | TypeInfo | pub | 型メタ（名前、カテゴリ、パッケージ、汎型） | Low |
| Enum | TypeCategory | pub | 型カテゴリ（BuiltIn/Struct/Interface/Alias/Generic/GenericInstance） | Low |
| Struct | TypeRegistry | pub | 型登録/解決、ジェネリックスコープ管理、実装探索 | Med |
| Struct | GoResolutionContext | pub | Goのスコープ/名前解決、import/モジュール解析 | High |
| Impl(Trait) | ResolutionScope for GoResolutionContext | pub（トレイト経由） | 名前の追加/解決/スコープ入退/可視シンボル列挙 | Med |
| Struct | GoInheritanceResolver | pub | 構造的型付けの追跡（実装/埋め込み/メソッド） | Med |
| Impl(Trait) | InheritanceResolver for GoInheritanceResolver | pub（トレイト経由） | 継承追加、メソッド解決、サブタイプ判定 | Med |

### Dependencies & Interactions

- 内部依存
  - GoResolutionContext → TypeRegistry（型解決/ジェネリクス）
  - GoResolutionContext → DocumentIndex（シンボル検索/パス列挙/ファイルパス）
  - GoResolutionContext → resolve_relative_import / resolve_vendor_import / resolve_symbol_in_package（内部補助）
  - TypeRegistry → GoInheritanceResolver（任意：実装互換性チェック）
  - GoInheritanceResolver（自身のマップを再帰的に走査）

- 外部依存（主要）
  | 依存 | 用途 |
  |-----|-----|
  | crate::parsing::{ResolutionScope, ScopeLevel, ScopeType, InheritanceResolver} | トレイト定義/スコープレベル |
  | crate::parsing::resolution::ImportBinding | importバインディング保持 |
  | crate::storage::DocumentIndex | シンボルインデックス参照 |
  | crate::{FileId, SymbolId, Visibility} | 識別子と可視性 |
  | crate::symbol::ScopeContext | シンボルの定義コンテキスト |
  | std::collections::HashMap | コレクション |
  | std::fs | go.mod読み取り（parse_go_mod） |

- 被依存推定
  - パーサ/アナライザからのスコープ管理（enter_scope/exit_scope、add_symbol）
  - インポート解析（populate_imports, register_import_binding）
  - IDEのジャンプ/補完での名前解決（resolve、resolve_*_symbols、handle_go_module_paths）
  - 型/インターフェース解析（TypeRegistry、GoInheritanceResolver）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| TypeRegistry::new | fn new() -> Self | 組み込み型初期化 | O(B) | O(B) |
| TypeRegistry::is_built_in_type | fn is_built_in_type(&self, &str) -> bool | 組み込み型判定 | O(1) | O(1) |
| TypeRegistry::register_type | fn register_type(&mut self, TypeInfo) | ユーザ定義型登録 | O(1) | O(1) |
| TypeRegistry::resolve_type | fn resolve_type(&self, &str) -> Option<&TypeInfo> | 型解決（ジェネリクス含む） | O(G) | O(1) |
| TypeRegistry::enter_generic_scope | fn enter_generic_scope(&mut self) | 汎型スコープ入る | O(1) | O(1) |
| TypeRegistry::exit_generic_scope | fn exit_generic_scope(&mut self) | 汎型スコープ出る | O(1) | O(1) |
| TypeRegistry::add_generic_parameter | fn add_generic_parameter(&mut self, String, Option<String>) | 型パラメータ登録 | O(1) | O(1) |
| TypeRegistry::find_types_implementing | fn find_types_implementing(&self, &str, Option<&GoInheritanceResolver>) -> Vec<&TypeInfo> | インターフェース実装候補探索 | O(T·C) | O(R) |
| TypeRegistry::type_implements_interface | fn type_implements_interface(&self, &str, &str, Option<&GoInheritanceResolver>) -> bool | 実装判定 | O(C) | O(1) |
| GoResolutionContext::new | fn new(FileId) -> Self | コンテキスト生成 | O(B) | O(B) |
| GoResolutionContext::add_import | fn add_import(&mut self, String, Option<String>) | import追加 | O(1) | O(1) |
| GoResolutionContext::add_import_symbol | fn add_import_symbol(&mut self, String, SymbolId, bool) | インポートシンボル追加 | O(1) | O(1) |
| GoResolutionContext::add_symbol_with_context | fn add_symbol_with_context(&mut self, String, SymbolId, Option<&ScopeContext>) | スコープ文脈での登録 | O(1) | O(1) |
| GoResolutionContext::resolve_local_package_symbols | fn resolve_local_package_symbols(&self, &str, &DocumentIndex) -> Option<SymbolId> | 同パッケージ解決 | O(N) | O(1) |
| GoResolutionContext::resolve_imported_package_symbols | fn resolve_imported_package_symbols(&self, &str, &str, &DocumentIndex, Option<&str>, Option<&str>) -> Option<SymbolId> | 他パッケージ解決（相対/ベンダ） | O(I + Q) | O(1) |
| GoResolutionContext::resolve_relative_import | fn resolve_relative_import(&self, &str, &str) -> Option<String> | 相対import解決 | O(P) | O(1) |
| GoResolutionContext::resolve_vendor_import | fn resolve_vendor_import(&self, &str, &str, &DocumentIndex) -> Option<SymbolId> | vendor解決 | O(N) | O(1) |
| GoResolutionContext::parse_go_mod | fn parse_go_mod(&self, &str) -> Option<GoModInfo> | go.mod解析 | O(L) | O(D+R) |
| GoResolutionContext::apply_module_replacements | fn apply_module_replacements(&self, &str, &GoModInfo) -> String | replace適用 | O(R) | O(1) |
| GoResolutionContext::handle_go_module_paths | fn handle_go_module_paths(&self, &str, &DocumentIndex) -> Option<String> | モジュールパス解決 | O(P+L) | O(D+R) |
| GoResolutionContext::is_standard_library_package | fn is_standard_library_package(&self, &str) -> bool | 標準ライブラリ判定 | O(S) | O(1) |
| GoResolutionContext::{register_type, resolve_type, is_built_in_type} | fn register_type(...), fn resolve_type(...), fn is_built_in_type(...) | 型API委譲 | O(1)/O(G) | O(1) |
| GoResolutionContext::{enter/exit}_generic_scope | fn enter_generic_scope(&mut self), fn exit_generic_scope(&mut self) | 汎型スコープ制御 | O(1) | O(1) |
| GoResolutionContext::add_generic_parameter | fn add_generic_parameter(&mut self, String, Option<String>) | 汎型パラメータ | O(1) | O(1) |
| GoResolutionContext::parse_and_register_generic_params | fn parse_and_register_generic_params(&mut self, &str) | "[T any]"解析 | O(K) | O(K) |
| ResolutionScope::resolve | fn resolve(&self, &str) -> Option<SymbolId> | 名前解決（ローカル→パッケージ→インポート→ドット） | O(1)~O(H) | O(1) |
| InheritanceResolver::{add_inheritance, resolve_method, get_inheritance_chain, is_subtype, add_type_methods, get_all_methods} | トレイトAPI | 構造的型付け/メソッド/継承 | O(1)~O(V+E) | O(V+E) |
| GoInheritanceResolver::new | fn new() -> Self | 生成 | O(1) | O(1) |
| GoInheritanceResolver::is_interface | fn is_interface(&self, &str) -> bool | インターフェース判定（ヒューリスティック） | O(1) | O(1) |
| GoInheritanceResolver::{add_struct_implements, add_interface_embeds} | fn ... | 実装/埋め込み登録 | O(1)~O(k) | O(k) |
| GoInheritanceResolver::{get_all_interfaces, check_struct_implements_interface, discover_implementations, find_implementations_of, register_type_methods, type_has_method} | fn ... | 実装探索/判定/登録 | O(V+E) | O(V+E) |

凡例: B=組み込み型数, G=ジェネリックスコープ段数, T=型数, C=互換性判定コスト, I=import数, Q=インデックスクエリコスト, N=候補数, P=パスセグメント数, L=ファイル行数, D=依存数, R=置換数, S=標準ライブラリ候補数, V/E=グラフの頂点/辺

各APIの詳細（抜粋）

1) GoResolutionContext::resolve_imported_package_symbols
- 目的と責務: import宣言からパッケージ別名/実名を解決し、相対import、vendor、通常解決の順にシンボルを探索。
- アルゴリズム:
  1. importsを走査してeffective_name（別名orパス末尾）一致を探す
  2. import_pathが"./" or "../"で始まればresolve_relative_import→resolve_symbol_in_package
  3. project_rootがあればresolve_vendor_import
  4. それ以外はresolve_symbol_in_package(import_path)
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | package_name | &str | import時の可視名（別名/末尾） |
  | symbol_name | &str | 探すシンボル |
  | document_index | &DocumentIndex | インデックス |
  | current_package_path | Option<&str> | 相対importの基準 |
  | project_root | Option<&str> | vendor探索の基準 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Option<SymbolId> | 解決成功ならSome |
- 使用例:
  ```rust
  let mut ctx = GoResolutionContext::new(FileId::new(1).unwrap());
  ctx.add_import("github.com/user/repo/utils".into(), Some("utils".into()));
  let id = ctx.resolve_imported_package_symbols(
      "utils", "Helper", &document_index, Some("myproj/pkg"), Some("/workspace")
  );
  ```
- エッジケース:
  - 別名の衝突（複数import同名）→ 最初の一致のみ考慮
  - vendor探索は全シンボル走査で高コスト
  - 相対importでモジュール境界越えの扱いが粗い（rootに潰す）

2) TypeRegistry::resolve_type
- 目的: 汎型パラメータ→登録型（組み込み含む）順で型名を解決。
- アルゴリズム:
  1. generic_contextsを後入れ先出しで探索
  2. 見つからなければtypes（組み込み含む）を探索
- 引数/戻り値:
  | 引数 | 型 |  | 戻り値 | 型 |
  |------|----|-|--------|----|
  | type_name | &str | | Option<&TypeInfo> | 参照 |
- 使用例:
  ```rust
  let mut reg = TypeRegistry::new();
  reg.enter_generic_scope();
  reg.add_generic_parameter("T".into(), Some("any".into()));
  assert_eq!(reg.resolve_type("T").unwrap().category, TypeCategory::Generic);
  ```
- エッジケース:
  - 同名の汎型とユーザ型→汎型が優先

3) GoInheritanceResolver::check_struct_implements_interface
- 目的: 構造体がインターフェースの全メソッドを持つか判定（構造的型付け）。
- アルゴリズム:
  1. interfaceの全メソッド集合を収集（埋め込み含む）
  2. structの全メソッド集合と包含判定
- 注意: 現実装は空インターフェースをfalseにする制限あり（バグ）
- 使用例:
  ```rust
  let mut res = GoInheritanceResolver::new();
  res.register_type_methods("Writer".into(), vec!["Write".into()]);
  res.register_type_methods("FileWriter".into(), vec!["Write".into(), "Close".into()]);
  assert!(res.check_struct_implements_interface("FileWriter", "Writer"));
  ```
- エッジケース:
  - 空インターフェース（any）→ 本来trueだが現実装はfalse

4) ResolutionScope::resolve（impl GoResolutionContext）
- 目的: 名前解決（ローカル→パッケージ→インポート→ドット付き名の特例）。
- ステップ:
  - ローカル→パッケージ→インポートを順に検索
  - '.' を含む場合は完全一致検索→2分割（pkg/typeとメンバ）で再帰的に解決試行
- 注意: 2分割再帰はスコープ非考慮で誤解決の恐れあり
- 使用例:
  ```rust
  let mut ctx = GoResolutionContext::new(FileId::new(1).unwrap());
  ctx.add_symbol("LocalVar".into(), SymbolId::new(1).unwrap(), ScopeLevel::Local);
  assert_eq!(ctx.resolve("LocalVar"), Some(SymbolId::new(1).unwrap()));
  ```
- エッジケース:
  - "pkg.Func" 形式での誤解決（関数がどのスコープでも一致すれば返る）

その他のAPI詳細は本チャンク外/省略（不明/割愛と記載）。

## Walkthrough & Data Flow

- シンボル登録フロー
  1. パーサがimportを抽出し、populate_imports / add_importで登録
  2. 各宣言をadd_symbol_with_context（またはResolutionScope::add_symbol）でローカル/パッケージ/インポートへ振り分け
  3. 関数/ブロック入退出でenter_scope/exit_scopeを呼ぶ（ローカルクリアは粗めの条件）

- 名前解決フロー
  1. 単純名: resolveがローカル→パッケージ→インポートの順でHashMap検索
  2. ドット付き名: まず完全一致（imported/package両方）を試し、失敗時は2分割で再帰解決（注意: スコープ無視の簡易実装）
  3. 同パッケージ他ファイル: resolve_local_package_symbolsがDocumentIndexからmodule_path一致で検索
  4. 他パッケージ: resolve_imported_package_symbolsが相対/ベンダ/通常の順でresolve_symbol_in_packageを呼ぶ

- モジュール/標準ライブラリ解決
  - is_standard_library_packageで既知パッケージ短絡
  - handle_go_module_pathsが最寄りのgo.modをfind_and_parse_go_modで探索→parse_go_mod→apply_module_replacementsを適用

- 型/ジェネリクス
  - TypeRegistryが組み込み型を保持し、enter/exitで汎型スコープをスタック管理。resolve_typeは汎型スコープ優先で検索。

- インターフェース実装
  - GoInheritanceResolverに実装/埋め込み/メソッドを登録
  - check_struct_implements_interfaceで完全包含（埋め込み含む）を確認
  - find_types_implementing / type_implements_interfaceでTypeRegistryと連携

Mermaidフローチャート（resolveの主要分岐）

```mermaid
flowchart TD
  A[resolve(name)] --> B{Local scope contains?}
  B -- yes --> R1[return local id]
  B -- no --> C{Package symbols contain?}
  C -- yes --> R2[return package id]
  C -- no --> D{Imported symbols contain?}
  D -- yes --> R3[return import id]
  D -- no --> E{name contains '.'?}
  E -- no --> R4[return None]
  E -- yes --> F{Full qualified match<br/>(imported or package)?}
  F -- yes --> R5[return qualified id]
  F -- no --> G[Split name into X.Y]
  G --> H{resolve(X) is Some?}
  H -- no --> R6[return None]
  H -- yes --> I[return resolve(Y)]
```

上記の図はimpl ResolutionScope for GoResolutionContext の resolve メソッドの主要分岐を示す（正確な行番号はこのチャンクからは不明）。

## Complexity & Performance

- HashMap中心の解決で基本はO(1)。ただし以下は潜在ボトルネック。
  - resolve_imported_package_symbols: import本数Iに比例し、さらにDocumentIndexクエリ（コストQ）。
  - resolve_vendor_import: find_symbols_by_name("*")で全候補走査→O(N)（大規模プロジェクトで高コスト）。
  - resolve_local_package_symbols / resolve_symbol_in_package: find_symbols_by_nameに依存（O(N)）。
  - find_and_parse_go_mod: 全インデックスパス走査→go.mod探索O(P)。大規模でやや重い。
  - GoInheritanceResolver::get_all_methods: 重複排除がVec.containsでO(k^2)。HashSet化で改善可。
- スケール限界/I/O
  - go.mod読み取りはfs I/O。頻繁呼びはキャッシュ必須（現状キャッシュなし）。
  - vendor解決の広範検索はインデックス側での事前キー化（module_path→シンボル）で高速化余地。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空インターフェース実装判定 | interface methods = [] | 任意のstructが実装扱い（Go仕様） | check_struct_implements_interfaceは!emptyを要求 | Bug |
| discoverで未登録struct探索 | type_methodsにのみ存在 | 実装推定を試みる | struct_implements.keys()のみ反復 | Bug |
| ドット付き名の誤解決 | "pkg.Func" | pkgスコープに限定してFunc探索 | self.resolveでグローバル再探索 | Bug/設計課題 |
| 末尾一致の誤マッチ | package_path=github.com/a/x, シンボルのmodule_path末尾"x" | 正しいパッケージに限定 | resolve_symbol_in_packageが末尾一致許容 | Bug/衝突リスク |
| ブロックスコープ終了 | if内の変数 | ブロック終了で不可視 | exit_scopeのクリア条件が粗い | Bug/仕様不足 |
| 相対importで上位超過 | "../../shared" | 期待はプロジェクト内にとどめる/None | keep_count=0でrootに潰す | 仕様要検討 |
| vendor解決の性能 | 大規模N | 高速 | 全候補走査O(N) | 性能課題 |
| go.mod解析の多様性 | "require x v1.2.3 // indirect" | コメント除去 | コメント行はskipだが末尾コメント未対応 | 制限 |
| 同パッケージ多定義 | 複数ファイル | 優先順位の定義 | 先頭一致を即返す | 仕様要検討 |
| get_current_module_pathの安定性 | ファイル内複数シンボル | 一貫したmodule_path | 最初のシンボルから取得 | 不安定/推測 |

セキュリティチェックリスト
- メモリ安全性: unsafe未使用、所有権/借用は標準コレクション中心で安全。整数/バッファオーバーフロー懸念なし。
- インジェクション: コマンド/SQLなし。パス扱いは文字列比較主体でPath正規化不使用（パス解決/比較の混乱リスク）。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密なし。ログ出力もなし（情報漏えいなし）。
- 並行性: 同期なし。スレッド共有時は外部で同期が必要（本モジュールはSend/Sync保証を明示しない）。

Rust特有の観点
- 所有権: 返り値はOptionやクローン最小限。TypeInfo登録時に所有権移動（register_type）。
- 借用/ライフタイム: 参照返却（resolve_type -> Option<&TypeInfo>）は内部のHashMapの借用、ライフタイムは&selfに紐づき妥当。
- unsafe境界: なし。
- 並行性/非同期: 非同期/awaitなし。共有状態はHashMapで内部可変、マルチスレッド利用時は外部で保護が必要。
- エラー設計: 失敗はOptionで落とし、詳細情報が失われる箇所が多い（parse_go_mod, handle_go_module_paths, find_and_parse_go_mod）。unwrap/expectはテストのみ。

## Design & Architecture Suggestions

- 名前解決の厳密化
  - ドット付き名: 「パッケージ名（import別名）」と「メンバ」の2段階解決に限定し、後半のメンバはそのパッケージスコープ内でのみ探索。現在の再帰的global探索は誤解決を誘発。
  - resolve_symbol_in_package: module_pathの完全一致（または事前正規化）のみにし、末尾一致は避ける。DocumentIndexに「module_path→{symbol}」の逆引きを追加すると良い。
- スコープ管理の精緻化
  - scope_stackにBlock/Function等を正確にpushし、ブロック終了でそのブロックで導入された識別子のみを削除できるスタック/シャドーイング構造（Vec<HashMap<...>>）へ。
- ベンダ/インデックス探索の最適化
  - resolve_vendor_importはワイルドカード全走査ではなく、module_pathで索引化されたクエリを用意（例: DocumentIndex.find_symbols_by_module_path_prefix）。
- go.mod解析/キャッシュ
  - find_and_parse_go_modの結果をファイルシステムの最終更新時刻と併せてキャッシュ。parse_go_modで末尾コメントやquoted pathにも対応（正規表現または簡易パーサ導入）。
- インターフェース実装ロジック
  - 空インターフェース: check_struct_implements_interfaceは空集合ならtrueを返すべき。
  - discover_implementations: 走査対象にtype_methodsのキーでインターフェースでない型全てを含める。
  - メソッド集合の重複排除: Vec→HashSetに置換し、O(n^2)を回避。
- Observabilityの追加
  - tracing/logで解決過程（どのスコープでヒット/どのインポート経路/モジュール解決結果）をデバッグ段階だけでも出力可能に。
- APIエラーの明確化
  - OptionではなくResult<_, ResolveError>などで失敗理由（見つからない/複数候補/可視性/パッケージ不一致）を返せるインターフェースの提供（上位は必要に応じOptionに落とす）。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本経路をカバー。以下の追加を推奨。

- 空インターフェースの実装判定
  ```rust
  #[test]
  fn test_empty_interface_is_implemented() {
      let mut res = GoInheritanceResolver::new();
      res.register_type_methods("Empty".into(), vec![]);
      res.register_type_methods("S".into(), vec![]);
      // 修正後の期待
      assert!(res.check_struct_implements_interface("S", "Empty"));
  }
  ```
- discover_implementationsが未登録structを見つける
  ```rust
  #[test]
  fn test_discover_unregistered_structs() {
      let mut res = GoInheritanceResolver::new();
      res.register_type_methods("I".into(), vec!["M".into()]);
      res.register_type_methods("S".into(), vec!["M".into()]);
      let found = res.discover_implementations();
      assert!(found.iter().any(|(s,i)| s=="S" && i=="I"));
  }
  ```
- ドット付き名の厳密解決（パッケージ限定）
  ```rust
  #[test]
  fn test_qualified_name_scoped_resolution() {
      let mut ctx = GoResolutionContext::new(FileId::new(1).unwrap());
      // 前提: "fmt" import, "Println"はfmtにのみ存在
      ctx.add_import("fmt".into(), None);
      // 同名"Println"をローカルに置いても "fmt.Println" はfmt側を解決すること
      ctx.add_symbol("Println".into(), SymbolId::new(99).unwrap(), ScopeLevel::Local);
      // 実装修正後: qualifiedはfmtスコープ内でのみ探索
      // ここではDocumentIndexのモックが必要（不明）
  }
  ```
- resolve_symbol_in_packageの末尾一致抑止
  ```rust
  // DocumentIndexの用意が必要。module_pathの完全一致のみにヒットするアサーションを追加。
  ```
- 相対importの上位超過
  ```rust
  #[test]
  fn test_relative_import_outside_module() {
      let ctx = GoResolutionContext::new(FileId::new(1).unwrap());
      // 設計に応じて None を返す等の期待仕様を明確化しテスト
      let res = ctx.resolve_relative_import("../../..", "a/b");
      // 期待: Some("") or None（現状は Some("")もありうる）→ 仕様を決める
  }
  ```
- go.mod解析の末尾コメント
  ```rust
  #[test]
  fn test_go_mod_require_with_comment() {
      let ctx = GoResolutionContext::new(FileId::new(1).unwrap());
      // require x v1.2.3 // indirect を含むファイルを作成し、期待通りdependenciesに入るか検証
  }
  ```

## Refactoring Plan & Best Practices

- フェーズ1（安全改善）
  - check_struct_implements_interface: 空集合はtrue
  - discover_implementations: 走査対象拡大（type_methods.keys()の非インターフェース型）
  - get_all_methods: 重複管理をHashSet化
- フェーズ2（解決の厳密化/性能）
  - resolve_symbol_in_package: module_path完全一致のみ
  - resolve_vendor_import: DocumentIndexにプレフィックス検索APIを追加して使用
  - resolveのドット付き名: パッケージ→メンバの二段階検索に限定、グローバル再解決を廃止
- フェーズ3（スコープモデル）
  - ローカルスコープをブロックごとのスタック（Vec<HashMap<...>>）へ変更
  - exit_scopeで対応ブロックのmapをpop
- フェーズ4（パス/モジュール）
  - Path/PathBufでの正規化・結合（文字列演算を廃止）
  - go.mod探索結果のキャッシュ（ファイル更新監視も可）
  - 標準ライブラリ判定の拡張（動的リスト or go env GOROOT参照; 可能なら設定経由）
- ベストプラクティス
  - Resultベースのエラー型導入（ResolutionError）
  - 競合/衝突時のデバッグログ追加
  - 単体テストのテーブルドリブン化でカバレッジ拡大

## Observability (Logging, Metrics, Tracing)

- ロギング（debugレベル）
  - どのスコープで命中したか、qualified名の解決経路、module/vendor/relativeの分岐結果
  - go.mod探索の見つかったパス/距離
- メトリクス
  - 解決成功率、解決レイテンシ（DocumentIndexクエリの時間）
  - ベンダ解決の候補走査数
- トレーシング
  - resolve呼び出しにspanを張り、import/パッケージ/ローカルの各ステップをイベントとして記録
- フィーチャフラグ
  - 詳細ログを本番で無効化できるようにfeatureまたは環境変数ガード

## Risks & Unknowns

- DocumentIndexのクエリ仕様（ワイルドカード"*", モジュールパス正規化/区切り、可視性/言語フィルタの意味）の詳細は不明。
- ScopeType/ScopeLevel/ScopeContextの全体設計が不明（ブロックスコープの意図する扱いを要確認）。
- 標準ライブラリリストは固定配列で不完全。Goのバージョンによる差分やサブパッケージの網羅性が不明。
- go.modの多様な構文（replaceの相対/バージョン指定、複数スペース、コメント）対応の範囲が不明。
- パス区切り（Windowsの'\'）とmodule_pathのフォーマットの整合性が不明（現実装は'/'前提の文字列操作）。
- インデックス内に同名シンボルが複数ある場合の解決優先度ポリシーが不明。

以上の不明点は、本モジュールの責務/期待仕様を明確化し、テストとAPIのエラー型で表現することでリスク低減が可能です。