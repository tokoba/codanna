# fixtures\go\generics.go Review

## TL;DR

- このファイルは、Go 1.18+のジェネリクスを広範に実演するサンプルで、**関数ジェネリクス**、**型パラメータ付き構造体/インタフェース**、**複合的制約**、**型エイリアス**を含む。
- 主要APIは、**Stack[T]**、**Map[K,V]**、**List[T]**（Container[T]実装）、**Filter/Map/Reduce**、**Sum/Combine**、**Processor[T]**、**Cache[K,V]** など。
- 重大なコンパイル/設計上の問題が複数存在:
  - type Map と func Map が同名でトップレベルに共存（Goでは禁止）→コンパイル不可。
  - Map[K,V] に **Size** メソッドが無いのに **Cache.Put** が `c.data.Size()` を呼び出す→コンパイル不可。
  - **Repository[T any]** が `*List[T]` フィールドを持つが、List は T に `comparable` を要求→型制約不一致で**構造体定義自体がコンパイル不可**。
  - import `"constraints"` が不明なパス（標準ではなく `golang.org/x/exp/constraints` が一般的）。
- 並行性: すべてのコレクション（Map、List、Stack、Cache、Repository）は**ゴルーチン安全ではない**。複数ゴルーチンからの利用で**data race**が起こりうる。
- エラー処理は最小（例: Processor.ProcessAll はエラーを早期リターン）。**Max** や **ProcessSerializableNumbers** は利用側の実装（Comparable/Serializable）に依存。
- パフォーマンスは概ね **O(n)** の線形走査中心。`Keys/Values/Items/Filter/Map/Reduce/Combine` は**新規スライス割り当て**を行うため、メモリコストに注意。

## Overview & Purpose

このファイルは、Goのジェネリクス機能を実例で示すための「多機能なデモパッケージ」です。基本的なジェネリック関数（Identity、Add、Pair）、制約（Number、Comparable、Serializable）を用いたアルゴリズム（Sum、Max、ProcessSerializableNumbers）、ジェネリック構造体（Stack、Map、List、Repository、Cache）とそのメソッド、関数型ユーティリティ（Filter、Map、Reduce、Combine）、ポインタ補助（Ptr、Deref、Zero）などが含まれます。ユースケースは `ExampleUsage` に簡潔に示されています。

目的は、型パラメータ・制約・メソッドの組み合わせを広く紹介することにあり、実運用よりも概念紹介に比重があります。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Func | Identity[T any] | pub | 値をそのまま返す | Low |
| Func | Add[T constraints.Ordered] | pub | 2値の加算 | Low |
| Func | Pair[T,U any] | pub | 2値を組で返す | Low |
| Func | Max[T Comparable] | pub | Compareに基づく最大値選択 | Low |
| Interface | Comparable | pub | 比較メソッド契約 | Med |
| Constraint | Number | pub | 数値型の型合併 | Low |
| Func | Sum[T Number] | pub | スライス合計 | Low |
| Struct | Stack[T any] | pub | LIFOコレクション | Med |
| Struct | Map[K comparable, V any] | pub | キー/値マップラッパ | Med |
| Func | NewMap[K,V] | pub | Map作成 | Low |
| Interface | Container[T any] | pub | コレクション契約 | Med |
| Struct | List[T comparable] | pub | Set風の線形コレクション | Med |
| Func | NewList[T] | pub | List作成 | Low |
| Func | PrintContainer[T any] | pub | コンテナ内容を出力 | Low |
| Interface | Serializable | pub | バイト列へのシリアライズ契約 | Med |
| Interface | SerializableNumber | pub | Number＋Serializableの合成 | Med |
| Func | ProcessSerializableNumbers[T SerializableNumber] | pub | 合計後にシリアライズ | Med |
| Struct | Processor[T Processable] | pub | 全要素の処理管理 | Med |
| Interface | Processable | pub | 処理状態の契約 | Med |
| Func | Filter[T any] | pub | 条件でフィルタ | Low |
| Func | Map[T,U any] | pub | スライス変換（名前衝突） | Low |
| Func | Reduce[T,U any] | pub | 畳み込み | Low |
| Struct | Repository[T any] | pub | Map埋め込み＋キャッシュ | Med |
| Func | NewRepository[T comparable] | pub | Repository作成（制約不一致） | Med |
| Func | Combine[T any] | pub | スライス結合（可変長） | Low |
| TypeAlias | StringMap[V any] | pub | Map[string,V] エイリアス | Low |
| TypeAlias | IntSet | pub | Map[int, struct{}] エイリアス | Low |
| Func | CreateStringMap[V any] | pub | StringMap作成（値返却） | Low |
| Struct | Cache[K comparable, V Serializable] | pub | 容量制限付きマップ | Med |
| Func | NewCache[K,V] | pub | Cache作成 | Low |
| Func | Zero[T any] | pub | ゼロ値生成 | Low |
| Func | Ptr[T any] | pub | ポインタ生成 | Low |
| Func | Deref[T any] | pub | nil安全なデリファレンス | Low |
| Func | ExampleUsage | pub | 使用例出力 | Low |

