## integration\test_external_import_resolution.rs Review

## TL;DR

- 目的: 外部インポートを検出し、同名のローカル記号への誤解決を防ぐための解決層ロジックを統合テストで検証する。
- 主要API利用: RustResolutionContext の populate_imports, register_import_binding, add_symbol, is_external_import, resolve を使用。
- 複雑箇所: 外部インポート検出と通常の名前解決の併用。特に「同名のローカル記号が存在する場合に、呼び出し側が is_external_import を先に確認すべき」というプロトコル。
- 重大リスク: 呼び出し側が is_external_import を確認せず resolve 結果を利用すると、外部シンボル参照がローカルに誤リンクされる。
- 互換性・エッジ: エイリアス（as）付き外部インポート、複数外部インポート、内部インポートと外部の判別、インポート未設定時の挙動を網羅。
- セキュリティ・安全性: unsafe不使用、並行性なし。FileId::new/ SymbolId::new の unwrap が不正値で panic しうるが、テスト前提では許容。
- 未検証領域: glob インポート（is_glob）、型限定（is_type_only）、ResolutionScope はこのチャンクには現れない/未使用。

## Overview & Purpose

このファイルは、外部インポート検出機構の正当性を検証する統合テスト群を含む。対象は codanna の解決コンテキスト RustResolutionContext で、外部インポート（例: indicatif::ProgressBar）がファイル内のローカル記号（例: struct ProgressBar）と同名の場合に、誤ってローカルへ解決してしまう事象を回避するため、「呼び出し側が is_external_import(name) を先に確認する」という契約を強調・検証している。

ポイント:
- 外部インポートは ImportOrigin::External として ImportBinding に登録。
- 同名ローカル記号は add_symbol で登録されるが、resolve は通常通りローカルに解決する可能性がある。
- is_external_import を先に確認することで、呼び出し側はローカル解決を意図的にスキップできる。

テストは Tantivy を用いず、純粋に解決レイヤーの挙動のみを検証する。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | register_binding | private (テストファイル内) | ImportBinding を組み立てて RustResolutionContext に登録 | Low |
| Function(test) | test_external_import_detection_prevents_local_resolution | private | 外部インポートが存在する同名ローカル記号のケースで、is_external_import が true になることを確認 | Med |
| Function(test) | test_internal_import_not_flagged_as_external | private | 内部インポートが external と誤検出されないことを確認 | Low |
| Function(test) | test_aliased_external_import_detection | private | エイリアス付き外部インポートの alias および元名で external 検出されることを確認 | Med |
| Function(test) | test_multiple_external_imports | private | 複数外部インポートの検出と非インポート名の非外部判定を確認 | Low |
| Function(test) | test_external_import_same_name_as_local_symbol | private | バグの主因シナリオ（同名ローカル存在）で is_external_import による誤解決防止を確認 | Med |
| Function(test) | test_no_imports_means_no_external_symbols | private | インポートが無い場合は何も external にならないことを確認 | Low |
| External Struct | RustResolutionContext | 外部依存 | 名前解決の管理、インポート情報・シンボル登録・問い合わせを提供 | 不明 |
| External Struct | Import | 外部依存 | インポートのメタデータ（path, alias, file_id, is_glob, is_type_only） | 不明 |
| External Struct | ImportBinding | 外部依存 | 本インポートから露出される名前・起源・解決シンボル | 不明 |
| External Enum | ImportOrigin | 外部依存 | インポート起源（External/Internal） | 不明 |
| External Enum | ScopeLevel | 外部依存 | シンボルのスコープレベル（Module等） | 不明 |
| External Newtype | FileId | 外部依存 | ファイル識別子。new() は Option を返す（unwrap あり） | 不明 |
| External Newtype | SymbolId | 外部依存 | シンボル識別子。new() は Option を返す（unwrap あり） | 不明 |

Dependencies & Interactions
- 内部依存:
  - 各テスト関数 → register_binding（補助関数）
  - 各テスト関数 → RustResolutionContext のメソッド（populate_imports, register_import_binding, add_symbol, is_external_import, resolve）
- 外部依存（クレート・モジュール）:

| 依存 | 用途 |
|-----|------|
| codanna::parsing::resolution::{ImportBinding, ImportOrigin, ResolutionScope} | ImportBinding/ImportOrigin の生成と登録（ResolutionScope は未使用） |
| codanna::parsing::rust::resolution::RustResolutionContext | 名前解決コンテキスト |
| codanna::parsing::{Import, ScopeLevel} | インポート情報とスコープ指定 |
| codanna::{FileId, SymbolId} | ID生成（new().unwrap()） |

- 被依存推定:
  - この統合テストファイルはテストハーネスからのみ実行される。プロダクションコードはこのファイルに依存しないが、RustResolutionContext の正しさ保証に寄与。

## API Surface (Public/Exported) and Data Contracts

このファイル自身の公開APIは存在しない（テスト関数と補助関数のみ）。ただし、本テストが前提とする外部API（RustResolutionContext など）と、テスト内補助関数を以下に整理する。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| register_binding | fn register_binding(context: &mut RustResolutionContext, import: &Import, exposed_name: &str, origin: ImportOrigin, resolved: Option<SymbolId>) | ImportBinding を生成してコンテキストに登録 | O(1) | O(1) |
| RustResolutionContext::populate_imports | fn populate_imports(imports: &[Import]) | インポートのメタ情報をコンテキストに取り込む | 不明 | 不明 |
| RustResolutionContext::register_import_binding | fn register_import_binding(binding: ImportBinding) | 露出名と起源・解決結果のバインディング登録 | 不明 | 不明 |
| RustResolutionContext::add_symbol | fn add_symbol(name: String, id: SymbolId, level: ScopeLevel) | ローカルシンボルの登録 | 不明 | 不明 |
| RustResolutionContext::is_external_import | fn is_external_import(name: &str) -> bool | 指定名が外部インポート由来か判定 | 不明 | 不明 |
| RustResolutionContext::resolve | fn resolve(name: &str) -> Option<SymbolId> | 名前からローカルシンボルに解決 | 不明 | 不明 |

各APIの詳細説明

1) register_binding
- 目的と責務
  - Import と露出名（exposed_name）をもとに ImportBinding を構築し、RustResolutionContext に登録するユーティリティ。
- アルゴリズム（ステップ分解）
  1. import を clone して ImportBinding に格納。
  2. exposed_name, origin, resolved_symbol を設定。
  3. context.register_import_binding(binding) を呼ぶ。
- 引数

| 引数名 | 型 | 説明 |
|--------|----|------|
| context | &mut RustResolutionContext | 登録先の解決コンテキスト |
| import | &Import | インポートメタデータ |
| exposed_name | &str | ファイル内から露出される名前（ショート名やフルパス名） |
| origin | ImportOrigin | External または Internal |
| resolved | Option<SymbolId> | 既に解決済みならシンボルID、未解決なら None |

- 戻り値

| 型 | 説明 |
|----|------|
| () | なし |

- 使用例

```rust
let import = Import { /* ... */ };
register_binding(&mut context, &import, "ProgressBar", ImportOrigin::External, None);
```

- エッジケース
  - exposed_name の重複登録: コンテキスト側の上書き/併合挙動はこのチャンクには現れない。
  - import.clone によるコスト: パス文字列を含むため O(len(path)) のコピーが走るが、テストでは許容。

2) RustResolutionContext::is_external_import
- 目的と責務
  - 指定名が外部インポート由来かどうかのブール判定を返し、誤解決防止のためのガードとして機能。
- アルゴリズム
  - このチャンクには現れない（内部データ構造は不明）。
- 引数

| 引数名 | 型 | 説明 |
|--------|----|------|
| name | &str | 照会するシンボル名（ショート名/エイリアス/フルパス想定） |

- 戻り値

| 型 | 説明 |
|----|------|
| bool | 外部インポート由来なら true |

- 使用例

```rust
assert!(context.is_external_import("ProgressBar"));
assert!(context.is_external_import("PBar")); // alias
```

- エッジケース
  - フルパス指定 "indicatif::ProgressBar" を外部と見なすか: テストでは登録後に true を確認。
  - 未登録名: false になる（test_multiple_external_imports より）。

