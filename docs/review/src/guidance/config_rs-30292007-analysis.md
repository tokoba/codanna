# guidance\config.rs Review

## TL;DR

- このファイルは、ガイダンス生成用の設定とテンプレート選択ロジックの中核を提供する。主要公開APIは、**GuidanceConfig::default**（L59-66）と **ToolGuidance::get_template**（L213-229）。
- テンプレート選択は、まず**カスタム範囲**を優先し、該当しなければ結果件数に応じて標準テンプレート（0件/1件/複数件）にフォールバックする。
- 重大なリスク: 逆直列化時のフィールド既定値が、`Default`実装の「豊富な初期テンプレート」と一致しない点（`tools`/`global_vars`が`#[serde(default)]`で空ベースに）。設定ソースにより初期値がばらつく可能性。
- **安全性**: unsafeは不使用。**エラー設計**は`Option<&str>`での存在チェックのみ。**並行性**は明示対応なしだが、型は`Send + Sync`満たすため読み取り共有は安全。
- 複雑箇所は「範囲テンプレートの優先順位と重複処理」。重複範囲が存在すると「最初に定義された範囲が常に選ばれる」仕様（L215-221）。
- 推奨改善: 逆直列化既定値の統一、範囲重複検出・ソート、信頼度の範囲検証、テンプレート変数解決APIの追加。

## Overview & Purpose

このモジュールは、ガイダンスシステムの設定（グローバル・ツール別）と、結果件数に応じたテンプレート選択ロジックを提供する。次の3構造体を中心に成り立つ。

- GuidanceConfig（L8-24）: システム全体の有効化フラグ、既定信頼度、ツール別テンプレート集合、グローバル変数を保持。
- ToolGuidance（L28-45）: ツール単位の「0件/1件/複数件」テンプレートと、件数範囲に応じたカスタムテンプレート群、ツール固有変数を保持。
- RangeTemplate（L49-56）: 件数範囲（min〜max, inclusive）に紐づくテンプレートを定義。

GuidanceConfig::default（L59-66）は豊富な初期テンプレート集合（L83-209）とグローバル変数（L77-81）を構築する。ToolGuidance::get_template（L213-229）は、件数から最適テンプレートを選ぶコアロジック。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | GuidanceConfig (L8-24) | pub | ガイダンス全体設定（有効化、既定信頼度、ツール別テンプレート、グローバル変数） | Low |
| Struct | ToolGuidance (L28-45) | pub | ツール単位のテンプレート（0/1/複数件、範囲カスタム）と変数保持 | Low |
| Struct | RangeTemplate (L49-56) | pub | 件数範囲（min/max, inclusive）に対応するテンプレート | Low |
| Function | GuidanceConfig::default (L59-66) | public via Default | 豊富な既定設定の構築 | Med |
| Function | ToolGuidance::get_template (L213-229) | pub | 件数に応じたテンプレート選択 | Low |
| Function | default_enabled (L69-71) | private | `enabled`のSerde既定値 | Low |
| Function | default_confidence (L73-75) | private | `default_confidence`のSerde既定値 | Low |
| Function | default_global_vars (L77-81) | private | 既定グローバル変数 | Low |
| Function | default_tool_templates (L83-209) | private | 既定ツール別テンプレート群 | Med |

### Dependencies & Interactions

- 内部依存
  - GuidanceConfig::default → default_tool_templates（L83-209）, default_global_vars（L77-81）
  - ToolGuidance::get_template → self.ranges（L40）, self.no_results/single_result/multiple_results（L30-36）
- 外部依存

| 依存 | 用途 | 備考 |
|------|------|------|
| std::collections::HashMap (L3) | テンプレート・変数のマップ管理 | 標準ライブラリ |
| serde::{Deserialize, Serialize} (L4) | 設定の直列化/逆直列化 | バージョン不明（このチャンクには現れない） |

- 被依存推定
  - ガイダンス生成ロジック（テンプレートへの値埋め込みやレンダリング）から参照される想定。
  - 設定ロード層（ファイル/環境変数/CLI）から`GuidanceConfig`への逆直列化。
  - 実際の利用箇所はこのチャンクには現れない。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| GuidanceConfig::default | `fn default() -> Self` | 既定設定の構築 | O(T + R) | O(T + R) |
| ToolGuidance::get_template | `fn get_template(&self, result_count: usize) -> Option<&str>` | 件数に応じたテンプレート選択 | O(R) | O(1) |

ここで、Tは既定ツール数（現状7）、Rは各ツールの範囲テンプレート数。

### Data Contracts

