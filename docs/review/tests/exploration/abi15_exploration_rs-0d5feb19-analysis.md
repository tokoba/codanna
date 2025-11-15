# exploration\abi15_exploration.rs Review

## TL;DR

- このファイルは、**Tree-sitter ABI-15**の機能を複数言語で体系的に探索し、Stage 2の**LanguageBehavior**設計に必要な「正確なノード名・フィールド名」を収集するための**テスト専用モジュール**。
- 主要な発見は、TypeScriptの**import_clauseはFIELDではなくCHILD**であること、**new_expression**配下の**type_arguments**検出方法、PHPの**interface_declaration / class_declaration**と**method_declaration**の関係など。
- コアロジックは「パース → ルートノード取得 → 子ノード再帰探索 → kind/field/byte_rangeで可視化」。時間計算量は概ね**O(N)**（N=ノード数）、空間は再帰深さ**O(H)**。
- 重大リスクは、広範な**unwrap**によるパニック、**非ASCIIコードにおけるbyte_rangeでのUTF-8境界問題**、大量**println**の非決定的な出力順（並列テスト時）など。
- 公開APIは**該当なし**（全て#[test]の内部関数）。本レポートは内部テスト関数をAPI的にドキュメント化。
- Rust安全性観点では**unsafeなし**、借用/ライフタイムは妥当。ただし**&code[node.byte_range()]**のスライス境界に注意。
- 設計・テストの提案として、ノード名検証を**assert**化、**共通ユーティリティ**化、**構造化ログ**出力化を推奨。

## Overview & Purpose

このファイルは、Tree-sitterのABI-15に関する**言語別のノード種別・フィールド情報**を探索し、LanguageBehaviorトレイト（リファクタリングStage 2）の**言語依存ロジック**（シンボル抽出・関係把握）を正しく実装するための「知識ベース」を整備する目的で作られています。

- 複数言語（TypeScript/JavaScript/Rust/Python/PHP/Go/C#）の**Languageメタデータ**（abi_version, node_kind_count, field_count）を取得し、重要なノード名を**id_for_node_kind**で確認。
- 実コード片をパースし、**Node.kind**, **field_name_for_child**, **child_by_field_name**, **byte_range**を用いた**構造探索**・**テキスト抽出**のパターンを整理。
- 「ノード名は**推測しない**で**テストで検証**」という方針を徹底する実例と、**言語固有の相違点**（例：TypeScriptのimport構造、Goのreceiver、PHPのmethod_declaration）を提示。