3) RustResolutionContext::resolve
- 目的と責務
  - 名前をローカルシンボルへ解決する。外部インポート検出は別途 is_external_import が担うため、resolve 自体はローカル優先で返す可能性がある。
- アルゴリズム
  - このチャンクには現れない。
- 引数

| 引数名 | 型 | 説明 |
|--------|----|------|
| name | &str | 解決対象名 |

- 戻り値

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 成功時はシンボルID、失敗時 None |

- 使用例

```rust
let id = context.resolve("helper");
assert_eq!(id, Some(internal_symbol_id));
```

- エッジケース
  - 外部インポートと同名ローカル: resolve はローカルを返すが、呼び出し側で is_external_import を先に確認すべき（test_external_import_detection_prevents_local_resolution）。

4) RustResolutionContext::populate_imports, register_import_binding, add_symbol
- 目的と責務
  - インポート情報の取り込み、露出名と起源のバインディング、ローカル記号の登録。
- 引数/戻り値
  - このチャンクには現れないが、使用方法から型は上表の通り。
- 使用例

```rust
context.populate_imports(&[external_import]);
context.register_import_binding(ImportBinding { /* ... */ });
context.add_symbol("ProgressBar".to_string(), local_symbol_id, ScopeLevel::Module);
```

- エッジケース
  - 重複シンボル/重複バインディング: 挙動は不明。

## Walkthrough & Data Flow

全テストの基本パターン:
1. FileId を生成（FileId::new(1).unwrap()）。
2. RustResolutionContext を作成。
3. Import を作成し、populate_imports で登録。
4. register_binding（補助）で ImportBinding を登録（ショート名、フルパス、必要に応じてエイリアス）。
5. 必要なら add_symbol でローカル記号（SymbolId）を登録。
6. is_external_import(name) で外部インポート判定。
7. resolve(name) でローカル解決の確認（外部であれば呼び出し側は利用を避ける）。

代表例（同名ローカルと外部インポートの競合）:

```rust
#[test]
fn test_external_import_detection_prevents_local_resolution() {
    let file_id = FileId::new(1).unwrap();
    let mut context = RustResolutionContext::new(file_id);

    let external_import = Import {
        path: "indicatif::ProgressBar".to_string(),
        alias: None, file_id, is_glob: false, is_type_only: false,
    };
    context.populate_imports(std::slice::from_ref(&external_import));
    register_binding(&mut context, &external_import, "ProgressBar", ImportOrigin::External, None);

    let local_symbol_id = SymbolId::new(100).unwrap();
    context.add_symbol("ProgressBar".to_string(), local_symbol_id, ScopeLevel::Module);

    // 外部検出は true
    assert!(context.is_external_import("ProgressBar"));

    // ただし resolve はローカルを返す -> 呼び出し側でガードが必要
    assert_eq!(context.resolve("ProgressBar"), Some(local_symbol_id));
}
```

同名ローカルのメソッド呼び出し誤リンク防止例:

```rust
#[test]
fn test_external_import_same_name_as_local_symbol() {
    // 外部 indicatif::ProgressBar を登録
    // ローカル struct ProgressBar とメソッド new を登録
    let receiver_is_external = context.is_external_import("ProgressBar");
    if receiver_is_external {
        // 呼び出し側はローカル解決をスキップ
    } else {
        // 誤り: ローカル ProgressBar::new に誤リンクしうる
    }
    assert!(receiver_is_external);
}
```

上記のフローは、resolve の前に is_external_import を必ず確認するという契約を前提にしている（行番号はこのチャンクでは不明・関数名で根拠を示した）。

## Complexity & Performance

- 各テストの計算量:
  - populate_imports と register_import_binding の呼び出し回数に比例。ループ（test_multiple_external_imports）はインポート数 m に対して O(m)。
  - is_external_import, resolve の内部計算量はこのチャンクには現れないため不明。
- メモリ使用:
  - Import/ImportBinding/シンボル登録に応じて O(m + s)（m:インポート数、s:シンボル数）。文字列の clone が発生。
- 実運用負荷要因（想定）:
  - I/O/ネットワーク/DB は関与しない。純粋なメモリ内データ構造操作。
