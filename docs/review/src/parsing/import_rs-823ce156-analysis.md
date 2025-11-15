# import.rs Review

## TL;DR

- 本ファイルは、言語パーサがソースファイルから抽出した**import文の表現**を保持するための、単一の公開データ構造体 **Import**（pub）を提供。
- 公開フィールドは5つ（path, alias, file_id, is_glob, is_type_only）。構造体は `Debug` と `Clone` を導出。コアロジックはなく、純粋なデータコンテナ。
- 重要なパフォーマンス特性: `Clone` は内部 `String` の再割り当てを伴うため、サイズに比例してコストがかかる（O(|path| + |alias|)）。大量に複製されるワークロードでは注意。
- **安全性**: `unsafe` なし、所有型（String, Option<String>）でメモリ安全。並行性は `FileId` の特性に依存（`Send/Sync`）。
- **リスク**: フィールドがすべて `pub` のため、不正値（空のpath、不整形のpath、言語差異の曖昧さ）の代入を防げない。多言語（Rust/TypeScript）を1つの表現に押し込むための仕様不明点が存在。
- **不明点**: `FileId` の定義や生成方法、path表現の正規化規約、複合インポート（例: `{A,B}`）の扱い。

## Overview & Purpose

このモジュールは、パーサがソースコードから検出した**import文**を共通的に表現するためのデータ構造を提供します。Rust の `use` や TypeScript の `import` といった言語固有表現を、以下の最小要素で抽象化します。

- インポート対象の**パス**（例: "std::collections::HashMap"）
- 任意の**エイリアス**（例: `as Baz`）
- **ファイル識別子**（どのファイルからのインポートか）
- **グロブ**かどうか（例: `use foo::*`）
- **型専用**かどうか（TypeScriptの `import type`）

本ファイルにはロジック（関数）は存在せず、データの受け渡しおよび保管が目的です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | Import | pub | import文の抽象表現（パス、エイリアス、所在ファイル、グロブ、型専用）を保持 | Low |

定義（全文引用）:

```rust
use crate::FileId;

/// Represents an import statement in a file
#[derive(Debug, Clone)]
pub struct Import {
    /// The path being imported (e.g., "std::collections::HashMap")
    pub path: String,
    /// The alias if any (e.g., "use foo::Bar as Baz")
    pub alias: Option<String>,
    /// Location in the file where this import appears
    pub file_id: FileId,
    /// Whether this is a glob import (e.g., "use foo::*")
    pub is_glob: bool,
    /// Whether this is a type-only import (TypeScript: `import type { Foo }`)
    pub is_type_only: bool,
}
```

### Dependencies & Interactions

- 内部依存: なし（このファイル内に関数や他構造体の依存は存在しない）
- 外部依存（推定表）:

  | 依存シンボル | 由来 | 用途 |
  |--------------|------|------|
  | FileId | crate::FileId | インポートが属するファイルの識別子 |

- 被依存推定（このモジュールを使用する可能性が高い箇所）
  - 言語パーサ（Rust/TypeScriptなど）の**AST/トークナイザ出力段**でのイベント蓄積
  - 静的解析・依存関係解析・ビルド最適化・デッドコード検出・リンター
  - IDE/エディタ補助（ジャンプ先ナビゲーション、リファクタリング補助）

## API Surface (Public/Exported) and Data Contracts

公開API一覧:

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| Import | pub struct Import { path: String, alias: Option<String>, file_id: FileId, is_glob: bool, is_type_only: bool } | import文の表現（データコンテナ） | O(|path| + |alias|) の構築/複製コスト | O(|path| + |alias|) |

詳細:

1) Import

1. 目的と責務
   - 複数言語の import 表現を、最小限の共通フィールドで表す*不変条件なし*のデータ構造体。
   - パイプライン間（パーサ→解析→出力）での情報受け渡しの契約（data contract）。

2. アルゴリズム
   - なし（純粋なデータ型）

