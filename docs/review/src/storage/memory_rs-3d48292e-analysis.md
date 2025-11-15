# memory.rs Review

## TL;DR

- 目的: **SymbolStore**はメモリ内で**Symbol**を管理し、ID/名前/ファイル/種別/位置で高速検索できる並行安全なインデックス付きストア。
- 主な公開API: new/insert/get/find_by_name/find_by_file/find_by_kind/find_at_position/remove/clear/len/is_empty/iter/to_vec/as_ids_for_name（全てスレッドセーフ）。
- 複雑箇所: 複数インデックス（by_name/by_file/symbols）の整合性維持。再挿入（同一ID差し替え）時のインデックス更新は非原子的で重複が生じ得る。
- 重大リスク: インデックスの**重複ID**・**陳腐化**（by_name/by_fileに残存）と**整合性の非原子性**。大量データ時にby_name/by_fileのVec操作がボトルネック。
- Rust安全性: **unsafe不使用**、**DashMap + Arc**で並行安全。ただし複数マップに跨る更新はトランザクションなしの整合性リスク。
- エラー設計: get/removeは**Option**返却で不在を表現。検索系は空Vec返却で扱いやすい。
- パフォーマンス: find_by_kindが**O(n)**、名前/ファイル検索はインデックスにより**O(k)**。削除時のretainが**O(k)**で大きなベクタでは負荷。

## Overview & Purpose

このファイルは、並行アクセス可能なメモリ内**シンボルストア**を提供します。主な責務は以下の通りです。

- シンボル（関数、構造体など）をIDをキーに保存
- 名前・ファイル単位の補助インデックスを維持
- 種別・ファイル内位置（行・列）での検索
- スレッドセーフな挿入・削除・列挙

内部的には**DashMap**を**Arc**でラップし、ストア自体をクローンして複数スレッドから共有できる設計です。

対象の型（Symbol, SymbolId, FileId, SymbolKind, Range）の詳細はこのチャンクには現れないため、不明。ただしテストより以下の利用前提が読み取れます。

- SymbolはClone可能、フィールドとしてid/name/kind/file_id/rangeを持つ（Symbol::new使用から推測）
- Rangeはcontains(line, column)を提供（find_at_positionで使用）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | SymbolStore | pub | シンボルの保存・検索・削除・列挙。名前/ファイルの補助インデックス維持 | Med |
| Field | symbols | private | SymbolId→Symbolの主インデックス（Arc<DashMap>） | Low |
| Field | by_name | private | name→Vec<SymbolId>の補助インデックス（Arc<DashMap>） | Med |
| Field | by_file | private | FileId→Vec<SymbolId>の補助インデックス（Arc<DashMap>） | Med |
| Impl | Default | pub | new()と同等の初期化 | Low |
| Mod | tests | private | 単体テスト群 | Low |

### Dependencies & Interactions

- 内部依存
  - insert → symbols.insert, by_name.entry(...).or_default().push, by_file.entry(...).or_default().push
  - find_by_name → by_name.get → get（symbols）
  - find_by_file → by_file.get → get（symbols）
  - find_by_kind → symbols.iter → filter by entry.kind
  - find_at_position → find_by_file → Range::contains
  - remove → symbols.remove → by_name.get_mut → Vec::retain、by_file.get_mut → Vec::retain
  - iter/to_vec → symbols.iterでSymbolをclone
- 外部依存（クレート/モジュール）
  | 依存 | 用途 |
  |------|------|
  | dashmap::DashMap | スレッドセーフなHashMap |
  | std::sync::Arc | 共有所有権でクローン可能に |
  | crate::{FileId, Symbol, SymbolId, SymbolKind} | ドメイン型 |
  | crate::Range（tests） | 位置範囲判定 |
