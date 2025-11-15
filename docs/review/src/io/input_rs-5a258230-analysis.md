# io\input.rs Review

## TL;DR

- 📦 このファイルは**JSON-RPC 2.0**のメッセージを表現する**データコントラクト（構造体）**を定義するのが目的。公開APIは3つの構造体（JsonRpcRequest/JsonRpcResponse/JsonRpcError）とエラーコードモジュール。
- 🔧 現時点で**関数やI/O処理は未実装**。コメントで将来の標準入力からのパースやIDE統合の方針が示されるが、このチャンクには実装がない。
- 🛡️ **Rust安全性**は高い（unsafeなし）。ただし**バリデーション**（jsonrpc=="2.0"確認、result/errorの相互排他）や**型安全なID/params**が未整備で、将来の利用時に不正データ受け入れリスクあり。
- ⚠️ **主要なリスク**: 大きなJSONによるメモリ圧迫、未検証入力の受け入れ、resultとerrorの同時設定の仕様違反、ID型の曖昧さ（数値/文字列/NULL）による処理の不一致。
- 🚀 **性能と複雑度**は低いが、(de)serializeは**O(n)**で入力サイズに比例。将来のストリーミングI/Oや非同期処理に発展させるなら設計配慮が必要。
- 🧪 優先すべきテスト: 仕様準拠の(逆)シリアライズ、**jsonrpc=="2.0"**の検証、**result/errorの相互排他**、各ID型の扱い、paramsの可否。

## Overview & Purpose

このファイルは、IDEや他ツールとの連携に向けた**JSON-RPC 2.0**ベースの入力取り扱いを**将来的に**実装するための土台として、**リクエスト/レスポンス/エラー**のデータ構造をRustで定義しています。現時点では**I/O処理やパーサ関数は存在せず**、コメントに将来像が記されています。これにより、serde/serde_jsonとの連携で**型安全にJSONメッセージを取り回す**ことが可能になります。

このチャンクには行番号情報が含まれていないため、正確な行番号の併記は不明です。構造体定義やモジュールはファイル全体（約76LOC）にわたり連続して記述されています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | JsonRpcRequest | pub | JSON-RPC 2.0のリクエストメッセージの表現（jsonrpc/version, method, params, id） | Low |
| Struct | JsonRpcResponse | pub | JSON-RPC 2.0のレスポンスメッセージの表現（jsonrpc/version, resultまたはerror, id） | Low |
| Struct | JsonRpcError | pub | JSON-RPC 2.0のエラーオブジェクト（code, message, data） | Low |
| Module | error_codes | pub | 標準のエラーコード定数（PARSE_ERROR等） | Low |

- フィールドのオプション性はserdeの属性で制御され、**Noneの場合はシリアライズ時に省略**されます（`#[serde(skip_serializing_if = "Option::is_none")]`）。
- 依存対象は**serde**（Serialize/Deserialize）と**serde_json::Value**（任意JSON値表現）です。

### Dependencies & Interactions

- 内部依存
  - JsonRpcResponseが**JsonRpcError**を含む（errorフィールド）。これ以外に構造体同士の呼び出しはない。
  - いずれの構造体も**serde_json::Value**を使用可能なフィールドとして持つ（柔軟だが動的・非型付け）。
- 外部依存（クレート/モジュール）

| 依存 | 用途 | 影響 |
|-----|------|------|
| serde::{Serialize, Deserialize} | データ構造の(逆)シリアライズ | JSON入出力の型安全性・互換性 |
| serde_json::Value | パラメータやIDの動的JSON値 | 柔軟だが型安全性が弱く、ランタイム検証が必要 |

- 被依存推定
  - 将来の**stdinパーサ**、**IDE/LSP統合**レイヤ、**メソッドディスパッチャ**、**レスポンス生成ユーティリティ**から利用される見込み。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| JsonRpcRequest | struct JsonRpcRequest { jsonrpc: String, method: String, params: Option<Value>, id: Option<Value> } | JSON-RPC 2.0のリクエストを表現 | O(n) (JSONの(逆)シリアライズ) | O(n) (メッセージサイズに比例) |
