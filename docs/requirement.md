# 严格 merge driver：需求与展示示例

本文定义严格策略和目标体验。示例中的 branch、commit、编号及逐行摘要用于说明需求，不保证当前 CLI 已逐项输出；实际诊断以 [fixtures](../tests/fixtures/) 为准，实现概况见 [WIP](../WIP.md)。

## 1. 核心规则

保留独立的 Git 行级基线，实体分析复用 weave，仅补充它不会拒绝的规则：

```text
必须人工处理 = Git 行级合并发生冲突 OR 严格实体规则发生冲突（包含 weave-core 明确拒绝合并的结果）
```

- Git 已经判为冲突的区域必须保留为未解决状态，即使 weave 能自动合并。
- 同一实体相对 base 被双方修改，无论双方结果是否相同、改动行是否重叠，都必须冲突。因为这时候直接合并，实际的业务逻辑可能实现有误，必须交给人类判定如何合并。
- weave-core 明确拒绝合并的结果必须保留为冲突，并说明上游拒绝原因；即使 Git 行级判定和本项目新增的实体规则均未报冲突，也不能忽略。
- 同一区域同时触发行级和实体规则，展示一个冲突块，保留两类原因。同一 entity 已有 weave 拒绝时，不再重复执行本项目的双方变化规则。
- 双方修改不同实体、Git 行级合并没有冲突，且 weave-core 没有明确拒绝合并时，允许合并。
- “双方修改成完全相同的结果”也判定为冲突。因为假设一种情况：双方都在更新一个`fileCount=2`变量：A和B都增加了3个文件，merge后，实际应该改为`fileCount=8`，但双方都改为了`fileCount=5`，存在“丢失更新”现象。

这里的“双方修改”是比较本次操作的 base / ours / theirs 三份快照，不是查询双方历史中是否曾经修改过该函数。改过又还原的实体，在快照比较中视为未修改。Git 跳过 driver 时的全局检查及剩余体验目标见第 8 节。

## 2. 展示约束

采用“文件内简要说明 + 终端详细报告”。

**文件内：** 使用标准 diff3 结构：`<<<<<<<`、`|||||||`、`=======`、`>>>>>>>`。默认同时展示 ours、base、theirs，原因和身份写在标记行后面。`=======` 保持独立一行。实际标记长度遵循 Git 传入的配置，示例使用 7 个字符。

diff3 与 zdiff3 的区别、实际输出示例及选型建议见 [diff3-vs-zdiff3.md](diff3-vs-zdiff3.md)。实体规则触发的冲突基于三方实体原文生成，默认移出三方相同的前后文，保留双方相同但不同于 base 的修改；纯行级冲突可以支持 zdiff3 的紧凑展示。展示格式不能改变严格规则的冲突判定。

**终端：** 用中文列出冲突原因、实体名称、双方相对 base 的动作、版本来源，以及需要人工决定的事项。使用稳定原因码，便于检索和后续工具读取。

**表达风格：** 信息保持简洁，function、branch、ours、theirs、base 等基础术语直接保留英文。版本身份优先用操作命令、branch 和 commit 表达，避免重复翻译；只在 cherry-pick、rebase 等容易混淆的场景补充必要的角色说明。示例 A 作为格式参考。

版本标签不重复打印 `ours:`、`base:`、`theirs:` 前缀：直接显示 Git 提供的标签。人类可读的 entity 类型 `function` 显示为 `ƒ`；难以让人直接猜到含义的其它类型统一显示为 `◇`。源码和分析数据仍保留原始 `entity_type`、名称字段。展示映射依据结构化的 entity 类型，不搜索或替换名称、源码或标签文本。

**生成边界：** 所有提示由确定性的解析、匹配、文本 diff、Git 上下文及固定模板生成。直接展示可核对的文本变化，不生成业务意图或正确结果的判断；重命名、移动等启发式匹配必须附依据，不确定时标注“疑似”或列出候选。人工处理提示按冲突类型选择固定模板，不代表程序发现了某个具体业务问题。需求背景中的丢失更新等例子用于说明规则动机，不作为程序能自动诊断的能力。

说明不插入任意一方的源码正文，避免“接受当前更改”后留下诊断注释。对于删除，保留真正的空侧，不向源码里填入“已删除”这样的占位文字。

判定范围和展示范围必须分开：保留三方真实文本与顺序，不能嵌套标记、复制或遗漏代码。当前按可靠顶层分区或整文件生成块，再裁剪三方共同前后文；不独立定位内部方法。具体选择顺序见 [conflict-rendering.md](conflict-rendering.md)。

**展示范围由 driver 控制。** diff3 不识别函数边界，也不会自动把双方相同修改变成冲突。driver 必须保留严格判定，全局分析新增冲突也复用同一渲染规则。

编辑器对自定义标签和 diff3 的兼容性需要单独验证。

## 3. 示例 A：同一函数，双方修改不同的行

