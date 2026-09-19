# 成员粒度与展示的取舍

**当前采用顶层 entity 判定和三方共同文本裁剪，不修改 weave-core，也不实现内部方法独立分析。** 渲染规则见 [conflict-rendering.md](conflict-rendering.md)，实际结果见[嵌套用例索引](nested-entity-fixtures.md)。本文说明这一取舍的接口依据。

## weave 能力与接口边界

以下依据固定在 weave-core `a3f501d19601126fefcc40a3ebb764b8d07d39fc`：

| 接口 / 实现 | 能力与限制 |
| --- | --- |
| [v2::analyze_default](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/mod.rs) | 公开分析经过 `filter_nested_entities`，提供顶层 entity 分类 |
| [v2/resolve.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/resolve.rs) | 完整合并可使用内部成员；先尝试 diffy，clean 时不会进入容器冲突处理 |
| [container.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/container.rs) | 可拆分 header、成员等区域并局部合并；其目标是自动解决，不是同 function 双方修改即拒绝 |
| [conflict.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/conflict.rs) | `EntityConflict` 不提供完整成员路径和三方原文范围；上游 marker 也不能替代范围协议 |

sem-core 能提取部分内层 entity、parent_id 和 byte 范围，但提取到语法节点不等于已有可靠的跨版本成员身份。weave 的匹配/分类关键内部接口仍非 public；`Host` 也不是能拦截所有自动解决路径的严格策略回调。

## 为什么保留顶层判定

直接采用 weave 的合并文本，会丢掉上游已自动合并、但本项目必须审核的修改。把方法片段当成文件重新解析，会改变 Python 缩进、C++ 访问区、模板和作用域上下文；解析 marker 则缺少可靠的 base 范围和三方映射。

共同文本裁剪无需成员匹配即可缩小展示，同时保留父 entity 的审核原因。例如 class 内同一方法的两处修改可只显示 x 到 z；修改不同方法时仍由整个 class 触发审核，不能把父原因改写成某个方法被双方修改。

## 若将来重新考虑成员粒度

需要只读的 scope 分析接口，在自动解决之前提供成员匹配、歧义、上游拒绝及三方原文范围，并能逐 byte 重建 header/gap/footer 与成员。不能仅公开容器合并函数或最终配对结果。

将不同方法的修改从 conflict 改为 clean 是严格策略变更，不是展示优化。它必须单独明确需求，并继续保留 Git 行级冲突、weave 拒绝及同一 function 双方修改规则。