| JsonRpcResponse | struct JsonRpcResponse { jsonrpc: String, result: Option<Value>, error: Option<JsonRpcError>, id: Option<Value> } | JSON-RPC 2.0のレスポンスを表現 | O(n) (JSONの(逆)シリアライズ) | O(n) |
| JsonRpcError | struct JsonRpcError { code: i32, message: String, data: Option<Value> } | エラー詳細の表現 | O(n) (JSONの(逆)シリアライズ) | O(n) |
| error_codes | mod error_codes { const ...: i32 } | 標準エラーコードの定義 | O(1) | O(1) |

以下、各「API」（データコントラクト）の詳細。

### JsonRpcRequest

1) 目的と責務
- **目的**: JSON-RPC 2.0仕様のリクエストメッセージを表現する。
- **責務**: バージョン文字列（"2.0"）、メソッド名、任意のパラメータ、任意のIDの保持。*仕様上は"jsonrpc"が必須で"2.0"固定、"method"必須、"params"/"id"は任意。*

2) アルゴリズム（ステップ）
- データ構造のみで**アルゴリズムの実装は無し**。バリデーションやI/Oは未実装。将来的には「jsonrpc==”2.0”検証」「paramsの型検証」「idの正規化」などを追加することが望ましい。現状は*該当なし*。

3) 引数（構造体フィールド）

| フィールド | 型 | 必須 | 説明 |
|-----------|----|------|------|
| jsonrpc | String | 必須 | プロトコルバージョン。仕様では"2.0"固定。 |
| method | String | 必須 | 呼び出すメソッド名。 |
| params | Option<serde_json::Value> | 任意 | メソッド引数。配列・オブジェクトが一般的。 |
| id | Option<serde_json::Value> | 任意 | リクエストID（数値・文字列・NULLなど許容）。 |

4) 戻り値
- *該当なし*（構造体なので戻り値の概念はなし）。

5) 使用例

```rust
use serde_json::json;
use io::input::JsonRpcRequest;

let req_json = r#"{
  "jsonrpc": "2.0",
  "method": "textDocument/references",
  "params": { "uri": "file:///main.rs", "position": { "line": 12, "character": 5 } },
  "id": 1
}"#;

let req: JsonRpcRequest = serde_json::from_str(req_json).expect("valid request");
// バリデーション（将来追加が望ましい）
// assert_eq!(req.jsonrpc, "2.0");
```

6) エッジケース
- jsonrpcが欠落または"2.0"以外。
- methodが空文字列。
- paramsが期待形式（配列/オブジェクト）でない。
- idが数値/文字列/NULL以外の複雑な値。

### JsonRpcResponse

1) 目的と責務
- **目的**: JSON-RPC 2.0仕様のレスポンスメッセージを表現。
- **責務**: 成功時はresult、失敗時はerrorを設定し、**相互排他**を守る。idは対応するリクエストのID。

2) アルゴリズム（ステップ）
- データ構造のみで**相互排他チェックのロジックは未実装**。将来は「resultとerrorの同時設定禁止」「idのコピー/正規化」を行うヘルパーが望ましい。現状は*該当なし*。

3) 引数（構造体フィールド）

| フィールド | 型 | 必須 | 説明 |
|-----------|----|------|------|
| jsonrpc | String | 必須 | "2.0"固定。 |
| result | Option<serde_json::Value> | 条件付き | 成功結果。errorがNoneの時にSome。 |
| error | Option<JsonRpcError> | 条件付き | エラー詳細。resultがNoneの時にSome。 |
| id | Option<serde_json::Value> | 任意 | 対応するリクエストのID。通知の場合はNone。 |

4) 戻り値
- *該当なし*。

5) 使用例

```rust
use serde_json::json;
use io::input::{JsonRpcResponse, JsonRpcError};

let ok = JsonRpcResponse {
    jsonrpc: "2.0".to_string(),
    result: Some(json!({"symbols": []})),
    error: None,
    id: Some(json!(1)),
};

let err = JsonRpcResponse {
    jsonrpc: "2.0".to_string(),
    result: None,
    error: Some(JsonRpcError { code: -32601, message: "Method not found".to_string(), data: None }),
    id: Some(json!(1)),
};

let ok_json = serde_json::to_string(&ok).unwrap();
let err_json = serde_json::to_string(&err).unwrap();
```

6) エッジケース
- resultとerrorが同時にSome（仕様違反）。
- resultもerrorもNone（仕様違反）。
- id不一致や欠落（通知と通常応答の差異）。

### JsonRpcError

