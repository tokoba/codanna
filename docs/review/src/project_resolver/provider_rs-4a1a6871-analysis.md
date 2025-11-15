# project_resolver\provider.rs Review

## TL;DR

- 目的: 多言語のプロジェクト設定ファイル（例: tsconfig.json, pyproject.toml, go.mod）を解決するための共通プロバイダ用**トレイト**を定義し、実装者が言語固有ロジックを提供できるようにする。
- 主要公開API: **ProjectResolutionProvider**（Send + Sync）トレイトの6メソッド（language_id, is_enabled, config_paths, compute_shas, rebuild_cache, select_affected_files）。
- 複雑箇所: compute_shasはI/O・ハッシュ計算により**O(Σファイルサイズ)**の負荷・エラー多発箇所。PathBufをキーにしたHashMapの**パス同一性**問題（正規化・大小文字差）。
- 重大リスク: 非同期未対応のI/O集中、**並行アクセス時のキャッシュ整合性**（Send+Sync要件）、**解決結果のエラー型の設計不明**（ResolutionResultの詳細不明）。
- 安全性: unsafeは不使用。**所有権/借用は安全**（参照引数、所有Vec/HashMapを返す）。ただし実装側の**内部可変性（Mutex/RwLock）**の設計が必要。
- テスト要点: 設定フラグの有効・無効、存在しないファイルのハッシュ計算エラー、重複パス処理、キャッシュ再構築の**冪等性**、並行呼び出しの**競合回避**。
- 観測性: compute_shasに**ハッシュ数/失敗数/時間**メトリクス、トレイト各メソッドのトレーススパン、設定起因の分岐ログ。

## Overview & Purpose

このファイルは、言語固有のプロジェクト設定解決ロジックを統一的に扱うための**コア・トレイト**を定義しています。TypeScript、Python、Goなどの各プロバイダがこのトレイトを実装することで、以下の共通機能を提供できます。

- 言語IDの公開
- 設定に基づく有効/無効判定
- 管理対象の設定ファイルパスの列挙
- 設定ファイルのSHA-256ハッシュ計算
- プロバイダ内部キャッシュの再構築
- 設定変更に影響を受けるファイル選択

この設計により、上位モジュールは言語に依存せず統一インターフェースで処理できます。

根拠: ProjectResolutionProviderトレイト定義（行番号はこのチャンクに含まれないため不明）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Trait | ProjectResolutionProvider | pub | 言語別設定解決の共通インターフェース | Med |

### Dependencies & Interactions

- 内部依存
  - このファイル内で定義されるのはトレイトのみで、内部関数呼び出しはありません。
  - トレイトメソッドの連携としては、通常のフローで「is_enabled → config_paths → compute_shas → rebuild_cache / select_affected_files」のような順序で使用されることが想定されます（実装・呼び出し側次第）。

- 外部依存（use）
  | 依存元 | シンボル | 目的 |
  |--------|----------|------|
  | std::path | PathBuf | 設定ファイルのパス表現 |
  | super | ResolutionResult | メソッドの結果型（成功/失敗） |
  | super | Sha256Hash | 設定ファイルのSHA-256ハッシュ表現 |
  | crate::config | Settings | 言語プロバイダの有効化やパスなどの設定 |

- 被依存推定（このモジュールを使用しうる箇所）
  - プロジェクト解決のオーケストレーション層（プロバイダを列挙・選別し、処理を実行するモジュール）
  - CLIやサーバのエンドポイントからの「設定再読込」「影響ファイル抽出」機能
  - キャッシュ管理コンポーネント（rebuild_cacheを呼び出す）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| language_id | fn language_id(&self) -> &'static str | 言語識別子の取得 | O(1) | O(1) |
| is_enabled | fn is_enabled(&self, settings: &Settings) -> bool | 設定に基づくプロバイダ有効判定 | O(1)〜O(k) | O(1) |
| config_paths | fn config_paths(&self, settings: &Settings) -> Vec<PathBuf> | 管理対象設定ファイルの列挙 | O(k) | O(k) |
| compute_shas | fn compute_shas(&self, configs: &[PathBuf]) -> ResolutionResult<HashMap<PathBuf, Sha256Hash>> | 設定ファイルのSHA-256計算 | O(Σ|file|) | O(n) |
| rebuild_cache | fn rebuild_cache(&self, settings: &Settings) -> ResolutionResult<()> | プロバイダ内部キャッシュ再構築 | 不明 | 不明 |
| select_affected_files | fn select_affected_files(&self, settings: &Settings) -> Vec<PathBuf> | 設定変更に影響されるファイル選択 | O(m) | O(m) |

