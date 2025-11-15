# interfaces.go Review

## TL;DR

- このファイルは、Goの**インターフェース宣言と具体実装の標本集**で、主要な公開APIは DataProcessor、EventHandler、Logger、Container、Reader/Writer などのインターフェースと、それらを実装する構造体群（FileProcessor、JSONProcessor、SimpleLogger、EventDispatcher、MapContainer）。
- コアロジックは、**データ処理のバリデーション＋実行（ProcessData）**、**バッファコピー（CopyData）**、**イベント配信（EventDispatcher.Handle）**、**ラッパー委譲（ProcessorWrapper）**、**シンプルロガー（SimpleLogger）**。
- 複雑箇所は少なく、主に**イベントハンドラのエラー伝播**、**CreateProcessor の型分岐**、**CopyData のEOF/エラー処理**、**Loggerのフォーマット引数**が要注意。
- 重大リスクとして、**WrapProcessor に nil を渡すとランタイムパニック**、**EventDispatcher/MapContainer は並行安全ではない**、**SimpleLogger のフォーマット文字列が未制御だとログ破損の可能性**、**CreateProcessor が未知型で nil を返すため利用側でヌル参照リスク**。
- パフォーマンスは概ね **O(n)**（n=入出力バイト数やハンドラ数）。CopyData は定常メモリで動作するが、FileProcessor.Write はバッファ増大によりヒープ使用が増加。
- セキュリティ観点（Rust的安全性の観点に相当）では、**Goはメモリ安全だがデータレースには弱い**ため、**共有マップ/ハンドラリストのロック**や**nil防御**が重要。
- 改善提案：**io.Reader/io.Writer の再利用**、**エラーを返すファクトリ（CreateProcessor）**、**EventDispatcher のハンドラ単位の解除**、**Container に generics/ロック導入**、**Logger にレベル比較/構造化ログ**。

## Overview & Purpose

このファイルは「interfaces」パッケージで、Goにおけるインターフェースの設計と実装を広範にデモする目的を持ちます。複数メソッドのインターフェース、標準ライブラリの埋め込み、関数型やチャネルを含むインターフェース、具体的な構造体による実装、インターフェースを引数/戻り値に持つ関数など、実務でよく遭遇するパターンを俯瞰できます。コアユースケースは以下の通りです。

- DataProcessor：バイト列データの検証と処理、メタデータ取得
- Reader/Writer：入出力の抽象化（CopyDataで利用）
- EventHandler：イベントのサブスクライブとディスパッチ
- Logger：レベル付きログ出力
- Container：任意型を格納するキー値ストア

このチャンクにネットワーク/I/O/DBなどの具体処理は含まれておらず、主としてインターフェースデザインの例示です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Interface | Any | pub | 空インターフェース（任意型） | Low |
| Interface | Stringer | pub | String() 文字列表現 | Low |
| Interface | Reader | pub | Read/Close（本パッケージ独自） | Low |
| Interface | Writer | pub | Write（本パッケージ独自） | Low |
| Interface | Logger | pub | 可変引数ログ出力/レベル設定 | Low |
| Interface | ReadWriteCloser | pub | Reader+Writer+io.Closer の合成 | Low |
| Interface | CustomWriter | pub | io.Writer + Flush | Low |
| Interface | DataProcessor | pub | Process/Validate/GetMetadata | Med |
| Interface | EventHandler | pub | Handle/Subscribe/Unsubscribe | Med |
| Interface | MessageBroker | pub | Send/Receive/Subscribe (チャネル) | Med |
| Interface | Container | pub | Store/Retrieve/Delete/Keys | Low |
| Struct | Event | pub | イベントの種別/データ/時刻 | Low |
| Struct | Message | pub | メッセージ本文/ヘッダ/トピック | Low |
| Struct | User | pub | ユーザ情報 + String 実装 | Low |
| Struct | FileProcessor | pub | Reader/Writer/DataProcessor 実装 | Med |
| Struct | JSONProcessor | pub | DataProcessor 実装（JSON用） | Low |
| Struct | SimpleLogger | pub | Logger 実装（標準出力） | Low |
| Struct | EventDispatcher | pub | EventHandler 実装（ハンドラ管理） | Med |
| Struct | ProcessorWrapper | pub | DataProcessor の委譲ラッパー | Low |
| Struct | MapContainer | pub | Container 実装（mapベース） | Low |
| Func | NewEventDispatcher | pub | EventDispatcher の生成 | Low |
| Func | ProcessData | pub | Validate→Process の安全な処理 | Low |
| Func | LogMessage | pub | INFO ログ便宜関数 | Low |
| Func | CopyData | pub | Reader→Writer のコピー | Med |
| Func | CreateProcessor | pub | DataProcessor ファクトリ | Low |
| Func | WrapProcessor | pub | ProcessorWrapper 生成 | Low |
| Func | NewMapContainer | pub | MapContainer 生成 | Low |
| Func | GetStringLength | pub | 型アサーションで文字長取得 | Low |
| Func | IsProcessor | pub | DataProcessor か判定 | Low |

