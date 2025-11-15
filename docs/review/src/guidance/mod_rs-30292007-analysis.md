# guidance/mod.rs Review

## TL;DR

- 本モジュールは、テンプレート置換に用いる文脈データを保持する構造体（TemplateContext）と、ガイダンス生成結果（GuidanceResult）を定義し、設定（GuidanceConfig）とエンジン（GuidanceEngine）を再輸出する公開エントリポイント。
- 公開APIは単純で、ビルダー風のメソッドチェーン（new → with_query → with_custom）で不可変オブジェクトを段階的に構築する方針。エラーは返さず、入力値の検証は行わない。
- 並行性・安全性は高い（unsafeなし、所有権で可変性を制御）。HashMapとStringによりSend/Syncは自動導出される可能性が高いが、明示はない。
- 重要なコントラクトは「has_results = result_count > 0」と「confidenceは0.0〜1.0想定（未強制）」。
- 既知の懸念: serde::Deserializeの未使用import、with_customのキー上書き挙動、confidence範囲未検証、GuidanceResultがSerialize未派生（外部I/Oが必要なら不足）。
- モジュール間の詳細（config/engine/templatesの内部）やI/O、テンプレート埋め込み仕様はこのチャンクには現れないため不明。

## Overview & Purpose

- 目的: マルチホップクエリ用の「案内（guidance）」生成において、テンプレートに流し込む文脈（ツール名・クエリ・結果件数・カスタム変数）を表現する構造体 TemplateContext と、生成結果を表現する GuidanceResult を提供する。
- 役割:
  - TemplateContext: テンプレート変数のコンテナ。ビルダー様式で段階的構築を支援。
  - GuidanceResult: ガイダンスメッセージとメタ（confidence, is_fallback）をまとめるDTO。
  - 再輸出: GuidanceConfig, GuidanceEngine を公開エイリアスとして提供し、利用者が guidance::GuidanceConfig / guidance::GuidanceEngine を介して設定・実行にアクセス可能にする。
- 適用範囲: テンプレート置換、ガイダンス生成の周辺。コアロジック（テンプレート評価やエンジン動作）は config/engine/templates にあり、このチャンクには現れない。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | config | pub mod | 設定定義（詳細は不明） | Low（不明） |
| Module | engine | pub mod | ガイダンス生成エンジン（詳細は不明） | Med（不明） |
| Module | templates | pub mod | テンプレート定義/適用（詳細は不明） | Med（不明） |
| Re-export | GuidanceConfig | pub use | 設定型を外部へ公開 | Low |
| Re-export | GuidanceEngine | pub use | エンジン型を外部へ公開 | Low |
| Struct | TemplateContext | pub | テンプレートに渡す文脈データ | Low |
| Impl fn | TemplateContext::new | pub | 初期化（tool, result_count） | Low |
| Impl fn | TemplateContext::with_query | pub | クエリの設定（Option<&str>） | Low |
| Impl fn | TemplateContext::with_custom | pub | カスタム変数の追加 | Low |
| Struct | GuidanceResult | pub | ガイダンス生成結果のDTO | Low |

- Dependencies & Interactions
  - 内部依存:
    - TemplateContextは標準ライブラリのHashMap<String, String>に依存。
    - serde::Serializeを派生（TemplateContext）。Deserializeはインポートされているが未使用。
  - 外部依存（使用クレート/モジュール）:

    | 依存 | 用途 | 備考 |
    |------|------|------|
    | std::collections::HashMap | カスタム変数の格納 | O(1)平均挿入 |
    | serde::Serialize | TemplateContextのシリアライズ | Deserializeは未使用 |
    | config, engine, templates | サブモジュール | 中身はこのチャンクには現れない |

  - 被依存推定（このモジュールを使う側）:
    - 上位のアプリケーション/サービス層がGuidanceEngineを通してガイダンス生成。
    - テンプレート適用処理がTemplateContextを入力として利用。
    - 設定ローダ/CLI/HTTPハンドラがGuidanceConfigとTemplateContextを組み立てる。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| GuidanceConfig (re-export) | pub use config::GuidanceConfig | 設定型の外部公開 | N/A | N/A |