### Dependencies & Interactions

- 内部依存
  - **ProcessSerializableNumbers** は **Sum** を使用。
  - **Repository** は `*Map[string,T]` を埋め込み、`*List[T]` をキャッシュに使用。`Transform` は（関数）**Map** を呼ぶ。
  - **Cache** は **Map** を内部データ構造として用い、**Keys/Delete/Set/Get** を利用。
  - **PrintContainer** は **Container[T]** の **Items/Size** を利用。
  - **Processor.ProcessAll** は **Processable** の **IsProcessed/Process** を利用。
  - **CreateStringMap** は **NewMap** を利用。
- 外部依存（表）
  | パッケージ | 用途 | 備考 |
  |------------|------|------|
  | fmt | 出力 | ライブラリ層では非推奨（ログに置換推奨） |
  | constraints | Ordered使用 | インポートパスが不明。このチャンクでは標準/expの特定不可（一般には golang.org/x/exp/constraints） |
- 被依存推定
  - 学習・検証用のテストコードから利用される前提のヘルパ群（Stack/Map/List/Filter/Reduce/Sum/Zero/Ptr/Deref）。
  - Repository/Cache/Processor は簡易ドメイン抽象の例として他モジュールから参照される可能性。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| Identity | `func Identity[T any](value T) T` | 値の恒等返し | O(1) | O(1) |
