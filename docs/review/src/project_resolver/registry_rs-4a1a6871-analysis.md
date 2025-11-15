# registry.rs Review

## TL;DR

- 目的: **プロジェクト構成解決プロバイダ**を登録・列挙し、設定に基づき**有効なプロバイダのみを抽出**する軽量レジストリを提供
- 主要公開API: **ResolutionProviderRegistry::providers**, **SimpleProviderRegistry::{new, add, active_providers, default}**
- 複雑箇所: 特になし。唯一のロジックは`active_providers`の**フィルタリング**（L34-L42）
- 重大リスク: `Arc<dyn ProjectResolutionProvider>`に**Send/Sync境界がないため**、並行利用の安全性が保証されない
- バグ可能性: **重複登録が防止されていない**、`providers()`は生スライスを返すため**並行読み取り前提の安全性は型で保証されない**
- パフォーマンス: すべて**O(1)**または`active_providers`の**O(n)**。現状問題なし
- セキュリティ: インジェクションや秘密情報の扱いは**該当なし**だが、**ロギング・監査性なし**

## Overview & Purpose

このファイルは、プロジェクト構成を解決するための複数のプロバイダ（`ProjectResolutionProvider`）を管理する**レジストリ**を提供します。目的は以下の通りです。

- プロバイダの登録・保持（`SimpleProviderRegistry`）
- 登録済みプロバイダの列挙（`ResolutionProviderRegistry::providers`）
- 設定（`crate::config::Settings`）に応じた**有効プロバイダの抽出**（`active_providers`）

シンプルで可読性の高い設計で、**共有所有権**のために`Arc`を用いています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Trait | ResolutionProviderRegistry | pub | レジストリからプロバイダスライスを取得するAPIを定義 | Low |
| Struct | SimpleProviderRegistry | pub | プロバイダの登録と列挙、設定に基づく有効プロバイダ抽出 | Low |
| Impl | Default for SimpleProviderRegistry | pub | `default()`で空レジストリ作成 | Low |
| Method | SimpleProviderRegistry::new | pub | 空のレジストリを生成 | Low |
| Method | SimpleProviderRegistry::add | pub | プロバイダをレジストリに追加 | Low |
| Method | SimpleProviderRegistry::active_providers | pub | 設定に基づき有効なプロバイダを抽出 | Low |
| Method | ResolutionProviderRegistry::providers | pub | 登録済みプロバイダのスライスを返す | Low |

Dependencies & Interactions
- 内部依存:
  - `SimpleProviderRegistry::active_providers`は`ProjectResolutionProvider::is_enabled`（`super::provider::ProjectResolutionProvider`に定義）を呼び出します（L40）。
  - `ResolutionProviderRegistry`トレイトは`SimpleProviderRegistry`に実装され、`providers()`から内部ベクタのスライスを返します（L46-L49）。
- 外部依存（同一クレート内モジュール・標準ライブラリ）:

  | 依存 | 用途 | 影響 |
  |------|------|------|
  | `super::provider::ProjectResolutionProvider` | プロバイダのトレイト（`is_enabled`使用） | 並行性境界・API契約に影響 |
  | `crate::config::Settings` | 有効判定の入力 | 設定内容に応じたフィルタ結果 |
  | `std::sync::Arc` | プロバイダ共有所有権 | メモリ安全・並行アクセスの下地 |

- 被依存推定:
  - 上位のプロジェクト解決コーディネータ、CLI/サーバの**プロジェクト読み込み**フローでこのレジストリが利用される可能性が高い
  - テストユーティリティとして**モックプロバイダ登録**に使われる可能性

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ResolutionProviderRegistry::providers | `fn providers(&self) -> &[Arc<dyn ProjectResolutionProvider>]` | 登録済みプロバイダ一覧（スライス）の取得 | O(1) | O(1) |
| SimpleProviderRegistry::new | `pub fn new() -> Self` | 空レジストリの作成 | O(1) | O(1) |
| SimpleProviderRegistry::add | `pub fn add(&mut self, provider: Arc<dyn ProjectResolutionProvider>)` | プロバイダの登録 | 平均O(1)（再割当時O(n)） | O(1)（ベクタ増加分） |
| SimpleProviderRegistry::active_providers | `pub fn active_providers(&self, settings: &crate::config::Settings) -> Vec<Arc<dyn ProjectResolutionProvider>>` | 設定に基づく有効プロバイダ抽出 | O(n) | O(k)（k=有効件数） |
| SimpleProviderRegistry::default | `fn default() -> Self`（`Default`トレイト経由） | デフォルトレジストリ生成 | O(1) | O(1) |

