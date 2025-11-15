# parsers\gdscript\test_relationships.rs Review

## TL;DR

- 目的: GDScript用パーサの関係抽出API（find_calls, find_uses）が、実際のGDScriptコード例（fixtures/player.gd）から期待通りの関係を検出できるかをテストする。
- 公開API: このファイル自体の公開APIはなし。外部APIとして GdscriptParser::new, find_calls, find_uses を利用。
- 複雑箇所: コール抽出ではシグナル検出（直接emit vs シグナル名の扱い）をOR条件で許容するなど、検出仕様の曖昧性を考慮。
- 重大リスク: assert!のメッセージでフォーマット引数を渡しておらず、"{calls:?}"や"{uses:?}"がそのまま文字列として表示される（デバッグ不能）。修正推奨。
- Rust安全性: include_str!で静的文字列を安全に取得。unsafeなし。並行性なし。期待するエラーはexpect()で明確化。
- パフォーマンス: テスト側は線形走査（iter().any()）を複数回。主なコストは外部パーサ実装側に依存。
- 不明点: タプル第3要素の意味（メタデータ）は未使用で不明。find_calls/find_usesの正確なシグネチャとアルゴリズムはこのチャンクには現れない。

## Overview & Purpose

このファイルは、codannaプロジェクトのGDScriptパーサ（GdscriptParser）が関係抽出（関数呼び出しやシーン・スクリプトの利用関係）を正しく行うかを検証するテスト群を提供する。固定のGDScriptフィクスチャ（player.gd）を読み込み、find_callsとfind_usesの結果を走査して、以下を確認する。

- 関数呼び出しの検出例（_ready → spawn_enemy、apply_damage → _reset、_reset → add_child）。
- シグナル発火の検出例（apply_damage が health_changed を発火、または emit_signal 経由）。
- 利用関係の検出例（スクリプトが CharacterBody2D を extends、定数 EnemyScene の preload、_reset 内の preload）。

