## integration\test_gdscript_mcp.rs Review

## TL;DR

- 🔍 目的: GDScript用のコードインテリジェンス（semantic search＋impact analysis）が end-to-end で動作するかを検証する統合テスト。
- 📦 主要API: **SimpleIndexer**（インデクシング＋埋め込み有効化）、**CodeIntelligenceServer::semantic_search_with_context**、**CodeIntelligenceServer::analyze_impact**。
- 🧩 複雑箇所: semantic searchの結果から文字列パースで**symbol_id**を抽出するロジック（構造化されておらず脆い）。
- ⚠️ 重大リスク: 86MBの埋め込みモデルダウンロード依存、ネットワーク・IOに起因する不安定性、非UTF-8パスでの`to_str().expect`パニック。
- 🧪 エラー設計: `expect`によるパニックベースのハンドリング（テストでは妥当だが、実運用コードでは非推奨）。
- 🚦 並行性: `#[tokio::test(flavor = "current_thread")]`で単一スレッドRuntime。共有状態はほぼ無く、**Arc**は設定共有のために使用。

## Overview & Purpose

このファイルは、GDScriptのサンプルコード（fixtures）をテンポラリワークスペースに展開し、**codanna**のインデクサーでインデックス化した後、**CodeIntelligenceServer**の2つの機能をE2Eで検証します。

- 検証対象機能:
  - **semantic_search_with_context**: クエリ「apply damage」で関連コード断片とコンテキスト（テキスト）を取得。
  - **analyze_impact**: semantic search結果内に露出される`symbol_id`を使い、影響分析を実行。

このテストはCI/CDには不適切なため`#[ignore]`指定されています（埋め込みモデルのダウンロードが必要）。ローカルで`cargo test -- --ignored`により手動実行を想定。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Const | PLAYER_FIXTURE | private | GDScriptプレイヤーのfixture文字列を埋め込み | Low |
| Const | ENEMY_FIXTURE | private | GDScript敵キャラのfixture文字列を埋め込み | Low |
| Const | HEAL_EFFECT_FIXTURE | private | GDScript回復エフェクトのfixture文字列を埋め込み | Low |
| Function | test_gdscript_semantic_search_and_analyze_impact | private (test) | ワークスペース構築、インデックス化、semantic search、impact分析、アサーション | Med |
| Struct(外部) | Settings | external | ワークスペース／インデックスパス／semantic設定の構成 | Low |
| Struct(外部) | SemanticSearchConfig | external | semantic searchのオン/オフその他設定 | Low |
| Struct(外部) | SimpleIndexer | external | ファイルインデックス化と semantic search 有効化 | Med |
| Struct(外部) | CodeIntelligenceServer | external | semantic search と impact analysis のサーバ | Med |
| Struct(外部) | SemanticSearchWithContextRequest | external | semantic search 入力（query, limit, threshold, lang） | Low |
| Struct(外部) | AnalyzeImpactRequest | external | 影響分析入力（symbol_name or symbol_id, max_depth） | Low |
| Enum(外部) | RawContent | external | レスポンスコンテンツ（Text等） | Low |
| Struct(外部) | Parameters<T> | external | リクエストラッパ | Low |

### Dependencies & Interactions

- 内部依存
  - 単一のテスト関数が、設定生成→インデックス化→サーバ操作→結果パース→アサーションまでを直列処理。
  - **Arc<Settings>**を複数箇所で共有（インデクサーに渡す）。

- 外部依存（推定）
  | クレート/モジュール | 用途 |
  |---------------------|------|
  | codanna::SimpleIndexer | ファイルのインデックス化、semantic search有効化 |
  | codanna::config::{SemanticSearchConfig, Settings} | 設定オブジェクト |
  | codanna::mcp::{CodeIntelligenceServer, SemanticSearchWithContextRequest, AnalyzeImpactRequest} | サーバとMCP（Model Context Protocol）関連リクエスト |
  | rmcp::handler::server::wrapper::Parameters | リクエストのラッパ |
  | rmcp::model::RawContent | レスポンスコンテンツの型 |
  | tempfile::TempDir | 一時ディレクトリ生成 |
  | tokio::test | 非同期テスト実行 |