注意: k, n, mはそれぞれ設定パス数、ハッシュ対象ファイル数、影響推定対象ファイル数。実コストは実装依存。行番号はこのチャンクに含まれないため不明。

---

以下、各APIの詳細。

### language_id

1. 目的と責務
   - 言語の識別子文字列（例: "typescript", "python", "go"）を返します。
   - 静的文字列として返すため、ライフタイムや所有権の問題がないのが特徴。

2. アルゴリズム（期待挙動）
   - 定数を返すのみ。

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &self | プロバイダインスタンス参照 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | &'static str | 言語識別子（不変、静的寿命） |

5. 使用例
   ```rust
   struct TsProvider;
   impl ProjectResolutionProvider for TsProvider {
       fn language_id(&self) -> &'static str {
           "typescript"
       }
       // ほかのメソッドは後述の例参照
       fn is_enabled(&self, _settings: &crate::config::Settings) -> bool { true }
       fn config_paths(&self, _settings: &crate::config::Settings) -> Vec<std::path::PathBuf> { vec![] }
       fn compute_shas(&self, _configs: &[std::path::PathBuf])
         -> super::ResolutionResult<std::collections::HashMap<std::path::PathBuf, super::Sha256Hash>> {
           Ok(std::collections::HashMap::new())
       }
       fn rebuild_cache(&self, _settings: &crate::config::Settings) -> super::ResolutionResult<()> { Ok(()) }
       fn select_affected_files(&self, _settings: &crate::config::Settings) -> Vec<std::path::PathBuf> { vec![] }
   }
   ```

6. エッジケース
   - なし（定数返却）

### is_enabled

1. 目的と責務
   - Settingsに基づき、プロバイダを有効化するかを判定。

2. アルゴリズム（期待挙動）
   - Settingsの該当言語フラグや条件をチェックして真偽を返す。

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &self | プロバイダ |
   | settings | &Settings | 設定（有効化フラグやパスなど） |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | bool | 有効ならtrue |

5. 使用例
   ```rust
   let settings = crate::config::Settings::default(); // 仮
   let provider = TsProvider;
   if provider.is_enabled(&settings) {
       // 次のステップへ
   }
   ```

6. エッジケース
   - Settingsが不完全・デフォルトの場合の扱い
   - 言語名のミスマッチ（language_idと設定キーの齟齬）

### config_paths

1. 目的と責務
   - 設定に基づき、対象となる設定ファイルのPathBuf一覧を返す。

2. アルゴリズム（期待挙動）
   - Settingsからルートディレクトリやパターンを読み取り、存在確認・フィルタリングして返却。

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &self | プロバイダ |
   | settings | &Settings | 検索ルート、パターン、除外ルールなど（詳細不明） |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Vec<PathBuf> | 設定ファイルパスの一覧 |

5. 使用例
   ```rust
   let paths = provider.config_paths(&settings);
   for p in &paths {
       println!("config: {}", p.display());
   }
   ```

6. エッジケース
   - パスが空/見つからない
   - 重複パス
   - シンボリックリンクの扱い
   - クロスプラットフォームの区切り文字・ケース差

### compute_shas

1. 目的と責務
   - 入力された設定ファイル群のSHA-256を計算し、PathBuf→Sha256Hashのマップを返す。I/Oと計算の中心。

2. アルゴリズム（期待挙動）
   - 各PathBufを開く
   - ファイル内容を読み込みストリーミングでSHA-256計算
   - マップに挿入
   - いずれかのファイルでエラーがあればResolutionResultで失敗を返す（詳細はResolutionResult次第）

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &self | プロバイダ |
   | configs | &[PathBuf] | ハッシュ対象設定ファイル |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | ResolutionResult<HashMap<PathBuf, Sha256Hash>> | 成功時はパス→ハッシュ、失敗時はエラー |

5. 使用例
   ```rust
   use std::collections::HashMap;
   let configs = provider.config_paths(&settings);
   match provider.compute_shas(&configs) {
       Ok(map) => {
           for (path, sha) in map.iter() {
               println!("{} => {}", path.display(), sha); // Sha256HashのDisplayは実装次第
           }
       }
       Err(err) => {
           eprintln!("hashing failed: {:?}", err);
       }
   }
   ```

