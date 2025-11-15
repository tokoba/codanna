# io\guidance_engine.rs Review

## TL;DR

- 目的: 設定（GuidanceConfig）に基づき、ツール別のテンプレートからガイダンステキストを生成する。
- 公開API: generate_guidance_from_config(&GuidanceConfig, &str, Option<&str>, usize) -> Option<String>（L6-L35）。
- コアロジック: ツール名でテンプレートを取得し、件数に応じてテンプレートを選択（select_template, L38-L52）。文字列置換で各種プレースホルダを埋める。
- 複雑箇所: テンプレート選択の優先順位（custom範囲が先、なければ標準テンプレート）、連続的な文字列置換による副作用。
- 重大リスク: Option::is_none_orの互換性不明（L41）。オーバーラップするカスタム範囲の優先順が明確でない。_queryが未使用。テンプレート/変数値の未サニタイズによるテキストインジェクションの懸念。
- エラー設計: 失敗理由が区別されないOption返却（disabled/テンプレート未登録/テンプレート未定義などの区別不可）。
- 並行性: 非同期/共有状態なしで安全だが、観測可能性（ログ/メトリクス）は未整備。

## Overview & Purpose

このファイルは、設定から動的にガイダンステキストを生成するためのエンジンです。ユーザーが利用するツール名と結果件数を入力すると、対応するテンプレートを選び、変数を埋め込んだテキストを返します。設定で機能が無効化されている場合はNoneを返します。

- 入力: GuidanceConfig（enabled/variables/templatesを想定）、tool（ツール識別子文字列）、query（現状未使用）、result_count（整数）。
- 出力: Option<String>（テンプレートが見つかり、選択できた場合はSome、そうでなければNone）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | generate_guidance_from_config | pub | 設定からテンプレート取得・選択・埋め込みを行いガイダンス文字列を生成 | Med |
| Function | select_template | private | 件数に応じたテンプレート選択（カスタム範囲優先、なければ標準） | Low |
| Struct（外部） | GuidanceConfig | external | enabledフラグ、templates、variablesを保持（本チャンクに具体定義なし） | 不明 |
| Struct（外部） | GuidanceTemplate | external | custom範囲、no/single/multipleの標準テンプレートを保持（本チャンクに具体定義なし） | 不明 |

### Dependencies & Interactions

- 内部依存
  - generate_guidance_from_config → select_template（L20で呼び出し）
  - GuidanceConfig.templatesからツール名をキーにGuidanceTemplateを取得（L17）
  - GuidanceConfig.variablesをループしプレースホルダ置換（L30-L32）

- 外部依存（モジュール/クレート）
  | 依存 | 種別 | 用途 | 備考 |
  |------|------|------|------|
  | crate::config::GuidanceConfig | モジュール | 設定の入力 | このチャンクには定義なし |
  | crate::config::GuidanceTemplate | モジュール | テンプレートの構造 | このチャンクには定義なし |
  | std::string::String / std::option::Option | 標準 | 文字列生成・存在判定 | replace, cloneの使用 |

- 被依存推定
  - CLI/GUIのヘルプ表示機能、検索結果UI、レコメンド/ヒントドロワーなどで利用される可能性が高い。
  - io配下の他モジュールから呼ばれるサービス層的ユーティリティ。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| generate_guidance_from_config | fn generate_guidance_from_config(config: &GuidanceConfig, tool: &str, _query: Option<&str>, result_count: usize) -> Option<String> | 設定に従いツール別テンプレートを選択し、変数を埋めてガイダンス文字列を返す | O(C + L*(V+2)) | O(L) |

- 記号の意味
  - C: custom範囲数（template.customの要素数）
  - V: 変数数（config.variablesのキー数）
  - L: テンプレート文字列の長さ

### generate_guidance_from_config 詳細

1. 目的と責務
   - 設定が有効か確認（L12-L14）。
   - ツール名でテンプレートを取得（L17）。
   - 件数に基づきテンプレートを選択（L20、select_template）。
   - 結果件数/result_count、ツール名/tool、設定変数/variablesの順で文字列置換（L26-L32）。
   - 完成した文字列を返却（L34）。

2. アルゴリズム（ステップ分解）
   - enabledがfalseならNone。
   - templates.get(tool)でGuidanceTemplateをOption取得、なければNone。
   - select_template(template, result_count)でOption<String>を取得、なければNone。
   - 置換順序:
     - {result_count} → 数値文字列
     - {tool} → ツール名
     - {key} → config.variablesの各値（ループ）
   - Some(result)で返却。

