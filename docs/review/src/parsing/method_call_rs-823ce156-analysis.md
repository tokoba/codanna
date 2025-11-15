# method_call.rs Review

## TL;DR

- 目的: **メソッド呼び出しの型認識を向上**させるため、呼び出し元・レシーバ・静的/インスタンス情報・ソース範囲を保持する**構造化表現**を提供
- 公開API: `MethodCall` と8つの関連メソッド（`new`, `with_receiver`, `static_method`, `to_simple_call`, `is_self_call`, `is_function_call`, `qualified_name`, `from_legacy_format`）
- コアロジック: レガシー文字列形式との**相互変換**（`from_legacy_format` と `to_simple_call`）。とくに `from_legacy_format` は `"self." / "::" / "@" / その他` の4分岐
- 主要な複雑箇所: レガシー形式の曖昧な分解（`contains("::")`, `contains("@")`）による**曖昧性・情報欠落**（例: 多段 `::`、空セグメント）
- 重大リスク: レガシー形式への変換で**インスタンス呼び出しのレシーバ情報が必ず失われる**（仕様上の意図だが影響が大きい）。`from_legacy_format` の分割ロジックが**過度にナイーブ**
- 安全性: **unsafeなし**、パニック要因なし。データは`String`所有でメモリ安全。並行性の懸念は*ほぼ無し*
- 改善提案: `split_once/rsplit_once` での厳密分解、`MethodCallKind`導入、`Display/From`実装、`Cow<'a>`等での**割当最小化**、**検証/バリデーション**の付与

## Overview & Purpose

このモジュールは、メソッド呼び出しを**文字列パターンではなく構造体**で表現することで、将来的な**型認識に基づく参照解決**を可能にするための基盤を提供します。現状のインデクシングは `(String, String, Range)` のレガシー形式（例: `"self.method"`, `"Type::method"`, `"receiver@method"`, `"method"`）を用いており、**インスタンス呼び出しのレシーバ情報が失われる**問題があります。本`MethodCall`はこれを解消する設計です（現状はインデクサ未統合）。

主な機能:
- 呼び出し元関数（caller）、メソッド名、レシーバ、静的呼び出しフラグ、ソース位置を保持
- レガシー形式からの**パース**と、レガシー形式への**ダウングレード**を提供
- 表示用の**修飾名**（`qualified_name`）を生成

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | MethodCall | pub | メソッド呼び出しの完全な文脈（caller/method/receiver/static/range）を保持 | Low |
| Assoc fn | MethodCall::new | pub | 構築（基本情報の設定） | Low |
| Method | MethodCall::with_receiver | pub | レシーバ設定（ビルダー） | Low |
| Method | MethodCall::static_method | pub | 静的呼び出しフラグ設定（ビルダー） | Low |
| Method | MethodCall::to_simple_call | pub | レガシー形式への変換（互換性のため） | Med |
| Method | MethodCall::is_self_call | pub | `self`呼び出しかどうか | Low |
| Method | MethodCall::is_function_call | pub | レシーバ無しの関数呼び出しかどうか | Low |
| Method | MethodCall::qualified_name | pub | 表示用の修飾名を生成 | Low |
| Assoc fn | MethodCall::from_legacy_format | pub | レガシー形式からのパース | Med |

### Dependencies & Interactions

- 内部依存
  - `from_legacy_format` → `MethodCall::new` → `.with_receiver()` → `.static_method()` の呼び出し連鎖
  - `to_simple_call`/`qualified_name` はフィールドのみ参照（内部呼出なし）
- 外部依存（表）
  - | 依存 | 用途 | 範囲 |
    |------|------|------|
    | `crate::Range` | ソース位置の表現 | 構造体フィールド、テスト |
    | `std::fmt`（派生 `Debug`） | デバッグ表示 | derive |
    | `eprintln!`（テストのみ） | デバッグ出力 | `#[cfg(test)]` |
- 被依存推定
  - 既存の**パーサ出力変換**（レガシー→構造化）
  - **インデクシングパイプライン**（将来統合時）
  - クロスリファレンス/呼び出しグラフ生成コンポーネント

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | `fn new(caller: &str, method_name: &str, range: Range) -> Self` | 構造体の基本構築 | O(|caller|+|method|) | O(|caller|+|method|) |
| with_receiver | `fn with_receiver(self, receiver: &str) -> Self` | レシーバ設定 | O(|receiver|) | O(|receiver|) |
| static_method | `fn static_method(self) -> Self` | 静的呼び出しフラグ設定 | O(1) | O(1) |
| to_simple_call | `fn to_simple_call(&self) -> (String, String, Range)` | レガシー形式への変換 | O(|caller|+|method|+|recv|) | 新規割当 |
| is_self_call | `fn is_self_call(&self) -> bool` | `self`呼び出しか判定 | O(1) | O(1) |
| is_function_call | `fn is_function_call(&self) -> bool` | レシーバ無しか判定 | O(1) | O(1) |
| qualified_name | `fn qualified_name(&self) -> String` | 表示用の修飾名生成 | O(|method|+|recv|) | 新規割当 |
| from_legacy_format | `fn from_legacy_format(caller: &str, target: &str, range: Range) -> Self` | レガシー形式からの構築 | O(|target|) | 新規割当 |

