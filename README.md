# strict-weave

`strict-weave` 是一个基于 Git 对象和 index 的严格 entity + 行级合并工具。它不注册 Git merge driver，也不要求 `.gitattributes`；命令本身负责读取完整的 base / ours / theirs tree，执行全局 entity 分析，再让 Git 建立标准冲突状态。

当前公开命令只有受限的：

```sh
strict-weave merge feature/payment
strict-weave cherry-pick <commit>
strict-weave rebase <upstream>
strict-weave stash apply [stash]
strict-weave stash pop [stash]
```

普通线性 rebase 支持多个 non-merge commit。冲突时解决文件并暂存后，继续执行：

```sh
git add <file>
strict-weave rebase --continue
# 或者放弃本次 rebase，恢复开始前的 branch、index 和工作区
strict-weave rebase --abort
```

每个待重放 commit 都会重新读取当前完整 tree 并执行一次全局分析；后续 commit 不会绕过严格检查。

只读计划和应用已有计划的形式分别是：

```sh
strict-weave merge feature/payment --plan -o /tmp/payment-plan.json
strict-weave merge feature/payment --apply /tmp/payment-plan.json
```

开始操作时要求工作区和 index 干净；merge/rebase 要求只有一个 merge-base。当前拒绝未实现的 Git 选项、文件 rename、filters、sparse checkout 和复杂布局变化。分析完成后才应用当前步骤。冲突时保留标准 index stages，工作区写入 strict-weave 冲突块；merge/cherry-pick 使用原生 Git 继续：

```sh
git add <file>
git merge --continue
git cherry-pick --continue
```

`strict-weave` 不会把不支持的命令或选项静默转交给 Git。直接执行 `git merge`、`git rebase` 等命令不会经过 strict-weave。stash 当前只支持没有独立 index 修改、没有未跟踪文件的普通 stash。

每个操作都支持 `--plan -o FILE` 和 `--apply FILE`；省略时分析后立即执行。计划绑定当前仓库、HEAD、index、target 和三方 tree；内容或 Git 状态变化后不能继续使用旧计划。`--plan` 不修改 HEAD、refs、index 或工作区；rebase 预演会写入不可变的 Git 对象，在第一个冲突处停止，并列出尚未分析的 commit。

默认执行时，每一步的分析报告保存在 `.git/strict-weave/operation-*/plan.json`。新操作始终重新分析，不会自动读取残留报告。继续 rebase 所需的进度单独保存，并校验当前 HEAD 和 branch；具体边界见 [操作说明](docs/workflows.md)。

## 构建与测试

```sh
cargo test -j 2 --offline --locked
cargo clippy -j 2 --offline --locked --all-targets -- -D warnings
cargo fmt --all -- --check
```

测试包含 entity、原因模型、语言 registry、文本 fixture，以及真实临时 Git 仓库中的 merge、cherry-pick、rebase 和 stash 流程。核心测试与集成测试共用 [tests/fixtures/](tests/fixtures/) 中的文本。

## 文档索引

- [需求与示例](docs/requirement.md)
- [当前分析规则](docs/analysis.md)
- [冲突原因模型](docs/conflict-reasons.md)
- [操作边界](docs/workflows.md)
- [测试说明](docs/testing.md)
- [语言支持](docs/languages.md)