6. エッジケース
   - ファイルが存在しない/アクセス権がない
   - 非正規パス/相対パス混在（PathBufのキー同一性問題）
   - 非常に大きなファイル（メモリ・時間）
   - 途中でI/Oエラー（部分結果の扱い）
   - バイナリ/テキストのエンコーディング問題（ハッシュとしては問題ないが読み方に注意）

### rebuild_cache

1. 目的と責務
   - Settingsに基づいてプロバイダ内部のキャッシュを再構築。設定変更反映の起点。

2. アルゴリズム（期待挙動）
   - 既存キャッシュを破棄または更新
   - 必要な派生データ（例: 正規化済みパス、パース済み設定）を再計算
   - エラーがあればResolutionResultで返す

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &self | プロバイダ |
   | settings | &Settings | キャッシュ構築に必要な情報 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | ResolutionResult<()> | 成否のみ |

5. 使用例
   ```rust
   if let Err(err) = provider.rebuild_cache(&settings) {
       eprintln!("cache rebuild failed: {:?}", err);
   }
   ```

6. エッジケース
   - 設定不整合（必要項目欠落）
   - 既存キャッシュとの不整合（ロックなしで同時更新）
   - 大規模設定で時間がかかる

### select_affected_files

1. 目的と責務
   - 設定変更によって影響を受けるファイル群を選択（再ビルド/再解析対象など）。

2. アルゴリズム（期待挙動）
   - Settingsとキャッシュ状態を参照して影響範囲を推定し、PathBuf一覧を返す

3. 引数
   | 名前 | 型 | 説明 |
   |------|----|------|
   | self | &self | プロバイダ |
   | settings | &Settings | 影響計算に必要な条件 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Vec<PathBuf> | 影響を受けるファイル一覧 |

5. 使用例
   ```rust
   let affected = provider.select_affected_files(&settings);
   // 影響ファイルに対し再処理を指示する
   ```

6. エッジケース
   - 設定変更が小さく影響なし（空ベクタ）
   - キャッシュ未構築時の扱い（防御的に全件？空？）
   - 設定の差分計算ロジック不明による誤選択

## Walkthrough & Data Flow

- 一般的なフロー（想定）
  1. 起点は上位モジュール。Settingsをロード。
  2. 各プロバイダへis_enabled(settings)でフィルタ。
  3. 有効なプロバイダに対してconfig_paths(settings)で設定ファイルを列挙。
  4. compute_shas(configs)でハッシュを計算し、変更検出やキャッシュキーに使用。
  5. 変更検出後、必要に応じてrebuild_cache(settings)を呼びキャッシュ更新。
  6. select_affected_files(settings)で再処理対象ファイルを決定。

- データの流れ
  - Settings（参照）→パス列挙（所有Vec<PathBuf）→ハッシュ計算（所有HashMap<PathBuf, Sha256Hash）→結果（ResolutionResultで伝播）

- ライフタイム/所有権
  - 全メソッドは&selfで受け、引数は&Settingsや&[PathBuf]の参照。戻り値は所有するVec/HashMap。借用が外へ漏れず安全。

根拠: トレイトメソッドシグネチャ（行番号不明）

## Complexity & Performance

- language_id: O(1) / O(1)
- is_enabled: 設定参照の範囲に依存。一般にO(1)〜O(k)（キー検索やフラグ判定）/ O(1)
- config_paths: パターンマッチやファイル探索を行う場合O(k + ディスク探索コスト)。I/Oボトルネック。
- compute_shas: O(Σ|file|)時間、O(n)空間（nはファイル数）。I/Oとハッシュ計算がボトルネック。並列化の余地あり。
- rebuild_cache: 実装依存。不明。高コストの可能性あり（パース、検証、索引構築）。
- select_affected_files: 実装依存。差分計算やグラフ解析があるとO(E)〜O(V+E)に拡大。

スケール限界・ボトルネック
- 大量設定ファイル・巨大ファイルでcompute_shasが支配的。
- 単一スレッドI/Oではレイテンシ増大。非同期化や並列化（スレッドプール）で緩和可能。
- PathBufをHashMapキーにする場合、同一ファイルの別表現（相対/絶対、シンボリックリンク）による重複・不一致に注意。

実運用負荷要因
- ディスクI/O、ネットワークファイルシステム（NFS）上のパス、権限チェック。
- 設定パース（rebuild_cacheで発生）や依存解決。

## Edge Cases, Bugs, and Security

