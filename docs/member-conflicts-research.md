# 成员级冲突块：weave-core 复用调研

> 历史调研：成员 scope 原型已撤回，当前采用[三方共同文本裁剪](conflict-rendering.md)，不修改 weave-core。以下保留当时的接口分析和候选方案，不代表当前实现。

调研基于当前固定版本 `a3f501d19601126fefcc40a3ebb764b8d07d39fc` 和 sem-core `0.25.0`。当前重构及 8 组嵌套 fixture 已保存到 commit `8a0f513`。本次仅调研，没有改变合并判定或 fixture 预期。

## 结论

weave 有成员级合并能力，但目前公开接口不足以直接实现我们的严格成员冲突。推荐把目标分两步：

1. 先在不改变判定的前提下，将能够可靠定位的冲突缩小到成员范围；无法定位则保留父级范围。
2. 为了长期复用上游匹配和分类，优先在 weave-core 增加只读的 scope 分析接口，返回成员匹配、变化分类和三方原文范围。通过上游补丁或固定版本的最小 fork 接入，不复制整套容器合并代码。

仅公开 `container_merge` 不够：它的目标是尽量自动合并，我们需要在同一 function 双方修改时保留冲突，包括不同行及完全相同的最终内容。

## 已核对的源码

以下链接均固定到本项目依赖的 revision：

- [v2/mod.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/mod.rs)：`analyze_default` → `read_version` → `filter_nested_entities`，公开分类只有顶层 entity。完整合并另保留 `base_all / ours_all / theirs_all`，供内部容器处理。
- [v2/resolve.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/resolve.rs)：`intra_entity` 先尝试 diffy，clean 时直接返回；之后才尝试容器合并。`inner_merge` 从原始 entity 集合取直接子成员。
- [container.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/container.rs)：将容器分为 header、prelude、members、tail、footer；按成员身份合并，必要时输出局部 marker。当前成员身份使用名称及同名序号。成员内部又尝试行级、decorator、statement 合并，并非对任意嵌套容器递归运行完整 entity pipeline。
- [merge.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/merge.rs)：容器类型包括 class、module、namespace、struct、impl 等；`get_child_entities` 依据 `parent_id`。`entity_merge_with_registry` 公开，但成员处理辅助函数是 `pub(crate)`。
- [host.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/host.rs)：`Host` 只有重复身份阈值和可选行级合并能力，不是严格策略回调。它不能阻止先行 diffy、相同内容等自动解决路径。
- [conflict.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/conflict.rs)：`EntityConflict` 没有成员路径或原文范围；`parse_weave_conflicts` 返回展示信息及 ours/theirs 文本，没有 base 范围和三方 byte 映射。

公开 `v2` 子模块不代表其中的实现函数公开。`RawEntity`、`build_arena`、`match_phase`、`classify`、`inner_merge`、plan/render 的关键入口仍为 `pub(crate)`，不能直接组合成外部成员分类器。

## 对现有 fixture 的实测

使用一次性 Rust 探针，直接读取 [nested/](../tests/fixtures/nested/) 三方文本，调用 registry 提取、`analyze_default` 和 `entity_merge`。没有去缩进、重写源码或修改输入。探针已移除，以下记录为此次实验结果，不冒充新增的自动化断言。

| fixture | `analyze_default` 目标 | weave 结果及 audit | 我们的现有结果 |
| --- | --- | --- | --- |
| namespace-function.cpp | module math | clean / DiffyMerged | namespace 冲突 |
| namespace-class-method.cpp | module app | clean / DiffyMerged | namespace 冲突 |
| class-different-methods.cpp | class Counter | clean / DiffyMerged | class 冲突 |
| namespace-macro.cpp | module app | conflict / InnerMerged | 整文件冲突 |
| class-method.py | class Counter | clean / DiffyMerged | class 冲突 |
| class-different-methods.py | class Counter | clean / DiffyMerged | class 冲突 |
| nested-class-method.py | class Outer | clean / DiffyMerged | 外层 class 冲突 |
| nested-function.py | function outer | clean / DiffyMerged | 外层 function 冲突 |

所有表中目标均被分类为 `(Edited, Edited)`。parser 能提取例子中的内层 function/class，并提供 `parent_id` 和 byte 范围；例如 `app → Counter → score`、`Outer → Inner → score` 都有完整父子关系。宏例子只提取了 module app 和 function score，没有独立 SCALE entity。

宏例子的 weave 输出已经把冲突缩小到 namespace 中的 `attributes` 文本区，保留 score 在块外。但 `MergeResult.conflicts` 仍只记录 module app 的 `BothModified` 及整个 module 的三方文本，未返回这个 attributes 区域的范围。七个 clean 例子则根本没有可供我们复用的冲突块。

这说明直接采用 `MergeResult.content` 或解析 marker，都无法完整解决需求。

## 可行路线比较

| 路线 | 能复用什么 | 限制 / 结论 |
| --- | --- | --- |
| 直接使用 weave 输出 | 上游成员合并、展示 | 会漏掉本项目新增的严格冲突；不能替代独立 Git 基线 |
| 解析上游 marker 后重写 | 上游已拒绝冲突的局部展示 | 缺少可靠三方范围、base 映射；无从覆盖上游 clean 的严格冲突，不推荐作为核心协议 |
| 把方法切出来再次调用 `entity_merge` / `analyze_default` | 公开整文件入口 | 片段失去语法和作用域上下文，Python 缩进、C++ 模板/访问区都可能受影响；并且整块容器再次分析仍然只得到它自己 |
| 自定义 registry 隐藏父 entity | parser 插件接口、上游整文件合并 | 理论上可让子 entity 成为顶层，但改变了 header/gap/作用域的含义；`analyze_default` 也不能传 registry。不是可靠的小适配 |
| 本地增加保守的范围定位层 | sem-core 原始 entity/parent/span；weave 顶层事实 | 无需 fork，可先改善简单布局；只做唯一身份匹配，不重写 rename/refusal 规则，复杂情况回退 |
| 上游增加 scope 分析 API | 上游匹配、分类、区域提取与保守拒绝 | 最符合长期复用目标；需要库接口改动及范围正确性测试，推荐方向 |

