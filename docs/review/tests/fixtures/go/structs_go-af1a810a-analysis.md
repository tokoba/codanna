# fixtures\go\structs.go Review

## TL;DR

- 🎯 目的: Goの構造体宣言、埋め込み、ファクトリ関数、メソッド（値/ポインタレシーバ）、関数フィールド（Callback/Validator）を網羅的に示すサンプル。主な公開APIはファクトリ関数群（NewUser/NewProduct/NewHandler）、ユーティリティ（ProcessUsers/VerifyUser/CopyUserInfo/CreatePerson）と各構造体メソッド。
- 🔌 主要公開API: NewUser, NewProduct, NewHandler, ProcessUsers, VerifyUser, CreateDefaultUser, CopyUserInfo, CreatePerson、および User/Person/Product/Address/Handler の公開メソッド。
- 🧠 コアロジック: Handler.Execute のバリデーション→コールバック実行、User.UpdateInfo の入力検証、ProcessUsers のリスト変換。
- ⚠️ 重大リスク: Handler.Callback が nil の場合の panic 可能性、VerifyUser(nil) による nil ポインタ参照、CopyUserInfo の dest(nil) 書き込み、SetAge/SetPrice で負値許可による不整合、Email/Name のタグや検証は意味論上未実装。
- 🔄 並行性: どの構造体もスレッド安全ではない（ロックなし）。複数ゴルーチンからの同時更新でデータ競合が起き得る。
- 🔐 セキュリティ: インジェクションは直接なしだが、interface{} を許す Handler で型安全性低下。ログ・秘密情報の扱いは該当なし。
- ⏱️ 性能: 大半が O(1)。ProcessUsers は O(n) 時間/O(n) 空間。NewProduct は map/slice 初期化による一定の割り当てあり。

## Overview & Purpose

このファイルは、Goの構造体設計・メソッド・埋め込み・ファクトリパターン・関数フィールドの例をまとめたものです。公開構造体（User, Product, Signal, Person, Address, Handler）と、それらに対するメソッド群、ならびに補助的なファクトリ/ユーティリティ関数が実装されています。データ契約は JSON タグや任意のタグ（validate, db）で示されますが、タグの機能自体はこのファイル内では実装されていません（このチャンクには現れない）。

主な関心点は以下です。
- 値レシーバとポインタレシーバの使い分け
- 構造体埋め込み（Person が User を匿名フィールドとして組み込む）
- Handler のコールバックとバリデータによる柔軟な実行
- 入力検証（UpdateInfo）、簡易バリデーション（NewHandler の Validator）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | User | pub | ユーザー情報（ID/Name/Email/Created、内部: age/verified） | Med |
| Struct | Product | pub | 製品情報（価格、カテゴリ、メタデータ、寸法、アクティブ状態） | Med |
| Struct | Signal | pub | 空構造体（存在/通知用） | Low |
| Struct | Person | pub | User を埋め込んだ人物情報（氏名・住所） | Med |
| Struct | Address | pub | 住所情報（Street/City/State/ZipCode/Country） | Low |
| Struct | Handler | pub | コールバック/バリデータを持つ実行器 | Med |
| Func | NewUser | pub | User のファクトリ（現在時刻で初期化、verified=false） | Low |
| Func | NewProduct | pub | Product のファクトリ（Categories/Metadata を初期化） | Low |
| Func | NewHandler | pub | Handler のファクトリ（簡易 Validator を内蔵） | Low |
| Func | ProcessUsers | pub | User 配列を表示名の配列に変換 | Low |
| Func | VerifyUser | pub | User を検証済みに更新（ポインタ経由） | Low |
| Func | CreateDefaultUser | pub | 既定値の User を生成 | Low |
| Func | CopyUserInfo | pub | source→dest へ基本情報のコピー | Low |
| Func | CreatePerson | pub | User を埋め込んだ Person を生成 | Low |
| Method | (User) GetDisplayName | pub | 表示名生成 "Name <Email>" | Low |
| Method | (*User) SetAge | pub | 非公開 age を設定 | Low |
| Method | (*User) Verify | pub | 非公開 verified を true にする | Low |
| Method | (User) IsVerified | pub | verified の取得 | Low |
| Method | (*User) UpdateInfo | pub | Name/Email の同時更新（空チェック） | Low |
| Method | (Product) GetFullName | pub | 名称 "Name (ID)" を生成 | Low |
| Method | (*Product) SetPrice | pub | 価格を設定 | Low |
| Method | (Product) GetDimensions | pub | 幅/高/奥を返す | Low |
| Method | (Person) GetFullName | pub | 氏名 "FirstName LastName" を生成 | Low |
| Method | (Person) GetUserInfo | pub | 埋め込み User の表示名 | Low |
| Method | (Address) GetFullAddress | pub | 住所表現の整形 | Low |
| Method | (Handler) Execute | pub | Validator→Callback 実行 | Med |

