# parsing\c\behavior.rs Review

## TL;DR

- 目的: **C言語向けのLanguageBehavior実装**として、モジュールパス生成、可視性、シンボル解決可能性、インポート管理などの振る舞いを提供。
- 公開API: **CBehavior::new**、**LanguageBehavior実装一式**（module_path_from_file, is_resolvable_symbol, is_symbol_visible_from_file 等）。
- 複雑箇所: **is_resolvable_symbol**のスコープ判定ロジック（ScopeContextに応じた多岐分岐）。
- 重大リスク: Windowsパス区切り（"\"）未対応による**モジュールパス不整合**、Cの`static`内部リンケージ未考慮による**可視性誤判定**、非UTF-8パスでの**None返却**未ハンドリング。
- Rust安全性: **unsafe不使用**。状態変更は**&self**経由のため、内部可変性（BehaviorState）の実装次第で**並行性の安全性不明**。
- パフォーマンス: ほぼ**O(1)**。パス文字列処理等は**O(n)**（n=パス長）。
- テスト優先: module_path_from_fileのOS差異、is_resolvable_symbolの分岐、is_symbol_visible_from_fileの可視性ポリシー。

## Overview & Purpose

このファイルは、C言語に特化したパーサ/リゾルバの振る舞いを実装するためのコンポーネントです。tree-sitterのC言語定義を用いて、次の責務を担います。

- C言語ファイルから**モジュールパス**を導出（"path/to/file.c" → "path::to::file"）。
- C言語における**可視性とシンボル解決可能性**の規則を適用（CはRustのような可視性修飾子は持たない）。
- **インポート管理**（ヘッダやモジュールパスにもとづく関連付け）。
- **関係種別のマッピング**（"uses" 等を一般化されたRelationKindに変換）。
- **解決コンテキスト**（ResolutionScope）や継承ルール（Cでは擬似的な"uses"）の提供。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | CBehavior | pub | C言語向けLanguageBehaviorの具象実装。tree-sitter言語ハンドルと状態を保持。 | Low |
| Struct(フィールド) | language: Language | private | tree-sitter C言語ハンドル | Low |
| Struct(フィールド) | state: BehaviorState | private | ファイル登録・インポート管理等の状態 | Med |
| Trait Impl | StatefulBehavior for CBehavior | n/a | 状態アクセス（state()）および状態付き補助メソッド利用 | Low |
| Trait Impl | LanguageBehavior for CBehavior | n/a | モジュール/可視性/解決可能性/関係/インポート/コンテキスト作成 | Med |
| Trait Impl | Default for CBehavior | n/a | new()の委譲 | Low |

### Dependencies & Interactions

- 内部依存
  - CBehavior.state（BehaviorState）への読み取り・更新（register_file_with_state, add_import_with_state, get_imports_from_state, get_module_path）。
  - StatefulBehaviorトレイトの**デフォルト実装メソッド**に依存している可能性（register_file_with_state 等）。実体はこのチャンク外で定義。状態更新を&selfで行うため、内部可変性が用いられていると推測。
  - CResolutionContext（ResolutionScopeの具象）を作成。
  - crate::parsing::resolution::GenericInheritanceResolver（InheritanceResolver）を作成。
- 外部依存（推定）
  | クレート/モジュール | 用途 |
  |----------------------|------|
  | tree_sitter_c::LANGUAGE | C言語用Languageハンドル |
  | tree_sitter::Language | 言語ハンドル型 |
  | crate::parsing::{LanguageBehavior, ResolutionScope, Import, InheritanceResolver} | トレイト/型 |
  | crate::parsing::behavior_state::{BehaviorState, StatefulBehavior} | 状態管理用 |
  | crate::relationship::RelationKind | 関係種別の汎用表現 |
  | super::resolution::CResolutionContext | C向け解決コンテキスト |
  | crate::{FileId, Visibility, Symbol, SymbolKind} | メタデータ/可視性/シンボル定義 |
- 被依存推定
  - プロジェクト全体の**パーサ/シンボル解決**機構からC言語モードとして利用。
  - Cファイルの**モジュールパス整形**や**インポート解決**に関与。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CBehavior::new | pub fn new() -> Self | C言語用振る舞いの生成 | O(1) | O(1) |