- 被依存推定
  - パーサ/インデクサが解析結果をinsert/insert_batchで投入
  - LSP的機能（定義ジャンプ、シンボル検索）がfind_*を利用
  - リンカ/ナビゲーション機能がfind_at_positionを利用
  - 解析後のクリーンアップでclear/removeを利用

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | `pub fn new() -> Self` | 空ストアの生成 | O(1) | O(1) |
| insert | `pub fn insert(&self, symbol: Symbol) -> SymbolId` | シンボル登録（インデックス更新） | O(1) 平均＋O(|name|) | O(1) 1件分 |
| insert_batch | `pub fn insert_batch(&self, symbols: impl IntoIterator<Item = Symbol>)` | 複数シンボルの一括登録 | O(m) | O(m) |
| get | `pub fn get(&self, id: SymbolId) -> Option<Symbol>` | IDで取得 | O(1) 平均 | O(size(Symbol))（clone） |
| find_by_name | `pub fn find_by_name(&self, name: &str) -> Vec<Symbol>` | 名前で検索 | O(k) | O(k) |
| find_by_file | `pub fn find_by_file(&self, file_id: FileId) -> Vec<Symbol>` | ファイルIDで検索 | O(k) | O(k) |
| find_by_kind | `pub fn find_by_kind(&self, kind: SymbolKind) -> Vec<Symbol>` | 種別で全件走査検索 | O(n) | O(n) |
| find_at_position | `pub fn find_at_position(&self, file_id: FileId, line: u32, column: u16) -> Option<Symbol>` | ファイル内位置で検索 | O(k) | O(1) |
| remove | `pub fn remove(&self, id: SymbolId) -> Option<Symbol>` | IDで削除（インデックス更新） | O(1)+O(kn)+O(kf) | O(1) |
| clear | `pub fn clear(&self)` | 全消去 | O(n) | O(1) |
| len | `pub fn len(&self) -> usize` | 件数取得 | O(1) | O(1) |
| is_empty | `pub fn is_empty(&self) -> bool` | 空判定 | O(1) | O(1) |
| iter | `pub fn iter(&self) -> impl Iterator<Item = Symbol> + '_` | 全件イテレータ（clone） | O(n) 消費時 | O(1) |
| to_vec | `pub fn to_vec(&self) -> Vec<Symbol>` | 全件Vec化 | O(n) | O(n) |
| as_ids_for_name | `pub fn as_ids_for_name(&self, name: &str) -> Option<Vec<SymbolId>>` | 名前に紐づくID一覧取得 | O(k) | O(k) |
| Default | `impl Default for SymbolStore` | new同等初期化 | O(1) | O(1) |

重要事項の根拠（関数名:行番号）は、このチャンクには行番号メタがないため「行番号:不明」と記します。

- 整合性リスク（insert/removeの非原子的更新）: SymbolStore::insert / SymbolStore::remove（行番号:不明）
- O(n)の全件走査（find_by_kind）: SymbolStore::find_by_kind（行番号:不明）

以下、主要APIを詳細化します（全API網羅、簡潔記載）。

1) new
- 目的と責務: 空のストアを作成。内部に3つのDashMapを持つ。
- アルゴリズム: Arc::new(DashMap::new())を3回生成して構造体に詰める。
- 引数: なし
- 戻り値: 新規SymbolStore
- 使用例:
```rust
let store = SymbolStore::new();
assert!(store.is_empty());
```
- エッジケース: 特になし。

2) insert
- 目的と責務: 1件のSymbolを登録し、主/補助インデックス更新。
- アルゴリズム:
  1. id/name/file_idを抽出
  2. symbols.insert(id, symbol)で登録（既存同IDは置換）
  3. by_name.entry(name).or_default().push(id)
  4. by_file.entry(file_id).or_default().push(id)
  5. id返却
- 引数:
  | 名 | 型 | 説明 |
  |----|----|------|
  | symbol | Symbol | 登録対象 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | SymbolId | 登録したID |
- 使用例:
```rust
let id = store.insert(symbol.clone());
assert_eq!(store.get(id).unwrap().id, id);
```
- エッジケース:
  - 同一IDを再挿入（差し替え）でby_name/by_fileに重複IDが蓄積し得る（バグ、後述）。
  - name.to_string()コストが文字列長に比例。

3) insert_batch
- 目的: 複数Symbolの一括登録（forでinsertを呼ぶ）。
- 使用例:
```rust
store.insert_batch(vec![s1, s2, s3]);
assert_eq!(store.len(), 3);
```
- エッジケース: 中途でpanicはしないが、途中まで挿入済みになる可能性（原子的ではない）。

4) get
- 目的: IDからSymbolを取得（cloneして返却）。
- 使用例:
```rust
if let Some(sym) = store.get(id) {
    println!("found: {}", sym.name.as_ref());
}
```
- エッジケース: 不在ならNone。cloneコストあり。

5) find_by_name
- 目的: 名前に紐づくIDベクタからSymbolを収集。
- アルゴリズム: by_name.get(name)→ids.iter→self.get(*id)のfilter_map→collect
- 使用例:
```rust
let syms = store.find_by_name("foo");
```
- エッジケース:
  - インデックスに陳腐IDが残ってもget(None)が捨てるため安全だが重複はそのまま返る可能性。