- GuidanceConfig
  - enabled: bool（Serde既定: true, L69-71）
  - default_confidence: f32（Serde既定: 0.8, L73-75）※範囲検証なし
  - tools: HashMap<String, ToolGuidance>（Serde既定: 空, L18-19／Default実装で豊富な既定値, L63）
  - global_vars: HashMap<String, String>（Serde既定: 空, L22-23／Default実装で`project_name=codanna`, L64, L77-81）
- ToolGuidance
  - no_results/single_result/multiple_results: Option<String>（L30-36）
  - ranges: Vec<RangeTemplate>（L39-40）
  - variables: HashMap<String, String>（L43-44）
- RangeTemplate
  - min: usize（inclusive, L51）
  - max: Option<usize>（inclusive, Noneは上限なし, L53）
  - template: String（L55）

### GuidanceConfig::default

1. 目的と責務
   - 豊富な既定テンプレートとグローバル変数をセットした完全な設定を構築（L59-66）。
2. アルゴリズム
   - enabled=true（L61）
   - default_confidence=0.8（L62）
   - tools=default_tool_templates()（L63）で各ツールのテンプレート群挿入（L86, L108, L123, L138, L153, L179, L194）
   - global_vars=default_global_vars()（L64, L77-81）
3. 引数

| 名称 | 型 | 役割 |
|-----|----|-----|
| なし | — | 既定設定生成 |

4. 戻り値

| 型 | 説明 |
|----|------|
| GuidanceConfig | 既定設定インスタンス |

5. 使用例
```rust
use guidance::config::GuidanceConfig;

let cfg = GuidanceConfig::default(); // L59-66
assert!(cfg.enabled);
assert_eq!(cfg.default_confidence, 0.8);
assert!(cfg.tools.contains_key("find_symbol"));
assert_eq!(cfg.global_vars.get("project_name").map(String::as_str), Some("codanna"));
```
6. エッジケース
- 設定ファイルから逆直列化した場合、`tools`/`global_vars`は`#[serde(default)]`により空になる点（L18-23）は、Default実装の値（L63-64）とは異なる。

### ToolGuidance::get_template

1. 目的と責務
   - 結果件数に対して最適なテンプレート（文字列）を選択（L213-229）。
2. アルゴリズム
   - 範囲テンプレート優先で線形探索（L215-221）
     - `in_range = result_count >= min && (max.is_none() || result_count <= max)`（L216-217）
     - 該当範囲が見つかればそのテンプレートを返す（L219-220）
   - 見つからなければ標準テンプレートにフォールバック（L224-228）
     - 0件 → no_results（L225）
     - 1件 → single_result（L226）
     - 2件以上 → multiple_results（L227）
3. 引数

| 名称 | 型 | 必須 | 説明 |
|-----|----|------|------|
| result_count | usize | Yes | 検索/解析結果件数 |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<&str> | 適用テンプレート文字列への借用参照。該当テンプレートが未設定ならNone |

5. 使用例
```rust
use guidance::config::{GuidanceConfig};

let cfg = GuidanceConfig::default();
let tg = cfg.tools.get("analyze_impact").unwrap();
let tpl = tg.get_template(3); // 範囲(2..=5)に該当: L164-168, L215-221
assert_eq!(tpl, Some("Limited impact radius with {result_count} affected symbols. This change is relatively contained."));
```
6. エッジケース
- 範囲が重複する場合、先に定義された範囲が優先される（線形探索のため, L215-221）。
- 標準テンプレートがNoneの場合はNoneを返す（L225-227）。
- `result_count`が非常に大きい場合は、上限なし範囲（`max=None`）がなければ複数件テンプレートへフォールバック。
- `result_count=0`や`1`の標準テンプレートが未設定だとNone。

## Walkthrough & Data Flow

- 設定の生成
  - `GuidanceConfig::default()`（L59-66）で既定のツール群（例: "find_symbol", "get_calls" など）とグローバル変数が構築される（L83-209, L77-81）。
- テンプレート選択のフロー
  - 呼び出し元は対象ツールの`ToolGuidance`を取得し（例: `cfg.tools.get("find_symbol")`）、`get_template(result_count)`（L213-229）を実行。
  - 範囲テンプレートが優先され、該当がなければ0/1/複数件の標準へフォールバック。
- テンプレートの利用
  - 文字列はプレースホルダ（例: `{result_count}`）を含むが、置換ロジックはこのチャンクには現れない。呼び出し側で`str::replace`等を用いる。
```rust
let cfg = GuidanceConfig::default();
if let Some(tool) = cfg.tools.get("semantic_search_docs") {
    if let Some(tpl) = tool.get_template(12) {
        // ここで {result_count} 等のプレースホルダを置換（このチャンクには現れない）
        let rendered = tpl.replace("{result_count}", "12");
        println!("{}", rendered);
    }
}
```
- データ依存関係
  - `get_template`は`ranges`（L39-40）と各標準テンプレート（L30-36）に依存。