| Default::default | fn default() -> Self | new()の委譲 | O(1) | O(1) |
| StatefulBehavior::state | fn state(&self) -> &BehaviorState | 内部状態への参照取得 | O(1) | O(1) |
| LanguageBehavior::format_module_path | fn format_module_path(&self, base_path:&str, symbol_name:&str)->String | "{base}::{symbol}"整形 | O(n) | O(n) |
| LanguageBehavior::parse_visibility | fn parse_visibility(&self, _signature:&str)->Visibility | Cでは全てPublic扱い | O(1) | O(1) |
| LanguageBehavior::module_separator | fn module_separator(&self)->&'static str | "::"提供 | O(1) | O(1) |
| LanguageBehavior::supports_traits | fn supports_traits(&self)->bool | Cはfalse | O(1) | O(1) |
| LanguageBehavior::supports_inherent_methods | fn supports_inherent_methods(&self)->bool | Cはfalse | O(1) | O(1) |
| LanguageBehavior::get_language | fn get_language(&self)->Language | tree-sitter言語ハンドル取得 | O(1) | O(1) |
| LanguageBehavior::module_path_from_file | fn module_path_from_file(&self, file_path:&Path, project_root:&Path)->Option<String> | パス→モジュールパス変換 | O(n) | O(n) |
| LanguageBehavior::create_resolution_context | fn create_resolution_context(&self, file_id:FileId)->Box<dyn ResolutionScope> | C用解決スコープ生成 | O(1) | O(1) |
| LanguageBehavior::create_inheritance_resolver | fn create_inheritance_resolver(&self)->Box<dyn InheritanceResolver> | 汎用継承リゾルバ（uses）生成 | O(1) | O(1) |
| LanguageBehavior::is_resolvable_symbol | fn is_resolvable_symbol(&self, symbol:&crate::Symbol)->bool | シンボルが解決対象か判定 | O(1) | O(1) |
| LanguageBehavior::format_method_call | fn format_method_call(&self, receiver:&str, method:&str)->String | Cの関数呼び形式へ変換 | O(n) | O(n) |
| LanguageBehavior::inheritance_relation_name | fn inheritance_relation_name(&self)->&'static str | Cでは"uses" | O(1) | O(1) |
| LanguageBehavior::map_relationship | fn map_relationship(&self, language_specific:&str)->RelationKind | 関係名→RelationKind変換 | O(1) | O(1) |
| LanguageBehavior::register_file | fn register_file(&self, path:PathBuf, file_id:FileId, module_path:String) | 状態へファイル登録 | O(1) | O(1) |
| LanguageBehavior::add_import | fn add_import(&self, import:crate::parsing::Import) | 状態へインポート追加 | O(1) | O(1) |
| LanguageBehavior::get_imports_for_file | fn get_imports_for_file(&self, file_id:FileId)->Vec<Import> | ファイルのインポート一覧取得 | O(k) | O(k) |
| LanguageBehavior::is_symbol_visible_from_file | fn is_symbol_visible_from_file(&self, symbol:&crate::Symbol, from_file:FileId)->bool | 可視性判定 | O(1) | O(1) |
| LanguageBehavior::get_module_path_for_file | fn get_module_path_for_file(&self, file_id:FileId)->Option<String> | モジュールパスを状態から取得 | O(1) | O(1) |
| LanguageBehavior::import_matches_symbol | fn import_matches_symbol(&self, import_path:&str, symbol_module_path:&str, _importing_module:Option<&str>)->bool | インポートとシンボルの一致判定 | O(n) | O(1) |

以下、主なAPIの詳細。

1) 目的と責務
- CBehavior::new
  - 目的: **C向け振る舞いの初期化**（tree-sitter言語ハンドル確保、状態初期化）。
- module_path_from_file
  - 目的: **ファイルパスからモジュールパス生成**（拡張子除去、"/"→"::"置換、空は"root"）。
- is_resolvable_symbol
  - 目的: **解決対象シンボルのフィルタ**（スコープや種別に基づく判定）。
