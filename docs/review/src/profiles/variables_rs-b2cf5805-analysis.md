# profiles\variables.rs Review

## TL;DR

- 目的: プロファイルテンプレート向けの変数を4階層（**Global** < **Manifest** < **Local** < **CLI**）で保持し、優先度に従って単一のマップへ統合する仕組み。
- 公開API: `Variables`構造体、`new`、`set_global`、`set_manifest`、`set_local`、`set_cli`、`merge`、`Default::default`。
- 複雑箇所: 優先度マージの正確性（衝突時の上書き）と、`merge`が全データをクローンするためのコスト設計。
- 重大リスク: 監視（上書きの可視化）の欠如による意図しない値のサイレント上書き、`HashMap`順序非決定性、`merge`のフルコピーによるメモリ・CPUオーバーヘッド。
- Rust安全性: `unsafe`なし、所有権・借用はシンプル。`&str`→`String`のヒープ確保が発生。並行読み取りは安全（`&self`）だが書き込みは`&mut self`が必要。
- 追加提案: レイヤー検索用`get(&str)`の導入、`merge`事前容量予約、キー検証、上書きイベントのログ追加、安定順序が必要なら`BTreeMap`採用。

## Overview & Purpose

このファイルは、プロファイルテンプレートに用いられる変数群を「グローバル」「マニフェスト」「ローカル」「CLI」の4階層で管理し、優先度に従って統合（マージ）する小さなユーティリティです。優先度は「CLI > Local > Manifest > Global」で、下位レイヤーの値は上位レイヤーに同一キーが存在すると上書きされます。

主な用途:
- 変数ソースが複数あるテンプレートレンダリング時に、最終的に使用する値集合を決定する。
- ユーザー指定（CLI）で定義済み値を上書きしたいケースに対応。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | Variables | pub | 4階層の`HashMap<String, String>`を保持 | Low |
| Function | Variables::new | pub | 空の各レイヤーマップを初期化 | Low |
| Function | Variables::set_global | pub | Globalレイヤーへの設定 | Low |
| Function | Variables::set_manifest | pub | Manifestレイヤーへの設定 | Low |
| Function | Variables::set_local | pub | Localレイヤーへの設定 | Low |
| Function | Variables::set_cli | pub | CLIレイヤーへの設定 | Low |
| Function | Variables::merge | pub | 優先度順で全レイヤーを統合 | Low |
| Trait Impl | Default for Variables | pub | `Variables::new()`の別名 | Low |

### Dependencies & Interactions

- 内部依存:
  - `Default::default` → `Variables::new`を呼び出し（行番号は不明）。
  - `set_*`メソッド群（global/manifest/local/cli）は、それぞれ対応する`HashMap`へ挿入するのみ（行番号は不明）。
  - `merge`は4レイヤーの`HashMap`を優先度に従い、低→高の順に`extend`して新しい`HashMap`に統合（行番号は不明）。
- 外部依存（標準ライブラリのみ）:

  | 依存 | 用途 |
  |------|------|
  | `std::collections::HashMap` | 変数格納とマージのベースデータ構造 |

- 被依存推定（このモジュールを利用する可能性がある箇所）:
  - テンプレートエンジン（プロファイル生成時の値解決）
  - CLI引数処理層（ユーザー指定の変数注入）
  - マニフェスト/設定ファイルローダー

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| Variables::new | `pub fn new() -> Self` | 空の変数セットを作成 | O(1) | O(1) |
| Variables::set_global | `pub fn set_global(&mut self, key: &str, value: &str)` | Globalにキー/値を設定 | O(1)平均 | O(1) |
| Variables::set_manifest | `pub fn set_manifest(&mut self, key: &str, value: &str)` | Manifestにキー/値を設定 | O(1)平均 | O(1) |
| Variables::set_local | `pub fn set_local(&mut self, key: &str, value: &str)` | Localにキー/値を設定 | O(1)平均 | O(1) |
| Variables::set_cli | `pub fn set_cli(&mut self, key: &str, value: &str)` | CLIにキー/値を設定 | O(1)平均 | O(1) |
| Variables::merge | `pub fn merge(&self) -> HashMap<String, String>` | 優先度順に統合した結果を返す | O(N) | O(N) |
| Default::default | `fn default() -> Self` | `new`の委譲 | O(1) | O(1) |