| GuidanceEngine (re-export) | pub use engine::GuidanceEngine | エンジン型の外部公開 | N/A | N/A |
| TemplateContext::new | pub fn new(tool: &str, result_count: usize) -> Self | 初期化（has_resultsを自動計算） | O(1) | O(1) |
| TemplateContext::with_query | pub fn with_query(self, query: Option<&str>) -> Self | クエリ設定（所有権移動のビルダー） | O(|query|) | O(|query|) |
| TemplateContext::with_custom | pub fn with_custom(self, key: &str, value: &str) -> Self | カスタム変数の追加/上書き | 平均O(1)+O(|k|+|v|) | O(|k|+|v|) |
| GuidanceResult | pub struct GuidanceResult { pub message: String, pub confidence: f32, pub is_fallback: bool } | 生成結果DTO | N/A | N/A |

- TemplateContext（データ契約）
  - フィールド:
    - tool: String（必須）
    - query: Option<String>（任意）
    - result_count: usize（検索結果数など）
    - has_results: bool（result_count > 0の派生）
    - custom: HashMap<String, String>（任意の追加変数）
  - シリアライズ: Serialize派生済み。フォーマット（JSON/YAML等）はserdeの上位層に依存。

- GuidanceResult（データ契約）
  - フィールド:
    - message: String（生成メッセージ）
    - confidence: f32（0.0〜1.0の想定、強制なし）
    - is_fallback: bool（フォールバックかどうか）
  - シリアライズ: このチャンクではSerialize未派生（外部送信が必要なら拡張余地）。

各APIの詳細:

1) TemplateContext::new
- 目的と責務
  - 必須情報toolとresult_countから初期化し、has_resultsをresult_count > 0で算出。
- アルゴリズム
  - toolをString化
  - query=None
  - result_countを代入
  - has_results = result_count > 0
  - custom=HashMap::new()
- 引数

  | 引数 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | tool | &str | 必須 | 実行中ツール名 |
  | result_count | usize | 必須 | 結果件数 |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | Self | 新しいTemplateContext |

- 使用例
  ```rust
  use guidance::TemplateContext;

  let ctx = TemplateContext::new("search", 3);
  assert_eq!(ctx.tool, "search");
  assert!(ctx.has_results);
  ```
- エッジケース
  - result_count == 0の場合にhas_resultsがfalseになることの確認
  - toolが空文字でも許容（検証なし）

2) TemplateContext::with_query
- 目的と責務
  - クエリ文字列（Option<&str>）を設定。ビルダー連鎖に対応。
- アルゴリズム
  - query.map(String::from)で所有権を確保し代入
- 引数

  | 引数 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | query | Option<&str> | 任意 | クエリ文字列（Noneで未設定） |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | Self | 更新後のTemplateContext |

- 使用例
  ```rust
  let ctx = TemplateContext::new("search", 0)
      .with_query(Some("rust builder pattern"));
  assert_eq!(ctx.query.as_deref(), Some("rust builder pattern"));
  ```
- エッジケース
  - Some("")（空文字列）を許容
  - Noneを与えて未設定を保持

3) TemplateContext::with_custom
- 目的と責務
  - 任意のキー/値をcustomに追加。既存キーは上書き。ビルダー連鎖に対応。
- アルゴリズム
  - key/valueをStringにコピーしHashMapにinsert
- 引数

  | 引数 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | key | &str | 必須 | 変数名 |
  | value | &str | 必須 | 値 |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | Self | 更新後のTemplateContext |

- 使用例
  ```rust
  let ctx = TemplateContext::new("search", 2)
      .with_custom("language", "rust")
      .with_custom("scope", "crate");
  assert_eq!(ctx.custom.get("language"), Some(&"rust".to_string()));
  ```
- エッジケース
  - 同一キーで上書き（古い値は破棄）
  - 空キーや空値の挿入も許容（検証なし）

4) GuidanceResult
- 目的と責務
  - ガイダンス生成の出力をDTOとして保持。
- データ契約上の留意
  - confidenceは0.0〜1.0を想定するが、コードで強制していないため検証は呼び出し側に委ねられる。
- 使用例
  ```rust
  use guidance::GuidanceResult;

  let res = GuidanceResult {
      message: "Try broadening your query.".to_string(),
      confidence: 0.72,
      is_fallback: false,
  };
  ```

注: 行番号はこのチャンクには現れないため不明。

## Walkthrough & Data Flow

- 典型的な流れ
  1. 呼び出し側がTemplateContext::newで文脈を初期化（tool, result_count → has_resultsが導出）。
  2. 必要に応じてwith_queryでクエリを設定。
  3. 必要に応じてwith_customで追加変数を設定（複数回呼べる）。
  4. 上位のtemplates/engine側（不明）がTemplateContextを消費してテンプレート評価→GuidanceResultを生成。