base 的 function a 包含 x、y、z 三条赋值；ours 删除 z，theirs 删除 x。对应 [disjoint.go fixture](../tests/fixtures/entities/disjoint.go.base)，用于展示策略，不作为可编译的 Go 程序。

终端报告：

```text
冲突 C001 · calc.go · ƒ a()
原因：同一函数被双方修改 [ENTITY_CONFLICT]
操作：git merge feature/drop-x

current branch: feature/drop-z
feature/drop-z @ a1b2c3d
feature/drop-x @ d4e5f6a
1122334

相对 base：
  ours   删除了 `z := 3`
  theirs 删除了 `x := 1`

行级判定：可自动合并
实体判定：双方修改同一函数，按严格规则暂停
选择一侧 / 编辑合并结果。
```

文件 `calc.go`：

```text
func a() int {
<<<<<<< feature/drop-z @ a1b2c3d | C001 ƒ a()：双方修改
    x := 1
    y := 2
||||||| 1122334
    x := 1
    y := 2
    z := 3
=======
    y := 2
    z := 3
>>>>>>> feature/drop-x @ d4e5f6a | C001
    return y
}
```

这里不把行级自动合并出的“只剩 y”写成已解决结果。共同前后文留在块外，三方中间文本各自保留；沿每一侧阅读仍可还原该侧版本。

## 4. 示例 B：不同实体，但必须保留行级冲突

base 只有：

```typescript
export function existing() { return 0; }
```

ours 在末尾新增 `alpha()`，theirs 在同一位置新增 `beta()`。Git 2.53.0 的 `git merge-file --diff3 -p ours base theirs` 返回 `1`，实际产生行级冲突，因此保留此例。

终端报告：

```text
冲突 C002 · helpers.ts · existing() 之后的插入位置
原因：双方在同一位置插入内容 [LINE_CONFLICT]

feature/add-alpha @ a1b2c3d | 新增 ƒ alpha()
feature/add-beta  @ d4e5f6a | 新增 ƒ beta()
1122334 | 此处为空

行级判定：冲突，必须保留
实体判定：涉及两个不同函数，未发生同一函数的双方修改
选择一侧 / 编辑合并结果及顺序。
```

文件 `helpers.ts`：

```text
export function existing() { return 0; }
<<<<<<< feature/add-alpha @ a1b2c3d | C002 同一插入位置：alpha() / beta()

export function alpha() { return 1; }
||||||| 1122334 | 此处为空
=======

export function beta() { return 2; }
>>>>>>> feature/add-beta @ d4e5f6a | C002
```

重点是解释“不同实体为什么仍然冲突”，不能将其误报为双方修改同一个函数，也不能因为实体不同就自动消掉冲突。

## 5. 示例 C：cherry-pick 时，一方删除、另一方修改

假设在 `release/1.2` 上 cherry-pick 提交 `9abc012`。接收方已删除 `validateInput`，待应用提交修改它。

提示只使用实体匹配、三方文本比较和可靠的 Git 操作上下文，不解释修改的业务含义。

```text
冲突 C003 · validate.ts · ƒ validateInput()
原因：ours 删除，theirs 修改 [ENTITY_CONFLICT]

操作：git cherry-pick 9abc012

current branch: release/1.2
release/1.2 @ 7def890
9abc012 | 待应用 commit
3456789 | 所选 parent

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
<<<<<<< release/1.2 @ 7def890 | C003 ƒ validateInput() 已删除
||||||| 3456789 | 所选 parent
export function validateInput(data: unknown) {
  return data !== null;
}
=======
export function validateInput(data: unknown) {
  return data != null;
}
>>>>>>> 9abc012 | C003 已修改
```

这些信息可以由确定性程序生成：

- `ƒ validateInput()` 来自解析和实体匹配；base 中存在、ours 未匹配到对应实体时，在本文件分析范围内记为 `deleted`；theirs 对应实体的文本与 base 不同时记为 `modified`。匹配不可靠时应报告不确定，不能直接断言删除；跨文件移动可由额外分析补充说明。
- 增删行直接来自实体文本的 diff，不生成“加强校验”“修复逻辑”等意图描述。上面的 `-` / `+` 是终端摘要；文件内保留三方差异及块外的共同前后文。
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
| 类中的不同方法 | `class User：双方修改` | 当前按整个 class 审核，不宣称已定位到内部方法 |
| 导入区、实体之间的空白或顶层文本 | `行级冲突：未关联到 function；相邻实体为 …` | 按三方文本处理；只有解析器识别出 import 等结构时才显示“导入区”，省略不可确定的相邻实体 |
| 解析失败／实体身份不确定 | `实体分析不可用或不确定，无法提供可靠的实体级解释` | 按明确的降级策略处理，不能声称已通过实体检查 |

重命名匹配依赖分析证据。若只能推测，应显示“可能由 parse() 重命名为 decode()”，不能把猜测写成事实。不同文件中的实体移动、跨文件引用正确性，不应承诺仅靠单文件 merge driver 就能可靠判定。

