# parsing\c\resolution.rs Review

## TL;DR

- 目的: C言語向けの**シンボル解決**（ローカル→モジュール→インポート→グローバル）と、C特有の**擬似的な継承関係**（typedef・構造体の合成）を扱う。
- 公開API: **CResolutionContext**（new/add_* 系・includes）、**CInheritanceResolver**（new/add_typedef/resolve_typedef）。
- コアロジック: `resolve`の解決順序、`exit_scope`によるローカルスコープの破棄、typedef連鎖解決、構成（composition）グラフの探索。
- 重大バグ: `resolve_method`に**訪問済み集合（visited）不在**で、合成グラフにサイクルがあると無限再帰の危険（関数: resolve_method, 行番号=不明）。
- Cのブロックスコープ未対応: `local_scope`が1層のみで、**ブロック退出時にローカルが残存**する設計上の問題（関数: exit_scope, 行番号=不明）。
- 安全性: **unsafe**は未使用。インジェクションや認証機構は該当なし。並行性は使っていないが、`&mut self`前提でシングルスレッド想定。
- パフォーマンス: ほぼ**O(1)**のマップ操作だが、メソッド・型関係の探索は**O(V+E)**（合成グラフ）に達する。

## Overview & Purpose

このモジュールは、C言語のスコープ規則に基づく**シンボル解決**（識別子→SymbolId）と、C言語における**継承風の関係**（構造体の合成、typedefエイリアス、構造体関連メソッドとしての関数ポインタ）の解決を提供する。

- CResolutionContext: Cのスコープモデル（ローカル・ファイル（モジュール）・ヘッダ由来・グローバル）での名前解決、インクルード情報、インポートバインディングの管理を行う。
- CInheritanceResolver: C言語における型間の関係（typedef、構成）やメソッド探索を扱う。

RustやTypeScript版と同じパターンの**ResolutionScope**・**InheritanceResolver**トレイトをC用に具体化している点が特徴。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | CResolutionContext | pub | Cのスコープ（ローカル/モジュール/インポート/グローバル）管理、解決、インクルード・インポートバインディング保持 | Med |
| Impl(Tr) | ResolutionScope for CResolutionContext | - | トレイト準拠の解決・スコープ操作・シンボル列挙・インポート取り込み | Med |
| Struct | CInheritanceResolver | pub | typedef連鎖、構成（composition）関係、型メソッド集合の管理 | Med |
| Impl(Tr) | InheritanceResolver for CInheritanceResolver | - | typedef/構成の関係構築、メソッド解決、チェーン取得、サブタイプ判定、メソッド集約 | Med |
| Helper | build_composition_chain | - | 構成チェーンのDFS構築（visited使用） | Low |
| Helper | is_composed_of_recursive | - | 親型の構成包含判定（visited使用） | Low |
| Helper | collect_all_methods | - | 型と構成先からメソッド集約（visited使用） | Med |

### Dependencies & Interactions

- 内部依存
  - CResolutionContext:
    - `resolve`が`local_scope`→`module_symbols`→`imported_symbols`→`global_symbols`の順で参照（関数: resolve, 行番号=不明）
    - `exit_scope`が`scope_stack`をpop後、トップが`None | Module | Global`なら`local_scope.clear()`（関数: exit_scope, 行番号=不明）
    - `populate_imports`が`Import`の`path`を`includes`へ保存（関数: populate_imports, 行番号=不明）
    - `register_import_binding`/`import_binding`が`import_bindings`を利用（関数: register_import_binding/import_binding, 行番号=不明）
  - CInheritanceResolver:
    - `resolve_typedef`が`typedef_map`を辿る（visitedあり）（関数: resolve_typedef, 行番号=不明）
    - `resolve_method`が`resolve_typedef`→`type_methods`→`composition_map`を再帰探索（visitedなし）（関数: resolve_method, 行番号=不明）
    - `get_inheritance_chain`が`build_composition_chain`を利用（関数: get_inheritance_chain, 行番号=不明）