6) find_by_file
- 目的: ファイルIDで検索。find_by_nameと同様。
- 使用例:
```rust
let syms = store.find_by_file(file_id);
```
- エッジケース: 重複ID返却の可能性。

7) find_by_kind
- 目的: 全件走査してkind一致を抽出。
- 使用例:
```rust
let funcs = store.find_by_kind(SymbolKind::Function);
```
- エッジケース: nが大きいとコスト増。並行反復でロックの粒度に注意。

8) find_at_position
- 目的: 同ファイル内でRange.contains(line, column)に一致する最初のSymbol。
- アルゴリズム: find_by_file(file_id)→into_iter→find(contains)
- 使用例:
```rust
let s = store.find_at_position(file_id, 12, 0);
```
- エッジケース: Range境界仕様（閉区間/半開区間）は不明。

9) remove
- 目的: IDによる削除。主インデックスから削除後、補助インデックスからIDをretainで除去。
- 使用例:
```rust
if let Some(removed) = store.remove(id) {
    assert!(store.get(id).is_none());
}
```
- エッジケース:
  - 既存でないIDならNone。
  - 補助インデックスでVec::retainがO(k)。大きいベクタで負荷。
  - 同一IDの重複がある場合、retainにより全て除去されるが、別名/別ファイルに誤って残る可能性（再挿入で属性変更時）。

10) clear
- 目的: 3つのインデックス全消去。
- 使用例:
```rust
store.clear();
assert!(store.is_empty());
```

11) len/is_empty
- 目的: 件数メタ情報。
- 使用例:
```rust
if !store.is_empty() { println!("count: {}", store.len()); }
```

12) iter
- 目的: 全件クローンを流すイテレータ。to_vecより割り当て回避。
- 使用例:
```rust
for sym in store.iter() { /* use sym */ }
```
- エッジケース: 大量cloneによるコスト。長寿命の参照は不可（クローン前提）。

13) to_vec
- 目的: 全件Vec化。イテレータと同様clone。
- 使用例:
```rust
let all = store.to_vec();
```

14) as_ids_for_name
- 目的: 名前→IDのスナップショット（Vecをclone）。コメントは参照返しと言うが実際は所有Vec。
- 使用例:
```rust
if let Some(ids) = store.as_ids_for_name("foo") {
    assert!(!ids.is_empty());
}
```
- エッジケース: Optionで不在を表現。重複IDもそのまま。

データ契約（推測）
- Symbol: Clone + Debug、name/kind/file_id/rangeを保持。idは一意。
- Range.contains(u32,u16): 真偽値返却（詳細は不明）。
- SymbolId/FileId: new(u32).unwrap()をテストが使用（不正値でErrの可能性）。

## Walkthrough & Data Flow

- 新規作成
  - new → 空のArc<DashMap>が3つ生成され、共有可能な**SymbolStore**ができる。
- 挿入フロー
  - insert(symbol):
    - symbols.insert(id, symbol)で主インデックスへ
    - by_name.entry(name).or_default().push(id)
    - by_file.entry(file_id).or_default().push(id)
    - 注: 複数マップの更新は非原子的
- 取得フロー
  - get(id): symbols.get(&id).map(clone)
- 検索フロー（名前/ファイル）
  - find_by_name(name): by_name.get(name)→Vec<SymbolId>を走査→symbols.getで存在するSymbolのみ返す
  - find_by_file(file_id): 同様
- 種別検索
  - find_by_kind(kind): symbols.iter()で全件走査→一致をclone収集
- 位置検索
  - find_at_position(file_id, line, col): find_by_file(file_id)→Range.containsで最初の一致を返す
- 削除フロー
  - remove(id): symbols.remove(&id)→取得したsymbolからname/file_idを参照→by_name/by_fileのVecからretainでidを削除
- クリア
  - clear(): 3インデックスのclear呼び出し
- 列挙
  - iter(): symbols.iter().map(Clone)
  - to_vec(): iter().collect()

非同期/並行性
- **DashMap**はシャード毎のロックでスレッドセーフ。Arcにより**SymbolStore**をクローンして複数スレッドで共有可能。
- ただし複数インデックス（symbols/by_name/by_file）の更新は一括ロックではないため**整合性は最終的整合性**。一時的不整合は検索側でfilter_mapが緩和。

## Complexity & Performance