1) 目的と責務
- **目的**: 失敗時のエラーメタデータ（code, message, data）を表現。
- **責務**: 標準コード（error_codes）に沿った整合性のあるエラーを提供。

2) アルゴリズム（ステップ）
- データ構造のみで**検証ロジックは未実装**。将来はコード範囲検証、メッセージのフォーマット統一など。現状は*該当なし*。

3) 引数（構造体フィールド）

| フィールド | 型 | 必須 | 説明 |
|-----------|----|------|------|
| code | i32 | 必須 | エラーコード（標準・拡張）。 |
| message | String | 必須 | 人間可読なエラーメッセージ。 |
| data | Option<serde_json::Value> | 任意 | 追加のエラーコンテキスト。 |

4) 戻り値
- *該当なし*。

5) 使用例

```rust
use io::input::{JsonRpcError, JsonRpcResponse};
use serde_json::json;

let e = JsonRpcError {
    code: -32602,
    message: "Invalid params".to_string(),
    data: Some(json!({"expected": ["uri", "position"]})),
};

let resp = JsonRpcResponse {
    jsonrpc: "2.0".to_string(),
    result: None,
    error: Some(e),
    id: Some(json!("req-123")),
};
```

6) エッジケース
- 標準コード以外を使う場合の取り扱い。
- messageが空のときの可読性。
- dataに大きなペイロードを含む場合のコスト。

### error_codes

- 標準JSON-RPCエラーコードの定数群（PARSE_ERROR, INVALID_REQUEST, METHOD_NOT_FOUND, INVALID_PARAMS, INTERNAL_ERROR）。
- 使用例

```rust
use io::input::{JsonRpcResponse, JsonRpcError};
use io::input::error_codes;

let resp = JsonRpcResponse {
    jsonrpc: "2.0".to_string(),
    result: None,
    error: Some(JsonRpcError {
        code: error_codes::METHOD_NOT_FOUND,
        message: "Method not found".into(),
        data: None,
    }),
    id: Some(serde_json::json!(1)),
};
```

## Walkthrough & Data Flow

- 入力: 外部ツールやIDEから送られる**JSON文字列**（将来はstdinなど）。
- デシリアライズ: `serde_json::from_str::<JsonRpcRequest>(...)`で**JsonRpcRequest**へ変換。
- ディスパッチ: `request.method`に基づきハンドラ決定（本チャンクでは未実装、*コメントで将来案が示唆*）。
- レスポンス生成: 成功なら`result: Some(...)`、失敗なら`error: Some(...)`の**JsonRpcResponse**を構築。`id`はリクエストから引き継ぐ。
- シリアライズ: `serde_json::to_string(&response)`でJSON文字列へ。
- 出力: stdoutへ書き出し（本チャンクでは未実装）。

参考となる将来案コメントの例（このチャンクに存在、ただし擬似コードであり未実装）:

```rust
// 例（将来の使用例）:
// let request = JsonRpcRequest::from_stdin()?; // 未実装
// let response = match request.method.as_str() {
//     "textDocument/symbols" => handle_symbols(request.params),      // 未実装
//     "textDocument/references" => handle_references(request.params),// 未実装
//     _ => JsonRpcResponse::method_not_found(request.id),            // 未実装
// };
// response.write_to_stdout()?; // 未実装
```

上記の図は該当する関数が本チャンクに存在しないため作成しません（Mermaid使用基準未満）。

## Complexity & Performance

- 時間計算量
  - (逆)シリアライズ: **O(n)**（nはJSON文字列長）。フィールドアクセスは**O(1)**。
- 空間計算量
  - メッセージサイズに比例する**O(n)**。`serde_json::Value`は動的データのため入力に依存。
- ボトルネック
  - 大規模`params`/`data`によるメモリ使用量増加。
  - 同期I/O（将来実装）ではブロッキングの可能性。非同期化やストリーミングパースの検討が有効。
- スケール限界
  - 巨大リクエスト/レスポンスの取り扱いではGCがないためヒープ断片化やアロケーション負荷が増える可能性。*バイナリペイロードや外部参照などの設計で緩和可*。
- 実運用負荷要因
  - 高頻度のJSONメッセージで(逆)シリアライズコストが累積。
  - IDE連携では低レイテンシ要求。**メソッドディスパッチ**のオーバーヘッド最小化が重要。

