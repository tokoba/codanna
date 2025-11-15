# project_resolver/sha.rs Review

## TL;DR

- 目的: 文字列およびファイル内容の**SHA-256**ハッシュを計算し、16進文字列（64桁）として返すユーティリティ
- 公開API: 
  - **compute_sha256(&str) -> Sha256Hash**
  - **compute_file_sha(&Path) -> ResolutionResult<Sha256Hash>**
- コアロジック: `sha2::Sha256`でハッシュし、`format!("{result:x}")`で16進化（L10–15）。ファイルは`read_to_string`で全読み込み（L21–25）。
- 主要な複雑箇所: エラー変換（`ResolutionError::cache_io`）の責務と意味合いがこのチャンクでは不明。非UTF-8ファイルを読み込めない設計（`read_to_string`使用）。
- 重大リスク: 大きなファイルでのメモリ使用とパフォーマンス劣化、非UTF-8ファイルの扱い失敗、同期I/Oによる非同期コンテキストでのブロッキング
- Rust安全性: 全て安全なRustで実装、`unsafe`なし、所有権・借用は自然。エラー設計は`ResolutionResult`の利用で妥当だが詳細は不明。
- 推奨改善: バイト列版APIの追加、ファイル読み込みのストリーミング化、非同期I/O対応、非UTF-8対応、観測性の追加

## Overview & Purpose

このファイルは、設定ファイル等の内容に対する**SHA-256ハッシュ**計算ユーティリティを提供します。文字列から直接ハッシュを計算する関数と、ファイルの内容を読み込んでハッシュを計算する関数の2つの公開APIにより、構成データの同一性検証やキャッシュキー生成に利用できます。結果は64桁の**16進文字列**としてラップ型`Sha256Hash`で返されます。

- 文字列のハッシュ: L10–15
- ファイルのハッシュ（I/O付き）: L21–25
- 付属テストで決定性・差異・桁数を検証: L31–50

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | compute_sha256 | pub | 文字列からSHA-256を計算し16進文字列で返す | Low |
| Function | compute_file_sha | pub | ファイル内容を読み取りSHA-256を計算。I/Oエラーをドメインエラーへ変換 | Low |
| Module | tests | private | 決定性・差異・16進の長さと文字種の検証 | Low |

### Dependencies & Interactions

- 内部依存
  - `compute_file_sha` → `compute_sha256`（L24で直接呼び出し）
- 外部依存（このチャンクで利用）
  | クレート/モジュール | 用途 | 備考 |
  |---------------------|------|------|
  | sha2::{Digest, Sha256} | SHA-256ハッシュ計算 | 安定したCryptoハッシュ。`Digest`トレイトの`update`/`finalize`使用 |
  | std::fs::read_to_string | ファイル読み込み（UTF-8前提） | 大きなファイルでは非効率、非UTF-8ではエラー |
  | std::path::Path | ファイルパス型 | 参照引数 |
  | super::{ResolutionError, ResolutionResult, Sha256Hash} | エラー/結果/ハッシュ型 | 定義はこのチャンクには現れない |
- 被依存推定（このモジュールを使用する可能性のある箇所）
  - 設定リゾルバやキャッシュ層で、設定内容の同一性チェック・キャッシュキー生成
  - ビルドシステムの依存解決で、ファイル変更検出
  - いずれも推定であり、このチャンクには具体的な呼び出し元は現れない

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| compute_sha256 | `pub fn compute_sha256(content: &str) -> Sha256Hash` | 文字列のSHA-256計算を行い16進文字列で返す | O(n) | O(1) |
| compute_file_sha | `pub fn compute_file_sha(path: &Path) -> ResolutionResult<Sha256Hash>` | ファイル内容を読み込み、SHA-256を計算。I/Oエラーをドメインエラーへ変換 | O(n) | O(n) |

詳細（各API）:

1) compute_sha256

1. 目的と責務
   - 入力文字列の**SHA-256**を計算し、64桁の小文字16進文字列として`Sha256Hash`に包んで返す（L10–15）。
2. アルゴリズム（ステップ分解）
   - `Sha256::new()`でハッシャを作成（L11）
   - `update(content.as_bytes())`でバイト列を入力（L12）
   - `finalize()`で32バイトのダイジェストを取得（L13）
   - `format!("{result:x}")`で16進小文字化（64桁）し、`Sha256Hash`へ格納（L14）