- 時間計算量
  - insert: 平均O(1)（ハッシュ）＋push O(1)（amortized）＋name.to_string O(|name|)
  - insert_batch: O(m)（各insertの合計）
  - get: O(1)平均＋cloneコスト
  - find_by_name/find_by_file: O(k)（ID数に比例）＋各clone
  - find_by_kind: O(n)（全件）
  - find_at_position: O(k)
  - remove: O(1)（主）＋O(kn)+O(kf)（補助のretain）
  - clear: O(n)
- 空間計算量
  - 主インデックス: O(n)
  - 補助インデックス: O(n)（重複があると>n）
  - to_vec: O(n)
- ボトルネック/スケール限界
  - 種別検索の全件走査（nが大きいプロジェクトで重い）
  - 補助インデックスがVecのため、削除時のretainがコスト高（kが大きい名前/ファイルで顕著）
  - 再挿入による重複IDで補助インデックスが膨張しやすい
- 実運用負荷要因
  - 多スレッド同時insertに伴うシャードロック競合
  - 長い名前のto_stringコスト
  - 大規模ファイル/名前に偏るシンボル分布でVecが肥大化

## Edge Cases, Bugs, and Security

セキュリティチェックリスト
- メモリ安全性: unsafe未使用。**バッファオーバーフロー/Use-after-free/整数オーバーフロー**の懸念なし。
- インジェクション: SQL/Command/Path等は未使用。該当なし。
- 認証・認可: 機能なし。該当なし。
- 秘密情報: ハードコード秘密・ログ漏えいなし。該当なし。
- 並行性: **DashMap**でキー/シャード単位のロックによりデータ競合は回避。ただし**多インデックス更新の非原子性**により一時的不整合が生じ得る。

詳細エッジケース

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 同一ID再挿入（属性変更） | insert(s1{name="A"}), insert(s1{name="B"}) | 旧インデックスからID除去、新インデックスに追加 | 旧インデックス除去が行われずpushのみ（重複・陳腐化） | Bug |
| 同一ID再挿入（同名同ファイル） | insert(id=1), insert(id=1) | インデックス重複回避 | Vecに同じidが複数push | Bug |
| removeで存在しないID | remove(不存在ID) | Noneを返す | symbols.removeがNoneでそのままNone返却 | OK |
| find_by_nameで陳腐ID | by_nameに残存ID、symbolsに無 | 無視して返却から除外 | filter_mapでget(None)を捨てる | OK（不整合緩和） |
| clearの並行呼び出し | 他スレッドがinsert中 | 全インデックスが空になる。途中のinsertは成功する可能性 | clearは各マップを個別にclear | 注意（非原子的） |
| 大量の同名シンボル | 同名でk=10^5 | find/removeのVec走査が重い | Vec::retain/iterがO(k) | Perf Risk |
| as_ids_for_nameの返却 | 参照取得期待 | 所有Vecを返す | コメントと挙動がズレ | Minor UX |

根拠（関数名:行番号:不明）
- 重複発生: SymbolStore::insertのby_name/by_file.push処理
- 陳腐化: SymbolStore::insertが旧インデックスからの除去を行っていない
- 緩和: SymbolStore::find_by_name/find_by_fileがself.getのOptionをfilter_map

## Design & Architecture Suggestions

- インデックス重複対策
  - Vecではなく**DashSet<SymbolId>**を使用して重複を回避（集合化）。
  - もしくはVec維持ならpush前に重複チェック（O(k)）だがコスト高。集合の方が良い。
- 原子的更新の提供
  - **upsert**APIを導入: 旧Symbol（同ID）の存在を確認し、名前/ファイルが変わる場合は旧インデックスから除去→新インデックスへ追加を一連のスコープで実施。
  - 1つのキーに対する「トランザクション風」操作はDashMapのwriteロックで粒度を上げる（ただし複数マップ跨ぎの完全原子化は困難）。
- インデックス構造の分離
  - `struct Indices { by_name: DashMap<String, DashSet<SymbolId>>, by_file: DashMap<FileId, DashSet<SymbolId>> }`を切り出し、更新ロジックを一箇所に集約。
- 取得のコスト削減
  - `iter()`で`&Symbol`の参照を返すのが理想だがDashMapの参照ライフタイムとイテレータ設計上難しいため、現状のclone戦略は妥当。必要なら`Arc<Symbol>`保管に切り替え、cloneを軽量化。
- 種別検索の効率化
  - `by_kind: DashMap<SymbolKind, DashSet<SymbolId>>`の補助インデックスを追加して`find_by_kind`をO(k)化（登録/削除時更新のコストは増加）。