### Dependencies & Interactions

- 内部依存
  - CreatePerson → NewUser を呼び出し、Person に埋め込む。
  - ProcessUsers → User.GetDisplayName を使用。
  - VerifyUser → User.Verify を使用。
  - CopyUserInfo → User のフィールドを直接コピー（age/Name/Email）。
  - Person.GetUserInfo → 埋め込み User のメソッド呼び出し。
  - Handler.Execute → Handler.Validator（任意）と Handler.Callback（必須想定）を使用。

- 外部依存

| パッケージ | 用途 |
|-----------|------|
| fmt | 文字列整形（Sprintf） |
| time | 現在時刻取得（time.Now） |

- 被依存推定
  - アプリケーションサービス層でのユーザー管理（User/Person）
  - 商品カタログや価格更新（Product）
  - 汎用イベント/メッセージ処理（Handler）
  - バッチ/レポート作成でのユーザー表示名変換（ProcessUsers）

## API Surface (Public/Exported) and Data Contracts

以下は公開APIの一覧です（行番号: 不明）。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| NewUser | func NewUser(name, email string) *User | User を現在時刻で初期化 | O(1) | O(1) |
| (User) GetDisplayName | func (u User) GetDisplayName() string | 表示名 "Name <Email>" を生成 | O(1) | O(1) |
| (*User) SetAge | func (u *User) SetAge(age int) | 年齢設定（非公開フィールド） | O(1) | O(1) |
| (*User) Verify | func (u *User) Verify() | 検証フラグを true | O(1) | O(1) |
| (User) IsVerified | func (u User) IsVerified() bool | 検証状態取得 | O(1) | O(1) |
| (*User) UpdateInfo | func (u *User) UpdateInfo(name, email string) error | 名前とメールの同時更新（空チェック） | O(1) | O(1) |
| (Product) GetFullName | func (p Product) GetFullName() string | "Name (ID)" 形式の表示名 | O(1) | O(1) |
| (*Product) SetPrice | func (p *Product) SetPrice(price float64) | 価格設定 | O(1) | O(1) |
| (Product) GetDimensions | func (p Product) GetDimensions() (float64, float64, float64) | 寸法の取得 | O(1) | O(1) |
| (Person) GetFullName | func (p Person) GetFullName() string | 氏名の整形 | O(1) | O(1) |
| (Person) GetUserInfo | func (p Person) GetUserInfo() string | 埋め込み User の表示名取得 | O(1) | O(1) |
| (Address) GetFullAddress | func (a Address) GetFullAddress() string | 住所の整形 | O(1) | O(1) |
| (Handler) Execute | func (h Handler) Execute(data interface{}) error | バリデーション→コールバック実行 | O(1) | O(1) |
| NewProduct | func NewProduct(id, name string, price float64) Product | Product の初期化（slice/map 準備） | O(1) | O(1) |
| NewHandler | func NewHandler(name string, callback func(data interface{}) error) *Handler | Handler の初期化（簡易 Validator 内蔵） | O(1) | O(1) |
| ProcessUsers | func ProcessUsers(users []User) []string | 表示名スライスへの変換 | O(n) | O(n) |
| VerifyUser | func VerifyUser(user *User) | User を検証済みに | O(1) | O(1) |
| CreateDefaultUser | func CreateDefaultUser() User | 既定 User 生成 | O(1) | O(1) |
| CopyUserInfo | func CopyUserInfo(source User, dest *User) | フィールドコピー | O(1) | O(1) |
| CreatePerson | func CreatePerson(firstName, lastName, email string) Person | User を埋め込んだ Person 生成 | O(1) | O(1) |