| Add | `func Add[T constraints.Ordered](a, b T) T` | 加算（順序付け型） | O(1) | O(1) |
| Pair | `func Pair[T,U any](first T, second U) (T, U)` | 2値のタプル | O(1) | O(1) |
| Max | `func Max[T Comparable](a, b T) T` | 比較で最大選択 | O(1) | O(1) |
| Sum | `func Sum[T Number](values []T) T` | スライス合計 | O(n) | O(1) |
| Stack.Push | `func (s *Stack[T]) Push(item T)` | 末尾追加 | Amortized O(1) | O(1) |
| Stack.Pop | `func (s *Stack[T]) Pop() (T, bool)` | 末尾取り出し | O(1) | O(1) |
| Stack.Peek | `func (s *Stack[T]) Peek() (T, bool)` | 末尾参照 | O(1) | O(1) |
| Stack.Size | `func (s *Stack[T]) Size() int` | 要素数 | O(1) | O(1) |
| Stack.IsEmpty | `func (s *Stack[T]) IsEmpty() bool` | 空判定 | O(1) | O(1) |
| NewMap | `func NewMap[K comparable, V any]() *Map[K, V]` | Map生成 | O(1) | O(1) |
| Map.Set | `func (m *Map[K,V]) Set(key K, value V)` | 代入 | Amortized O(1) | O(1) |
| Map.Get | `func (m *Map[K,V]) Get(key K) (V, bool)` | 取得 | Amortized O(1) | O(1) |
| Map.Delete | `func (m *Map[K,V]) Delete(key K) bool` | 削除 | Amortized O(1) | O(1) |
| Map.Keys | `func (m *Map[K,V]) Keys() []K` | キー配列生成 | O(n) | O(n) |
| Map.Values | `func (m *Map[K,V]) Values() []V` | 値配列生成 | O(n) | O(n) |
| NewList | `func NewList[T comparable]() *List[T]` | List生成 | O(1) | O(1) |
| List.Add | `func (l *List[T]) Add(item T)` | 追加 | Amortized O(1) | O(1) |
| List.Remove | `func (l *List[T]) Remove(item T) bool` | 1件削除 | O(n) | O(1) |
| List.Contains | `func (l *List[T]) Contains(item T) bool` | 存在判定 | O(n) | O(1) |
| List.Size | `func (l *List[T]) Size() int` | 要素数 | O(1) | O(1) |
| List.Items | `func (l *List[T]) Items() []T` | コピー返却 | O(n) | O(n) |
| PrintContainer | `func PrintContainer[T any](container Container[T])` | 概要出力 | O(n) | O(n) |
| ProcessSerializableNumbers | `func ProcessSerializableNumbers[T SerializableNumber](numbers []T) ([]byte, error)` | 合計→シリアライズ | O(n) | O(1) |
| Processor.ProcessAll | `func (p *Processor[T]) ProcessAll() error` | 未処理のみ処理 | O(n) | O(1) |
| Filter | `func Filter[T any](slice []T, predicate func(T) bool) []T` | 条件抽出 | O(n) | O(k) |
| Map（関数） | `func Map[T,U any](slice []T, mapper func(T) U) []U` | 変換 | O(n) | O(n) |
| Reduce | `func Reduce[T,U any](slice []T, initial U, reducer func(U,T) U) U` | 畳み込み | O(n) | O(1) |
| NewRepository | `func NewRepository[T comparable]() *Repository[T]` | Repository生成 | — | — |
| Repository.Store | `func (r *Repository[T]) Store(id string, item T)` | 保存＋キャッシュ | Amortized O(1) | O(1) |
| Repository.Transform | `func (r *Repository[T]) Transform[U any](transformer func(T) U) []U` | 値の一括変換 | O(n) | O(n) |
| Combine | `func Combine[T any](items ...[]T) []T` | 結合 | O(Σlen) | O(Σlen) |
| CreateStringMap | `func CreateStringMap[V any]() StringMap[V]` | StringMap生成 | O(1) | O(1) |
| NewCache | `func NewCache[K comparable, V Serializable](maxSize int) *Cache[K, V]` | Cache生成 | O(1) | O(1) |
| Cache.Put | `func (c *Cache[K,V]) Put(key K, value V) error` | 追加＋単純追い出し | O(1)〜O(n) | O(1) |
| Cache.Get | `func (c *Cache[K,V]) Get(key K) (V, bool)` | 取得 | Amortized O(1) | O(1) |
| Zero | `func Zero[T any]() T` | ゼロ値 | O(1) | O(1) |
| Ptr | `func Ptr[T any](value T) *T` | ポインタ作成 | O(1) | O(1) |
| Deref | `func Deref[T any](ptr *T) T` | nil安全デリファレンス | O(1) | O(1) |

注: Map（関数）と Map（型）の同名は Go の同一識別子空間における衝突であり、このままではコンパイル不可（詳細は後述）。

以下、主要APIのみ詳細化します（その他は概要表参照）。

1) Sum[T Number]
- 目的と責務
  - 数値スライスの合計を返す。型合併制約 Number によって算術演算を保証。
- アルゴリズム（ステップ）
  - total を T のゼロ値で初期化
  - スライスを走査し `total += v`
  - total を返す
- 引数
  | 名 | 型 | 必須 | 説明 |
  |----|----|------|------|
  | values | []T | はい | 合計対象スライス |
- 戻り値
  | 名 | 型 | 説明 |
  |----|----|------|
  | total | T | 合計値 |
- 使用例
  ```go
  nums := []int{1,2,3}
  got := generics.Sum(nums) // 6
  ```
