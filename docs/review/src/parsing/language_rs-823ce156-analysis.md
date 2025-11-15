# parsing\language.rs Review

## TL;DR

- 目的: ファイル拡張子からの言語判定と、言語列挙（enum）＋周辺ユーティリティの提供。レジストリとの移行期間を支える互換APIを含む。
- 公開API: Language enumと7つのメソッド（to_language_id, from_language_id, from_extension, from_path, extensions, config_key, name）＋Display実装。
- 複雑箇所: from_extension。レジストリ参照（Mutexロック）→固定テーブルのフォールバック、大小文字対応、複数拡張子の網羅。
- 重大リスク: Path::extensionの仕様上、"go.mod"/"go.sum"はfrom_pathで検出されない（拡張子は"mod"/"sum"になる）。レジストリとenumの不整合が起きた場合、検出がNoneで終わる。
- エラー/並行性: レジストリがPoisonedな場合はフォールバックし理由を隠蔽。Mutexによる軽量ロックでデッドロックの気配は低いが、待ち時間増加の可能性あり。
- 性能: すべてO(1)または拡張子長nに対してO(n)（to_lowercase）。I/Oなしで軽量。
- 改善提案: 拡張子→言語の静的マップ化、"go.mod"等の複合拡張子のfrom_path対応、レジストリ使用失敗の可観測化（ログ/メトリクス）、enumとレジストリ定義の整合性検証テスト。

## Overview & Purpose

このモジュールは、プログラミング言語を表す**Language**列挙型と、その言語をファイル拡張子やパスから検出する機能、言語に紐づく拡張子や表示名などの**言語固有設定**を提供します。レジストリ（super::get_registry, super::LanguageId）への移行期間中であり、互換性のために**LanguageIdとの相互変換**メソッドが存在します。用途は次の通りです。

- フロントエンドやCLIがファイルから言語を判別する。
- 設定（config）キーや表示名の取得。
- レジストリの言語定義との橋渡し。

I/Oや複雑な並行処理はなく、純粋に文字列と列挙型を扱う軽量なロジックです。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | Language | pub | 言語の列挙（Rust/Python/JS/TS/Php/Go/C/Cpp/CSharp/Gdscript/Kotlin） | Low |
| Impl | Language::to_language_id | pub | Language → LanguageId 変換（移行用） | Low |
| Impl | Language::from_language_id | pub | LanguageId → Language 逆変換（移行用） | Low |
| Impl | Language::from_extension | pub | 拡張子文字列から言語検出（レジストリ優先、固定テーブルにフォールバック） | Med |
| Impl | Language::from_path | pub | パスの拡張子から言語検出（from_extensionに委譲） | Low |
| Impl | Language::extensions | pub | 言語に紐づく標準拡張子の列挙 | Low |
| Impl | Language::config_key | pub | 設定キー文字列の取得 | Low |
| Impl | Language::name | pub | 人間可読名の取得 | Low |
| Trait Impl | Display for Language | public impl | 表示用（name()に委譲） | Low |
| Module | tests (cfg(test)) | private | 単体テスト | Low |

### Dependencies & Interactions

- 内部依存
  - Display::fmt → Language::name
  - Language::from_path → Language::from_extension
  - Language::from_extension → super::get_registry, registry.get_by_extension → Language::from_language_id
  - Language::to_language_id / from_language_id ↔ super::LanguageId

- 外部依存（このチャンクで確認できるもの）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | serde::{Serialize, Deserialize} | Languageのシリアライズ/デシリアライズ | API/設定渡しに有用 |
  | super::LanguageId | レジストリとの橋渡し | 型詳細は不明 |
  | super::get_registry | レジストリ取得 | 実体・ロック戦略は不明 |

- 被依存推定（このモジュールを利用する可能性が高い箇所）
  - ファイル解析/インデクシング/ハイライト機能
  - 設定ローダ（config_key利用）
  - ログ/表示UI（Display, name利用）
  - レジストリ管理ロジック（LanguageIdとの相互変換）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| Language | enum Language { Rust, Python, JavaScript, TypeScript, Php, Go, C, Cpp, CSharp, Gdscript, Kotlin } | 言語の型表現 | N/A | N/A |