データ契約（構造体フィールド/タグ）
- User
  - ID int64 `json:"id" db:"user_id"`
  - Name string `json:"name" validate:"required"`
  - Email string `json:"email" validate:"email"`
  - age int `json:"-"`（非公開）
  - verified bool（非公開）
  - Created time.Time `json:"created_at"`
- Product: ID, Name, Price, Categories([]string), Metadata(map[string]interface{}), Dimensions(匿名ネスト), IsActive(*bool)
- Person: User（埋め込み）, FirstName, LastName, Address
- Address: Street, City, State, ZipCode, Country
- Handler: Name, Callback(func(interface{}) error), Validator(func(string) bool)

各APIの詳細説明（抜粋: 主要APIにフォーカス）

1) NewUser
- 目的と責務: 新規 User を現在時刻で初期化し、verified=false で返す。
- アルゴリズム
  - 引数 name/email をセット
  - Created に time.Now() を設定
  - verified=false を設定
  - ポインタで返却
- 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| name | string | ✅ | ユーザー名 |
| email | string | ✅ | メールアドレス |

- 戻り値

| 型 | 説明 |
|----|------|
| *User | 初期化済みユーザー |

- 使用例
```go
u := structs.NewUser("Alice", "alice@example.com")
fmt.Println(u.GetDisplayName()) // "Alice <alice@example.com>"
```
- エッジケース
  - name/email が空文字でも許容される（UpdateInfo と異なり検証なし）。後工程でのバリデーションが必要。

2) (*User) UpdateInfo
- 目的と責務: ユーザー名とメールを同時更新。空文字はエラー。
- アルゴリズム
  - name=="" または email=="" の場合 fmt.Errorf を返す
  - それ以外はフィールド更新
- 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| name | string | ✅ | 新しいユーザー名 |
| email | string | ✅ | 新しいメール |

- 戻り値

| 型 | 説明 |
|----|------|
| error | 不正入力時エラー |

- 使用例
```go
if err := u.UpdateInfo("Bob", "bob@example.com"); err != nil {
    // handle
}
```
- エッジケース
  - 空文字はエラーを返すが、フォーマット検証（正当なメール書式など）はない。

3) (Handler) Execute
- 目的と責務: Optional Validator を実行し、問題なければ Callback を呼ぶ。
- アルゴリズム
  - Validator が設定されている場合
    - data が string 型なら Validator(input) を評価
    - false の場合 "validation failed" エラー
  - Callback(data) を呼び出し、その結果（error）を返す
- 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| data | interface{} | ✅ | 任意の入力（string の場合のみ Validator を適用） |

- 戻り値

| 型 | 説明 |
|----|------|
| error | Callback または Validator のエラー |

- 使用例
```go
h := structs.NewHandler("echo", func(d interface{}) error {
    fmt.Println(d)
    return nil
})
if err := h.Execute("hello"); err != nil { /* ... */ }
```
- エッジケース
  - Callback が nil の場合は panic（このチャンクには防御なし）。
  - data が string 以外でも Validator はスキップされる。

4) ProcessUsers
- 目的と責務: []User の表示名スライスを返す。
- アルゴリズム
  - len(users) で names スライス確保
  - range で GetDisplayName を詰める
- 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| users | []User | ✅ | ユーザー配列 |

- 戻り値

| 型 | 説明 |
|----|------|
| []string | 表示名配列 |

- 使用例
```go
names := structs.ProcessUsers([]structs.User{*u})
```
- エッジケース
  - users が nil/空でも安全に動作（len=0 のスライスを返す）。

5) VerifyUser
- 目的と責務: User を検証済みにするユーティリティ。
- アルゴリズム: user.Verify() を呼ぶ。
- 引数/戻り値
  - 引数: user *User（nil 不可）
  - 戻り値: なし