3. 引数
   | 名前 | 型 | 必須 | 説明 |
   |------|----|------|------|
   | content | &str | Yes | 入力文字列。UTF-8テキストとして扱う |
4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Sha256Hash | 64桁16進のハッシュ文字列を保持（内包フィールドは少なくともこのモジュールから可視であることがテストより示唆、L47–49） |
5. 使用例
   ```rust
   let hash = compute_sha256("hello world");
   // 例: hash.0 は "b94d27b9934d3e08a52e52d7da7dabfa..."（64桁）を保持
   ```
6. エッジケース
   - 空文字列: ハッシュは定義済み（SHA-256("")）
   - 非ASCII文字列: UTF-8として処理。問題なし
   - 非常に長い文字列: 時間O(n)、追加メモリは一定だが入力保持分は呼び出し側責務

2) compute_file_sha

1. 目的と責務
   - 指定パスのファイルを**UTF-8文字列**として読み込み、その内容のSHA-256を返す。I/Oエラーは`ResolutionError::cache_io`へ変換（L21–25）。
2. アルゴリズム（ステップ分解）
   - `std::fs::read_to_string(path)`でファイル全体をUTF-8として読み込む（L22）
   - 読み込み失敗を`ResolutionError::cache_io(path.to_path_buf(), e)`へ変換（L22–23）
   - 成功時は`compute_sha256(&content)`でハッシュ計算（L24）
3. 引数
   | 名前 | 型 | 必須 | 説明 |
   |------|----|------|------|
   | path | &Path | Yes | 入力ファイルへのパス |
4. 戻り値
   | 型 | 説明 |
   |----|------|
   | ResolutionResult<Sha256Hash> | 成功時`Ok(Sha256Hash)`、失敗時`ResolutionError` |
5. 使用例
   ```rust
   use std::path::Path;
   let path = Path::new("config.yaml");
   let hash = compute_file_sha(path)?;
   // hash.0 は 64桁16進文字列
   ```
6. エッジケース
   - ファイルが存在しない/アクセス不可: `ResolutionError::cache_io`へ変換され失敗
   - 非UTF-8バイト列を含むファイル: `read_to_string`が失敗し、上記エラーへ
   - 非常に大きなファイル: 全読み込みのためメモリ負荷大。O(n)空間
   - 空ファイル: 空文字列のSHA-256（定義済み）を返す

## Walkthrough & Data Flow

- compute_sha256（L10–15）
  - 入力`&str`を`as_bytes()`で借用し、`Sha256`ハッシャへ逐次投入。`finalize()`でダイジェスト取得後、フォーマットで16進文字列へ変換し、`Sha256Hash`（おそらく`tuple struct`）に格納。
- compute_file_sha（L21–25）
  - `Path`参照を受け取り、`std::fs::read_to_string(path)`でファイル全体を読み込む。
  - 読み込み失敗時、`map_err`で`ResolutionError::cache_io(path.to_path_buf(), e)`へ変換。ここで`PathBuf`へコピーするため小さな追加コストが発生。
  - 読み込み成功時に`compute_sha256(&content)`へ委譲。

データの流れは直線的で分岐が少なく、Mermaid図の基準（4条件以上/3状態以上/3アクター以上）に該当しないため図は省略。

根拠（関数名:行番号）
- 文字列ハッシュの更新と確定: compute_sha256:L11–L14
- ファイル読み込みとエラー変換: compute_file_sha:L22–L23
- ファイル内容からの再利用: compute_file_sha:L24

## Complexity & Performance

- compute_sha256
  - 時間計算量: O(n)（nは`content`バイト長）
  - 空間計算量: O(1)（ハッシュ状態と出力文字列のみ）
  - ボトルネック: 16進フォーマットの文字列生成（64桁）。通常軽微
- compute_file_sha
  - 時間計算量: O(n)（nはファイルサイズ）
  - 空間計算量: O(n)（`read_to_string`で全内容をメモリに保持）
  - ボトルネック: 大規模ファイルの全読み込み。I/O待機＋メモリ使用増
  - スケール限界: 数百MB以上のファイルでは遅延・メモリ圧迫。ストリーミングが望ましい
  - 実運用負荷要因: 同期I/Oにより非同期環境でのブロッキング。大量ファイル処理でスループット低下

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価

- メモリ安全性
  - Buffer overflow / Use-after-free: 安全Rustのみで`unsafe`なし。該当なし
  - Integer overflow: 該当なし