| to_language_id | fn to_language_id(&self) -> super::LanguageId | enum → LanguageIdへの変換（移行用） | O(1) | O(1) |
| from_language_id | fn from_language_id(id: super::LanguageId) -> Option<Self> | LanguageId → enumへの逆変換 | O(1) | O(1) |
| from_extension | fn from_extension(ext: &str) -> Option<Self> | 拡張子文字列から言語判定 | O(n)（to_lowercase） | O(1) |
| from_path | fn from_path(path: &std::path::Path) -> Option<Self> | パスから拡張子抽出→言語判定 | O(n)（拡張子長） | O(1) |
| extensions | fn extensions(&self) -> &[&str] | 言語の標準拡張子配列 | O(1) | O(1) |
| config_key | fn config_key(&self) -> &str | 設定キーの取得 | O(1) | O(1) |
| name | fn name(&self) -> &str | 人間可読名の取得 | O(1) | O(1) |
| Display | impl Display for Language | ユーザ向け表示（nameに委譲） | O(1) | O(1) |

以下、各APIの詳細。

1) Language（enum）
- 目的と責務: サポート言語の有限集合を型安全に表現。Serialize/Deserialize可能。
- データ契約: 列挙子は固定。追加時は各メソッドのmatchに整合性が必要。
- 使用例:
  ```rust
  let lang = Language::Rust;
  assert_eq!(lang.name(), "Rust");
  ```
- エッジケース:
  - 列挙子追加時の未更新メソッド（to_language_id, extensionsなど）による不整合。

2) to_language_id
- 目的と責務: レジストリ移行中の互換API。Languageを静的文字列IDへ写像。
- アルゴリズム:
  - match selfで対応する静的文字列をsuper::LanguageId::newで生成。
- 引数:
  | 名称 | 型 | 意味 |
  |------|----|------|
  | self | &Language | 対象言語 |
- 戻り値:
  | 型 | 意味 |
  |----|------|
  | super::LanguageId | レジストリID |
- 使用例:
  ```rust
  let id = Language::Python.to_language_id();
  assert_eq!(id.as_str(), "python"); // as_str()はこのチャンクには現れないが想定
  ```
- エッジケース:
  - レジストリ側に未登録の言語IDとの不整合（不明）。

3) from_language_id
- 目的と責務: 逆変換。未知IDはNone。
- アルゴリズム:
  - id.as_str()で文字列取得しmatch。
- 引数:
  | 名称 | 型 | 意味 |
  |------|----|------|
  | id | super::LanguageId | レジストリID |
- 戻り値:
  | 型 | 意味 |
  |----|------|
  | Option<Language> | 対応する言語 or None |
- 使用例:
  ```rust
  if let Some(lang) = Language::from_language_id(Language::Rust.to_language_id()) {
      assert_eq!(lang, Language::Rust);
  }
  ```
- エッジケース:
  - レジストリに存在するがenumに未追加のID→None。

4) from_extension
- 目的と責務: 拡張子文字列から言語を検出。レジストリ優先→フォールバック。
- アルゴリズム（主要ステップ）:
  1. 拡張子を小文字化。
  2. super::get_registry().lock()を試み、成功したらregistry.get_by_extension(&ext_lower)。
  3. 見つかればdef.id()からfrom_language_idでenumへ。
  4. 見つからなければハードコードテーブルでmatch。
- 引数:
  | 名称 | 型 | 意味 |
  |------|----|------|
  | ext | &str | 拡張子（ドットなし推奨。ケース非依存） |
- 戻り値:
  | 型 | 意味 |
  |----|------|
  | Option<Language> | 検出された言語 or None |
- 使用例:
  ```rust
  assert_eq!(Language::from_extension("RS"), Some(Language::Rust));
  assert_eq!(Language::from_extension("php5"), Some(Language::Php));
  assert_eq!(Language::from_extension("unknown"), None);
  ```
- エッジケース:
  - "go.mod"/"go.sum"は文字列なら検出可能だが、Pathからは拡張子が"mod"/"sum"になるため不整合（詳細は後述）。
  - レジストリがPoisoned（lock失敗）→フォールバックで検出するが理由は露見しない。

