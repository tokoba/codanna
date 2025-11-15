```markdown
# クイックスタート

5分で Codanna を動かしましょう。

## インストール

```bash
cargo install codanna --all-features
```

## セットアップ

プロジェクトで Codanna を初期化します:

```bash
codanna init
```

これによりデフォルト設定の `.codanna/settings.toml` が作成されます。

## コードをインデックス化

コードベースから検索可能なインデックスを作成します:

```bash
# 何がインデックス化されるかを確認（ドライラン、任意）
codanna index src --dry-run

# コードをインデックス化
codanna index src --progress
```

## 本当に知りたいことを尋ねる

```bash
# セマンティック検索 - 認証関連のドキュメントコメントを持つ関数を検索
codanna mcp semantic_search_docs query:"where do we resolve symbol references" limit:3
```

## Codanna の精度と速度は？

実際に試してみてください:

```bash
# `time` コマンドを使って次のように実行します
time codanna mcp semantic_search_docs query:"where do we resolve symbol references" limit:3
```

**0.16秒**で3件の結果を出力

```text
Found 3 semantically similar result(s) for 'where do we resolve symbol references':

1. resolve_symbol (Method) - Similarity: 0.592
   File: src/parsing/language_behavior.rs:252
   Doc: Resolve a symbol using language-specific resolution rules  Default implementation delegates to the resolution context.
   Signature: fn resolve_symbol(
        &self,
        name: &str,
        context: &dyn ResolutionScope,
        _document_index: &DocumentIndex,
    ) -> Option<SymbolId>

2. resolve_symbol (Method) - Similarity: 0.577
   File: src/indexing/resolver.rs:107
   Doc: Resolve a symbol reference to its actual definition  Given a symbol name used in a file, this tries to resolve it to the actual...
   Signature: pub fn resolve_symbol<F>(
        &self,
        name: &str,
        from_file: FileId,
        document_index: &DocumentIndex,
        get_behavior: F,
    ) -> Option<SymbolId>
    where
        F: Fn(LanguageId) -> Box<dyn crate::parsing::LanguageBehavior>,

3. is_resolvable_symbol (Method) - Similarity: 0.532
   File: src/parsing/language_behavior.rs:412
   Doc: Check if a symbol should be resolvable (added to resolution context)  Languages override this to filter which symbols are available for resolution....
   Signature: fn is_resolvable_symbol(&self, symbol: &Symbol) -> bool

codanna mcp semantic_search_docs query:"where do we resolve symbol references  0.16s user 0.05s system 177% cpu 0.120 total
```

## 次のステップ

- AI アシスタントとの [integrations](../integrations/) を設定する
- [CLI コマンド](../user-guide/cli-reference.md) について詳しく学ぶ
- プロジェクト用の [設定](../user-guide/configuration.md) を構成する
```