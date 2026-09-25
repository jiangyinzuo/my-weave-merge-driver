# strict-weave 操作命令

strict-weave 不实现 Git merge driver，也不通过 `.gitattributes` 注入 Git。用户需要显式使用 strict-weave 支持的命令；其它 Git 命令继续由 Git 自己执行，不会自动获得严格 entity 分析。

当前支持：

```sh
# 预分析，不修改 Git
strict-weave merge feature/payment --plan -o /tmp/payment-plan.json

# 预分析并立即 apply
strict-weave merge feature/payment
strict-weave cherry-pick <commit>
strict-weave rebase <upstream>
strict-weave stash apply [stash]
strict-weave stash pop [stash]

# 消费已经校验过的预分析计划
strict-weave merge feature/payment --apply /tmp/payment-plan.json
```

`--apply` 会重新确认 repository、target、base / ours / theirs、HEAD、index 和完整报告内容；任一项变化都会拒绝执行。计划文件不能被当前操作之外的旧缓存替代。

命令会先检查工作区和 index，解析实际 commits，读取完整三棵 tree，完成全局分析，然后再调用 Git plumbing 建立标准操作状态。发现冲突时：

- 工作区写入 strict-weave 冲突块；
- index 保留 stage 1/2/3；
- Git 仍负责 `add`、继续、跳过和中止；
- 分析报告写入 `.git/strict-weave/operation-*/analysis.json`。

当前明确拒绝：多个 merge-base、文件 rename、filters/renormalize、sparse checkout、脏工作区、多个待重放 commit、interactive rebase、`--rebase-merges`、复杂 Git 选项、不支持的 commit 类型，以及带独立 index 修改或未跟踪文件的 stash。拒绝发生在修改仓库之前。

成功且没有冲突时，strict-weave 完成对应 commit。发生冲突时命令返回 `1` 并保留 Git 状态；分析或状态错误返回 `129`。

标准 Git 操作示例：

```sh
git add path/to/file
git merge --continue
git cherry-pick --continue
git merge --abort
git cherry-pick --abort
git rebase --continue
git rebase --abort
```

这些状态命令仍由 Git 处理。strict-weave 不提供同名状态机包装。