上表描述待处理事项，使用固定模板，不动态生成“两个职责”“应调整哪些调用方”等业务结论。重命名报告应附实际名称、位置及文本比较依据，不能仅输出猜测结果。

独立智能 diff 暂缓实现，保留的检视需求见 [smart-diff.md](smart-diff.md)。

## 7. weave-core 能提供什么，需要补充什么

复用上游的语言 registry、三方匹配、变化分类及拒绝结果；仅补充它不会拒绝的严格规则，以及原文分区、行级下限、中文展示和跨文件候选。已实现规则与代码入口统一维护于 [analysis.md](analysis.md)，语言边界见 [languages.md](languages.md)。

公开分析以顶层 entity 为主，内部 arena、三方引用和匹配依据不完全公开，不能只凭同名串联成员身份。成员粒度的取舍见 [member-conflicts-research.md](member-conflicts-research.md)，跨文件重命名的公共 API 复用见 [weave-rename-research.md](weave-rename-research.md)。

版本来源不是实体解析结果。优先使用 Git 提供的标签；无法确认 branch/操作时保留未知，不能假设 driver 执行时已有 MERGE_HEAD/CHERRY_PICK_HEAD，或把 commit hash、虚拟 base 冒充已确认的 branch/commit。

## 8. 整个文件内容相同时的需求约束

Git 可能在双方文件内容相同时直接采用该内容，不调用自定义 merge driver。因此，仅在 `.gitattributes` 中配置 driver，不能保证所有“双方修改同一实体”的情况都会被检查。

当前 `merge/rebase/pull/stash` 包装命令对明确三方进行预分析，有审核项就停止对应步骤；rebase 逐步检查。独立 prepare 则由调用方检查退出码。两者均不自动安装 index stages，详见 [workflows.md](workflows.md) 和 [global-analysis.md](global-analysis.md)。

约束如下：

1. 在对应步骤被视为成功或生成最终 commit 前，检查全部变化路径，不能只扫描 Git 已有冲突文件，也不能跳过 `ours == theirs`。
2. 使用该步骤实际的三方基准。对 base 中已有且双方保留的 entity，`ours != base && theirs != base` 必须审核；三方相同或仅一侧变化不因本规则冲突。
3. 保留 ours、base、theirs 的展示，不能根据计数器等示例推导业务结果。
4. 测试必须包含真实 Git 流程，覆盖 Git 跳过 driver 的相同文件修改，而不只直接调用 driver。

**尚未完成的体验目标：** 将预分析命中项呈现为 Git 可识别的未解决冲突，供人编辑后正常继续；保存已审核事实，避免对已确认的同一结果无限重复报错。当前包装只在执行前停止，`rebase --continue` 会重新检查，不能声称已实现这个审核闭环。

相同结果的报告示例：

```text
冲突 C004 · count.ts · ƒ getFileCount()
原因：同一 ƒ 被双方修改，虽然结果相同，仍须人工审核 [ENTITY_CONFLICT]

ours 相对 base 的文本变化（theirs 相同）：
-  return 2;
+  return 5;

行级判定：无冲突；Git 可能直接采用相同文件，跳过 driver
实体判定：双方均修改该函数，按严格规则暂停
需人工处理：确认保留当前结果 / 编辑合并结果。
```

以上目标不等于已提供完整操作级语义保证：仅配置 driver、忽略 prepare 返回值、或超出已实现的移动规则，仍有边界。

## 9. 当前策略与未完成目标

| 项目 | 当前选择 |
| --- | --- |
| 双删、同名双新增 | 当前保守实现也纳入双方变化审核；并非只检查 modified/modified |
| entity 粒度 | 顶层 entity；class/namespace 内部成员不独立分析 |
| 格式、附着注释 | 计入原文变化；跨文件候选忽略部分格式不豁免严格冲突 |
| 展示 | 默认 diff3；自生成块裁剪三方共同前后文；Git zdiff3 可选 |
| 解析失败、身份模糊 | 双方变化时保守冲突或明确报错，不声称通过实体检查 |
| 行级下限 | 原始三份文本的 Git merge-file；源码依据见 [diff3-vs-zdiff3.md](diff3-vs-zdiff3.md#为什么可以只调用一次-git) |

这些是当前行为，不再列为待选择的初步建议。尚未完成的是上述审核闭环、更细粒度身份与移动识别、可靠上下文下的更丰富文本摘要；独立智能 diff 暂缓。调整这些能力仍不能取消既有严格约束。

## 10. 父子原因模型

父原因表达需审核事实，子依据说明 `git`、`weave`、`analyze` 的来源。同一可靠 entity 优先保留 weave 拒绝，再采用 weave 分类上的严格策略，最后检查未覆盖原文；不重复实现上游规则。

稳定原因码、全部动作与拒绝组合、归并及展示规则集中维护于 [conflict-reasons.md](conflict-reasons.md)，并通过 `include_str!` 供 rustdoc 使用。需求示例中的编号、逐行摘要和角色解释不是新增的原因码协议。