3. 引数
   | 引数名 | 型 | 必須 | 説明 |
   |--------|----|------|------|
   | config | &GuidanceConfig | はい | ガイダンス生成に必要な設定。enabled, templates, variablesを使用。 |
   | tool | &str | はい | テンプレート選択のキーとなるツール識別子。 |
   | _query | Option<&str> | いいえ | クエリ文字列。現状未使用（下線付与で未使用の意図、L6-L11）。 |
   | result_count | usize | はい | 結果件数。テンプレート選択および{result_count}置換に使用。 |

4. 戻り値
   | 型 | 条件 | 説明 |
   |----|------|------|
   | Option<String> | Some | テンプレート選択・文字列生成に成功。 |
   | Option<String> | None | 無効化（enabled=false）、ツール未登録、件数に合うテンプレートがNoneなどのいずれか。理由は区別されない。 |

5. 使用例
   - 注意: GuidanceConfig/GuidanceTemplateの具体定義はこのチャンクには現れないため、ここでは概念的な例として呼び出しのみ示します。

   ```rust
   use crate::io::guidance_engine::generate_guidance_from_config;
   use crate::config::{GuidanceConfig}; // 実際の構造はこのチャンクには現れない

   fn example(config: &GuidanceConfig) {
       let tool = "search";
       let result_count = 3usize;
       // _queryは現状未使用だが、将来拡張のためOption<&str>を渡せる
       let guidance = generate_guidance_from_config(config, tool, Some("rust tutorial"), result_count);
       if let Some(text) = guidance {
           println!("Guidance: {}", text);
       } else {
           println!("No guidance available");
       }
   }
   ```

6. エッジケース
   - enabled=falseで直ちにNone。
   - ツールに対応するテンプレートがない場合None。
   - custom範囲に該当しない場合は標準テンプレート（no/single/multiple）にフォールバック。
   - テンプレートがNone（標準が未設定）の場合None。
   - 変数にキーが存在してもテンプレートにプレースホルダがなければ置換されない（副作用なし）。
   - 同名プレースホルダの繰り返し出現はすべて置換される（String::replaceは全置換）。

## Walkthrough & Data Flow

- 入力フロー
  - 呼び出し側 → generate_guidance_from_config(config, tool, _query, result_count)
- 処理フロー（根拠: generate_guidance_from_config L6-L35）
  1. enabledチェック（L12） → falseならNone（L13）
  2. templates.get(tool)（L17） → Noneなら早期None
  3. select_template呼び出し（L20） → Noneなら早期None
  4. 置換処理（L26-L32）
     - {result_count} → result_count（L26）
     - {tool} → tool（L27）
     - {key} → variablesの各値（L30-L32）
  5. Some(result)で返却（L34）

- テンプレート選択フロー（根拠: select_template L38-L52）
  1. customを先頭から順に走査（L40）
  2. in_range判定（L41）
     - 下限: result_count >= range.min
     - 上限: range.maxがNoneなら上限なし、Someならresult_count <= max
     - Option::is_none_orの存在・意味はこのチャンクでは不明（標準APIまたは拡張トレイトの可能性）
  3. 該当する最初のcustomのtemplateを返却（L43）
  4. 該当なしならmatchで標準テンプレートへフォールバック（L48-L52）

## Complexity & Performance

- 時間計算量
  - generate_guidance_from_config: O(C + L*(V+2))
    - C: custom範囲走査（select_template内）
    - 置換はString::replaceの全走査を伴うため、テンプレート長Lに比例し、回数はV+2回
  - select_template: O(C)（customの線形走査）＋O(1)（match分岐）

- 空間計算量
  - O(L): 最終文字列（result）と途中clone（template_str.clone()）のため、テンプレート長に比例
  - 追加の一時文字列（replace結果）も同程度だが、古いresultは都度破棄される

- ボトルネック
  - 多数の変数（Vが大）かつ長大なテンプレート（Lが大）の場合、連続replaceがコスト増。
  - custom範囲数（C）が非常に大きい場合の線形走査。

- スケール限界
  - 1回の生成でCPU-bound（I/Oなし）のため、並列呼び出しはスレッド数に比例してスケール可能。
  - テンプレート長・変数数が非常に大きいとヒープ割り当てとコピーが増え、GCはないがアロケーション負荷増。

