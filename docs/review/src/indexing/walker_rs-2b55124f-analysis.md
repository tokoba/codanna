# walker.rs Review

## TL;DR

- 目的: **ファイルシステムを走査**して、インデックス対象のソースファイルを拡張子ベースで抽出する（.gitignoreやカスタム無視ファイル対応）。
- 主要公開API: **FileWalker::new**, **FileWalker::walk**, **FileWalker::count_files**（拡張子はレジストリ由来）。
- 複雑箇所: **ignore::WalkBuilder**の設定と、**拡張子フィルタ**の組み合わせによる抽出ロジック（O(N·M)）。*M=有効拡張子数*。
- 重大リスク: **hidden(false)**の設定とコメントの不一致により、隠しディレクトリ配下の非隠しファイルが誤って抽出される可能性。
- エラー/安全: **unsafeなし**。レジストリのMutexロック失敗時は空集合でフェイルソフトだが、ユーザーに理由が見えない（ロギング等未実装）。
- 並行性: **Arc<Settings>**で共有設定を安全に参照。レジストリは**lock()**で同期。歩査自体は同期的（並列化なし）。
- セキュリティ: パスやコマンドインジェクションは該当なし。ただし秘密情報・隠しディレクトリの扱いに注意が必要。

## Overview & Purpose

このモジュールは、与えられたルートディレクトリからファイルシステムを走査し、インデックス対象となる**ソースファイル**を抽出するための**効率的なウォーカー**を提供します。以下に対応します。

- **.gitignore**（ローカル・グローバル・exclude）の尊重
- **カスタム無視ファイル**（.codannaignore）の尊重
- **言語フィルタ**（有効言語の拡張子に基づく）
- **隠しファイルの除外**（先頭が'.'のファイル名）

主にインデクサの前段で、解析対象ファイルの列挙に利用されることを目的としています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | FileWalker | pub | 設定を保持し、ファイル走査とフィルタを提供 | Med |
| Impl fn | new | pub | FileWalkerの生成（設定の受け取り） | Low |
| Impl fn | walk | pub | ディレクトリ走査し、抽出条件に合致するファイルのIteratorを返す | Med |
| Impl fn | get_enabled_extensions | private | レジストリから有効拡張子一覧を取得 | Low |
| Impl fn | count_files | pub | 抽出対象ファイルのカウント | Low |
| Mod | tests | private | ユニットテスト一式 | Med |

### Dependencies & Interactions

- 内部依存
  - FileWalker::walk → FileWalker::get_enabled_extensions（拡張子リスト取得）
  - FileWalker::count_files → FileWalker::walk（イテレータのcount）
- 外部依存（クレート/モジュール）
  - ignore::WalkBuilder（ディレクトリ走査設定と構築）
  - crate::Settings（有効言語などの設定を参照）
  - crate::parsing::get_registry（拡張子レジストリの同期取得）
- 被依存推定
  - インデクサの上位モジュール（例: indexingパイプライン）がこのウォーカーで列挙したファイルをパーサに渡す可能性が高い。具体的呼び出し元はこのチャンクには現れない。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| FileWalker::new | fn new(settings: Arc<Settings>) -> Self | 設定を保持するウォーカーの生成 | O(1) | O(1) |
| FileWalker::walk | fn walk(&self, root: &Path) -> impl Iterator<Item = PathBuf> | 走査して条件に合致するファイルを列挙 | O(N·M) | O(M) |
| FileWalker::count_files | fn count_files(&self, root: &Path) -> usize | 抽出対象ファイル数の取得 | O(N·M) | O(M) |

ここで、N=訪問するファイル数、M=有効拡張子数。get_enabled_extensionsは内部APIで、Mの取得にO(M)相当のコストがかかります。

### FileWalker::new

1) 目的と責務
- 設定（Settings）を共有（Arc）で保持する**FileWalker**インスタンスを生成します。

2) アルゴリズム（ステップ分解）
- 引数のArc<Settings>をフィールドに代入するだけ。

3) 引数

| 引数名 | 型 | 説明 |
|-------|----|------|
| settings | Arc<Settings> | 共有される設定オブジェクト（言語有効化等を含む） |

4) 戻り値

| 型 | 説明 |
|----|------|
| FileWalker | ウォーカーインスタンス |

5) 使用例
```rust
use std::sync::Arc;
use std::path::Path;
let settings = Arc::new(Settings::default());
let walker = FileWalker::new(settings);
let paths: Vec<_> = walker.walk(Path::new(".")).collect();
```

6) エッジケース
- Arc<Settings>が空や未初期化: 設計上ありえない（型で保証）。
- Settings内容の妥当性は未検証（このチャンクには検証ロジックなし）。

