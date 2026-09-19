# weave 重命名匹配与跨文件复用调研

调研基于本项目固定的 weave-core `a3f501d19601126fefcc40a3ebb764b8d07d39fc` 和 sem-core `0.25.0`。以下记录上游匹配依据及本项目的复用边界；名称归一化跨文件候选已在 `src/analysis/moves.rs` 实现，依赖版本不变。

## 结论

可以复用一部分，但当前公开 API 不能直接提供跨文件重命名候选及完整依据。

- **已实现、无需修改 weave 的路线：** 复用公开的 `binding::replace_at_word_boundaries`，为现有跨文件 deleted/added 分析增加“自身名称归一化后的原文相同”候选。直接比较归一化字符串，并保留全部歧义；这是复用上游的文本操作，跨文件候选规则仍属于本项目的 analyze。只能称为“疑似重命名并移动”，不能称为只改函数名或语义等价。
- **完整复用上游匹配规则：** 需要 weave 增加只读的候选与依据接口，公开一对一选择之前的候选，明确匹配度量及来源/目标身份。仅公开最终 `Renamed` 分类或内部 `match_phase` 不足以满足本项目保留歧义的要求。
- 不建议把多个文件拼成一个伪文件来调用 `analyze_default`。这会改变语言、作用域、同名身份和调用关系；上游结果也不会保留原文件映射。

当前采用第一条，并补充公开 Tree-sitter 解析下的格式无关匹配；相似度及调用佐证尚未复用。如需支持移动同时编辑，再考虑上游 API。下面是这一判断的具体依据。

## weave 实际使用的依据

当前 v2 使用自己的 matcher，不是直接调用 sem-core 的 `match_entities`。首先用 `(parent, entity_type, name, ordinal)` 进行身份匹配，再对未匹配 entity 逐侧推断重命名。

| 阶段 | 实际比较 | 门槛及限制 |
| --- | --- | --- |
| 相同归一化内容 | 将 entity 整段区域文本中自身名称按词边界替换为 `__ENTITY__`，计算 64-bit `DefaultHasher` hash | hash 相同后按 key 排序配对，并检查类型相同；该阶段没有额外 parent 相等检查，也没有再比较归一化字符串 |
| 一般文本相似度 | 整段文本用 `split_whitespace` 分割，取不同字符串组成的集合，计算 Jaccard `交集 / 并集` | 至少 `0.7`，类型及 parent 相同；用稀有 token 的倒排索引生成候选 |
| 短文本保留度 | 两侧完整文本各不超过 12 个不同空白分隔 token；按固定关键词剔除定义行后，计算 `交集 / 较小集合大小` | 至少 `0.7`；候选共享 token 在 branch 短文本中的出现频次不超过 4；类型及 parent 相同，目标名称未出现在 base |
| 调用名称变化佐证 | 已按身份匹配的其它 entity 原先调用 old，后来不再调用 old、而开始调用 new | 类型及 parent 相同，new 未出现在 base；通常还要求 Jaccard 至少 `0.3`，两侧均为短文本时降为 `0` |

这里的 token 是**空白分隔字符串，不是 Tree-sitter 的语法 token**。Jaccard 忽略顺序和重复次数，不能用于证明控制流、调用次序或语义等价。短文本阶段的“移除定义行”也是固定关键词判断，不是语法树级函数体提取；单行 function 的整行可能被去掉。

调用名称来自文本扫描：识别后跟 `(` 的标识符，排除定义位置和 `obj.method(...)` 一类属性访问。对同一调用者中消失的调用名和新出现的调用名构造组合，不是名称绑定或调用图证明。不能将这类佐证描述成已确认的引用更新。

上游解析边界先归一化 BOM、CRLF，区域提取还可能处理语言的 entity 分隔符。其 body hash 使用包含附着注释等内容的区域文本，不只是花括号内部；不是我们保存的原始字节指纹。

## 归一化不等于只修改定义名

`replace_at_word_boundaries` 在整段文本上替换，不区分定义、引用、注释和字符串字面量。例如：

```go
// base
func old() string {
    return "old"
}
```

```go
// branch
func new() string {
    return "new"
}
```

