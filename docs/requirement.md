# 严格 merge driver：冲突信息与需求讨论草案

本文依据 `AGENTS.md`，描述目标需求与展示方式；初步实现情况和限制见 [WIP.md](../WIP.md)。分支名、提交号和冲突编号均为示意。

查阅的 weave 上游版本：`a3f501d19601126fefcc40a3ebb764b8d07d39fc`。以下区分已有能力、需要新增的行为和待确认的选择。

## 1. 建议的核心规则

把两个判定独立运行，再取冲突的并集：

```text
必须人工处理 = Git 行级合并发生冲突 OR 严格实体规则发生冲突（包含 weave-core 明确拒绝合并的结果）
```

- Git 已经判为冲突的区域必须保留为未解决状态，即使 weave 能自动合并。
- 同一实体相对 base 被双方修改，无论双方结果是否相同、改动行是否重叠，都必须冲突。因为这时候直接合并，实际的业务逻辑可能实现有误，必须交给人类判定如何合并。
- weave-core 明确拒绝合并的结果必须保留为冲突，并说明上游拒绝原因；即使 Git 行级判定和本项目新增的实体规则均未报冲突，也不能忽略。
- 同一区域同时触发两条规则，展示一个冲突块，并列出两个原因。
- 双方修改不同实体、Git 行级合并没有冲突，且 weave-core 没有明确拒绝合并时，允许合并。
- “双方修改成完全相同的结果”也判定为冲突。因为假设一种情况：双方都在更新一个`fileCount=2`变量：A和B都增加了3个文件，merge后，实际应该改为`fileCount=8`，但双方都改为了`fileCount=5`，存在“丢失更新”现象。

这里的“双方修改”是比较本次操作的 base / ours / theirs 三份快照，不是查询双方历史中是否曾经修改过该函数。改过又还原的实体，在快照比较中视为未修改。driver 未被 Git 调用时，仍有第 8 节规定的全局检查需求；当前预分析能提前报告，但不能迫使 Git 安装 index conflicts。

## 2. 推荐的展示方式

采用“文件内简要说明 + 终端详细报告”。

**文件内：** 使用标准 diff3 结构：`<<<<<<<`、`|||||||`、`=======`、`>>>>>>>`。默认同时展示 ours、base、theirs，原因和身份写在标记行后面。`=======` 保持独立一行。实际标记长度遵循 Git 传入的配置，示例使用 7 个字符。

diff3 与 zdiff3 的区别、实际输出示例及选型建议见 [diff3-vs-zdiff3.md](diff3-vs-zdiff3.md)。实体规则触发的冲突默认保留完整三方实体及双方相同的修改；纯行级冲突可以支持 zdiff3 的紧凑展示。展示格式不能改变严格规则的冲突判定。

**终端：** 用中文列出冲突原因、实体名称、双方相对 base 的动作、版本来源，以及需要人工决定的事项。使用稳定原因码，便于检索和后续工具读取。

**表达风格：** 信息保持简洁，function、branch、ours、theirs、base 等基础术语直接保留英文。版本身份优先用操作命令、branch 和 commit 表达，避免重复翻译；只在 cherry-pick、rebase 等容易混淆的场景补充必要的角色说明。示例 A 作为格式参考。

**生成边界：** 所有提示由确定性的解析、匹配、文本 diff、Git 上下文及固定模板生成。直接展示可核对的文本变化，不生成业务意图或正确结果的判断；重命名、移动等启发式匹配必须附依据，不确定时标注“疑似”或列出候选。人工处理提示按冲突类型选择固定模板，不代表程序发现了某个具体业务问题。需求背景中的丢失更新等例子用于说明规则动机，不作为程序能自动诊断的能力。

说明不插入任意一方的源码正文，避免“接受当前更改”后留下诊断注释。对于删除，保留真正的空侧，不向源码里填入“已删除”这样的占位文字。

实体冲突优先包住整个函数或方法；行级冲突可扩展到覆盖相关实体的连续区域，但必须保留三方的真实文本与顺序。多个扩展区域重叠时合并为一个块，不产生嵌套标记，也不重复代码。实体移动导致不能安全扩大范围时，保留原行级块，在报告中关联实体。

**展示范围也需要由我们的 driver 控制。** diff3 本身不识别函数边界，也不保证展示完整实体；driver 必须根据三方实体范围选择并渲染冲突块，不能直接依赖 Git 默认的行级输出。双方结果相同时，仍须按严格规则构造冲突，不能在渲染时因内容相同而自动消除。全局分析结果触发 driver 冲突时，也复用这套逻辑。