- 使用例
```go
structs.VerifyUser(u)
```
- エッジケース
  - user が nil なら panic（このチャンクには防御なし）。

6) CreatePerson
- 目的と責務: NewUser で作成した User を埋め込んだ Person を生成。
- アルゴリズム
  - NewUser(fmt.Sprintf("%s %s", firstName, lastName), email)
  - Address.Country="Unknown" を設定
  - Person を返す
- 引数/戻り値
  - 引数: firstName/lastName/email（すべて必須）
  - 戻り値: Person（値）
- 使用例
```go
p := structs.CreatePerson("Alice", "Smith", "alice@example.com")
fmt.Println(p.GetFullName())     // "Alice Smith"
fmt.Println(p.GetUserInfo())     // "Alice Smith <alice@example.com>"
```
- エッジケース
  - email の書式検証なし。Country は "Unknown" 固定。

7) CopyUserInfo
- 目的と責務: source→dest の Name/Email/age をコピー。
- アルゴリズム: フィールド代入。
- 引数/戻り値
  - 引数: source User（値）、dest *User（nil 不可）
  - 戻り値: なし
- 使用例
```go
structs.CopyUserInfo(*u, u2)
```
- エッジケース
  - dest が nil だと panic。

8) NewProduct
- 目的: Product 初期化。Categories と Metadata を空で初期化。
- 使用例
```go
p := structs.NewProduct("P001", "Book", 19.99)
p.SetPrice(24.99)
```

9) NewHandler
- 目的: Handler 初期化。Validator は「len(input)>0」で初期化。
- 使用例
```go
h := structs.NewHandler("nonempty", cb)
_ = h.Execute("ok") // Validator OK
```

10) (Address) GetFullAddress
- 目的: 住所整形
- 使用例
```go
addr := structs.Address{Street:"1 Main", City:"NY", State:"NY", ZipCode:"10001", Country:"US"}
fmt.Println(addr.GetFullAddress()) // "1 Main, NY, NY 10001, US"
```

その他のメソッド（GetDisplayName, GetFullName, GetDimensions, etc.）は表の通りで直線的な O(1) 処理です。

## Walkthrough & Data Flow

- User の生成と更新
  - NewUser → *User を作成（Created=time.Now(), verified=false）
  - UpdateInfo → Name/Email の同時更新（空文字チェック）
  - Verify/IsVerified → 検証フラグの更新・取得
  - SetAge → 内部フィールド更新（検証なし）
- Person の作成
  - CreatePerson → NewUser を呼び、User を埋め込んだ Person を構築。Address.Country="Unknown" をデフォルト設定。
- Product
  - NewProduct → Categories([]string) と Metadata(map[string]interface{}) を空初期化。SetPrice で更新可能。GetDimensions で寸法取得。
- Handler
  - NewHandler → 簡易 Validator を設定（文字列長>0）
  - Execute → data が string なら Validator。OK なら Callback(data) 実行。
- 補助ユーティリティ
  - ProcessUsers → []User を []string に変換（各要素 GetDisplayName）
  - VerifyUser → User.Verify をラップ
  - CopyUserInfo → 値→ポインタへのフィールドコピー
  - CreateDefaultUser → 既定値で User を返す

本コードは分岐が少なく、3 以上の状態遷移や 4 以上の分岐は存在しないため Mermaid 図は作成しません（このチャンクには該当条件なし）。

## Complexity & Performance

- 時間計算量
  - 大半の API は O(1)。
  - ProcessUsers は O(n)（n=len(users) のイテレーション）。
- 空間計算量
  - 大半は O(1)。
  - ProcessUsers は O(n) のスライスを生成。
  - NewProduct は slice/map の初期割り当て（少量、O(1)）。
- ボトルネック/スケール限界
  - ProcessUsers の線形変換が唯一のスケーリング要素。巨大な users では GC 負荷と割り当て増加。
- 実運用負荷要因
  - time.Now() 呼び出しは軽微。
  - Handler.Callback の処理内容次第で CPU/IO コストが左右される（このチャンクには現れない）。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト
