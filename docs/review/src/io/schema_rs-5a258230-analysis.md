# schema.rs Review

## TL;DR

- 目的: CLI/MCPの出力を統一するための**ゼロコスト抽象**を提供し、異なる出力形に対応しつつ型安全と低オーバーヘッドを維持
- 主要公開API: **UnifiedOutput**, **OutputData**, **UnifiedOutputBuilder**（items/grouped/ranked/contextual + build）, **search_results_output**, **impact_analysis_output**, **symbol_list_output**
- 複雑箇所: 出力形の分岐（Items/Grouped/Contextual/Ranked/Single/Empty）と**untagged + flatten**のシリアライズ設計、count/exit_codeの算出ロジック
- 重要なデータ契約: OutputStatus/EntityTypeはsnake_case、OutputDataはuntaggedでトップレベルに**items/groups/results/item**が現れる、exit_codeはシリアライズ対象外
- Rust安全性: **unsafe未使用**、**Cow**による借用最適化、ジェネリクスの型境界に注意（Serializeが必要）
- 重大リスク: untaggedのデシリアライズ曖昧性（ただしUnifiedOutputはDeserializeしていないため回避）、Contextの値型がserde_json::Valueであるため巨大ペイロードに注意
- 並行性: 同期のみ、共有状態なし。型がSend/Syncになるかは**Tと参照寿命'a次第**（設計上の前提を明確化推奨）

## Overview & Purpose

このファイルは io/schema.rs（パス: io/schema.rs）の**統一出力スキーマ**を定義します。CLIやMCPツールからの出力の整形を統一し、以下を達成します。

- 異なる出力形（単一/リスト/グループ化/ランキング/文脈付き/空）を一つの型で表現
- **Cow<'a, str>** を用いた借用による**割り当て削減**
- **Serialize**導出によるJSON出力のシンプル化（untagged + flattenで軽量）
- ビルダーAPIでの**ergonomic**な構築と**count/exit_code**の自動計算
- テキスト出力向けの**fmt::Display**実装

用途としては、検索結果のランキング、影響分析のグループ化、文脈付き結果の提示など、開発者向けCLI/MCP出力に適合します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | UnifiedOutput<'a, T> | pub | 統一出力のトップレベル。status/entity_type/count/data/metadata/guidance/exit_codeを保持 | Med |
| Enum | OutputStatus | pub | 成功/未検出/部分成功/エラーの状態管理 | Low |
| Enum | EntityType | pub | 出力対象のエンティティ種別（Symbol, Functionなど） | Low |
| Enum | OutputData<'a, T> | pub | 出力形（Items/Grouped/Contextual/Ranked/Single/Empty） | Med |
| Struct | ContextualItem<'a, T> | pub | アイテムに追加のコンテキストと関係を付与 | Med |
| Struct | RankedItem<'a, T> | pub | アイテムにスコア/順位/軽量メタデータを付与 | Low |
| Struct | ItemRelationships<'a> | pub | 呼び出し関係/実装関係/影響の集合 | Med |
| Struct | RelatedItem<'a> | pub | 関連アイテムの最小情報（id/name/kind/file_path） | Low |
| Struct | OutputMetadata<'a> | pub | クエリ/ツール/タイミング/切り詰め/追加メタ | Low |
| Trait | IntoUnifiedOutput<'a> | pub | ゼロコストでUnifiedOutputに変換する契約 | Low |
| Struct | UnifiedOutputBuilder<'a, T> | pub | 出力構築のビルダー（items/grouped/ranked/contextual/...） | Med |
| Fn | symbol_list_output | pub | Symbolの単純リスト出力を構築 | Low |
| Fn | search_results_output<'a, T> | pub | (T, f32)のペアをランキング出力に変換しメタ付与 | Low |
| Fn | impact_analysis_output<'a> | pub | SymbolKind毎にグループ化された影響分析出力を構築 | Low |
| Impl | fmt::Display for UnifiedOutput<'a, T: Display> | pub | テキスト出力の整形 | Med |

### Dependencies & Interactions

- 内部依存
  - UnifiedOutputBuilder.build → OutputData各バリアントを集計して**count**を算出し、**status**から**exit_code**へマッピング（関数名: build, 行番号: 不明）
  - search_results_output → RankedItemを生成し、OutputMetadata.queryにCow::Borrowedを設定、Builderに委譲
  - impact_analysis_output → HashMap<SymbolKind, Vec<Symbol>>をCow::Ownedのキーに変換しBuilderに委譲
  - Display impl → OutputData各形に応じてテキスト整形