这是目标格式；不同编辑器对自定义标签和 diff3 的支持需要在实现时验证。

## 3. 示例 A：同一函数，双方修改不同的行

沿用 `AGENTS.md` 中的 Go 示例。它用于展示合并策略，不作为可编译的 Go 测试程序。

终端报告：

```text
冲突 C001 · calc.go · function a()
原因：同一函数被双方修改 [ENTITY_BOTH_CHANGED]
操作：git merge feature/drop-x

current branch: feature/drop-z
ours          : feature/drop-z @ a1b2c3d
theirs        : feature/drop-x @ d4e5f6a
base          : 1122334

相对 base：
  ours   删除了 `z := 3`
  theirs 删除了 `x := 1`

行级判定：可自动合并
实体判定：双方修改同一函数，按严格规则暂停
需人工处理：选择一侧 / 编辑合并结果。
```

文件 `calc.go`：

```text
<<<<<<< ours: feature/drop-z @ a1b2c3d | C001 function a()：双方修改
func a() int {
    x := 1
    y := 2
    return y
}
||||||| base: 1122334
func a() int {
    x := 1
    y := 2
    z := 3
    return y
}
=======
func a() int {
    y := 2
    z := 3
    return y
}
>>>>>>> theirs: feature/drop-x @ d4e5f6a | C001
```

这里不把行级自动合并出的“只剩 y”写成已解决结果。完整函数让人直接看见每一方真正提交的版本。

## 4. 示例 B：不同实体，但必须保留行级冲突

base 只有：

```typescript
export function existing() { return 0; }
```

ours 在末尾新增 `alpha()`，theirs 在同一位置新增 `beta()`。Git 2.53.0 的 `git merge-file --diff3 -p ours base theirs` 返回 `1`，实际产生行级冲突，因此保留此例。

终端报告：

```text
冲突 C002 · helpers.ts · existing() 之后的插入位置
原因：双方在同一位置插入内容 [LINE_INSERT_INSERT]

ours   : feature/add-alpha @ a1b2c3d | 新增 function alpha()
theirs : feature/add-beta  @ d4e5f6a | 新增 function beta()
base   : 1122334 | 此处为空

行级判定：冲突，必须保留
实体判定：涉及两个不同函数，未发生同一函数的双方修改
需人工处理：选择一侧 / 编辑合并结果及顺序。
```

文件 `helpers.ts`：

```text
export function existing() { return 0; }
<<<<<<< ours: feature/add-alpha @ a1b2c3d | C002 同一插入位置：alpha() / beta()

export function alpha() { return 1; }
||||||| base: 1122334 | 此处为空
=======

export function beta() { return 2; }
>>>>>>> theirs: feature/add-beta @ d4e5f6a | C002
```

重点是解释“不同实体为什么仍然冲突”，不能将其误报为双方修改同一个函数，也不能因为实体不同就自动消掉冲突。

同理，如果某个 JSON 文件的键实体可被可靠识别，双方改动不同键但落在同一行，可以报告：

```text
原因：同一文本行上的修改重叠 [LINE_OVERLAP]
ours   修改键 server.port：8080 → 9090
theirs 修改键 server.host："localhost" → "0.0.0.0"
实体不同，但行级合并已经冲突；按配置保留，等待人工处理。
```

键路径和原值、新值通过结构化解析与三方比较提取，无需推断业务含义；前提是路径和对应关系可可靠确定。重复键或匹配有歧义时保留原始文本 diff，不强行给出唯一键路径。该摘要仍需我们实现，并非 weave-core 已直接提供。

## 5. 示例 C：cherry-pick 时，一方删除、另一方修改

假设在 `release/1.2` 上 cherry-pick 提交 `9abc012`。接收方已删除 `validateInput`，待应用提交修改它。

提示只使用实体匹配、三方文本比较和可靠的 Git 操作上下文，不解释修改的业务含义。

```text
冲突 C003 · validate.ts · function validateInput()
原因：ours 删除，theirs 修改 [ENTITY_DELETE_MODIFY]

操作：git cherry-pick 9abc012

current branch: release/1.2
ours          : release/1.2 @ 7def890
theirs        : 9abc012 | 待应用 commit
base          : 3456789 | 所选 parent

相对 base：
  ours   : deleted
  theirs : modified

theirs 的文本变化（base → theirs）：
-  return data !== null;
+  return data != null;

需人工处理：删除 / 保留 / 编辑后保留。
```