以下、各APIの詳細。

### MethodCall::new

1) 目的と責務
- 呼び出し元、メソッド名、範囲を受け取り、レシーバ未設定・静的フラグオフで初期化

2) アルゴリズム
- 引数の &str を `String` にコピー
- `receiver=None`, `is_static=false` をセット

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| caller | &str | Yes | 呼び出し元関数名 |
| method_name | &str | Yes | メソッド名 |
| range | Range | Yes | ソース位置 |

4) 戻り値
| 型 | 説明 |
|----|------|
| MethodCall | 初期化済みインスタンス |

5) 使用例
```rust
let call = MethodCall::new("main", "process_items", Range::new(1,0,1,10));
```

6) エッジケース
- `caller`/`method_name` が空文字でも許容（*非推奨：バリデーション無し*）
- `range` が無効でも検証しない（Rangeの仕様はこのチャンクでは不明）

### MethodCall::with_receiver

1) 目的と責務
- レシーバ（`"self"`, 変数名、型名等）を設定するビルダー

2) アルゴリズム
- `receiver` を `Some(receiver.to_string())` に設定し `self` を返す

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| receiver | &str | Yes | レシーバ表現 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Self | ビルダー連鎖用の自己 |

5) 使用例
```rust
let call = MethodCall::new("handler", "clone", range).with_receiver("data");
```

6) エッジケース
- 空文字レシーバ可（*非推奨*）
- 静的呼び出しかどうかの判定はここではしない

### MethodCall::static_method

1) 目的と責務
- 静的メソッド呼び出しであることを示すフラグをセット

2) アルゴリズム
- `is_static = true`

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| なし | - | - | - |

4) 戻り値
| 型 | 説明 |
|----|------|
| Self | ビルダー連鎖用の自己 |

5) 使用例
```rust
let call = MethodCall::new("main", "new", range).with_receiver("String").static_method();
```

6) エッジケース
- レシーバ未設定のまま静的化すると、`qualified_name`/`to_simple_call` では型情報が失われる（*要注意*）

### MethodCall::to_simple_call

1) 目的と責務
- 現行インデクサのレガシー形式 `(caller, target, range)` へダウングレード
- 互換性のための一時的手段

2) アルゴリズム（簡略）
- `receiver == Some("self")` → `"self.method"`
- `is_static == true && receiver.is_some()` → `"Type::method"`
- `receiver.is_none()` → `"method"`
- 上記以外（レシーバあり・非静的）→ `"method"`（レシーバ情報は失われる）

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| なし | - | - | `&self` メソッド |

4) 戻り値
| 型 | 説明 |
|----|------|
| (String, String, Range) | `(caller, target, range)` |

5) 使用例
```rust
let (caller, target, range) = MethodCall::new("save","validate",range).with_receiver("self").to_simple_call();
// caller="save", target="self.validate"
```

6) エッジケース
- 静的フラグが真でもレシーバ未設定だと `"method"` になる（型名喪失）
- インスタンス呼び出しは常に `"method"` へ縮退（意図した仕様）

### MethodCall::is_self_call

1) 目的と責務
- `receiver == "self"` の判定

2) アルゴリズム
- `self.receiver.as_deref() == Some("self")`

3) 引数/戻り値
- 引数なし、戻り値 `bool`

4) 使用例
```rust
assert!(MethodCall::new("f","g",range).with_receiver("self").is_self_call());
```

5) エッジケース
- 大文字小文字は区別。`"Self"`は別物

### MethodCall::is_function_call

1) 目的と責務
- レシーバ無し（=関数呼び出し）判定

2) アルゴリズム
- `self.receiver.is_none()`

3) 引数/戻り値
- 引数なし、戻り値 `bool`

4) 使用例
```rust
assert!(MethodCall::new("main","println",range).is_function_call());
```

5) エッジケース
- 静的フラグが真でもレシーバが無ければ `true` を返す（概念的には「関数呼び出し」扱い）

### MethodCall::qualified_name

