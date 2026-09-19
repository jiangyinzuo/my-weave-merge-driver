# 当前实现状态

安装与使用见 [README](README.md)，规则与专题文档见其[文档索引](README.md#文档索引)。本文只列实现概况和未覆盖能力。

## 已实现

| 范围 | 当前实现 | 详细说明 |
| --- | --- | --- |
| 严格合并 | 保留 Git 行级冲突及 weave 拒绝；对上游未拒绝的双方 entity 变化补充冲突，包含相同结果；不可靠时保守回退 | [分析规则](docs/analysis.md) |
| 语言与粒度 | 使用上游 registry；按顶层 entity 分析 class/namespace，不独立匹配内部方法 | [语言支持](docs/languages.md) |
| 展示 | 中文父子原因、明确证据来源；自生成块裁剪三方共同前后文；Git zdiff3 可选 | [原因模型](docs/conflict-reasons.md)、[渲染](docs/conflict-rendering.md) |
| 全局分析 | 显式三方快照、只读报告、driver 指纹校验；跨文件移动及名称归一化/格式无关的疑似重命名，保留全部候选 | [协议](docs/global-analysis.md) |
| 操作命令 | merge、逐步骤 rebase（含交互式及 rebase-merges）、pull、stash apply/pop；预分析通过才运行对应原生 Git 步骤 | [操作说明](docs/workflows.md) |
| 验证 | 单/多文件文本 fixtures、Git 冲突包含关系、原文还原、语言与原因模型，以及真实临时仓库 CLI 测试 | [测试说明](docs/testing.md) |

分析集中在 `src/analysis/`，展示在 `src/merge/`，操作适配在 `src/workflow/`。依赖原版 weave-core `a3f501d19601126fefcc40a3ebb764b8d07d39fc` 和 sem-core `0.25.0`，无本地 patch。

## 尚未覆盖

- 内部成员独立分析、同文件位置/作用域移动、名称归一化后仍有内容或结构变化的跨文件匹配、引用与语义正确性验证。
- 通用格式化分类或格式化冲突豁免；当前忽略 token 间空白仅用于增加重命名移动候选。
- 独立智能 diff、cherry-pick 包装、多 merge base/虚拟 base、octopus 和任意 Git 参数透传。
- 将预分析审核项自动安装为 Git index conflict，以及“审核后接受同一结果”的持久化流程。当前包装在步骤执行前停止，rebase continue 会重新检查。

仅配置 driver 或忽略手动 prepare 的非零退出码时，Git 跳过 driver 的路径仍可能漏掉严格 entity 审核。包装入口补充已实现规则的全局检查，不等于能识别所有移动或语义冲突；输入/报告限制见[全局协议](docs/global-analysis.md#限制)，并发与 Git 适配边界见[操作说明](docs/workflows.md#报告与边界)。