Nは全レイヤーに存在するキー総数。

### Variables::new

1) 目的と責務
- 新規の`Variables`インスタンスを初期化し、各レイヤーを空の`HashMap`として用意します。

2) アルゴリズム（ステップ分解）
- `HashMap::new()`で4つの空マップを作成。
- 構造体へ設定して返却。

3) 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| なし | - | - | - |

4) 戻り値

| 型 | 説明 |
|----|------|
| `Self` | 空の変数セット |

5) 使用例
```rust
use profiles::variables::Variables;

let vars = Variables::new();
assert_eq!(vars.merge().len(), 0);
```

6) エッジケース
- 特になし（初期化のみ、常に成功）。

### Variables::set_global / set_manifest / set_local / set_cli

1) 目的と責務
- 指定レイヤーにキー/値をセットします。既存キーがある場合は上書きされます。

2) アルゴリズム（ステップ分解）
- `HashMap::insert(key.to_string(), value.to_string())`で挿入。

3) 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| key | `&str` | Yes | 変数名（文字列） |
| value | `&str` | Yes | 変数値（文字列） |

4) 戻り値

| 型 | 説明 |
|----|------|
| `()` | なし（副作用で内部マップ更新） |

5) 使用例
```rust
use profiles::variables::Variables;

let mut vars = Variables::new();
vars.set_global("region", "us-east-1");
vars.set_manifest("region", "eu-west-1");
vars.set_local("region", "ap-northeast-1");
vars.set_cli("region", "eu-central-1");

let merged = vars.merge();
assert_eq!(merged.get("region"), Some(&"eu-central-1".to_string()));
```

6) エッジケース
- 空文字キー: 受け付け可能だが意味的に不適切。検証は実装されていない。
- 極端に長いキー/値: メモリ使用量増加。制限・検証はない。
- 重複キー: 同一レイヤーでは上書き、レイヤー間では優先度に従い最終的に上位レイヤーが勝つ。

### Variables::merge

1) 目的と責務
- 優先度「CLI > Local > Manifest > Global」に従って、全レイヤーを1つの`HashMap`に統合します。

2) アルゴリズム（ステップ分解）
- 空`result`を作成。
- `global.clone()`を`result.extend(...)`で追加。
- `manifest.clone()`を追加（同キーがあれば上書き）。
- `local.clone()`を追加（同キーがあれば上書き）。
- `cli.clone()`を追加（同キーがあれば上書き）。
- `result`を返す。

3) 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | `&self` | Yes | 読み取りのみ |

4) 戻り値

| 型 | 説明 |
|----|------|
| `HashMap<String, String>` | 統合結果（各キーは最上位レイヤーの値） |

5) 使用例
```rust
use profiles::variables::Variables;

let mut vars = Variables::new();
vars.set_global("a", "G");
vars.set_manifest("a", "M");
vars.set_local("a", "L");
vars.set_cli("a", "C");
vars.set_global("b", "GB");
vars.set_local("c", "LC");

let merged = vars.merge();
assert_eq!(merged.get("a"), Some(&"C".to_string())); // CLI優先
assert_eq!(merged.get("b"), Some(&"GB".to_string())); // Globalのみ
assert_eq!(merged.get("c"), Some(&"LC".to_string())); // Localのみ
```

6) エッジケース
- キー衝突: 下位レイヤーは上位で上書き。監査ログがないため上書き検知不可。
- 空の各レイヤー: 正しく空の結果や下位のみを返す。
- 大量データ: 全レイヤーをクローンするためコスト増。

### Default::default

1) 目的と責務
- `Variables::new()`と同じ初期化を提供（`Default`トレイト準拠）。

2) アルゴリズム（ステップ分解）
- `Variables::new()`を呼び出すだけ。