5) from_path
- 目的と責務: Pathから拡張子取得→from_extensionで言語検出。
- アルゴリズム:
  1. path.extension()で最終拡張子を取得（OsStr）。
  2. to_str()でUTF-8に変換（失敗ならNone）。
  3. from_extension(ext)に委譲。
- 引数:
  | 名称 | 型 | 意味 |
  |------|----|------|
  | path | &std::path::Path | 対象パス |
- 戻り値:
  | 型 | 意味 |
  |----|------|
  | Option<Language> | 言語 or None |
- 使用例:
  ```rust
  use std::path::Path;
  assert_eq!(Language::from_path(Path::new("main.rs")), Some(Language::Rust));
  assert_eq!(Language::from_path(Path::new("README.md")), None);
  ```
- エッジケース:
  - 非UTF-8拡張子はNone。
  - 複合拡張子（"go.mod"等）はPath::extensionが"mod"/"sum"のみ返すため検出不可（バグ/仕様要検討）。

6) extensions
- 目的と責務: 各言語の代表拡張子配列を返す。
- アルゴリズム: match selfで静的スライス参照を返す。
- 引数/戻り値:
  | 引数 | 型 | |
  |------|----|-|
  | self | &Language | |
  | 戻り値 | &[&str] | 拡張子一覧 |
- 使用例:
  ```rust
  assert!(Language::Go.extensions().contains(&"go"));
  ```
- エッジケース:
  - 静的配列の更新漏れ（言語追加時）。

7) config_key
- 目的と責務: 設定で用いるキー文字列を返す。
- アルゴリズム: match selfで静的文字列。
- 使用例:
  ```rust
  assert_eq!(Language::CSharp.config_key(), "csharp");
  ```
- エッジケース: 追加時の更新漏れ。

8) name
- 目的と責務: ユーザ向け表示名を返す。
- アルゴリズム: match selfで静的文字列（"C++", "C#"など表記揺れに注意）。
- 使用例:
  ```rust
  assert_eq!(Language::Cpp.name(), "C++");
  ```
- エッジケース: 表示名の国際化対応は未考慮（このチャンクには現れない）。

9) Display for Language
- 目的と責務: フォーマット時にname()を表示。
- 使用例:
  ```rust
  let s = format!("{}", Language::Python);
  assert_eq!(s, "Python");
  ```
- エッジケース: 特になし。

## Walkthrough & Data Flow

主要なデータフローはfrom_extensionで次の通りです。

```mermaid
flowchart TD
    A[入力 ext: &str] --> B[小文字化 ext_lower]
    B --> C{レジストリ取得とロック成功?}
    C -- いいえ --> E[ハードコードmatchによる判定]
    C -- はい --> D{registry.get_by_extension(ext_lower)}
    D -- Some(def) --> F[def.id() → from_language_id]
    F -- Some(lang) --> G[Some(Language)]
    F -- None --> E
    D -- None --> E
    E -- 該当あり --> G
    E -- 該当なし --> H[None]
```

上記の図は`from_extension`関数の主要分岐を示す（行番号: 不明）。

参考抜粋コード（重要部分のみ）:

```rust
pub fn from_extension(ext: &str) -> Option<Self> {
    let ext_lower = ext.to_lowercase();

    // Try the registry first for registered languages
    let registry = super::get_registry();
    if let Ok(registry) = registry.lock() {
        if let Some(def) = registry.get_by_extension(&ext_lower) {
            return Self::from_language_id(def.id());
        }
    }

    // Fallback to hardcoded for languages not yet in registry
    match ext_lower.as_str() {
        "rs" => Some(Language::Rust),
        "py" | "pyi" => Some(Language::Python),
        /* ... 省略 ... */
        "kt" | "kts" => Some(Language::Kotlin),
        _ => None,
    }
}
```

from_pathは、Pathから拡張子（最終セグメント）を取り出してfrom_extensionへ委譲する直線的フローです。

## Complexity & Performance

- to_language_id, from_language_id, extensions, config_key, name, Display::fmt
  - 時間: O(1)
  - 空間: O(1)
- from_extension
  - 時間: O(n)（nは拡張子長。to_lowercaseに比例）＋ レジストリlookup（実装依存、通常O(1)/O(log m)想定）
  - 空間: O(n)（小文字化の一時String）