- スケール限界:
  - 大量のインポート・シンボルを扱う場合の is_external_import/resolve の効率は内部構造次第（HashMap/Trie 等不明）。

## Edge Cases, Bugs, and Security

エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 外部と同名ローカル | use indicatif::ProgressBar + struct ProgressBar | is_external_import("ProgressBar") = true。呼び出し側は resolve 結果を使わない | test_external_import_detection_prevents_local_resolution | ✅ テスト済 |
| 内部インポート | use crate::utils::helper | is_external_import("helper") = false。resolve はローカルID | test_internal_import_not_flagged_as_external | ✅ テスト済 |
| エイリアス外部インポート | use indicatif::ProgressBar as PBar | is_external_import("PBar") = true、"ProgressBar" も true | test_aliased_external_import_detection | ✅ テスト済 |
| 複数外部インポート | indicatif, serde, tokio | それぞれのショート名が true、未登録名は false | test_multiple_external_imports | ✅ テスト済 |
| インポート無し | なし | すべて false | test_no_imports_means_no_external_symbols | ✅ テスト済 |
| glob インポート | use foo::* | 不明 | このチャンクには現れない | 未検証 |
| 型限定（type-only） | use foo::Bar; is_type_only=true | 不明 | このチャンクには現れない | 未検証 |
| フルパス名判定 | "indicatif::ProgressBar" | true | 複数テストで登録 | ✅ テスト済 |

セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（標準安全 Rust、unsafe 不使用）。
  - String/clone: 正常範囲。過大入力はテスト対象外。
- インジェクション
  - SQL/Command/Path traversal: 該当なし（I/Oなし）。
- 認証・認可
  - 権限チェック漏れ/セッション固定: 該当なし。
- 秘密情報
  - ハードコード秘密/ログ漏洩: 該当なし。println! はデバッグ出力のみ。
- 並行性
  - Race condition/Deadlock: 該当なし（シングルスレッドテスト）。

Rust特有の観点
- 所有権
  - register_binding(import: &Import) 内で import.clone を実行。テストでは妥当。move は行われない。
- 借用
  - &mut RustResolutionContext をテスト内で排他的に使用。借用期間は関数スコープで完結。
- ライフタイム
  - 明示的ライフタイムなし。&str 引数（exposed_name）と String の所有が明確。
- unsafe 境界
  - unsafe ブロックは存在しない。
- 並行性・非同期
  - Send/Sync 境界や await は登場しない。このチャンクには現れない。
- エラー設計
  - FileId::new(...).unwrap(), SymbolId::new(...).unwrap を使用。無効な ID 値の場合 panic し得るが、テスト前提では許容。Result/Option の変換・From/Into はこのチャンクには現れない。
- panic 箇所
  - unwrap 呼び出し（各テスト関数冒頭）。入力を固定しており回避不能時は早期失敗を意図。

## Design & Architecture Suggestions

- 誤用防止のための API デザイン
  - 提案: RustResolutionContext::resolve のオプション引数や別メソッド（例: resolve_local_only / resolve_external_aware）を用意し、外部インポート該当名に対するローカル解決を内部で拒否/None を返すモードを提供すると、呼び出し側の is_external_import 前提が不要になり、誤用を防げる。
  - 代替: resolve が「もし is_external_import(name) なら None を返す」責務を担う設定フラグをコンテキストに持たせる。
- バインディング登録の一貫性
  - ショート名とフルパスを両方登録しているが、重複やオーバーライドのルールを統一する（外部コード側の仕様文書が必要）。
- 未使用 import の整理
  - ResolutionScope はこのファイルで未使用。use の削除で可読性向上。
- エイリアス処理の一元化
  - register_binding が alias, 元名, フルパスの3種を手動登録している。ヘルパーを拡張し、Import の alias/パスから自動的に複数バインディングを生成するユーティリティを作ると重複が減る。

## Testing Strategy (Unit/Integration) with Examples

既存テストは主要シナリオを網羅。追加推奨テスト:

- glob インポート
```rust
#[test]
fn test_glob_import_external_detection() {
    let file_id = FileId::new(1).unwrap();
    let mut context = RustResolutionContext::new(file_id);
    let import = Import {
        path: "foo::*".to_string(), alias: None, file_id,
        is_glob: true, is_type_only: false,
    };
    context.populate_imports(std::slice::from_ref(&import));
    // このチャンクには現れない: glob の具体的なバインディング方法
    assert!(context.is_external_import("Bar"), "globで露出したBarが外部扱い");
}
```

- type-only インポート
```rust
#[test]
fn test_type_only_import_detection() {
    let file_id = FileId::new(1).unwrap();
    let mut context = RustResolutionContext::new(file_id);
    let import = Import {
        path: "foo::TypeOnly".to_string(), alias: None, file_id,
        is_glob: false, is_type_only: true,
    };
    context.populate_imports(std::slice::from_ref(&import));
    // このチャンクには現れない: type-only の解決ポリシー
    assert!(context.is_external_import("TypeOnly"));
}
```

- エイリアスとローカル衝突
```rust
#[test]
fn test_alias_conflicts_with_local() {
    let file_id = FileId::new(1).unwrap();
    let mut context = RustResolutionContext::new(file_id);

    let import = Import { path: "ext::Thing".to_string(), alias: Some("T".to_string()), file_id, is_glob: false, is_type_only: false };
    context.populate_imports(std::slice::from_ref(&import));
    register_binding(&mut context, &import, "T", ImportOrigin::External, None);

    let local_id = SymbolId::new(42).unwrap();
    context.add_symbol("T".to_string(), local_id, ScopeLevel::Module);

    assert!(context.is_external_import("T"));
    assert_eq!(context.resolve("T"), Some(local_id), "resolveはローカルを返すが、呼び出し側は使用禁止");
}
```

- フルパス指定の判定強化
```rust
#[test]
fn test_full_path_detection() {
    let file_id = FileId::new(1).unwrap();
    let mut context = RustResolutionContext::new(file_id);
    let import = Import { path: "pkg::Sub::Item".to_string(), alias: None, file_id, is_glob: false, is_type_only: false };
    context.populate_imports(std::slice::from_ref(&import));
    register_binding(&mut context, &import, "pkg::Sub::Item", ImportOrigin::External, None);

    assert!(context.is_external_import("pkg::Sub::Item"));
    assert!(!context.is_external_import("pkg::Other"));
}
```

## Refactoring Plan & Best Practices

- ヘルパー関数の拡張
  - register_binding を rename して register_all_bindings(import) のようにし、alias/ショート名/フルパスを一括登録。
- 重複ロジックの削減
  - 各テストでの「ショート名抽出（rsplit）」や「二重登録」をヘルパーに委譲。
- エラー表示の改善
  - unwrap の代わりに expect を使用し、失敗時に明確なメッセージを付与。
- 定数化/共通化
  - "indicatif::ProgressBar" 等の文字列を定数にし、変更容易性と誤記防止を向上。
- 未使用 import の削除
  - ResolutionScope を削除。

## Observability (Logging, Metrics, Tracing)

- 現状: println! による手続き的ログ。テスト実行時の理解を助ける。
- 改善案:
  - assert メッセージをより具体的に（期待値・実際値を明示）。
  - ログ整形（タグ付け）でテストケース識別を強化。
  - 大量テスト時は env_logger 等でフィルタリング可能にする（このチャンクには現れない）。

## Risks & Unknowns

- 呼び出し側契約依存
  - resolve の前に is_external_import を必須チェックとする設計は誤用リスクがある。API での誤用防止が望ましい。
- 内部実装不明
  - RustResolutionContext のデータ構造（セット/マップ/トライ等）、衝突時の優先度、glob/type-only の扱いはこのチャンクには現れない。
- エイリアスとフルパスの競合ルール
  - 同名が複数バインディングされる時の最終判定・上書きルールが不明。
- パフォーマンス特性
  - is_external_import/resolve の計算量保証が不明。大規模プロジェクトでのスケール挙動は未評価。
- テストの前提値
  - FileId::new(...).unwrap, SymbolId::new(...).unwrap の有効範囲が不明。無効値が引数になると panic。テストでは固定値なので問題ないが、生成規則はこのチャンクには現れない。