- 外部依存（表）
  | 依存 | 用途 |
  |------|------|
  | crate::parsing::resolution::ImportBinding | Cのimportバインディング管理 |
  | crate::parsing::{InheritanceResolver, ResolutionScope, ScopeLevel, ScopeType} | トレイトおよびスコープ表現 |
  | crate::{FileId, SymbolId} | ファイル/シンボル識別子 |
  | std::collections::{HashMap, HashSet} | マップ/集合構造 |

- 被依存推定
  - C言語パーサフェーズ（宣言・typedef・struct合成の検出）
  - 全体のクロスリファレンス/IDE機能（定義/参照解決）
  - ドキュメンテーション生成やコードナビゲーション

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CResolutionContext | struct CResolutionContext | Cスコープ解決コンテキスト | N/A | N/A |
| CResolutionContext::new | fn new(file_id: FileId) -> Self | 新規コンテキスト作成 | O(1) | O(1) |
| CResolutionContext::add_include | fn add_include(&mut self, header_path: String) | インクルードを追加 | O(1) | O(1) amort. |
| CResolutionContext::add_local | fn add_local(&mut self, name: String, symbol_id: SymbolId) | ローカルシンボル登録 | O(1) avg | O(1) |
| CResolutionContext::add_module_symbol | fn add_module_symbol(&mut self, name: String, symbol_id: SymbolId) | モジュール（ファイル）スコープ登録 | O(1) avg | O(1) |
| CResolutionContext::add_import_symbol | fn add_import_symbol(&mut self, name: String, symbol_id: SymbolId) | ヘッダ由来シンボル登録 | O(1) avg | O(1) |
| CResolutionContext::add_global_symbol | fn add_global_symbol(&mut self, name: String, symbol_id: SymbolId) | プロジェクトグローバル登録 | O(1) avg | O(1) |
| CResolutionContext::includes | fn includes(&self) -> &[String] | インクルード一覧取得 | O(1) | O(1) |
| CInheritanceResolver | struct CInheritanceResolver | Cの擬似継承関係解決コンテキスト | N/A | N/A |
| CInheritanceResolver::new | fn new() -> Self | 新規コンテキスト作成 | O(1) | O(1) |
| CInheritanceResolver::add_typedef | fn add_typedef(&mut self, alias: String, underlying_type: String) | typedef関係の追加 | O(1) avg | O(1) |
| CInheritanceResolver::resolve_typedef | fn resolve_typedef(&self, type_name: &str) -> String | typedef連鎖の最終型解決 | O(k) | O(k) |

以下、各APIの詳細。

1) CResolutionContext（struct）
- 目的と責務: Cのスコープ解決用の状態保持（ローカル、モジュール、インポート、グローバル、スコープスタック、インクルード、インポートバインディング）。
- データ契約:
  - local_scope/module_symbols/imported_symbols/global_symbols: 文字列キーでSymbolIdを保持。
  - scope_stack: ScopeTypeの積み重ね（例: Function, Module, Global, Block等）。
  - includes: ヘッダパスのリスト。
  - import_bindings: 公開名→ImportBinding。
- 使用例:
  ```rust
  let file_id = FileId::new(1).unwrap();
  let mut ctx = CResolutionContext::new(file_id);
  ctx.add_module_symbol("printf".to_string(), SymbolId::new(10).unwrap());
  ```
- エッジケース:
  - 空のコンテキストでも有効。
  - 文字列キーの重複は後勝ち（HashMap::insert）。

2) CResolutionContext::new
- アルゴリズム: すべてのコレクションを空で初期化。
- 引数
  | 名 | 型 | 意味 |
  |----|----|------|
  | file_id | FileId | ファイル識別子 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Self | 新しいコンテキスト |
- 使用例:
  ```rust
  let ctx = CResolutionContext::new(FileId::new(1).unwrap());
  ```
