# profiles/reference.rs Review

## TL;DR

- 目的: 入力文字列の「profile@provider」構文を解析して、プロファイル名とオプションのプロバイダ名を保持するシンプルなデータ構造を提供（parse: L32-44）。
- 公開API: **ProfileReference**（データ契約, L11-14）、**parse**（L32-44）、**new**（L47-49）、**has_provider**（L52-54）、**Display**（L57-65）、**From<&str>**（L67-71）、**From<String>**（L73-77）。
- コアロジック: `split_once('@')`で最初の@のみを分岐に使用。複数@の扱いは「最初の@のみ分割、以降はプロバイダ文字列に含める」（テストで明示, L100-105）。
- 重大リスク: 入力検証がなく、空プロファイル（"@provider", L147-152）や空プロバイダ（"profile@", L155-160）を許容。仕様として妥当かは要合意。
- パフォーマンス: `parse`は部分文字列を`String`にコピーするためO(n)。`From<String>`はより効率的に所有文字列を再利用できる改善余地あり（現実装は余分な割当を誘発, L73-77）。
- 安全性: **unsafeなし**、データ競合なし、メモリ安全性は標準ライブラリに依存して保証。エラー設計は`Result`非採用で厳密性が低い。
- 推奨: 厳格解析（空セグメントや禁止文字を拒否）を行う`parse_strict`/`TryFrom<&str>`の追加、`From<String>`の割当削減、借用ベースAPI（`ProfileRef<'a>`）導入。

## Overview & Purpose

このファイルは、"profile@provider"形式の参照文字列を簡易的に解析し、プロファイル名（必須）とプロバイダ名（任意）を保持する**ProfileReference**構造体を提供します。  
主な用途は、設定やCLI引数などから入力されたプロファイル参照を内部表現に変換し、表示（再シリアライズ）や判定を容易にすることです。

外部依存は最小限で、`std::fmt`の`Display`実装に限定されます（L3, L57-65）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | ProfileReference | pub | プロファイル名とオプションのプロバイダ名を保持（L11-14） | Low |
| Method | parse(&str) -> Self | pub | 文字列を解析し`ProfileReference`を生成（L32-44） | Low |
| Method | new(String, Option<String>) -> Self | pub | 直接フィールドから生成（L47-49） | Low |
| Method | has_provider(&self) -> bool | pub | プロバイダ指定有無の判定（L52-54） | Low |
| Trait impl | fmt::Display | public impl | `profile@provider`形式で文字列化（L57-65） | Low |
| Trait impl | From<&str> | public impl | `&str`からの変換（L67-71） | Low |
| Trait impl | From<String> | public impl | `String`からの変換（L73-77） | Low |
| Module | tests | cfg(test) | 単体テスト群（L79-161） | Low |

### Dependencies & Interactions

- 内部依存
  - `From<&str>::from` → `ProfileReference::parse`（L68-70 → L32-44）
  - `From<String>::from` → `ProfileReference::parse`（L74-76 → L32-44）
  - `fmt::Display::fmt`は`ProfileReference`のフィールドに依存（L58-63）
  - `has_provider`は`Option<String>`の状態に依存（L52-54）

- 外部依存（標準のみ）

| 依存 | 用途 | 備考 |
|------|------|------|
| std::fmt | Display実装 | フォーマット出力（L57-65） |

- 被依存推定
  - 設定ローダ、CLI引数解析、プロファイル選択ロジック等がこの構造体を利用する可能性あり（このチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ProfileReference | struct { pub profile: String, pub provider: Option<String> } | データ契約（プロファイル参照の保持） | O(1) | O(|profile| + |provider|) |
| ProfileReference::parse | fn parse(input: &str) -> Self | 文字列を解析して構造体へ変換 | O(n) | O(n) |
| ProfileReference::new | fn new(profile: String, provider: Option<String>) -> Self | フィールドから直接構築 | O(1) | O(1)（引数所有のため追加割当なし） |
| ProfileReference::has_provider | fn has_provider(&self) -> bool | プロバイダ有無の判定 | O(1) | O(1) |
| fmt::Display for ProfileReference | fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result | 構造体を`profile@provider`で文字列化 | O(n) | O(1) |
| From<&str> for ProfileReference | fn from(s: &str) -> Self | `&str`からの便利変換 | O(n) | O(n) |
| From<String> for ProfileReference | fn from(s: String) -> Self | `String`からの便利変換 | O(n) | O(n)（現実装は追加割当あり） |