### Dependencies & Interactions

- 内部依存
  - FileProcessor は Reader/Writer/DataProcessor を実装
  - JSONProcessor は DataProcessor を実装
  - SimpleLogger は Logger を実装
  - EventDispatcher は EventHandler を実装
  - ProcessorWrapper は DataProcessor を委譲
  - MapContainer は Container を実装
  - ProcessData は DataProcessor を使用（Validate→Process）
  - CopyData は Reader と Writer を使用
  - LogMessage は Logger を使用
  - CreateProcessor は FileProcessor/JSONProcessor を返す（未知型は nil）
- 外部依存（標準ライブラリ）
  - fmt（ログ/文字列整形）
  - io（EOF, Writer/Closer）
  - time（Event.Time）
- 被依存推定
  - データ処理機能を必要とするサービス層、イベント駆動処理、ログユーティリティ、簡易的なストレージ抽象層などが本モジュールを利用し得る

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| Any | type Any interface{} | 任意型の受け皿 | O(1) | O(1) |
| Stringer | interface { String() string } | 文字列表現取得 | O(1) | O(1) |
| Reader | interface { Read([]byte)(int,error); Close() error } | 読み取り抽象 | Read:O(n) | O(1) |
| Writer | interface { Write([]byte)(int,error) } | 書き込み抽象 | O(n) | O(1) |
| Logger | interface { Log(level, format string, args ...interface{}); SetLevel(string) } | ログ出力/レベル設定 | O(L) | O(1) |
| ReadWriteCloser | interface { Reader; Writer; io.Closer } | R/W/Close 合成 | 依存先に同じ | 依存先に同じ |
| CustomWriter | interface { io.Writer; Flush() error } | 書き込み+フラッシュ | Write:O(n) | O(1) |
| DataProcessor | interface { Process([]byte)([]byte,error); Validate(interface{}) bool; GetMetadata() map[string]interface{} } | データ処理/検証/メタ情報 | Process:O(n) | O(n) |
| EventHandler | interface { Handle(Event) error; Subscribe(string, func(Event) error); Unsubscribe(string) } | イベント配信/購読管理 | O(H) | O(1) |
| MessageBroker | interface { Send(string) error; Receive() <-chan string; Subscribe(string) <-chan Message } | メッセージ配信/購読 | 実装依存 | 実装依存 |
| Event | struct { Type string; Data interface{}; Time time.Time } | イベントデータ構造 | O(1) | O(1) |
| Message | struct { Topic, Content string; Headers map[string]string } | メッセージ構造 | O(1) | O(1) |
| User.String | func (User) String() string | ユーザの文字列表現 | O(|Name|+|Email|) | O(1) |
| FileProcessor.Read | func (*FileProcessor) Read([]byte) (int,error) | バッファ読み取り（デモ） | O(n) | O(1) |
| FileProcessor.Write | func (*FileProcessor) Write([]byte) (int,error) | バッファ追記 | O(n) | O(n)（増加） |
| FileProcessor.Close | func (*FileProcessor) Close() error | 閉じるフラグ設定 | O(1) | O(1) |
| FileProcessor.Process | func (*FileProcessor) Process([]byte) ([]byte,error) | 入力コピー返却 | O(n) | O(n) |
| FileProcessor.Validate | func (*FileProcessor) Validate(interface{}) bool | []byte 型チェック | O(1) | O(1) |
| FileProcessor.GetMetadata | func (*FileProcessor) GetMetadata() map[string]interface{} | ファイル名/サイズ/閉塞状態 | O(1) | O(1) |
| JSONProcessor.Process | func (*JSONProcessor) Process([]byte) ([]byte,error) | JSON処理（ダミー返し） | O(1) | O(1) |
| JSONProcessor.Validate | func (*JSONProcessor) Validate(interface{}) bool | JSON検証（常にtrue） | O(1) | O(1) |
| JSONProcessor.GetMetadata | func (*JSONProcessor) GetMetadata() map[string]interface{} | 設定返却 | O(1) | O(1) |
| SimpleLogger.Log | func (*SimpleLogger) Log(level, format string, args ...interface{}) | レベル一致時にPrintf | O(L) | O(1) |
| SimpleLogger.SetLevel | func (*SimpleLogger) SetLevel(string) | レベル更新 | O(1) | O(1) |
| EventDispatcher.Handle | func (*EventDispatcher) Handle(Event) error | ハンドラ順次呼び出し | O(H) | O(1) |
| EventDispatcher.Subscribe | func (*EventDispatcher) Subscribe(string, func(Event) error) | ハンドラ追加 | O(1) | O(1) |
| EventDispatcher.Unsubscribe | func (*EventDispatcher) Unsubscribe(string) | イベント種別ごと削除 | O(1) | O(1) |
| NewEventDispatcher | func NewEventDispatcher() *EventDispatcher | 生成 | O(1) | O(1) |
| ProcessData | func ProcessData(DataProcessor, []byte) ([]byte, error) | Validate→Process | O(n) | O(n) |
| LogMessage | func LogMessage(Logger, string) | INFO ログ委譲 | O(L) | O(1) |
| CopyData | func CopyData(Reader, Writer) error | 1024Bバッファでコピー | O(N) | O(1) |
| CreateProcessor | func CreateProcessor(string) DataProcessor | 型で切替 | O(1) | O(1) |
| WrapProcessor | func WrapProcessor(DataProcessor) DataProcessor | ラップを返す | O(1) | O(1) |
| ProcessorWrapper.Process | func (*ProcessorWrapper) Process([]byte) ([]byte,error) | 委譲 | 依存先準拠 | 依存先準拠 |
| ProcessorWrapper.Validate | func (*ProcessorWrapper) Validate(interface{}) bool | 委譲 | O(1) | O(1) |
| ProcessorWrapper.GetMetadata | func (*ProcessorWrapper) GetMetadata() map[string]interface{} | メタに"wrapped"追加 | O(1) | O(1) |
| GetStringLength | func GetStringLength(interface{}) int | 文字なら長さ返却 | O(1) | O(1) |
| IsProcessor | func IsProcessor(interface{}) bool | DataProcessor か判定 | O(1) | O(1) |
| Container | interface { Store(string, interface{}); Retrieve(string) (interface{}, bool); Delete(string) bool; Keys() []string } | 汎用KV | Keys:O(n) | O(n) |
| MapContainer.Store | func (*MapContainer) Store(string, interface{}) | 格納 | O(1) | O(1) |
| MapContainer.Retrieve | func (*MapContainer) Retrieve(string) (interface{}, bool) | 取得 | O(1) | O(1) |
| MapContainer.Delete | func (*MapContainer) Delete(string) bool | 削除 | O(1) | O(1) |
| MapContainer.Keys | func (*MapContainer) Keys() []string | キー列挙 | O(n) | O(n) |
| NewMapContainer | func NewMapContainer() *MapContainer | 生成 | O(1) | O(1) |