- is_symbol_visible_from_file
  - 目的: **ファイル間可視性判定**（Cでは基本Public扱い）。
- register_file / add_import / get_imports_for_file / get_module_path_for_file
  - 目的: **状態（BehaviorState）との連携**。ファイル登録・インポート管理。
- map_relationship / inheritance_relation_name
  - 目的: **Cの関係性表現を一般化**（"uses"等）。
- format_method_call
  - 目的: **Cの関数呼びスタイル表現**（"method(receiver)"）。

2) アルゴリズム（ステップ分解、主要なもの）
- module_path_from_file
  1. file_pathからproject_rootをstrip_prefix（失敗ならNone）。
  2. 相対パスを文字列化（非UTF-8ならNone）。
  3. ".c"または".h"拡張子をstrip_suffix。
  4. '/'を"::"へ置換。
  5. 空なら"root"を返し、Some(module_path)。
- is_resolvable_symbol
  1. symbol.scope_contextがSomeかを確認。
  2. Someの場合、ScopeContextで分岐:
     - Module/Global/Package → true
     - Local/Parameter → false
     - ClassMember → kindがMethodかvisibilityがPublicならtrue、そうでなければfalse
  3. Noneの場合、kindがFunction/Struct/Enum/Constantならtrue、その他はfalse。
- is_symbol_visible_from_file
  1. 同一ファイルならtrue。
  2. VisibilityがPublic/Crateならtrue、それ以外はfalse。

3) 引数（例: module_path_from_file）

| 引数名 | 型 | 必須 | 意味 |
|--------|----|------|------|
| file_path | &Path | はい | 対象ファイルの絶対/相対パス |
| project_root | &Path | はい | プロジェクトルートのパス |

4) 戻り値（例: module_path_from_file）

| 戻り値 | 型 | 条件 |
|--------|----|------|
| module_path | Option<String> | 成功時Some、失敗時None（非UTF-8やstrip_prefix失敗） |

5) 使用例

```rust
use std::path::Path;
use crate::parsing::c::behavior::CBehavior;

let beh = CBehavior::new();
// Linux系の例
let file = Path::new("/proj/src/foo/bar.c");
let root = Path::new("/proj");
let mp = beh.module_path_from_file(file, root);
assert_eq!(mp.as_deref(), Some("src::foo::bar"));

// メソッド呼びの文字列（Cでは関数呼び）
let call = beh.format_method_call("ctx", "process");
assert_eq!(call, "process(ctx)");
```

6) エッジケース
- module_path_from_file
  - 非UTF-8パス → None
  - Windows区切り（"\"）→ 置換対象外で"::"に変換されず不整合
  - 拡張子が大文字（".C", ".H"）→ strip_suffixできない
- is_resolvable_symbol
  - scope_contextがNone → kindに依存したフォールバック
  - ClassMemberだがCでメソッド概念が薄い → 想定外ケースの混入可能性
- is_symbol_visible_from_file
  - parse_visibilityが常にPublic → Cの`static`内部リンケージ未反映

## Walkthrough & Data Flow

- CBehavior::new → tree_sitter_c::LANGUAGEを取り込み、BehaviorState::new()で初期化。
- register_file → StatefulBehaviorの補助メソッド register_file_with_state(...) 経由で state にファイル情報を登録。
- add_import → StatefulBehaviorの補助メソッド add_import_with_state(...) を介し state にインポートを追加。
- get_imports_for_file → StatefulBehaviorの補助メソッド get_imports_from_state(file_id) で状態から取得。
- module_path_from_file → Path操作→文字列整形→モジュールセパレータ変換→返却。
- is_resolvable_symbol → scope_contextの有無で分岐→スコープ/種別に応じた真偽値。
- is_symbol_visible_from_file → file_id一致チェック→Visibilityに従った可視性。

Mermaid図（is_resolvable_symbolの主分岐）

