# 当前实现状态

## 已实现

| 范围 | 当前状态 |
| --- | --- |
| 全局三方分析 | 从 Git commits/trees 读取完整快照，复用 weave entity 规则和跨文件移动分析 |
| `strict-weave merge TARGET` | 单目标、单一 merge-base、干净工作区；分析后执行原生 Git merge plumbing |
| `strict-weave cherry-pick COMMIT` | 仅支持单 parent commit；复用同一三方分析和标准 Git 冲突状态 |
| `strict-weave rebase UPSTREAM` | 仅支持一个待重放 commit；冲突后建立原生 `rebase-merge` 状态 |
| `strict-weave stash apply/pop` | 支持普通 stash；无冲突时 `pop` 删除 stash，冲突时保留 stash |
| 冲突落盘 | 工作区使用 strict-weave 冲突块，index 保留 stage 1/2/3，Git 可继续/中止 |
| 缓存 | 每次操作在 `.git/strict-weave/operation-*` 保存分析报告；操作锁防止并发 strict-weave 写入 |
| 正确性边界 | 分析失败、多个 merge-base、属性转换、rename、复杂布局均拒绝执行，不猜测 |

## 后续阶段

- 多 commit、interactive rebase、`--rebase-merges` 及 merge commit rebase。
- interactive rebase、`--rebase-merges`、多提交 cherry-pick 和其它 Git 选项。
- 在每阶段完成后重构操作层，减少重复的 Git plumbing 代码。