- 実運用負荷
  - I/O/ネットワーク/DBアクセスなし（このチャンクには現れない）。
  - 設定・テンプレートは事前ロードされる前提が妥当。

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 機能無効化 | enabled=false | Noneを返す | generate_guidance_from_config L12-L14 | OK |
| ツール未登録 | templates.get("unknown") → None | Noneを返す | generate_guidance_from_config L17 | OK |
| custom上限なし | range.max=None, min=3, result_count=100 | customテンプレートを選ぶ | select_template L41 | OK |
| custom重複/オーバーラップ | custom: [min=0..10], [min=5..15], result_count=7 | 先に一致したエントリを選ぶ（順序依存） | select_template L40-L44 | 仕様不明（明示的でない） |
| 標準テンプレート未設定 | template.no_results=Noneなど | Noneを返す | select_template L48-L52 | OK |
| 変数キー未定義 | variablesに"author"なし、テンプレートに{author} | 未置換（そのまま残る） | generate_guidance_from_config L30-L32 | 仕様要検討 |
| _query未使用 | _query=Some("...") | 動作に影響なし | 関数引数L6-L11 | 未対応（将来拡張枠） |

- 潜在バグ/仕様上の曖昧さ
  - Option::is_none_orの互換性不明（L41）。標準のOptionに存在しない環境ではコンパイルエラー。拡張トレイト由来の可能性あり。このチャンクには定義が現れない。
  - customの順序依存で、複数範囲が重複すると先勝ちになる。明確な仕様化と重複検出の警告が望ましい。
  - 連続replaceにより、変数値が別のプレースホルダを含む場合、置換順序に依存する副作用が発生しうる（例: {tool}を値に含むvalueで再置換）。

- セキュリティチェックリスト
  - メモリ安全性: 標準的な所有権/借用のみ。unsafeなし。Buffer overflow, Use-after-free, Integer overflowは発生しない（純RustのString操作）。
  - インジェクション:
    - SQL/Command: 該当なし（このチャンクには現れない）。
    - Path traversal: 該当なし。
    - テキスト/HTMLインジェクション: config.variablesやテンプレートが外部入力に由来する場合、レンダリング先（HTML/Markdown/UI）でのエスケープが必要。現状無加工で返すため、表示側で対策が必要。
  - 認証・認可: 該当なし。
  - 秘密情報: ハードコード秘匿情報なし。ただしvariablesに機密情報を入れる場合のログ漏洩に注意（現状ロギングなし）。
  - 並行性: 共有可変状態なし。Race/Deadlockの懸念なし。

- Rust特有の観点
  - 所有権: config, toolは借用参照（L6-L11）。戻り値は新規String（所有権が呼び出し側に移動）。
  - 借用: &GuidanceConfig, &strの不変借用のみ。可変借用なし。
  - ライフタイム: 明示的ライフタイム不要。cloneによりテンプレート断片の所有権を新規取得（L23, L43, L49-L51）。
  - unsafe境界: なし。
  - 並行性・非同期: Send/Sync境界の記述なし。関数は純粋で副作用なし。awaitは使用しない。キャンセル考慮不要。
  - エラー設計: Optionで「ない」を表現。理由の区別ができないため、必要に応じてResultや詳細なenumの導入を検討。

## Design & Architecture Suggestions

- エラーの表現改善
  - Option<String>では原因が不明なため、GuidanceError（Disabled/UnknownTool/NoTemplate/InvalidRangeなど）を持つResult<String, GuidanceError>にすることでデバッグ容易化。
- custom範囲仕様の明確化
  - 範囲のオーバーラップ検知と警告（起動時検証）。
  - 範囲を非重複に正規化する、あるいは優先度フィールドを導入。
- 置換エンジンの堅牢化
  - 1パスでのトークン化置換（テンプレート走査し、一度のバッファ構築）によりO(L+Σtoken)へ削減。
  - テンプレートエンジン（例: Handlebars/Tera）の導入を検討。ただし外部依存はプロジェクト方針に依存。
- _queryの扱い
  - クエリに応じたテンプレート選択や追加プレースホルダ（{query}）をサポートするか、未使用なら引数削除。
- バージョン互換性
  - Option::is_none_or使用の互換性を要確認。非対応環境向けに「range.max.map_or(true, |max| result_count <= max)」へ置換可能。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト観点
  - enabled=falseでNoneを返す。
  - 未登録ツールでNoneを返す。
  - custom範囲に該当する選択（境界: min、max、上限なし）。
  - 標準テンプレートへのフォールバック（0件、1件、複数）。
  - 変数置換（複数キー、存在しないキー）。
  - オーバーラップ範囲で先勝ち確認（customの順序依存）。