文件中的空 ours 区域表达“已删除”：

```text
<<<<<<< ours: release/1.2 @ 7def890 | C003 function validateInput() 已删除
||||||| base: 3456789 | 所选 parent
export function validateInput(data: unknown) {
  return data !== null;
}
=======
export function validateInput(data: unknown) {
  return data != null;
}
>>>>>>> theirs: 9abc012 | C003 已修改
```

这些信息可以由确定性程序生成：

- `function validateInput()` 来自解析和实体匹配；base 中存在、ours 未匹配到对应实体时，在本文件分析范围内记为 `deleted`；theirs 对应实体的文本与 base 不同时记为 `modified`。匹配不可靠时应报告不确定，不能直接断言删除；跨文件移动可由额外分析补充说明。
- 增删行直接来自实体文本的 diff，不生成“加强校验”“修复逻辑”等意图描述。上面的 `-` / `+` 是终端摘要；文件内仍保留完整三方实体。
- “删除 / 保留 / 编辑后保留”是该冲突类型的固定提示，不推荐某个业务结果。
- branch、commit、操作和所选 parent 来自可靠的 Git 上下文；无法获取时省略或标记未知。若展示 commit subject，应原样读取并标为提交信息，不能当作程序对改动的判断。

不要将 cherry-pick 的 theirs 写成“对方分支”：一个提交可能被多个分支包含，也可能没有对应分支。对 merge commit 的 cherry-pick，base 是 `-m` 选择的父提交对应基准，不能总假设是第一父提交。

其他操作使用以下身份说明：

| 操作 | ours | theirs | base |
| --- | --- | --- | --- |
| merge | 当前接收方分支／提交 | 本次合入分支／提交 | Git 实际传入的合并基准，可能为虚拟基准 |
| cherry-pick | 当前接收方 HEAD | 本次待应用提交 | 待应用提交的所选父提交对应基准 |
| rebase | 已重放到的目标历史，可能包含先前重放的提交 | 当前正在重放的原提交 | 本次三方比较基准 |
| 无可靠操作上下文 | 接收侧输入（来源未识别） | 合入侧输入（来源未识别） | 基准输入（来源未识别） |

保留 `ours` / `theirs` 英文方便与 Git、编辑器对照，角色容易混淆时补充简短说明。rebase 时尤其不能把 ours 笼统写成“你的开发分支”。

## 6. 其他冲突可以怎样解释

| 情形 | 建议中文说明 | 人工需要决定的事项 |
| --- | --- | --- |
| 同一实体双方修改，且行级也冲突 | `function a()：双方文本均改变，同时存在行级冲突` | 选择一侧 / 编辑合并结果；同一区域只显示一个冲突块 |
| 同名实体被双方新增，内容不同 | `function parse()：双方新增，文本不同；base 中无对应实体` | 选择一侧 / 编辑合并结果 |
| 一方重命名，另一方修改 | `function parse()：ours 疑似重命名为 decode()；theirs 文本改变` | 确认匹配关系，选择或编辑最终定义 |
| 一方移动，另一方修改 | `function target()：ours 文本改变；theirs 疑似从 old.ts 移至 new.ts` | 必须冲突；尽可能关联目标位置，由人决定最终位置和文本，详见 [modify-vs-move.md](modify-vs-move.md) |
| 双方重命名到不同名称 | `function parse()：重命名候选 ours → decode()，theirs → parseInput()` | 确认匹配关系，选择或编辑最终定义 |
| 类中的不同方法 | `ours 修改 User.save()；theirs 修改 User.load()` | 是否触发实体冲突取决于采用方法粒度还是整个类粒度 |
| 导入区、实体之间的空白或顶层文本 | `行级冲突：未关联到 function；相邻实体为 …` | 按三方文本处理；只有解析器识别出 import 等结构时才显示“导入区”，省略不可确定的相邻实体 |
| 解析失败／实体身份不确定 | `实体分析不可用或不确定，无法提供可靠的实体级解释` | 按明确的降级策略处理，不能声称已通过实体检查 |

重命名匹配依赖分析证据。若只能推测，应显示“可能由 parse() 重命名为 decode()”，不能把猜测写成事实。不同文件中的实体移动、跨文件引用正确性，不应承诺仅靠单文件 merge driver 就能可靠判定。

