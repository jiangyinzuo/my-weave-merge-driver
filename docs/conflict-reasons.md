# 父子冲突原因

各项分析的触发条件、执行顺序和代码入口见 [已实现的分析](analysis.md)。本文描述这些分析生成的原因模型。

代码按职责分为 `src/reason/mod.rs`（数据类型、上游映射和归并）、`display.rs`（摘要与详细文本）和 `validate.rs`（反序列化数据校验）。本文件通过 `include_str!` 包含为 reason 模块文档；展示不参与冲突判定。

父节点 `Reason` = 必须人工审核的事实，子节点 `Evidence` = 检查依据。
weave、原文比较、Git、全局匹配是并列的证据来源，不是固定的父子层级。
任一父节点存在即为冲突；子节点的显示/折叠、去重、diff3/zdiff3 均不参与
是否冲突的判定。纯移动候选和单方解析警告不是冲突，不得单独创建父节点。

来源由 Evidence 类型确定：git = 行级基线；weave = 上游分类或拒绝；
analyze = 本项目的原文检查、布局检查、能力降级和跨文件分析。
同一可靠 entity 依次采用：weave 明确拒绝 → weave 分类上的严格策略 →
未覆盖原文的严格补充。已有拒绝不再追加分类规则；已有实体冲突不再重复
执行原文阻断规则；可补充逐 byte 相同结果的说明。weave 的匹配、动作分类、拒绝规则均直接复用，不在本项目重写。
weave 分类本身不代表上游拒绝：双方变更必须审核是本项目额外的策略。
原文补充覆盖 weave 名称归一化等可能忽略的字节变化；例如上游计算 body hash
前把自身名称替换成 __ENTITY__，可能把不同原文判为 unchanged。CRLF/LF、
BOM 也会归一化，原文分区无法完整还原时由能力降级保守阻断。
非实体文本、布局、跨文件移动亦属于额外规则。
无法可靠关联的目标不互相替代，保留独立原因以免漏报。
来源标签表示数据出处，不能理解为 weave 和 analyze 共同拒绝；
“weave 未返回分类”引起的保守阻断属于 analyze。

## 全部父原因及其触发条件

| `Kind` / 稳定代码 | 触发条件 | 子依据 | 默认展示 |
| --- | --- | --- | --- |
| `LineConflict` / `LINE_CONFLICT` | 原始三份文本的 Git merge-file --diff3/--zdiff3 返回冲突，覆盖普通模式冲突；分区内行级冲突亦保留 | `GitLineConflict` | Git 行级冲突 |
| `EntityConflict` / `ENTITY_CONFLICT` | 优先保留 weave 拒绝；未拒绝目标用 weave 双方变化分类补充严格冲突；仍未覆盖时检查原文双方变化 | `RawBothChanged`、`IdenticalEdits`、`WeaveActions`、`WeaveRefusal`，任一即可独立触发 | entity；额外动作/拒绝展开 |
| `NonEntityConflict` / `UNMODELED_BOTH_CHANGED` | 对应的非实体分区相对 base 在两侧都变化 | `RawBothChanged` | 非实体区域双方修改 |
| `AnalysisUnavailable` / `ENTITY_ANALYSIS_UNAVAILABLE` | 文件双方变化，且无法获得可靠原文分区或 weave 分类 | `PartitionUnavailable` 或 `WeaveUnavailable` | 无法可靠分析；整文件保守冲突 |
| `LayoutChanged` / `ENTITY_LAYOUT_CHANGED` | 三方可解析，但实体/间隙序列不同，且文件双方变化 | `LayoutChanged` | 实体增删或顺序变化 |
| `ModifyVsMove` / `GLOBAL_MODIFY_VS_MOVE` | 一侧存在跨文件精确移动或名称归一化匹配的删除/新增候选，另一侧源实体变化或无法确认未变 | `MoveCandidate` | 疑似移动与另一侧变化冲突 |

“双方变化”按本次快照判断，包含结果相同、格式/附着注释变化；非历史提交扫描。
双删、同名双新增在当前保守策略中亦须审核；父原因不宣称业务语义错误。
同一文件可以同时有多个父节点；line conflict 不强行归属 entity，防止丢掉
无法定位的行级证据。父子深度固定为两层，不接受任意递归原因树。

