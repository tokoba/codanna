# mod.rs Review

## TL;DR

- 目的: コアとなるID型（**SymbolId**, **FileId**）、位置情報（**Range**）、記号種別（**SymbolKind**）、インデックス結果（**IndexingResult**）、および文字列縮約（**CompactString**）を提供する共通型モジュール。キャッシュ・新規インデックスの区別を表現。
- 主要公開API: SymbolId/FileId の生成・取得、Range の包含判定、SymbolKind の文字列パース（既定フォールバック付）、IndexingResult のユーティリティ、compact_string。
- 複雑箇所: SymbolKind::from_str は14分岐の厳密な大文字一致のマッチング。Range の包含判定は行・列の境界含む比較。
- 重大リスク: Range::new に入力検証がなく「start > end」や「列の逆転」を許容してしまう可能性。SymbolKind パースの大小文字差・未知値フォールバックが仕様誤解を招く恐れ。エラー型が &'static str と素朴。
- Rust安全性: unsafe 不使用。所有権・借用はシンプルで Copy 型中心。Box<str> の使用によりコンパクトなヒープ所有文字列。
- 並行性: すべての型は Send + Sync（中身がプリミティブ/Box<str>）でデータ競合なし。非同期・awaitは登場しない。
- 追加提案: Range の不変条件チェック、IDには NonZeroU32 の採用、SymbolKind に Unknown 変種または TryFrom の導入、エラー型の強化、パースの大小文字正規化。

## Overview & Purpose

このモジュールは、インデックスやシンボル解析の基盤となる型群を定義・公開します。外部とのデータ契約（serdeでのシリアライズ/デシリアライズ）をサポートし、他モジュール（例: symbol_counter）での集計・参照を容易にします。

- **SymbolId / FileId**: 0以外の u32 をラップする識別子。新規作成は 0 を拒否（Option で表現）。
- **IndexingResult**: インデックス結果（新規かキャッシュ）を明確化。ファイルID取得ユーティリティあり。
- **Range**: 行・列範囲の位置情報（両端含む）。範囲内チェック API。
- **SymbolKind**: 記号種別（関数、構造体など）を列挙。文字列からのパースおよび既定値へのフォールバック。
- **CompactString**: Box<str> によるコンパクト文字列（ヒープ所有、不変）。
- **SymbolCounter**: 下位モジュールからの再エクスポート（詳細はこのチャンクには現れない）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | symbol_counter | private + re-export | 記号数の集計（推定） | 不明 |
| Struct | SymbolId(u32) | pub | シンボル識別子（0禁止） | Low |
| Struct | FileId(u32) | pub | ファイル識別子（0禁止） | Low |
| Enum | IndexingResult | pub | インデックス結果の状態保持 | Low |
| Struct | Range | pub | 行・列での位置範囲、包含判定 | Low |
| Enum | SymbolKind | pub | 記号種別の列挙と文字列パース | Med |
| Type Alias | CompactString = Box<str> | pub | ヒープ所有の不変文字列 | Low |
| Function | compact_string(&str) -> CompactString | pub | &str を Box<str> に変換 | Low |
| Trait Impl | FromStr for SymbolKind | pub | 文字列から種別へ厳密パース | Med |

### Dependencies & Interactions

- 内部依存
  - SymbolKind::from_str_with_default → 標準の FromStr 実装を呼び出し（s.parse()）。結果が Err の場合に既定値（Function）へフォールバック。
  - IndexingResult::file_id / is_cached は自身の列挙値に対する直交ユーティリティ。
- 外部依存（このファイル中に現れるもの）
  - serde（Serialize, Deserialize 派生）
  - std::str::FromStr（SymbolKind のパース）
- 被依存推定
  - インデクサ/パーサが SymbolKind, Range を使用
  - キャッシュ管理層が IndexingResult を使用
  - 検索・可視化レイヤが SymbolId, FileId をキーとして使用
  - symbol_counter はこの型群を前提にカウント機能を提供（詳細不明）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| SymbolId::new | fn new(value: u32) -> Option<SymbolId> | 0禁止のID生成 | O(1) | O(1) |
