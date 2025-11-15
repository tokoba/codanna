# parsers\typescript\test_jsx_uses.rs Review

## TL;DR

- 目的: **TypeScriptParser::find_uses** が JSX/TSX でのコンポーネント使用関係を正しく検出するかをテストする
- 主要な公開API: このファイル自体の公開APIはなし。外部APIとして **TypeScriptParser::new**, **LanguageParser::find_uses** を使用（詳細は不明）
- 複雑箇所: 使用結果を絞り込み・検証するフィルタ（大文字/小文字の判定）が簡潔だが、空文字やUnicodeケースでのパニック・誤判定の潜在リスクあり
- 重大リスク: `component.chars().next().unwrap()` による空文字列でのパニック可能性、`expect` による初期化失敗時の強制パニック
- セキュリティ観点: テストコードのため外部入力は限定的。インジェクションなどの懸念は基本的に無し
- パフォーマンス: 本テスト内の処理はベクタの線形走査程度（O(U)）。パーサ自体の計算量はこのチャンクでは不明
- 改善点: 重複ロジックのヘルパー化、空文字/Unicode対策、表現力の高いアサーションユーティリティ導入

## Overview & Purpose

このファイルは Rust のテストモジュールで、TypeScript/TSX コードに含まれる JSX コンポーネント使用を抽出するパーサの挙動を検証します。具体的には以下の観点をテストしています。

- React コンポーネント（先頭が大文字の要素）利用の検出
- HTML 要素（先頭が小文字のタグ）利用の無視
- 自閉（self-closing）コンポーネントの検出

いずれのテストも、`TypeScriptParser::new()` でパーサを生成し、`find_uses(&str)` を実行して得られる使用関係 `uses` をフィルタ・アサートしています。

このチャンクではパーサの内部仕様・戻り値型の詳細は提示されておらず、不明です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | tests | private（#[cfg(test)] 内） | テスト群の定義 | Low |
| Function | test_jsx_component_usage_tracking | private（#[test]） | JSXの大文字コンポーネント使用検出を確認 | Low |
| Function | test_jsx_ignores_lowercase_elements | private（#[test]） | 小文字HTML要素を無視することを確認 | Low |
| Function | test_jsx_self_closing_components | private（#[test]） | 自閉コンポーネント使用検出を確認 | Low |

### Dependencies & Interactions

- 内部依存
  - 各テストは独立しており、相互呼び出しなし
  - 共通の外部API（TypeScriptParser::new と find_uses）を使用
- 外部依存（表）

  | 依存 | 種別 | 用途 | 備考 |
  |------|------|------|------|
  | codanna::parsing::LanguageParser | Trait | パーサ共通インターフェース（find_usesの由来） | 具体定義はこのチャンクでは不明 |
  | codanna::parsing::typescript::TypeScriptParser | Struct/Type | TypeScript/TSX用パーサ実装の生成 | new() と find_uses(&str) の使用のみ確認 |

- 被依存推定
  - このテストは TypeScript パーサの機能回帰検証として、CI や開発中の品質保証で利用されることが想定されます
  - 他モジュールから直接参照されることはない（#[cfg(test)] 範囲）

## API Surface (Public/Exported) and Data Contracts

このファイル自身の公開APIは存在しません（#[cfg(test)] 内のテストのみ）。外部APIの仕様はこのチャンクでは不明です。

外部API利用一覧（参考）

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| TypeScriptParser::new | 不明（Resultを返すことのみ判別可） | パーサの生成 | 不明 | 不明 |
| LanguageParser::find_uses | 不明（&strを受けVecを返すことのみ判別可） | コード中の「使用関係」を抽出 | 不明 | 不明 |

テスト関数一覧（このファイル内）

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| test_jsx_component_usage_tracking | fn test_jsx_component_usage_tracking() | JSXコンポーネント（Button）の使用検出 | O(U) | O(U) |
| test_jsx_ignores_lowercase_elements | fn test_jsx_ignores_lowercase_elements() | 小文字HTML要素（div, span）の非検出 | O(U) | O(U) |
| test_jsx_self_closing_components | fn test_jsx_self_closing_components() | 自閉コンポーネント（CustomComponent）の検出 | O(U) | O(U) |

各APIの詳細（テスト関数のみ記述）

1) test_jsx_component_usage_tracking
- 目的と責務
  - React/JSXにおける大文字コンポーネントの使用を `find_uses` が検出できるか確認