- メモリ安全性: Go はメモリ安全だが、nil ポインタ参照の危険あり（VerifyUser(nil), CopyUserInfo の dest=nil, Handler.Callback=nil の呼び出し）。Buffer overflow/Use-after-free/Integer overflow は直接的なコードはなし。
- インジェクション: SQL/Command/Path traversal は該当なし。fmt の使用のみ。
- 認証・認可: 該当なし（このチャンクには現れない）。
- 秘密情報: ハードコードされた秘密なし。ログ出力なし（漏えいなし）。
- 並行性: ロックなし。複数ゴルーチンからの同時書き込み（SetAge/Verify/UpdateInfo/SetPrice/CopyUserInfo 等）でデータ競合の可能性。Handler.Callback が並行操作をする場合の安全性は未定義（このチャンクには現れない）。

詳細エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| UpdateInfo 空文字 | name="", email="a@b" | エラー返却 | あり | OK |
| UpdateInfo 空文字両方 | name="", email="" | エラー返却 | あり | OK |
| NewUser 空文字許容 | name="", email="" | 作成は成功、後検証必要 | あり | 要運用判断 |
| VerifyUser nil | user=nil | panic しないよう防御が望ましい | なし | 要修正 |
| CopyUserInfo dest=nil | dest=nil | panic 防止のチェックが必要 | なし | 要修正 |
| Handler.Callback nil | Callback=nil | 実行時 panic 回避が必要 | なし | 要修正 |
| Handler.Execute 非文字列 | data=123 | Validatorスキップ、Callbackへ委譲 | あり | OK |
| Handler.Validator 厳格性 | " "（空白のみ） | 望ましくは NG | 現実装は len>0 でOK | 要改善 |
| Product.SetPrice 負値 | price=-1 | 望ましくは拒否 | 検証なし | 要改善 |
| User.SetAge 負値 | age=-5 | 望ましくは拒否 | 検証なし | 要改善 |
| ProcessUsers nilスライス | users=nil | 空の []string を返す | あり | OK |
| CreatePerson email不正 | "not-an-email" | 望ましくは拒否 | 検証なし | 要改善 |
| Address 欠損 | ZipCode="" | 整形は行うが品質低下 | 仕様通り | 要運用判断 |

## Design & Architecture Suggestions

- 入力検証の強化
  - **User.UpdateInfo** で空だけでなくメール書式チェックを追加。
  - **SetAge/SetPrice** に下限（>=0）チェック追加。
- nil セーフティ
  - **VerifyUser/CopyUserInfo/Handler.Execute** で nil チェックを実施し、明確なエラーを返す。
- 型安全性の向上
  - **Handler** の `interface{}` を用途別のジェネリック（Go1.18+）または型パラメータ/インタフェースに置換し、Validator/Callback のシグネチャを揃える。
- コンカレンシー対応
  - 共有状態を更新するメソッド（User/Product）に **同期化**（mutex など）を検討、もしくは不変オブジェクト＋ビルダー/コピーオンライトにする。
- エラー設計
  - `fmt.Errorf` の定数エラー/ラップ（%w）利用で識別可能に。Sentinel を避けて型付けエラーを返す。
- テスト容易性
  - `time.Now()` の注入（クロックインタフェース）により決定性を高める。
- データ契約の明文化
  - JSON タグと validate タグの期待動作を README/コメントに記載。
- API整合性
  - NewProduct はポインタ返却に統一するか、他のファクトリと整合を取る。

## Testing Strategy (Unit/Integration) with Examples

- 単体テストの観点
  - 正常系/異常系/境界値（空文字、負値、nil）を網羅。
  - **Handler.Execute** は Validator のあり/なし、data の型差異、Callback がエラーを返すケースをテスト。
  - **ProcessUsers** は空/n>0 のケース。
  - **CopyUserInfo/VerifyUser** は nil 防御を含めテスト。

- 例: User.UpdateInfo
```go
package structs_test

import (
	"testing"

	"github.com/your/module/structs"
)

func TestUpdateInfo(t *testing.T) {
	u := structs.NewUser("Alice", "alice@example.com")
	if err := u.UpdateInfo("Bob", "bob@example.com"); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if err := u.UpdateInfo("", "x@y"); err == nil {
		t.Fatalf("expected error on empty name")
	}
	if err := u.UpdateInfo("X", ""); err == nil {
		t.Fatalf("expected error on empty email")
	}
}
```

