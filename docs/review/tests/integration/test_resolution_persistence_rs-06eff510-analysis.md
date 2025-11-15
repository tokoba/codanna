# integration\test_resolution_persistence.rs Review

## TL;DR

- 目的: 永続化層の基本動作検証（保存→読込→差分判定）と出力ファイル構造の確認
- 主要公開API: このファイル自体の公開APIはなし。使用外部APIは ResolutionPersistence::{new, save, load}, ResolutionIndex::{new, update_sha, add_mapping, set_rules, needs_rebuild}
- 複雑箇所: なし（直線的なI/Oテスト）
- 重大リスク: データ契約（マッピング・ルール）の保存/読込の同値性検証が不足、後処理でエラー無視、レース条件（並列実行）に対する一意ディレクトリ未使用
- Rust安全性: unsafe不使用、短寿命参照と所有権は良好、expectで失敗即failする点はテストとして妥当
- パフォーマンス: I/O中心、テスト規模では問題なし。大量データ時のスケール検証は未実施
- セキュリティ: 機密情報やインジェクションの懸念は低いが、ファイル権限・破損ファイルの取り扱いテストが未網羅

## Overview & Purpose

このファイルは、codanna::project_resolver::persist に属する永続化機能（ResolutionPersistence/ResolutionIndex/ResolutionRules）の統合テストです。主に以下を確認します。

- ResolutionIndex を構築し、ResolutionPersistence::save で保存、ResolutionPersistence::load で復元できること
- 復元後の needs_rebuild が、SHAの異同に応じて適切に真偽を返すこと
- 永続化先のファイルパスが期待どおりに作成されること

2つのテストが含まれ、I/Oによる実ファイルの作成と削除を行います。テストは一時ディレクトリ下に作業領域を作成し、最後に削除します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | tests | private | 統合テストモジュール | Low |
| Function | test_resolution_persistence_save_and_load | private | インデックスの保存/読込と needs_rebuild 判定の検証 | Low |
| Function | test_resolution_persistence_file_structure | private | 期待される永続化ファイルの存在確認 | Low |
| External Struct | ResolutionPersistence | external | インデックス永続化の入口（保存/読込） | Med（推測） |
| External Struct | ResolutionIndex | external | 解決インデックスの保持、SHA・マッピング・ルールの操作 | Med（推測） |
| External Struct | ResolutionRules | external | tsconfigベースのパス解決ルールの保持 | Low（推測） |
| External Struct | Sha256Hash | external | SHA-256のハッシュ値表現 | Low（推測） |

### Dependencies & Interactions

- 内部依存
  - なし（2つのテスト関数は独立）

- 外部依存（表）

| クレート/モジュール | 型/関数 | 用途 |
|--------------------|---------|------|
| codanna::project_resolver::persist | ResolutionPersistence::{new, save, load} | インデックスの永続化管理 |
| codanna::project_resolver::persist | ResolutionIndex::{new, update_sha, add_mapping, set_rules, needs_rebuild} | インデックスの構築・照会 |
| codanna::project_resolver | Sha256Hash::from_bytes | SHA値の生成 |
| std::env | temp_dir | 一時ディレクトリのルート取得 |
| std::fs | create_dir_all, remove_dir_all | テスト用ディレクトリの作成/削除 |
| std::path | Path, PathBuf::join | パス操作 |
| std::collections | HashMap | ルールパスの定義 |

- 被依存推定
  - このテストは永続化機能の信頼性を担保するため、CIやリリース前の統合検証で使用されると推定されます。他モジュール（プロジェクト解決ロジック）に対して、永続化層が正しく動作しているという前提を提供します。

## API Surface (Public/Exported) and Data Contracts

このファイル自体が提供する公開APIはありません（テスト専用）。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| 該当なし | 該当なし | 該当なし | 該当なし | 該当なし |

参考として、このファイルで使用している外部APIの呼び出し形を示します（シグネチャ詳細はこのチャンクには現れないため不明）。