- インジェクション
  - SQL/Command/Path traversal: 本関数は任意パスを読み込むため、上位のパス検証がない場合、意図しない場所の読み込みリスクはあるが、このユーティリティ自体はI/Oのみでインジェクションはしない
- 認証・認可
  - 権限チェック漏れ: OS権限に依存。アプリ層でのパス制約が必要
  - セッション固定: 該当なし
- 秘密情報
  - Hard-coded secrets: なし
  - Log leakage: ログ出力はなし。エラーにパスを含める可能性（`cache_io(path, e)`）があり、ログに出す場合はパス情報の扱いに注意
- 並行性
  - Race condition / Deadlock: 共有状態なし。該当なし
  - 同期I/O: 非同期コンテキストでブロッキングの可能性

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | `""` | 空文字列のSHA-256を返す | compute_sha256 | ✅ |
| 非ASCII文字列 | `"日本語"` | UTF-8として問題なくハッシュ | compute_sha256 | ✅ |
| 非UTF-8ファイル | バイナリファイル | エラー（UTF-8デコード失敗） | compute_file_sha（read_to_string） | ⚠️ 設計上の制約 |
| 大規模ファイル | >100MB | 成功だがメモリ・時間増 | compute_file_sha（全読み込み） | ⚠️ パフォーマンス懸念 |
| 不存在ファイル | `/no/such/file` | `ResolutionError::cache_io`で失敗 | compute_file_sha | ✅ |
| 権限不足 | `/root/secret` | 同上 | compute_file_sha | ✅ |
| 競合アクセス | 同時読み取り | OS/FSが許す限り成功 | compute_file_sha | ✅ |

重要な主張の根拠（関数名:行番号）
- 非UTF-8ファイルで失敗する可能性: compute_file_sha:L22（`read_to_string`はUTF-8要求）
- エラー変換の挙動: compute_file_sha:L22–L23

## Design & Architecture Suggestions

- 入力の柔軟性向上
  - 提案: **compute_sha256_bytes(&[u8]) -> Sha256Hash** を追加し、バイナリ入力に対応（非UTF-8ファイルも扱える）
  - 提案: **compute_file_sha_bytes(&Path) -> ResolutionResult<Sha256Hash>** で `std::fs::read`（バイト列）を用いる
- ストリーミング処理
  - 提案: `std::fs::File` + `std::io::BufReader`でチャンク更新（`hasher.update(chunk)`）し、巨大ファイル対応とメモリ効率化
- 非同期I/O対応
  - 提案: ランタイム使用時は`tokio::fs::File` + `tokio::io::AsyncReadExt`で非同期版API（例: `async fn compute_file_sha_async`）
- エラー設計
  - `ResolutionError::cache_io`の意味（キャッシュ層のI/Oエラー？）はこのチャンクでは不明。用途に応じてエラー分類（NotFound/Permission/InvalidDataなど）を強化
  - 読み込みAPIのUTF-8前提かどうかを型や関数名に表現（例: `compute_file_sha_text` vs `compute_file_sha_bytes`）
- データ契約
  - `Sha256Hash`の内部表現と可視性を明確化（フィールド公開の最小化、`Deref<Target=str>`実装や`as_str()`提供など）

## Testing Strategy (Unit/Integration) with Examples

既存テスト（L31–50）は有用ですが、以下の追加を推奨:

- バイナリファイル（非UTF-8）でのI/Oエラー確認
  ```rust
  #[test]
  fn file_sha_non_utf8_fails() {
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    let mut path = PathBuf::from("tmp_non_utf8.bin");
    let mut f = File::create(&path).unwrap();
    // 非UTF-8のバイト
    f.write_all(&[0xff, 0xfe, 0xfd]).unwrap();

    let res = compute_file_sha(&path);
    assert!(res.is_err()); // read_to_string が失敗するはず
    std::fs::remove_file(&path).unwrap();
  }
  ```
- 空ファイルのハッシュ
  ```rust
  #[test]
  fn file_sha_empty_file() {
    use std::fs::File;
    use std::path::PathBuf;

    let path = PathBuf::from("tmp_empty.txt");
    File::create(&path).unwrap();
    let res = compute_file_sha(&path).unwrap();
    // SHA-256("") の既知値との比較も可能
    assert_eq!(res.0.len(), 64);
    std::fs::remove_file(&path).unwrap();
  }
  ```