上表描述待处理事项，使用固定模板，不动态生成“两个职责”“应调整哪些调用方”等业务结论。重命名报告应附实际名称、位置及文本比较依据，不能仅输出猜测结果。

实体匹配能力还应延伸为独立的智能 diff，供人检视移动和移动后的修改，并与严格合并共用分析逻辑。需求及 Git 自带能力的边界见 [smart-diff.md](smart-diff.md)。

## 7. weave-core 能提供什么，需要补充什么

以下链接固定到本次查阅的上游提交，避免后续 API 变化导致误解。

| 能力 | 已有依据 | 本项目还需要做什么 |
| --- | --- | --- |
| 三方实体匹配及改动分类 | [`v2::analyze_default`、`Analysis::iter/label`](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/v2/mod.rs)，可观察双方编辑、删除、新增、重命名等 `Cell` | 在自动解决之前应用严格规则，不能仅检查最终的 `MergeResult.conflicts` |
| 实体冲突与三方内容 | [`EntityConflict`、`ConflictKind`](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/conflict.rs) 包含实体名称、类型、冲突类别及可选三方内容 | 增加本项目的原因码；为 weave 原本会自动解决的区域构造冲突 |
| 实体文本范围与间隙 | [`region::extract_regions`](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/region.rs) 接收 sem-core 实体，生成实体区和间隙区 | 准备解析实体，将行级块关联到三方范围，并处理范围扩展、重叠和移动 |
| 相对 base 的文本变化 | [`diagnose::diagnose`](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/diagnose.rs) 给出双方增删文本和重叠信息 | 翻译为中文模板；用实际增删内容描述变化，避免推断业务意图 |
| 冲突解释和处理提示 | [`explain`](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/explain.rs) 已有解释结构及部分处理提示 | 其入口使用上游合并判定，不能直接覆盖我们新增的严格冲突；需要复用数据模型或另建解释层 |
| 自动解决的审计记录 | [`MergeResult.audit`、`ResolutionStrategy`](https://github.com/Ataraxy-Labs/weave/blob/a3f501d19601126fefcc40a3ebb764b8d07d39fc/crates/weave-core/src/merge.rs) 记录实体解决策略 | 可辅助解释，但不能替代原始输入的严格判定和范围数据 |
| 中文、版本标签、Git 行级冲突保留 | 现有 `MarkerFormat` 提供标记长度、增强模式及注释前缀 | 中文渲染、动态身份标签、独立行级检查都需要 driver 补充 |

一个重要接口限制：当前 `Analysis` 虽然可枚举分类和名称，但 `Matching` 内的 arena、`Triple` 的三方引用以及匹配证据等仍是 `pub(crate)`。不能假定只依赖现有公开 API 就能拿到上述所有原文、作用域、范围与重命名依据。实现时需要评估为 weave-core 增补只读接口，或结合 sem-core 的公开解析结果；同名方法不能只凭名称关联。

另外，当前公开分析入口主要处理顶层实体。若选择“类中每个方法独立判断”，需要继续核对嵌套实体分析路径，不能直接把顶层分类当成方法级分类。

版本来源也不是实体解析结果。应优先使用 Git 传入的版本标签和可靠调用上下文，再用仓库状态补充。driver 执行时未必已有 `MERGE_HEAD` / `CHERRY_PICK_HEAD`，因此不能只依赖这些文件；无法确认来源时保留输入标签并标注未知。提交号不能冒充为已确认的分支名，虚拟 base 也不能冒充某个真实提交。

## 8. 整个文件内容相同时的需求约束

Git 可能在双方文件内容相同时直接采用该内容，不调用自定义 merge driver。因此，仅在 `.gitattributes` 中配置 driver，不能保证所有“双方修改同一实体”的情况都会被检查。

**覆盖 Git 跳过 driver 的路径仍是完整严格保证的要求，但当前架构不提供 Git 操作包装命令。** 已实现 `driver prepare BASE OURS THEIRS --output FILE`，显式三方分析 → 只读结果 → 原生 Git 操作 → 各文件 driver 读取。预分析发现审核项返回 `1`，可由 shell 阻止后续操作；它本身不安装 index stages。见 [global-analysis.md](global-analysis.md)。

以下是完整目标约束，其中自动暂停原生 Git、安装冲突 stages 和多步流程覆盖尚未实现，不能把现有 driver 宣称为已满足：

1. 在操作被视为成功或自动生成最终提交之前，检查本次操作中双方相对 base 均有变化的文件；不能只扫描 Git 已报告的冲突文件，也不能因为 `ours == theirs` 就跳过实体检查。
2. 使用与本次 merge、cherry-pick 或 rebase 步骤一致的三方基准。操作开始前保留必要的版本身份和输入，避免检查时 HEAD 已改变；不能随意选取一个共同祖先替代 Git 使用的基准。
3. 对 base 中已有且双方均保留的实体，只要 `ours != base` 且 `theirs != base`，就必须冲突，包括 `ours == theirs` 的情况。文件相同本身不是冲突原因；仍需确认实体的双方修改，无法可靠分析时采用明确的降级策略。
4. 命中规则时，必须暂停操作，输出中文原因，并将该文件呈现为 Git 可识别的未解决冲突，保留三方版本供人工处理。仅打印警告、仅写入冲突标记但仍将索引视为已解决，或者等最终提交生成后再提醒，都不满足要求。
5. 即使 ours 和 theirs 完全相同，也要保留两侧及 base 的展示。用户可以审核后接受现有内容，也可以修改它；工具不能根据 `fileCount` 示例自行推导应该累加，更不能自动消除这个冲突。
6. 人工解决后允许通过 Git 原生 continue/abort 继续或取消操作。继续时必须区分已审核的冲突与尚未检查的内容，避免对同一已确认结果无限重复报冲突；rebase 等多步操作仍须逐步检查。

相同结果的报告示例：

```text
冲突 C004 · count.ts · function getFileCount()
原因：同一函数被双方修改，虽然结果相同，仍须人工审核 [ENTITY_BOTH_CHANGED]

ours 相对 base 的文本变化（theirs 相同）：
-  return 2;
+  return 5;

行级判定：无冲突；Git 可能直接采用相同文件，跳过 driver
实体判定：双方均修改该函数，按严格规则暂停
需人工处理：确认保留当前结果 / 编辑合并结果。
```

验收至少覆盖：双方整个文件相同且同一函数均相对 base 改变时必须暂停；三方内容完全相同时不因本规则冲突；只有一方改变时不因本规则冲突；文件整体不同但某个函数被双方改成相同内容时也必须暂停。测试必须通过真实 Git 操作验证，不能只直接调用 driver。

产品应明确标注：当前尚未提供完整操作级保证。预分析能提前报告风险，但若忽略返回值继续执行原生 Git，或仅配置 driver，仍存在检查盲区。双方删除、双方新增同名实体的判定范围仍按第 9 节单独确认。

## 9. 待一起确认的需求

已确定：双方将同一已有函数改成相同内容也必须冲突；weave-core 明确拒绝合并的结果必须保留。这两项不再作为待定选项。

| 问题 | 建议初始选择 | 其他选择的影响 |
| --- | --- | --- |
| 双方删除同一实体、双方新增同名同内容实体，是否也必须冲突？ | 建议也纳入双方变更的审核范围，尚待确认；已有函数双方修改成相同内容必须冲突的规则已确定 | 若纳入，需要为双删保留 base 并展示两个空侧，为双方新增展示空 base；实际行级冲突及 weave-core 明确拒绝的结果始终保留 |
| 实体粒度？ | 函数／方法作为审核单位；类头、字段及其他实体按语言支持另定 | 整个类作为单位会使两个不同方法的修改也冲突，实现较直接但更频繁 |
| 格式与注释算修改吗？ | 初版计入附着于实体的格式、注释变化；比较时的换行规范化需另行写清楚 | 若忽略，需要单独定义文本标准化和注释归属规则 |
| 冲突文件展示多大范围？ | 同一实体冲突默认完整函数／方法，显示 base | 大函数会很长；可另提供紧凑展示，但不能改变冲突判定 |
| 没有解析器、解析失败或身份模糊怎么办？ | 严格模式建议：双方均有变化且无法可靠分析时暂停，清楚报告能力不足；单方变化可保留 | 宽松模式仅做行级合并，但不能保证“同一函数双方修改就冲突” |
| “传统行级合并”以什么为基准？ | 固定并记录 Git 版本与 `git merge-file` 的选项，使用原始三份输入检查 | 不同选项可能改变结果；不能把 weave 内部的行级回退等同于这个基准 |

本草案优先确定三个用户体验约定：为什么停下来、每一侧究竟是谁、需要人决定什么。稳定原因码与中文正文同时保留，首版不需要依赖大模型生成解释。
