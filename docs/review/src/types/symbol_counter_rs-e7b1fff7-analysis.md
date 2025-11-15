# types\symbol_counter.rs Review

## TL;DR

- **目的**: 型安全なユニークなシンボルID（1始まり、0禁止）を生成する**カウンタ**を提供
- **公開API**: `SymbolCounter::new`, `next_id`, `current_count`, `reset`, `from_value`, `Default` 実装（いずれも O(1)）
- **データ契約**: 返すIDは常に非ゼロかつ単調増加（1,2,3, …）。内部表現は**NonZeroU32**でゼロ不許可を型で保証
- **コアロジック**: `next_id` は現在値を返し、その後 `checked_add(1)` で次回値へ更新（オーバーフロー時はパニック）
- **複雑箇所**: オーバーフロー時のパニック設計と `from_value` の「次に返す値」を直接設定する仕様
- **重要リスク**: 
  - 4,294,967,295 を超える発行で**パニック**（実質非現実的だが理論上）
  - `from_value(0)` は**パニック**
  - `Clone` により別インスタンスが同じ系列を生成可能（ユニーク性はインスタンス内のみ）
  - スレッドセーフではない設計（単一スレッド前提）

## Overview & Purpose

このモジュールは、プロジェクトの型安全方針に沿ってプリミティブな整数の乱用を避け、ユニークなシンボルIDを安全に発行する**型安全なカウンタ**（`SymbolCounter`）を提供します。主な特徴は以下です。

- ID は常に1以上（0は不使用）で、**NonZeroU32**により型レベルで不変条件を保証
- ID は**単調増加**で生成
- 実装は**単一スレッド**での使用を前提（パーサはファイル毎に単一スレッド）
- 返却型は `super::SymbolId`（このチャンクには定義なし。テストからタプル構造体で `.0` が `u32` と推測可能）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | SymbolCounter | pub | 非ゼロの連番IDを生成・管理 | Low |
| Field | next_id: NonZeroU32 | private | 次に返すID（1始まり）を保持 | Low |
| Method | new() -> Self | pub | カウンタを1から開始 | Low |
| Method | next_id(&mut self) -> super::SymbolId | pub | 現在のIDを返し、内部状態を+1 | Low |
| Method | current_count(&self) -> u32 | pub | 生成済みID個数を返す（next_id - 1） | Low |
| Method | reset(&mut self) | pub | カウンタを1に戻す | Low |
| Method | from_value(u32) -> Self | pub | 次に返す値を明示したカウンタを生成（0はパニック） | Low |
| Trait Impl | Default | public impl | `new()` と等価 | Low |
| Tests | mod tests | private | 基本動作のユニットテスト | Low |

### Dependencies & Interactions

- 内部依存
  - `Default::default` -> `SymbolCounter::new`
  - `SymbolCounter::next_id` は `self.next_id` を読み取り、`checked_add(1)` で更新
  - `current_count` は `self.next_id.get() - 1` を計算
  - `reset` は `self.next_id = NonZeroU32::new(1).unwrap()` に戻す
- 外部依存

| 依存 | 用途 | 備考 |
|------|------|------|
| std::num::NonZeroU32 | 0でないIDの保証 | ゼロ不許可を型で表現 |
| super::SymbolId | 返却する型 | このチャンクには定義なし（テストより `pub struct SymbolId(pub u32)` と推測） |

- 被依存推定
  - パーサ/トークナイザ/シンボルテーブルなど、入力ファイル内での**一意なシンボル識別子**が必要なコンポーネント
  - 統計/進捗表示での**現在発行数**の参照

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | `pub fn new() -> Self` | 1から開始するカウンタ生成 | O(1) | O(1) |
| next_id | `pub fn next_id(&mut self) -> super::SymbolId` | 現IDを返し内部を+1 | O(1) | O(1) |
| current_count | `pub fn current_count(&self) -> u32` | 生成済みID数を返す | O(1) | O(1) |
| reset | `pub fn reset(&mut self)` | カウンタを1に戻す | O(1) | O(1) |
| from_value | `pub fn from_value(start_from: u32) -> Self` | 次に返す値を指定して生成（0でパニック） | O(1) | O(1) |
| Default::default | `fn default() -> Self` | `new()` と同じ | O(1) | O(1) |

以下、各APIの詳細:

### 1) SymbolCounter::new

1. 目的と責務
   - カウンタを**1から開始**することを保証するコンストラクタ

2. アルゴリズム（ステップ）
   - `NonZeroU32::new(1)` を `expect` で包み `next_id` に格納

3. 引数
   - なし

4. 戻り値

| 型 | 意味 |
|----|------|
| `Self` | 1から開始するカウンタ |

5. 使用例
```rust
let mut counter = SymbolCounter::new();
assert_eq!(counter.current_count(), 0);
assert_eq!(counter.next_id().0, 1);
```

6. エッジケース
- 特になし（コンストラクタは常に成功）

### 2) SymbolCounter::next_id

1. 目的と責務
   - 現在のIDを `super::SymbolId` として返し、その後**次回値**へインクリメント

2. アルゴリズム
   - `current = self.next_id`
   - `self.next_id = NonZeroU32::new(current.get().checked_add(1).expect(...)).expect(...)`
   - `return SymbolId(current.get())`

3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| self | `&mut self` | 可変参照（単調増加の更新に必要） |

4. 戻り値

| 型 | 意味 |
|----|------|
| `super::SymbolId` | 今回発行されたID（1以上の `u32`） |

5. 使用例
```rust
let mut counter = SymbolCounter::new();
let id1 = counter.next_id();
let id2 = counter.next_id();
assert_eq!(id1.0, 1);
assert_eq!(id2.0, 2);
```

6. エッジケース
- オーバーフロー（約43億回発行後）で `expect` により**パニック**（「Symbol counter overflow - file has more than 4 billion symbols」）

### 3) SymbolCounter::current_count

1. 目的と責務
   - これまでに発行済みのID**個数**を返す（0, 1, 2, ...）

2. アルゴリズム
   - `self.next_id.get() - 1` を返す

3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| self | `&self` | 参照のみ |

4. 戻り値

| 型 | 意味 |
|----|------|
| `u32` | 発行済み数（開始直後は0） |

5. 使用例
```rust
let mut counter = SymbolCounter::new();
assert_eq!(counter.current_count(), 0);
counter.next_id();
assert_eq!(counter.current_count(), 1);
```

6. エッジケース
- 発行前は常に0
- `u32::MAX - 1` まで増え得る（理論上）

### 4) SymbolCounter::reset

1. 目的と責務
   - カウンタを初期状態（次に返す値=1、発行済み数=0）に戻す

2. アルゴリズム
   - `self.next_id = NonZeroU32::new(1).unwrap()`

3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| self | `&mut self` | 可変参照 |

4. 戻り値

| 型 | 意味 |
|----|------|
| `()` | 副作用のみ |

5. 使用例
```rust
let mut counter = SymbolCounter::new();
counter.next_id(); // 1
counter.reset();
assert_eq!(counter.current_count(), 0);
assert_eq!(counter.next_id().0, 1);
```

6. エッジケース
- 特になし（常に成功）

### 5) SymbolCounter::from_value

1. 目的と責務
   - **次に返すID**を `start_from` に設定して新規生成（0は禁止）

2. アルゴリズム
   - `self.next_id = NonZeroU32::new(start_from).expect("Counter value must be non-zero")`

3. 引数

| 名前 | 型 | 意味 |
|------|----|------|
| start_from | `u32` | 次に返すID。0はパニック |

4. 戻り値

| 型 | 意味 |
|----|------|
| `Self` | 指定値から開始するカウンタ |

5. 使用例
```rust
let mut counter = SymbolCounter::from_value(10);
assert_eq!(counter.next_id().0, 10);
assert_eq!(counter.next_id().0, 11);
```

6. エッジケース
- `start_from == 0` は**パニック**
- `start_from == u32::MAX` の場合、最初の発行は `u32::MAX`、次の呼び出しで**オーバーフロー・パニック**

### 6) Default::default

- `SymbolCounter::new()` と同値

## Walkthrough & Data Flow

典型的な利用フロー:

1. `SymbolCounter::new()` で初期化（次に返すID=1、発行済み数=0）
2. `next_id()` を呼ぶたびに
   - 現在の `next_id` をIDとして返却
   - 内部 `next_id` を `+1`
   - `current_count()` は返却回数と一致（0→1→2→…）
3. `reset()` で再度1からやり直し
4. `from_value(x)` で「次に返すID」を `x` に設定して新たに開始

