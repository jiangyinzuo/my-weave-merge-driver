# 已实现的分析

本目录对应 `src/analysis/` 的实际规则；完整原因数据类型见 [conflict-reasons.md](conflict-reasons.md)。分析结果是可确定验证的文本事实、weave 分类/拒绝和保守回退，不推断业务意图。

## 从哪里读代码

- [analysis/local.rs](../src/analysis/local.rs)：单文件规则的执行顺序，收集完整原因和展示材料。
- [analysis/line.rs](../src/analysis/line.rs)：调用 Git，保留原始文本的行级冲突下限。
- [analysis/partition.rs](../src/analysis/partition.rs)：可靠原文分区和 entity 身份关联，是后续规则的前提。
- [analysis/upstream.rs](../src/analysis/upstream.rs)：复用 weave 拒绝及分类，补充双方变化严格策略。
- [analysis/raw.rs](../src/analysis/raw.rs)：布局、未覆盖原文和相同修改结果。
- [analysis/moves.rs](../src/analysis/moves.rs)：跨文件移动候选、另一侧状态及共同审核策略。
- [analysis/syntax.rs](../src/analysis/syntax.rs)：复用 sem-core 解析接口，生成保留结构和原文 token 的格式无关候选键。
- [analysis/global.rs](../src/analysis/global.rs)：三份快照、报告读写和 driver 输入校验。
- [merge/render.rs](../src/merge/render.rs)：只读分析结果，选择 Git 输出、分区块或整文件块；[merge/conflict.rs](../src/merge/conflict.rs) 负责共同文本裁剪。

上述路径均相对 `src/`。`merge::merge_with_style` 依次校验输入、执行分析、生成展示。原有 `analysis::analyze/prepare/save/load_for_driver/augment` 库接口保持不变。

## 单文件规则目录

| 分析 | 触发条件 / 依据 | 结果 | 代码入口 |
| --- | --- | --- | --- |
| Git 行级基线 | 原始 base / ours / theirs 经 `git merge-file --diff3/--zdiff3` 返回冲突 | `LINE_CONFLICT`，后续任何分析都不能取消 | `line::line_merge` |
| weave 拒绝 | `weave_core::entity_merge` 返回 entity conflict | 保留 `WeaveRefusal`，产生 `ENTITY_CONFLICT` | `upstream::weave_refusals` |
| 同一 entity 双方变化 | weave 分类的两侧动作均为 added / deleted / modified / renamed / rename+modified，且可靠目标尚未被拒绝覆盖 | `WeaveActions`，产生 `ENTITY_CONFLICT`；相同结果也不例外 | `upstream::add_strict_entity_reasons` |
| 分析能力不足 | 文件双方变化，三侧原文分区不可靠，或 weave 无法返回分类 | `PartitionUnavailable` / `WeaveUnavailable`，产生 `ENTITY_ANALYSIS_UNAVAILABLE` | `upstream::add_strict_entity_reasons` |
| entity 布局变化 | 三方可分区，但 entity / 间隙的 key 序列不同，且文件双方变化 | `ENTITY_LAYOUT_CHANGED`，不猜测分区对应关系 | `raw::analyze_regions` |
| 未覆盖 entity 原文变化 | 布局一致、已有原因未覆盖该 entity，且两侧原文均不等于 base | `RawBothChanged`，产生 `ENTITY_CONFLICT`；两侧结果相同时使用 `IdenticalEdits` | `raw::record_part_conflict` |
| 非 entity 原文变化 | 布局一致，对应间隙的两侧原文均不等于 base | `RawBothChanged`，产生 `UNMODELED_BOTH_CHANGED` | `raw::record_part_conflict` |
| 相同修改结果 | 可靠且对齐的 entity 原文满足 `ours == theirs != base` | `IdenticalEdits` 明确说明结果相同；已有原因仅补充说明，不重复添加阻断规则 | `raw::record_part_conflict` |
| 分区行级检查 | 已对齐、没有严格原因的区域由 Git 合并，返回冲突 | 保留该区域的 `LINE_CONFLICT`；不能覆盖整文件基线 | `local::analyze_region_lines` |

weave 的匹配、变化分类和五类拒绝直接复用上游；本项目的额外策略是“同一 entity 双方变化必须审核”。同一可靠目标依次采用拒绝、分类严格策略、未覆盖原文补充。来源与全部动作组合见 [父子原因目录](conflict-reasons.md)。

