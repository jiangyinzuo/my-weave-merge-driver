# 原生 Git 操作

strict-weave 的公开接口只有 Git merge driver 和只读的 `prepare`。配置完成后，直接使用 Git 的 `merge`、`rebase`、`cherry-pick`、`pull`、`stash` 以及其它操作。Git 负责 index stages、sequencer、冲突文件和所有流程状态，因此继续、跳过、退出和中止都使用对应的 Git 命令。

## 配置

```sh
git config --local merge.strict-weave.driver \
  'strict-weave driver %O %A %B %P %L --ours-label %X --base-label %S --theirs-label %Y'
```

再通过 `.gitattributes` 选择文件：

```gitattributes
*.go merge=strict-weave
*.rs merge=strict-weave
*.ts merge=strict-weave
```

Git 只会为 attributes 选中的路径调用 driver。没有被调用的路径仍由 Git 自己处理；这也是跨文件移动、相同文件内容等场景可能需要显式 `prepare` 的原因。

## 原生操作

```sh
git merge feature/payment
git rebase main
git rebase -i --rebase-merges main
git cherry-pick <commit>
git pull --rebase
git stash pop --index
```

发生冲突后，编辑文件并执行 `git add`，再按当前 Git 操作继续：

```sh
git merge --continue
git rebase --continue
git cherry-pick --continue
```

根据当前 Git 操作使用 `git merge --abort`、`git rebase --abort`、`git cherry-pick --abort` 等原生命令。strict-weave 不复制这些状态机，也不要求用户改用另一套命令。

## 可选全局预分析

Git 没有通用的 merge-driver 全局前置 hook。需要在操作前检查所有变化路径时，调用只读 `prepare`：

```sh
strict-weave prepare BASE OURS THEIRS -o /tmp/analysis.json
```

`prepare` 只读 Git 对象，不修改 index、工作区、refs 或 Git 流程状态。返回 `0` 表示没有当前规则要求审核的项，返回 `1` 表示报告中存在审核项，返回 `129` 表示分析失败。调用方自行决定是否继续执行原生 Git：

```sh
strict-weave prepare BASE HEAD feature/payment -o /tmp/analysis.json && \
STRICT_WEAVE_ANALYSIS=/tmp/analysis.json git merge feature/payment
```

报告必须使用本次操作实际的 base / ours / theirs，并且每次操作使用新的输出路径。`prepare` 不会把审核项安装为 Git index conflict；它只是帮助调用方在 Git 启动前查看风险。driver 消费报告时仍会校验 schema、路径和三侧指纹。

## 边界

- 仅配置 driver 时，Git 可能因 fast-forward、相同 blob、删除或 rename 而不调用它；这是 Git 的正常优化。
- `prepare` 不能强迫 Git 调用 driver，也不能替代 Git 的冲突状态管理。
- driver 永远保留 Git 行级冲突；weave 拒绝和 strict-weave 的额外 entity 规则只能增加冲突，不能把 Git 的 conflict 变成成功。
- 旧版 `strict-weave merge/rebase/pull/stash` 包装入口暂时作为隐藏兼容命令保留，不再出现在公开帮助或文档示例中；它们不代表推荐 API。

具体报告字段和校验规则见 [全局分析协议](global-analysis.md)，分析顺序见 [分析规则](analysis.md)。