## 建议的最小上游接口

以下是接口设计目标，不是当前已经存在的 API，也不建议仅把若干私有函数改成 public 就直接依赖其内部类型。

只读分析阶段接收完整 base / ours / theirs 和路径，返回树状的 scope 分析结果：

- 三方对应的父 scope、直接子成员及匹配依据。跨版本身份使用匹配结果，不能把某一版本的 `parent_id` 当作稳定跨版本 ID。
- 每个成员的三方变化分类，保留现有上游分类和 refusal，不复制一套规则。
- 三方原文 byte 范围，区分实体本体和附着注释、decorator；header、gap、footer 同样有明确归属。
- 不可靠的分区、重复名称、重叠及缺失信息，作为显式状态返回。
- 成员级上游拒绝若需精确展示，应返回结构化目标及范围；当前外层 `BothModified` 不能被我们自行解释成某一个子成员的拒绝。

实现上先复用现有提取、匹配、分类与容器分解，再为 scope 建立可靠层级，而不是复制容器合并器。当前容器分解使用按行 join，注释中只承诺至多忽略末尾换行；严格 driver 需要增加原文范围映射和逐 byte 重建验证，不能直接假设它满足我们的要求。

我们在只读事实之上应用严格策略和中文展示。分析应在自动解决之前覆盖所有成员，不能只给失败的 `container_merge` 加回调：不同修改行会在更早的 diffy 路径被自动解决，相同最终内容也可能提前返回。

## 首轮落地范围

建议先完成“展示下沉”，保持当前所有冲突判定。在尚未扩展上游 API 时，也可以用保守的本地范围定位层验证这一过程：

1. 保留原始整文件 Git 基线和全部上游拒绝，得到当前严格原因。
2. 在已可靠对应的父 entity 内，读取 parser 给出的直接子成员；仅接受三方类型、名称唯一且次序一致的情况。不在这一步实现 rename、move、重载匹配。
3. 将父区域拆成原文切片，验证无重叠、无遗漏、三方分别逐 byte 重建。未变化的父级骨架和兄弟成员可以留在块外。
4. 仅当父级严格修改可完全归因于可靠的子范围时，下沉展示；同一子 function 双方修改仍展示完整 function，包括相同最终文本。附着注释及未建模文本必须归入覆盖检查。
5. namespace/class 可继续下沉；普通 function 仍是严格规则的边界。`outer` 内的嵌套 function 暂不默认替代整个 outer 的审核范围，以免“修改同一函数”的要求被递归拆散。
6. Git 已报整文件行级冲突时，首版保留当前整文件回退。即使分区内均 clean，也不能推断原始冲突消失。以后只有获得可靠行级 hunk 和三方范围覆盖证明，才缩小这类输出。
7. 上游拒绝缺少成员级依据、父级骨架变化、成员增删/重排、同名重载、解析失败、宏/条件编译区域不能可靠分区时，保留父级或整文件冲突。

宏用例可证明上游已有局部展示能力，但它同时触发 Git 冲突且缺少结构化成员范围，不能作为首轮安全下沉的默认成功案例。嵌套 namespace/class 的简单方法修改则是较好的首轮目标。

只改展示时，不应把父级 weave 证据直接改名挂到子方法上。可以保留父原因、附加 `analyze` 的原文定位依据；若得到上游 scope 分类，再增加真正的成员级事实。成员身份需要完整 scope 路径，不能仅使用 `function score` 合并不同 class 的同名方法。

## 判定与展示必须分开推进

`class-different-methods.cpp/.py` 当前要求的是父 class 审核，但没有同一个方法被双方修改。若只缩小展示，不能为了找到子冲突而捏造“first 或 second 双方修改”；首版仍保留 class 冲突。

若后续决定把 class/namespace 仅作为容器、允许不同方法独立修改，则这两组可能变成 clean。这是严格策略的调整，不是渲染优化；必须单独明确需求，且不能取消原生 Git 或 weave 的拒绝。修改同一个 function 的规则始终保留。

## 验证要求

- 保留当前 8 组 output 基线；实施优化时检查实际 diff 后再调整预期。
- 简单同方法修改应只缩小范围，退出码和严格理由不能消失；不符合下沉条件的案例保持原有保守输出。
- 补充同名方法分属不同 class、重载/重排、方法删除对修改、父骨架与方法同时修改、同内容双方修改，以及带注释/decorator 的用例。
- 补充 CRLF、无末尾换行、同一行多个 entity、宏/条件编译分隔符等范围不可靠案例，要求回退而不是丢失文本。
- 保留现有普通 Git 冲突包含关系测试，增加跨分区边界的行级冲突，防止局部 clean 掩盖整文件 conflict。
- 若改变结构化成员身份或原因协议，应更新全局分析 schema/engine 并拒绝旧报告。

本次未修改依赖、实现策略或输出预期。下一步宜先验证只读 scope 原型及三方原文分区，再接入展示。