セキュリティチェックリストに沿った評価（このファイルはトレイトのみのため挙動は実装依存。以下は実装時の注意。）

- メモリ安全性
  - Buffer overflow: 標準ライブラリ使用とRustの型安全により通常発生しない。ファイル読み込み時のバッファサイズ管理は実装側で適切に。
  - Use-after-free: 参照引数のみで所有権は返り値に閉じるため安全。内部キャッシュが参照を保持する場合はライフタイム管理に注意。
  - Integer overflow: ファイルサイズや合計バイト数の集計でu64を使用推奨。
- インジェクション
  - Path traversal: config_pathsやcompute_shasが外部入力を許す場合、相対パス・親ディレクトリ参照を正規化・検証する。
  - SQL/Command: 本トレイトには該当なし。
- 認証・認可
  - 権限チェック漏れ: compute_shasでアクセス権のないファイルに対処（適切なエラー伝播）。
  - セッション固定: 該当なし。
- 秘密情報
  - Hard-coded secrets: 該当なし。
  - Log leakage: ファイル内容やハッシュ値のログ出力は慎重に。ハッシュは内容の存在証明に使われうるため扱いに注意。
- 並行性
  - Race condition: トレイトにSend + Sync境界があり、実装が内部キャッシュを持つ場合はMutex/RwLockで保護する。
  - Deadlock: 複数メソッドで同一ロックをネストするとデッドロックリスク。ロック順序・粒度の設計。
  - 共有状態の更新と参照の分離（read-heavyならRwLock）。
- Rust特有の観点
  - 所有権: 引数は参照（&Settings, &[PathBuf]）、戻り値は所有Vec/HashMap。外部に安全に引き渡せる。
  - 借用: &selfのみ。内部可変性を使う場合はSync境界を満たすように。
  - ライフタイム: language_idの&'static strは安全。Settings参照寿命は呼び出しスコープに限定。
  - unsafe境界: このファイルにunsafeは登場しない（不明扱いではなく「該当なし」）。
  - Send/Sync: トレイト全体にSend + Syncを要求し、実装はスレッド安全である必要。
  - await境界/非同期: メソッドは同期的。I/O集中の場合はasync版の導入検討。
  - エラー設計: ResolutionResultの詳細不明。I/O/検証/キャッシュエラーの分類とDisplay/Debugの整備が望ましい。
  - panic箇所: unwrap/expect禁止。エラーはResolutionResultで返す。

詳細エッジケース表（期待挙動は仕様策定が必要）

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空の設定パス一覧 | configs = [] | Ok(empty map) | compute_shas | 不明 |
| ファイルなし | configs = ["missing.json"] | Err(NotFound) | compute_shas | 不明 |
| アクセス不可 | configs = ["secret.json"] | Err(PermissionDenied) | compute_shas | 不明 |
| 重複パス | configs = ["a.json","./a.json"] | 正規化して重複排除 or 別エントリ | compute_shas | 不明 |
| 大容量ファイル | configsに1GB超 | ストリーミングで計算、タイムアウト考慮 | compute_shas | 不明 |
| 設定無効 | settingsで無効化 | is_enabledがfalse | is_enabled | 実装次第 |
| キャッシュ未構築で影響選択 | 初回起動 | 空 or フルスキャン | select_affected_files | 不明 |

「実装」「状態」はこのチャンクには現れないため不明。

## Design & Architecture Suggestions

- 明確なエラー型
  - ResolutionResultの中身を統一（例: enum ResolutionError { Io, Parse, InvalidConfig, Cache, ... }）し、各メソッドから適切に返す。
- 非同期対応
  - compute_shasをasyncにし、Tokio等で非同期I/O・並列ハッシュ計算を可能にする。または内部的にスレッドプールを使用。
- パス正規化ポリシー
  - HashMapキーにPathBufを用いる場合、canonicalize（許可環境下）やケース感度ルールを定義し、重複・不一致を回避。
- キャッシュの整合性
  - rebuild_cacheとselect_affected_files間の契約（キャッシュ必須/任意）をドキュメント化。内部状態はRwLockで保護。
- 既定実装（default methods）
  - 一部メソッドにデフォルト実装（例: is_enabledの基本判定、config_pathsの簡易フィルタ）を提供すると実装コスト低減。
- 拡張性
  - 設定ファイル以外（環境変数、プラグイン設定）にも対応可能なAPI拡張を検討。

## Testing Strategy (Unit/Integration) with Examples