```mermaid
flowchart TD
  A[入力: symbol] --> B{scope_contextがあるか?}
  B -- Yes --> C{ScopeContext}
  C -- Module/Global/Package --> D[true]
  C -- Local/Parameter --> E[false]
  C -- ClassMember --> F{kind==Method または visibility==Public?}
  F -- Yes --> D
  F -- No --> E
  B -- No --> G{kind in {Function,Struct,Enum,Constant}?}
  G -- Yes --> D
  G -- No --> E
```

上記の図は`is_resolvable_symbol`関数（行番号不明・このチャンク内）の主要分岐を示す。

## Complexity & Performance

- 全体: ほぼ**O(1)**。文字列整形系は**O(n)**（n=文字列長）。
- ボトルネック:
  - module_path_from_fileの**文字列作成・置換**。
- スケール限界:
  - 膨大なファイル数での**状態管理**（BehaviorState）の内部構造次第。get_imports_for_fileは返却サイズkに比例。
- 実運用負荷要因:
  - I/O/ネットワーク/DBは本ファイルでは**未関与**。
  - **OS差異**（パス区切り）による不整合がパスベース解決に影響。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 非UTF-8パス | OsStrがUTF-8でない | Some("...")か安全なフォールバック | to_str()?でNone→None返却 | 設計どおりだがフォールバック欠如 |
| Windows区切り未対応 | "src\\lib\\a.c" | "src::lib::a" | '/'のみ"::"に置換 | Bug（OS依存不整合） |
| 大文字拡張子 | "file.C" | ".C"も除去 | "c","h"のみstrip_suffix対象 | Bug（拡張子大小無視なし） |
| project_rootと一致 | file_path==project_root | "root" | 空→"root" | OK |
| Cの内部リンケージ | `static`関数/変数 | 他ファイルから不可視 | parse_visibilityがPublic固定 | Limitation（可視性誤判定） |
| import一致厳格 | import_path="a::b", symbol_module_path="a::b::c" | 部分一致/ワイルドカードなど | 完全一致のみ | Limitation |
| scope_context欠如 | symbol.scope_context=None | 後方互換の安全な判定 | kindに依存してtrue/false | OK（互換） |

セキュリティチェックリスト
- メモリ安全性: unsafe未使用、**Buffer overflow/Use-after-free/Integer overflow**の懸念なし。
- インジェクション: **SQL/Command/Path traversal**なし（パス文字列処理のみ、外部I/Oなし）。
- 認証・認可: 該当なし。
- 秘密情報: **ハードコード秘密**なし。ログ出力なし。
- 並行性: **Race/Deadlock**対策はコード上不明。BehaviorStateが内部可変性を用いる可能性が高く、**Send/Sync**や同期プリミティブの有無はこのチャンクでは不明。

Rust特有の観点
- 所有権: get_languageは**clone**で返却（所有権の移動なし）。state()は**不変参照**返却。
- 借用: 状態更新系が&selfで呼ばれているため、**内部可変性**（例: RefCell/Mutex）前提。詳細は不明。
- ライフタイム: 明示的ライフタイム不要。返却はOwned（String）またはClone（Language）。
- unsafe境界: **unsafeブロックなし**。
- 並行性・非同期: **非同期処理なし**。**Send/Sync**境界やデータ競合対策は**不明**。
- エラー設計: Option/Resultの使い分けは限定的。module_path_from_fileは**Option**で失敗を表現。**panic**（unwrap/expect）は不使用。エラー変換は**不明**。

## Design & Architecture Suggestions

- パス処理のOS非依存化
  - Path::components()で**成分を"::"でjoin**することで、"/"や"\\"の差異を解消。
  - 拡張子除去は**case-insensitive**対応（.c/.hを小文字化して比較）。
