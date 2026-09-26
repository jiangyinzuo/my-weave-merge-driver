# 当前实现状态

## 已实现

| 范围 | 当前状态 |
| --- | --- |
| 全局三方分析 | 从 Git commits/trees 读取完整快照，复用 weave entity 规则和跨文件移动分析 |
| `strict-weave merge TARGET` | 单目标、单一 merge-base、干净工作区；分析后执行原生 Git merge plumbing |
| `strict-weave cherry-pick COMMIT` | 仅支持单 parent commit；复用同一三方分析和标准 Git 冲突状态 |
| `strict-weave rebase UPSTREAM` | 支持多个线性 non-merge commit；每个 commit 单独全局分析，冲突后由 strict-weave `--continue/--abort` 管理状态 |
| `strict-weave stash apply/pop` | 支持普通 stash；无冲突时 `pop` 删除 stash，冲突时保留 stash |
| 冲突落盘 | 工作区使用 strict-weave 冲突块，index 保留 stage 1/2/3，Git 可继续/中止 |
| 计划与缓存 | 每次操作在 `.git/strict-weave/operation-*` 保存绑定的 `plan.json`；操作锁防止并发 strict-weave 写入 |
| 正确性边界 | 分析失败、多个 merge-base、属性转换、rename、复杂布局均拒绝执行，不猜测 |

## 后续阶段

- interactive rebase、`--rebase-merges` 及 merge commit rebase。
- 其它尚未支持的 Git 选项和复杂对象类型。