- エッジケース
  - 空スライス: 合計はゼロ値（0, 0.0）
  - 大きな合計: 整数オーバーフローはGo仕様に従いラップ（例: int32）
  - 非整数/浮動混在: Numberは同一T内でのみ許可。混在は不可。

2) Stack[T]
- 目的と責務
  - 汎用的なLIFOコレクションの提供。
- アルゴリズム
  - Push: append
  - Pop: 末尾要素取得→スライス短縮
  - Peek: 末尾参照
- 引数/戻り値（Pop/Peek）
  | メソッド | 戻り値1 | 戻り値2 |
  |----------|---------|---------|
  | Pop | T（取り出し値 or ゼロ値） | bool（成功） |
  | Peek | T（参照値 or ゼロ値） | bool（成功） |
- 使用例
  ```go
  var s generics.Stack[string]
  s.Push("a"); s.Push("b")
  v, ok := s.Pop() // "b", true
  ```
- エッジケース
  - 空StackのPop/Peekはゼロ値＋falseを返す（安全）。

3) Map[K,V]（型）＋ NewMap/Set/Get/Delete/Keys/Values
- 目的と責務
  - Goの組み込みmapに型安全なジェネリックラッパと補助メソッドを提供。
- アルゴリズム
  - Set/Get/Delete は Go 組み込みの `map[K]V` を委譲。
  - Keys/Values は走査して新規スライスに詰め替え。
- 使用例
  ```go
  m := generics.NewMap[int,string]()
  m.Set(1, "one")
  v, ok := m.Get(1) // "one", true
  ks := m.Keys()    // []int{1}
  ```
- エッジケース
  - 存在しないキー: Getはゼロ値＋false。
  - 大量キー/値: Keys/Valuesは全コピーのためメモリ圧。

4) List[T comparable]（Container実装）
- 目的と責務
  - 線形格納＋equals（==）ベースの検索/削除。
- アルゴリズム
  - Remove: 最初に一致した位置のみ削除（複数同値は1件のみ）
  - Items: 内部スライスをコピーして返す（外部からの破壊防止）
- 使用例
  ```go
  l := generics.NewList[int]()
  l.Add(1); l.Add(2)
  _ = l.Remove(1)     // true
  has := l.Contains(2) // true
  ```
- エッジケース
  - 重複要素: Removeは先頭一致のみ削除。
  - 大規模リスト: Contains/RemoveはO(n)。

5) Filter/Map/Reduce（関数）
- 目的と責務
  - 関数型ユーティリティ。
- 使用例
  ```go
  evens := generics.Filter([]int{1,2,3,4}, func(n int) bool { return n%2==0 })
  // Mapは型名と衝突。名称修正後を推奨（後述）。
  doubled := generics.Map([]int{1,2}, func(n int) int { return n*2 })
  sum := generics.Reduce([]int{1,2,3}, 0, func(acc, n int) int { return acc + n })
  ```
- エッジケース
  - 空スライス: Filter/Mapは空を返し、Reduceは初期値を返す。

6) Processor[T Processable].ProcessAll
- 目的と責務
  - 未処理の要素のみ `Process()` を呼ぶ。
- アルゴリズム
  - 走査し、`!IsProcessed()` の場合に `Process()`。最初のエラーで中断。
- 使用例
  ```go
  type item struct{ done bool }
  func (i *item) Process() error { i.done=true; return nil }
  func (i *item) IsProcessed() bool { return i.done }

  p := generics.Processor[*item]{items: []*item{{}, {}}}
  _ = p.ProcessAll()
  ```
- エッジケース
  - 途中でエラー: 即時返却（短絡）。

7) Cache[K,V Serializable].Put/Get
- 目的と責務
  - 容量制限と単純追い出し（最初のキー）を備えたキャッシュ。
- アルゴリズム
  - `Size()` が上限以上なら `Keys()[0]` を削除後に Set（ただし本ファイルでは Map に Size が無くコンパイル不可）。
- 使用例
  ```go
  // V は Serializable を満たす型が必要
  c := generics.NewCache[int, MySerializable](10)
  _ = c.Put(1, MySerializable{/*...*/})
  v, ok := c.Get(1)
  ```
