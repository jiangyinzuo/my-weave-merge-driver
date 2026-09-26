# strict-weave

`strict-weave` 是一个基于 Git 对象和 index 的严格 entity + 行级合并工具。它先读取完整的 base / ours / theirs tree，执行全局 entity 分析，再让 Git 建立标准操作状态。

它是显式命令，不是 Git merge driver：直接执行 `git merge`、`git rebase` 等命令不会经过 strict-weave，也不需要配置 `.gitattributes`。

## 快速开始

在干净的工作区和 index 中执行：

```sh
strict-weave merge feature/payment
strict-weave cherry-pick <commit>
strict-weave rebase <upstream>
strict-weave stash apply [stash]
strict-weave stash pop [stash]
```

命令先完成分析和状态检查，再调用 Git。发生冲突时返回 `1`，保留标准 index stages 和工作区中的 strict-weave 冲突块；分析、状态或参数错误返回 `129`。

解决冲突后：

```sh
git add <file>
git merge --continue             # merge
git cherry-pick --continue       # cherry-pick
strict-weave rebase --continue   # strict-weave rebase
```

放弃 merge 或 cherry-pick 使用 Git：

```sh
git merge --abort
git cherry-pick --abort
```

放弃 strict-weave rebase 使用：

```sh
strict-weave rebase --abort
```

普通线性 rebase 可以包含多个 non-merge commit；每个 commit 都会重新分析当前完整 tree。当前不支持 interactive rebase、`--rebase-merges` 和 merge commit rebase。

## 只读计划

`--plan` 只分析，不修改 HEAD、refs、index 或工作区；它始终需要 `-o/--output`。建议把计划放在 `/tmp` 或已被 Git 忽略的目录：

```sh
strict-weave merge feature/payment --plan -o /tmp/payment-plan.json
strict-weave merge feature/payment --apply /tmp/payment-plan.json
```

计划文件放在工作区内且未被忽略，会使工作区变脏，随后 `--apply` 会按设计拒绝执行。`--apply` 会重新确认仓库、HEAD、index、target、三方 tree 和完整报告；任一项变化都必须重新 `--plan`。

从仓库子目录执行时，计划相对路径以调用目录为准，但分析范围仍是整个仓库。

## 支持边界

- merge 只支持单 target 和单一 merge-base。
- cherry-pick 只支持 non-merge commit。
- stash 只支持普通 stash；不支持独立 index 修改或未跟踪文件。
- 当前拒绝 rename、filters/renormalize、sparse checkout、复杂布局、多个 merge-base 和未实现的 Git 选项。
- 原生 Git 返回失败且没有 unmerged index 时，strict-weave 不会安装结果，也不会删除 stash。

## 文档索引

- [需求与示例](docs/requirement.md)
- [当前分析规则](docs/analysis.md)
- [冲突原因模型](docs/conflict-reasons.md)
- [操作边界](docs/workflows.md)
- [测试与 fixture](docs/testing.md)
- [语言支持](docs/languages.md)

## 构建与测试

```sh
cargo test -j 2 --offline --locked
cargo clippy -j 2 --offline --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

核心测试和真实 Git 集成测试共用 [tests/fixtures/](tests/fixtures/) 中的输入与预期输出。