- エッジケース:
  - file_idは格納のみ、即時利用はなし。

3) CResolutionContext::add_include
- アルゴリズム: `includes.push(header_path)`.
- 引数
  | 名 | 型 | 意味 |
  |----|----|------|
  | header_path | String | ヘッダパス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例:
  ```rust
  ctx.add_include("stdio.h".to_string());
  ```
- エッジケース:
  - 重複インクルードはそのまま追加（重複排除なし）。

4) CResolutionContext::add_local / add_module_symbol / add_import_symbol / add_global_symbol
- アルゴリズム: それぞれ対応するHashMapへ`insert(name, symbol_id)`.
- 引数
  | 名 | 型 | 意味 |
  |----|----|------|
  | name | String | シンボル名 |
  | symbol_id | SymbolId | シンボルID |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例:
  ```rust
  ctx.add_local("i".to_string(), SymbolId::new(1).unwrap());
  ctx.add_module_symbol("main".to_string(), SymbolId::new(2).unwrap());
  ctx.add_import_symbol("size_t".to_string(), SymbolId::new(3).unwrap());
  ctx.add_global_symbol("global_util".to_string(), SymbolId::new(4).unwrap());
  ```
- エッジケース:
  - 同名シンボルの再登録は上書き（後勝ち）。
  - `resolve`ではローカルが最優先でヒット（関数: resolve, 行番号=不明）。

5) CResolutionContext::includes
- アルゴリズム: `&self.includes`を返すだけ。
- 引数
  | 名 | 型 | 意味 |
  |----|----|------|
  | なし | - | - |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | &[String] | インクルードのスライス |
- 使用例:
  ```rust
  for inc in ctx.includes() { println!("{}", inc); }
  ```
- エッジケース:
  - 空なら空スライス。

6) CInheritanceResolver（struct）
- 目的と責務: typedef連鎖、構成（構造体の埋め込み）の関係、および型に紐づくメソッド（関数ポインタ名の集合）を管理。
- データ契約:
  - composition_map: `type -> Vec<(composed_type, kind)>`
  - type_methods: `type -> Vec<method_name>`
  - typedef_map: `alias -> underlying_type`
- 使用例:
  ```rust
  let mut inh = CInheritanceResolver::new();
  inh.add_typedef("size_type".to_string(), "unsigned long".to_string());
  ```

7) CInheritanceResolver::new
- アルゴリズム: 空のマップ群で初期化。
- 引数/戻り値は上記通り。

8) CInheritanceResolver::add_typedef
- アルゴリズム: `typedef_map.insert(alias, underlying_type)`.
- 引数
  | 名 | 型 | 意味 |
  |----|----|------|
  | alias | String | 別名 |
  | underlying_type | String | 実体型 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例:
  ```rust
  inh.add_typedef("my_int".to_string(), "int".to_string());
  ```
- エッジケース:
  - 連鎖typedefにも対応（後述`resolve_typedef`）。

9) CInheritanceResolver::resolve_typedef
- アルゴリズム（訪問検出あり）:
  1. `current = type_name`から開始。
  2. `typedef_map[current]`があれば辿る。
  3. `visited`に記録してサイクルを回避。
  4. 最終的な基底型の文字列を返す。
- 引数
  | 名 | 型 | 意味 |
  |----|----|------|
  | type_name | &str | 元の型（別名含む） |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | String | 連鎖を解いた最終型 |
- 使用例:
  ```rust
  let resolved = inh.resolve_typedef("size_type"); // "unsigned long" 等
  ```
- エッジケース:
  - サイクル検出時は打ち切り（*循環typedefは想定外*）。最後に到達した型を返す。

## Walkthrough & Data Flow

- シンボル解決のフロー（関数: resolve, 行番号=不明）
  1. ローカルスコープ（`local_scope`）に存在すれば返す。
  2. モジュールスコープ（`module_symbols`）を検索。
  3. インポート（ヘッダ由来、`imported_symbols`）を検索。
  4. グローバル（`global_symbols`）を検索。
  5. 見つからなければ`None`。