- 大きなファイル（性能観点のスモーク）
  ```rust
  #[test]
  fn file_sha_large_file_smoke() {
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    let path = PathBuf::from("tmp_large.txt");
    let mut f = File::create(&path).unwrap();
    // 約数MBのデータを作成
    for _ in 0..5_000 {
      f.write_all(b"1234567890abcdef1234567890abcdef\n").unwrap();
    }
    let res = compute_file_sha(&path).unwrap();
    assert_eq!(res.0.len(), 64);
    std::fs::remove_file(&path).unwrap();
  }
  ```
- 文字列APIの空文字列・非ASCII
  ```rust
  #[test]
  fn sha256_handles_empty_and_unicode() {
    let empty = compute_sha256("");
    assert_eq!(empty.0.len(), 64);
    let jp = compute_sha256("日本語コンテンツ");
    assert_eq!(jp.0.len(), 64);
  }
  ```

エラー設計の検証
- `ResolutionError::cache_io`のバリアント・メッセージ（不明）に対し、NotFound/Permission/InvalidData等をトリガーする入力で分類が期待通りかを検証（このチャンクには詳細が現れないため、別モジュールの定義に依存）

## Refactoring Plan & Best Practices

- API強化
  1. 追加: `pub fn compute_sha256_bytes(bytes: &[u8]) -> Sha256Hash`
  2. 追加: `pub fn compute_file_sha_bytes(path: &Path) -> ResolutionResult<Sha256Hash>`（`std::fs::read`使用）
  3. 追加（任意）: `pub async fn compute_file_sha_async(path: &Path) -> ResolutionResult<Sha256Hash>`（非同期I/O）
- 実装最適化
  - ストリーミング: `BufReader` + 8–64KBチャンクで`hasher.update`を繰り返す
  - 文字列/バイトの両対応により、用途に応じたI/O選択を可能にする
- 型設計
  - `Sha256Hash`のフィールドを非公開化し、`impl Display` or `as_str()`を提供
  - 必要なら型安全な新タイプ（lowercase・固定長保証）を強化
- エラー伝播
  - `ResolutionError`に`source()`実装を付与し、`std::error::Error`チェーンを維持
  - 失敗時のコンテキスト（パス、操作種別）を含める

## Observability (Logging, Metrics, Tracing)

- ロギング
  - I/O失敗時に呼び出し側で`warn!`/`error!`を出せるよう、エラー型に十分なコンテキスト（パス・OSエラー）を含める
- メトリクス
  - ハッシュ成功/失敗のカウンタ、処理バイト数のヒストグラム、処理時間分布（大規模ファイルの可視化に有用）
- トレーシング
  - `tracing`でスパン（例: `hash_file`）を張り、パス・サイズ・結果をタグ化
- 現状: 本チャンクには観測コードは現れない

## Risks & Unknowns

- `Sha256Hash`の定義と可視性
  - テストが`hash.0`にアクセス（L47–49）していることから、少なくともこのモジュールから第一フィールドが可視だが、型の場所・公開範囲の詳細は不明（このチャンクには現れない）
- `ResolutionError::cache_io`の詳細
  - どのようなカテゴリ・メッセージ・`source`を持つか不明。適切な分類かは上位設計次第
- `ResolutionResult`の実体
  - `type Result<T> = std::result::Result<T, E>`に類する型エイリアスと推定されるが、確証はない（このチャンクには現れない）
- 呼び出し元の利用パターン
  - 非同期環境/大規模ファイル/バイナリ設定の有無は不明。要件によっては現実装の制約が影響しうる

Rust特有の観点（このファイルの状況）

- メモリ安全性・所有権/借用
  - 入力は`&str`/`&Path`の不変借用。`Sha256Hash`は新規`String`生成で所有権移動（compute_sha256:L14）
  - 明示的ライフタイムは不要。`unsafe`ブロックは存在しない（不使用）
- 並行性・非同期
  - `Send/Sync`境界へ影響する共有状態はない。同期I/Oにより非同期タスクのブロッキングに注意
- エラー設計
  - 失敗は`ResolutionResult`で返し、`map_err`でドメインエラーへ変換（compute_file_sha:L22–L23）
  - `unwrap`/`expect`は本体に存在しない（テストには`unwrap`が登場する例を追加したが、既存テストは使用していない）