| API名 | 呼び出し形（このファイルからの利用） | 目的 | Time | Space |
|-------|-----------------------------|------|------|-------|
| ResolutionPersistence::new | ResolutionPersistence::new(&temp_dir) | 永続化マネージャの生成 | 不明 | 不明 |
| ResolutionPersistence::save | persistence.save("typescript", &index) | インデックスの保存 | 不明 | 不明 |
| ResolutionPersistence::load | persistence.load("typescript) | インデックスの読込 | 不明 | 不明 |
| ResolutionIndex::new | ResolutionIndex::new() | 空インデックスの生成 | 不明 | 不明 |
| ResolutionIndex::update_sha | index.update_sha(tsconfig_path, &Sha256Hash::from_bytes(&[42;32])) | パス→SHAの登録 | 不明 | 不明 |
| ResolutionIndex::add_mapping | index.add_mapping("src/**/*.ts", tsconfig_path) | グロブ→tsconfigのマッピング追加 | 不明 | 不明 |
| ResolutionIndex::set_rules | index.set_rules(tsconfig_path, rules) | tsconfig→ルールの設定 | 不明 | 不明 |
| ResolutionIndex::needs_rebuild | loaded_index.needs_rebuild(tsconfig_path, &Sha256Hash::from_bytes(&[43;32])) | 再ビルド要否判定 | 不明 | 不明 |
| Sha256Hash::from_bytes | Sha256Hash::from_bytes(&[n;32]) | バイト列からSHA生成 | 不明 | 不明 |

各APIの詳細（このファイルから推測できる範囲で記述。明確な仕様は不明）:

1) ResolutionPersistence::save
- 目的と責務: 指定キー（例: "typescript"）に紐づくインデックスをファイルへ保存
- アルゴリズム（推測）:
  - ベースディレクトリ配下に保存先パスを組み立てる
  - インデックスをシリアライズしてファイルへ書き込む
- 引数:
  - key: &str（推測）
  - index: &ResolutionIndex（推測）
- 戻り値:
  - Result<(), E>（このファイルでは expect 使用）
- 使用例:
  ```rust
  // 重要部分のみ抜粋
  let temp_dir = std::env::temp_dir().join("codanna_test_persist");
  let persistence = ResolutionPersistence::new(&temp_dir);
  let mut index = ResolutionIndex::new();
  /* ... 省略: indexの構築 ... */
  persistence.save("typescript", &index).expect("Failed to save");
  ```
- エッジケース:
  - ベースディレクトリが存在しない/権限不足
  - 既存ファイルの上書き
  - シリアライズ失敗（不正データ）

2) ResolutionPersistence::load
- 目的と責務: 指定キーに紐づくインデックスをファイルから復元
- アルゴリズム（推測）:
  - 保存パスを探索
  - JSON/バイナリなどからデシリアライズ
- 引数: key: &str（推測）
- 戻り値: Result<ResolutionIndex, E>
- 使用例:
  ```rust
  let loaded_index = persistence.load("typescript").expect("Failed to load");
  ```
- エッジケース:
  - ファイル不存在 → Err
  - ファイル破損 → Err
  - バージョン不一致 → Err（推測）

3) ResolutionIndex::update_sha
- 目的: パスと対応するSHAを更新
- 使用例:
  ```rust
  let tsconfig_path = Path::new("examples/typescript/tsconfig.json");
  index.update_sha(tsconfig_path, &Sha256Hash::from_bytes(&[42; 32]));
  ```
- エッジケース:
  - 同一パスへ複数回更新（上書き）

4) ResolutionIndex::add_mapping
- 目的: グロブパターンとtsconfigパスの対応を追加
- 使用例:
  ```rust
  index.add_mapping("src/**/*.ts", tsconfig_path);
  ```

5) ResolutionIndex::set_rules
- 目的: tsconfig単位のルール設定
- 使用例:
  ```rust
  let rules = ResolutionRules {
      base_url: Some("./".to_string()),
      paths: HashMap::from([
          ("@components/*".to_string(), vec!["src/components/*".to_string()]),
          ("@utils/*".to_string(), vec!["src/utils/*".to_string()]),
      ]),
  };
  index.set_rules(tsconfig_path, rules);
  ```

6) ResolutionIndex::needs_rebuild
- 目的: 保存済み情報と現在のSHA比較による再ビルド要否判定
- 使用例:
  ```rust
  assert!(loaded_index.needs_rebuild(
      tsconfig_path,
      &Sha256Hash::from_bytes(&[43; 32])
  ));
  assert!(!loaded_index.needs_rebuild(
      tsconfig_path,
      &Sha256Hash::from_bytes(&[42; 32])
  ));
  ```
- エッジケース:
  - 未登録パスに対する判定（期待値は不明）