- 外部依存（クレート/モジュール）
  | 依存 | 用途 |
  |------|------|
  | crate::io::ExitCode | OS準拠の終了コード割り当て |
  | crate::symbol::Symbol | Symbol出力に使用 |
  | crate::types::{SymbolId, SymbolKind} | 関連アイテム/グループキーに使用 |
  | serde::{Serialize, Deserialize} | JSONシリアライズ/デシリアライズ（UnifiedOutputはSerializeのみ） |
  | std::borrow::Cow | 借用/所有の両立 |
  | std::collections::HashMap | グループ/メタデータ/コンテキスト |
  | std::fmt | テキスト整形 |

- 被依存推定
  - CLIコマンドの結果整形層
  - MCPツールの応答生成
  - 検索/影響分析/インデックス情報のプレゼンテーション層

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| UnifiedOutput | struct UnifiedOutput<'a, T> | 統一出力のトップレベル容器 | O(1) 構築 | O(n) データ保持 |
| OutputStatus | enum OutputStatus | 出力の状態表現 | — | — |
| EntityType | enum EntityType | エンティティ種別 | — | — |
| OutputData | enum OutputData<'a, T> | 出力形（Items/Grouped/Contextual/Ranked/Single/Empty） | 集計: O(n) | O(n) |
| ContextualItem | struct ContextualItem<'a, T> | 文脈情報付アイテム | — | O(k) context |
| RankedItem | struct RankedItem<'a, T> | スコア/順位付アイテム | 生成: O(n) | O(n) |
| ItemRelationships | struct ItemRelationships<'a> | 関係情報の集合 | — | O(m) |
| RelatedItem | struct RelatedItem<'a> | 関連アイテム最小情報 | — | O(1) |
| OutputMetadata | struct OutputMetadata<'a> | メタデータ | — | O(e) |
| IntoUnifiedOutput | trait IntoUnifiedOutput<'a> | ゼロコスト変換契約 | 実装次第 | 実装次第 |
| UnifiedOutputBuilder::items | fn items(Vec<T>, EntityType) -> Self | リスト出力のビルダー生成 | O(1) | O(1) |
| UnifiedOutputBuilder::grouped | fn grouped(HashMap<Cow<'a, str>, Vec<T>>, EntityType) -> Self | グループ化出力のビルダー生成 | O(g) 集計 | O(1) |
| UnifiedOutputBuilder::ranked | fn ranked(Vec<RankedItem<'a, T>>, EntityType) -> Self | ランキング出力のビルダー生成 | O(1) | O(1) |
| UnifiedOutputBuilder::contextual | fn contextual(Vec<ContextualItem<'a, T>>, EntityType) -> Self | 文脈付き出力のビルダー生成 | O(1) | O(1) |
| UnifiedOutputBuilder::with_metadata | fn with_metadata(self, OutputMetadata<'a>) -> Self | メタデータ付与 | O(1) | O(1) |
| UnifiedOutputBuilder::with_guidance | fn with_guidance(self, impl Into<Cow<'a, str>>) -> Self | ガイダンス付与 | O(1) | O(1) |
| UnifiedOutputBuilder::with_status | fn with_status(self, OutputStatus) -> Self | ステータス上書き | O(1) | O(1) |
| UnifiedOutputBuilder::build | fn build(self) -> UnifiedOutput<'a, T> | count/exit_code計算して構築 | O(n) | O(1) +
| symbol_list_output | fn symbol_list_output(Vec<Symbol>) -> UnifiedOutput<'static, Symbol> | Symbolの単純リスト出力 | O(1) + build O(n) | O(n) |
| search_results_output | fn search_results_output<'a, T>(Vec<(T, f32)>, &'a str) -> UnifiedOutput<'a, T> | ランキング出力の生成（順位付与 + クエリメタ） | O(n) | O(n) |
| impact_analysis_output | fn impact_analysis_output<'a>(HashMap<SymbolKind, Vec<Symbol>>) -> UnifiedOutput<'a, Symbol> | 影響分析の種別グループ化出力 | O(n + g) | O(n) |
| Display impl | impl fmt::Display for UnifiedOutput<'a, T: fmt::Display> | テキスト出力 | O(n) | — |

詳細説明（主要API）

1) UnifiedOutput<'a, T>
- 目的と責務
  - 異なる形のデータを**一括統合**するトップレベル。**status/entity_type/count/data/metadata/guidance/exit_code**を保持し、シリアライズ時にdataをトップレベルへflattenします。
- アルゴリズム
  - 構築はBuilderで行い、countとexit_codeを算出。Serialize派生によりJSON化。
- 引数
  | フィールド | 型 | 説明 |
  |-----------|----|------|
  | status | OutputStatus | 操作の状態 |
  | entity_type | EntityType | エンティティ種別 |
  | count | usize | 要素数（Builder計算） |
  | data | OutputData<'a, T> | 実データ |
  | metadata | Option<OutputMetadata<'a>> | メタ情報 |
  | guidance | Option<Cow<'a, str>> | AIガイダンス |
  | exit_code | ExitCode | シリアライズ対象外 |
- 戻り値
  - 該当なし（構造体）
- 使用例
  ```rust
  // リストを直接UnifiedOutputにしたい場合はBuilder経由で
  let out = UnifiedOutputBuilder::items(vec![1,2,3], EntityType::IndexInfo).build();
  ```
- エッジケース
  - dataがEmpty → count=0、statusがNotFoundの場合はExitCode::NotFound

2) OutputData<'a, T>
- 目的と責務
  - 出力形を切り替えるバリアント。serdeで**untagged**、UnifiedOutputで**flatten**されるため、JSONではバリアントのフィールドが直接現れる。
- アルゴリズム
  - Builder.build時にバリアント別にcount計算。
- 代表的使用例
  ```rust
  let data = OutputData::Items { items: vec!["a", "b"] };
  ```
- エッジケース
  - Groupedのgroupsが空 → count=0

3) UnifiedOutputBuilder<'a, T>::build
- 目的と責務
  - 出力形から**count**を算出し、**status→exit_code**をマップしてUnifiedOutputを構築。
- アルゴリズム（ステップ）
  1. match self.dataで要素数を算出（Items/Grouped/Contextual/Ranked/Single/Empty）
  2. match self.statusでExitCodeへ変換
  3. UnifiedOutputを返す
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | self | Self | ビルダー所有権 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | UnifiedOutput<'a, T> | 完成した統一出力 |
- 使用例
  ```rust
  let out = UnifiedOutputBuilder::ranked(vec![], EntityType::SearchResult)
      .with_guidance("Try another query")
      .with_status(OutputStatus::NotFound)
      .build();
  ```
- エッジケース
  - PartialSuccess → ExitCode::Success（成功扱い）

4) search_results_output<'a, T>
- 目的と責務
  - (T, f32) → RankedItemへの変換、順位付与、クエリメタ付与