不含示例说明注释的这两段源码，均归一化为 `func __ENTITY__() string { ... return "__ENTITY__" ... }`，上游归类为 `Renamed`，但返回值显然不同。因此当前提示明确写“按词边界替换各自名称后，区域文本相同”，不能写“函数体未改变”。归一化 sentinel 也不是语义身份；原文中真实出现 `__ENTITY__` 等情况不能据此推断编辑意图。

## 确定性与歧义

上游要生成用于合并的一对一关系，而本项目的跨文件分析要保留不确定性，两者目标不同：

1. 相同 body hash 的 bucket 先分别按 key 排序，再 `zip` 配对。一个旧 function 对应两个同形新 function 时，只会选其中一个，另一个是新增。
2. 其它候选按内部 confidence 降序，平分时按身份 key 排序；贪心选取，每个 entity 只能使用一次。confidence 是排序权重，不是统计概率。
3. 两侧同 key 新增优先于不一致的 rename 推断；上游会保留这些新增，再重新推断 rename。因此逐对调用与在完整三方上下文中调用不一定等价。

最终结果可以是确定性的，但不意味着匹配唯一可信。直接把最终配对当作完整的跨文件候选列表，会漏掉其它同样有依据的候选，违背我们保留歧义的规则。

另外，私有 `Link::Signature { similarity }` 同时承载普通 Jaccard 和短文本 containment；即使将它公开，也必须扩展度量标识才能准确向人解释分数。

## 公开接口能复用到哪里

| 接口 | 当前可见性 / 作用 | 跨文件用途 |
| --- | --- | --- |
| `v2::analyze_default` | public；接受三份文本和一个路径，返回 `Analysis` | 能复用单文件分类；无多文件输入、候选列表或范围映射 |
| `Analysis::iter` / `label`、`Cell::actions` | public；读取分类及代表名称，代表名称优先采用 base | 可以看到 `Renamed` / `RenameEdited`，但读不到两端 entity 身份和匹配依据 |
| `Analysis.matching` | 字段 public，但 `Matching` 的 arena、triples、relational 等字段均为 `pub(crate)` | 不能从外部提取候选；不能解析 Debug 文本作为生产协议 |
| `RawEntity`、`build_arena`、`match_phase`、`Link`、各候选函数 | crate 私有或模块私有 | 无法直接输入跨文件 entity 集合；需要上游接口改动 |
| `binding::replace_at_word_boundaries` | public；可直接复用归一化文本操作 | 可用于保守的跨文件候选补充，无需 fork |
| `entity_merge` 的 audit / refusal | 能在部分结果中看到 `Renamed { from, to }` 或重命名拒绝信息 | 是合并决策结果，缺少完整候选、分数和三方范围，不能替代只读候选 API |
| sem-core `model::identity::match_entities` | public；另一套支持文件路径的两方匹配器 | 可以另行评估，但不等于复用了当前 weave v2 的规则，也不提供全部歧义候选 |

sem-core 的默认相似度还包含文本 token 总数比例过滤，fuzzy 阶段阈值为 `0.8`；与 weave 的 `0.7`、短文本、调用佐证机制不同。不能因为 weave 依赖 sem-core 就将两套行为混为一谈。

## 公开入口探针结果

使用临时 Rust 探针调用本项目依赖的 `analyze_default(base, branch, base, "probe.go")`，读取 public actions；仅为调研通过 Debug 输出核对私有 Link，不将其用于产品。另调用公开的归一化函数及 sem-core matcher 对照。源码和结果位于本地 `target/rename-research/`，不是新增的 fixture 自动断言。

| 输入变化 | weave actions（branch / 不变侧） | 探针显示的 Link |
| --- | --- | --- |
| `old → new`，其余文本完全相同 | `Renamed / Unchanged` | `BodyHash` |
| `old → new`，同时 `"old" → "new"` | `Renamed / Unchanged` | `BodyHash` |
| 改名并交换 `first()`、`second()` 调用顺序 | `RenameEdited / Unchanged` | `Signature ≈ 0.714286` |
| 一个 old 变成同形 alpha 和 beta | old 被配到 alpha；beta 为 `Added / Absent` | old 使用 `BodyHash` |
| 改名并把 `return 1` 改为 `return completelyDifferent()`，caller 同时改调用名 | `RenameEdited / Unchanged` | `CallSiteCorroborated ≈ 0.555556` |
| 改名并将函数体缩进从空格改为 tab | `RenameEdited / Unchanged` | `Signature = 0.75` |