以下、各APIの詳細です。

1) ProfileReference（データ契約）
- 目的と責務
  - 入力参照の正規化結果を保持。`profile`は必須、`provider`は任意（L11-14）。
- フィールド

| フィールド | 型 | 説明 |
|-----------|----|------|
| profile | String | プロファイル名（必須） |
| provider | Option<String> | プロバイダ名（任意、None=未指定） |

- 使用例
```rust
use codanna::profiles::reference::ProfileReference;

let r = ProfileReference { profile: "codanna".to_string(), provider: Some("claude-provider".to_string()) };
assert_eq!(r.profile, "codanna");
assert!(r.provider.is_some());
```
- エッジケース
  - 空文字を許容するかは呼び出し側の仕様次第（この実装は許容）。

2) ProfileReference::parse
- 目的と責務
  - 文字列から`ProfileReference`を構築。最初の'@'で分割、後続の'@'はプロバイダ側に含める（L32-44, L100-105）。
- アルゴリズム
  1. `input.split_once('@')`で最初の'@'の位置を探索（L33）。
  2. 見つかれば左を`profile`、右を`provider`とし、それぞれ`String`化（L35-37）。
  3. 見つからなければ`provider=None`として`profile`全体を`String`化（L39-42）。
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| input | &str | はい | 入力参照文字列 |

- 戻り値

| 型 | 意味 |
|----|------|
| Self | 解析結果 |

- 使用例
```rust
let ref1 = ProfileReference::parse("codanna");
assert_eq!(ref1.profile, "codanna");
assert_eq!(ref1.provider, None);

let ref2 = ProfileReference::parse("codanna@claude-provider");
assert_eq!(ref2.profile, "codanna");
assert_eq!(ref2.provider, Some("claude-provider".to_string()));
```
- エッジケース
  - 空プロファイル: "@provider" → profile=""（L147-152）
  - 空プロバイダ: "profile@" → provider=Some("")（L155-160）
  - 複数'@': "a@b@c" → provider="b@c"（L100-105）

3) ProfileReference::new
- 目的と責務
  - 既に所有している`String`から構築（L47-49）。追加割当なし。
- 引数

| 名称 | 型 | 必須 | 説明 |
|------|----|------|------|
| profile | String | はい | プロファイル名（所有） |
| provider | Option<String> | いいえ | プロバイダ名（所有） |

- 戻り値

| 型 | 意味 |
|----|------|
| Self | 構築結果 |

- 使用例
```rust
let r = ProfileReference::new("codanna".to_string(), Some("claude-provider".to_string()));
assert_eq!(r.to_string(), "codanna@claude-provider");
```
- エッジケース
  - None指定時は`profile`のみの表示（L108-111）。

4) ProfileReference::has_provider
- 目的と責務
  - `provider`がSomeかどうかを返す（L52-54）。
- 使用例
```rust
let r = ProfileReference::parse("codanna");
assert!(!r.has_provider());
```

5) fmt::Display for ProfileReference
- 目的と責務
  - フォーマット時に`"profile@provider"`または`"profile"`を出力（L57-65）。
- アルゴリズム
  1. `provider`がSomeなら`write!(..., "{}@{}", ...)`（L59-61）
  2. Noneなら`profile`のみ（L62-63）
- 使用例
```rust
let r = ProfileReference::parse("codanna@provider");
assert_eq!(r.to_string(), "codanna@provider");
```
- エッジケース
  - 空プロバイダでも`"profile@"`の形で出力（L155-160 → 表示は仕様上そのまま）。

6) From<&str> for ProfileReference
- 目的と責務
  - `&str`から`parse`を呼ぶ糖衣（L67-71）。
- 使用例
```rust
let r: ProfileReference = "codanna@provider".into();
```

7) From<String> for ProfileReference
- 目的と責務
  - `String`から`parse`を呼ぶ糖衣（L73-77）。
- 留意点
  - 現実装は`parse(&s)`で部分文字列を新規割当するため、`profile`側には元の`String`を再利用できず、追加割当が発生する可能性（改善余地あり）。

## Walkthrough & Data Flow

- 入力フロー
  - 呼び出しは主に`ProfileReference::parse(input)`または`Into<ProfileReference>`（From impl）を通じて行われます（L67-77 → L32-44）。