期待する成果は、LanguageBehaviorの各言語実装で**正しいノード名・フィールド名**を使い、**データフローと抽出アルゴリズム**が一貫することです。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | abi15_tests | private | 言語別ABI-15探索テスト群のコンテナ | Low |
| Function (#[test]) | explore_typescript_interface_extends_structure | private | TypeScriptのinterface/class継承・実装構造の探索 | Med |
| Function (local) | analyze_node | private | ノードの再帰探索とextends/implementsに特化した子探索 | Med |
| Function (#[test]) | explore_typescript_generic_constructor_nodes | private | new_expression配下のtype_arguments探索 | Med |
| Function (local) | print_node_tree | private | ノードツリー全体表示（kind/kind_id/field） | Med |
| Function (local) | find_new_expressions | private | new_expressionノードの検出と詳細表示 | Med |
| Function (#[test]) | explore_rust_abi15_features | private | Rust言語メタデータと主要ノード種別の探索 | Low |
| Function (#[test]) | explore_python_abi15_features | private | Pythonの基本ノード種別の探索 | Low |
| Function (#[test]) | explore_python_abi15_comprehensive | private | Pythonの包括的ノード種別マッピング | Low |
| Function (#[test]) | explore_typescript_abi15_comprehensive | private | TypeScriptの包括的ノード種別マッピング | Low |
| Function (#[test]) | explore_php_defines_comprehensive | private | PHPのinterface/class内のmethod宣言（defines関係）の探索 | Med |
| Function (local) | print_php_tree | private | PHPノードツリー表示 | Low |
| Function (local) | find_php_defines | private | interface/classノード下のmethod_declaration検出 | Med |
| Function (#[test]) | explore_php_abi15_features | private | PHPの主要ノード種別探索 | Low |
| Function (#[test]) | explore_typescript_abi15_features | private | TypeScriptの主要ノード種別とTS/JS差分探索 | Low |
| Function (#[test]) | explore_go_abi15_comprehensive | private | Goの包括的ノード種別マッピング | Low |
| Function (#[test]) | explore_go_node_structure | private | Goの各コード片に対するノード構造表示 | Med |
| Function (local) | print_go_node_tree | private | Goノードツリー表示（field名併記） | Med |
| Function (#[test]) | explore_csharp_abi15_comprehensive | private | C#の包括的ノード種別マッピング | Low |
| Function (#[test]) | explore_language_behavior_candidates | private | 言語間の共通概念ノードマッピング（関数/クラス/メソッド） | Low |
| Function (#[test]) | explore_typescript_import_structure | private | TypeScriptのimport_statement構造（CHILD vs FIELD検証） | Med |
| Function (local) | analyze_import_node | private | import_statement子ノードとimport_clause内部の詳細探索 | Med |

### Dependencies & Interactions

- 内部依存
  - explore_typescript_interface_extends_structure → analyze_node
  - explore_typescript_generic_constructor_nodes → print_node_tree / find_new_expressions
  - explore_php_defines_comprehensive → print_php_tree / find_php_defines
  - explore_go_node_structure → print_go_node_tree
  - explore_typescript_import_structure → analyze_import_node
- 外部依存（主要クレート）
  - tree_sitter: Parser, Language, Node, Cursor
  - tree_sitter_typescript, tree_sitter_javascript
  - tree_sitter_rust
  - tree_sitter_python
  - tree_sitter_php
  - tree_sitter_go
  - tree_sitter_c_sharp
- 被依存推定
  - 本モジュールはテスト専用で「設計知見を提供」。他モジュールから**直接呼び出しはされない**が、Stage 2のLanguageBehavior実装・テストに**間接的に依存**される。

## API Surface (Public/Exported) and Data Contracts

公開API（pub/外部から利用可能）は「該当なし」。以下はテスト内の内部APIとして整理。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| explore_typescript_interface_extends_structure | fn() | TSのinterface/class継承構造の探索 | O(N) | O(H) |
| analyze_node | fn(node: Node, code: &str, depth: usize) | ノード再帰探索＋extends/implements/heritageの深掘り | O(N) | O(H) |
| explore_typescript_generic_constructor_nodes | fn() | new_expression＋type_argumentsの探索 | O(N) | O(H) |
| print_node_tree | fn(node: Node, code: &str, indent: usize) | ツリー表示（kind/kind_id/field） | O(N) | O(H) |
| find_new_expressions | fn(node: Node, code: &str, depth: usize) | new_expression検出と子表示 | O(N) | O(H) |
| explore_rust_abi15_features | fn() | Rust言語メタデータ・主要ノードのID確認 | O(K) | O(1) |
| explore_python_abi15_features | fn() | Pythonの基本ノードのID確認 | O(K) | O(1) |
| explore_python_abi15_comprehensive | fn() | Pythonの包括的ノードマッピング（カテゴリ別） | O(K) | O(1) |
| explore_typescript_abi15_comprehensive | fn() | TSの包括的ノードマッピング（カテゴリ別） | O(K) | O(1) |
| explore_php_defines_comprehensive | fn() | PHPのdefines関係（interface/class → method）探索 | O(N) | O(H) |
| print_php_tree | fn(node: Node, code: &str, depth: usize) | PHPツリー表示 | O(N) | O(H) |
| find_php_defines | fn(node: Node, code: &str, depth: usize) | interface/class下のmethod検出 | O(N) | O(H) |
| explore_php_abi15_features | fn() | PHPの主要ノードのID確認 | O(K) | O(1) |
| explore_typescript_abi15_features | fn() | TSの主要ノードのID確認＋JSとのノード数差分 | O(K) | O(1) |
| explore_go_abi15_comprehensive | fn() | Goの包括的ノードマッピング | O(K) | O(1) |
| explore_go_node_structure | fn() | Goのコード例ごとのノード構造表示 | O(N) | O(H) |
| print_go_node_tree | fn(node: Node, code: &str, indent: usize) | Goツリー表示 | O(N) | O(H) |
| explore_csharp_abi15_comprehensive | fn() | C#の包括的ノードマッピング | O(K) | O(1) |
| explore_language_behavior_candidates | fn() | 言語間共通概念のノード名比較 | O(1) | O(1) |
| explore_typescript_import_structure | fn() | TS import_statementのCHILD/FIELD検証 | O(N) | O(H) |
| analyze_import_node | fn(node: Node, code: &str) | import_statementの子とimport_clause内部を詳細表示 | O(M) | O(D) |

詳細は主要APIに絞って説明します（他はメタデータ出力中心のため略）。

### 1) explore_typescript_import_structure

1. 目的と責務
   - TypeScriptの**import_statement**構造を具体的に検証し、**import_clauseがFIELDではなくCHILD**であることを明示化。これにより、実装で**child_by_field_name("import_clause")がNone**になる問題を回避する方針を確立。
2. アルゴリズム
   - Parser生成 → TS言語設定 → 複数のインポート構文例をパース
   - ルート直下の**import_statement**を列挙
   - 各**import_statement**の子ノードを**field_name_for_child**と併せて表示
   - 子に**import_clause**があればその内部の**identifier / named_imports / namespace_import**などを列挙
3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | なし | なし | テスト関数。内部でコードサンプルを持つ |
4. 戻り値
   | 型 | 意味 |
   |----|------|
   | () | 標準出力に構造を表示 |
5. 使用例
   ```rust
   #[test]
   fn explore_typescript_import_structure() {
       // cargo test --nocapture で出力を確認
   }
   ```
6. エッジケース
   - import_clauseの存在しない**副作用インポート**（例: import './styles.css';）
   - **namespace_import**の深いネスト（'ns'識別子が内部にある）
   - **type-only import**（先頭に'type'キーワードの子が現れる）

### 2) analyze_import_node（上記テストのローカル補助）

1. 目的と責務
   - 単一の**import_statement**ノードの子一覧と**import_clause**内部を詳細表示。
2. アルゴリズム
   - 子ノードを列挙しkind/field/textを出力
   - 子が**import_clause**ならその孫をさらに列挙
   - **namespace_import**があればその内部も再列挙
3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | node | tree_sitter::Node | import_statementのノード |
   | code | &str | 元ソース |
4. 戻り値
   | 型 | 意味 |
   |----|------|
   | () | 標準出力に構造を表示 |
5. 使用例
   ```rust
   fn analyze_import_node(node: Node, code: &str) {
       // import_statementの子構造を詳細表示
   }
   ```
6. エッジケース
   - **import_clauseが存在しない**ケースへの対応（子として出現しない場合は何もしない）
   - **namespace_import**内部の識別子の場所の相違

### 3) explore_typescript_generic_constructor_nodes

1. 目的と責務
   - TSの**new_expression**配下にある**type_arguments**（例: new Map<string, Session>()）の検出と可視化。
2. アルゴリズム
   - 簡易TSコードのサンプルをパース
   - 全ツリー表示（kind/kind_id/field）
   - 再帰で**new_expression**を検出し、その子の**type_arguments**を列挙
3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | なし | なし | テスト関数 |
4. 戻り値
   | 型 | 意味 |
   |----|------|
   | () | 標準出力に構造を表示 |
5. 使用例
   ```rust
   #[test]
   fn explore_typescript_generic_constructor_nodes() {
       // new Map<string, Session>() のtype_argumentsを表示
   }
   ```
6. エッジケース
   - **型指定なし**のnew（new Map()）
   - **ネストしたジェネリック**（Array<Map<string, User>>）
   - 関数呼び出しのジェネリック（useState<Session>(null)）との区別

### 4) explore_typescript_interface_extends_structure

1. 目的と責務
   - TSの**interface_declaration**と**class_declaration**の継承・実装構造を**extends/implements/class_heritage**を中心に検証。
2. アルゴリズム
   - TSコードをパース後、**analyze_node**で全ノードを再帰巡回
   - **extends_clause / extends_type_clause / implements_clause / class_heritage**に該当する子を深掘り
3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | なし | なし | テスト関数 |
4. 戻り値
   | 型 | 意味 |
   |----|------|
   | () | 標準出力に探索結果を表示 |
5. 使用例
   ```rust
   #[test]
   fn explore_typescript_interface_extends_structure() { /* ... */ }
   ```
6. エッジケース
   - **複数継承**や複数implementsの存在
   - **extends_clause vs extends_type_clause**の正確な種別違い

### 5) explore_php_defines_comprehensive

1. 目的と責務
   - PHPにおける**interface_declaration / class_declaration**配下の**method_declaration**を検出し、**"DEFINES: Interface/Class → Method"**関係を可視化。
2. アルゴリズム
   - PHPコードをパース
   - ツリーを表示（print_php_tree）
   - 再帰でinterface/classを見つけ、**name**フィールドと**method_declaration**の**name**フィールドを抽出
3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | なし | なし | テスト関数 |
4. 戻り値
   | 型 | 意味 |
   |----|------|
   | () | 標準出力に構造表示 |
5. 使用例
   ```rust
   #[test]
   fn explore_php_defines_comprehensive() {
       // DEFINESの関係を表示
   }
   ```
6. エッジケース
   - メソッドなしのinterface/class
   - **trait**や**abstract**等、他構造への拡張が必要な場合（このチャンクには現れない）

### 6) explore_go_node_structure

1. 目的と責務
   - Goの代表的構文（package/import/struct/interface/method/ジェネリック/チャネル/ゴルーチン/defer）ごとの**ノード構造とフィールド**を表示。
2. アルゴリズム
   - 複数コード片を順にパースし、**print_go_node_tree**でkind/kind_id/field名を再帰表示
3. 引数
   | 名前 | 型 | 意味 |
   |------|----|------|
   | なし | なし | テスト関数 |
4. 戻り値
   | 型 | 意味 |
   |----|------|
   | () | 標準出力に構造表示 |
5. 使用例
   ```rust
   #[test]
   fn explore_go_node_structure() {
       // 各コード片のノード構造を確認
   }
   ```
6. エッジケース
   - **importグループ**や**エイリアス/ドット/ブランク**インポートの多様性
   - **receiver**構文のフィールド位置

## Walkthrough & Data Flow

- 共通フロー
  1. **Language**取得（例: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()）
  2. **Parser**生成 → **set_language**（unwrapで失敗時はパニック）
  3. **parse(code, None)** → **tree.root_node()**
  4. **Node.walk()**でカーソル取得 → **children**の列挙、**field_name_for_child(i)** でフィールド名確認
  5. **child_by_field_name("name"/"source")**などのフィールド取得
  6. **node.byte_range()**で元コードのスライスを抽出し可視化

- TypeScript Importの具体例
  - import_statement直下に**import_clause**が「子」として存在（FIELDではない）
  - **source**はFIELD（child_by_field_name("source")が有効）
  - import_clause内部に**identifier（default import）/ named_imports / namespace_import**などが並ぶ

- Type Argumentsの探索
  - **new_expression**の子に**type_arguments**が現れる場合があり、さらにその中の**型引数**ノード群を列挙

- PHPのdefines関係
  - **interface_declaration / class_declaration**ノードの**name**フィールドと、配下の**method_declaration**ノードの**name**フィールドを抽出し、定義関係として表示

データは、入力**code: &str** → **Tree** → **Node列挙** → **kind/field/byte_range**という流れで処理され、可視化に至ります。

## Complexity & Performance

- パース: Tree-sitterのパースは入力長Lに対して概ね**O(L)**。
- ノード再帰探索: ノード数Nに対して**O(N)**、再帰深さHに応じてスタック使用**O(H)**。
- メタデータ探索（id_for_node_kindのリスト走査）: 名前数K（固定配列）に対して**O(K)**。
- 主なボトルネック
  - 大量の**println!**によるI/O出力コスト
  - 非常に深いツリーでの再帰によるコールオーバーヘッド
- スケール限界
  - このファイルは短いサンプルコードを対象としているため、実運用レベルの巨大コードでは**出力量**と**再帰**がボトルネックになり得る。
- 実運用負荷要因
  - ネットワーク/DB/I/Oなし。CPU＋標準出力のみ。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - **unsafe**は存在せず、Rustの所有権/借用は妥当。
  - 注意点: **&code[node.byte_range()]**はUTF-8境界を満たさないとパニック可能。Tree-sitterのbyte_rangeは多くの場合トークン境界＝UTF-8境界だが、**非ASCII**を含む入力では念のため要留意。
- インジェクション
  - SQL/Command/Path等のインジェクション対象はなし（標準出力のみ）。
- 認証・認可
  - 対象外。
- 秘密情報
  - ハードコード秘密情報なし。ログ漏えいの懸念も低い（コードサンプルのみ）。
- 並行性
  - テストは独立だが、**cargo testの並列実行**で**printlnの順序が非決定的**になり得る。出力解析を行う場合は**--test-threads=1**等で制御推奨。

詳細なエッジケース評価:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 非ASCIIを含むソースでbyte_rangeスライス | "こんにちは" など | 正常にスライスする | &code[node.byte_range()] | 要注意（UTF-8境界） |
| parseの失敗時 | 不正コード/言語未設定 | 失敗をハンドリング | unwrap()/if let Some(tree) | 一部パニック可能 |
| import_clauseをFIELDと誤認 | import構文全般 | child列挙で取得 | child_by_field_nameだとNone | 修正方針明示済 |
| 極端に深いAST | 大量ネスト | 再帰で処理 | 再帰探索 | スタック増大の可能性 |
| 並列テストのログ競合 | 多数テスト | 出力干渉なし | println! 多用 | 出力順非決定的 |

Rust特有の観点（このチャンクに現れるもののみ）
- 所有権: 文字列**code: &str**と**Node**はテスト関数内スコープで借用関係が閉じており安全（行番号不明）。
- 借用: **&mut cursor**は子列挙時の局所的可変借用で期間は短く安全。
- ライフタイム: **Node**は**Tree**に依存、関数内で完結。
- unsafe境界: **該当なし**。
- 並行性/非同期: **Send/Sync**要件なし、非同期なし、awaitなし、キャンセルなし。
- エラー設計: **unwrap**多用。探索用途のテストとはいえ、**Result/Option**を活用した明示的失敗表示が望ましい。

## Design & Architecture Suggestions

- **ユーティリティ化**: 共通の再帰表示関数（print_*_tree）や探索パターン（find_*）をモジュール外に切り出し、言語別に**Strategy**として差し替え可能に。
- **構造化出力**: println!ではなく**構造化ログ（JSON）**にして、上位テストで自動検証可能に。
- **アサーション導入**: 重要ノード（例: extends_clause, type_arguments, method_declaration）が**必ず検出される**ことをassertで保証。「推測しない」をコードレベルで実現。
- **マッピングテーブル**: 言語別カテゴリ（関数、クラス、型、インポート等）を**静的配列/ハッシュ**にまとめ、**LanguageBehavior**構築時に**id_for_node_kind**でバリデーション。
- **テストデータ整備**: 代表的パターン（複数継承、ジェネリックネスト、importのvariant）を**ゴールデンテスト**化。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト（メタデータ検証）
  ```rust
  #[test]
  fn ts_has_import_nodes() {
      let lang = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
      assert_ne!(lang.id_for_node_kind("import_statement", true), 0);
      assert_ne!(lang.id_for_node_kind("import_clause", true), 0);
      // CHILDかFIELDかの違いはパース後に検証する
  }
  ```
- 構造検証（import_clauseのCHILD確認）
  ```rust
  #[test]
  fn ts_import_clause_is_child() {
      use tree_sitter::{Parser};
      let mut p = Parser::new();
      p.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
      let code = "import { A } from 'm';";
      let tree = p.parse(code, None).unwrap();
      let root = tree.root_node();
      let mut cursor = root.walk();
      let import = root.children(&mut cursor)
                       .find(|c| c.kind() == "import_statement")
                       .expect("import_statement not found");
      // フィールドでは取得不可
      assert!(import.child_by_field_name("import_clause").is_none());
      // 子ノードから取得
      let mut ic = import.walk();
      assert!(import.children(&mut ic).any(|c| c.kind() == "import_clause"));
  }
  ```
- ジェネリックコンストラクタ（type_arguments確認）
  ```rust
  #[test]
  fn ts_new_expression_has_type_arguments() {
      use tree_sitter::{Parser};
      let mut p = Parser::new();
      p.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
      let code = "const m = new Map<string, number>();";
      let tree = p.parse(code, None).unwrap();
      let root = tree.root_node();
      fn find(node: tree_sitter::Node, code: &str) -> bool {
          if node.kind() == "new_expression" {
              let mut c = node.walk();
              return node.children(&mut c).any(|ch| ch.kind() == "type_arguments");
          }
          let mut c = node.walk();
          for ch in node.children(&mut c) {
              if find(ch, code) { return true; }
          }
          false
      }
      assert!(find(root, code));
  }
  ```
- PHP defines関係（method検出）
  ```rust
  #[test]
  fn php_class_has_methods() {
      use tree_sitter::{Parser};
      let mut p = Parser::new();
      p.set_language(&tree_sitter_php::LANGUAGE_PHP.into()).unwrap();
      let code = "<?php class A { public function f() {} }";
      let tree = p.parse(code, None).unwrap();
      let root = tree.root_node();
      let mut cursor = root.walk();
      let class = root.children(&mut cursor).find(|c| c.kind() == "class_declaration").unwrap();
      let mut c2 = class.walk();
      assert!(class.children(&mut c2).any(|m| m.kind() == "method_declaration"));
  }
  ```

## Refactoring Plan & Best Practices

- **unwrapの削減**: set_language/parseは失敗を**明示的に扱い**、原因をログ出力（言語不一致、コード不正）。
- **共通関数集約**: print_node_tree/print_php_tree/print_go_node_treeを**ジェネリック化**（表示フォーマッタの抽象化）。
- **定数・カテゴリ化**: 各言語の「カテゴリ別ノード名配列」を**const**で一元化し、テスト・実装双方から参照。
- **差分テスト**: グラマー更新に備え、**node_kind_countの差分**や**重要ノードの有無**を定期検証。
- **構造化ログ**: 出力を**JSON**にし、上位テストで**パース→アサート**できるようにする。
- **深さ制限/フィルタ**: 再帰表示は**最大深さ**や**対象kindのフィルタ**でノイズを削減。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 現状**println!**中心。推奨は**構造化ログ**（例: serde_jsonでkind/field/text/位置をJSONに）。
- メトリクス
  - 観測値例: **検出ノード数**, **カテゴリごとの命中数**, **未検出ノード**。
- トレーシング
  - 必要性は低いが、言語別探索の**開始/終了**、**コード片ID**を出力し、失敗時の追跡を容易に。

## Risks & Unknowns

- グラマー更新による**ノード名の変更**や**FIELD/CHILD構造の変化**。このファイルの知見は**時点依存**。継続的な再検証が必要。
- **supertype情報**や**予約語機能**についてはコメントでTODO（このチャンクには現れない）。現状**不明**。
- **byte_rangeとUTF-8境界**の一般保証は文書化されていない部分があり、非ASCII入力時の**安全性は要確認**。
- cargo testの**並列出力**は非決定的。ログ解析を自動化する場合は**単一スレッド実行**や**構造化ログ**の必須化が望ましい。

以上の探索によって、**正確なノード名・フィールド名**と**抽出手続き**が明確化されました。特に、TypeScriptの**import_clauseはCHILD**という知見は、今後の**LanguageBehavior**実装での**バグ回避**に直結します。