- アルゴリズム
  - enumerateでrank=1-based付与、Cow::Borrowed(query)をmetadata.queryに設定、Builderで構築
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | results | Vec<(T, f32)> | アイテムとスコア |
  | query | &'a str | 検索クエリ |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | UnifiedOutput<'a, T> | ランキング結果 |
- 使用例
  ```rust
  let output = search_results_output(vec![("x", 0.9), ("y", 0.8)], "foo");
  ```
- エッジケース
  - resultsが空 → status=NotFound

5) impact_analysis_output<'a>
- 目的と責務
  - SymbolKind毎にSymbolをグループ化して出力
- アルゴリズム
  - HashMap<SymbolKind, Vec<Symbol>>をmapし、キーをformat!("{kind:?}")で文字列化してCow::Ownedにする
  - Builder.groupedで構築
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | symbols_by_kind | HashMap<SymbolKind, Vec<Symbol>> | 種別毎のシンボル群 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | UnifiedOutput<'a, Symbol> | グループ化結果 |
- 使用例
  ```rust
  let mut by_kind = std::collections::HashMap::new();
  // by_kind.insert(SymbolKind::Function, vec![/* Symbol */]);
  let out = impact_analysis_output(by_kind);
  ```
- エッジケース
  - すべてのグループが空 → status=NotFound

6) fmt::Display for UnifiedOutput<'a, T: Display>
- 目的と責務
  - ヒューマンリーダブルなテキスト出力
- アルゴリズム
  - dataバリアントごとにforループで整形。Rankedではrank/scoreを出力、ContextualではContextキー/値をインデントして出力
- 使用例
  ```rust
  println!("{}", out); // 各形に応じて適切に整形される
  ```

データ契約（JSON例）

- Items（flatten + untagged）
  ```json
  {
    "status": "success",
    "entity_type": "symbol",
    "count": 2,
    "items": [{"id":1,"name":"foo"},{"id":2,"name":"bar"}],
    "metadata": {"query":"foo"},
    "guidance": "✅"
  }
  ```