| SymbolId::value | fn value(&self) -> u32 | 内部値参照 | O(1) | O(1) |
| SymbolId::to_u32 | fn to_u32(self) -> u32 | 所有権を伴う値取得 | O(1) | O(1) |
| FileId::new | fn new(value: u32) -> Option<FileId> | 0禁止のID生成 | O(1) | O(1) |
| FileId::value | fn value(&self) -> u32 | 内部値参照 | O(1) | O(1) |
| FileId::to_u32 | fn to_u32(self) -> u32 | 所有権を伴う値取得 | O(1) | O(1) |
| IndexingResult::file_id | fn file_id(&self) -> FileId | 結果からファイルID取得 | O(1) | O(1) |
| IndexingResult::is_cached | fn is_cached(&self) -> bool | キャッシュ判定 | O(1) | O(1) |
| Range::new | fn new(u32,u16,u32,u16) -> Range | 範囲生成（検証なし） | O(1) | O(1) |
| Range::contains | fn contains(&self, u32, u16) -> bool | 位置が範囲内か判定 | O(1) | O(1) |
| SymbolKind::from_str | fn from_str(&str) -> Result<SymbolKind, &'static str> | 厳密な大文字一致パース | O(1) | O(1) |
| SymbolKind::from_str_with_default | fn from_str_with_default(&str) -> SymbolKind | 既定値フォールバック付パース | O(1) | O(1) |
| compact_string | fn compact_string(&str) -> CompactString | &str を Box<str> 化 | O(n) | O(n) |
| CompactString | type CompactString = Box<str> | データ契約（不変文字列所有） | N/A | N/A |
| SymbolCounter | pub use symbol_counter::SymbolCounter | 記号数集計（詳細不明） | 不明 | 不明 |

データ契約のポイント
- **SymbolId / FileId**: 値は 0 非許容（new が None を返す）。シリアライズは素の u32 フィールドとして行われる（serde派生）。
- **Range**: 行・列は非負整数。両端を含む包含判定。生成時の整合性（start ≤ end）保証は実装されていない。
- **SymbolKind**: 14固定バリアント。文字列は大文字の英語名に限り一致。未知値は Err または Function へフォールバック（from_str_with_default）。
- **CompactString**: Box<str> によりメモリフットプリントを抑えた不変所有文字列。

以下、主なAPIの詳細。

### SymbolId::new

1. 目的と責務
   - 0 を拒否した安全な識別子生成。

2. アルゴリズム
   - value == 0 → None
   - それ以外 → Some(SymbolId(value))

3. 引数
   | 名 | 型 | 説明 |
   |----|----|------|
   | value | u32 | 0禁止の識別子値 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Option<SymbolId> | 成功時 Some、0なら None |

5. 使用例
   ```rust
   let id = SymbolId::new(42).expect("non-zero id");
   assert_eq!(id.value(), 42);
   ```

6. エッジケース
   - 0入力は None
   - u32の最大値も受理

同様の仕様が FileId::new にも適用されます。

### IndexingResult::{file_id, is_cached}

1. 目的と責務
   - 結果からファイルIDを取得、およびキャッシュかどうかの判定。

2. アルゴリズム
   - file_id: Indexed(id) / Cached(id) から id を返す
   - is_cached: Cached(_) に一致するか判定

3. 引数
   | 名 | 型 | 説明 |
   |----|----|------|
   | self | &IndexingResult | 状態値 |

4. 戻り値
   | 関数 | 型 | 説明 |
   |------|----|------|
   | file_id | FileId | ファイルID |
   | is_cached | bool | キャッシュなら true |

5. 使用例
   ```rust
   let res = IndexingResult::Indexed(FileId::new(1).unwrap());
   assert_eq!(res.file_id().value(), 1);
   assert!(!res.is_cached());
   ```

6. エッジケース
   - FileId が 0 になるケースは new が拒否するため通常発生しない

### Range::{new, contains}

1. 目的と責務
   - 行・列から範囲を作成し、座標が範囲内かチェック。

2. アルゴリズム（contains）
   - 行が start_line 未満または end_line 超過 → false
   - 行が start_line かつ column が start_column 未満 → false
   - 行が end_line かつ column が end_column 超過 → false
   - それ以外 → true

3. 引数
   | 名 | 型 | 説明 |
   |----|----|------|
   | start_line | u32 | 開始行 |
   | start_column | u16 | 開始列 |
   | end_line | u32 | 終了行 |
   | end_column | u16 | 終了列 |
   | line | u32 | 判定対象行（contains時） |
   | column | u16 | 判定対象列（contains時） |

4. 戻り値
   | 関数 | 型 | 説明 |
   |------|----|------|
   | new | Range | 範囲（検証なし） |
   | contains | bool | 範囲内なら true |

5. 使用例
   ```rust
   let r = Range::new(10, 5, 15, 20);
   assert!(r.contains(10, 5));   // 始端含む
   assert!(r.contains(12, 0));   // 中間行は列を問わず含む
   assert!(!r.contains(9, 99));  // 行が手前
   assert!(!r.contains(15, 21)); // 終端超過の列
   ```