- 被依存推定
  - このモジュールは統合テスト専用であり、他モジュールからの利用は「テスト実行時のみ」。本番コードからの直接依存は「不明／該当なし」。

## API Surface (Public/Exported) and Data Contracts

このファイル自身に公開APIはありません（テスト関数のみ）。ここでは「このテストで利用している外部API」を列挙します。シグネチャは使用状況からの推定であり、正確な型は「不明」とします。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| test_gdscript_semantic_search_and_analyze_impact | async fn () | 統合テスト本体 | 不明 | 不明 |
| SimpleIndexer::with_settings | fn (Arc<Settings>) -> SimpleIndexer | 設定注入でインデクサ生成 | O(1) | O(1) |
| SimpleIndexer::enable_semantic_search | fn () -> Result<(), E> | 埋め込みモデル等の準備と有効化 | モデルロード依存（不明） | モデルサイズ依存 |
| SimpleIndexer::index_file | fn (&str) -> Result<(), E> | 対象ファイルのインデクス化 | O(n)（n=ファイル長＋前処理） | O(1)〜O(n) |
| CodeIntelligenceServer::new | fn (SimpleIndexer) -> CodeIntelligenceServer | インデクサを用いてサーバ生成 | O(1) | O(1) |
| CodeIntelligenceServer::semantic_search_with_context | async fn (Parameters<SemanticSearchWithContextRequest>) -> Result<Response, E> | 语義検索＋コンテキスト付与 | 不明（通常はO(N·d)） | 不明 |
| CodeIntelligenceServer::analyze_impact | async fn (Parameters<AnalyzeImpactRequest>) -> Result<Response, E> | 指定シンボルの影響範囲分析 | 不明 | 不明 |
| RawContent::Text | variant carrying TextBlock { text: String } | レスポンス内テキスト抽出 | O(テキスト長) | O(テキスト長) |

詳細（主要API）

1) CodeIntelligenceServer::semantic_search_with_context
- 目的と責務
  - 入力クエリ（ここでは「apply damage」）に対して、関連コードスニペットと周辺コンテキストを返す。
- アルゴリズム（推定）
  - インデックス済みドキュメント群から埋め込みベクトル生成済みのコンテンツに対し、クエリベクトルとの類似度でランキング→上位k件のコンテキスト抽出→レスポンス化。
- 引数
  | 名前 | 型 | 意味 |
  |------|----|------|
  | Parameters | Parameters<SemanticSearchWithContextRequest> | リクエストラップ |
  | request.query | String | 検索クエリ |
  | request.limit | u32（推定） | 上位件数 |
  | request.threshold | Option<f32>（推定） | 類似度しきい値 |
  | request.lang | Option<String> | 言語ヒント（ここでは "gdscript"） |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | Result<Response, E> | 成功時はコンテンツ配列などを含むレスポンス |
- 使用例
  ```rust
  let semantic_result = server
      .semantic_search_with_context(Parameters(SemanticSearchWithContextRequest {
          query: "apply damage".to_string(),
          limit: 1,
          threshold: None,
          lang: Some("gdscript".to_string()),
      }))
      .await
      .expect("semantic_search_with_context should succeed");
  ```
- エッジケース
  - threshold未指定でノイズが増える可能性
  - langヒントが一致しないとスコア悪化
  - コンテンツがText以外の場合の扱い（このテストでは除外）

2) CodeIntelligenceServer::analyze_impact
- 目的と責務
  - シンボル（名前またはID）を起点に、影響を受けるシンボルや参照範囲を分析。
- アルゴリズム（推定）
  - 参照グラフ／呼び出しグラフを辿り、`max_depth`まで到達ノードを収集してレポート化。
