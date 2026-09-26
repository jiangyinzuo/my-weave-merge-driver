# 全局分析与 Git 操作

strict-weave 操作命令在真正修改仓库前读取明确的 base / ours / theirs 三棵 tree。报告计划与三方 tree ID、仓库路径、target、HEAD 和 index 指纹绑定，可以保存在 /tmp，也可以保存在本仓库的 operation 目录：

```text
.git/strict-weave/operation-*/plan.json
```

计划由 Git 对象读取生成，不依赖工作区文本，也不推断当前命令。`--plan` 不修改 HEAD、refs、index 或工作区；rebase 预演可能写入不可变的 Git 对象。默认操作分析后立即 apply；`--apply` 重新计算并核对指定计划。分析失败、输入不支持或状态发生变化时，不应用当前步骤；先前已经完成的 rebase 步骤保留在恢复进度中。

报告保存完整的文件指纹、entity 原因、跨文件移动候选和 warnings。默认执行始终生成新报告，不扫描 operation 目录，不按时间选择“最新缓存”；因此旧报告、其它命令的报告和未写完的报告都不会被自动采用。只有显式 `--apply FILE` 才读取外部计划，并校验实际对象、操作、仓库和 index，完整报告必须与重新计算的结果一致。

冲突路径使用标准 Git index stages：stage 1 是 base，stage 2 是 ours，stage 3 是 theirs。工作区内容由 strict-weave 根据相同三方文本渲染，因而 `git status`、`git add`、`git merge --continue` 仍然按原生 Git 规则工作。

当前实现不支持虚拟 merge-base、属性转换、filters、rename 路径映射、interactive/merge rebase 和复杂 stash 状态。普通线性 rebase 的每个 commit 都重新构造三方关系并分析；未能可靠恢复三方关系时拒绝执行，不能将旧缓存或猜测结果用于当前操作。

rebase 的 base 是原 commit 的 parent，ours 是当前重放 HEAD，theirs 是原 commit。`--continue` 先提交实际暂存的解决结果，再用这个 HEAD 分析下一步的所有路径，包括原本能自动合并的路径。恢复文件 `rebase-state.json` 只保存进度、选项和 OID，不代替实体分析报告；恢复边界和预演中的 pending 步骤见 [workflows.md](workflows.md#多-commit-rebase)。