1) 目的と責務
- 表示/デバッグ用の**修飾名**を生成
- 静的: `"Type::method"`, インスタンス: `"receiver.method"`, 関数: `"method"`

2) アルゴリズム
- `(Some(receiver), true)` → `format!("{}::{}", receiver, method)`
- `(Some(receiver), false)` → `format!("{}.{}", receiver, method)`
- `(None, _)` → `method.to_string()`

3) 引数/戻り値
- 引数なし、戻り値 `String`

4) 使用例
```rust
assert_eq!(MethodCall::new("p","push",r).with_receiver("items").qualified_name(),"items.push");
```

5) エッジケース
- 静的フラグ真でもレシーバ無し → メソッド名のみになる（型名欠落）

### MethodCall::from_legacy_format

1) 目的と責務
- レガシーの target 文字列（`"self.method"`, `"Type::method"`, `"receiver@method"`, `"method"`）を `MethodCall` に復元

2) アルゴリズム（主要分岐）
- `target.strip_prefix("self.")` → `receiver="self"` / `method=残部`
- `target.contains("::")` → `split("::")` → 2パートなら静的呼び出し、それ以外はフォールバック
- `target.contains("@")` → `split("@")` → 2パートならレシーバヒント、それ以外はフォールバック
- 上記以外 → 関数orインスタンス（レシーバ情報なし）

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| caller | &str | Yes | 呼び出し元 |
| target | &str | Yes | レガシー形式文字列 |
| range | Range | Yes | 位置 |

4) 戻り値
| 型 | 説明 |
|----|------|
| MethodCall | 構築された呼び出し |

5) 使用例
```rust
let call = MethodCall::from_legacy_format("main","HashMap::new",range);
assert!(call.is_static && call.receiver.as_deref()==Some("HashMap"));
```

6) エッジケース
- 多段 `::`（例: `"std::collections::HashMap::new"`）はフォールバックでレシーバ無し・非静的になる
- 空セグメント（例: `"Type::"`, `"@write"`, `"file@"`）で空メソッド/空レシーバを生成する可能性（現実装では2分割を満たせば受け入れる）
- `"self."` だけのケースで空メソッド名が入る

## Walkthrough & Data Flow

- 入力（現在のパーサ出力）
  - レガシー形式: `(caller, target, range)`
- 変換（構造化への取り込み）
  - `MethodCall::from_legacy_format(caller, target, range)` により、`receiver`/`is_static`/`method_name` を推定
- 利用（表示/内部処理）
  - `qualified_name()` で `"Type::method"` / `"receiver.method"` / `"method"` を統一的に表示
- 互換出力（現行インデクサへ）
  - `to_simple_call()` で `(caller, target, range)` にダウングレード
  - 注意: インスタンス呼び出しの**レシーバは失われる**

Mermaid フローチャート: `from_legacy_format` の主要分岐

```mermaid
flowchart TD
  A[Input: caller, target, range] --> B{target starts with "self."?}
  B -- Yes --> C[method = target["self.".len()..]; receiver="self"; static=false]
  B -- No --> D{target contains "::"?}
  D -- Yes --> E[parts = target.split("::"); if parts.len()==2]
  E -- True --> F[receiver=parts[0]; method=parts[1]; static=true]
  E -- False --> G[Fallback: method=target; receiver=None; static=false]
  D -- No --> H{target contains "@"?}
  H -- Yes --> I[parts = target.split("@"); if parts.len()==2]
  I -- True --> J[receiver=parts[0]; method=parts[1]; static=false]
  I -- False --> G
  H -- No --> G
  C --> K[Return MethodCall]
  F --> K
  G --> K
  J --> K
```

上記の図は `from_legacy_format` 関数（行番号: 不明。本チャンクには行番号情報が含まれません）の主要分岐を示す。

Mermaid フローチャート: `to_simple_call` の主要分岐

```mermaid
flowchart TD
  A[Input: &self] --> B{receiver is Some?}
  B -- No --> C[target = method_name]
  B -- Yes --> D{receiver == "self"?}
  D -- Yes --> E[target = "self.method"]
  D -- No --> F{is_static?}
  F -- Yes --> G[target = "Type::method"]
  F -- No --> H[target = "method"]:::warn
  E --> I[(caller, target, range)]
  G --> I
  C --> I
  H --> I

  classDef warn fill:#fff3cd,stroke:#ffec99,color:#995c00;
```

上記の図は `to_simple_call` 関数（行番号: 不明）の主要分岐と、インスタンス呼び出しが `"method"` に縮退する点（警告色）を示す。

## Complexity & Performance