- 引数
  | 名前 | 型 | 意味 |
  |------|----|------|
  | Parameters | Parameters<AnalyzeImpactRequest> | リクエストラップ |
  | request.symbol_name | Option<String> | 名前指定（今回はNone） |
  | request.symbol_id | Option<u32> | ID指定（今回はSome） |
  | request.max_depth | u32 | 解析深さ |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | Result<Response, E> | 影響範囲のテキスト等 |
- 使用例
  ```rust
  let impact_result = server
      .analyze_impact(Parameters(AnalyzeImpactRequest {
          symbol_name: None,
          symbol_id: Some(apply_damage_symbol_id),
          max_depth: 2,
      }))
      .await
      .expect("analyze_impact should succeed");
  ```
- エッジケース
  - `symbol_id`が不正・未知の場合は「No symbols would be impacted」などの結果
  - `max_depth`が大きすぎると計算コスト増

3) RawContent::Text 抽出
- 目的と責務
  - レスポンスからテキストのみ抽出して結合。
- 使用例（抜粋）
  ```rust
  let semantic_text = semantic_result
      .content
      .iter()
      .filter_map(|content| match &content.raw {
          RawContent::Text(block) => Some(block.text.as_str()),
          _ => None,
      })
      .collect::<Vec<_>>()
      .join("\n");
  ```
- エッジケース
  - Text以外のRawContentが多いと出力が空になる

4) test関数内のsymbol_id抽出（文字列パース）
- 目的と責務
  - semantic出力中の`[symbol_id:NNN]`に従いIDを抽出。
- 使用例（抜粋）
  ```rust
  let apply_damage_symbol_id = semantic_text
      .split("[symbol_id:")
      .nth(1)
      .and_then(|rest| rest.split(']').next())
      .and_then(|digits| digits.parse::<u32>().ok())
      .expect("semantic output should expose symbol_id for apply_damage");
  ```
- エッジケース
  - 出力フォーマット変更時に抽出失敗→panic

データコントラクト（このテストで前提にしている項目）
- SemanticSearchWithContextRequest: { query: String, limit: u32（推定）, threshold: Option<f32>（推定）, lang: Option<String> }
- AnalyzeImpactRequest: { symbol_name: Option<String>, symbol_id: Option<u32>, max_depth: u32 }
- Response.content: `Iterator<Item = { raw: RawContent }>`（正確な型は不明）
- RawContent::Text: `TextBlock { text: String }`（推定）

## Walkthrough & Data Flow

1. TempDirで一時ワークスペースを作成。
2. GDScript fixtures（player, enemy, heal_effect）を階層構造で書き出し。
3. `.codanna-index`ディレクトリ作成。
4. **Settings**を構築し、`semantic_search.enabled = true`。
5. **Arc<Settings>**を**SimpleIndexer**へ渡して生成。
6. `enable_semantic_search()`で埋め込みモデルをロード（ダウンロードが必要・重い）。
7. 対象3ファイルを`index_file(&str)`でインデックス化。
8. **CodeIntelligenceServer**を`new(indexer)`で生成（indexerはムーブされる）。
9. `semantic_search_with_context`実行→レスポンスから`RawContent::Text`だけ抽出し結合。
10. 期待語「apply_damage」含有をアサート。
11. 文字列内から`[symbol_id:NNN]`パターンでID抽出。
12. `analyze_impact`を`symbol_id`指定で実行。
13. レスポンスからText抽出＋結合。
14. 「apply_damage」または「No symbols would be impacted」を含むことをアサート。

並行性・非同期
- テストは`current_thread`フレーバーで実行されるためスレッド間スケジューリングは無し。
- `.await`ポイントは`semantic_search_with_context`と`analyze_impact`の2箇所。
- 共有状態は**Arc<Settings>**のみ。インデクサはサーバ生成時にムーブされ、その後はサーバを通してのみ利用。

## Complexity & Performance