- Grouped
  ```json
  {
    "status": "success",
    "entity_type": "impact",
    "count": 3,
    "groups": {
      "Function": [{"id":1,"name":"f"}],
      "Class": [{"id":2,"name":"C"},{"id":3,"name":"D"}]
    }
  }
  ```
- Ranked
  ```json
  {
    "status": "success",
    "entity_type": "search_result",
    "count": 2,
    "results": [
      {"item":"x","score":0.950,"rank":1,"metadata":{}},
      {"item":"y","score":0.850,"rank":2,"metadata":{}}
    ],
    "metadata": {"query":"foo"}
  }
  ```
- Single
  ```json
  {
    "status": "success",
    "entity_type": "mixed",
    "count": 1,
    "item": {"id": 42}
  }
  ```
- Empty
  ```json
  {
    "status": "not_found",
    "entity_type": "symbol",
    "count": 0
  }
  ```

注意点
- OutputStatus/EntityTypeは**snake_case**（例: "search_result"）
- UnifiedOutputは**Serializeのみ**（Deserializeなし）で、**untagged**によりバリアントタグは出力されません
- guidance/metadata/relationships/metadata内のHashMapは**空なら出力省略**（skip_serializing_if）

## Walkthrough & Data Flow

典型的フロー
- 検索結果（Vec<(T, f32)>）→ search_results_output
  - RankedItemへ変換（rank=1..n、score設定）
  - OutputMetadata.queryにCow::Borrowed(query)
  - UnifiedOutputBuilder::ranked → buildでcount/exit_code計算
- 影響分析（HashMap<SymbolKind, Vec<Symbol>>）→ impact_analysis_output
  - SymbolKindを文字列化してCow::Ownedキーに変換
  - UnifiedOutputBuilder::grouped → build
- シンボルリスト（Vec<Symbol>）→ symbol_list_output
  - UnifiedOutputBuilder::items → build

Mermaidフローチャート（Builderのcount/exit_code計算）

```mermaid
flowchart TD
  A[UnifiedOutputBuilder::build] --> B{data variant}
  B -->|Items| C[count = items.len()]
  B -->|Grouped| D[count = sum(groups.values().len())]
  B -->|Contextual| E[count = results.len()]
  B -->|Ranked| F[count = results.len()]
  B -->|Single| G[count = 1]
  B -->|Empty| H[count = 0]
  C --> I{status}
  D --> I
  E --> I
  F --> I
  G --> I
  H --> I
  I -->|Success| J[exit_code = ExitCode::Success]
  I -->|NotFound| K[exit_code = ExitCode::NotFound]
  I -->|PartialSuccess| L[exit_code = ExitCode::Success]
  I -->|Error| M[exit_code = ExitCode::GeneralError]
  J --> N[return UnifiedOutput]
  K --> N
  L --> N
  M --> N
```

上記の図は`UnifiedOutputBuilder::build`関数の主要分岐を示す（行番号: 不明）。

テキスト出力の流れ（fmt::Display）
- Items: 各itemを1行で出力
- Grouped: グループ名と件数を見出しにし、各itemをインデント
- Contextual: itemを出力後、contextが空でなければ「Context:」セクションにキー/値
- Ranked: rankがSomeなら「N. 」を前置、scoreを括弧で表示
- Single: itemを1行
- Empty: 「No results found」

## Complexity & Performance

- Builder.build
  - 時間: O(n)（各バリアントで要素数集計）
  - 空間: O(1)（count/exit_code算出のみ）
- search_results_output
  - 時間: O(n)（enumerate + map）
  - 空間: O(n)（RankedItemベクタ生成）
- impact_analysis_output
  - 時間: O(n + g)（シンボル合計n、グループ数gのキー生成）
  - 空間: O(n)（グループ化済み構造）
  - 注意: SymbolKind→String化で**割り当て**が発生（Cow::Owned）。ホットパスではキーを借用できるよう最適化余地あり。
- Display出力
  - 時間: O(n)
  - 空間: O(1)（ストリーム出力）

ボトルネック/スケール限界
- 大量のContext/metadataや深い関係（ItemRelationships）が付くと**シリアライズ負荷**が上昇
- GroupedキーのString化が多い場合に**割り当て頻度**が増える
- Rankedでscore/rankを全件出力するため、**I/O帯域**が律速になる可能性

実運用負荷要因
- 標準出力/ファイルへの書き込み（I/O）
- JSON生成（Serialize）
- 検索スコア計算は外部（このファイルでは受け取りのみ）