詳細
1) ResolutionProviderRegistry::providers（L7-L9, 実装 L46-L49）
- 目的と責務
  - レジストリ内部の登録済みプロバイダを、読み取り専用スライスで提供する
- アルゴリズム
  - 内部ベクタへの参照を返すのみ
- 引数

  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | `&self` | はい | レジストリ参照 |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | `&[Arc<dyn ProjectResolutionProvider>]` | 登録済みプロバイダの共有参照スライス |

- 使用例
  ```rust
  let registry = SimpleProviderRegistry::new();
  let slice = ResolutionProviderRegistry::providers(&registry);
  for p in slice {
      // 参照のみ
  }
  ```
- エッジケース
  - 登録数0の場合は空スライスを返す
  - 返すのはスライスであり、構造の変更は不可。安全

2) SimpleProviderRegistry::new（L23-L27）
- 目的と責務
  - 空のレジストリを構築
- アルゴリズム
  - 空ベクタを初期化
- 引数: なし
- 戻り値

  | 型 | 説明 |
  |----|------|
  | `Self` | 空のレジストリ |

- 使用例
  ```rust
  let mut registry = SimpleProviderRegistry::new();
  assert_eq!(ResolutionProviderRegistry::providers(&registry).len(), 0);
  ```
- エッジケース
  - 特になし

3) SimpleProviderRegistry::add（L29-L31）
- 目的と責務
  - 1つのプロバイダをレジストリへ追加
- アルゴリズム
  - 内部ベクタへ`push`
- 引数

  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | `&mut self` | はい | 変更可能参照 |
  | provider | `Arc<dyn ProjectResolutionProvider>` | はい | 追加する共有所有プロバイダ |

- 戻り値: なし
- 使用例
  ```rust
  let mut registry = SimpleProviderRegistry::new();
  // provider は Arc<dyn ProjectResolutionProvider>
  registry.add(provider.clone());
  ```
- エッジケース
  - 重複追加の防止はしていない（必要ならコール側で制御）

4) SimpleProviderRegistry::active_providers（L34-L42）
- 目的と責務
  - 設定（`Settings`）に基づいて有効なプロバイダだけを抽出
- アルゴリズム（ステップ分解）
  1. `self.providers.iter()`で全件走査（L38-L39）
  2. 各要素に対し`p.is_enabled(settings)`で有効判定（L40）
  3. trueのみ`.cloned()`で`Arc`複製（L41）
  4. `.collect()`で`Vec<Arc<...>>`にまとめて返却（L42）
- 引数

  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | `&self` | はい | レジストリ参照 |
  | settings | `&crate::config::Settings` | はい | 有効判定に用いる設定 |

- 戻り値

  | 型 | 説明 |
  |----|------|
  | `Vec<Arc<dyn ProjectResolutionProvider>>` | 有効なプロバイダ一覧 |

- 使用例
  ```rust
  // settings は &crate::config::Settings
  let actives = registry.active_providers(settings);
  for p in actives {
      // 有効プロバイダのみ
  }
  ```
- エッジケース
  - すべて無効の場合は空ベクタ
  - `is_enabled`の実装次第（このチャンクでは不明）

5) SimpleProviderRegistry::default（L16-L20）
- 目的と責務
  - `Default`トレイト実装により`SimpleProviderRegistry::default()`で空レジストリを生成
- アルゴリズム
  - `Self::new()`へ委譲（L18）
- 使用例
  ```rust
  let registry = SimpleProviderRegistry::default();
  ```

## Walkthrough & Data Flow

- 登録フェーズ
  - 呼び出し元が`Arc<dyn ProjectResolutionProvider>`を作成し、`SimpleProviderRegistry::add`（L29-L31）で内部`Vec`に追加
  - 所有権: `Arc`のクローンは増えない。`add`は所有権をレジストリに移す（値の移動: add(L29)の引数で受け取り、ベクタに格納）