6. エッジケース
   - start_line > end_line や start_column > end_column の場合の意味は未定義（検証なし）
   - 列の比較は端の行のみで行う（中間行は列無視）

### SymbolKind::{from_str, from_str_with_default}

1. 目的と責務
   - 文字列から SymbolKind を厳密にパース。既定値を返すヘルパーあり。

2. アルゴリズム
   - 大文字の固定文字列 14 種に一致 → 対応する変種
   - それ以外 → Err("Unknown symbol kind")
   - from_str_with_default: parse() の結果が Err なら Function

3. 引数
   | 名 | 型 | 説明 |
   |----|----|------|
   | s | &str | 入力文字列 |

4. 戻り値
   | 関数 | 型 | 説明 |
   |------|----|------|
   | from_str | Result<SymbolKind, &'static str> | 成功/失敗 |
   | from_str_with_default | SymbolKind | 失敗時 Function |

5. 使用例
   ```rust
   use std::str::FromStr;

   let k = SymbolKind::from_str("Struct").unwrap();
   assert_eq!(k, SymbolKind::Struct);

   let k2 = SymbolKind::from_str("struct"); // 大文字小文字違い
   assert!(k2.is_err());

   let k3 = SymbolKind::from_str_with_default("unknown");
   assert_eq!(k3, SymbolKind::Function); // 既定フォールバック
   ```

6. エッジケース
   - 大文字小文字の違いで一致しない（"function" は Err）
   - 前後空白で一致しない（" Function " は Err）
   - 新しい種別追加時に漏れがあると Err（または既定値）になる

### compact_string

1. 目的と責務
   - &str を **CompactString**（Box<str>）へ所有化しメモリ効率を向上。

2. アルゴリズム
   - s.into() により Box<str> へ変換（From<&str> 実装に依拠）

3. 引数
   | 名 | 型 | 説明 |
   |----|----|------|
   | s | &str | 入力文字列 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | CompactString | Box<str>（不変所有） |

5. 使用例
   ```rust
   let cs: CompactString = compact_string("hello");
   assert_eq!(&*cs, "hello");
   ```

6. エッジケース
   - 非UTF-8は &str として渡せないため対象外
   - 非常に長い文字列は O(n) のアロケーション・コピー

## Walkthrough & Data Flow