データ流れの要点:
- 内部状態は `NonZeroU32 next_id` の1フィールドのみ
- `super::SymbolId` は毎回新規に値コピーで返却（割当なし）
- すべての操作は**O(1)** で副作用は `next_id` の更新のみ

このチャンクには状態遷移や条件分岐が複雑な箇所は現れないため、Mermaid図は割愛します。

## Complexity & Performance

- 時間計算量
  - `new`, `next_id`, `current_count`, `reset`, `from_value`, `default` はすべて **O(1)**
- 空間計算量
  - インスタンスあたり **O(1)**（`NonZeroU32` 4バイト程度）
- パフォーマンス要因
  - `checked_add(1)` と `NonZeroU32::new(...)` は定数時間で低オーバーヘッド
  - ロック/アトミックなし（単一スレッド前提）
- スケール限界
  - 理論上、約43億回を超える発行で**パニック**（現実的には到達困難）
  - 複数カウンタを併用すればインスタンス間でIDが重複し得る（グローバル一意ではない）

## Edge Cases, Bugs, and Security

セキュリティチェックリスト（このモジュールの性質上、実害のある攻撃面は基本的に該当なし）:
- メモリ安全性: 所有権・借用は安全。配列やポインタ操作なし。整数オーバーフローは `checked_add` 済みで**パニック**として扱い
- インジェクション: SQL/Command/Path いずれも**該当なし**
- 認証・認可: **該当なし**
- 秘密情報: ハードコード秘密・ログ漏洩ともに**該当なし**
- 並行性: 内部に共有可変状態はあるが `&mut self` で制限。ロックは不要。マルチスレッド共有設計ではないため**多重スレッドでの同時利用は想定外**

詳細エッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 初期状態のカウント | なし | `current_count() == 0` | `self.next_id.get() - 1` | OK |
| インクリメント | `next_id()` 連続呼び出し | 1,2,3,… と増加 | `checked_add(1)` | OK |
| リセット | `reset()` | 次で1を返す、カウント0へ | `NonZeroU32::new(1)` | OK |
| 任意開始 | `from_value(10)` | 次に10、その後11 | `NonZeroU32::new(10)` | OK（テスト未あり） |
| ゼロ開始の禁止 | `from_value(0)` | パニック | `expect("Counter value must be non-zero")` | OK（テスト未あり） |
| オーバーフロー | `from_value(u32::MAX)` 後 `next_id()` を2回 | 1回目は `u32::MAX`、2回目でパニック | `checked_add(1).expect(...)` | OK（テスト未あり） |
| クローンの一意性 | `let a = counter.clone()` | 別インスタンスで同系列開始（重複し得る） | `#[derive(Clone)]` | 設計上仕様 |

Rust特有の観点:
- 所有権: 返却値 `super::SymbolId` は値ムーブ（`next_id`）; 参照の生存期間問題は**なし**
- 借用: 変化操作は `&mut self` 必須でデータ競合回避
- ライフタイム: 明示的ライフタイム**不要**
- unsafe境界: **なし**（unsafe ブロックの使用箇所なし）
- Send/Sync: フィールドが `NonZeroU32` のみであり自動的に `Send + Sync` になり得るが、同時可変アクセスは借用規則で禁止（このチャンクには明示の境界は現れない）
- エラー設計: オーバーフロー/ゼロ開始は**パニック**設計。`Result/Option` 非採用
- panic箇所: 
  - `new` 内 `expect("1 is non-zero")`（到達不能）
  - `next_id` 内 `expect("Symbol counter overflow - ...")` と `expect("Incremented value is non-zero")`（前者のみ現実的）
  - `from_value` 内 `expect("Counter value must be non-zero")`（0入力時）

根拠: 各関数の実装はこのチャンクの該当関数本体に記述（行番号はこのチャンクには現れない）

## Design & Architecture Suggestions

- オーバーフロー対処のAPI設計
  - 現状は**パニック**。API利用者が制御可能なよう、以下を追加提案:
    - `try_next_id(&mut self) -> Result<SymbolId, OverflowError>` または `Option<SymbolId>`
    - `try_from_value(u32) -> Result<Self, NonZeroError>`（0時にエラー）
  - 既存関数はそのまま残し、失敗しない前提のユースケース向けと位置づけ