- from_path
  - 時間: O(n)（nは拡張子長）＋ from_extensionに依存
  - 空間: O(1)（to_strで借用、from_extensionでO(n)）

ボトルネック/スケール限界:
- レジストリのMutexロック競合が多い場合に待ち時間増加。
- 大量ファイルを一括判定する場合、to_lowercaseの都度割り当てが微小ながら蓄積。
- I/O/ネットワーク/DBは本モジュールにはなし。

## Edge Cases, Bugs, and Security

セキュリティ観点は限定的ですが、チェックリストで評価します。

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（純粋なRust安全コード、unsafeなし）。
  - 所有権/借用: &str, &Pathの不変借用のみ。to_lowercaseで新規String（所有権は関数内完結）。
  - ライフタイム: 明示的パラメータ不要。返却値は静的リテラルまたはOptionで所有権問題なし。
- インジェクション（SQL/Command/Path traversal）
  - 該当なし。外部コマンド/DB呼び出しなし。拡張子マッチのみ。
- 認証・認可
  - 該当なし。
- 秘密情報
  - Hard-coded secrets: 該当なし。
  - Log leakage: ログ処理なし。
- 並行性
  - Race condition/Deadlock: Mutexロックは短期間。get_by_extension呼び出しのみの臨界区間。デッドロックの兆候なし。
  - Poisoned Mutex: if let Ok(...)で失敗時にフォールバック。失敗原因が外部へ伝播しないため可観測性低い。
  - Send/Sync: 本チャンクでは型境界不明。「不明」。
  - 非同期/await境界: 該当なし。
  - キャンセル: 該当なし。

エッジケース詳細（重要なものを表で整理）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 大文字拡張子 | "RS" | Rust検出 | to_lowercaseで対応 | 想定通り |
| 複合拡張子のPath | Path("go.mod") | Go検出 | Path::extensionは"mod"→from_extension("mod")はNone | バグ |
| 非UTF-8拡張子 | Path拡張子に非UTF-8 | 検出不可だが理由が分かるとよい | to_str()がNone→from_pathはNone | 要検討 |
| 隠しファイル | ".gitignore" | None | extension()がNone | 想定通り |
| ヘッダ拡張子の曖昧さ | "h" | C/Cppどちらか | Cに割り当て | 要検討 |
| レジストリID不整合 | registryが未知言語ID | 判定成功 or 明確なエラー | from_language_idがNone→全体としてNone | 要検討 |
| レジストリロック失敗 | Poisoned | フォールバック判定＋観測可能 | フォールバックのみで理由非表示 | 要検討 |

重要な主張の根拠（関数名:行番号）:
- from_pathはPath::extension→to_str→from_extensionへ委譲するため"go.mod"が検出できない可能性がある。（行番号: 不明。このチャンクでは行番号が提供されていません）
- from_extensionが"go.mod"/"go.sum"をハードコードでサポート。（行番号: 不明）

## Design & Architecture Suggestions

- 複合拡張子の対応強化
  - from_pathでファイル名全体を解析し、"go.mod"/"go.sum"のようなケースを特別扱い（file_stemやfile_nameで判定）。
  - またはPath::file_nameの文字列を小文字化し、既知の複合拡張子に対して先にテーブル照合。
- 拡張子→言語マップの静的化
  - 大規模matchではなく、lazy_static/once_cellを用いたHashMapで管理。拡張が容易で重複ミスを削減。
- レジストリ整合性の検証
  - CIテストでLanguage列挙子とレジストリ定義の双方向完全性（to_language_id→from_language_idが往復同値）をチェック。
- エラー／可観測性
  - レジストリロック失敗（Poisoned）時にログ/メトリクスを記録し、原因追跡を容易に。
  - from_pathで非UTF-8の場合、OptionではなくResultに拡張（上位でハンドリング可能）する検討。
- 設計の一貫性
  - extensions/config_key/nameのmatch分岐は列挙子追加時の更新漏れリスクがあるため、LanguageIdベースの単一点定義（レジストリ主導）へ漸進的移行。

## Testing Strategy (Unit/Integration) with Examples

追加/改善すべきテスト例（ユニット）:

