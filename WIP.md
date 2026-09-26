# 当前实现状态

## 已实现

| 范围 | 当前状态 |
| --- | --- |
| 全局三方分析 | 从 Git commits/trees 读取完整快照，复用 weave entity 规则和跨文件移动分析 |
| `strict-weave merge TARGET` | 单目标、单一 merge-base、干净工作区；分析后执行原生 Git merge plumbing |
| `strict-weave cherry-pick COMMIT` | 仅支持单 parent commit；复用同一三方分析和标准 Git 冲突状态 |
| `strict-weave rebase UPSTREAM` | 支持多个线性 non-merge commit；每个 commit 单独全局分析，冲突后由 strict-weave `--continue/--abort` 管理状态 |
| `strict-weave stash apply/pop` | 支持普通 stash；无冲突时 `pop` 删除 stash，冲突时保留 stash |
| 冲突落盘 | 工作区使用 strict-weave 冲突块，index 保留 stage 1/2/3；merge/cherry-pick 由 Git 继续/中止，rebase 由 strict-weave 管理 |
| 计划 | `--plan` 不修改 HEAD、refs、index 或工作区；rebase 预演到首个冲突为止，后续 commit 标记为 pending；`--apply` 重新计算并校验报告 |
| 报告隔离 | 每步在 `.git/strict-weave/operation-*` 保存新的 `plan.json`，不扫描旧报告；操作锁防止并发 strict-weave 写入 |
| rebase 恢复 | 保存逐步进度、显示选项及提交 OID；提交或更新 branch 失败可重试，未完整应用的步骤只能 `--abort`；异常 HEAD/branch 和其它 Git 操作会被拒绝 |
| 正确性边界 | 分析失败、多个 merge-base、属性转换、rename、复杂布局均拒绝执行，不猜测 |

## 代码与验证

`src/operation/` 按职责拆分：`git.rs` 封装 Git plumbing 和操作检查，`plan.rs` 负责三方分析、计划校验与冲突安装；`merge.rs`、`stash.rs` 执行对应操作。`rebase/` 独立维护逐 commit 重放、恢复状态和只读预演。

端到端用例位于 `tests/integration/`，与核心测试共用 `tests/fixtures/`。覆盖旧报告残留、过期计划拒绝、多 commit 重放、人工解决后的跨文件分析、显示选项继承，以及应用/提交/中止失败的恢复。测试维护方式见 [docs/testing.md](docs/testing.md)。

## 后续阶段

- interactive rebase、`--rebase-merges` 及 merge commit rebase。
- 其它尚未支持的 Git 选项和复杂对象类型。