3. 引数（= フィールド仕様）

   | フィールド | 型 | 必須 | 説明 |
   |------------|----|------|------|
   | path | String | 必須 | インポート対象の識別パス（フォーマットはパーサの規約に依存）。例: "std::collections::HashMap" |
   | alias | Option<String> | 任意 | エイリアス名。未指定は None。空文字は避けるのが望ましい（規約要定義）。 |
   | file_id | FileId | 必須 | この import が現れたファイルの識別子。具体型はこのチャンクには現れない。 |
   | is_glob | bool | 必須 | グロブ指定（例: `use foo::*`）なら true。 |
   | is_type_only | bool | 必須 | 型専用 import（TypeScript の `import type`）なら true。 |

4. 戻り値
   - 該当なし（構造体そのもの）

5. 使用例

   - Rust の通常 import

     ```rust
     use crate::{FileId, parsing::import::Import};

     fn make_hashmap_import(file_id: FileId) -> Import {
         Import {
             path: "std::collections::HashMap".to_string(),
             alias: None,
             file_id,
             is_glob: false,
             is_type_only: false,
         }
     }
     ```

   - Rust のエイリアス import

     ```rust
     use crate::{FileId, parsing::import::Import};

     fn make_alias_import(file_id: FileId) -> Import {
         Import {
             path: "foo::Bar".to_string(),
             alias: Some("Baz".to_string()),
             file_id,
             is_glob: false,
             is_type_only: false,
         }
     }
     ```

   - TypeScript の type-only import（表現の詳細はパーサ規約に依存）

     ```rust
     use crate::{FileId, parsing::import::Import};

     fn make_ts_type_only(file_id: FileId) -> Import {
         // path の表現（例: "lib::Foo" や "lib/Foo" など）はプロジェクト規約に従うこと
         Import {
             path: "lib::Foo".to_string(), // 例。正規化規約はこのチャンクには現れない
             alias: None,
             file_id,
             is_glob: false,
             is_type_only: true,
         }
     }
     ```

6. エッジケース
   - path が空文字や空白のみ
   - alias に空文字が入る（None と区別が曖昧化）
   - is_glob と is_type_only の両立可否（言語仕様により無効の可能性）
   - 複合 import（例: `use a::{A,B}`）の分解表現
   - path の正規化（区切り子、ケース、引用符など）

## Walkthrough & Data Flow

- 典型的フロー（概念）:
  1. パーサがソースを走査し、import 構文を検出。
  2. 言語ごとの規約に沿って `path` を構築し、必要に応じて `alias` を付与。
  3. 現在解析中のファイルに対応する `file_id: FileId` を割り当て。
  4. グロブや type-only などのフラグを設定。
  5. `Import` を下流の解析器やインデクサへ渡す。

- このチャンクには関数/ロジックがないため、制御フローの分岐や状態遷移図は該当なし。

## Complexity & Performance

- 時間計算量
  - 生成: `String` の割り当てに比例（O(|path| + |alias|)）。フラグ設定は O(1)。
  - 複製（`Clone`）: 内部 `String` のディープコピーに比例（O(|path| + |alias|)）。
- 空間計算量
  - O(|path| + |alias|)（`String` のヒープ使用量）。`bool` は僅少、`FileId` は型依存。
- ボトルネック/スケール限界
  - 大規模プロジェクトで import 数が多い場合、`Clone` の多用が GC 不在の Rust でもヒープコピー増大を招く。
  - 繰り返し共有されるパスが多いなら、重複排除（インターン/`Arc<str>`/`Cow<'a, str>`）検討余地あり。
- 実運用負荷要因
  - I/O/ネットワーク/DB は本型自体には関与しない。外部での集約・出力時のメモリプレッシャーが主。

## Edge Cases, Bugs, and Security