- 入力文字列（例えば言語サーバの出力やソース解析結果のメタデータ）→ **SymbolKind::from_str** で種別へマッピング。未知値は **from_str_with_default** により **Function** フォールバックが可能。
- ソースコード位置情報 → **Range** で保持し、カーソル位置が **contains** で属するか判定。
- インデックス処理 → 結果を **IndexingResult::{Indexed, Cached}** で表現。上位ロジックは **file_id** と **is_cached** を利用して分岐。
- ID生成 → **SymbolId::new**/**FileId::new** を通し 0 を拒否したセーフな識別子を使用。

Mermaid図（SymbolKind パースの主要分岐）

```mermaid
flowchart TD
  A[入力 s:&str] --> B{認識済み?\nFunction/Method/Struct/.../Macro}
  B -->|Function| C[Ok(Function)]
  B -->|Method| D[Ok(Method)]
  B -->|Struct| E[Ok(Struct)]
  B -->|Enum| F[Ok(Enum)]
  B -->|Trait| G[Ok(Trait)]
  B -->|Interface| H[Ok(Interface)]
  B -->|Class| I[Ok(Class)]
  B -->|Module| J[Ok(Module)]
  B -->|Variable| K[Ok(Variable)]
  B -->|Constant| L[Ok(Constant)]
  B -->|Field| M[Ok(Field)]
  B -->|Parameter| N[Ok(Parameter)]
  B -->|TypeAlias| O[Ok(TypeAlias)]
  B -->|Macro| P[Ok(Macro)]
  B -->|その他| Z[Err("Unknown symbol kind")]
```

上記の図は `impl FromStr for SymbolKind` のマッチ分岐（行番号は不明）を示しています。

対応コード抜粋

```rust
impl FromStr for SymbolKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Function" => Ok(SymbolKind::Function),
            "Method" => Ok(SymbolKind::Method),
            "Struct" => Ok(SymbolKind::Struct),
            "Enum" => Ok(SymbolKind::Enum),
            "Trait" => Ok(SymbolKind::Trait),
            "Interface" => Ok(SymbolKind::Interface),
            "Class" => Ok(SymbolKind::Class),
            "Module" => Ok(SymbolKind::Module),
            "Variable" => Ok(SymbolKind::Variable),
            "Constant" => Ok(SymbolKind::Constant),
            "Field" => Ok(SymbolKind::Field),
            "Parameter" => Ok(SymbolKind::Parameter),
            "TypeAlias" => Ok(SymbolKind::TypeAlias),
            "Macro" => Ok(SymbolKind::Macro),
            _ => Err("Unknown symbol kind"),
        }
    }
}
```

## Complexity & Performance

- 時間計算量
  - **SymbolId/FileId new/value/to_u32**: O(1)
  - **IndexingResult file_id/is_cached**: O(1)
  - **Range new/contains**: O(1)
  - **SymbolKind from_str/from_str_with_default**: O(1)（分岐数は定数14）
  - **compact_string**: O(n)（文字列長）でアロケーションとコピー

- 空間計算量
  - ほぼ O(1)。compact_string のみ O(n) で文字列格納。

- ボトルネック
  - 大量の文字列変換において **compact_string** が支配的。シンボル数が多い場合はアロケーション圧が増大。
  - SymbolKind のパースは定数時間で軽量。

- 実運用負荷要因
  - I/O・ネットワーク・DB はこのファイルには登場しない。
  - 多数の Range 判定は CPU 依存だが O(1) のためスケールしやすい。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価
- メモリ安全性: unsafe 不使用。Buffer overflow, Use-after-free, Integer overflow の懸念なし（u32/u16 使用、演算は比較のみ）。
- インジェクション: SQL/Command/Path traversal 不該当（I/Oなし）。
- 認証・認可: 機能なし（このチャンクには現れない）。
- 秘密情報: ハードコード秘密なし。ログ漏えいも不該当。
- 並行性: Race/Deadlock 不該当（共有可変状態なし）。

詳細エッジケース

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 0のID拒否 | SymbolId::new(0) | None | 0でNoneを返す | OK |
| Rangeの逆転（行） | Range::new(15,0,10,0) | Errまたは正規化 | 検証なし。containsの意味が曖昧 | 要対応 |
| Rangeの逆転（列） | Range::new(10,20,10,5) | Errまたは正規化 | 検証なし。containsの境界が不正確 | 要対応 |
| 列比較の適用範囲 | Range::new(10,5,15,20) で (12,0) | 中間行は列無視で包含 | 実装通り包含 | OK |
| SymbolKindの大小文字 | "function" | Err（厳密一致） | Err 返却 | OK（仕様次第） |
| 未知種別の既定値 | from_str_with_default("???") | 明示的 Unknown か Err | Functionへフォールバック | 仕様の明確化要 |
| 余分な空白 | " Function " | trim後一致 | trimなしで Err | 要検討 |
| 長大文字列の確保 | compact_string(巨大) | メモリ圧増 | Box<str>へ所有化 | 注意 |
| エラー情報密度 | from_str の Err | 詳細なエラー型 | &'static str 固定文言 | 要改善 |
| シリアライズ互換性 | serde導入 | バリアントの追加/変更で互換注意 | Serialize/Deserialize派生 | 注意 |

Rust特有の観点

- 所有権
  - **to_u32(self)** は値をコピーして返す（Copy）。所有権の移動はない実質。
  - **compact_string** は &str を所有化（新たな Box<str> を確保）。元の &str の所有権に影響なし。
- 借用
  - メソッドは不変借用（&self）中心。可変借用なし。
- ライフタイム
  - 明示的ライフタイムなし。Box<str> は所有のためライフタイム問題なし。
- unsafe境界
  - unsafe 不使用。
- Send/Sync
  - SymbolId/FileId/Range/IndexingResult/SymbolKind はプリミティブ/列挙で自動的に Send + Sync。
  - CompactString(Box<str>) も Send + Sync。
- 非同期/await
  - 非同期コードなし。キャンセル・await 境界なし。
- エラー設計
  - **Result vs Option**: ID生成は Option（0拒否）で妥当。ただし NonZeroU32 採用でより型安全化が可能。
  - **panic**: 本実装は panic 要素なし。テストでのみ unwrap 使用。
  - **エラー変換**: From/Into 実装はなし。from_str の Err は &'static str で情報密度低。

## Design & Architecture Suggestions

- **Range の不変条件**の導入
  - new で `start_line <= end_line` と同一行なら `start_column <= end_column` を検証し、違反は Result で返す。または正規化。
  ```rust
  impl Range {
      pub fn try_new(sl: u32, sc: u16, el: u32, ec: u16) -> Result<Self, RangeError> { /* 検証 */ }
  }
  ```
- **NonZeroU32 の採用**
  - SymbolId/FileId 内部を `std::num::NonZeroU32` に置換し、コンストラクタに `TryFrom<u32>` を提供。0の不変性を型で保証。
- **SymbolKind のエラー強化/Unknown 変種**
  - `Unknown(String)` を追加するか、独自の Error 型を用意（列挙値、詳細メッセージ、元文字列を保持）。
  - パース時に `trim()` と大小文字正規化（to_ascii_uppercase）を検討。
- **Display/FromStr/Serde 表現の一貫性**
  - Display 実装で文字列表現を定義し、Serde の `#[serde(rename = "...")]` を活用して外部フォーマットの安定化。