3) 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| なし | - | - | - |

4) 戻り値

| 型 | 説明 |
|----|------|
| `Self` | 空の変数セット |

5) 使用例
```rust
use profiles::variables::Variables;

let vars: Variables = Default::default();
assert_eq!(vars.merge().len(), 0);
```

6) エッジケース
- 特になし。

## Walkthrough & Data Flow

- 典型的フロー:
  1. `Variables::new()`または`Default::default()`で空集合を用意。
  2. 各ソースから読み取った値を`set_global`/`set_manifest`/`set_local`/`set_cli`で投入。
  3. `merge()`を呼ぶと、優先度順に統合された`HashMap<String, String>`が得られる。

- データ競合の例（上書きの連鎖）:
  - 同じキー`"region"`が各レイヤーに存在する場合、`merge()`結果はCLI値となる。
  - `manifest`→`local`→`cli`の順に`extend`されるため、最後に追加された値が勝つ。

- 戻り値は新規の`HashMap`であり、各レイヤーの元データから独立しています。結果を書き換えても元レイヤーには影響しません。

「コード行番号」はこのチャンクでは不明のため、関数名のみで根拠を示しています（例: 優先度順は`Variables::merge`の処理順に依存）。

## Complexity & Performance

- `set_*`: 平均O(1)時間、O(1)追加メモリ（`String`化に伴うヒープ確保あり）。
- `merge`: 時間O(N)、空間O(N)（N=全キー数）。4回の`HashMap::extend`と4マップ分のクローンが行われます。
- ボトルネック:
  - 大規模データでは`merge()`の全コピーがCPU・メモリに負荷。
  - `HashMap`のリハッシュが複数回起きうる（`result`の容量未予約）。
- スケール限界:
  - 毎回完全統合する設計は、頻繁に同一データを再マージするワークロードに不向き。
- 実運用負荷要因:
  - I/Oやネットワークはこのチャンクには現れない。CPU/メモリ負荷のみが関与。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: すべて安全（`unsafe`未使用、標準`HashMap`・`String`のみ）。
- インジェクション
  - SQL/Command/Path traversal: このチャンクには現れない。値の利用先で対策が必要。
- 認証・認可
  - このチャンクには現れない。
- 秘密情報
  - 値に秘密情報が含まれる可能性はあるが、ログ出力はそもそも存在しないため漏えいは「このチャンクでは」なし。監査面では上書き検知がない点に注意。
- 並行性
  - 読み取り（`&self`で`merge`）は競合なし。書き込みは`&mut self`が要求されるためデータ競合を防ぐ設計。
  - マルチスレッドで同一`Variables`に対して同時に`set_*`するには外部同期が必要（`Mutex`など）。内部ロックはない。
- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字キー | `set_cli("", "x")` | 可能ならエラーまたは拒否 | 検証なし | 許容（サイレント） |
| 重複キーの上書き | `a`が全レイヤーに存在 | 最上位（CLI）が勝つ | `merge`の`extend`順により実現 | OK |
| 大量キー | 100kキー各レイヤー | 完全統合（性能低下） | 全クローン | パフォーマンス懸念 |
| Unicodeキー/値 | `"地域"`, `"東京"` | 問題なく保持 | `String` | OK |
| キー前後空白 | `" region "` | 定義通り扱う（トリムなし） | 検証なし | 許容（注意） |

## Design & Architecture Suggestions