- Unitテスト
  - is_enabledのON/OFF
  - config_pathsのフィルタリング、重複排除、存在確認
  - compute_shasの成功/失敗（NotFound, PermissionDenied, 部分失敗時の挙動）
  - rebuild_cacheの冪等性（連続呼び出しで同状態）
  - select_affected_filesの差分推定（設定変更前後）

- 並行性テスト
  - 複数スレッドからcompute_shas/rebuild_cache/select_affected_filesを同時呼び出しし、データ競合がないことを検証。

- 例（簡易モックプロバイダ）
  ```rust
  use std::{collections::HashMap, path::PathBuf};
  use project_resolver::{provider::ProjectResolutionProvider, ResolutionResult, Sha256Hash};
  use crate::config::Settings;

  struct MockProvider;
  impl ProjectResolutionProvider for MockProvider {
      fn language_id(&self) -> &'static str { "mock" }
      fn is_enabled(&self, _settings: &Settings) -> bool { true }
      fn config_paths(&self, _settings: &Settings) -> Vec<PathBuf> {
          vec![PathBuf::from("tests/data/a.json"), PathBuf::from("tests/data/b.json")]
      }
      fn compute_shas(&self, configs: &[PathBuf])
        -> ResolutionResult<HashMap<PathBuf, Sha256Hash>> {
          let mut map = HashMap::new();
          for p in configs {
              // 実装ではファイル読み込みとSHA-256計算
              // ここではダミー値を使用
              map.insert(p.clone(), Sha256Hash::from_hex("00".repeat(32)).unwrap()); /* ... 仮 ... */
          }
          Ok(map)
      }
      fn rebuild_cache(&self, _settings: &Settings) -> ResolutionResult<()> { Ok(()) }
      fn select_affected_files(&self, _settings: &Settings) -> Vec<PathBuf> {
          vec![PathBuf::from("src/main.ts")]
      }
  }

  #[test]
  fn test_enabled_and_paths() {
      let p = MockProvider;
      let s = Settings::default(); // 仮
      assert!(p.is_enabled(&s));
      let paths = p.config_paths(&s);
      assert!(!paths.is_empty());
  }
  ```

注意: Sha256Hash::from_hex等はこのチャンクには現れないため仮。

## Refactoring Plan & Best Practices

- トレイトドキュメントの契約強化
  - 各メソッドの期待する前提条件・事後条件をRustdocで明示（例: compute_shasは存在確認済みのファイルを入力とする）。
- PathBufキーの方針統一
  - 正規化を行うヘルパー関数を用意し、全プロバイダで共通利用。
- Result型の標準化
  - ResolutionResultのエイリアスだけでなく、エラー型の階層を設計しFrom/Intoで変換容易に。
- ログと計測の仕組みをデフォルト化
  - トレイトの既定実装で計測フック（トレース/メトリクス）を用意し、実装者はコアロジックに集中。
- 非同期版トレイトの検討
  - 高負荷環境向けにasyncトレイト（dyn-traitではasync-traitまたはGATsを活用）を併設。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - is_enabled: 設定値による判定理由をdebugレベルで。
  - config_paths: 取得件数、除外件数、探索時間。
  - compute_shas: 開始/終了、対象件数、失敗件数、各失敗の原因（I/O/パース）。
- メトリクス
  - provider_language_idごとのハッシュ計算時間（ヒストグラム）
  - ハッシュ対象ファイル数、エラー率
  - キャッシュ再構築時間・成功率
- トレーシング
  - 各メソッドにspanを張り、settingsハッシュ（安全な範囲）や対象件数をタグ化。
  - 呼び出しチェーン（is_enabled→config_paths→compute_shas→rebuild_cache→select_affected_files）を1トレースで関連付け。

## Risks & Unknowns

- ResolutionResult/Sha256Hash/Settingsの詳細不明
  - エラーの分類・伝播、ハッシュ表示・比較、設定の構造が不明であり、実装判断に影響。
- 非同期・並列化の方針不明
  - 同期トレイトのまま高負荷に耐えるか、async化するかの設計判断が未確定。
- キャッシュの有無と整合性契約不明
  - rebuild_cacheの必須性、有効期限、更新タイミングなどが上位設計に依存。
- パス正規化・重複排除ポリシー不明
  - HashMapキーのPathBufの扱いが環境差により不定。
- 行番号情報がこのチャンクにないため、コード内具体箇所の参照は「不明」。