- データの方向
  - TemplateContextは入力データの集約点。
  - GuidanceResultは出力データの集約点。
- スレッドセーフ性
  - ビルダーはselfを消費して返すため、途中でのデータ競合を避ける設計。
  - 完成後のTemplateContextは不変構造として共有しやすい。

本ファイル内の処理は単線的で条件分岐が少ないため、Mermaid図は非掲載（基準により省略）。

## Complexity & Performance

- TemplateContext::new
  - 時間: O(1)
  - 空間: O(1)
- TemplateContext::with_query
  - 時間: O(|query|)（文字列コピー）
  - 空間: O(|query|)（所有権確保）
- TemplateContext::with_custom
  - 時間: 平均O(1)（HashMap挿入） + 文字列コピー O(|key|+|value|)
  - 空間: O(|key|+|value|)（新規格納）
- スケール限界・ボトルネック
  - customに大量のエントリを追加するとメモリ消費と再ハッシュコストが増加。
  - 事前にエントリ数が見積もれるなら容量予約（HashMap::with_capacity）を検討。

## Edge Cases, Bugs, and Security

- エッジケース一覧

  | エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
  |-------------|--------|----------|------|------|
  | 結果ゼロ | result_count=0 | has_results=false | あり | OK |
  | 大きな結果数 | result_count=usize::MAX | has_results=true | あり | OK |
  | 空クエリ | Some("") | 空文字を受容 | あり | OK |
  | クエリ未設定 | None | query=None | あり | OK |
  | 空キーのcustom | key="" | 空キーを許容 | あり | 要検討 |
  | customの上書き | 同一keyで再設定 | 最新値で上書き | あり | 仕様確認要 |
  | 大量custom | 1e5個 | メモリ増加だが動作 | あり | パフォーマンス注意 |
  | 非ASCII文字 | 日本語/絵文字 | UTF-8として保持 | あり | OK |
  | confidence範囲外 | confidence=-0.1 or 1.5 | 許容されてしまう | なし | 要対策 |

- 既知/潜在バグ・改善点
  - 未使用import: serde::Deserialize がこのチャンクでは未使用。警告対象。
  - GuidanceResultのSerialize未派生: 外部I/O（JSONレスポンス等）に使うならSerialize/Deserialize派生を検討。
  - confidence値のバリデーションがない: 0.0..=1.0の範囲を型（newtype）やスマートコンストラクタで保証したい。
  - with_customの上書き: 上書きが意図か不明。誤上書きを避けるAPI（with_custom_if_absent等）も検討。
  - HashMap容量予約なし: 事前に数がわかるケースでは初期容量を指定できるAPIがあると再ハッシュを減らせる。

- セキュリティチェックリスト
  - メモリ安全性: unsafeなし、所有権に忠実。バッファオーバーフロー/Use-after-free/整数オーバーフローの懸念なし（標準型使用）。
  - インジェクション: 本ファイル単体では外部I/Oなし。テンプレート適用時のインジェクション（未エスケープ）はtemplates側の責務（不明）。
  - 認証・認可: 該当なし（このチャンクには現れない）。
  - 秘密情報: customに機密を入れた場合のログ漏洩リスクは上位層のログ方針次第。ここではログ出力なし。
  - 並行性: ビルダーが所有権移動で整合性を担保。グローバル可変状態なし → デッドロック/レースなし。

- Rust特有の観点（詳細）
  - 所有権/借用: with_*がselfを消費してSelfを返すため、可変借用の期間問題は回避。ライフタイム指定不要。
  - unsafe境界: なし。
  - Send/Sync: String/HashMapにより自動実装される可能性が高いが、本チャンクでは明示なし。型パラメータもないため一般には問題になりにくい。
  - 非同期/await: 非該当（同期コードのみ）。
  - エラー設計: Result/Optionのうち、Optionはフィールド（query）に使用。APIは失敗を返さないため、入力検証は上位で行う設計。

## Design & Architecture Suggestions

- 値検証の強化
  - confidenceを型安全に: newtype Confidence(f32)で0.0..=1.0のスマートコンストラクタを提供。
  - with_customで空キーを拒否/警告するAPIを用意、あるいはResultを返すバリアントを追加。
- ビルダー/初期容量
  - TemplateContextBuilderを導入し、with_capacityやwith_customs(IntoIterator)を提供。
  - TemplateContextにDefault実装（tool=""、result_count=0）を追加すると利便性向上。