- スコープ管理（関数: enter_scope/exit_scope, 行番号=不明）
  - `enter_scope`: `scope_stack.push(scope_type)`のみ（ホイスティングなし）。
  - `exit_scope`: `pop`後にスタックの最上位が`None | Module | Global`なら`local_scope.clear()`を呼び出し。「関数スコープを抜けたらローカルをクリア」という意図だが、ブロックスコープのローカル変数はクリアされない。

- インポート取り込み（関数: populate_imports, 行番号=不明）
  - `Import.path`文字列を`includes`へ保存。`import_bindings`は`register_import_binding`で公開名→バインディングを登録し、`import_binding`で検索・複製返却。

- 型関係とメソッド解決（関数: resolve_typedef, resolve_method, 行番号=不明）
  - 型名はまず`resolve_typedef`で連鎖解決。
  - メソッドは`type_methods[resolved_type]`に含まれるか検査し、なければ`composition_map[resolved_type]`の構成先へ再帰検索。
  - 注意: `resolve_method`は**visited未使用**で、構成グラフが循環すると無限再帰の危険あり。

## Complexity & Performance

- マップ操作（add_*系、`resolve`の1ヒット）: 平均O(1)時間、O(1)追加空間。ハッシュ衝突が極端な場合はO(n)最悪も理論上。
- `symbols_in_scope`: 全マップの合計サイズに比例（O(L+M+I+G)）。
- `populate_imports`: インポート数kに対しO(k)（文字列クローンコストあり）。
- `resolve_typedef`: 連鎖長kに対しO(k)時間、訪問集合でO(k)空間。
- 構成グラフ探索（`build_composition_chain`、`is_composed_of_recursive`、`collect_all_methods`）: グラフ頂点/辺の数に比例しO(V+E)。
- `resolve_method`: visited不在で理論上O(∞)（循環時）/ O(V+E)（非循環）。スタックオーバーフローの危険。

ボトルネック:
- 大規模コードベースでの`symbols_in_scope`やメソッド集約は線形合計のコスト。
- `resolve_method`の無限再帰は重大な性能・信頼性問題。

スケール限界:
- 巨大なヘッダ群の取り込みで`includes`が大きくなるが、文字列スライス参照のみで返すため影響小。
- 大規模型グラフではDFSのコスト、かつ循環対策必須。

I/O/ネットワーク/DBはこのチャンクには現れない。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 同名のローカルとモジュールの衝突 | nameが両方にある | ローカル優先で解決 | resolveでローカル→モジュールの順に探索（関数: resolve, 行番号=不明） | OK |
| 未定義シンボル | "unknown" | Noneを返す | resolveで全マップ不一致ならNone | OK |
| 関数スコープ退出時のローカル解放 | FunctionからModuleへ戻る | ローカルをクリア | exit_scopeでクリア（関数: exit_scope, 行番号=不明） | OK |
| ブロックスコープ退出時 | BlockからFunctionへ戻る | ブロックローカルは不可視に | 現状クリアしない（単一`local_scope`で保持） | 要修正 |
| 重複インクルード | "stdio.h"を複数回 | 一意化するか無視 | そのままpush（重複許容） | 設計判断 |
| typedef循環 | A→B, B→A | サイクル検出で打ち切り | visitedでループ回避（関数: resolve_typedef, 行番号=不明） | OK（妥協） |
| 構成循環 | XがYを構成、YがXを構成 | 無限ループ回避 | resolve_methodはvisitedなしで再帰 | 致命的 |
| メソッド未定義 | typeにmethodなし | None | resolve_methodで見つからなければNone | OK |
| import_binding未登録 | name不一致 | None | import_bindingで未ヒットならNone | OK |