根拠: FileWalker::new（行番号不明）

### FileWalker::walk

1) 目的と責務
- ルートディレクトリ以下を走査し、**拡張子が有効**かつ**隠しファイルでない**、**.gitignore/.codannaignoreで除外されない**通常ファイルのみを**Iterator**で返します。

2) アルゴリズム（ステップ分解）
- WalkBuilderをrootで初期化。
- 設定:
  - hidden(false)
  - git_ignore(true), git_global(true), git_exclude(true)
  - follow_links(false)
  - max_depth(None)
  - require_git(false)
  - add_custom_ignore_filename(".codannaignore")
- enabled_extensions = get_enabled_extensions()
- build()でイテレータ生成し、以下のフィルタを順に適用:
  - Result::okでアクセス不能エントリを除外
  - file_typeがSomeかつis_file()のみ通す
  - ファイル名が'.'始まりなら除外（隠しファイル対応）
  - path.extension().to_str()が有効拡張子リストに含まれるか判定
  - 条件合致したパスをPathBufで返す

3) 引数

| 引数名 | 型 | 説明 |
|-------|----|------|
| root | &Path | 走査開始位置（ディレクトリまたはファイル） |

4) 戻り値

| 型 | 説明 |
|----|------|
| impl Iterator<Item = PathBuf> | 条件を満たすファイルパスの遅延列挙 |

5) 使用例
```rust
use std::path::Path;
let walker = FileWalker::new(Arc::new(Settings::default()));
for path in walker.walk(Path::new("src")) {
    println!("Indexing: {}", path.display());
}
```

6) エッジケース
- rootがファイル: WalkBuilderはそれを対象として扱い、フィルタ条件を満たせば1件返す。
- 非UTF-8の拡張子: extension.to_str()がNoneとなり除外される。
- 隠しディレクトリ配下の通常ファイル: hidden(false)設定のため、ディレクトリは走査され、ファイル名が'.'始まりでない場合に抽出される（意図と不一致の可能性）。
- レジストリロック失敗: 有効拡張子が空となり、結果は空イテレータ（フェイルソフト）。

根拠: FileWalker::walk（行番号不明）

### FileWalker::count_files

1) 目的と責務
- walk(root)の結果をカウントし、**ドライラン**や進捗見積に役立てる。

2) アルゴリズム（ステップ分解）
- self.walk(root).count() を返すのみ。

3) 引数

| 引数名 | 型 | 説明 |
|-------|----|------|
| root | &Path | 走査開始位置 |

4) 戻り値

| 型 | 説明 |
|----|------|
| usize | 抽出対象ファイル数 |

5) 使用例
```rust
let n = FileWalker::new(Arc::new(Settings::default()))
    .count_files(Path::new("."));
println!("Will index {} files", n);
```

6) エッジケース
- walkのエッジケースに従う（上記参照）。

根拠: FileWalker::count_files（行番号不明）

## Walkthrough & Data Flow

- 入力: Arc<Settings>（FileWalker生成時）、&Path root（walk実行時）
- 処理:
  - WalkBuilderにrootを設定し、隠し・gitignore・リンク追従等の挙動を構成。
  - レジストリから**enabled_extensions**（Vec<String>）を取得。
  - Walkをbuildし、エントリ（DirEntry）ストリームに対して段階的にフィルタ:
    - エラーのあるエントリはスキップ（Result::ok）
    - file_typeがファイルのものだけ通過
    - ファイル名が'.'始まりなら除外
    - 拡張子がenabled_extensionsに含まれる場合のみ通過
  - 通過したエントリのpathをPathBufへ変換し、Iteratorとして返却。
- 出力: 条件に合致するPathBufの列挙（遅延評価）

データ契約（このチャンクから読み取れる範囲）:
- get_registry().lock()が成功すると、registry.enabled_extensions(&Settings)は少なくともIteratorを返す前提。
- enabled_extensionsは**String（小文字/正規化不明）**の集合。拡張子比較は**大小文字区別**（ext == ext_str）。

根拠: FileWalker::get_enabled_extensions, FileWalker::walk（行番号不明）

## Complexity & Performance

- 時間計算量:
  - walk: O(N·M) ただし、Nは走査されたファイル数、Mは有効拡張子数（拡張子判定でany()）。
- 空間計算量:
  - walk: O(M)（enabled_extensionsベクトル）、イテレータは遅延でO(1)追加。
- ボトルネック:
  - 大規模ディレクトリでのディスクI/O。
  - 各ファイルの拡張子判定でO(M)線形探索（Mが大きい場合は非効率）。
- スケール限界:
  - 単スレッドのため、ディスク並列性を活用できない。
  - 非UTF-8拡張子は無視されるため、多言語環境での網羅性に制約。