- エッジケース
  - 空キャッシュでPut: そのまま追加。
  - 上限到達: 最初のキーを削除（順序保証は map にない点に注意）。

8) Repository[T]
- 目的と責務
  - `Map[string,T]` の薄いラッパ＋ `List[T]` ベースのキャッシュと一括変換。
- 重大指摘
  - `Repository[T any]` に対して `cache *List[T]` はコンパイル不可（List は T に comparable が必要）。`NewRepository[T comparable]` と合わせて型定義側も `T comparable` に修正が必要。
- 使用例
  ```go
  // 修正後の前提
  r := generics.NewRepository[int]()
  r.Store("id1", 10)
  us := r.Transform(func(n int) string { return fmt.Sprint(n) })
  ```

## Walkthrough & Data Flow

- ExampleUsage の流れ
  - Stack[string] に "hello", "world" を Push →後続操作は未使用。
  - Map[int,string] を生成し、キー 1/2 に "one"/"two" を Set。
  - numbers := []int{1,2,3,4,5} を合計（Sum）→ 偶数のみ Filter → 2倍に Map（関数）→ fmt.Printf で出力。
- データフロー（主要部）
  - Sum: values（[]T）→加算→total（T）
  - Filter: slice（[]T）→ predicate(T)→ result（合格のみ []T）
  - Map（関数）: slice（[]T）→ mapper(T→U)→ result（[]U）
  - Repository.Transform: `Values()`（[]T）→ Map（関数）→ []U

注: Map（型）と Map（関数）の同名衝突により、このファイルはそのままではビルド不可。上記フローは意図された動作の説明です。

## Complexity & Performance

- 時間計算量
  - 線形走査系（Sum/Keys/Values/Items/Filter/Map/Reduce/Transform/Combine/Processor.ProcessAll）はすべて O(n)。
  - マップ操作（Set/Get/Delete）は平均的に Amortized O(1)。
  - List.Remove/Contains は O(n)。
  - Cache.Put は `Size()` 判定＋ Keys/Delete の組合せ（実際には Size 未実装）。追い出しの Keys は O(n)（大規模時ボトルネック）。
- 空間計算量
  - Keys/Values/Items/Filter/Map/Combine/Transform は新規スライスを生成するため O(n) 追加メモリ。
  - Reduce は O(1)。
- ボトルネック/スケール限界
  - 大規模 Map で `Keys()` による全コピーは高コスト。
  - List の線形削除/検索は要素数増に比例して遅くなる。
  - Repository/Cache は**並行アクセス非対応**。高頻度更新は data race を招く。

## Edge Cases, Bugs, and Security

- セキュリティチェックリスト
  - メモリ安全性: Go はポインタ安全でバッファオーバーフロー/Use-after-free は通常起こらない。`Deref` は nil をゼロ値に安全変換。
  - インジェクション: DB/OSコマンド未使用。`fmt.Printf` のみ使用。
  - 認証・認可: 該当なし。
  - 秘密情報: ハードコードされた秘密なし。ログ出力は Example/Print のみ。
  - 並行性: すべてのデータ構造は**非スレッドセーフ**。多ゴルーチンでの同時アクセスは race の危険。

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| Map 型と Map 関数の同名 | — | コンパイル成功 | Goでは同一識別子衝突 | バグ（コンパイル不可） |
| Cache.Put の Size 呼び出し | — | MapにSizeがあり上限判定可能 | MapにSize無し | バグ（コンパイル不可） |
| Repository の型制約不一致 | T=any | List[T comparable] をフィールドに持つ | Repository[T any] 定義 | バグ（コンパイル不可） |
| import "constraints" | — | 正しいパス解決 | パス短縮で不明 | バグ/不明 |
| List.Remove の重複要素 | items=[1,1,2], item=1 | 先頭一致のみ削除 | その通り | OK |
| Stack.Pop 空 | items=[] | (ゼロ値,false) | ゼロ値返却 | OK |
| Deref nil | ptr=nil | ゼロ値返却 | Zero[T]() | OK |
| Keys/Values 大サイズ | n≳10^6 | メモリ確保増 | 全コピー | 要注意 |
| Filter 空スライス | [] | [] | 空を返す | OK |
| Reduce 空スライス | [], init=0 | init返却 | その通り | OK |