このチャンクには行番号が含まれていないため、根拠提示での行番号は不明です（関数名で特定しています）。

### 各APIの詳細説明（主要なもの）

1) ProcessData
- 目的と責務
  - 入力バイト列を DataProcessor に渡す前に Validate で型/内容チェックを行い、安全に Process する。
- アルゴリズム（ステップ分解）
  1. processor.Validate(data) が false なら error を返す。
  2. processor.Process(data) の結果を返す。
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| processor | DataProcessor | 必須 | 処理対象の実装 |
| data | []byte | 必須 | 入力データ |

- 戻り値

| 名称 | 型 | 説明 |
|------|----|------|
| result | []byte | 処理結果 |
| err | error | 検証失敗または処理失敗 |

- 使用例
```go
out, err := interfaces.ProcessData(&interfaces.FileProcessor{}, []byte("abc"))
if err != nil { /* handle */ }
fmt.Println(string(out))
```
- エッジケース
  - processor が nil（WrapProcessor参照）だと後続メソッド呼び出しでパニックの可能性
  - Validate が常に true の実装（JSONProcessor）では検証が有効でない

2) CopyData
- 目的と責務
  - Reader から Writer にデータを1024バイトチャンクでコピーする。
- アルゴリズム
  1. 1024バイトのバッファを作成。
  2. ループで src.Read(buffer)。
  3. err が非 nil かつ io.EOF でないなら即 err を返す。
  4. n==0 なら終了。
  5. dst.Write(buffer[:n]) のエラーを返す。
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| src | Reader | 必須 | 読み取り側（Close を持つ独自 Reader） |
| dst | Writer | 必須 | 書き込み側 |