这里最后一行尤其说明：rename 分类不是格式化分析。同样的 Go 纯改名案例在不同文件路径下，sem-core 返回 `Moved`；字面量变化和交换调用次序案例则返回 deleted+added，进一步确认两套 matcher 不能直接替换。

## 当前接入边界

当前保留精确 move 检查，并增加名称不同的 deleted/added 候选：要求相同语言/grammar、相同 entity 类型、可靠原文区域，复用上述公开函数后逐 byte 比较归一化结果；不相同时，复用 sem-core 的 `parse_tree` 比较语法表示，只忽略 token 间空白。每个候选保留源/目标路径、旧名/新名和具体归一化依据；同形多目标全部报告。该规则比上游最终 matcher 更保守，也不声称与其所有 rename 分类相同。

移动侧按 base → ours 和 base → theirs 分别分析。另一侧对源 entity 修改、删除或无法确认未变时，继续在相关路径增加审核原因。新增关联只能增加说明或冲突，不能取消 Git 行级冲突、weave 拒绝或既有严格原因。未匹配到候选仍不能说明不存在移动。

要完整复用相似度及调用佐证，推荐上游提供独立的只读候选 API，复用已有 candidate generation，但返回选择前的关系和明确度量；跨文件接口还须携带路径、语言、作用域和源码范围。不能仅把多个 `RawEntity` 混入现有 Arena，因为其 key 没有文件路径，会让不同文件同名 entity 相互占用身份。

`MoveEvidence` 已使用 `MoveMatch::Exact / NameNormalized` 区分依据，重命名候选保存 grammar/type、旧/新名称、两侧替换次数，报告升级为 schema v5 / engine v9。原文出现 `__ENTITY__` 时跳过归一化匹配；不会把 sentinel 当成身份。默认提示列出文本依据、位置、歧义数量和另一侧状态；详细模式补充公开函数、替换范围及限制。

25 组新增多文件 fixture 覆盖双向 rename+move、纯移动、复制、多来源/目标（含精确候选混合）、注释/字符串、docstring、Unicode 自引用、另一侧删除/无法解析，以及 grammar、格式化、额外 body 编辑、词边界、sentinel、同文件排除。独立内部成员、相似度和调用佐证仍未纳入该规则。

## 格式无关比较

新增 `src/analysis/syntax.rs` 复用 sem-core 公开的 `language_config_for_content` 和 `parse_tree`。名称归一化后，直接比较完整有序语法表示，包括节点类型、字段名、是否 named、嵌套及叶子原文；保留注释/字符串/字符/模板等子树原文，不裁剪其内部空白。只忽略 token 之间的 ASCII 空白。解析失败、missing/error 节点或未覆盖非空白字节均放弃这个候选键。

没有使用上游 `structural_hash`：该函数会跳过注释、裁剪叶子首尾空白，且仅返回 hash，不能满足这里的原文保留和直接比较要求。也没有使用空白分隔 token 集合相似度；当前要求表示完全相同，不做模糊阈值判断。

`rename-format-file` 验证 `legacy/pricing.go → billing/prices.go`，同时 `calculateTotal → calculateTotals` 并格式化，与另一侧函数修改相遇；匹配 19 个 token，两个路径均审核。Python 正例允许缩进宽度改变；反例验证缩进改变嵌套、字面量/注释/模板内部空白及额外 body 编辑不被忽略。更多格式转换（如引号风格、分号或括号变化）并不保证匹配，且无论是否匹配都不放宽单文件冲突判定。

## 源码依据（固定 revision）

- [match_phase.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/match_phase.rs)：`build_arena`、`infer_renames`、四类候选和贪心选择。
- [binding.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/binding.rs)：词边界替换和调用名扫描。
- [types.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/types.rs)：私有 Key/Link、Triple/Matching 字段及公开 actions。
- [classify.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/classify.rs)：按名称及 body hash 是否相等区分 Renamed 与 RenameEdited。
- [v2/mod.rs](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/mod.rs)：单文件公开入口、编码归一化、顶层提取与区域文本处理。
- sem-core `0.25.0` 的 `src/model/identity.rs`：独立两方 matcher，源码随 Cargo registry 依赖安装。