- インデックス化（`index_file`）: 時間 O(n)（n=ファイルサイズ＋解析コスト）、空間 O(1)〜O(n)。
- semantic search: 一般的に、コーパスサイズN、埋め込み次元dの場合、検索は O(N·d)（近似探索であればより小さい）。本実装の詳細は不明。
- impact analysis: グラフ探索で O(E+V)×深さ（`max_depth`）。詳細は不明。
- ボトルネック
  - 初回モデルダウンロード（86MB）とロード時間。
  - 埋め込み生成の計算コスト。
  - ファイルIO（fixture書き出し、インデックス保存）。
- スケール限界
  - ワークスペースが巨大になるほどインデックス時間・検索時間が増加。
  - 単一スレッドRuntimeのためCPU並列は活用されない。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価
- メモリ安全性
  - Rust安全：`unsafe`使用なし、所有権はコンパイラ管理。Arc共有は読み取りのみで安全。
  - Integer overflow: `u32`への`parse`で発生しない（失敗時はNone→`expect`でpanic）。
- インジェクション
  - SQL/Command/Path traversal: ユーザー入力無し。固定fixtureのみ。脅威は低。
- 認証・認可
  - 該当なし。テストコードで外部サービス認証無し。
- 秘密情報
  - ハードコード秘密情報なし。ログも出力しないため漏洩リスク低。
- 並行性
  - current_threadでデータ競合・デッドロックの可能性は極小。共有状態はArc設定のみ。

詳細エッジケース表

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 非UTF-8パス | OS側で非UTF-8のTempDir | インデックス化時も安全に処理 | `to_str().expect("utf8 path")`でpanic | 要改善 |
| モデル未取得/ダウンロード失敗 | オフライン | gracefullyな失敗報告 | `enable_semantic_search().expect(...)`でpanic | 要改善 |
| semantic結果がText以外 | RawContent::Image等（仮） | テキスト以外も扱うか明示 | Text以外を除外しjoin→空文字の可能性 | 許容/改善検討 |
| 期待語が出ない | クエリが不一致 | テスト失敗 | `assert!(semantic_text.contains(...))`で失敗 | 想定通り |
| symbol_idフォーマット変更 | 出力が`[symbol_id:...]`でない | フォールバック・明示エラー | 文字列パース失敗→panic | 要改善 |
| 影響なし | 孤立シンボル | 「No symbols would be impacted」等の文言 | その文言も許容してassert | 想定通り |
| ディレクトリ作成失敗 | 権限不足 | 明示的なエラー | `expect`でpanic | テストとして許容 |

Rust特有の観点（このチャンクに現れた範囲）
- 所有権: `indexer`は`CodeIntelligenceServer::new(indexer)`でムーブされる（行番号はこのチャンクには含まれないため不明）。以後`indexer`は未使用。
- 借用: `std::fs::write(&full_path, contents)`で一時借用。問題なし。
- ライフタイム: 明示ライフタイム不要。`Arc<Settings>`はテストスコープ内で有効。
- unsafe境界: 使用なし。
- Send/Sync: `Arc<Settings>`は`Send + Sync`前提だが、current_threadで並列利用無し。
- await境界: 2箇所（semantic/impact）。途中でキャンセル処理は無し。
- エラー設計: ほぼ`expect`／`assert!`で早期パニック。テストとして妥当だが実運用のエラー伝播には不適。

## Design & Architecture Suggestions

- 出力の構造化
  - semantic searchの結果から**symbol_id**を文字列パースせず、明示的なフィールドとして返すデータ構造への変更を推奨（例: `SearchHit { symbol_id: u32, snippet: String, ... }`）。
- エラー処理
  - テスト以外のコードでは`anyhow`や`thiserror`でコンテキスト付与した`Result`を返す設計に。
  - モデルダウンロード失敗時のリトライ/キャッシュパス設定を許容。
- 路径/文字コード
  - `OsStr`ベースでパスを扱い、非UTF-8でも安全に`index_file`へ渡せるAPIへ。
- テスト安定性
  - 結果のフォーマットに依存しないアサーション（symbol_idは構造化、テキストは正規化）で脆さ回避。
  - しきい値やseedを設定できるなら固定して結果安定化。