- 時間計算量
  - `new`: O(|caller| + |method|)
  - `with_receiver`: O(|receiver|)
  - `static_method`: O(1)
  - `to_simple_call`: O(|caller| + |method| + |receiver|)（文字列結合・複製）
  - `qualified_name`: O(|method| + |receiver|)
  - `from_legacy_format`: O(|target|)（`strip_prefix`/`contains`/`split`）
- 空間計算量
  - いずれも結果の `String` 割当相応。`from_legacy_format` の `split(...).collect()` は一時 `Vec` 生成を伴う
- ボトルネック/スケール限界
  - 大量の変換における**文字列割当/複製**が主要コスト
  - `split(...).collect()` による**不要ベクタ割当**は避けられる（`split_once/rsplit_once` 使用で削減可能）
- 実運用負荷要因
  - I/O/ネットワーク/DB は無し。本構造の作成・整形コストのみ

## Edge Cases, Bugs, and Security

エッジケース詳細（期待動作・現実装・状態）

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空caller | caller="" | エラー/警告 or 許容 | 許容 | 要検討 |
| 空method | "self." / "Type::" / "file@" | エラー/無視/警告 | 生成され得る | 要改善 |
| 多段namespace | "std::collections::HashMap::new" | 最後の `::` で分割 | フォールバック（非静的・レシーバ無） | 要改善 |
| 複数@ | "a@b@c" | エラー or 最初/最後で分割 | フォールバック | 要改善 |
| レシーバ無し静的 | is_static=true, receiver=None | 何らかの防止/警告 | そのまま（表示・変換で型喪失） | 要改善 |
| インスタンス→レガシー | receiver=Some("x"), is_static=false | レシーバ保持 | `"method"` に縮退 | 仕様（制約） |
| 空白混入 | "  self.validate  " | トリミング | 現状トリムなし | 要改善 |
| 無効文字 | methodに'.','@','::'含有 | バリデーション | そのまま許容 | 要改善 |
| 大文字/小文字 | "Self.validate" | 大小区別維持 | 区別 | OK |
| Range不正 | 範囲の整合性崩れ | バリデーション | ノーチェック | 不明 |

セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 安全。`String`/`Option` のみ、`unsafe` なし
- インジェクション
  - SQL/Command/Path traversal: 対象外。I/O無し
- 認証・認可
  - 権限チェック/セッション: 対象外
- 秘密情報
  - ハードコード秘密/ログ漏えい: ライブラリ本体では無し。テストで `eprintln!` により任意文字列が出力されるが実害は低い
- 並行性
  - Race/Deadlock: 状態共有無し。`MethodCall` は不変/所有データのみで競合なし

Rust特有の観点（詳細）
- 所有権
  - `new`/`with_receiver` で &str → String への所有確立（ムーブ: 自己`self`のムーブチェーン、行番号: 不明）
- 借用
  - ビルダーは `self` をムーブして返すため、可変借用の衝突は無し
- ライフタイム
  - 明示ライフタイム不要（全て所有データ）
- unsafe境界
  - なし（ファイル全体）
- 並行性・非同期
  - `Send/Sync`: `String` と（仮）`Range` が `Send + Sync` であれば自動導出される（`Range`の実装は不明）
  - await/キャンセル: 非同期なし
- エラー設計
  - `Result`/`Option` を返すAPIは無し（全て**infallible**）
  - panic: `unwrap/expect` 不使用
  - エラー変換: 該当なし

## Design & Architecture Suggestions

- フォーマット分解の堅牢化
  - `split("::").collect()` → `rsplit_once("::")`（多段namespaceでも最後の`::`を評価）
  - `split("@").collect()` → `split_once("@")`（両端が空のケースを検出しやすい）
  - 空セグメントは**reject**するか、`Result` を返して通知
- APIの厳密化/拡張
  - `enum MethodCallKind { Self_, Static{type_name}, Instance{receiver}, Function }` を導入し**不変条件**を型で表現
  - `Display` 実装で `qualified_name()` 等価の表現を自然に提供
  - `From<(String, String, Range)>`/`TryFrom` でレガシー変換を型に組み込む
  - `Eq`/`Hash` 派生（重複排除/セット化用途）
- 割当最小化（ゼロコスト目標の前進）
  - `MethodCall` を `Cow<'a, str>` ベースの**借用対応型**に（必要に応じて所有化）
  - `to_simple_call` の返却に参照版（例: `(&str, String, &Range)`）を追加するか、互換レイヤでのみ所有化
- バリデーション
  - メソッド名/レシーバに対する**識別子検証**（空/無効文字を弾く）
  - `static_method` 呼び出し時に**レシーバ必須**のチェック（型名が無ければエラー/警告）