データ契約（このチャンクから読み取れる範囲）:
- ResolutionRules: base_url: Option<String>, paths: HashMap<String, Vec<String>>
- ResolutionIndex: tsconfig_path（Path）をキーとした SHA・マッピング・ルールの集合（詳細構造は不明）
- 永続化ファイル構造: index/resolvers/typescript_resolution.json（拡張子は .json と推定）

## Walkthrough & Data Flow

テスト1: test_resolution_persistence_save_and_load
- フロー
  1. 一時ディレクトリの作成（temp_dir/codanna_test_persist）
  2. ResolutionPersistence を初期化（ベースディレクトリに temp_dir）
  3. ResolutionIndex を作成し、SHA/マッピング/ルールを設定
  4. save("typescript", &index) 実行
  5. load("typescript") 実行
  6. needs_rebuild(tsconfig_path, sha_diff) → true の検証
  7. needs_rebuild(tsconfig_path, sha_same) → false の検証
  8. 一時ディレクトリ削除

- コード抜粋
  ```rust
  #[test]
  fn test_resolution_persistence_save_and_load() {
      let temp_dir = std::env::temp_dir().join("codanna_test_persist");
      fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
      let persistence = ResolutionPersistence::new(&temp_dir);

      let mut index = ResolutionIndex::new();
      let tsconfig_path = Path::new("examples/typescript/tsconfig.json");
      index.update_sha(tsconfig_path, &Sha256Hash::from_bytes(&[42; 32]));
      index.add_mapping("src/**/*.ts", tsconfig_path);
      index.set_rules(tsconfig_path, ResolutionRules {
          base_url: Some("./".to_string()),
          paths: HashMap::from([
              ("@components/*".to_string(), vec!["src/components/*".to_string()]),
              ("@utils/*".to_string(), vec!["src/utils/*".to_string()]),
          ]),
      });

      persistence.save("typescript", &index).expect("Failed to save");
      let loaded_index = persistence.load("typescript").expect("Failed to load");

      assert!(loaded_index.needs_rebuild(tsconfig_path, &Sha256Hash::from_bytes(&[43; 32])));
      assert!(!loaded_index.needs_rebuild(tsconfig_path, &Sha256Hash::from_bytes(&[42; 32])));

      fs::remove_dir_all(&temp_dir).ok();
  }
  ```

テスト2: test_resolution_persistence_file_structure
- フロー
  1. 一時ディレクトリの作成（temp_dir/codanna_test_persist_structure）
  2. ResolutionPersistence を初期化
  3. ResolutionIndex に最小限のSHA設定のみ
  4. save("typescript", &index) 実行
  5. 期待ファイル（index/resolvers/typescript_resolution.json）の存在確認
  6. 一時ディレクトリ削除

- コード抜粋
  ```rust
  #[test]
  fn test_resolution_persistence_file_structure() {
      let temp_dir = std::env::temp_dir().join("codanna_test_persist_structure");
      fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
      let persistence = ResolutionPersistence::new(&temp_dir);

      let mut index = ResolutionIndex::new();
      let tsconfig_path = Path::new("examples/typescript/tsconfig.json");
      index.update_sha(tsconfig_path, &Sha256Hash::from_bytes(&[1; 32]));

      persistence.save("typescript", &index).expect("Failed to save");

      let expected_file = temp_dir.join("index/resolvers/typescript_resolution.json");
      assert!(expected_file.exists(), "Resolution file should exist at {expected_file:?}");

      fs::remove_dir_all(&temp_dir).ok();
  }
  ```

## Complexity & Performance

- 時間計算量
  - テストコード自体は O(1) 近似（固定数操作）
  - save/load の計算量はインデックス内要素数を n とすると、シリアライズ/デシリアライズで O(n) と推測（このチャンクでは不明）
  - needs_rebuild は対象パスの照会で O(1) または O(log n)/O(1)（内部構造次第、詳細不明）

- 空間計算量
  - インメモリの ResolutionIndex は登録項目数に比例して O(n)
  - 永続化ファイルサイズも O(n)

- ボトルネック
  - ディスクI/O（ファイル書き込み/読み込み）が支配的
  - ハッシュ計算は本テストでは from_bytes のみでコスト極小

- スケール限界
  - 大規模なインデックス（多数パス・ルール）での save/load 時間・メモリ増加が想定
  - JSON形式の場合、巨大ファイルの読み込みコスト・エラー耐性（破損時の復旧）も課題