セキュリティチェックリスト:
- メモリ安全性: unsafe未使用。Buffer overflow/Use-after-free/Integer overflowは該当なし（標準コレクションの利用に限定）。
- インジェクション: SQL/Command/Path traversalなし（I/Oなし）。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密なし。ログ出力なし（情報漏洩の心配なし）。
- 並行性: スレッドセーフ設計ではない（可変参照`&mut self`前提）。レース/デッドロックは該当なし。

Rust特有の観点:
- 所有権: 文字列キー/値は`HashMap`へ移動（例: add_*系、関数: add_local等, 行番号=不明）。戻り時にクローンする箇所（import_binding）あり。
- 借用: `&self`/`&mut self`のメソッド設計で安全。可変借用はメソッドスコープ内に限定。
- ライフタイム: 明示的ライフタイムは不要。返却はスライス（includes）で借用寿命を安全に提供。
- unsafe境界: なし。
- Send/Sync: 型自体はフィールドが`String/HashMap`であり、一般にSend/Sync要件は型パラメータ次第だが、このチャンクでは明示境界なし。並行利用は想定外。
- 非同期/await: 非同期は該当なし。
- エラー設計: `Option`で未解決を表現。`Result`は使用なし。`unwrap`/`expect`はテストコード内のみで妥当。

## Design & Architecture Suggestions

- ブロックスコープ対応:
  - 現在`local_scope`が単一で、ブロック退出時に変数が残存する。`Vec<HashMap<String, SymbolId>>`による**スコープごとのスタック**構造にすることで、`enter_scope(Block)`で`push`、`exit_scope`で`pop`し、`resolve`はスタック上から順に検索できるようにする。
- resolve_methodの無限再帰対策:
  - `resolve_method`にも`visited: HashSet<String>`を導入し、構成グラフの循環を検出して打ち切りにする（関数: resolve_method, 行番号=不明）。これによりスタックオーバーフローを防止。
- includesの重複整理:
  - `includes`を`HashSet<String>`にするか、挿入時に重複チェックしてノイズを削減。
- API整合性:
  - `populate_imports`と`add_include`の責務を整理（両者が同じ`includes`を操作）。導入元（直接記述・解析導出）で区別するフィールドを持つか、フラグを追加。
- 文字列クローン最適化:
  - `symbols_in_scope`での`name.clone()`は必要だが、使用頻度次第ではイテレータ返却や借用を検討。
- exit_scopeの判定簡素化:
  - 「関数を抜けたときだけクリア」ならpopした**スコープ種別（popped）**で判定する方が明確（C拡張でネスト関数があり得ても安全）。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本動作（解決、優先順位、関数退出でのローカルクリア）をカバー。以下の追加テストを提案。

- ブロックスコープのローカル寿命（現状は失敗する想定）
  ```rust
  #[test]
  fn test_block_scope_lifetime() {
      let file_id = FileId::new(1).unwrap();
      let mut ctx = CResolutionContext::new(file_id);
      let id_block = SymbolId::new(100).unwrap();
      ctx.enter_scope(ScopeType::Function { hoisting: false });
      ctx.enter_scope(ScopeType::Block);
      ctx.add_local("x".to_string(), id_block);
      assert_eq!(ctx.resolve("x"), Some(id_block));
      ctx.exit_scope(); // exit Block
      // 期待: ブロック退出でxは不可視
      // 現実: クリアされないためSome(id_block)になるバグの可能性
      assert_eq!(ctx.resolve("x"), None);
  }
  ```
- resolve_methodの循環防止
  ```rust
  #[test]
  fn test_resolve_method_cycle() {
      let mut inh = CInheritanceResolver::new();
      // X composed of Y, Y composed of X -> 循環
      inh.add_inheritance("X".to_string(), "Y".to_string(), "composition");
      inh.add_inheritance("Y".to_string(), "X".to_string(), "composition");
      inh.add_type_methods("X".to_string(), vec!["foo".to_string()]);
      // 期待: 循環でも安全に探索する（visited導入後にパス）
      // 現状: resolve_methodがvisitedなしで無限再帰の危険
      assert_eq!(inh.resolve_method("Y", "foo"), Some("X".to_string()));
  }
  ```