- 🔧 優先度検索APIの追加: `fn get(&self, key: &str) -> Option<&str>`で、マージせずとも優先度に従った単一キー解決を可能にする（CPU/メモリ削減）。
- 🔧 `merge`の容量予約: `HashMap::with_capacity(total_len)`でリハッシュ回数を減らし性能改善。
- 🔧 上書き監査/ロギング: `merge`時に、下位レイヤーの値が上位で上書きされたキーを収集・ログ出力/メトリクス化することで不意の上書きを検知。
- 🔧 安定順序が必要なら`BTreeMap`の選択肢を検討（非決定的な順序が問題となるユースケースで役立つ）。
- 🔧 API統合: `set(&mut self, tier: Tier, key: impl Into<String>, val: impl Into<String>)`のような共通設定API。`Tier`は`enum`で表現。
- 🔧 値型の見直し: 値に`Arc<str>`や`Cow<'a, str>`を用いることでコピー削減（ただしライフタイム設計が複雑化）。
- 🔧 バリデーション: キー形式（空文字、制御文字、禁止文字）に対する検証を追加。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト観点
  - 優先度: 同一キーが全レイヤーに存在する場合、CLIが勝つ。
  - 結合: レイヤーごとに異なるキーが全て含まれる。
  - 上書き: 下位レイヤーの値が上位で上書きされること。
  - 空セット: 新規作成・`default`で空。
  - 大文字小文字/Unicodeキーの取り扱い。

- 例: 優先度と結合
```rust
#[cfg(test)]
mod tests {
    use super::Variables;

    #[test]
    fn merge_priority_and_union() {
        let mut vars = Variables::new();
        vars.set_global("k1", "G");
        vars.set_manifest("k1", "M");
        vars.set_local("k1", "L");
        vars.set_cli("k1", "C");
        vars.set_global("only_g", "G2");
        vars.set_local("only_l", "L2");

        let merged = vars.merge();
        assert_eq!(merged.get("k1"), Some(&"C".to_string()));
        assert_eq!(merged.get("only_g"), Some(&"G2".to_string()));
        assert_eq!(merged.get("only_l"), Some(&"L2".to_string()));
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn default_is_new() {
        let vars_default: Variables = Default::default();
        let vars_new = Variables::new();
        assert_eq!(vars_default.merge(), vars_new.merge());
    }

    #[test]
    fn unicode_keys_values() {
        let mut vars = Variables::new();
        vars.set_manifest("地域", "東京");
        let merged = vars.merge();
        assert_eq!(merged.get("地域"), Some(&"東京".to_string()));
    }
}
```

- 例: 上書き検知（将来の監査用に拡張した場合）
  - 現状はログ機構がないため、このチャンクには現れない。拡張後に上書きキー一覧のアサーションを追加。

## Refactoring Plan & Best Practices

- `merge`の最適化:
  - 合計サイズ計測後に`HashMap::with_capacity`で容量を予約。
  - クローンを避ける代替（例: 目的が単一キー解決なら`get`導入）。
- API改善:
  - 共通`set`関数 + `Tier`列挙型。
  - `insert_*`で`impl Into<String>`を受け、呼び出し側の余計な`to_string`を不要化。
- 機能追加:
  - `remove_*`や`clear_*`で柔軟な管理。
  - `iter_*`で各レイヤーの列挙を提供。
- 仕様面:
  - キー検証ポリシー（空文字禁止など）。
  - 上書きメトリクス/ログ。

## Observability (Logging, Metrics, Tracing)

- ログ: `merge`時に上書きが発生したキーを収集して`debug`/`info`ログへ。例「key=region overridden by tier=CLI from tier=Local」。
- メトリクス: 上書き件数、レイヤーごとのキー数、`merge`の処理時間・メモリ使用量（ヒストグラム）。
- トレーシング: テンプレートレンダリングのスパン内で、変数解決ステップをサブスパンとして記録。
- 現状: このチャンクには観測機構は現れない。追加は上位レイヤーでの責務にしてもよい。

## Risks & Unknowns

- 優先度の固定性: 現在はCLIが最上位に固定。変更可能性は仕様上「不明」。
- HashMap順序: 非決定的順序の影響（テンプレートが順序依存なら問題）。順序要件は「不明」。
- 大規模運用時の`merge`コスト: 実際のキー数・呼び出し頻度は「不明」。必要に応じて設計見直しが必要。
- エラーハンドリング要件: キー形式やサイズ制限などのポリシーは「不明」。現在はすべて受け入れる。
- コード行番号: このチャンクには行番号が含まれないため、詳細な位置特定は「不明」。