根拠の行番号: このチャンクには行番号情報がないため「関数名のみ」提示。

## Design & Architecture Suggestions

- 同名衝突の解消
  - 関数 **Map** を別名（例: **MapSlice**）に変更。これに伴い **Repository.Transform** も修正。
- Map に **Size() int** を追加
  ```go
  func (m *Map[K,V]) Size() int { return len(m.data) }
  ```
  - これで **Cache.Put** の上限判定が可能に。
- Repository の型制約整合
  - `type Repository[T comparable] struct { *Map[string,T]; cache *List[T] }`
  - あるいはキャッシュを `Container[T]` 抽象にして `List[T]` 以外の実装も許容。
- import 修正
  - `import "golang.org/x/exp/constraints"` とし、**Ordered** を使用。
  - もしくは組込比較に限定するなら **constraints.Ordered** を **Number** にまとめて使う。
- Comparable の再設計
  - 実用的には **constraints.Ordered** や `cmp.Compare`（Go 1.21以降）を使う方が実装容易。
  - 独自 `Comparable` はメソッド `Compare(other Comparable)` の引数が広すぎる。`Compare(other T)` 相当の型安全性は Go のインタフェースでは表現困難。用途を限定するかジェネリック関数側で `~` の型合併を使う設計へ。
- 並行性対応
  - Map/List/Stack/Cache/Repository に対し **mutex** を導入したスレッドセーフ版を別型として提供するか、ドキュメントで「非ゴルーチン安全」である旨を明記。
- ライブラリと出力の分離
  - `fmt.Printf` をライブラリから除去し、呼出側に責務委譲。必要ならロガーをインジェクト。
- APIの整形
  - `CreateStringMap` は値返却よりポインタ返却の方が一貫性が高い（`*Map[string,V]`）。現在は `*NewMap` のデリファレンスで**コピー**が発生。

## Testing Strategy (Unit/Integration) with Examples

注: 以下のテスト例は、Refactoring Plan（MapSlice 名称変更、Map.Size 追加、Repository[T comparable] など）を適用後に動作する想定です。

- 単体テスト: Sum/Stack/List/Filter/MapSlice/Reduce/Processor/Deref/Cache
- 統合テスト: Repository と Cache の連携（Keys/Transform/Eviction）