## Complexity & Performance

- GuidanceConfig::default
  - 時間計算量: O(T + R)（T=ツール数, R=各ツールの範囲テンプレート総数）。現状は定数規模（約7ツール、範囲は少数）。
  - 空間計算量: O(T + R) マップとベクタの構築。
- ToolGuidance::get_template
  - 時間計算量: O(R)（範囲テンプレートの線形走査）→ 典型的に非常に小さい。
  - 空間計算量: O(1)。
- ボトルネック
  - 範囲数が大きくなると線形探索が増えるが、現状は小規模。I/Oやネットワーク/DBは関与なし。
- スケール限界
  - 多数ツール・多数範囲を設定ファイルで運用する場合、テンプレート選択は依然軽量だが、設定ロード時のメモリ消費は増加。

## Edge Cases, Bugs, and Security

- 主要な挙動・潜在的バグ
  - Serde既定値とDefault既定値の不一致
    - `tools`/`global_vars`に`#[serde(default)]`（L18-23）が指定されており、逆直列化時に欠落すると「空」が入る。一方`GuidanceConfig::default()`（L59-66）は「豊富な既定値」を設定するため、設定導入経路により初期値がばらつく。期待と異なる挙動を引き起こし得る。
  - 範囲の重複
    - 重複範囲が定義されると「先勝ち」（L215-221）。意図しないテンプレート選択の可能性。範囲検証はこのチャンクには現れない。
  - 値検証の欠如
    - `default_confidence`（L62, L73-75）は[0.0, 1.0]制約の保証がない。外部設定で不正値が入り得る。
  - 変数解決
    - `global_vars`/`variables`は保持のみ（L22-23, L43-44）。テンプレートへの変数埋め込みの仕組みはこのチャンクには現れない。

- セキュリティチェックリスト
  - メモリ安全性
    - Buffer overflow / Use-after-free / Integer overflow: 該当なし（標準コレクションと安全な参照返却。unsafe不使用）。
    - 所有権/借用: `get_template`は`&self`から`&str`を返すのみ（L213-229）。所有権移動なし。ライフタイムは`self`に依存。
  - インジェクション
    - SQL/Command/Path traversal: このチャンクには現れない。テンプレート中の文字列は表示/ログに使われる想定であり、コマンド実行等は不明。
  - 認証・認可
    - 該当なし。
  - 秘密情報
    - ハードコード: `project_name="codanna"`（L79-80）。秘密ではないが、環境依存値なら設定化が望ましい。
    - ログ漏えい: 該当なし（ログ機能なし）。
  - 並行性
    - Race condition / Deadlock: 該当なし。構造体は不変で使えば安全。`HashMap<String, _>`や`String`は`Send + Sync`を満たすため、読み取り共有は安全。可変操作は呼び出し側の同期が必要。

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| Serde既定 vs Default不一致 | JSONに`tools`/`global_vars`欠落 | 豊富な既定値が欲しい場合は自動補完 | 現行は空に（L18-23） | 注意必要 |
| 範囲重複 | ranges: [min=2..=5], [min=3..=10] | 明確な優先順位/検証 | 先勝ち（L215-221） | 改善余地 |
| 標準テンプレート未設定 | no_results/single_result/multiple_resultsいずれもNone | Noneが返る | as_derefでNone（L225-227） | 仕様 |
| 上限なし範囲 | max=None | 上限なしでマッチ | `map_or(true, ...)`で対応（L217） | OK |
| 信頼度不正値 | default_confidence=2.0 | [0,1]にクランプ/エラー | 検証なし | 改善余地 |
| 未定義ツール参照 | tools.get("unknown") | None/エラー | HashMapの仕様 | 仕様 |

## Design & Architecture Suggestions

- 逆直列化既定値の統一
  - `#[serde(default = "default_tool_templates")]`（L18-19に適用）、`#[serde(default = "default_global_vars")]`（L22-23に適用）へ変更し、`Default`実装と同一の初期値を保証。これにより「設定経路による初期値差異」を解消。
- 範囲テンプレートの健全性検証
  - 構築時に範囲の重複/順序をチェックし、重複時に警告/エラー、もしくは`min`昇順へソートして決定的な優先規則を明示。
- 信頼度のバリデーション
  - `default_confidence`を[0.0, 1.0]へクランプまたは不正値でエラーにするBuilder/APIを提供。
- 変数解決APIの追加
  - `{result_count}`や`global_vars`/`variables`を安全に展開する`render(&self, ctx: &HashMap<String, String>) -> String`等を追加。*テンプレートエンジン導入は過剰であれば最小限の置換でも可*。