- 例（概念テスト: 型定義はこのチャンクには現れないため擬似的な形で記述）
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      // 注意: GuidanceConfig/GuidanceTemplateの実体はcrate::config側にあり、このチャンクには現れない。
      // 実プロジェクトでは、適切なビルダー/フィクスチャを用意してテストしてください。

      #[test]
      fn returns_none_when_disabled() {
          // config.enabled = false の状態を用意
          // let config = make_config_disabled(); // 擬似
          // let out = generate_guidance_from_config(&config, "search", None, 3);
          // assert!(out.is_none());
      }

      #[test]
      fn select_custom_range_first() {
          // customに [min=0..10], [min=5..15] を順序通り設定
          // result_count=7で先頭のcustomが選ばれること
          // let config = make_config_with_overlap(); // 擬似
          // let out = generate_guidance_from_config(&config, "search", None, 7).unwrap();
          // assert_eq!(out, "custom-0-10");
      }

      #[test]
      fn fallback_to_standard_templates() {
          // customが空or不一致、result_count=0/1/2でno/single/multipleが選ばれる
          // let config = make_config_with_standard_only(); // 擬似
          // assert_eq!(generate_guidance_from_config(&config, "search", None, 0).unwrap(), "no");
          // assert_eq!(generate_guidance_from_config(&config, "search", None, 1).unwrap(), "one");
          // assert_eq!(generate_guidance_from_config(&config, "search", None, 2).unwrap(), "many");
      }

      #[test]
      fn variables_are_replaced() {
          // variablesに {"env":"prod","tool":"override"} を設定
          // テンプレート "Tool:{tool}, Env:{env}, Count:{result_count}"
          // {tool}は内蔵置換→その後カスタム置換で再適用されないことを確認（仕様要検討）
          // let config = make_config_with_vars(); // 擬似
          // let out = generate_guidance_from_config(&config, "search", None, 3).unwrap();
          // assert!(out.contains("Count:3"));
      }
  }
  ```

- 統合テスト
  - 実際の設定ファイル読み込み→Config構築→API呼び出し→UI表示までのエンドツーエンド。
  - 異常系（テンプレート欠落、変数未定義）での挙動検証。

## Refactoring Plan & Best Practices

- 置換最適化
  - 連続String::replaceより、テンプレート走査しHashMapでプレースホルダ→値を解決する一括置換。
  - プレースホルダを「{name}」固定にし、名前の検証（英数字/アンダースコアのみ）を追加。

- select_templateの互換性改善
  - is_none_orをmap_or(true, |max| result_count <= max)へ変更（互換性広い）。
  ```rust
  let in_range = result_count >= range.min
      && range.max.map_or(true, |max| result_count <= max);
  ```

- エラー型の導入
  - 以下の例のようなResult化（概念案）。
  ```rust
  enum GuidanceError {
      Disabled,
      UnknownTool,
      NoTemplateForCount,
  }
  // 既存呼び出し側の負担を考慮し、Optionを維持しつつログに詳細を出す選択肢もあり。
  ```

- _queryの扱い
  - 実際に{query}プレースホルダをサポートするか、引数削除でインターフェイスを簡素化。

- ドキュメント化
  - custom範囲の仕様（半開区間/閉区間、重複時の優先順位）をREADME/ドキュメント化。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - disabledでのスキップ、unknown tool、テンプレート未設定のケースでdebugログ。
  - customマッチの範囲情報（min/max）をtraceログで出力可能。

- メトリクス
  - guidance生成成功/失敗カウンタ。
  - テンプレート選択種別（custom/standard）の割合。
  - 置換に要した時間の簡易計測（必要なら）。

- トレーシング
  - request_id（呼び出しコンテキスト）をスパンに乗せる設計（このチャンクにはトレーシング無し）。

## Risks & Unknowns

- Option::is_none_orの存在/互換性が環境依存（このチャンクには拡張トレイトの定義は現れない）。ビルド対象Rustのバージョンや外部トレイトに左右される。
- GuidanceConfig/GuidanceTemplateの詳細不明（フィールド型・保証）。このチャンクから推測できる範囲:
  - GuidanceConfig: enabled(bool), templates（Map: &str→GuidanceTemplate）, variables（Map: String→String）
  - GuidanceTemplate: custom（範囲のVec）、no_results/single_result/multiple_results（Option<String>）
- custom範囲の重複・順序の仕様が不明。
- _query未使用の意図（将来拡張か、不要な引数か）不明。
- 表示先のコンテキスト（HTML/CLI/Markdown）が不明で、エスケープ戦略の適否が判断不能。

以上を踏まえ、公開APIの安定性を確保しつつ、互換性（Option::is_none_or）、エラー可観測性、テンプレート仕様の明文化、置換処理の最適化と安全性向上を優先して検討することを推奨します。