- アルゴリズム（ステップ）
  - TSXコード文字列を用意（MyPage と AnotherComponent が Button を使用）
  - パーサ生成（new + expect）
  - find_uses を実行して結果のベクタを取得
  - 結果をログ出力
  - uses が空でないことをアサート
  - component が "Button" の要素をフィルタし、空でないことをアサート
- 引数

  | 名前 | 型 | 意味 |
  |------|----|------|
  | なし | - | テスト関数のため引数なし |

- 戻り値

  | 型 | 意味 |
  |----|------|
  | () | テストの成否（パニックで失敗） |

- 使用例（抜粋）

```rust
#[test]
fn test_jsx_component_usage_tracking() {
    let code = r#"
import React from 'react';
import { Button } from './button';

export function MyPage() {
  return (
    <div>
      <Button>Click me</Button>
    </div>
  );
}

export function AnotherComponent() {
  return <Button>Another</Button>;
}
"#;

    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);

    /* ... 省略 ... */

    let button_uses: Vec<_> = uses
        .iter()
        .filter(|(_, component, _)| *component == "Button")
        .collect();

    assert!(
        !button_uses.is_empty(),
        "Should find at least one usage of Button component"
    );
}
```

- エッジケース
  - uses が空（パーサが検出失敗）：テストが失敗
  - "Button" が未検出：テストが失敗
  - 返却タプルの component が空文字：後続テストには影響なし（この関数では空文字扱いなし）

2) test_jsx_ignores_lowercase_elements
- 目的と責務
  - 小文字のHTMLタグ（div, span）が使用関係として記録されないことの確認
- アルゴリズム
  - TSXコード文字列（div, span使用）を用意
  - パーサ生成と find_uses 実行
  - uses を `.iter()` で走査し、`component.chars().next().unwrap().is_lowercase()` で先頭小文字のものを抽出
  - 結果が空であることをアサート
- 引数・戻り値は上と同様
- 使用例（全文引用）

```rust
#[test]
fn test_jsx_ignores_lowercase_elements() {
    let code = r#"
export function Component() {
  return (
    <div>
      <span>Text</span>
    </div>
  );
}
"#;

    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);

    // Should NOT track lowercase HTML elements (div, span)
    let html_uses: Vec<_> = uses
        .iter()
        .filter(|(_, component, _)| component.chars().next().unwrap().is_lowercase())
        .collect();

    assert!(
        html_uses.is_empty(),
        "Should not track lowercase HTML elements, only uppercase React components"
    );
}
```

- エッジケース
  - component が空文字の場合、`unwrap()` でパニックする可能性
  - Unicode の大小文字特性に依存するため、ASCII以外の先頭文字で期待と異なる判定の可能性

3) test_jsx_self_closing_components
- 目的と責務
  - 自閉タグ `<CustomComponent />` の使用検出が正しく行われることの確認
- アルゴリズム
  - TSXコード文字列（CustomComponent の自閉使用）を用意
  - パーサ生成と find_uses 実行
  - uses を `.iter()` で走査し、`component == "CustomComponent"` を抽出
  - 1件のみであることを `assert_eq!(custom_uses.len(), 1)` でアサート
- 使用例（全文引用）

```rust
#[test]
fn test_jsx_self_closing_components() {
    let code = r#"
export function App() {
  return <CustomComponent />;
}
"#;

    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);

    println!("Found {} uses:", uses.len());
    for (from, to, range) in &uses {
        println!("  {} uses {} at line {}", from, to, range.start_line);
    }

    let custom_uses: Vec<_> = uses
        .iter()
        .filter(|(_, component, _)| *component == "CustomComponent")
        .collect();

    assert_eq!(
        custom_uses.len(),
        1,
        "Should find self-closing component usage"
    );
}
```

- エッジケース
  - 複数回使用された場合（同関数内・別関数内）、`len()==1` のアサートが壊れる（テスト意図的に限定ケース）

データコントラクト（find_uses の戻り値）
- このチャンクでは型詳細は不明。使用箇所から、`uses` は反復可能なコレクションで、要素は `(from, to, range)` のタプル（型詳細不明）であると推測されます
- `range` は少なくとも `start_line` フィールドを持つ構造体である模様（出力使用より）

## Walkthrough & Data Flow