- 例: Handler.Execute
```go
func TestHandlerExecute(t *testing.T) {
	cbErr := func(d interface{}) error { return fmt.Errorf("fail") }
	h := structs.NewHandler("h1", cbErr)
	if err := h.Execute("ok"); err == nil {
		t.Fatalf("expected callback error")
	}
	// Validator should fail on empty string
	if err := h.Execute(""); err == nil {
		t.Fatalf("expected validation failed error")
	}
	// Non-string bypasses validator
	ok := structs.NewHandler("h2", func(d interface{}) error { return nil })
	if err := ok.Execute(123); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}
```

- 例: ProcessUsers
```go
func TestProcessUsers(t *testing.T) {
	u := structs.NewUser("Alice", "alice@example.com")
	names := structs.ProcessUsers([]structs.User{*u})
	if len(names) != 1 || names[0] == "" {
		t.Fatalf("bad names: %#v", names)
	}
	if names2 := structs.ProcessUsers(nil); len(names2) != 0 {
		t.Fatalf("expected empty slice")
	}
}
```

- 例: CopyUserInfo/VerifyUser の nil 安全（期待動作を決めてから）
```go
func TestCopyUserInfoNilDest(t *testing.T) {
	defer func() {
		if r := recover(); r == nil {
			t.Fatalf("expected panic without nil guard")
		}
	}()
	src := structs.CreateDefaultUser()
	structs.CopyUserInfo(src, nil) // current code panics
}
```

- 並行テスト（データ競合の検出）
  - `-race` オプションで、同一 User を別ゴルーチンから UpdateInfo/Verify/SetAge を並行に実行し競合検出。

```go
func TestUserConcurrentUpdate(t *testing.T) {
	u := structs.CreateDefaultUser()
	done := make(chan struct{})
	go func() { _ = u.UpdateInfo("A", "a@a"); done <- struct{}{} }()
	go func() { u.SetAge(30); done <- struct{}{} }()
	go func() { u.Verify(); done <- struct{}{} }()
	<-done; <-done; <-done
}
```

## Refactoring Plan & Best Practices

- Handler の安全化
  - **Callback を必須**にするバリデーション（NewHandler 内、nil の場合エラー返却）と Execute 内の防御。
  - **Validator のシグネチャ統一**（引数型をジェネリック/インタフェースで拘束）。
- 入力検証の一貫性
  - NewUser/CreatePerson でも Name/Email の検証を実施。UpdateInfo と同じルールに統一。
- エラーの明確化
  - `errors.New` や `fmt.Errorf("%w", err)` を用い、呼び出し側が判別可能なエラー型を定義（例: ErrEmptyName, ErrEmptyEmail, ErrInvalidPrice）。
- 可観測性/DI
  - time ソースをインジェクション可能に。ログ/メトリクス計測フックを提供。
- 不変性志向
  - 複数ゴルーチンで共有する可能性がある User/Product は**不変設計**に寄せ、更新は新インスタンス返却（関数型スタイル）を採用。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - **UpdateInfo/Execute** の失敗時に構造化ログ（理由/入力）を出力。PII（Email）は最小限またはマスク化。
- メトリクス
  - **Handler.Execute** で
    - 実行回数（counter）
    - バリデーション失敗/Callback 失敗（counter）
    - 実行時間（histogram）
- トレーシング
  - **Execute** にトレーススパンを追加し、Validator と Callback をサブスパンとして計測（このチャンクには実装なし）。

## Risks & Unknowns

- コールバックの性質（CPU/IO、再試行方針）は不明（このチャンクには現れない）。
- バリデーション基準（メール正当性、年齢範囲、価格下限）は不明。
- 住所フィールドの仕様（必須/任意、フォーマット）は不明。
- ライン番号はこのチャンクに含まれないため記載不可（行番号: 不明）。
- `validate`/`db` タグの利用先は不明（このチャンクには現れない）。