# 全局分析与 Git 操作

strict-weave 操作命令在真正修改仓库前读取明确的 base / ours / theirs 三棵 tree。报告计划与三方 tree ID、仓库路径、target、HEAD 和 index 指纹绑定，可以保存在 /tmp，也可以保存在本仓库的 operation 目录：

```text
.git/strict-weave/operation-*/plan.json
```

计划由 Git 对象读取生成，不依赖工作区文本，也不推断当前命令。`--plan` 只写计划；默认操作在校验计划后继续 apply；`--apply` 读取外部计划。分析失败、输入不支持或状态发生变化时，命令在写入 index 和工作区前退出。

报告保存完整的文件指纹、entity 原因、跨文件移动候选和 warnings。它是操作内部的审计材料；冲突是否生效由操作层在写入 index 时再次决定，不接受上一次 operation 的报告。

冲突路径使用标准 Git index stages：stage 1 是 base，stage 2 是 ours，stage 3 是 theirs。工作区内容由 strict-weave 根据相同三方文本渲染，因而 `git status`、`git add`、`git merge --continue` 仍然按原生 Git 规则工作。

当前实现不支持虚拟 merge-base、属性转换、filters、rename 路径映射、interactive/merge rebase 和复杂 stash 状态。普通线性 rebase 的每个 commit 都重新构造三方关系并分析；未能可靠恢复三方关系时拒绝执行，不能将旧缓存或猜测结果用于当前操作。