共通フロー（各テスト）
1. TS/TSX コードの文字列を構築（r#"... "# の raw 文字列）
2. `TypeScriptParser::new()` を呼び出し、`expect("Failed to create parser")` で生成失敗時は即パニック
3. `parser.find_uses(code)` を呼び出し、使用関係のコレクション（`uses`）を取得
4. デバッグ出力（println）で `uses.len()` と各要素の `(from, to, range.start_line)` を表示（任意）
5. テスト主題に応じたフィルタリング・アサーション
   - 大文字コンポーネント（"Button"）存在の確認
   - 小文字HTML要素の非検出確認
   - 自閉コンポーネント（"CustomComponent"）が1件検出されることの確認

データの流れ
- 入力: &str のコード
- 処理: find_uses による解析（詳細不明）
- 出力: uses: 反復可能なタプルのコレクション
- 二次処理: `.iter().filter(...)` による選別、長さ確認、アサーション

## Complexity & Performance

- テスト内処理の計算量
  - フィルタリングや走査は `uses.len()` を U とすると O(U)
  - 追加メモリはフィルタ結果のベクタ生成分で O(U)
- ボトルネック
  - 実質 `find_uses` の内部が支配的だが、このチャンクでは不明
- スケール限界
  - 巨大な `uses` に対しても線形走査のみのためテスト自体の負荷は小さい
- 実運用負荷要因
  - I/O/ネットワーク/DB操作は存在しない（テスト内）
  - パーサの語彙/構文複雑性次第で `find_uses` の時間/メモリが増える可能性（不明）

## Edge Cases, Bugs, and Security

セキュリティチェックリスト観点（テストコードとしての評価）
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（安全なRust、unsafe不使用）
- インジェクション
  - SQL/Command/Path traversal: 該当なし（外部コマンド・DB操作なし）
- 認証・認可
  - 権限チェック漏れ / セッション固定: 該当なし
- 秘密情報
  - Hard-coded secrets / Log leakage: 該当なし（テストデータのみ）
- 並行性
  - Race condition / Deadlock: 該当なし（単一スレッドのテスト）

潜在的な不具合・注意点
- unwrapのパニックリスク
  - `component.chars().next().unwrap().is_lowercase()` は component が空文字の場合にパニック
  - find_uses が空名の要素を返す設計であればテストが不安定
  - 対策: `if let Some(c) = component.chars().next() { c.is_lowercase() } else { false }`
- 期待件数が固定のアサート
  - `assert_eq!(custom_uses.len(), 1)` は同名コンポーネントの複数使用時に失敗
  - 対策: 「1件以上」や「特定位置」など、要件に即した柔軟な検証へ
- 大小文字判定の曖昧性
  - `.is_lowercase()` は Unicode に依存。ASCII以外の先頭文字（例: "ÉxampleComponent"）で期待がずれる可能性
  - 対策: JSX慣習に合わせて `.is_ascii_lowercase()` を選ぶなど要件定義の明確化

エッジケース詳細（このチャンクの実装・状態不明点を明記）

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空コード文字列 | "" | uses が空である | 不明（このチャンクでは find_uses 未定義） | 不明 |
| 空の component 名 | uses に ("from","",range) が含まれる | 小文字要素判定時に安全にスキップ | unwrapによるパニック可能性 | 要改善 |
| 小文字HTMLタグを含む | `<div><span>Text</span></div>` | uses に含まれない | フィルタで is_lowercase を確認 | テストで検証済（実行結果はこのチャンクでは不明） |
| 自閉コンポーネント単一使用 | `<CustomComponent />` | 1件検出 | len==1 をアサート | テストで検証済（実行結果はこのチャンクでは不明） |
| 同名コンポーネント複数使用 | `<CustomComponent /><CustomComponent />` | 2件以上検出 | 1件に固定したアサーションでは失敗 | 要テスト追加 |

## Design & Architecture Suggestions

- テストヘルパーの導入
  - パーサ生成・find_uses 実行・フィルタの重複をヘルパー関数に抽出
  - 例: `fn find_component_uses(code: &str, name: &str) -> Vec<Use>` のようなユーティリティ（Use型は不明のためジェネリック/タプルで）
- アサーションユーティリティ
  - 「HTMLタグを含まない」「コンポーネントXがN件以上」などのドメイン固有アサート関数でテスト意図を明確化
- 大文字/小文字判定の要件定義
  - JSXの慣習に基づくASCII前提か、Unicodeも許容するかを仕様化
  - 判定ロジックの共通化（is_ascii_uppercase/is_ascii_lowercase など）