- 戻り値

| 名称 | 型 | 説明 |
|------|----|------|
| err | error | 読み書き中のエラー |

- 使用例
```go
// 簡易モックで実演
type srcMock struct{ buf []byte; closed bool }
func (s *srcMock) Read(b []byte) (int, error) { n := copy(b, s.buf); s.buf = s.buf[n:]; if n==0 { return 0, io.EOF } ; return n, nil }
func (s *srcMock) Close() error { s.closed = true; return nil }

type dstMock struct{ buf []byte }
func (d *dstMock) Write(b []byte) (int, error) { d.buf = append(d.buf, b...); return len(b), nil }

s := &srcMock{buf: []byte("hello")}
d := &dstMock{}
_ = interfaces.CopyData(s, d)
fmt.Println(string(d.buf)) // "hello"
```
- エッジケース
  - src.Read が常に 0,nil を返す場合、ループが即終了
  - EOF は正常な終了扱い
  - dst.Write の部分書き込みは考慮されず、戻り値のみ確認

3) EventDispatcher.Handle
- 目的と責務
  - event.Type に登録されたハンドラを順次実行し、最初のエラーで停止して返す。
- アルゴリズム
  1. e.handlers マップから event.Type のリストを取得。
  2. 存在しなければ何もしないで nil を返す。
  3. for で各 handler(event) を呼び、err があれば即返す。
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| event | Event | 必須 | イベント |

- 戻り値

| 名称 | 型 | 説明 |
|------|----|------|
| err | error | ハンドラが返した最初のエラー |

- 使用例
```go
d := interfaces.NewEventDispatcher()
d.Subscribe("Created", func(e interfaces.Event) error { fmt.Println("ok"); return nil })
_ = d.Handle(interfaces.Event{Type:"Created"})
```
- エッジケース
  - ハンドラが0件なら何もしないで成功（nil）
  - 複数ハンドラのうち、先頭でエラーが出ると後続は実行されない
  - 並行呼び出し時はデータレースの危険（ロックなし）

4) CreateProcessor
- 目的と責務
  - 文字列で DataProcessor 実装を選択して返す。
- アルゴリズム
  - switch: "file"→FileProcessor、"json"→JSONProcessor、その他→nil。
- 引数/戻り値

| 名称 | 型 | 説明 |
|------|----|------|
| processorType | string | 型名 |
| ret | DataProcessor | 実装または nil |

- 使用例
```go
p := interfaces.CreateProcessor("file")
if p == nil { /* unknown type */ }
```
- エッジケース
  - 未知型で nil → 利用側は nil チェック必須

5) WrapProcessor / ProcessorWrapper
- 目的と責務
  - 既存の DataProcessor をラップし、委譲＋メタデータに "wrapped": true を追加。
- アルゴリズム
  - 委譲（Validate/Process）。
  - GetMetadata: 下位の map に "wrapped" を付与。
- エッジケース
  - 引数 processor が nil だと委譲呼び出しでパニック
  - メタデータへの書き込みで元の実装の map を破壊的変更する可能性

6) MapContainer
- 目的と責務
  - 任意型値のキー値ストア。
- 使用例
```go
m := interfaces.NewMapContainer()
m.Store("k", 123)
v, ok := m.Retrieve("k")
keys := m.Keys() // ["k"]
deleted := m.Delete("k")
```
- エッジケース
  - Keys は順序未規定
  - 並行アクセスはデータレース

## Walkthrough & Data Flow

- データ処理の基本フロー
  - 呼び出し元は CreateProcessor で DataProcessor を取得（"file"/"json"）。未知型なら nil（要防御）。
  - ProcessData は Validate→Process の順で安全に委譲。FileProcessor は []byte 型を要求し、入力をコピー返却。JSONProcessor はこのデモでは入力をそのまま返す。
  - WrapProcessor を使う場合、ProcessorWrapper 経由で同一の処理を委譲し、メタデータの "wrapped" フラグを付与。