- API整合性
  - `as_ids_for_name`のコメントを実装に合わせて更新（参照ではなく所有Vecを返す旨）。
  - 名前キーのto_stringを避けるため、Symbol.nameの内部表現に合わせてclone/borrow（例: `name.clone()`）に変更し、変換コスト削減。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本動作・並行挿入をカバー。以下を追加推奨。

- 同一ID再挿入での重複検出
```rust
#[test]
fn test_reinsert_duplicate_index() {
    let store = SymbolStore::new();
    let mut s = create_test_symbol(1, "A", 1);
    store.insert(s.clone());
    s.name = "B".into(); // 型に応じて設定方法変更
    store.insert(s.clone());
    let ids_a = store.as_ids_for_name("A").unwrap_or_default();
    let ids_b = store.as_ids_for_name("B").unwrap_or_default();
    assert!(ids_a.is_empty(), "旧名のインデックスが残っている可能性");
    assert!(ids_b.len() == 1, "重複や更新漏れを検出");
}
```

- 重複挿入（同名同ファイル）でインデックス重複
```rust
#[test]
fn test_duplicate_push_same_id() {
    let store = SymbolStore::new();
    let s = create_test_symbol(1, "X", 1);
    store.insert(s.clone());
    store.insert(s.clone()); // 同一ID再挿入
    let ids = store.as_ids_for_name("X").unwrap();
    // Vecを使用している限り重複が存在し得る
    assert!(ids.iter().filter(|&&id| id == s.id).count() == 1, "重複を検出");
}
```

- clearの並行性
```rust
#[test]
fn test_clear_race() {
    use std::thread;
    let store = SymbolStore::new();
    for i in 0..1000 {
        store.insert(create_test_symbol(i, "f", 1));
    }
    let s2 = store.clone();
    let handle = thread::spawn(move || {
        s2.clear();
    });
    // 並行して取得
    let _ = store.find_by_name("f");
    handle.join().unwrap();
    assert!(store.is_empty());
}
```

- find_by_kindの性能境界（ベンチマーク）
  - 大量挿入後の所要時間計測（Criterionなど、別クレート）

- property-basedテスト（proptest）
  - ランダムな挿入/削除/検索シーケンスで整合性検証

## Refactoring Plan & Best Practices

1. インデックスをVec→DashSetへ変更（by_name/by_file）
   - insert: `.insert(id)`、remove: `.remove(&id)`でO(1)平均、重複排除
2. upsert APIの追加
   - 既存のinsertは新規のみ、変更はupsertが旧インデックス更新を担う
3. 名前処理の最適化
   - `symbol.name.clone()`を利用（型がString/Arc<str>等に応じ最適化）し、to_string頻度を減らす
4. by_kindインデックスの導入（必要に応じて）
   - 登録/削除のコスト増とトレードオフ
5. APIドキュメント整備
   - コメントの整合性（as_ids_for_name）と非原子的更新の注意点を明示
6. 大量データ向け最適化
   - retain多用を避ける設計（集合構造）と、SmallVec/IndexSetなど用途に応じた選択

## Observability (Logging, Metrics, Tracing)

- ロギング（log/tracing）
  - insert/remove/clear時に**debug**ログで件数やキーを記録（大量時はサンプリング）
- メトリクス
  - カウンタ: inserts_total, removes_total
  - ゲージ: symbols_len, by_name_keys, by_file_keys
  - ヒストグラム: find_by_kind_duration, remove_retain_len
- トレーシング
  - リクエストスコープでfind系にspanを付与し、フィルタ件数やヒット率をタグ化

## Risks & Unknowns

- 外部型の仕様不明
  - Symbol/Rangeの詳細（名前の型、Range.containsの境界仕様）は不明。このチャンクには現れない。
- Send/Sync境界
  - Symbol型がSend+Syncかは不明。DashMapはTがSend+Syncであることを要求するため、型仕様次第で並行使用に制約が出る可能性。このチャンクには現れない。
- インデックス整合性
  - 複数マップに跨る更新の原子性確保は難しく、アプリケーションレベルで許容するか、構造変更が必要。
- 高負荷時の性能
  - Vecベースのretainにより削除が重い。データ分布次第で顕著。
- メモリ使用量
  - 重複IDによるインデックス肥大化リスク。早期改善が望ましい。

以上により、現状の**SymbolStore**は軽量・簡潔で並行安全なメモリストアとして有用ですが、インデックス整合性と重複対策を早期に導入することで、実運用での信頼性・性能が大きく向上します。