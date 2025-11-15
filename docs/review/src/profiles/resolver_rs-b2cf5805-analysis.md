# profiles\resolver.rs Review

## TL;DR

- 目的: 複数ソース（CLI/Local/Manifest）からプロファイル名を優先順位付きで選択するシンプルな**解決ロジック**。
- 公開API: **ProfileResolver::new**, **ProfileResolver::resolve_profile_name**, **Default::default**（実体は new を呼ぶ）。
- 核心ロジック: **Option::or**の連鎖による定数時間の短絡評価（L15-L22）。
- 複雑箇所: 仕様的には単純だが、入力が**Option<String>（所有権移動）**であるため、上位呼び出し側に**不必要な clone**を強いる可能性がある。
- 重大リスク: 空文字列を「有効なプロファイル名」としてそのまま返す点、**検証/正規化が一切無い**点、優先順位の**固定化**。
- Rust安全性: **unsafe不使用**、**所有権**は値を受け取って必要に応じて移動、**Send/Sync**はZSTゆえに安全。
- 並行性/エラー: 共有状態なしでデータ競合なし、戻り値は**Option**で失敗の情報は持たない（文脈を失う可能性）。

## Overview & Purpose

このファイルは、複数の候補ソース（CLI引数、ローカル設定、マニフェスト）から「どのプロファイル名を使うか」を決定する**プロフィール解決**（Profile resolution）ロジックを提供します。優先順位は明確に定義されており、CLI > Local > Manifest の順に短絡評価で最初に見つかった Some(String) を返します。すべて None の場合は None を返します。

用途としては、設定ロードやコマンド実行時に使用するプロファイル選択の標準化された判定を提供する軽量ユーティリティです。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | ProfileResolver | pub | プロファイル名解決のための名前空間/型（ZST） | Low |
| Impl fn | new | pub | Resolverのインスタンス生成（ZSTのため定数時間） | Low |
| Impl fn | resolve_profile_name | pub | 優先順位付きソースからプロファイル名を選択 | Low |
| Trait impl | Default::default | pub（Defaultを介して呼出し可） | new を呼び出すシンタックスシュガー | Low |

### Dependencies & Interactions

- 内部依存
  - Default::default（L25-L29）は new（L9-L11）を呼び出します。
  - resolve_profile_name（L15-L22）は引数で渡された Option<String> を**所有権移動**で `Option::or` に渡し、短絡的に選択します。

- 外部依存（クレート・モジュール）
  - 使用クレート: 標準ライブラリ（std）以外の依存はこのチャンクには現れない。該当なし。

- 被依存推定（このモジュールを使用する可能性がある箇所）
  - CLI引数解析層
  - 設定ローダ（ローカル設定ファイル読取）
  - マニフェスト/プロジェクト設定ローダ
  - 実際のプロファイル適用ロジック（環境/認証/接続設定選択）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | `pub fn new() -> Self` | Resolverインスタンス生成（ZST） | O(1) | O(1) |
| resolve_profile_name | `pub fn resolve_profile_name(&self, cli: Option<String>, local: Option<String>, manifest: Option<String>) -> Option<String>` | 優先順位付きでプロファイル名を選択 | O(1) | O(1) |
| Default::default | `fn default() -> Self` | `new()` と同値のデフォルト生成 | O(1) | O(1) |

### ProfileResolver::new

1. 目的と責務
   - ProfileResolver のインスタンスを生成します。中身のない**ゼロサイズ型（ZST）**のため、生成オーバーヘッドはありません。（L9-L11）

2. アルゴリズム
   - Self を返すだけ。

3. 引数
   - なし

4. 戻り値
   - Self（ProfileResolver）

5. 使用例
```rust
let resolver = profiles::resolver::ProfileResolver::new();
```

6. エッジケース
   - 特になし（ZSTで副作用なし）

### ProfileResolver::resolve_profile_name

1. 目的と責務
   - CLI > Local > Manifest の優先度で、最初に Some(String) なプロファイル名を返します。（L13-L22）

2. アルゴリズム（ステップ分解）
   - 入力: `cli`, `local`, `manifest`（いずれも Option<String>）
   - `cli.or(local)` を評価:
     - `cli` が Some なら `cli` の値を返す（以降評価しない）
     - `cli` が None なら `local` を評価
   - `(...).or(manifest)` を評価:
     - 前段が Some ならそれを返す
     - 前段が None なら `manifest` を返す
   - 結果を返す（None の場合もあり）（L21）

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| cli | Option<String> | 任意 | CLIで指定されたプロファイル名。Someなら最優先 |
| local | Option<String> | 任意 | ローカル設定に定義されたプロファイル名 |
| manifest | Option<String> | 任意 | マニフェストに定義されたプロファイル名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<String> | 最初に見つかった Some(String) を返す。全て None の場合は None |