- 入出力コピー
  - CopyData は Reader から 1024 バイト毎に読み、EOF/エラー処理を行いながら Writer に書き出す。定常メモリで大きなデータを扱える。
- イベント配信
  - EventDispatcher.Subscribe で eventType→[]handler を構築。
  - Handle は該当ハンドラ群を順次実行し、エラーがあればその場で返す（短絡）。Unsubscribe は eventType の登録全削除。
- ロギング
  - LogMessage は Logger.Log("INFO", message) を呼ぶ便宜関数。SimpleLogger は level が一致した場合に fmt.Printf で "[level] format\n" を出力する。

このチャンクには複雑な条件分岐や3以上のアクターが絡む高度な状態遷移は現れていないため、Mermaid図は作成していません。

## Complexity & Performance

- 時間計算量
  - CopyData: O(N)（総バイト数 N）。Read/Write 依存。
  - FileProcessor.Write/Process: O(n)（n=入力サイズ）
  - ProcessData: O(n)（Process に従属）
  - EventDispatcher.Handle: O(H)（H=ハンドラ数）
  - MapContainer.Keys: O(k)（k=キー数）
- 空間計算量
  - CopyData: O(1)（固定1024Bバッファ）
  - FileProcessor.Process: O(n)（結果コピー）
  - MapContainer.Keys: O(k)（キーのスライス作成）
- ボトルネック/スケール限界
  - FileProcessor のバッファは append で増え続けるため、大量データでヒープフットプリントが増大。
  - EventDispatcher は同期実行かつ短絡エラー伝播で、ハンドラが重いと遅延。並行処理やエラー集約は未実装。
  - SimpleLogger は標準出力（fmt.Printf）依存で I/O が遅い場合コストが高い。

## Edge Cases, Bugs, and Security

- メモリ安全性（Rust的観点の補完）
  - Go は型/境界チェックでバッファオーバーフローは避けられる一方、**nil インターフェースのメソッド呼び出しでランタイムパニック**が起こり得る（WrapProcessor, ProcessorWrapper）。
  - 大容量 append によるメモリ消費増（FileProcessor.Write）。リークではないが容量計画が必要。
- インジェクション
  - SimpleLogger.Log は呼び出し元から渡された format をそのまま fmt.Printf に渡すため、**フォーマット文字列の意図しない解釈**（例："%s" だが args が空）でログが乱れる可能性。悪用で任意コード実行は発生しないが、ログの可用性に影響。
  - SQL/Command/Path traversal はこのチャンクには現れない。
- 認証・認可
  - 機能該当なし。
- 秘密情報
  - ログ出力における機密漏えいの懸念（機密を format に渡す場合）。このチャンクには明示的なハードコード秘密はない。