- 解析
  - 最初に`split_once('@')`（L33）で分割判定。成功時は左右を`String`化して格納（L35-37）、失敗時は`provider=None`（L39-42）。
- 出力
  - `Display`（L57-65）が`to_string()`を通じてシリアライズ（テストで往復確認, L121-128）。

## Complexity & Performance

- 時間計算量
  - parse: O(n)（`split_once`走査 + 部分文字列のコピー）
  - Display: O(n)（フォーマット書き込み）
  - has_provider/new: O(1)
- 空間計算量
  - parse: O(n)（`profile`/`provider`の新規`String`割当）
  - From<&str>/<String>: 実質parseと同等
- ボトルネック
  - 文字列コピーによる割当が主。大量・高頻度の解析でGC/Allocator負荷増。
- 改善余地
  - `From<String>`で元の`String`を再利用する最適化が可能（下記Refactoring参照）。
  - 借用ベースのAPI（`&str`を保持する軽量ビュー）で割当を削減可能。

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | profile="" かつ provider=None | parseは`split_once`失敗→profile=""、provider=None（L39-42） | テストなし |
| 空プロファイル | "@provider" | profile=""、provider=Some("provider") | parseの分割で左が空（L33-37） | テストで確認（L147-152） |
| 空プロバイダ | "profile@" | profile="profile"、provider=Some("") | 分割で右が空（L33-37） | テストで確認（L155-160） |
| 複数'@' | "a@b@c" | profile="a"、provider="b@c" | 最初の@のみ有効（L33） | テストで確認（L100-105） |
| プロバイダ未指定 | "codanna" | provider=None | else分岐（L39-42） | テストで確認（L84-89） |
| 空白やトリム | " profile @ provider " | 未定（そのまま保持） | トリム処理なし | 不明（このチャンクには現れない） |

- バグの可能性
  - 入力検証なしのため、空や不正文字（禁則）を含む文字列も受理。仕様に依存。
  - From<String>の実装は所有文字列の再利用が不十分で、不要な割当を発生させる可能性（L73-77）。機能バグではないが効率面で改善余地。

- セキュリティチェックリスト
  - メモリ安全性: 標準APIのみ、**unsafeなし**（ファイル内にunsafeブロックは存在しない）。Buffer overflow / Use-after-free / Integer overflow の懸念なし。
  - インジェクション: 本コード単体では外部システムに渡さないため、SQL/Command/Path traversalリスクは直接なし。だが呼び出し側で利用する場合は妥当性検証が望ましい。
  - 認証・認可: 該当なし（このチャンクには現れない）。
  - 秘密情報: ハードコード秘密やログ漏えいなし。
  - 並行性: 共有可変状態なし、**Race/Deadlock**の懸念なし。`ProfileReference`は`String`のみで構成されるため自動的に`Send`、`Sync`（借用なし、静的に安全）。

- Rust特有の観点
  - 所有権: `parse`は`&str`から新規`String`を生成し所有権を取得（L35-41）。`From<String>`は引数`s`の所有権を受けつつも実体は再利用せずに新規生成（L74-76）。
  - 借用: `Display::fmt`では`&self`の不変借用のみ、可変借用なし（L58）。
  - ライフタイム: 明示的ライフタイム不要。すべて所有型。
  - unsafe境界: 使用なし。
  - 非同期/並行: 非同期境界やawaitは登場しない。
  - エラー設計: `Result`ではなく常に成功として値を返す設計。厳密なバリデーションが必要なら`TryFrom<&str>`や`parse_strict`の導入が有効。

## Design & Architecture Suggestions

- 入力厳格化
  - `parse_strict(input: &str) -> Result<ProfileReference, ParseError>`を追加し、空セグメントや禁止文字（例: 制御文字）を拒否。
  - `TryFrom<&str>`/`TryFrom<String>`の実装でエラー型を統合。
- 表現の最適化
  - 借用ビュー型の追加: `struct ProfileRef<'a> { profile: &'a str, provider: Option<&'a str> }` を導入し、割当なしで解析結果を扱えるAPIを用意。
- 利用性の向上
  - `ProfileReference::provider()`で`Option<&str>`を返す読み取り専用APIの追加（コピー不要）。
  - 正規化ポリシー（トリムやケース規則）をオプションで提供。

## Testing Strategy (Unit/Integration) with Examples