- シリアライズ整合性
  - GuidanceResultにSerialize/Deserialize派生を検討（外部IFに出すなら必須）。
  - serde::Deserializeの未使用importを削除してクリーンに。
- APIの意図明確化
  - with_custom_if_absentやinsert_custom_returning_oldで上書きポリシーを明示。
  - has_resultsの定義をドキュメントで明確化（result_countにのみ依存する旨）。
- ドキュメントと例
  - 各メソッドにdoctest例を付与し、利用方法と挙動を保証。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト（examples）

  1) newの基本挙動
  ```rust
  #[test]
  fn new_sets_has_results_correctly() {
      let z = guidance::TemplateContext::new("search", 0);
      assert_eq!(z.has_results, false);

      let p = guidance::TemplateContext::new("search", 5);
      assert_eq!(p.has_results, true);
  }
  ```

  2) with_queryの設定
  ```rust
  #[test]
  fn with_query_sets_and_overwrites() {
      let ctx = guidance::TemplateContext::new("s", 1)
          .with_query(Some("first"))
          .with_query(Some("second"));
      assert_eq!(ctx.query.as_deref(), Some("second"));

      let ctx2 = guidance::TemplateContext::new("s", 1).with_query(None);
      assert!(ctx2.query.is_none());
  }
  ```

  3) with_customの上書き
  ```rust
  #[test]
  fn with_custom_inserts_and_overwrites() {
      let ctx = guidance::TemplateContext::new("s", 1)
          .with_custom("k", "v1")
          .with_custom("k", "v2");
      assert_eq!(ctx.custom.get("k").map(String::as_str), Some("v2"));
  }
  ```

  4) Unicode/空文字の扱い
  ```rust
  #[test]
  fn unicode_and_empty_values() {
      let ctx = guidance::TemplateContext::new("🔍", 0)
          .with_query(Some("日本語"))
          .with_custom("", "");
      assert_eq!(ctx.tool, "🔍");
      assert_eq!(ctx.query.as_deref(), Some("日本語"));
      assert_eq!(ctx.custom.get("").map(String::as_str), Some(""));
  }
  ```

- プロパティテスト
  - with_customのキー重複時に必ず最後に追加した値が残る。
  - result_count > 0 ⇔ has_results == true の恒等をチェック。

- 統合テスト
  - engine/templates側と連携し、TemplateContextからテンプレート文字列へ正常に置換されるか（このチャンクには現れないため詳細不明）。

## Refactoring Plan & Best Practices

- 逐次的改善
  - 未使用のserde::Deserialize import削除。
  - GuidanceResultにSerialize/Deserializeの派生追加（要件次第）。
  - TemplateContextにDefault実装とfrom_parts系コンストラクタを提供。
  - with_customs<I: IntoIterator<Item=(K,V)>>で一括挿入。HashMap::with_capacityを内部で使用可能に。
  - confidenceを型で拘束（Confidence::new(f32) -> Result<Confidence, Error>）。
- API整備
  - 上書き方針のメソッド分離（insert/insert_if_absent/merge）。
  - エラー可能APIはResultを返し、失敗理由を型で表現。
- ドキュメンテーション
  - has_resultsの定義、confidenceのスケール、customのキー命名規約を明確化。

## Observability (Logging, Metrics, Tracing)

- このチャンクには観測コードは現れない。上位層での推奨:
  - ログ: GuidanceEngineが生成したGuidanceResultのis_fallbackやconfidenceの分布をサンプリングして記録（PII/機密はマスク）。
  - メトリクス: guidance_generation_total、fallback_rate、avg_confidence、custom_variables_count_histogram。
  - トレーシング: テンプレート選択→埋め込み→出力までのspanを関連付け。TemplateContextのサイズやキー数をtag化（匿名化）。

## Risks & Unknowns

- 不明点（このチャンクには現れない）
  - config/engine/templatesの内部仕様、テンプレート言語、エスケープ戦略、I/O（DB/ネットワーク）の有無。
  - GuidanceResultが外部IF（HTTP/CLI/ファイル）に渡るかどうか。
- リスク
  - confidence未検証による下流ロジックの誤判定。
  - custom上書きによる意図しないテンプレート変数の汚染。
  - 大量customによるヒープ使用量増加。
- 緩和策
  - 型での制約/バリデーション導入、API分離（上書き/非上書き）、容量予約・上限設定。