- 互換性を維持しつつ情報損失を減らす
  - レガシー`target`に新しい記法（例: `"receiver@method"` を正式採用）を段階導入
  - 変換時に警告/メトリクス（失われたレシーバ件数をカウント）

## Testing Strategy (Unit/Integration) with Examples

現状のテストカバレッジ
- レガシー→構造化→レガシーの**往復**（互換性検証）
- self/static/instance/function の**代表ケース**
- **メソッドチェーン**の基礎分解（将来拡張の準備）
- **型推論シナリオ**（将来の型統合のユースケース）

追加で望まれるテスト（サンプル）
```rust
#[test]
fn test_from_legacy_format_multiscope_static() {
    let r = Range::new(1,0,1,1);
    // 期待: 最後の "::" で分割（改善後）。現実装では fallback。
    let call = MethodCall::from_legacy_format("main", "std::collections::HashMap::new", r);
    // 改善後なら:
    // assert_eq!(call.receiver.as_deref(), Some("std::collections::HashMap"));
    // assert_eq!(call.method_name, "new");
    // assert!(call.is_static);
    // 現状の仕様確認:
    assert!(call.receiver.is_none());
    assert!(!call.is_static);
}

#[test]
fn test_invalid_segments_are_rejected_or_marked() {
    let r = Range::new(1,0,1,1);
    // 空メソッド
    let call = MethodCall::from_legacy_format("main", "Type::", r);
    assert_eq!(call.method_name, ""); // 現仕様の問題点を可視化

    let call = MethodCall::from_legacy_format("main", "self.", r);
    assert_eq!(call.method_name, ""); // 同上
}

#[test]
fn test_static_without_receiver_behavior() {
    // ユーザーが誤ってレシーバ無しで static_flag だけ立てた場合
    let r = Range::new(1,0,1,1);
    let call = MethodCall::new("main", "new", r).static_method();
    // qualified_name は型名なし。改善余地あり。
    assert_eq!(call.qualified_name(), "new");
    let (_, target, _) = call.to_simple_call();
    assert_eq!(target, "new");
}
```

統合テスト観点
- 実パーサ出力（レガシー）との**一致率**検証
- 変換過程での**情報損失メトリクス**（件数・比率）
- 大規模入力での**割当/時間**（ベンチ）確認

## Refactoring Plan & Best Practices

- ステップ1（安全な内部変更）
  - `from_legacy_format`: `split_once/rsplit_once` 採用、空セグメント検出、`rsplit_once("::")` で多段namespace対応
  - ベクタ`collect()`除去で割当削減
- ステップ2（API拡張・非破壊）
  - `MethodCallKind` 導入、`Display` 実装、`TryFrom<(caller, target, range)>` 追加
  - 検証関数（`validate_ident` 等）でフィールド整合性チェック
  - ロスが起きる `to_simple_call` に「ロス有り」フラグや警告コールバックを追加
- ステップ3（性能改善）
  - `Cow<'a, str>` ベース（または `&'a str`）のバリアント追加、必要時のみ所有化
  - `qualified_name` のフォーマットキャッシュ（必要であれば）
- ステップ4（移行）
  - インデクサのレガシー形式から `MethodCall` への切替
  - 互換レイヤの段階的廃止

Best Practices
- 不変条件を型で表す（enum + 構築ファクトリ）
- 失敗し得る処理は `Result` を返して健全性向上
- `#[must_use]` の活用は適切（現状 `to_simple_call`/`qualified_name` に付与済み）

## Observability (Logging, Metrics, Tracing)

- すでに `Debug` 派生があり、デバッグ出力しやすい
- 推奨
  - ロスト情報の**メトリクス**（例: `methodcall.receiver_lost_total`）
  - `tracing` の導入（`from_legacy_format` の分岐ごとに `trace!`）
  - バリデーション失敗時の**構造化ログ**（レベル: warn）

## Risks & Unknowns

- 未統合: インデクシングへの**本格統合が未済**（仕様変更時の影響が未知）
- `Range` 型の仕様: ライフサイクルや `Send/Sync`、等価性の前提が**不明**
- ドメイン上の曖昧性: レシーバ文字列が**「変数名」か「型名」か**を区別できない（`is_static` に依存）
- 互換性制約: レガシー形式が**インスタンスレシーバの情報を表現不能**（移行計画が鍵）
- ロケール/識別子: 非ASCII識別子の扱いとバリデーション要件は**このチャンクでは不明**

以上の評価は、このファイル（本チャンク）に基づくものであり、他ファイルの実装・仕様は「不明」または「このチャンクには現れない」です。