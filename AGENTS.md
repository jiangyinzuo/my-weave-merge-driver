# 项目约束

基于 [weave-core](https://github.com/Ataraxy-Labs/weave) 实现严格的 entity + 行级 Git merge driver。需求及示例集中维护于 [docs/requirement.md](docs/requirement.md)，当前状态见 [WIP.md](WIP.md)。

- **禁止漏报 Git 行级冲突。** 原始三方文本的 Git 行级合并发生 conflict 时，即使 weave 能自动合并，也必须保留冲突。
- **同一 function 双方修改必须冲突。** 包括修改不同行、得到完全相同结果；判定比较明确的 base / ours / theirs 快照。示例 A 展示双方删除同一函数中不同语句的情形。
- 复用 weave 的匹配、分类与拒绝，仅补充上游未拒绝的严格规则；不能为简化展示取消任何必要冲突。
- 注重正确性和确定性。无法可靠分析时保守冲突或明确报错，不能误报成功。
- 提示尽量使用中文；function、branch、ours、theirs、base 等基础术语保留英文。使用可靠的 Git 标签和操作上下文说明双方身份，不能猜测分支来源。
- 提示由实体分析、文本 diff、Git 上下文和固定模板生成，不推断业务意图或正确结果。启发式匹配须提供依据、保留歧义并标注“疑似”；人工建议不代表语义验证。
- 优先提供 entity 层级原因及可读的冲突块。需求示例 B 说明不同 entity 仍须保留行级冲突的情况。