- 機能エッジケース（本チャンクの実装では未バリデーション）

  | エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
  |-------------|--------|----------|------|------|
  | 空文字列の path | "" | エラー/拒否が望ましい | バリデーションなし | 未対応 |
  | 空白のみの path | "  " | エラー/正規化が望ましい | バリデーションなし | 未対応 |
  | 空文字 alias | Some("") | None に正規化が望ましい | 正規化なし | 未対応 |
  | is_glob と is_type_only の併用 | true/true | 言語規約で禁止なら拒否 | 検証なし | 不明 |
  | 複合 import の扱い | `use a::{A,B}` | A と B を個別 Import として表現など | 規約不明 | 不明 |
  | path の言語差異 | "a::b" vs "a/b" | 統一ルールに正規化 | 規約不明 | 不明 |

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: いずれも該当なし（所有型と安全な標準型のみ、`unsafe` なし）。
- インジェクション
  - SQL / Command / Path traversal: 本型はデータコンテナであり攻撃面はない。ただし、下流で `path` をコマンド/クエリへ埋め込む場合は別途サニタイズが必要。
- 認証・認可
  - 不要（本型はドメインデータ）。
- 秘密情報
  - Hard-coded secrets: 該当なし。
  - Log leakage: `Debug` で `path`/`alias` が出力されるため、ログポリシー上の配慮は必要（ソース由来の値のため一般に問題は小）。
- 並行性
  - Race condition / Deadlock: 該当なし（同期原語なし）。
  - `Send/Sync`: `String` は `Send+Sync`。本型の `Send/Sync` は最終的に `FileId` に依存。`FileId` が `Send+Sync` なら本型も成立。
- Rust特有の観点
  - 所有権: フィールドはすべて所有（`String`/`Option<String>`）。ムーブ/クローンのコスト・意味が明確。
  - 借用/ライフタイム: 明示ライフタイムなし。借用の複雑さなし。
  - unsafe 境界: なし。
  - エラー設計: バリデーションがないため「不正状態を構築できる」。API 設計としては `new()` で検証するのが望ましい。
  - panic: なし（unwrap/expect もなし）。

## Design & Architecture Suggestions

- フィールドの**非公開化**と**コンストラクタ/ビルダー**の追加
  - 例: `Import::new(path, alias, file_id).with_glob(is_glob).with_type_only(is_type_only)?`
  - ここで path や alias の正規化（trim, 空文字→None）、相互排他（言語規約に反する組合せの拒否）を実施。
- path 表現の**正規化ポリシー**の明文化
  - 区切り（`::`/`/`/`.`）、ケース、引用符、モジュール名 vs シンボル名の分離など。
  - 必要なら `ImportPath` の newtype を導入して規約を型レベルで拘束。
- 多言語サポートの表現力強化
  - 言語差異を吸収するため `ImportKind`（例: RustUse, TsImport）や `scope`/`symbol`/`module` の分離を検討。
  - 複合 import の表現（子要素の配列、もしくはパーサ側で複数 `Import` に分割する規約の明示）。
- メモリ効率
  - 大規模で重複の多い `path` を扱うなら**インターン**（`Arc<str>`/`&'a str` + `Cow<'a, str>`）の採用を検討。
- 比較・重複除去用途
  - `PartialEq`, `Eq`, `Hash`, `Ord` の導出を検討（運用要件次第）。
- ユーティリティ
  - `is_rust_glob()`, `is_type_only_ts()` のようなヘルパーは、プロジェクト側の言語識別設計と一緒に。

## Testing Strategy (Unit/Integration) with Examples

目的: データ契約の一貫性検証、バリデーション（導入後）のテスト、クローン/デバッグ可能性の確認。

- 単体テスト
  - 正常系
    - 通常 import（alias なし）
    - alias あり import
    - glob フラグの正/負
    - type-only フラグの正/負
  - 境界系
    - 空文字/空白のみの path（将来の `new()` がエラーにするか、正規化するか）
    - 空文字 alias の正規化
    - is_glob と is_type_only の併用可否（規約に基づく）
  - クローン動作
    - `Clone` によりフィールド値が一致すること（`FileId` の比較はこのチャンク外の仕様に依存）

サンプルテスト（現状の public フィールドを直接設定する例）:

```rust
#[cfg(test)]
mod tests {
    use super::Import;
    use crate::FileId;

    fn dummy_file_id() -> FileId {
        // 具体的な作り方はこのチャンクには現れないため、テスト環境側のダミー生成を想定
        // 例: FileId::new(1) などのヘルパを用意する
        unimplemented!("FileId のダミー生成はプロジェクト側で実装してください");
    }

    #[test]
    fn import_basic() {
        let file_id = dummy_file_id();
        let imp = Import {
            path: "std::collections::HashMap".into(),
            alias: None,
            file_id,
            is_glob: false,
            is_type_only: false,
        };
        assert_eq!(imp.path, "std::collections::HashMap");
        assert!(imp.alias.is_none());
        assert!(!imp.is_glob);
        assert!(!imp.is_type_only);
    }

    #[test]
    fn import_with_alias_and_flags() {
        let file_id = dummy_file_id();
        let imp = Import {
            path: "foo::Bar".into(),
            alias: Some("Baz".into()),
            file_id,
            is_glob: true,
            is_type_only: false,
        };
        assert_eq!(imp.path, "foo::Bar");
        assert_eq!(imp.alias.as_deref(), Some("Baz"));
        assert!(imp.is_glob);
        assert!(!imp.is_type_only);
    }

    #[test]
    fn clone_should_copy_all_fields() {
        let file_id = dummy_file_id();
        let imp = Import {
            path: "x::y".into(),
            alias: Some("Z".into()),
            file_id,
            is_glob: false,
            is_type_only: true,
        };
        let c = imp.clone();
        assert_eq!(c.path, imp.path);
        assert_eq!(c.alias, imp.alias);
        assert_eq!(c.is_glob, imp.is_glob);
        assert_eq!(c.is_type_only, imp.is_type_only);
        // file_id の比較方法は FileId の仕様に依存（必要に応じて比較）
    }
}
```

- 統合テスト
  - 実パーサから `Import` が生成される一連の流れを検証（サンプルソース → 期待 `Import` 群）
  - 言語ごとの差異（Rust/TypeScript）を越えた正規化結果の一致を確認

## Refactoring Plan & Best Practices

- API 強化
  - `pub` フィールドを private にし、`Import::new(...)` で不変条件（非空 path、alias の trim/正規化、無効組合せの拒否）を保証。
  - `TryFrom<ParserNode>` の導入でパーサからの構築時にバリデーション。
- 型設計
  - `ImportPath` newtype（内部 `Arc<str>` など）で表現と正規化を一元化。
  - 多言語対応のため `ImportKind`/`Language` を追加し、`is_type_only` を `ImportKind::TypeOnly` へ吸収。
- パフォーマンス
  - 重複の多い文字列に対して**インターンテーブル**または `Arc<str>` を導入し、`Clone` コストを削減。
  - `SmallString`/`SmolStr` 的最適化はシンボルが短い場合に有効。
- ユーティリティ
  - 正規化ヘルパ（`normalize_path()`, `normalize_alias()`）を追加。
  - 比較/集合用途に `PartialEq`, `Eq`, `Hash` の導出検討。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 解析ステップでの import 検出時に `debug!`: ファイル、path、alias、フラグを出力（大量の場合はレート制限）。
  - 警告: 不正/曖昧な表現（空 path、未知の区切り文字）を検出した場合に `warn!`。
- メトリクス
  - `imports.total`、`imports.by_file{file_id}`、`imports.glob.count`、`imports.type_only.count`。
  - path 正規化失敗カウンタ。
- トレーシング
  - パーサスパンに import 生成イベントをアノテートして、下流での相関を容易化。

## Risks & Unknowns

- `FileId` の仕様不明（`Send/Sync`/比較/表示/生成方法）。
- `path` の正規化規約不在（言語横断での一貫性が未定義）。
- 複合 import（`{A,B}`）をどう分解/保持するかの方針不明。
- `is_glob` と `is_type_only` の併用可否（言語仕様起因）。バリデーションがないため不正状態の混入リスク。
- すべてのフィールドが `pub` のため、外部から容易に不正値が作れてしまう点（契約違反の検出が遅延）。