# 操作流程

strict-weave 是显式命令，不是 Git merge driver。它只包装下面这些常用流程；直接执行 Git 命令不会经过严格分析。

```sh
strict-weave merge TARGET
strict-weave cherry-pick COMMIT
strict-weave rebase UPSTREAM
strict-weave stash apply [STASH]
strict-weave stash pop [STASH]
```

开始前，工作区和 index 必须干净。命令会解析实际 commits，读取完整的 base / ours / theirs tree，完成全局分析和状态检查，然后调用 Git。分析或状态错误不会修改 Git 状态。

## 返回值

| 返回值 | 含义 |
| --- | --- |
| `0` | 操作完成且没有待解决冲突 |
| `1` | 发现冲突，或 `--plan` 生成的计划包含冲突 |
| `129` | 参数、分析、Git 状态或应用过程错误 |

原生 Git 返回 `1` 时，strict-weave 还会检查 index 是否真的有 unmerged entries。没有时视为应用失败：不安装严格结果，也不删除 stash。

## 冲突后

冲突时工作区包含 strict-weave 冲突块，index 保留 stage 1/2/3。编辑文件并暂存后，按操作类型继续：

```sh
git add path/to/file
git merge --continue             # merge
git cherry-pick --continue       # cherry-pick
strict-weave rebase --continue   # strict-weave rebase
```

放弃 merge 或 cherry-pick 使用 `git merge --abort` 或 `git cherry-pick --abort`。放弃 strict-weave rebase 使用 `strict-weave rebase --abort`。rebase 不能使用 `git rebase --continue`，否则后续 commit 不会经过全局分析。

## 计划

计划是只读分析结果：

```sh
strict-weave merge TARGET --plan -o /tmp/strict-weave-plan.json
strict-weave merge TARGET --apply /tmp/strict-weave-plan.json
```

`--plan` 始终需要 `-o/--output`。计划文件建议放在 `/tmp` 或已忽略的目录；放在工作区内且未被忽略会产生未跟踪文件，`--apply` 会因工作区不干净而拒绝。

`--apply` 会重新确认仓库路径、HEAD、index、target、base / ours / theirs 和完整报告。任何一项变化都必须重新生成计划。计划只绑定当前操作，不会读取旧的 `.git/strict-weave/operation-*` 报告。

从仓库子目录执行时，计划相对路径以调用目录为准；分析范围仍是整个仓库。

## rebase

普通线性 rebase 支持多个 non-merge commit。每个 commit 都重新分析当前完整 tree：

1. `strict-weave rebase UPSTREAM` 在第一个冲突处暂停。
2. 解决冲突并运行 `git add`。
3. 运行 `strict-weave rebase --continue`，再分析下一个 commit。
4. 需要放弃时运行 `strict-weave rebase --abort`。

rebase 期间使用 detached HEAD；全部完成后才更新原 branch。`--continue` 会继承开始时的 `--zdiff3` 和 `--explain-reasons`。当前不支持 interactive rebase、`--rebase-merges`、merge commit rebase、`--skip` 或 `git rebase --continue`。

## 当前边界

- merge 只支持单 target 和单一 merge-base。
- cherry-pick 只支持 non-merge commit。
- stash 只支持没有独立 index 修改、没有未跟踪文件的普通 stash。
- 拒绝 rename、filters/renormalize、sparse checkout、复杂布局和未实现的 Git 选项。
- 发现分析不可靠时保守拒绝执行，不猜测业务意图。