- 並行性
  - EventDispatcher.handlers（map）/MapContainer.data（map）は**ロックなし**で変更されるため、**データレース**/**panic（concurrent map writes）**のリスク。
  - SimpleLogger.SetLevel は**原子的更新ではない**ため、他ゴルーチンが Log と競合するとレベル判定が揺らぐ可能性。
  - FileProcessor の Read/Write/Close は**非同期安全ではない**（closed フラグや buffer の競合）。

### エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ReaderがEOF | src.Read→(0, io.EOF) | 正常終了 | CopyData | 対応済 |
| Readerがエラー | src.Read→(0, err≠EOF) | errを返す | CopyData | 対応済 |
| Writerがエラー | dst.Write→err | errを返す | CopyData | 対応済 |
| FileProcessorがclosed | f.closed=true後 | Read/Writeはエラー | FileProcessor.Read/Write | 対応済 |
| ProcessDataの無効データ | Validate=false | "invalid data" error | ProcessData | 対応済 |
| CreateProcessor未知型 | "xml" | nil返却 | CreateProcessor | 対応済（ただし危険） |
| WrapProcessorにnil | processor=nil | 委譲時にpanic | ProcessorWrapper | 未対策 |
| JSONProcessor.Validate | 任意 | true返却 | JSONProcessor.Validate | 簡略実装（検証なし） |
| EventDispatcherに未登録 | event.Type未登録 | nil返却（何もしない） | Handle | 対応済 |
| Unsubscribeで一括解除 | eventType指定 | 全ハンドラ削除 | Unsubscribe | 対応済（細粒度解除不可） |
| MapContainer.Retrieve失敗 | 未登録キー | (nil,false) | Retrieve | 対応済 |
| GetStringLength非文字 | 123 | 0返却 | GetStringLength | 対応済 |

## Design & Architecture Suggestions

- 標準インターフェース準拠
  - 独自 Reader/Writer をやめて、可能なら io.Reader/io.Writer の再利用に統一。ReadWriteCloser も io.ReadWriteCloser を直接使う。
- エラー設計
  - CreateProcessor は nil 返却ではなく `(DataProcessor, error)` を返し、未知型を明示的エラー化。
  - FileProcessor.Read の「実装省略」部分を明確化し、EOF/部分読み取りの正確な挙動を定義。
  - Sentinel エラーや errors.Is/As 対応で呼び出し側が分岐しやすくする。
- 並行安全性
  - EventDispatcher.handlers と MapContainer.data に `sync.RWMutex` を導入。Subscribe/Unsubscribe/Handle/Store/Retrieve/Delete/Keys で適切にロック。
  - SimpleLogger の level を `atomic.Value` や `sync/atomic` で更新。
- イベントAPI
  - Unsubscribe はハンドラ単位の解除（ハンドラID/トークンを返して管理）に拡張。
  - Handle のエラー集約（すべて実行してエラーをまとめて返す）や非同期ディスパッチの追加（ワーカープール/チャネル）。
- メタデータ/コピー
  - ProcessorWrapper.GetMetadata は**防御的コピー**で返却し、下位実装の map を破壊しない。
- ジェネリクス
  - MapContainer を `Container[T any]` に置き換え、型安全性と変換コスト削減。
- ロギング
  - レベル比較は**優先度**（例：INFO<=WARN<=ERROR）。構造化ログ（key-value）や `fmt.Printf` ではなく `log/slog` 等の採用を検討。
- Context対応
  - 長時間処理（Process/Handle）に `context.Context` を受け付け、キャンセル/タイムアウトを伝播。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト
  - ProcessData（成功/検証失敗）
  - CopyData（通常/EOF/Readエラー/Writeエラー）
  - EventDispatcher（未登録/複数ハンドラ/エラー伝播）
  - WrapProcessor（nil渡しでpanicすることの確認と防御追加後の挙動）
  - MapContainer（Store/Retrieve/Delete/Keys）
  - GetStringLength/IsProcessor（型アサーション）
  - SimpleLogger（レベル一致/不一致・フォーマット文字列の影響）

- 統合テスト
  - CreateProcessor→ProcessData→WrapProcessor→GetMetadata の一連の流れ

例（抜粋）:

```go
package interfaces_test

import (
	"bytes"
	"errors"
	"io"
	"testing"

	"github.com/your/module/interfaces"
)

func TestProcessData_FileProcessor_OK(t *testing.T) {
	fp := &interfaces.FileProcessor{}
	in := []byte("abc")
	out, err := interfaces.ProcessData(fp, in)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if string(out) != "abc" {
		t.Fatalf("want abc, got %s", out)
	}
}

func TestProcessData_Invalid(t *testing.T) {
	fp := &interfaces.FileProcessor{}
	// FileProcessor.Validate は []byte 以外で false
	_, err := interfaces.ProcessData(fp, []byte(nil))
	// 注意: []byte(nil) は []byte 型なので Validate は true。無効例として interface{} に string を渡すテストはラップが必要。
	// このテストはファクトリや呼び出し側設計に依存するため、適宜調整。
	if err != nil {
		t.Logf("got error as expected: %v", err)
	}
}

type srcMock struct{ buf []byte }
func (s *srcMock) Read(b []byte) (int, error) { if len(s.buf)==0 { return 0, io.EOF } ; n := copy(b, s.buf); s.buf = s.buf[n:]; return n, nil }
func (s *srcMock) Close() error { return nil }

type dstMock struct{ buf []byte }
func (d *dstMock) Write(b []byte) (int, error) { d.buf = append(d.buf, b...); return len(b), nil }

func TestCopyData_OK(t *testing.T) {
	s := &srcMock{buf: []byte("hello world")}
	d := &dstMock{}
	if err := interfaces.CopyData(s, d); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if string(d.buf) != "hello world" {
		t.Fatalf("want hello world, got %s", string(d.buf))
	}
}

func TestEventDispatcher_Handle_ErrorShortCircuit(t *testing.T) {
	d := interfaces.NewEventDispatcher()
	calls := 0
	d.Subscribe("X", func(e interfaces.Event) error { calls++; return errors.New("fail") })
	d.Subscribe("X", func(e interfaces.Event) error { calls++; return nil })
	err := d.Handle(interfaces.Event{Type:"X"})
	if err == nil || calls != 1 {
		t.Fatalf("should stop at first error; calls=%d, err=%v", calls, err)
	}
}

func TestWrapProcessor_NilPanic(t *testing.T) {
	defer func() {
		if r := recover(); r == nil {
			t.Fatalf("expected panic on nil processor")
		}
	}()
	w := interfaces.WrapProcessor(nil) // DataProcessor が nil
	_, _ = w.Process([]byte("x"))      // ここでpanic
}

func TestMapContainer_Basic(t *testing.T) {
	m := interfaces.NewMapContainer()
	m.Store("a", 1)
	v, ok := m.Retrieve("a")
	if !ok || v.(int) != 1 {
		t.Fatalf("retrieve failed")
	}
	if !m.Delete("a") {
		t.Fatalf("delete failed")
	}
	if _, ok := m.Retrieve("a"); ok {
		t.Fatalf("still exists after delete")
	}
	keys := m.Keys()
	if len(keys) != 0 {
		t.Fatalf("keys should be empty; got %v", keys)
	}
}
```

テストにおけるログ検証は、SimpleLogger が fmt.Printf を直接呼んでいるため標準出力のキャプチャが必要（例：`os.Stdout` を一時的に差し替え）。ここでは省略。

## Refactoring Plan & Best Practices

- API破壊を避けた段階的改善
  1. CreateProcessor を `(DataProcessor, error)` に変更。既存呼び出し側に nil チェック→エラー処理へ移行ガイドを提供。
  2. ProcessorWrapper に nil ガード（nil の場合は no-op 実装を内部で採用するか、コンストラクタでエラー）。
  3. EventDispatcher/MapContainer に `sync.RWMutex` を導入。並行ユースケースの追加テストを作成。
  4. Logger を構造化＋レベル優先度対応に差し替え（SimpleLogger は Deprecated 化）。
  5. Reader/Writer の独自定義を廃止し、io.Reader/io.Writer 使用箇所へ移行。CopyData のシグネチャを `func CopyData(src io.Reader, dst io.Writer) error` に変更。
  6. ProcessorWrapper.GetMetadata は map コピーを返す。
  7. Container をジェネリクス化（`type Container[T any] interface { ... }`）して型安全化。

- ベストプラクティス
  - エラーは文脈付き（`fmt.Errorf("...: %w", err)`）でラップ。
  - 返す map/slice は防御的コピー（外部からの変更で内部状態が破壊されないように）。
  - 可変引数ログは format と args の妥当性を事前に検証するか、`fmt.Println` ベースの安全な経路に限定。

## Observability (Logging, Metrics, Tracing)

- ログ
  - SimpleLogger の出力は標準出力直書き。運用では**構造化ログ（key-value）**、**タイムスタンプ**、**呼び出しコンテキスト**（request ID 等）付与を推奨。
  - レベルの等価一致ではなく優先度によるフィルタリング（INFO<=WARN<=ERROR）。
- メトリクス
  - EventDispatcher.Handle の**ハンドラ呼び出し数**、**失敗数**、**処理時間**を計測（prometheus 等）。
  - CopyData の**総バイト数**、**IO エラー回数**。
- トレーシング
  - DataProcessor.Process にトレーススパン。WrapProcessor で「wrapped」属性をタグに反映。
  - イベント処理のスパンを event.Type ごとに発行。

## Risks & Unknowns

- 不明/該当なし
  - 行番号指定はこのチャンクには現れないため不明。
  - 実際の I/O 実装（FileProcessor.Read の詳細）は省略されている。
  - MessageBroker はインターフェース定義のみで実装なし。チャネルのライフサイクル/バッファリング/エラー伝播は不明。
- 既知のリスク
  - nil DataProcessor の委譲呼び出しによるパニック（WrapProcessor）。
  - Map とハンドラリストの並行非安全性。
  - CreateProcessor の未知型で nil を返して呼び出し側が誤用する可能性。
  - SimpleLogger の format 依存でログ破損（可読性低下）。

以上に基づき、公開APIとコアロジックは明快ですが、実運用を想定した安全性（エラー設計/並行性/観測可能性）は拡張の余地があります。