例如 disjoint.go 只保存上游分类，严格策略将其升级为冲突：

```text
EntityConflict(function a)
└─ WeaveActions { ours: Modified, theirs: Modified }
```

修改/删除直接保留 WeaveRefusal::ModifyDelete 及方向，不再追加分类或
本地重复依据，也无需分类/拒绝的展示配对。若 weave 未拒绝另一个 entity，
仍继续检查那个 entity；优先级按可靠目标生效，不是全文件短路。
渲染复用已选定的原因；原文分区负责可靠身份、显示范围、未覆盖区域的补充和相同结果说明。

## 全部 weave 子原因

`WeaveRefusal` 穷尽映射固定依赖版本的 ConflictKind，无通配分支：

- `BothModified`：双方修改且 weave 未解决。
- `ModifyDelete`：一侧修改、另一侧删除，保存修改方。
- `BothAdded`：双方新增同一标识且内容不同。
- `RenameRename`：双方重命名，保存 base/ours/theirs 三个名称。
- `RenameModify`：一侧重命名、另一侧修改，保存旧名、新名、重命名方。

`Change` 穷尽 weave 动作：Added、Absent、Deleted、Unchanged、Modified、
Renamed、RenameModified。仅 Added/Deleted/Modified/Renamed/RenameModified
计入双方变化；因此 5×5 种动作组合均可作为严格实体冲突的证据。
renamed 仅为上游候选分类，不能宣称语义身份已证明。所有双方动作默认展开，
包括修改、删除、新增和重命名；详细模式包含所有来源，不损失依据。

下表穷举会触发严格规则的 25 种双方动作组合（行=ours，列=theirs）。
R 表示 rename candidate，RM 表示 rename + modified candidate；名称关联
由上游提供，表格不把候选升级为已确认事实。每格均须 EntityConflict；
已有同目标 refusal 时直接采用，否则依据该分类添加严格冲突：

| ours / theirs | added | deleted | modified | R | RM |
| --- | --- | --- | --- | --- | --- |
| added | 同标识双方新增 | 新增/删除 | 新增/修改 | 新增/重命名 | 新增/重命名并修改 |
| deleted | 删除/新增 | 双删（当前保守策略） | 删除/修改 | 删除/重命名 | 删除/重命名并修改 |
| modified | 修改/新增 | 修改/删除 | 双方修改，结果相同也冲突 | 修改/重命名 | 修改/重命名并修改 |
| R | 重命名/新增 | 重命名/删除 | 重命名/修改 | 双方重命名，同目标也审核 | 重命名/重命名并修改 |
| RM | 重命名并修改/新增 | 重命名并修改/删除 | 重命名并修改/修改 | 重命名并修改/重命名 | 双方重命名并修改 |

上游实际三方匹配未必能产生每格；此表完整说明我们的接收策略。
其余 24 种包含 unchanged/absent 的组合不由 WeaveActions 触发冲突，
但 RawBothChanged、WeaveRefusal、GitLineConflict 等独立依据仍可触发。

## 跨文件子依据

`MoveCandidate` 来源为 analyze，保存方向、base 源位置、移动侧目标位置、
候选来源/目标数量和另一侧状态。`matched_by` 穷举两种文本匹配依据：

- `Exact`（JSON `exact`）：类型、名称、完整原始 entity 区域文本相同。
- `NameNormalized`（JSON `name_normalized`）：grammar/type 相同，名称不同，
  各自名称经 weave 公开的词边界替换后，比较完整区域文本或格式无关语法表示。
  保存 grammar、entity_type、old_name、new_name、两侧替换次数。
  `comparison` 穷举 `Text` / `Syntax { tokens }`：前者逐 byte 比较原文；后者复用 sem-core 解析后，直接比较节点类型、字段、嵌套与有序 token 原文，仅忽略 token 间空白。注释/字面量子树保留原文并计为一个比较单元，tokens 必须大于零。原文匹配优先，同一对不重复计数。
  名称替换次数必须相等且大于零；原文已含 `__ENTITY__` 时跳过该规则。