- **compact_string の命名**
  - 役割を明確化するため `to_compact_string` や `own_str` 等、所有化を示す命名を検討。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本を網羅。以下の追加を推奨。

- Range の不正入力検証（現状は「検証なし」なので期待動作を決める）
  ```rust
  #[test]
  fn range_inverted_lines_behaviour() {
      let r = Range::new(15, 0, 10, 0);
      // 仕様決定に応じて期待値を設定（例: contains は常に false）
      assert!(!r.contains(12, 0));
  }
  ```

- 同一行での列逆転
  ```rust
  #[test]
  fn range_same_line_inverted_columns() {
      let r = Range::new(10, 20, 10, 5);
      assert_eq!(r.contains(10, 10), /* 仕様次第: false を推奨 */ false);
  }
  ```

- SymbolKind のトリミング・大小文字
  ```rust
  #[test]
  fn symbol_kind_case_and_whitespace() {
      assert!(SymbolKind::from_str(" function ").is_err());
      assert!(SymbolKind::from_str("function").is_err());
      let k = SymbolKind::from_str_with_default("function");
      assert_eq!(k, SymbolKind::Function);
  }
  ```

- property-based testing（proptest/quickcheck）で Range::contains の単調性検証
  ```rust
  // 擬似コード: startとendを正規化した場合に包含が反例を生じないことを検証
  ```

- Serde ラウンドトリップ
  ```rust
  #[test]
  fn serde_roundtrip_symbol_id() {
      let id = SymbolId::new(7).unwrap();
      let json = serde_json::to_string(&id).unwrap();
      let back: SymbolId = serde_json::from_str(&json).unwrap();
      assert_eq!(back, id);
  }
  ```

## Refactoring Plan & Best Practices

- ステップ1: **NonZeroU32** を内部表現に導入し、`TryFrom<u32>` 実装を追加。既存 `new` は非推奨化。
- ステップ2: **Range::try_new** を追加し、正規化または検証を行う。既存 `new` は非推奨化または `try_new(..).unwrap()` に置換する。
- ステップ3: **SymbolKind** のパース改善
  - `trim()` とケース無視（ASCII 大文字化）オプションを用意。
  - 明示的な **Unknown** 変種か強化されたエラー型を導入。
- ステップ4: API 一貫性
  - `to_u32`/`value` の命名規約整備（`get()` vs `into_inner()` 等）。
  - `Display`/`FromStr`/`serde(rename)` を統合。
- ステップ5: ドキュメントコメントとデータ契約の明記
  - 範囲の包含規則（両端含む、列比較の適用範囲）を rustdoc に明記。

## Observability (Logging, Metrics, Tracing)

- 現状ロギング・メトリクス・トレースはなし（このチャンクには現れない）。
- 提案
  - インデックス結果の統計（Indexed/Cached の比率）を **SymbolCounter** と連携してメトリクス化（不明）。
  - Range の異常検知（逆転インプット発生回数）をカウンタで記録。
  - SymbolKind パース失敗件数のメトリクスを追加し、入力品質を可視化。

## Risks & Unknowns

- **SymbolCounter の実装不明**: 再エクスポートされているが、このチャンクには現れないため責務・性能・安全性は不明。
- **Rangeの仕様リスク**: 逆転の不変条件未検証により、上位ロジックが誤判定する可能性。
- **SymbolKind フォールバックの意味合い**: 未知種別を Function とする仕様が誤集計・誤分類につながる恐れ。仕様の明確化・変更可否の確認が必要。
- **Serde 互換性**: 変種追加・名称変更時に後方互換性が揺らぐ可能性。外部フォーマットに対するバージョニング方針が不明。
- **行番号などの参照**: このチャンクには行番号情報がないため、詳細なコード位置の根拠は「不明」。