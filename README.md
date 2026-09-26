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
strict-weave rebase --continue
strict-weave rebase --abort
```

每个待重放 commit 都会重新读取当前完整 tree 并执行一次全局分析；后续 commit 不会绕过严格检查。

只读计划和应用已有计划的形式分别是：

```sh
strict-weave merge feature/payment --plan -o /tmp/payment-plan.json
strict-weave merge feature/payment --apply /tmp/payment-plan.json
```

第一阶段要求工作区和 index 干净、只有一个 merge-base，并拒绝未实现的 Git 选项、rename、filters、sparse checkout 和复杂布局变化。分析完成后才修改 Git 状态。冲突时保留标准 index stages，工作区写入 strict-weave 冲突块；可以继续使用原生 Git：

```sh
git add <file>
git merge --continue
git cherry-pick --continue
```

`strict-weave` 不会把不支持的命令或选项静默转交给 Git。直接执行 `git merge`、`git rebase` 等命令不会经过 strict-weave。stash 当前只支持没有独立 index 修改、没有未跟踪文件的普通 stash。

每个操作还支持两种模式：`--plan -o FILE` 只分析并写出计划，默认模式分析后立即执行，`--apply FILE` 校验并消费已有计划。计划绑定当前仓库、HEAD、index、target 和三方 tree；内容或 Git 状态变化后不能继续使用旧计划。

全局分析报告保存在当前仓库的 `.git/strict-weave/operation-*/analysis.json`，同时绑定本次三方 tree。报告只用于审计和诊断，Git 的 index、工作区和 refs 仍由操作层按标准方式维护。

## 构建与测试

```sh
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
cargo fmt --check
```

测试包含 entity、原因模型、语言 registry、文本 fixture，以及真实临时 Git 仓库中的 merge、cherry-pick、rebase 和 stash 流程。

## 文档索引

- [需求与示例](docs/requirement.md)
- [当前分析规则](docs/analysis.md)
- [冲突原因模型](docs/conflict-reasons.md)
- [操作边界](docs/workflows.md)
- [测试说明](docs/testing.md)
- [语言支持](docs/languages.md)