このファイル自身は公開APIを持たず、テスト関数のみを含む。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | load_fixture | private | GDScriptフィクスチャ文字列を静的に読み込む | Low |
| Test Function | test_gdscript_find_calls_captures_signals_and_scene_calls | private (#[test]) | find_callsの結果から関数呼び出し・シグナル発火検出を確認 | Med |
| Test Function | test_gdscript_find_uses_detects_extends_and_preloads | private (#[test]) | find_usesの結果からextends・preload検出を確認 | Med |
| Const | SCRIPT_SCOPE | private | スクリプト全体スコープを表す特別なソース識別子 | Low |

### Dependencies & Interactions

- 内部依存
  - テスト関数 → load_fixture（フィクスチャ取得）
  - テスト関数 → GdscriptParser::new, find_calls, find_uses（外部API呼び出し）

- 外部依存（表）
  | 依存 | シンボル/機能 | 用途 |
  |------|---------------|------|
  | codanna::parsing::LanguageParser | トレイト | GdscriptParserが実装していると推定（このチャンクには現れない） |
  | codanna::parsing::gdscript::GdscriptParser | 型 | GDScriptコード解析・関係抽出 |
  | Rust標準 | include_str! | コンパイル時にファイル内容を &'static str として取り込む |
  | Rust標準 | #[test], assert!, expect | テスト・アサーション・エラーハンドリング |

- 被依存推定
  - プロジェクトのCIや開発者ローカルでのテスト実行時に使用。
  - GDScriptパーサの実装変更時の回帰検証として機能。

## API Surface (Public/Exported) and Data Contracts

このファイル自体の公開APIは存在しない（テスト専用）。以下は、間接的に利用している外部APIの一覧（詳細はこのチャンクには現れないため不明）。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| GdscriptParser::new | 不明（Result推定） | パーサの生成 | 不明 | 不明 |
| LanguageParser::find_calls | 不明（&str入力、Vec推定） | 関数呼び出し・シグナル等のエッジ抽出 | 不明 | 不明 |
| LanguageParser::find_uses | 不明（&str入力、Vec推定） | extendsやpreload等の利用関係抽出 | 不明 | 不明 |

データ契約（テストから推測できる範囲のみ、正確な仕様は不明）:
- find_calls の戻り値はイテラブルなコレクションで、各要素が3要素タプル（caller, callee, メタ情報）である。caller/calleeは文字列（&StringまたはString）で、3番目は未使用（不明）。
- find_uses の戻り値も同様に3要素タプル（source, target, メタ情報）で、source/targetは文字列。sourceにSCRIPT_SCOPE（"<script>"）が現れることがある。

各APIの詳細説明（このファイルの公開APIはないため、該当なし）。

## Walkthrough & Data Flow

- load_fixture（フィクスチャ読込）
  - include_str!("../../fixtures/gdscript/player.gd") により、GDScriptサンプルコードを &'static str で取得。
  - 役割: テストの入力ソースを提供。

- test_gdscript_find_calls_captures_signals_and_scene_calls
  1. code = load_fixture()
  2. parser = GdscriptParser::new().expect("...")（生成失敗でpanic）
  3. calls = parser.find_calls(code)
  4. calls.iter().any(...) を複数回使って期待関係を検証
     - _ready → spawn_enemy
     - apply_damage → health_changed もしくは emit_signal（OR条件で許容）
     - apply_damage → _reset
     - _reset → add_child

- test_gdscript_find_uses_detects_extends_and_preloads
  1. code = load_fixture()
  2. parser = GdscriptParser::new().expect("...")（生成失敗でpanic）
  3. uses = parser.find_uses(code)
  4. uses.iter().any(...) を用いて以下を検証
     - <script> → CharacterBody2D（extends）
     - EnemyScene → res://enemies/enemy.gd（preload）
     - _reset → res://effects/heal_effect.gd（preload）

データフローの要点:
- 入力: &'static str のGDScriptコード
- 処理: GdscriptParser による解析（外部）、結果はVec等のコレクション
- 検証: iter().any(...) による線形走査で該当エッジの存在確認
- 出力: テストの成否（panic/成功）

## Complexity & Performance

- テスト側の計算量
  - calls検証: iter().any(...) を4回行うため、O(4|calls|) = O(|calls|)
  - uses検証: iter().any(...) を3回行うため、O(3|uses|) = O(|uses|)
  - 空間計算量: callsとusesの格納分でO(|calls| + |uses|)
- 実運用負荷要因
  - 主なコストはGdscriptParserの解析処理（字句解析・構文解析・関係抽出）に依存。このチャンクには現れないため詳細不明。
  - テストはI/Oゼロ（include_str!によりビルド時取り込み）、ネットワーク・DBアクセスなし。

## Edge Cases, Bugs, and Security

- バグ
  - アサートメッセージのフォーマット指定が機能していない
    - 例: "expected _ready to call spawn_enemy, got {calls:?}" としているが、第2引数のみで追加のフォーマット引数を渡していないため、{calls:?}がそのまま文字列として出力される。
    - 修正例:
      ```rust
      assert!(
          calls.iter().any(|(caller, callee, _)| *caller == "_ready" && *callee == "spawn_enemy"),
          "expected _ready to call spawn_enemy, got {:?}",
          calls
      );
      ```
    - 同様の修正を他のassert!にも適用推奨。
  - タプル第3要素の意味が不明で未検証
    - 第3要素（メタ情報）が正しく設定・解釈されているか検証していないため、仕様変更時にテストの網羅性が不足する可能性。

- エッジケース（テスト観点）
  | エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
  |-------------|--------|----------|------|------|
  | アサートメッセージのフォーマット | 既存フィクスチャ | 失敗時にcalls/usesの内容がデバッグ出力される | フォーマット引数未指定 | NG |
  | パーサ生成失敗 | GdscriptParser::new()がErr | panic（expectのメッセージ出力） | expect使用 | OK |
  | フィクスチャ欠落 | player.gd不在 | ビルド時に失敗（include_str!） | include_str! | OK |
  | シグナル名の扱い | emit_signal("health_changed") | health_changed または emit_signal として検出 | OR条件で許容 | OK（仕様依存） |
  | 余分な関係の誤検出 | ノイズのあるGDScript | 誤検出がないこと | 未テスト | 不明 |
  | 位置情報の有無 | 第3要素が位置/種別 | 一貫したメタ情報 | 未検証 | 不明 |

- セキュリティチェックリスト
  - メモリ安全性: unsafeなし。include_str!による &'static str は安全。バッファオーバーフロー・Use-after-free・整数オーバーフローの懸念なし。
  - インジェクション: 外部入力なし。SQL/Command/Path traversalの懸念なし。
  - 認証・認可: 該当なし（テストコード）。
  - 秘密情報: ハードコード秘密なし。ログへの秘密漏洩なし。
  - 並行性: スレッド・非同期なし。レースコンディション・デッドロックなし。

- Rust特有の観点
  - 所有権/借用: include_str!で &'static str を返す設計は適切。calls/usesは所有するコレクション（Vec推定）で、iter()により不変借用の範囲で安全に参照。
  - ライフタイム: 明示的ライフタイム指定不要。'static文字列の利用で安全。
  - unsafe境界: 使用なし。
  - 並行性/非同期: Send/Sync要件に関与せず。await境界なし。キャンセルも該当なし。
  - エラー設計: new().expect(...) により生成失敗は即panicに変換。テストでは妥当。unwrap/expectの使用は許容。

## Design & Architecture Suggestions

- アサーションのDRY化
  - 共通の関係検証ヘルパー関数を導入すると可読性・保守性が向上。
    ```rust
    fn assert_has_call(calls: &[(String, String, /*...*/)], caller: &str, callee: &str) {
        assert!(
            calls.iter().any(|(c, a, _)| c == caller && a == callee),
            "expected {} to call {}, got {:?}",
            caller, callee, calls
        );
    }
    ```
- メタ情報の型定義
  - タプル第3要素が位置情報や種別（CallKind/UseKindなど）なら、列挙型や専用構造体にすると意味が明確になり、テストでも厳密に検証可能。
- SCRIPT_SCOPEの共通化
  - "<script>"の約束はパーサ側で定数公開（例: LanguageParser::SCRIPT_SCOPE）されていると、テストと実装の不整合を防げる。
- スナップショットテストの導入
  - calls/uses全体を比較するスナップショットを追加すると、仕様変更時の差分把握が容易（例: instaクレートの利用。導入の妥当性はプロジェクト方針次第）。

## Testing Strategy (Unit/Integration) with Examples

- 既存テストの強化
  - フォーマット修正でデバッグ容易化。
  - ネガティブケース（特定の関係が存在しないこと）を追加。
  - メタ情報（第3要素）を検証するテスト（位置情報、種別など）。不明であれば「存在すること」だけでなく「規格に一致すること」を主張。

- 例: コール関係のヘルパー利用
  ```rust
  #[test]
  fn calls_graph_basic_edges() {
      let code = include_str!("../../fixtures/gdscript/player.gd");
      let mut parser = GdscriptParser::new().expect("Failed to create GDScript parser");
      let calls = parser.find_calls(code);

      assert!(
          calls.iter().any(|(c, a, _)| c == "_ready" && a == "spawn_enemy"),
          "expected _ready -> spawn_enemy, got {:?}",
          calls
      );
      assert!(
          calls.iter().any(|(c, a, _)| c == "apply_damage" && a == "_reset"),
          "expected apply_damage -> _reset, got {:?}",
          calls
      );
  }
  ```

- 例: 利用関係（extends/preload）
  ```rust
  #[test]
  fn uses_extends_and_preloads() {
      let code = include_str!("../../fixtures/gdscript/player.gd");
      let mut parser = GdscriptParser::new().expect("Failed to create GDScript parser");
      let uses = parser.find_uses(code);

      assert!(
          uses.iter().any(|(s, t, _)| s == SCRIPT_SCOPE && t == "CharacterBody2D"),
          "expected <script> extends CharacterBody2D, got {:?}",
          uses
      );
      assert!(
          uses.iter().any(|(s, t, _)| s == "EnemyScene" && t == "res://enemies/enemy.gd"),
          "expected EnemyScene preload enemy.gd, got {:?}",
          uses
      );
  }
  ```

- 追加テスト案
  - 複数シグナル発火の同時検出（direct emit と wrapper経由）。
  - ネストした関数内/条件分岐内の呼び出し検出。
  - extendsが複数ある/予期せぬ宣言の扱い（仕様準拠の確認）。
  - 無関係なコード（コメント/文字列リテラル中の疑似呼び出し）の誤検出防止。

## Refactoring Plan & Best Practices

- 重複除去
  - calls/usesの走査が複数回行われているため、HashSet等に変換して一括検証すると可読性と効率が上がる。
    ```rust
    use std::collections::HashSet;

    fn to_edge_set(edges: &[(String, String, /*...*/)]) -> HashSet<(String, String)> {
        edges.iter().map(|(a,b,_)| (a.clone(), b.clone())).collect()
    }
    ```
- アサートメッセージの標準化
  - すべてのアサートで "{:?}" による全体出力を併記し、失敗時の診断を統一。
- テストの粒度
  - 関係ごとに小さなテストに分割（1ケース1主張）し、失敗時の原因特定を容易に。
- 可読性
  - 変数名を統一（calls, uses）。クロージャ引数をc, aなど短縮せず、caller, calleeなど意味のある名前を利用。

## Observability (Logging, Metrics, Tracing)

- 現状テスト側にロギングはなし。必要性は低いが、パーサ実装側に以下があるとデバッグが容易。
  - ログ（解析開始/終了、発見したエッジ数、無視したトークン数）
  - メトリクス（解析時間、トークン数、エッジ数）
  - トレース（特定コード位置からエッジが生成された経路、オプションで出力）

テスト側では失敗時のデータ出力（{:?}でcalls/uses全体）を確実に表示することが最低限の可観測性。

## Risks & Unknowns

- 不明点
  - find_calls/find_usesの正確なシグネチャ・戻り値の第3要素の意味・アルゴリズムは、このチャンクには現れないため不明。
  - SCRIPT_SCOPE("<script>")がパーサ内の仕様として安定かどうか（将来変更の可能性）。
- 依存リスク
  - フィクスチャ（player.gd）の内容変更によりテストが壊れやすい。スナップショットや期待仕様コメントで意図を明文化すると軽減。
- テストの診断性
  - 現状のアサートメッセージフォーマット不備により、失敗時の原因調査が困難。全アサートのフォーマット修正が必要。