5. 使用例
```rust
use profiles::resolver::ProfileResolver;

let r = ProfileResolver::new();

assert_eq!(
    r.resolve_profile_name(Some("cli".to_string()), Some("local".to_string()), Some("manifest".to_string())),
    Some("cli".to_string())
);

assert_eq!(
    r.resolve_profile_name(None, Some("local".to_string()), Some("manifest".to_string())),
    Some("local".to_string())
);

assert_eq!(
    r.resolve_profile_name(None, None, Some("manifest".to_string())),
    Some("manifest".to_string())
);

assert_eq!(
    r.resolve_profile_name(None, None, None),
    None
);
```

6. エッジケース
- 空文字列 "" が与えられた場合でも Some("") として返す（仕様要確認）
- 同一文字列が複数ソースに存在しても、優先順位に従って最初のものが返る（重複排除なし）
- 非ASCII文字列は問題なし（Rustの String は UTF-8）
- 前後空白や大文字小文字の差異は未処理（正規化なし）

### Default::default

1. 目的と責務
   - ProfileResolver のデフォルトインスタンス生成。`new()` 呼び出しの糖衣（L25-L29）。

2. アルゴリズム
   - `Self::new()` を呼び出すだけ。

3. 引数
   - なし

4. 戻り値
   - Self（ProfileResolver）

5. 使用例
```rust
let resolver: profiles::resolver::ProfileResolver = Default::default();
```

6. エッジケース
   - 特になし

## Walkthrough & Data Flow

- 関数: `resolve_profile_name`（L15-L22）
  - データ入力: `cli`, `local`, `manifest` はそれぞれ Option<String> として関数に**所有権移動**で渡されます。
  - 評価フロー:
    1. `cli.or(local)` を評価（L21）
       - `cli` が Some(String) の場合、その String の所有権は返り値に移動。`local` は評価されず、そのまま破棄される（Drop）。
       - `cli` が None の場合、`local` の評価に進み、`local` が Some ならそれを返す。
    2. 前段の結果に対して `.or(manifest)` を評価（L21）
       - 前段が None であれば `manifest` を返す。
    3. 戻り値は Option<String>（Someなら選ばれた String の所有権が返却）。
  - 副作用: なし。`&self` は不変参照で状態を持たないため、**純粋関数的**に振る舞います。
  - 例外/エラー: なし。None は「選択不能」を意味するが、原因（なぜ None か）の情報は付与されない。

## Complexity & Performance

- 時間計算量: O(1)。最大3回の Option 評価のみ。
- 空間計算量: O(1)。一時的なスタック上の参照/オプションのみ。
- ボトルネック: 事実上なし。大きな String を持つ場合でも、選択時は**移動**のみで再割り当ては発生しない（呼び出し元での clone が発生しているかは上位設計次第）。
- スケール限界: 無視できるレベル。I/O/ネットワーク/DB を一切行わないため、ホットパスでも問題になりにくい。

## Edge Cases, Bugs, and Security

### エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | cli=Some(""), local=None, manifest=None | 仕様次第。通常は拒否が妥当 | Some("") を返す（L21） | 仕様要確認 |
| すべて None | None/None/None | None を返す | None を返す（L21） | OK |
| 複数ソースが Some | cli=Some("a"), local=Some("b") | "a" を返す（CLI優先） | cli.or(local) で "a"（L21） | OK |
| 重複値 | cli=Some("dev"), manifest=Some("dev") | "dev" を返す | CLI側を返す（L21） | OK |
| 前後空白 | cli=Some(" dev "), ... | トリムして比較/選択が望ましい | トリムなしでそのまま返す | 仕様要確認 |
| 巨大文字列 | cli=Some(非常に長い) | 移動で返す（コピー回避） | 移動で返す | OK |

### バグの可能性

- 入力検証がないため、**空文字列や不正フォーマット**をそのまま採用する危険。
- 戻り値が Option のため、**失敗理由の可視性がない**（例えば「CLI指定が空文字だった」等の文脈が失われる）。

### セキュリティチェックリスト

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（純粋な Option と String の移動のみ）。unsafe 不使用（ファイル全体）。
  - 所有権/借用: 引数の Option<String> は**所有権移動**で消費され、返り値に必要ならそのまま移動される（L15-L22）。
- インジェクション
  - SQL/Command/Path traversal: 本コードは入出力なしで該当なし。ただし「プロファイル名」が後段でコマンド/パスに使われるなら、別層での検証が必要。
- 認証・認可
  - 権限チェック漏れ/セッション固定: 該当なし。
- 秘密情報
  - ハードコード秘密情報/ログ漏洩: 該当なし（ログなし、定数なし）。
- 並行性
  - Race condition/Deadlock: 該当なし。状態を持たない ZST、&self は不変。
  - Send/Sync: ZST かつ内部可変状態なしのため、**自動的に Send + Sync**（不変性により共有安全）。

### Rust特有の観点（詳細チェックリスト）

- 所有権（resolve_profile_name: L15-L22）
  - `cli`, `local`, `manifest` は関数に渡された時点で**所有権が移動**。`Option::or` により、先頭の Some(String) の**中身の String**も返り値へ移動。
- 借用
  - `&self` の不変借用のみ。内部状態なし。可変借用は存在しない。