- 命名の明確化
  - `from_value` は「次に返す値」を直接指定する設計。誤解防止のため `from_next_id` などへ改名を検討

- ドキュメンテーションの強化
  - `Clone` の挙動（別インスタンスによる重複ID生成可能）を明記
  - `SymbolId` の仕様（非ゼロ保証など）へのリンク/説明（このチャンクには現れない）

- アノテーション
  - `#[must_use]` を `from_value` にも付与（インスタンスの取りこぼし防止）
  - 微細最適化として `#[inline]` を各メソッドに付与は任意

- const化（ツールチェインが許せば）
  - `const fn new() -> Self` の検討（`NonZeroU32::new(1)` が const なら可能）

- 非同期/並行ユースのガード
  - モジュールレベルのドキュメントで「スレッド間共有は想定外」を明示。必要なら `Arc<Mutex<SymbolCounter>>` 併用ガイドを提示

## Testing Strategy (Unit/Integration) with Examples

既存テスト（このチャンク内）:
- `starts_at_one`: 最初の `next_id()` が1
- `increments`: 1,2,3 と増加
- `current_count`: 発行数の整合
- `reset`: リセット後に0へ戻り、再度1
- `default_impl`: `Default` が初期状態

追加推奨テスト:

1) from_value の正常系
```rust
#[test]
fn test_from_value_starts_from_given() {
    let mut counter = SymbolCounter::from_value(10);
    assert_eq!(counter.next_id().0, 10);
    assert_eq!(counter.next_id().0, 11);
    assert_eq!(counter.current_count(), 2);
}
```

2) from_value のゼロ入力でパニック
```rust
#[test]
#[should_panic(expected = "Counter value must be non-zero")]
fn test_from_value_zero_panics() {
    let _ = SymbolCounter::from_value(0);
}
```

3) オーバーフローパス（境界テスト）
```rust
#[test]
#[should_panic(expected = "Symbol counter overflow")]
fn test_overflow_panics() {
    let mut counter = SymbolCounter::from_value(u32::MAX);
    let _ = counter.next_id(); // u32::MAX
    let _ = counter.next_id(); // ここでパニック
}
```

4) Clone の挙動（系列重複の明示）
```rust
#[test]
fn test_clone_produces_independent_sequences() {
    let mut c1 = SymbolCounter::new();
    let mut c2 = c1.clone();
    assert_eq!(c1.next_id().0, 1);
    assert_eq!(c2.next_id().0, 1); // 同じ系列を独立に生成
}
```

5) 性質テスト（プロパティ）
- すべての返却値が非ゼロ
- 単調増加であること（隣接差が常に+1）
- `reset` 後の挙動が初期状態と等価

## Refactoring Plan & Best Practices

- 例外（パニック）より**戻り値エラー**を選べるAPIの追加（`try_next_id`, `try_from_value`）
- `from_value` の命名改善（`from_next_id` 等）で意図を明確化
- `#[must_use]` をコンストラクタ群に統一付与（`from_value`）
- ドキュメントに**スレッドモデル**と**Cloneの注意点**を明記
- `Copy` は付与しない（誤コピーによる重複系列生成リスクが高まるため、現状の `Clone` のみで十分）
- ベンチマークは任意（O(1) で十分高速だが、ホットパスなら `#[inline]` の効果計測）

## Observability (Logging, Metrics, Tracing)

- ロギング/トレース: このレベルの単純なカウンタには通常不要
- メトリクス:
  - 利用側で `current_count()` をポーリングして進捗をレポート
  - オーバーフローが発生した場合はパニックで終了するため、回避設計（`try_next_id`）導入時はエラー件数メトリクスを記録可能

## Risks & Unknowns

- `super::SymbolId` の正確な定義は**このチャンクには現れない**。テストから「タプル構造体で `.0` が `u32` かつフィールド公開」と推測されるが、最終仕様は上位モジュール依存
- グローバル一意性は**保証しない**（インスタンス単位）。複数 `SymbolCounter`（含む `clone`）の併用で重複IDが発生し得る
- マルチスレッド使用は**想定外**。共有するなら外側で同期化が必要（例: `Mutex`）
- パニック設計（オーバーフロー / 0開始）により、ライブラリとしては**エラー指向API**の選択肢が望まれる場面がある（利用側要件次第）