- 既存テストは主要ケースをカバー（プロファイルのみ、プロバイダあり、複数@、空セグメント、往復表示; L84-160）。
- 追加推奨テスト
  - ホワイトスペースとトリミングの挙動
  - Unicode/非ASCII（例: 日本語や絵文字を含むプロファイル/プロバイダ）
  - 極端に長い文字列（パフォーマンスと割当の健全性）
  - 厳格解析（導入後）でのエラーケース

例: Unicodeを含むケース
```rust
#[test]
fn test_unicode_profile_and_provider() {
    let r = ProfileReference::parse("プロファイル@プロバイダ✨");
    assert_eq!(r.profile, "プロファイル");
    assert_eq!(r.provider, Some("プロバイダ✨".to_string()));
    assert_eq!(r.to_string(), "プロファイル@プロバイダ✨");
}
```

例: トリムポリシー（導入時）
```rust
#[test]
fn test_parse_strict_trims_and_validates() {
    // 仮のAPI: parse_strictで前後空白を拒否/トリムなどの仕様がある場合
    // let r = ProfileReference::parse_strict("  profile  @  provider  ").unwrap();
    // assert_eq!(r.profile, "profile");
    // assert_eq!(r.provider.as_deref(), Some("provider"));
}
```

## Refactoring Plan & Best Practices

- `From<String>`の割当削減（所有文字列再利用）
```rust
impl From<String> for ProfileReference {
    fn from(mut s: String) -> Self {
        if let Some(idx) = s.find('@') {
            // 再利用: 左側は元のStringをtruncateして所有権を保持
            let provider = s[idx + 1..].to_string();
            s.truncate(idx);
            ProfileReference { profile: s, provider: Some(provider) }
        } else {
            ProfileReference { profile: s, provider: None }
        }
    }
}
```
- 厳格解析APIの追加（エラー型設計）
```rust
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("empty profile name")]
    EmptyProfile,
    #[error("invalid character in profile/provider")]
    InvalidChar,
}

impl ProfileReference {
    pub fn parse_strict(input: &str) -> Result<Self, ParseError> {
        // 例: 空プロファイル拒否
        if let Some((profile, provider)) = input.split_once('@') {
            if profile.is_empty() { return Err(ParseError::EmptyProfile); }
            // 追加の禁止文字チェックなど
            Ok(Self { profile: profile.to_string(), provider: Some(provider.to_string()) })
        } else {
            if input.is_empty() { return Err(ParseError::EmptyProfile); }
            Ok(Self { profile: input.to_string(), provider: None })
        }
    }
}
```
- 借用ビュー型の導入（割当無し）
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileRef<'a> {
    pub profile: &'a str,
    pub provider: Option<&'a str>,
}

impl<'a> ProfileRef<'a> {
    pub fn parse(input: &'a str) -> Self {
        if let Some((p, prov)) = input.split_once('@') {
            Self { profile: p, provider: Some(prov) }
        } else {
            Self { profile: input, provider: None }
        }
    }
}
```
- API拡張
  - `fn provider(&self) -> Option<&str>`を追加し、コピー不要で参照取得。

## Observability (Logging, Metrics, Tracing)

- 現状、ログ/メトリクス/トレースは不要な単純モジュール。
- 厳格解析を導入する場合は、エラー件数や入力分布（`provider`有無比率など）をメトリクス収集すると実運用でのチューニングに有用。
- 例: エラー時に`trace!`レベルで入力と要因を記録（個人情報/秘密情報を含まない前提で）。

## Risks & Unknowns

- 仕様不明点
  - 空プロファイル/空プロバイダを許容するか（現実装は許容, L147-160）。上流の仕様合意が必要。
  - 複数'@'の扱い（現実装は最初の'@'のみ分割, L33）。プロバイダ名に'@'が含まれてよいか。
  - トリム/正規化ポリシー（このチャンクには現れない）。
- 運用上のリスク
  - 検証なしの入力を他コンポーネントへ渡すと、後段でエラーやインジェクション様の誤用リスクが生じ得る（このモジュール単体では攻撃ベクトルなし）。
  - 大規模に解析する場合の割当負荷（`parse`のO(n)コピー）。改善は可能（Refactoring参照）。

以上により、本ファイルは機能的にシンプルかつ安全だが、入力検証の厳格化と小さなパフォーマンス改善で、堅牢性と効率をさらに高められます。