两者均要求源 deleted、异路径目标 added，不证明语义身份。所有匹配组合保留；
`sources` 为当前目标的候选来源数，`destinations` 为当前来源的候选目标数，
统计同时包含精确和归一化匹配。不会因为某个同名候选而丢弃其它改名候选。

`Opposite` 穷举 unchanged / modified / deleted / unknown：只有 unchanged 不因
此候选新增父原因；其余三类在源与目标路径产生 `GLOBAL_MODIFY_VS_MOVE`。
unknown 表示源路径在另一侧无法可靠解析，不能写成已确认修改。

`opposite_moves` 保存同一 base 位置在另一侧的全部候选，元素为非递归的
`OppositeMove { target, matched_by, source_count, destination_count }`。
源位置已消失仍保留 `opposite=deleted`；展示在其后补充另一侧方向、疑似目标和候选数。
详细模式展开另一侧的匹配依据。关联只能在 deleted 状态出现，须合法、排序且不重复；
没有候选时不推断另一侧已经移动。此字段只丰富说明，不改变冲突策略。

默认重命名诊断列出名称、路径/行号、匹配条件、替换次数、歧义数量和另一侧状态。
详细模式解释替换可能涉及定义、自引用、注释和字符串，并未检查外部调用或语义。
纯重命名移动也显示非阻断 `关联 [analyze]`；已在父原因下显示的候选不重复打印。

## 身份、去重及持久化

父节点只按 `Kind` + `Subject` 精确合并，子依据按完整结构排序去重。
不对人类提示使用 starts_with/contains 来决定冲突或包含关系。
本地 entity 以类型和名称定位；仅在可靠分区中名称唯一且上游标签唯一时，
才把 weave 分类挂到本地 entity。无法确认者用独立 Weave subject（含来源
和条目序号），即使显示名一样也不合并。不同非实体分区使用不同 key。`Subject::Gap` 另存非空 `label`，由可靠分区的相邻 entity 类型和名称生成；人类输出不显示内部 key，改用“文件开头、function a 之前”“function a 与 function b 之间”“文件末尾、function b 之后”的非实体区域说明。标签不推断区域用途或业务语义。
跨文件移动身份包含方向和源/目标位置，歧义候选不会被折叠成唯一关联。
`normalize` 合并只会减少重复节点，不会消除最后一个冲突依据。
非阻断的 related_moves 独立保存；其出现或去重不改变 conflicted()。

全局报告序列化完整父子结构；格式版本及校验协议见 [global-analysis.md](global-analysis.md)。
旧算法报告必须拒绝并重新生成，保证匹配依据与当前规则一致。
JSON 结构错误、未知枚举、空证据或父子/目标类型不兼容均视为处理错误。

## 不属于冲突原因的处理错误与限制

binary/非 UTF-8/超限、已有 marker、marker 宽度非法、Git 子进程失败、
I/O 失败、分析文件损坏/旧版/缺路径/指纹不匹配等返回 Err（CLI 129），
不伪装成 clean，也不把错误文字注入原文。参数解析错误由 CLI 返回 2。
`PartitionUnavailable` 统一涵盖未支持语言、语法错误、无实体、边界重叠、
重复名称、分区不能完整还原原文；现有解析 API 未区分具体失败原因，故不猜测。
全局移动侧解析失败会产生非阻断 warning；只有结合另一侧移动候选或本地双方
变化时才触发保守冲突。操作命令会将原因对应的路径安装为 Git index conflict；merge/cherry-pick 由 Git 继续或中止，rebase 使用 strict-weave 的 `--continue/--abort`，以保证后续 commit 重新全局分析。

## 相同修改结果说明

`IdenticalEdits` 来源为 analyze：仅在三方原文分区可靠、布局一致且对应 entity 满足 `ours == theirs != base` 时添加。比较包含格式和附着注释，不使用归一化 hash；不推断编辑历史或业务语义相同。该事实描述 entity 本身，不要求整个文件相同。

已有 weave 原因保留，追加此原文事实用于说明；未被 weave 覆盖的相同变化直接采用此事实，避免再添加重复的 RawBothChanged。默认父原因和 marker 提示“双方修改结果相同，但均不同于 base”；详细模式标明逐 byte 比较来源。普通双方修改默认展开 weave 动作分类。未知/不可靠原文不宣称结果相同。