- API設計
  - `Parameters<T>`のラップ意義を明確化。不要ならシンプルな関数シグネチャへ。
  - Impact分析結果も構造化（影響シンボル配列＋テキスト要約）で機械可読性を向上。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト提案
  - symbol_id抽出ロジックを分離し、フォーマット変更に強いテストを追加。
  - RawContentのバリアントごとのフィルタリング関数をテスト。

- 統合テスト提案
  - 小さなローカルモデルやモックを用意し、CIでも実行可能な軽量版テストを用意。
  - 設定を外部化して、モデルパスやしきい値を環境変数で制御。

- 例: テキスト抽出ユーティリティ（擬似コード）
  ```rust
  fn extract_texts(contents: &[Content]) -> String {
      contents
          .iter()
          .filter_map(|c| match &c.raw {
              RawContent::Text(tb) => Some(tb.text.as_str()),
              _ => None,
          })
          .collect::<Vec<_>>()
          .join("\n")
  }

  #[test]
  fn test_extract_texts() {
      // Content/RawContentはテスト用のダミー型/値を仮定
      let contents = vec![
          Content { raw: RawContent::Text(TextBlock { text: "hello".into() }) },
          Content { raw: RawContent::Binary(vec![1,2,3]) }, /* ... 省略 ... */
      ];
      assert_eq!(extract_texts(&contents), "hello");
  }
  ```

- 例: symbol_id抽出の安全版（出力が構造化された場合）
  ```rust
  // 望ましいAPI（仮）
  struct SearchHit {
      symbol_id: u32,
      snippet: String,
      /* ... 省略 ... */
  }

  fn pick_apply_damage_id(hits: &[SearchHit]) -> Option<u32> {
      hits.iter()
          .find(|h| h.snippet.contains("apply_damage"))
          .map(|h| h.symbol_id)
  }
  ```

## Refactoring Plan & Best Practices

- 関数分割
  - フィクスチャ書き出し、設定生成、インデックス化、検索、影響分析、アサーションを小関数へ分離。
- エラー取り扱い
  - `expect`の乱用を減らし、`anyhow::Result`で呼び出し元にエラー伝播。
- パス取り扱い
  - `Path`/`OsStr`を通しUTF-8前提を撤廃。`index_file`が`&Path`を受けるAPIならそれを使用。
- テスト安定化
  - モデルダウンロードの事前準備（キャッシュ）や、ネットワーク依存除去。
  - しきい値・seedの固定化、言語ヒントの厳密指定。
- 可読性
  - 文字列パースより**型安全**な抽出に移行。

## Observability (Logging, Metrics, Tracing)

- ログ
  - モデルロード開始/完了、インデックス件数、検索クエリ、結果件数、impact解析深さなどを`tracing`でINFO/DEBUGログ化。
- メトリクス
  - インデックス時間、検索時間、impact時間、ダウンロードサイズをメトリクスに。
- トレーシング
  - `semantic_search_with_context`と`analyze_impact`をspanで囲み、外部IOや計算の内部イベントを記録。

## Risks & Unknowns

- 実装詳細の不明点
  - **codanna**のインデクシング/検索アルゴリズム、`Response`の正確な型、`RawContent`バリアントはこのチャンクには現れないため不明。
- 外部依存リスク
  - 大容量モデルダウンロードに依存、ネットワークエラーやレート制限。
- 非決定性
  - 埋め込みモデル更新や類似度計算の実装差異で結果が揺れる可能性。
- パフォーマンス
  - 大規模ワークスペースでのスケール挙動は未確認。
- 互換性
  - Windows等の非UTF-8ファイルシステムパスでの`to_str().expect`は脆弱。

以上の内容は、当該ファイルのテストコードから読み取れる範囲での分析です。正確な行番号や完全な型情報は「このチャンクには現れない」ため、必要に応じて実装側ソース（codanna/rmcp）を参照してください。