- Cの可視性の向上
  - tree-sitter ASTから**ファイルスコープ`static`**を検出し、Visibility::Private相当へマップする。
  - ヘッダ（.h）で宣言されたかどうかのメタデータ管理で**Public**/**Private**をより正確に。
- is_resolvable_symbolのC向け単純化
  - ScopeContext::ClassMember分岐は**Cでは不自然**。C専用ロジックに絞る（関数/struct/enum/constant）。
- import一致の拡張
  - **前方一致**やワイルドカード、相対モジュール（"::"接頭辞の扱い）をサポート。
- API契約の明示
  - module_path_from_fileの**None条件**（非UTF-8/ルート外）をドキュメント化。
- 並行性対策
  - BehaviorStateを**Arc<RwLock<...>**等で保護し、**Send/Sync**を明示。スレッド安全なread/writeの提供。

## Testing Strategy (Unit/Integration) with Examples

- module_path_from_file
  - Linux/Unixパス、Windows風パス、非UTF-8、拡張子大小差異、project_root一致ケース。
```rust
#[test]
fn unix_path_to_module_path() {
    let beh = CBehavior::new();
    let file = std::path::Path::new("/p/src/foo/bar.c");
    let root = std::path::Path::new("/p");
    assert_eq!(beh.module_path_from_file(file, root).as_deref(), Some("src::foo::bar"));
}

#[test]
fn windows_like_path_not_converted() {
    let beh = CBehavior::new();
    // 非Windows環境でも、文字列としての'\'が残ることを確認
    let file = std::path::Path::new("src\\lib\\a.c");
    let root = std::path::Path::new("src\\lib");
    // strip_prefixはOS依存で失敗する可能性があるため、ここでは相対想定の例
    // 実際のWindows環境でのE2Eテストを推奨
    let res = beh.module_path_from_file(file, root);
    assert!(res.is_some());
    assert_eq!(res.unwrap(), "a"); // 現実には"\"が残る可能性。改善後は"::"を期待
}
```

- is_resolvable_symbol
  - ScopeContext別（Module/Global/Package/Local/Parameter/ClassMember）とkind別（Function/Struct/Enum/Constant/その他）を網羅。
- is_symbol_visible_from_file
  - 同一file_id、異なるfile_idでVisibility各種。Cの`static`未考慮の現状仕様を検証。
- import_matches_symbol
  - 完全一致、部分一致不可ケースの確認。
- register_file/add_import/get_imports_for_file
  - 状態に登録・取得できるか（BehaviorStateのモック/テストダブルが必要）。

Integrationテスト
- Cファイル群とヘッダを用いた**モジュール→インポート→解決**までのシナリオを構築。

## Refactoring Plan & Best Practices

- module_path_from_fileの改善
  - Path::components().map(|c| c.to_string_lossy()).collect::<Vec<_>>().join("::") を用いる。
  - 拡張子比較は小文字化（to_ascii_lowercase）して ".c"/".h" を除去。
- 可視性の正確化
  - ASTから**storage_class_specifier("static")**検出 → Visibility::Private。
  - ヘッダ宣言の情報をstateに保持し**Public**判定。
- C専用分岐へ最適化
  - is_resolvable_symbolのClassMember分岐を**C向けにオフ**または例外扱い。
- importマッチングの強化
  - **前方一致**や**エイリアス**（typedef等）を考慮した柔軟マッチ。
- ドキュメントと契約
  - 各APIの**失敗条件/返却契約**を明示。
- 並行性
  - **Send/Sync境界**をトレイト実装や型に明示し、**データ競合**を防止。

## Observability (Logging, Metrics, Tracing)

- ログ
  - module_path_from_fileが**None**を返すケース（非UTF-8/strip_prefix失敗）で**warnログ**。
  - register_file/add_import成功/失敗時の**debugログ**。
- メトリクス
  - **解決成功率**、**インポート件数**、**モジュール生成失敗数**。
- トレーシング
  - **ファイル登録→インポート→解決**のスパンを発行し、ボトルネック可視化。

## Risks & Unknowns

- BehaviorState内部
  - **内部可変性の実装不明**（RefCell/Mutex等）。並行アクセス時の安全性や**Send/Sync**対応が不明。
- CResolutionContext/GenericInheritanceResolverの仕様
  - どのように**関係/継承（uses）**を扱うか詳細不明。
- Cの可視性
  - `static`やヘッダ宣言の扱いが現状ロジックに反映されていない。**誤判定リスク**。
- OS依存
  - Windowsパスの扱いが現状**不完全**で、モジュールパスやインポート一致に**不整合を生む**可能性。
- 非UTF-8パス
  - **None返却**で上位がどう扱うか不明。フォールバックルールの欠如。