- 列挙フェーズ
  - `ResolutionProviderRegistry::providers`（L7-L9、実装 L46-L49）で読み取り専用スライスを取得して走査可能
- 有効抽出フェーズ
  - `active_providers`（L34-L42）で全プロバイダを`Settings`でフィルタリング
  - `Arc`を`cloned()`して返却するため、レジストリ外でもライフタイム管理が容易

データフローは直線的で、外部依存は`ProjectResolutionProvider::is_enabled`による判定のみです。

## Complexity & Performance

- `new`: 時間O(1), 空間O(1)
- `add`: 平均時間O(1)、ベクタ容量拡張時O(n)。空間は要素数に比例（O(1)増分）
- `providers`: 時間O(1)、空間O(1)
- `active_providers`: 時間O(n)、空間O(k)（k=有効件数、最悪O(n)）

ボトルネック:
- プロバイダ数が非常に多い場合、`active_providers`が線形で走査。現状スケール上の問題は少ないが、`is_enabled`が重い計算やI/Oを含む場合は全体が律速される可能性あり。

実運用負荷要因:
- `is_enabled`の実装次第でI/O（設定読み込み等）やネットワーク依存があり得るが、このチャンクでは不明。

## Edge Cases, Bugs, and Security

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空レジストリ | providers空 | 空のスライス/ベクタ返却 | new(L23-L27), providers(L46-L49), active_providers(L34-L42) | 良好 |
| すべて無効 | is_enabledが全てfalse | 空ベクタ返却 | active_providers(L34-L42) | 良好 |
| 重複登録 | 同一providerをadd複数回 | 重複要素が存在 | add(L29-L31) | 要改善 |
| 大量件数 | nが非常に大 | 線形時間でフィルタ | active_providers(L34-L42) | 良好 |
| Settings不整合 | 不正設定 | is_enabled次第 | 不明 | 不明 |

セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow: 該当なし（Rust安全なAPIのみ）
  - Use-after-free: `Arc`により防止される（`cloned()`で参照カウント増加、L41）
  - Integer overflow: 該当なし
- インジェクション
  - SQL/Command/Path traversal: 該当なし（このチャンクでは外部I/Oなし）
- 認証・認可
  - 該当なし（このチャンクでは認証文脈なし）
- 秘密情報
  - Hard-coded secrets: 該当なし
  - Log leakage: ログ出力なし（監査性不足の観点では後述）
- 並行性
  - Race condition: `add`が`&mut self`であり同時書き込みは型で防止
  - Deadlock: 該当なし（ロック未使用）
  - 重要リスク: `Arc<dyn ProjectResolutionProvider>`に`Send + Sync`境界がないため、レジストリやプロバイダを**多スレッドで安全に共有できる保証がない**。`SimpleProviderRegistry`自体の`Send/Sync`導出も未保証。

## Design & Architecture Suggestions

- 並行利用の安全性強化
  - `Arc<dyn ProjectResolutionProvider + Send + Sync>`へ変更し、トレイト`ProjectResolutionProvider`が`Send + Sync`であることを要求する。これにより、レジストリをスレッド間で安全に共有可能。
- API改善
  - 重複登録防止: `add`時に同一性（ID/型/ポインタアドレス等）をチェックする仕組みを導入。
  - 取り外しAPI: `remove`や`clear`を追加して動的管理性を向上。
  - スナップショットAPI: `active_providers`の結果をキャッシュする（Settingsが不変の間）などの最適化ポイント。
- インターフェース分離
  - 読み取り専用インターフェース（`ResolutionProviderRegistry`）と書き込みAPI（`add`）の分離は良い。さらに`&dyn ResolutionProviderRegistry`を渡す設計を徹底するとテスト容易性が増す。
- エラーモデル
  - `add`で重複検知時に`Result<(), Error>`を返すなど、失敗可能性を型で表現。

## Testing Strategy (Unit/Integration) with Examples

ユニットテスト観点
- new/Defaultの初期状態
- addによる登録数増加
- active_providersのフィルタ動作（有効/無効の混在、全有効、全無効）
- 重複登録時の振る舞い（現状は許容）