- 実運用負荷要因
  - ファイルシステムのレイテンシ・ロック
  - 権限やパス長、複数プロセスからの同時アクセス

## Edge Cases, Bugs, and Security

セキュリティチェックリストとエッジケース評価（このチャンクで確認可能な範囲）。

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（unsafe未使用、標準APIのみ）
  - 所有権/借用: &temp_dir を参照受け渡し、tsconfig_path は関数スコープ内有効。借用期間は適切
  - ライフタイム: 明示的ライフタイムは不要。Path参照の使用はスコープ内に限定

- インジェクション
  - SQL/Command/Path traversal: 外部入力なし、パスはテストで固定生成。Path traversalのリスク低
  - シリアライズ/デシリアライズ: フォーマット注入の可能性は理論上あるが、このテストでは正常系のみ

- 認証・認可
  - 該当なし（ローカルファイルI/Oのみ）

- 秘密情報
  - Hard-coded secrets: 該当なし
  - Log leakage: ログ出力なし。expect のメッセージは一般情報のみ

- 並行性
  - Race condition: 各テストで異なるディレクトリ名を使用しており衝突は低い。ただし固定名のため、並列CIで同ユーザー/同環境におけるディレクトリ再利用や削除競合の可能性がゼロではない
  - Deadlock: 該当なし
  - Send/Sync: 外部型の Send/Sync は不明。このテストでは単一スレッド

- エラー設計
  - Result vs Option: save/load は Result。expect により失敗時に即テスト失敗
  - panic箇所: expect 使用はテストとして妥当
  - エラー変換: このチャンクでは不明

詳細エッジケース（期待動作/実装/状態）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 一時ディレクトリ作成失敗 | 権限なし/パス不可 | テスト失敗（panic） | create_dir_all().expect(...) | OK |
| 保存先ファイル権限不足 | ROファイルシステム | Errでテスト失敗 | save(...).expect(...) | OK（正常系のみ） |
| 読込時ファイル不存在 | 直前に削除済み | Errを返す | load("typescript") | 未検証 |
| 読込時ファイル破損 | 不正JSON | Errを返す | load("typescript") | 未検証 |
| SHA差異判定 | 42→43 | true | needs_rebuild(...) | OK |
| SHA同一判定 | 42→42 | false | needs_rebuild(...) | OK |
| マッピング/ルール永続性 | 追加→読込後一致 | 同値性が保たれる | 比較未実施 | 未検証 |
| 後処理の削除失敗 | ファイルロック | 無視して終了 | remove_dir_all(...).ok() | 仕様上問題だが改善余地 |

Rust特有の観点（詳細チェックリスト）
- 所有権: temp_dir は PathBuf（所有）、&temp_dir を new に渡す→所有権移動なし。index は mutable で操作後 &index を save に渡す（所有権維持）
- 借用: tsconfig_path（&Path）は関数スコープ内でのみ使用され安全
- ライフタイム: 明示ライフタイム不要。参照は同スコープで完結
- unsafe境界: unsafe ブロックはこのチャンクには現れない
- Send/Sync: テストは単一スレッド、外部型の Send/Sync 要件は不明
- データ競合: 共有可変状態なし
- await境界/非同期: 非同期処理はこのチャンクには現れない
- キャンセル: 非同期なしのため該当なし
- エラー設計: expect による早期failはテストで妥当だが、失敗ケースの網羅は不足

## Design & Architecture Suggestions

- データ契約の検証強化
  - 保存前の ResolutionIndex と、読込後の ResolutionIndex の同値性（SHA、マッピング、ルール）を比較検証するテストを追加
  - 例: set_rules/add_mapping で設定した内容が load 後も同一であることを確認できる取得APIがあるなら使用（このチャンクには現れないため不明）

- 保存先パスの依存を低減
  - テストで具体的ファイルパス "index/resolvers/typescript_resolution.json" に依存せず、永続化層のAPIからパスを問い合わせるメソッドがあるならそれを利用（なければ検討）

- 一意ディレクトリの使用
  - 並列実行時の衝突を避けるため、一時ディレクトリに UUID などのランダムサフィックスを付与
  - tempfile クレートの利用で自動クリーンアップを担保

- エラー系のテスト
  - ファイル不存在/破損/権限不足などの異常系テストを追加し、エラー型・メッセージの妥当性も確認

## Testing Strategy (Unit/Integration) with Examples

推奨追加テスト（このチャンクに存在するAPIのみで書ける例中心）