## Edge Cases, Bugs, and Security

エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空Items | items=[] | status=NotFound, count=0, exit=NotFound | Builder.items/build | 対応済 |
| 空Grouped | groups={} | status=NotFound, count=0 | Builder.grouped/build | 対応済 |
| 空Ranked | results=[] | status=NotFound, count=0 | Builder.ranked/build | 対応済 |
| 空Contextual | results=[] | status=NotFound, count=0 | Builder.contextual/build | 対応済 |
| Single | item=T | count=1 | build | 対応済 |
| Empty | OutputData::Empty | 「No results found」出力 | Display | 対応済 |
| Context空 | context.is_empty() | Contextセクション非表示 | Display | 対応済 |
| rankなし | rank=None | 前置番号非表示 | Display | 対応済 |

セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow: 該当なし（Rust安全型、unsafe未使用）
  - Use-after-free: 該当なし（所有権/借用に従う）
  - Integer overflow: count集計にusize使用。非常に大きな入力でのオーバーフローはプラットフォーム依存だが現実的には稀。必要ならsaturatingやu64への変更を検討。
- インジェクション
  - SQL/Command/Path traversal: 該当なし（このファイルでは外部コマンド/パス処理なし）
  - テキスト出力: ユーザ提供の文字列が含まれうるがCLI出力では**エスケープ不要**。JSON出力ではserdeが適切にエスケープ。
- 認証・認可
  - 該当なし（この層は表示専用）
- 秘密情報
  - Hard-coded secrets: 該当なし
  - Log leakage: Display/Serializeで**context/metadata**に敏感情報が含まれる可能性。取り扱いポリシーを上位レイヤで要管理。
- 並行性
  - Race condition/Deadlock: 該当なし（共有ミュータブル状態なし）
  - Send/Sync: 型のSend/Syncは**Tと'aに依存**。必要なら型境界（T: Send + Sync + Serialize）を導入。

潜在バグ/注意点
- OutputDataが**untagged**のため、Deserialize時に曖昧性が生じる可能性（ただしUnifiedOutputはDeserializeを派生していないため、現状は問題なし）
- impact_analysis_outputでグループキーをformat!で**Owned**化しており、「ゼロコスト」方針からは外れる。借用可能なら**Borrowed**利用を検討。

## Design & Architecture Suggestions

- データ契約の明確化
  - UnifiedOutputの**Serializeのみ**は良い設計。もし将来Deserializeが必要なら、untaggedの曖昧性解決のため**tagged enum**や**別ラッパ**を検討。
- Cowの利用指針
  - impact_analysis_outputのキーは**Cow::Borrowed**を使えるように上位層で文字列参照を提供する設計へ寄せると**割り当て削減**。
- 型境界の明確化
  - JSON出力前提のため、公開APIでTに**Serialize**境界を付ける（BuilderやUnifiedOutputにwhere T: Serializeを付ける）と誤用防止になる。
- ExitCodeの拡張
  - PartialSuccessをSuccessへマップしているが、必要に応じて**警告レベル**のExitCode追加を検討。
- IntoUnifiedOutputの実装ガイド
  - よく使うドメイン型（Symbolや検索結果型）に対する**IntoUnifiedOutput実装**を用意すると利便性向上。

## Testing Strategy (Unit/Integration) with Examples

既存テスト
```rust
#[test]
fn test_zero_cost_builder() {
    let symbols: Vec<Symbol> = vec![];
    let output = UnifiedOutputBuilder::items(symbols, EntityType::Symbol).build();

    assert_eq!(output.status, OutputStatus::NotFound);
    assert_eq!(output.count, 0);
    assert_eq!(output.exit_code, ExitCode::NotFound);
}

#[test]
fn test_ranked_output() {
    let results = vec![("item1", 0.95), ("item2", 0.85)];

    let output = search_results_output(results, "test query");
    assert_eq!(output.count, 2);
    assert_eq!(output.entity_type, EntityType::SearchResult);

    if let OutputData::Ranked { results } = output.data {
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].score, 0.95);
        assert_eq!(results[0].rank, Some(1));
    } else {
        panic!("Expected Ranked variant");
    }
}
```

追加提案
- Builder.count検証
  - Items/Grouped/Contextual/Ranked/Single/Emptyの**全バリアント**でcountとstatus→exit_codeの一致を検証
- Displayの整形テスト
  - Rankedで「N. item (score: sss)」のフォーマット、Groupedの見出し「group (len):」などの出力をスナップショットで確認