- 実運用負荷要因:
  - ネットワークファイルシステム（NFS等）上でのI/O遅延。
  - 大量の.ignore/.gitignore評価によるメタデータ読み取りコスト。

## Edge Cases, Bugs, and Security

セキュリティチェックリストとエッジケース評価:

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（Rust安全性により防止、unsafeなし）。
- インジェクション
  - SQL / Command / Path traversal: 該当なし（外部コマンド未実行、パスはローカル走査）。
- 認証・認可
  - 権限チェック漏れ: OSが権限管理。アクセス不可エントリはResult::okで除外。
  - セッション固定等: 該当なし。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: ログ未実装。失敗要因が利用者に可視化されない。
- 並行性
  - Race condition / Deadlock: レジストリMutexのlockのみ。Deadlockの兆候なし。このチャンクには非同期/並列実行なし。
  - Poisoned lock: 捕捉して空リスト返却（フェイルソフト）。ただし、利用者に原因が伝わらない。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 隠しディレクトリ配下の通常ファイル | `.hidden/visible.rs` | 除外（隠しディレクトリも除外） | `builder.hidden(false)`により隠しディレクトリを走査、ファイル名が'.'で始まらないため通過 | Bug（意図不一致の可能性） |
| 隠しファイル | `.hidden.rs` | 除外 | ファイル名先頭'.'で除外 | OK |
| 非UTF-8拡張子 | `file.\xFF` | 定義次第だが一般に除外 | `extension.to_str()`がNoneで除外 | OK（仕様として記載推奨） |
| 拡張子大小文字差 | `FILE.RS` | 受け入れ（大小文字非依存が望ましい） | `ext == ext_str`で区別あり | 仕様未定（改善候補） |
| レジストリロック失敗 | Mutex poisoned | 警告の上で可能なら継続 | 空拡張子集合で結果ゼロ | Degrade（ロギングなし） |
| rootがファイル | `main.rs` | 1件を返す（有効拡張子なら） | WalkBuilder処理→フィルタ通過 | OK |
| シンボリックリンク | `link -> file.rs` | follow_links(false)ならリンク追跡なし | 設定はfalseだが、エントリがどう扱われるかはignore依存 | 不明（このチャンクには現れない） |
| .codannaignoreの仕様 | パターンの追加/無視 | .gitignore互換の除外 | `add_custom_ignore_filename(".codannaignore")`のみ | 仕様最小（設定からの追加は未実装） |

Rust特有の観点（詳細チェックリスト）:
- 所有権: FileWalkerはArc<Settings>を所有。moveキャプチャでenabled_extensionsをイテレータに渡す（FileWalker::walk）。
- 借用/ライフタイム: &Pathを短期借用。明示的ライフタイム不要。
- unsafe境界: なし。
- Send/Sync: Arc<Settings>は通常Send/Sync。Walkはスレッドセーフであるが本コードは並列未使用。
- データ競合: レジストリはMutexにより保護。
- await境界/キャンセル: 非async。
- エラー設計: I/Oエラーはignoreクレートの内部。外部にはパス列挙の失敗エントリをスキップ（Result::ok）。panic要素なし。

根拠: FileWalker::walk, get_enabled_extensions（行番号不明）

## Design & Architecture Suggestions

- 隠しディレクトリの扱い修正
  - 意図が「隠しディレクトリは歩査しない」なら、WalkBuilderで**hidden(true)**にするか、パスの各コンポーネントに'.'始まりが含まれる場合は除外する追加チェックを行う。
- 拡張子判定の改善
  - 大小文字非依存のマッチング（ext_str.to_ascii_lowercase()）や、あらかじめHashSet<String>でO(1)判定。
- カスタム無視パターンの設定対応
  - Settingsから**glob**や**.gitignore互換**のパターンを読み込み、WalkBuilderのカスタムignoreへ反映。
- 並列化
  - ignore::WalkParallelの活用で大規模プロジェクトの走査を高速化（並列安全な集計が必要）。
- 観測性
  - スキップ理由（隠し・ignore・拡張子不一致）を**debugログ**で可視化。計測用**メトリクス**（歩査件数、フィルタ通過率、I/O時間）を追加。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本ケースをカバーしています。以下の追加テストを推奨します。

- 隠しディレクトリ配下のファイル
```rust
#[test]
fn test_hidden_directory_files_are_excluded() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let root = temp_dir.path();
    std::fs::create_dir(root.join(".hidden")).unwrap();
    std::fs::write(root.join(".hidden/visible.rs"), "fn x(){}").unwrap();

    let settings = Arc::new(Settings::default());
    let walker = FileWalker::new(settings);

    let files: Vec<_> = walker.walk(root).collect();
    // 期待: 除外（設計意図が隠しディレクトリ非走査なら）
    assert!(files.iter().all(|p| !p.to_string_lossy().contains(".hidden")));
}
```