原文分区使用 weave 相同的语言 registry，只接受无解析错误、无歧义重叠/重复身份且拼接后逐 byte 等于输入的分区。class、namespace 等外层 entity 覆盖内部成员；身份不能可靠关联时保留独立 weave 目标，不仅凭名称合并原因。

双方变化判断比较相对 base 的最终文本/分类，不观察编辑历史。所有原文比较保留格式和附着注释。单文件严格判定不豁免格式修改。跨文件重命名候选可忽略语法 token 间的空白，但保留结构和 token 内容；这不是通用格式化或语义等价证明，不能据此取消冲突。

## 跨文件规则目录

全局预分析显式接收 base / ours / theirs，对每个变化路径复用单文件分析，再执行以下规则。

| 分析 | 触发条件 / 依据 | 结果 | 代码入口 |
| --- | --- | --- | --- |
| 移动候选 | 逐侧比较 base → ours / theirs：源 entity deleted，异路径 entity added，类型、名称、原文（含附着注释）完全相同 | `MoveEvidence`；所有歧义候选均保留，只是关联，不单独冲突 | `moves::find_candidates` |
| 重命名移动候选 | 源 deleted、异路径目标 added，名称不同、grammar/type 相同；复用 weave 词边界替换后完整 entity 原文逐 byte 相同 | `MoveMatch::NameNormalized`；标注疑似重命名并移动，保留全部候选及替换次数 | `moves::add_rename_candidates` |
| 重命名移动的格式回退 | 名称归一化原文不同，但语法结构、字段、嵌套及有序 token 原文相同，只忽略 token 间空白 | `RenameComparison::Syntax { tokens }`；原文匹配优先，同一对不重复计数 | `syntax::normalized_syntax` |
| 移动的另一侧状态 | 按 base 源路径和 entity key 查询另一侧：原文相同 / 不同 / 消失 / 无法解析 | unchanged / modified / deleted / unknown；不把 unknown 写成已确认修改 | `moves::find_candidates` |
| 另一侧移动关联 | 两侧候选具有完全相同的 base 位置 | `opposite_moves` 保留全部目标及依据；deleted 后补充疑似移动，不改变阻断规则 | `moves::find_candidates` |
| 移动与另一侧变化 | 移动候选的另一侧不是 unchanged | 源路径、目标路径均增加 `GLOBAL_MODIFY_VS_MOVE` | `moves::conflict_reason` |
| 移动分析不完整 | 某份变化文件不能可靠分区 | 非阻断 warning；继续保留单文件严格检查 | `global::analyze` |

复制且保留源 entity 不算移动。名称归一化可能替换自引用、注释和字符串，不证明只改了定义名或语义等价；原文含 `__ENTITY__` 时跳过归一化匹配。名称归一化后先比较原文，再比较格式无关语法表示。保留注释/字面量内部空白、token 顺序及结构；Python 缩进改变嵌套不算格式差异。额外 token 内容编辑、同文件作用域/位置移动尚未识别。候选超过 10000 时明确失败，不截断为可能漏报的报告。Git 跳过 driver 的路径仍需调用方审阅 prepare 报告；协议与限制见 [global-analysis.md](global-analysis.md)。

weave 重命名匹配的依据、公开接口限制及已实现的第一阶段见 [重命名匹配调研](weave-rename-research.md)。本规则复用公开文本操作和语言 registry，属于 analyze；未复用上游私有相似度、调用佐证和一对一选择。

## 校验与展示边界

输入格式/大小/已有 marker 校验、报告版本/路径/指纹校验是处理前提；失败返回错误，不能当作 clean。它们不推断代码变化类型。全局结果只增加冲突。

两种展示检查不新增或消除原因：三方逐 byte 相同的前后文可以移到块外；zdiff3 下确认 base 原文完全保留且双方仅追加不同新 entity 时，可以使用 Git 的紧凑输出。后者由 `partition::distinct_entity_appends` 提供事实，保留行级和布局原因。

`LocalAnalysis` 收集全部原因、Git 输出和区域数据后，`merge/render.rs` 只读这些结果。新增分析应在分析阶段生成事实或原因，并明确它是阻断规则还是已有原因的解释。格式无关匹配仅用于增加跨文件候选；没有自动消除格式化冲突的规则。

现有验证使用 `tests/fixtures/` 的输入输出文本及 `tests/merge.rs`、`tests/analysis.rs` 的结构化断言。重构必须保持 fixture 输出、诊断、退出码和全局报告含义；Git 冲突包含关系有独立的 64 组三方组合验证。