## Edge Cases, Bugs, and Security

セキュリティチェックリストの観点で評価。

- メモリ安全性
  - unsafeは**未使用**。所有権・借用の問題も少ない（全フィールドが所有型）。
  - 大きい`serde_json::Value`の受け入れにより**メモリ圧迫**の可能性。
- インジェクション
  - SQL/コマンド/パスは**このチャンクには現れない**。ただし`params`が後段の処理に渡される際、**未検証**だとインジェクションベクトルになり得る（将来のハンドラ設計で防御必須）。
- 認証・認可
  - **該当なし**。このチャンクはI/Oレイヤ未実装で認証フローを持たない。
- 秘密情報
  - ハードコードされた秘密情報は**なし**。ただし将来のログ出力で`params`/`data`を全量記録すると**秘密情報漏洩リスク**あり。
- 並行性
  - **並行処理は未実装**。共有状態もないため現状のレースやデッドロックは**該当なし**。
  - serde_json::ValueのSend/Syncについては実装詳細に依存するため本チャンクのみでは**不明**。並行処理導入時には型境界の検証が必要。

詳細なエッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| jsonrpc欠落 | `{ "method": "m" }` | Err(INVALID_REQUEST) | このチャンクには現れない | 未対応 |
| jsonrpc!="2.0" | `{ "jsonrpc": "1.0", "method": "m" }` | Err(INVALID_REQUEST) | このチャンクには現れない | 未対応 |
| method空文字 | `{ "jsonrpc": "2.0", "method": "" }` | Err(INVALID_REQUEST) | このチャンクには現れない | 未対応 |
| params型不正 | `"params": 123` | Err(INVALID_PARAMS) | このチャンクには現れない | 未対応 |
| id未設定（通知） | `id`なし | レスポンス不要 | このチャンクには現れない | 未対応 |
| resultとerror同時設定 | `result: {...}, error: {...}` | Err(INTERNAL_ERROR)/拒否 | データ構造のみ | 未検証 |
| result/error共にNone | どちらもNone | Err(INTERNAL_ERROR)/拒否 | データ構造のみ | 未検証 |
| 大規模data/params | 数MB〜 | メモリ制限/拒否 | このチャンクには現れない | 未対応 |
| id型のばらつき | 数値/文字列/NULL/配列 | 一貫した取り扱い | このチャンクには現れない | 未対応 |
| 不正UTF-8 | バイト列混入 | デコードエラー | serdeに依存 | 要テスト |

## Design & Architecture Suggestions

- バリデーションAPIを追加
  - JsonRpcRequestに`validate()`（jsonrpc=="2.0"、method非空、params形状チェック）。
  - JsonRpcResponseに`validate()`（result XOR errorの相互排他、jsonrpc=="2.0"、idの整合）。
- ヘルパービルダーの導入
  - `JsonRpcResponse::ok(id, result)`, `JsonRpcResponse::err(id, code, message, data)`で相互排他を強制。
- ID型の正規化
  - `enum Id { Num(i64), Str(String), Null }`を導入し**型安全化**。`serde`でカスタム(逆)シリアライズ。
- 型安全なparams
  - メソッドごとに`T: DeserializeOwned`を受け取る仕組みを用意し、`params: Option<Value>`から`Option<T>`への変換ユーティリティ。
- エラーコードの拡充
  - `error_codes`にアプリケーション固有コードの名前空間を追加（例: `APP_INTERNAL`, `NOT_SUPPORTED`）。
- I/O層の分離
  - `io::reader`（stdin/pipe）と`io::writer`（stdout）を分離し、**非同期**（tokio）版も提供。
- 仕様準拠の保証
  - `serde`のカスタムデシリアライザで**バージョン固定**と**相互排他**を強制。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト
  - 正常系: リクエスト/レスポンスの(逆)シリアライズのラウンドトリップ。
  - 異常系: `jsonrpc!="2.0"`, `method空`, `result&error同時`の拒否（validate導入後）。
  - ID型: 数値/文字列/NULLをテスト。
  - 大規模payload: サイズ制限（設計時に導入）テスト。

- プロパティテスト
  - `proptest`でランダムなJSON構造に対する(逆)シリアライズの健全性、UTF-8境界、フィールド省略時の挙動。

- 統合テスト（将来）
  - stdinからの入力→ディスパッチ→stdoutへのレスポンスまでのエンドツーエンド。非同期版も含める。