- 読込ファイル不存在の検証
  ```rust
  #[test]
  fn test_load_missing_returns_err() {
      let temp_dir = std::env::temp_dir().join(format!("codanna_test_missing_{}", uuid::Uuid::new_v4()));
      fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
      let persistence = ResolutionPersistence::new(&temp_dir);

      // 保存せずに読込
      let result = persistence.load("typescript");
      assert!(result.is_err(), "Load should error when file does not exist");

      fs::remove_dir_all(&temp_dir).ok();
  }
  ```

- 権限不足（可能ならOS依存対応）
  - 例: 読み取り専用ディレクトリに保存して Err を期待（環境により難易度高、CIではスキップ可能）

- 大規模データの保存/読込
  - 多数のエントリを持つ ResolutionIndex を作成して save/load の性能と正確性を検証（取得APIがあれば同値性チェック）

- ファイル破損時の読込
  - 保存後にファイル内容を意図的に壊して load の Err を検証
  ```rust
  #[test]
  fn test_load_corrupted_returns_err() {
      let temp_dir = std::env::temp_dir().join(format!("codanna_test_corrupt_{}", uuid::Uuid::new_v4()));
      fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
      let persistence = ResolutionPersistence::new(&temp_dir);

      let mut index = ResolutionIndex::new();
      let tsconfig_path = Path::new("examples/typescript/tsconfig.json");
      index.update_sha(tsconfig_path, &codanna::project_resolver::Sha256Hash::from_bytes(&[1; 32]));
      persistence.save("typescript", &index).expect("Failed to save");

      let expected_file = temp_dir.join("index/resolvers/typescript_resolution.json");
      fs::write(&expected_file, b"{not: valid json").expect("Failed to corrupt file");

      let result = persistence.load("typescript");
      assert!(result.is_err(), "Load should fail on corrupted file");

      fs::remove_dir_all(&temp_dir).ok();
  }
  ```

注意: 上記例で使用している uuid::Uuid は外部クレート。テスト環境に応じて導入してください。導入不可ならタイムスタンプや乱数等で代替。

## Refactoring Plan & Best Practices

- 共通処理の抽出
  - 一時ディレクトリ作成/削除のヘルパー関数を導入して重複排除
  - インデックス初期化（update_sha 等）の共通セットアップを関数化

- tempfile クレートの活用
  - 自動削除される TempDir により後処理の .ok() を廃止し、失敗時もクリーンアップ

- 明示的アサーションの追加
  - 読込後のインデックスに対して、設定したマッピング/ルールの検証を追加（取得APIが存在する場合のみ）

- エラーメッセージの改善
  - expect のメッセージをもう少し具体的に（保存先パス・キーなど）することで、失敗時の原因特定を容易にする

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 永続化層側で保存先パス、保存サイズ、処理時間等を debug/info で出力できると、テストや運用時のトラブルシュートが容易
  - テスト側は通常ロギング不要だが、異常系テストではエラー内容の検証に役立つ

- メトリクス
  - save/load の処理時間や成功/失敗カウンタをメトリクス化（運用向け）

- トレーシング
  - 大規模プロジェクトでは、永続化の呼び出しにスパンを付与し I/O 時間の可視化が有用（このチャンクには現れない）

## Risks & Unknowns

- Unknowns（このチャンクには現れない/不明）
  - ResolutionPersistence::save/load の具体的シグネチャと内部実装（フォーマット、エラー型、バージョン管理）
  - ResolutionIndex の内部構造（キー/値の詳細、取得API）
  - ResolutionRules の完全なフィールドセットとバリデーション仕様
  - needs_rebuild の判定仕様（未登録パス、ルール変更時の判定など）
  - 永続化ファイルの互換性（バージョンアップ時のマイグレーション）

- Risks
  - テストが正常系中心で、異常系（ファイル破損/権限不足/不存在）を十分にカバーしていない
  - ディレクトリ名が固定で、並列実行時の衝突リスクが微小ながら存在
  - 永続化フォーマットに依存したファイルパスチェック（内部仕様変更でテストが脆くなる可能性）
  - クリーンアップ失敗時の静かな無視（.ok()）により、CI環境で一時ファイルが蓄積する可能性

以上より、このテストは永続化の基本的な動作を押さえていますが、データ契約の完全性検証と異常系の網羅を拡充すると、より堅牢な品質保証につながります。