1) 複合拡張子のPath検証（既存の不整合の可視化）
```rust
#[test]
fn test_go_mod_from_path_should_fail_currently() {
    use std::path::Path;
    // 現仕様ではNoneになるはず
    assert_eq!(Language::from_path(Path::new("go.mod")), None);
    assert_eq!(Language::from_path(Path::new("go.sum")), None);
}
```

2) レジストリ往復整合性（移行中APIの健全性）
```rust
#[test]
fn test_language_id_roundtrip() {
    for lang in [
        Language::Rust, Language::Python, Language::JavaScript, Language::TypeScript,
        Language::Php, Language::Go, Language::C, Language::Cpp, Language::CSharp,
        Language::Gdscript, Language::Kotlin
    ] {
        let id = lang.to_language_id();
        // from_language_idがSomeで返ることを期待
        let back = Language::from_language_id(id).expect("roundtrip failed");
        assert_eq!(back, lang);
    }
}
```

3) 非UTF-8拡張子
```rust
#[test]
fn test_non_utf8_extension() {
    use std::ffi::OsString;
    use std::path::PathBuf;
    let mut p = PathBuf::from("file");
    // 非UTF-8のバイト列拡張子を付加（実際の生成は環境依存）
    let ext = OsString::from_vec(vec![0xff, 0xfe, 0xfd]);
    p.set_extension(ext);
    assert_eq!(Language::from_path(&p), None);
}
```

4) 大文字拡張子の網羅
```rust
#[test]
fn test_uppercase_extensions() {
    assert_eq!(Language::from_extension("PHP"), Some(Language::Php));
    assert_eq!(Language::from_extension("TSX"), Some(Language::TypeScript));
}
```

5) ヘッダ拡張子の曖昧性
```rust
#[test]
fn test_header_h_is_c() {
    assert_eq!(Language::from_extension("h"), Some(Language::C));
}
```

6) 未知拡張子
```rust
#[test]
fn test_unknown_extension() {
    assert_eq!(Language::from_extension("xyz"), None);
}
```

統合テスト（レジストリがある場合、詳細は「不明」。このチャンクにはレジストリ構造が現れないため省略）。

## Refactoring Plan & Best Practices

- ステップ1: 拡張子テーブルの集約
  - HashMap<&'static str, Language>をstaticで持ち、from_extensionのフォールバックmatchを置換。
- ステップ2: 複合拡張子対応
  - from_pathでfile_name小文字化→既知複合拡張子テーブルを先に照合。その後通常のextension照合。
- ステップ3: レジストリ整合性の型安全化
  - LanguageIdをnewtypeで静的strに限定し、列挙子追加時にコンパイルエラーで検知できる仕組み（不明: このチャンクにはLanguageIdの定義が現れない）。
- ステップ4: エラー/ログ設計
  - from_extensionでレジストリロック失敗時にdebugログを出す（ログクレートは不明）。オプションでResult型を返す新APIを追加。
- ベストプラクティス:
  - 列挙子追加時はextensions/config_key/name/to_language_id/from_language_idの全更新をPRテンプレートで強制。
  - テストで拡張子→言語の包括的セットを検証（プロパティテストも有用）。

## Observability (Logging, Metrics, Tracing)

- ログ
  - レジストリロックが失敗した場合（Poisoned）にdebug/warnログを記録。
  - 未知拡張子入力時（None）を必要に応じてtraceし、運用でのカバレッジ把握。
- メトリクス
  - 判定成功/失敗のカウンタ。
  - レジストリ経由判定 vs フォールバック判定の比率。
- トレーシング
  - from_extensionの分岐（レジストリ経由/フォールバック）にspan追加（トレーシングクレートは不明）。

このチャンクではログ/メトリクス/トレーシングの実装は「不明」。

## Risks & Unknowns

- レジストリAPI/型詳細（super::get_registry, super::LanguageId, registry.get_by_extension）は「不明」。Mutexの具象型、Poisonedの扱いも「不明」。
- レジストリ定義とLanguage enumの整合性（特に新言語追加・削除時の挙動）は「不明」。
- 国際化/ローカライズ要件は「不明」。
- 将来の移行計画（「This is a transitional method」記載あり）の具体的完了条件・時期は「不明」。

以上の点を踏まえ、現状のAPIは軽量で安全ですが、複合拡張子（go.mod/ go.sum）とレジストリ整合性の可観測性に注意が必要です。