使用例テストコード（ユニット・正常系）:

```rust
#[test]
fn request_roundtrip() {
    use serde_json::json;
    use crate::io::input::JsonRpcRequest;

    let original = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "textDocument/references".to_string(),
        params: Some(json!({"uri": "file:///main.rs"})),
        id: Some(json!(1)),
    };

    let s = serde_json::to_string(&original).unwrap();
    let parsed: JsonRpcRequest = serde_json::from_str(&s).unwrap();

    assert_eq!(parsed.jsonrpc, "2.0");
    assert_eq!(parsed.method, "textDocument/references");
    assert_eq!(parsed.id, Some(json!(1)));
}

#[test]
fn response_ok_err_mutual_exclusive() {
    use serde_json::json;
    use crate::io::input::{JsonRpcResponse, JsonRpcError};

    // OK
    let ok = JsonRpcResponse {
        jsonrpc: "2.0".into(),
        result: Some(json!({"ok": true})),
        error: None,
        id: Some(json!("abc")),
    };
    let ok_s = serde_json::to_string(&ok).unwrap();
    let ok_parsed: JsonRpcResponse = serde_json::from_str(&ok_s).unwrap();
    assert!(ok_parsed.error.is_none());
    assert!(ok_parsed.result.is_some());

    // Err
    let err = JsonRpcResponse {
        jsonrpc: "2.0".into(),
        result: None,
        error: Some(JsonRpcError { code: -32601, message: "Method not found".into(), data: None }),
        id: Some(json!("abc")),
    };
    let err_s = serde_json::to_string(&err).unwrap();
    let err_parsed: JsonRpcResponse = serde_json::from_str(&err_s).unwrap();
    assert!(err_parsed.result.is_none());
    assert!(err_parsed.error.is_some());
}
```

## Refactoring Plan & Best Practices

- コントラクトの明示化
  - `Id`専用enumと、メソッドごとの`Params<T>`（ジェネリック）で**型安全**に。
- バリデーションの組み込み
  - `TryFrom<Value> for JsonRpcRequest`で**構築時検証**、`validate()`でランタイム検証。
- 相互排他の強制
  - `ResponsePayload`を`enum { Result(Value), Error(JsonRpcError) }`にし、構造上同時設定不能にする。
- ビルダーパターン
  - `JsonRpcResponseBuilder::new(id).ok(result)` / `.err(code, message, data)`で組み立て。
- エラーの型設計
  - `thiserror`でドメインエラー型を定義し、`JsonRpcError`への`From`を提供。
- ドキュメンテーション強化
  - フィールドごとの仕様準拠（必須/任意、型の許容）の注記をRustdocに追加。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 受信/送信メッセージの**メタ情報のみ**（method, id, サイズ）をINFOログ。本文はPII/秘密情報回避のため省略かマスク。
  - 失敗時はWARN/ERRORで`code`と`message`を記録。
- メトリクス
  - メソッド別リクエスト数、エラー率、ペイロードサイズ分布、(逆)シリアライズ時間ヒストグラム。
- トレーシング
  - `method`と`id`をspanに含め、ディスパッチ/ハンドラ/レスポンス生成を紐付け。*非同期I/O導入時に有効*。

## Risks & Unknowns

- 仕様準拠の度合い
  - バージョン固定や相互排他が**コード上未強制**。現状は**不明**で、利用側の責任に委ねられる。
- IDとparamsの型取り扱い
  - `serde_json::Value`に依存し**曖昧**。システム全体での一貫性が**不明**。
- 並行性境界
  - 型のSend/Sync、非同期I/O設計の方針、キャンセル対応などが**このチャンクには現れない**。
- 将来のI/O実装
  - `from_stdin`や`write_to_stdout`は**未実装**。実装方法（同期/非同期、バッファリング、ストリーミング）の選択が**不明**。
- エラーハンドリングポリシー
  - どのエラーをユーザーへ返し、どれを内部エラーとするかのポリシーが**未定**。

以上の通り、このファイルは**JSON-RPC 2.0のデータ定義の核**を提供していますが、**バリデーション・I/O・ディスパッチのロジックは未実装**です。次のステップとして、仕様準拠を強制する**型設計と検証ロジック**、そして**堅牢なI/O層**の追加を推奨します。