- 静的初期化の最適化
  - テンプレートが不変なら`lazy_static`/`once_cell::sync::Lazy`で共有し、`GuidanceConfig::default()`が参照をクローンする構造にするとコスト削減・一貫性向上。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト
  - 範囲優先の確認
  - 標準テンプレートフォールバック
  - 逆直列化既定値（serde）とDefault既定値の差異
  - 包含境界（inclusive）の検証（min/max）
- 統合テスト
  - 設定ファイル（JSON/TOML）からロード→テンプレート選択→レンダリングの一連の流れ

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn range_priority_wins() {
        let tg = ToolGuidance {
            no_results: None,
            single_result: None,
            multiple_results: Some("fallback".into),
            ranges: vec![
                RangeTemplate { min: 2, max: Some(5), template: "range1".into() },
                RangeTemplate { min: 3, max: Some(10), template: "range2".into() },
            ],
            variables: HashMap::new(),
        };
        assert_eq!(tg.get_template(4), Some("range1")); // 先勝ち（L215-221）
    }

    #[test]
    fn fallback_to_standard_templates() {
        let tg = ToolGuidance {
            no_results: Some("none".into()),
            single_result: Some("one".into()),
            multiple_results: Some("many".into()),
            ranges: vec![],
            variables: HashMap::new(),
        };
        assert_eq!(tg.get_template(0), Some("none")); // L225
        assert_eq!(tg.get_template(1), Some("one"));  // L226
        assert_eq!(tg.get_template(2), Some("many")); // L227
    }

    #[test]
    fn serde_default_is_empty_collections() {
        // tools/global_varsが欠落したJSON
        let v = json!({
            "enabled": true,
            "default_confidence": 0.5
        });
        let cfg: GuidanceConfig = serde_json::from_value(v).unwrap();
        assert!(cfg.tools.is_empty());      // L18-19
        assert!(cfg.global_vars.is_empty()); // L22-23
        assert_eq!(cfg.default_confidence, 0.5);
    }

    #[test]
    fn default_config_populates_rich_templates() {
        let cfg = GuidanceConfig::default(); // L59-66
        for key in [
            "semantic_search_docs",
            "find_symbol",
            "get_calls",
            "find_callers",
            "analyze_impact",
            "search_symbols",
            "semantic_search_with_context",
        ] {
            assert!(cfg.tools.contains_key(key), "missing {}", key);
        }
        assert_eq!(cfg.global_vars.get("project_name").map(String::as_str), Some("codanna"));
    }

    #[test]
    fn inclusive_bounds() {
        let tg = ToolGuidance {
            no_results: None,
            single_result: None,
            multiple_results: None,
            ranges: vec![
                RangeTemplate { min: 2, max: Some(5), template: "in".into() },
            ],
            variables: HashMap::new(),
        };
        assert_eq!(tg.get_template(2), Some("in")); // min inclusive
        assert_eq!(tg.get_template(5), Some("in")); // max inclusive
        assert_eq!(tg.get_template(6), None);
    }
}
```

## Refactoring Plan & Best Practices

- Serde属性の見直し
  - `tools`/`global_vars`に`#[serde(default = "default_tool_templates")]`/`#[serde(default = "default_global_vars")]`を適用し、Default実装と一致させる。これにより利用経路差による挙動差異を解消。
- 範囲管理の強化
  - 追加API: `validate_ranges(&self) -> Result<(), Error>`で重複/無効範囲検出。
  - 範囲を`min`昇順にソートして決定的な選択を保証。
- 値検証の導入
  - Builderパターン（`GuidanceConfigBuilder`）で`default_confidence`の範囲検証を行い、生成時に不正値を拒否。
- テンプレート変数解決の抽象化
  - シンプルな置換ユーティリティ（`render_template(tpl: &str, vars: &HashMap<String, String>)`）を用意。*このチャンクには現れないが拡張容易*。
- ドキュメンテーション
  - 範囲の「inclusive」であること、範囲重複時の優先規則（先勝ち）をdocに明記。

## Observability (Logging, Metrics, Tracing)

- ログ
  - `get_template`で「範囲命中」「標準フォールバック」「未設定（None）」を`trace`/`debug`ログ可能に。
- メトリクス
  - ヒット件数、フォールバック回数、None率などをカウンタで計測。A/Bテストやテンプレート改善に活用。
- トレーシング
  - テンプレート選択にスパンを設け、上位の検索/解析リクエストIDと紐付ける。*このチャンクには現れないが設計上有益*。

## Risks & Unknowns

- テンプレート変数の置換規則（衝突、エスケープ、有効キー）はこのチャンクには現れない。
- 設定のロード元（ファイル/環境/DB）やバージョン管理、移行戦略は不明。
- 国際化・多言語テンプレート対応は不明。
- 信頼度の意味論（0.8の意味、閾値の使用箇所）はこのチャンクには現れない。