- 柔軟な件数アサート
  - 使用件数が要件次第で増減しうるため、「>=1」や「特定行に存在」などに拡張

## Testing Strategy (Unit/Integration) with Examples

- 追加ユニットテスト案
  - ネスト・複合ケース
    - 子孫要素両方にコンポーネントがある場合の検出

```rust
#[test]
fn test_nested_components_detection() {
    let code = r#"
export function Page() {
  return (
    <Layout>
      <Header />
      <Content>
        <Card><Title /></Card>
      </Content>
    </Layout>
  );
}
"#;
    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);
    let names = ["Layout", "Header", "Content", "Card", "Title"];
    for n in names {
        assert!(uses.iter().any(|(_, c, _)| *c == n), "missing {}", n);
    }
}
```

  - import エイリアス

```rust
#[test]
fn test_import_alias_component_usage() {
    let code = r#"
import { Button as Btn } from './button';
export function Page() { return <Btn/>; }
"#;
    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);
    assert!(uses.iter().any(|(_, c, _)| *c == "Btn"));
}
```

  - default export と名前付きの混在

```rust
#[test]
fn test_default_export_component_usage() {
    let code = r#"
import Button from './button';
export function Page() { return <Button/>; }
"#;
    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);
    assert!(uses.iter().any(|(_, c, _)| *c == "Button"));
}
```

  - フラグメントや式内使用

```rust
#[test]
fn test_fragment_and_expression_usage() {
    let code = r#"
export function Page() {
  const Comp = Math.random() > 0.5 ? A : B;
  return <>
    <Comp/>
    {true && <A/>}
    {false || <B/>}
  </>;
}
"#;
    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);
    for n in ["A","B","Comp"] {
        assert!(uses.iter().any(|(_, c, _)| *c == n));
    }
}
```

- 負荷/回帰テスト
  - 大規模ファイル（多数コンポーネント）の解析時間・メモリ測定は別途必要（このチャンクでは不明）
  - HTMLタグの網羅（div, span, a, img 等）を含む誤検出防止テスト

## Refactoring Plan & Best Practices

- 重複の削減
  - パーサ生成と find_uses の呼び出しを共通関数へ

```rust
fn run_uses(code: &str) -> Vec<(String, String, Range)> {
    // Range 型やタプル型はこのチャンクでは不明。擬似署名として表現。
    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    parser.find_uses(code)
}
```

- 安全な文字判定

```rust
fn is_lowercase_first_ascii(s: &str) -> bool {
    s.as_bytes().first().map(|b| b.is_ascii_lowercase()).unwrap_or(false)
}
```

- アサーションの表現力強化

```rust
fn assert_uses_contains(uses: &[(String, String, Range)], name: &str) {
    assert!(
        uses.iter().any(|(_, c, _)| c == name),
        "expected to find use of component: {}",
        name
    );
}
```

- ログの抑制
  - `println!` はテストノイズになりがち。必要時のみ `RUST_LOG` で制御できる `log`/`tracing` を活用

## Observability (Logging, Metrics, Tracing)

- 現状
  - テスト内で `println!` により検出結果を出力
- 推奨
  - `log` または `tracing` を導入し、`env_logger`/`tracing-subscriber` 経由で必要時のみ表示
  - 解析メトリクス（uses 件数、検出率など）を集約する仕組みはテスト外のベンチ/計測コードで実施（このチャンクでは未導入）
- トレース
  - パーサ内部のトレースはこのチャンクでは不明。必要に応じて `debug` レベルでノード探索・JSXノード検出の詳細ログを追加

## Risks & Unknowns

- 不明点
  - `TypeScriptParser::new` の正確なシグネチャ・エラー型
  - `LanguageParser::find_uses` の戻り値型（タプル要素の具体型、`Range` の詳細）
  - コンポーネント使用検出の厳密な仕様（import 解析の有無、別名・default export 対応など）
- リスク
  - 大文字/小文字判定の仕様が環境依存（Unicode）で期待とズレる可能性
  - 仕様拡張（複数使用、条件分岐、動的コンポーネント名）時のテスト不足
  - `expect` による初期化失敗のパニックは、CI環境差異で発生時に原因特定が難しい可能性（ただしテストでは一般的）
- 対応策
  - 仕様ドキュメント化（使用関係の定義範囲）
  - 追加テスト群の整備（上述のテスト戦略）
  - unwrapの回避、アサーションユーティリティの導入による安定性向上