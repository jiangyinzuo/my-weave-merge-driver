# strict-weave 操作命令

strict-weave 不实现 Git merge driver，也不通过 `.gitattributes` 注入 Git。用户需要显式使用 strict-weave 支持的命令；其它 Git 命令继续由 Git 自己执行，不会自动获得严格 entity 分析。

当前支持：

```sh
# 预分析，不修改 HEAD、refs、index 或工作区
strict-weave merge feature/payment --plan -o /tmp/payment-plan.json

# 预分析并立即 apply
strict-weave merge feature/payment
strict-weave cherry-pick <commit>
strict-weave rebase <upstream>
strict-weave stash apply [stash]
strict-weave stash pop [stash]

# rebase 冲突后的状态控制
strict-weave rebase --continue
strict-weave rebase --abort

# 消费已经校验过的预分析计划
strict-weave merge feature/payment --apply /tmp/payment-plan.json
```

`--apply` 会重新确认 repository、target、base / ours / theirs、HEAD、index 和完整报告内容；任一项变化都会拒绝执行。计划文件不能被当前操作之外的旧缓存替代。

`-o` 和 `--apply` 的相对路径以执行命令时的目录为准。从仓库子目录执行也会分析完整 tree。

命令会先检查工作区和 index，解析实际 commits，读取完整三棵 tree，完成全局分析，然后再调用 Git plumbing 建立标准操作状态。发现冲突时：

- 工作区写入 strict-weave 冲突块；
- index 保留 stage 1/2/3；
- 使用 `git add` 暂存解决结果；
- 默认执行时，每步报告写入 `.git/strict-weave/operation-*/plan.json`，不自动读取旧报告。

当前明确拒绝：多个 merge-base、文件 rename、filters/renormalize、sparse checkout、脏工作区、interactive rebase、`--rebase-merges`、merge commit、复杂 Git 选项，以及带独立 index 修改或未跟踪文件的 stash。普通线性 rebase 可以包含多个 non-merge commit；每一步都由 strict-weave 分析后才应用。

merge/cherry-pick/rebase 成功且没有冲突时，strict-weave 完成对应 commit；stash 只应用改动。发生冲突时命令返回 `1` 并保留 Git 状态；分析或状态错误返回 `129`。

原生 Git 返回 `1` 时还会检查 index 是否包含未解决冲突。没有冲突项则视为应用失败，不安装严格结果，也不删除 stash。

标准 Git 操作示例：

```sh
git add path/to/file
git merge --continue
git cherry-pick --continue
git merge --abort
git cherry-pick --abort
```

merge 和 cherry-pick 的状态命令仍由 Git 处理。rebase 的逐 commit 状态由 strict-weave 管理，因此使用 `strict-weave rebase --continue/--abort`。

## 多 commit rebase

开始重放前固定 upstream、原 branch 和待重放 commit 顺序。每一步的 base 是原 commit 的 parent，ours 是当前已经重放到的 HEAD，theirs 是原 commit；三棵完整 tree 都重新分析。人工解决并 `git add` 后，`--continue` 提交实际暂存结果，后续分析基于这个新结果，保留原 commit 的 author 和 message。

重放期间使用 detached HEAD，原 branch 到全部完成时才更新；`--abort` 恢复开始前的 branch、index 和工作区，丢弃本次重放中的已跟踪改动。`--continue` 自动继承开始时的 `--zdiff3`、`--explain-reasons`。当前不提供 `--skip`，也不能用 `git rebase --continue` 代替，否则无法保证后续全局分析。

进度保存在 `.git/strict-weave/rebase-state.json`，与分析报告分开。继续或中止前校验仓库、HEAD、原 branch 和 Git 操作状态，拒绝覆盖外部变化。分析、提交或最终 branch 更新失败时保留进度，修复问题后可重试；若应用步骤没有完整写入，`--continue` 会拒绝提交，只允许 `--abort`。中止失败也保留进度供再次中止。

`rebase --plan` 使用 `git merge-tree --write-tree` 逐步预演，无冲突时继续，在第一个 Git 或严格 entity 冲突处停止。计划包含已分析步骤及尚待分析的 `pending` commit；不会猜测人工解决后的 tree。预演会写入不可变的 Git 对象，不改变 HEAD、refs、index 或工作区。`--apply` 重新计算并核对完整计划，实际执行时仍逐步分析。