このチャンクでは`ProjectResolutionProvider`と`Settings`の詳細が不明のため、擬似的なスタブを用いたテスト例を示します（コンパイル保証はこのチャンクでは不明）。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // 擬似スタブ: 実際のトレイト定義に合わせて修正が必要
    struct FakeProvider {
        enabled: bool,
    }

    impl super::super::provider::ProjectResolutionProvider for FakeProvider {
        fn is_enabled(&self, _settings: &crate::config::Settings) -> bool {
            self.enabled
        }
        // 他メソッドがあれば必要に応じてスタブ化
    }

    #[test]
    fn new_registry_is_empty() {
        let registry = SimpleProviderRegistry::new();
        assert_eq!(ResolutionProviderRegistry::providers(&registry).len(), 0);
    }

    #[test]
    fn add_increases_count() {
        let mut registry = SimpleProviderRegistry::new();
        let p = Arc::new(FakeProvider { enabled: true }) as Arc<dyn super::super::provider::ProjectResolutionProvider>;
        registry.add(p);
        assert_eq!(ResolutionProviderRegistry::providers(&registry).len(), 1);
    }

    #[test]
    fn active_filters_correctly() {
        let mut registry = SimpleProviderRegistry::new();
        let p1 = Arc::new(FakeProvider { enabled: true }) as Arc<dyn super::super::provider::ProjectResolutionProvider>;
        let p2 = Arc::new(FakeProvider { enabled: false }) as Arc<dyn super::super::provider::ProjectResolutionProvider>;
        registry.add(p1.clone());
        registry.add(p2.clone());

        // Settingsの具体が不明のためダミーを使用（ここは適宜修正）
        let settings = unsafe { std::mem::zeroed::<crate::config::Settings>() }; // 実際は適切に構築すること

        let actives = registry.active_providers(&settings);
        assert_eq!(actives.len(), 1);
        // 同一Arcであることを確認（アドレス比較など）
    }
}
```

統合テスト観点
- 実際の`Settings`を読み込み、複数実装の`ProjectResolutionProvider`を登録して有効化判定の連携を検証。

## Refactoring Plan & Best Practices

- 型境界の強化
  - `Arc<dyn ProjectResolutionProvider + Send + Sync>`へ変更し、並行安全性を明示。
- データ構造の選択
  - 重複回避が必要なら`Vec`→`IndexSet`（順序維持+重複排除）や`HashSet`（同値判定に`Arc`ポインタアドレス/ID）を検討。
- APIの一貫性
  - `providers()`は不変スライス返却で良いが、使用者が有効/無効を意識せず使えるよう`iter_active(settings)`などのイテレータ版も追加可能。
- ドキュメンテーション
  - `ProjectResolutionProvider::is_enabled`の契約（副作用なし、計算コストの目安）を明記。
- エラー設計
  - `add`における重複や不正状態を`Result`で返す方針に整備。

## Observability (Logging, Metrics, Tracing)

- ログ
  - `add`時にプロバイダ登録ログ（ID/型名）を`debug`レベルで記録
  - `active_providers`呼び出し時に「入力Settingsの概要」「有効件数」を`trace/debug`で記録
- メトリクス
  - 登録総数、アクティブ総数、`is_enabled`判定時間のヒストグラム
- トレーシング
  - `active_providers`にスパンを付与し、各`is_enabled`呼び出しを子スパンで可視化（I/Oを含む場合便利）

このチャンクにはロギング・メトリクス・トレーシングの実装は「該当なし」。

## Risks & Unknowns

- 並行性の型保証不足
  - `Send/Sync`境界がないため、**多スレッド共有時の安全性が保証されない**。上位設計で単一スレッド利用に限定しているなら問題は少ないが、明示的に境界を付与することを推奨。
- `ProjectResolutionProvider`トレイトの詳細不明
  - `is_enabled`以外の契約・副作用・スレッド安全性は「不明」。
- `Settings`の構造不明
  - 有効判定の要件・性能への影響は「不明」。
- 重複登録の影響
  - 現状重複が発生しうるため、**同じプロバイダの多重実行**や順序依存の副作用が起きる可能性。
- ライフサイクル管理
  - `Arc`の共有で破棄は安全だが、**プロバイダのシャットダウン/リソース解放**の契約はこのチャンクでは「不明」。