- typedef連鎖とサイクル打ち切り
  ```rust
  #[test]
  fn test_typedef_chain_and_cycle() {
      let mut inh = CInheritanceResolver::new();
      inh.add_typedef("A".to_string(), "B".to_string());
      inh.add_typedef("B".to_string(), "C".to_string());
      assert_eq!(inh.resolve_typedef("A"), "C");
      // 循環
      inh.add_typedef("C".to_string(), "A".to_string());
      let result = inh.resolve_typedef("A");
      // 期待: 訪問検出で打ち切り（何らかの安定値を返す）
      assert!(result == "C" || result == "A" || result == "B"); // 実装の挙動に依存
  }
  ```
- includesの重複
  ```rust
  #[test]
  fn test_duplicate_includes() {
      let file_id = FileId::new(1).unwrap();
      let mut ctx = CResolutionContext::new(file_id);
      ctx.add_include("stdio.h".to_string());
      ctx.add_include("stdio.h".to_string());
      assert_eq!(ctx.includes().len(), 2); // 現実: 重複保持
  }
  ```

統合テスト案:
- パーサと連携して、関数パラメータ・ローカル・グローバル・ヘッダ由来シンボルを実際に登録し、`resolve`で期待どおりにヒットするか検証。
- 大規模ヘッダ（typedef多数、構成多数）で`get_all_methods`の正確性と性能（時間・メモリ）を検証。

## Refactoring Plan & Best Practices

- スコープスタックのローカル管理を**ネスト可能な`Vec<HashMap<...>>`**へ:
  - enter_scope(Block/Function)で`push`、exit_scopeで`pop`、`resolve`は上から順。
  - これによりブロック退出で自動的にローカルが不可視化。
- `resolve_method`へ**visited導入**:
  - シグネチャを内部的に`fn resolve_method_inner(&self, type_name: &str, method: &str, visited: &mut HashSet<String>)`に分離。
- `includes`重複排除:
  - `HashSet<String>`または`Vec<String>`＋`contains`チェック。
- 文字列処理の最適化:
  - `symbols_in_scope`の大量cloneを最小化（必要なら`Cow<'_, str>`やイテレータ返却検討）。
- APIドキュメント強化:
  - 「解決順序」や「ブロックスコープの扱い」の**仕様明記**（現状はコメントのみで一部矛盾）。

## Observability (Logging, Metrics, Tracing)

- ログ（低頻度・デバッグレベル推奨）
  - スコープ遷移: enter/exit時に`scope_stack`の状態とクリア動作をログ。
  - 解決失敗: `resolve`で見つからないキーをデバッグログ（大量発生の可能性があるためレート制限）。
- メトリクス
  - `resolve`のヒット率（ローカル/モジュール/インポート/グローバル）。
  - `typedef`連鎖長の分布、構成グラフサイズ（V/E）。
- トレース
  - 連鎖/再帰探索（typedef・構成）にスパンを付与して長い探索の可視化。

このチャンクには観測機構の実装は現れない。

## Risks & Unknowns

- 不明点
  - `ScopeType`の全バリアントと、ブロックスコープ扱いの意図（このチャンクには現れない）。
  - `ImportBinding`の完全な仕様（どのようなヘッダ解決を行うか）は不明。
  - `FileId`/`SymbolId`の内部表現・安定性は不明。
- リスク
  - 合成グラフ循環時の`resolve_method`無限再帰（⚠️重大）。
  - ブロック退出時のローカル残存による誤解決（仕様違反・品質低下）。
  - 大量の`includes`重複による不要メモリ消費。
  - `symbols_in_scope`の大量cloneによる一時領域増大。

以上の評価は、このチャンク内のコードに基づく。関数名は明記したが、行番号は「このチャンクには行番号情報がないため不明」。