- Cowの借用/所有
  - guidance/metadata.queryでBorrowedとOwned双方を通すテスト（ライフタイムの健全性確認）
- メタデータ省略
  - skip_serializing_ifの挙動確認（空のHashMap/Noneがシリアライズされないこと）

例: Groupedバリアントのcount計算テスト
```rust
#[test]
fn test_grouped_count_and_status() {
    use std::borrow::Cow;
    let mut groups = std::collections::HashMap::new();
    groups.insert(Cow::Borrowed("A"), vec![1, 2]);
    groups.insert(Cow::Borrowed("B"), vec![]);

    let out = UnifiedOutputBuilder::grouped(groups, EntityType::Impact).build();
    assert_eq!(out.count, 2);
    assert_eq!(out.status, OutputStatus::Success);
}
```

## Refactoring Plan & Best Practices

- TにSerialize境界を追加
  - 例: `#[derive(Serialize)] pub struct UnifiedOutput<'a, T: Serialize> { ... }`（注: ジェネリック派生では自動でT: Serializeが要求されますが、Builder関数にも明示する）
- Cowの借用活用
  - impact_analysis_outputのキー生成を**Borrowed**優先に見直し、必要時のみOwned
- APIの拡張
  - `UnifiedOutputBuilder::single(item, entity_type)` の追加（Singleバリアントを直感的に構築）
  - `UnifiedOutputBuilder::empty(entity_type)` の追加
- Displayの拡張
  - guidanceやmetadataの出力オプション（CLIフラグで切り替え）を考慮
- IntoUnifiedOutputの実装テンプレート
  ```rust
  impl<'a> IntoUnifiedOutput<'a> for Symbol {
      fn into_unified(self, entity_type: EntityType) -> UnifiedOutput<'a, Self> {
          UnifiedOutputBuilder::items(vec![self], entity_type).build()
      }
  }
  ```

## Observability (Logging, Metrics, Tracing)

- 現状: ロギング/メトリクス/トレースは**未実装**
- 追加提案
  - OutputMetadataに**生成経路**や**シリアライズ時間**を記録（timing_ms既存フィールドの活用）
  - 生成時に**tracing**のイベントを発行（出力形、count、status）
  - truncationフラグの利用（大量出力を切り詰めた場合にtrue設定）

## Risks & Unknowns

- Unknowns
  - Symbol/ExitCode/EntityTypeの利用範囲（このチャンクには現れない）
  - 上位レイヤでの**巨大context/metadata**の可能性（パフォーマンス影響）
  - 具体的なCLI/MCPの**消費側契約**（このチャンクには現れない）
- Risks
  - untaggedのバリアント曖昧性（将来Deserializeが必要になった場合に問題化）
  - impact_analysis_outputのキーの**文字列割り当てコスト**
  - TがDisplay/Serializeを満たさない型での誤用（コンパイル時に検出されるが、APIドキュメントでの**明示**が望ましい）

## Edge Cases, Bugs, and Security（Rust特有の観点）

- メモリ安全性（所有権/借用/ライフタイム）
  - 所有権の移動: Builderコンストラクタ（items/grouped/ranked/contextual）で引数の所有権を**受け取り**、buildでUnifiedOutputへ**移動**（関数名: build, 行番号: 不明）
  - 借用: guidance/metadata.query/groupキーなどは**Cow::Borrowed**を通じて借用可能
  - ライフタイム: 'a は**Cowの借用側寿命**。出力の寿命が借用元を超えないように設計されている
- unsafe境界
  - 使用箇所: **なし**
  - 安全性根拠: 標準ライブラリ/serdeの安全なAPIのみ使用
- 並行性・非同期
  - Send/Sync: 型のSend/Syncは**T**と**'a**に依存。共有ミュータブル状態は保持していないためデータ競合なし
  - 非同期/await: **なし**
  - キャンセル: **なし**
- エラー設計
  - Result/Option: このモジュールは**表示用**であり、エラーは**OutputStatus**で表現
  - panic箇所: テスト内の`panic!("Expected Ranked variant")`のみ。ライブラリ本体では**panicなし**
  - エラー変換: **なし**（必要なら上位層でResultをUnifiedOutputへ変換するユーティリティを追加可能）

以上のとおり、本ファイルは**統一出力の中心的スキーマ**として、型安全・低コスト・拡張性のバランスが良い設計です。用途拡大時はSerialize境界の明示、Cowの借用最適化、Deserialize方針の整理を行うとさらに堅牢になります。