- ライフタイム
  - 明示的ライフタイム不要。所有 String を返すため、返り値は呼び出し側で独立に生存。
- unsafe 境界
  - なし（ファイル全体に unsafe ブロックなし）。
- 並行性・非同期
  - 非同期境界なし（await 不要）。共有状態なしでデータ競合なし。
- エラー設計
  - Result vs Option: ここでは Option を採用し、**「値がない」**のみを表現。エラー理由の表現は不可。必要なら Result<Option<String>, Error> などへ拡張検討。
- panic 箇所
  - unwrap/expect 不使用、panic 不在。

## Design & Architecture Suggestions

- 入力の型改善
  - 呼び出し側で所有権を奪われないように、引数を `Option<&str>` に変更し、返り値を `Option<String>` とすることで、選択された場合のみ **to_owned** で最小コストのコピーを行う。
- 柔軟な優先順位
  - 優先順位を固定から**構成可能**にする。例: `ProfileSource { cli, local, manifest }` と `enum Priority { Cli, Local, Manifest }` の Vec による順序指定、もしくはビルダーで設定。
- 入力検証/正規化
  - 前後空白トリム、空文字拒否、許容文字のバリデーション（英数字/ハイフン/アンダースコアなど）を追加。
- エラー情報付与
  - 失敗（None）時に、どのソースが提供されたが無効だったか等の**診断情報**を返したい場合は、`Result<Option<String>, ResolveError>` へ拡張。
- ドキュメント強化
  - 「空文字列の扱い」「正規化の有無」「重複の扱い」を明確にする。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト（優先順位と基本動作）
```rust
#[cfg(test)]
mod tests {
    use super::ProfileResolver;

    #[test]
    fn cli_takes_priority() {
        let r = ProfileResolver::new();
        assert_eq!(
            r.resolve_profile_name(
                Some("cli".to_string()),
                Some("local".to_string()),
                Some("manifest".to_string())
            ),
            Some("cli".to_string())
        );
    }

    #[test]
    fn local_used_when_cli_none() {
        let r = ProfileResolver::new();
        assert_eq!(
            r.resolve_profile_name(None, Some("local".to_string()), Some("manifest".to_string())),
            Some("local".to_string())
        );
    }

    #[test]
    fn manifest_used_when_others_none() {
        let r = ProfileResolver::new();
        assert_eq!(
            r.resolve_profile_name(None, None, Some("manifest".to_string())),
            Some("manifest".to_string())
        );
    }

    #[test]
    fn returns_none_when_all_none() {
        let r = ProfileResolver::new();
        assert_eq!(r.resolve_profile_name(None, None, None), None);
    }

    #[test]
    fn empty_string_is_returned_as_is() {
        let r = ProfileResolver::new();
        assert_eq!(
            r.resolve_profile_name(Some("".to_string()), Some("local".to_string()), None),
            Some("".to_string())
        );
    }
}
```

- 統合テスト（仕様拡張時）
  - CLIパーサ、設定ローダ、マニフェストローダと組み合わせ、各層の出力が Option<String> として結合されるパスを確認。
  - 空文字や不正文字列が上流でフィルタリングされるかの検証。

## Refactoring Plan & Best Practices

- ステップ1: `resolve_profile_name` を `Option<&str>` 受け取り、返り値を `Option<String>` に変更。選択時のみ `to_owned()`。
- ステップ2: トリムと空文字拒否のオプションを導入（例: `ProfileResolver::with_normalization(trim: bool, reject_empty: bool)`）。
- ステップ3: 優先順位の外部指定を可能にする API を追加（ビルダーまたは列挙とベクタ）。
- ステップ4: `ResolveError` を設計し、詳細な失敗理由を持てるよう `Result` 版の API を追加。
- ベストプラクティス:
  - 入力はできる限り**借用**で受けて必要時のみ**所有化**。
  - エッジケースの仕様をドキュメントで明記。
  - ログやメトリクスは**呼び出し側**で追加（この層はピュアに保つ）。

## Observability (Logging, Metrics, Tracing)

- 現状ログ/メトリクス/トレースは**このチャンクには現れない**。
- 提案:
  - 呼び出し側で「どのソースが選ばれたか」「選ばれなかった理由（空/不正）」を**DEBUGログ**に出す。
  - 選択率（CLI/Local/Manifest）を**カウンタメトリクス**で可視化し、設定ソースの有効性を分析。
  - トレースでは、リクエスト/コマンド単位でタグに選択されたプロファイル名を付与。

## Risks & Unknowns

- 仕様不明点:
  - 空文字列の扱い（現状は受理）と正規化ポリシー。
  - 許容される文字セット、最大長、予約語。
  - 複数ソースが矛盾した場合の警告や記録の要否。
- リスク:
  - None 戻り時に**原因情報が欠落**し、診断性が低い。
  - 引数が `Option<String>` のため、上位層で**不要な clone**が生じうる（パフォーマンス微小ながら非効率）。
- 運用上の懸念:
  - 後段がこの値を外部I/O（ファイルパス/コマンド）に使う場合、適切な**サニタイズ**を必須とする設計が必要。