- 非UTF-8拡張子の扱い
```rust
#[test]
fn test_non_utf8_extension_is_skipped() {
    use std::os::unix::ffi::OsStrExt;
    let temp_dir = tempfile::TempDir::new().unwrap();
    let root = temp_dir.path();
    let mut path = root.join("file");
    path.as_mut_os_string().push(std::ffi::OsStr::from_bytes(b".\xFF"));
    std::fs::write(&path, "content").unwrap();

    let settings = Arc::new(Settings::default());
    let walker = FileWalker::new(settings);

    let files: Vec<_> = walker.walk(root).collect();
    assert!(files.is_empty());
}
```

- 大文字拡張子
```rust
#[test]
fn test_uppercase_extension_match() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let root = temp_dir.path();
    std::fs::write(root.join("MAIN.RS"), "fn main(){}").unwrap();

    let settings = Arc::new(Settings::default());
    let walker = FileWalker::new(settings);

    let files: Vec<_> = walker.walk(root).collect();
    // 現状は大小文字区別のため、失敗する可能性あり
    // 改善後は通過を期待
    // assert!(files.iter().any(|p| p.ends_with("MAIN.RS")));
}
```

- .codannaignoreの動作（簡易）
```rust
#[test]
fn test_codannaignore_respected() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let root = temp_dir.path();
    std::fs::write(root.join(".codannaignore"), "skip.rs\n").unwrap();
    std::fs::write(root.join("skip.rs"), "fn x(){}").unwrap();
    std::fs::write(root.join("keep.rs"), "fn y(){}").unwrap();

    let settings = Arc::new(Settings::default());
    let walker = FileWalker::new(settings);

    let files: Vec<_> = walker.walk(root).collect();
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("keep.rs"));
}
```

- シンボリックリンク（プラットフォーム依存）
```rust
#[test]
fn test_symlink_not_followed() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let temp_dir = tempfile::TempDir::new().unwrap();
        let root = temp_dir.path();
        std::fs::write(root.join("real.rs"), "fn x(){}").unwrap();
        symlink(root.join("real.rs"), root.join("link.rs")).unwrap();

        let settings = Arc::new(Settings::default());
        let walker = FileWalker::new(settings);
        let files: Vec<_> = walker.walk(root).collect();

        // follow_links(false)の挙動に依存（ignoreの仕様次第）
        // プロジェクト方針に合わせて期待値を決める
    }
}
```

## Refactoring Plan & Best Practices

- フィルタ関数の分離
  - 例: `fn is_indexable(&self, entry: &DirEntry, exts: &HashSet<String>) -> bool` を作り、テスト容易性と再利用性を高める。
- 拡張子集合の型をHashSetに
  - `get_enabled_extensions`で`HashSet<String>`を返し、O(1)判定に。
- 隠しディレクトリ検出
  - パスコンポーネントに'.'始まりが含まれるかをチェックするヘルパーを導入。
- 設定由来の無視パターン
  - Settingsからのglob群をWalkBuilderに適用。必要なら一時ファイルではなく**in-memory ignore**の仕組みを検討。
- エラーとログ
  - レジストリロック失敗時やI/O除外時の**debug/infoログ**を追加して運用性を高める。
- インターフェイスの明確化
  - `walk`の返すIteratorの抽象化（具体型のtype alias）を付与し、型名の明示性を調整してテスト・モック容易性を向上。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - スキップ理由（隠し、ignore、拡張子不一致、非UTF-8）の**debugログ**。
  - レジストリロック失敗時の**warnログ**。
- メトリクス
  - 走査ファイル数、フィルタ通過率、I/O時間、ignoreルール適用件数。
- トレーシング
  - 大規模走査時のパフォーマンスボトルネック特定のため、**span**をroot単位、サブディレクトリ単位に付与（このチャンクにはトレーシング未実装）。

## Risks & Unknowns

- ignoreクレートの**シンボリックリンク**扱い詳細（follow_links=false時にリンクそのものが列挙されるか）はこのチャンクでは不明。
- Settings/レジストリの**enabled_extensions**の内容（大小文字、重複、エイリアス）仕様は不明。
- `.codannaignore`の正確な仕様と、Settingsからの**カスタムパターン**連携は未実装（TODOあり）。
- テストで`settings.languages.get_mut("python").unwrap()`を前提にしているが、Settingsの初期内容はこのチャンクには現れないため、テストの堅牢性は不明。

根拠: コードコメントと呼び出し関係（行番号不明、該当仕様はこのチャンクには現れない）