```go
package generics_test

import (
	"testing"

	"github.com/your/module/generics"
)

func TestSum_Int(t *testing.T) {
	nums := []int{1,2,3,4}
	if got := generics.Sum(nums); got != 10 {
		t.Fatalf("Sum=%d want=10", got)
	}
}

func TestStack_PushPop(t *testing.T) {
	var s generics.Stack[int]
	s.Push(1); s.Push(2)
	v, ok := s.Pop()
	if !ok || v != 2 { t.Fatalf("Pop=%d ok=%v", v, ok) }
	v, ok = s.Pop()
	if !ok || v != 1 { t.Fatalf("Pop=%d ok=%v", v, ok) }
	_, ok = s.Pop()
	if ok { t.Fatalf("expected empty pop") }
}

func TestList_RemoveContains(t *testing.T) {
	l := generics.NewList[string]()
	l.Add("a"); l.Add("b"); l.Add("a")
	if !l.Remove("a") { t.Fatalf("remove failed") }
	if !l.Contains("a") { t.Fatalf("should still contain second 'a'") }
}

func TestFilter_MapSlice_Reduce(t *testing.T) {
	// MapSlice は関数名変更後
	ev := generics.Filter([]int{1,2,3,4}, func(n int) bool { return n%2==0 })
	if len(ev) != 2 { t.Fatalf("evens=%v", ev) }
	db := generics.MapSlice(ev, func(n int) int { return n*2 })
	if db[0] != 4 || db[1] != 8 { t.Fatalf("doubled=%v", db) }
	sum := generics.Reduce(db, 0, func(acc, n int) int { return acc+n })
	if sum != 12 { t.Fatalf("sum=%d", sum) }
}

type item struct{ done bool }
func (i *item) Process() error    { i.done = true; return nil }
func (i *item) IsProcessed() bool { return i.done }

func TestProcessor_ProcessAll(t *testing.T) {
	p := generics.Processor[*item]{items: []*item{{}, {}}}
	if err := p.ProcessAll(); err != nil { t.Fatal(err) }
	for _, it := range p.items {
		if !it.done { t.Fatalf("item not processed") }
	}
}

func TestDeref(t *testing.T) {
	var p *int
	if got := generics.Deref(p); got != 0 { t.Fatalf("nil deref=%d", got) }
	v := 42
	if got := generics.Deref(&v); got != 42 { t.Fatalf("deref=%d", got) }
}

type SNum int
func (s SNum) Serialize() ([]byte, error) { return []byte{byte(s)}, nil }
func (s *SNum) Deserialize(b []byte) error { *s = SNum(b[0]); return nil }

func TestProcessSerializableNumbers(t *testing.T) {
	// SNum は Number + Serializable を満たす必要あり
	ns := []SNum{1, 2, 3}
	b, err := generics.ProcessSerializableNumbers(ns)
	if err != nil || len(b) != 1 || b[0] != byte(6) {
		t.Fatalf("got=%v err=%v", b, err)
	}
}

func TestCache_PutGet_Evict(t *testing.T) {
	c := generics.NewCache[int, SNum](2)
	_ = c.Put(1, SNum(1))
	_ = c.Put(2, SNum(2))
	_ = c.Put(3, SNum(3)) // 追い出し発生（Size, Keys 実装後）
	if _, ok := c.Get(1); ok && c.Data.Size() == 2 {
		t.Fatalf("key 1 should be evicted")
	}
}
```

## Refactoring Plan & Best Practices

- 優先度高
  1. 関数 **Map** を **MapSlice** に改名（衝突解消）。
  2. **Map.Size() int** を追加し、**Cache.Put** の上限判定を修正。
  3. **Repository[T comparable]** に変更（型定義/メソッド/コンストラクタを一貫）。または `cache Container[T]` として抽象化。
  4. `import "golang.org/x/exp/constraints"` に修正し、**Add** の `constraints.Ordered` を正しく解決。
- 中期
  5. **Comparable** の利用場面を絞るか、**Max** を `constraints.Ordered` で代替可能なら置換。
  6. 追い出し方針の明確化（LRU等）。`Keys()` ベースの追い出しは順序非決定で非決定的動作。
- ベストプラクティス
  - ライブラリから直接の `fmt.Printf` を排し、呼出側に委譲。
  - 大規模スライス操作はプリアロケーション・再利用でアロケーション削減。
  - 明確なドキュメント: 各データ構造は**非ゴルーチン安全**であることを記載。

## Observability (Logging, Metrics, Tracing)

- 現状は `PrintContainer` と `ExampleUsage` の `fmt.Printf` のみ。ライブラリとしては出力せず、必要に応じて以下を検討:
  - ロガーインタフェースの注入（例: `type Logger interface{ Printf(...) }`）。
  - メトリクス（操作回数/失敗回数）をカウンタで記録（導入は外部で）。
  - トレースは対象外（純粋メモリ構造のため）。

## Risks & Unknowns

- 不明/未定義
  - `constraints` の正確なインポートパス（このチャンクには現れない）。
  - `Comparable` と `Serializable` を満たす具体型の設計は外部依存で不明。
- 重大リスク
  - **識別子衝突（Map）**、**Missing Size**、**型制約不一致（Repository）** により現状は**コンパイル不可**。
  - **並行性**が考慮されていないため、実運用での multi-goroutine アクセスは data race の危険。
- 仕様上の注意
  - `Cache` の追い出しは map のイテレーション順に依存し非決定的。期待動作の文書化が必要。
  - `CreateStringMap` の値返却はコピーを伴い、ポインタ返却と不整合。用途に応じて見直し推奨。