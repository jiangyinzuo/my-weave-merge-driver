# 显式三方全局分析与 driver

单文件和跨文件规则的代码入口见 [已实现的分析](analysis.md)。移动匹配与审核策略位于 `src/analysis/moves.rs`，快照和报告读写位于 `src/analysis/global.rs`。

当前架构：

```text
明确的 base / ours / theirs
             ↓
strict-weave driver prepare
             ↓
只读分析结果文件（JSON）
             ↓
调用方执行原生 Git 操作 → 各文件 driver 校验输入并读取关联信息
```

工具只提供 driver，prepare 是其可选准备阶段。不包装 merge、rebase、cherry-pick 或 stash；不提供 diff 命令。Git 不提供可由 merge driver 注册的通用操作前 hook，所以全局采集必须由调用方显式触发。

## 输入、输出及退出状态

```sh
strict-weave driver prepare BASE OURS THEIRS --output /absolute/path/analysis.json
```

三个位置参数顺序固定，均是明确的 Git revision/tree。工具只用 `rev-parse`、`ls-tree`、`cat-file` 读取对象；不读取工作区作为版本，不推导 merge base，不调用 external diff、filters 或 hooks。结果保存已解析 tree ID，不依赖以后可能移动的 branch 引用。

报告按路径排序，包含每个变化文件三侧 SHA-256 指纹、严格冲突原因、移动关联、解析降级提示。当前格式为 schema v5 / strict-weave-global-v9，`reasons` 保存父节点 `kind`、`subject` 和完整子依据 `evidence`。旧 schema 或旧 engine 报告不兼容，须重新运行 prepare；未知类型、空证据或不合法的父子组合会拒绝读取。缺失文件以 null 指纹表示，空文件有正常指纹。非阻断关联保存在 `related_moves` / `move_candidates` 中，有移动候选不等于有冲突。首次写入使用临时文件原子安装，拒绝覆盖已有结果，并设置只读权限。只读权限防止意外修改，不是防篡改认证。

- `0`：完成，未发现当前内容规则要求审核的项。
- `1`：完成，存在审核项；JSON 仍已写出，stderr 列出原因。
- `129`：输入、分析、文件写入等错误；不生成可被误当成成功结果的新文件。
- `2`：CLI 参数错误。

prepare 不创建冲突文件或 index stages。需要先阅读 `1` 对应的报告；只有成功返回 `0` 才继续的调用方可直接使用 shell `&&`。无论哪种方式，工具都不代替用户执行 Git 操作。

## driver 消费协议

Git driver 原有调用形式保持：

```sh
strict-weave driver BASE_FILE OURS_FILE THEIRS_FILE SOURCE_PATH 7
```

可以附加 `--analysis FILE`，或由本次 Git 操作继承 `STRICT_WEAVE_ANALYSIS`。显式参数优先，且路径应使用绝对路径。推荐仅给一条 Git 命令设置环境变量，避免错误复用：

```sh
STRICT_WEAVE_ANALYSIS=/absolute/path/analysis.json git merge feature/example
```

driver 依次验证结果格式版本、路径及三侧文本指纹，再执行原有单文件严格检查。结果文件缺失、损坏、不兼容、缺少路径或输入不匹配，均在写回 ours 前退出 `129`。全局结果只增加审核项和说明，不能取消行级冲突或上游拒绝。原本 clean 的局部合并若命中全局冲突，则构造整文件 diff3 并返回 `1`，由调用它的 Git 管理 index。

没有分析文件时，driver 仍独立执行现有严格规则。不向共享文件追加状态，因而多个文件 driver 可并发读取同一结果。

## 父子原因的展示

`driver` 和 `driver prepare` 均支持 `--explain-reasons`。默认显示父原因和必要子依据（删除/新增/重命名动作、上游拒绝、能力不足、跨文件匹配）；详细模式展开包括原文与 weave 分类在内的全部子依据。两种模式使用同一结构，不改变冲突判定、输出范围或缓存内容。

来源标签统一为 `git`（行级基线）、`weave`（上游分类/拒绝）、`analyze`（额外的原文字节、布局、跨文件检查及保守降级）。来源说明数据出处；weave 分类上的严格冲突策略属于本项目，不等于 weave 已拒绝。

同一可靠 entity 优先采用 weave 拒绝；没有拒绝时，复用 weave 分类执行“双方变更必须冲突”；仍未覆盖的区域才检查原文。`disjoint.go` 因此只保留 weave 的 modified/modified 分类，修改/删除只保留 weave 的 modify_delete 拒绝及方向。无需重复分类与原文依据，也无需配对展示。身份无法确认时不按同名猜测覆盖；Git 行级冲突独立保留。

原文补充仍然必要：weave 会归一化 CRLF/LF 和 BOM，而本项目把原始文本变化也视为修改。`encoding.go` 验证双方仅将 LF 改为 CRLF 时仍然冲突（当前原文分区无法完整还原 CRLF，保守回退整文件）。weave 计算 body hash 前还会把自身名称替换成 `__ENTITY__`；原文真的把自引用改成该标识时，上游可能仍判为 unchanged，由我们的原文字节补充拦截。分区检查还负责完整展示、非实体区域及布局变化；跨文件关联由全局分析补充。完整规则目录见 [conflict-reasons.md](conflict-reasons.md)。

## 移动与疑似重命名规则

分别比较 base → ours 与 base → theirs。在一侧，源路径的某 entity 消失，另一条路径新增类型、名称、原始实体区域文本完全相同的 entity，产生“疑似移动”候选。附着注释也在原文比较内。多个源/目标全部保留，输出候选数，不通过遍历顺序选一个“正确”关联。

名称不同时，增加另一条匹配规则：要求同 grammar、同 entity 类型，复用 `weave_core::binding::replace_at_word_boundaries` 将双方各自名称替换为 `__ENTITY__`，完整区域文本逐 byte 相同才建立“疑似重命名并移动”候选。若原文不同，再复用 sem-core 公开的 `parse_tree` 比较完整语法表示：节点类型、字段、嵌套、token 顺序和原文均相同才匹配，仅忽略 token 间空白。注释、字符串、字符、模板等子树按原文保存；不把 Python 缩进导致的结构变化当作格式化。独立 entity 解析失败或存在未覆盖的非空白文本时，不使用这条回退规则。

grammar 使用 sem-core 的公开 registry，允许 `.js` / `.mjs` 等共享 grammar 的扩展名。原文包含 sentinel 时跳过此规则；不以 hash 或相似度判等。

JSON 的 `matched_by.kind` 区分 `exact` 与 `name_normalized`。后者另存 grammar、类型、旧名、新名和替换次数，以及 `comparison.kind=text/syntax`；syntax 模式保存比较单元数 `tokens`（注释/字面量子树整体算一个单元）。同一对优先记录原文匹配，避免重复候选。两侧次数相等且大于零。来源/目标候选数量包含两种匹配依据，全部保留，不优先选择同名候选。复用公共文本操作不意味着 weave 已判定此跨文件关系，诊断来源仍是 analyze。

若另一侧改变了该 base entity，包括无法确认其未改变的解析失败，则源路径和目标路径都记录 `GLOBAL_MODIFY_VS_MOVE`。例如：

```text
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：function calculate 疑似移动与另一侧变化需共同审核
  依据 [analyze]：MOVE_CANDIDATE：theirs 疑似移动 function calculate · source.go:1 → target.go:1；源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；sources=1，destinations=1；另一侧=modified
```

另一侧状态明确区分 unchanged、modified、deleted、无法确认；解析失败显示“无法确认”，不会写成已证实的修改。若另一侧也有同一 base entity 的移动候选，`opposite_moves` 保存全部目标、匹配依据和数量；诊断显示如 `另一侧=deleted（ours：疑似移动 …；候选数=1）`，保留 deleted 事实，不将疑似关联升级为确定身份。

源与目标文件的冲突 marker 均补充文件级移动关联，分别挂在相应 ours/theirs 方向，并含路径及行号。已有局部冲突也会标注，但不改源码和范围；双方移动时同时展示另一侧目标。详细规则见 [conflict-rendering.md](conflict-rendering.md)。

重命名移动示例（完整预期见 `tests/fixtures/multi-file/rename-modify.stderr-details/target.go`）：

```text
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：function calculate 疑似重命名并移动（calculate → renamed），与另一侧变化需共同审核
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 function calculate → renamed · source.go:1 → target.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，区域文本逐 byte 相同（含附着注释）；替换次数=1/1；sources=1，destinations=1；另一侧=modified；仅为文本候选，未证明语义等价
```

`--explain-reasons` 另说明所用公共函数、文本替换范围和候选数量含义。替换可能涉及自引用、注释和字符串，不等于只改定义名，也没有验证其它文件的调用更新。`sources` 为当前目标的候选来源数，`destinations` 为当前来源的候选目标数。另一侧未变时只显示 `关联 [analyze]`，不新增冲突。

这是确定性文本证据，不证明语义身份。复制后仍保留源 entity 不满足 deleted 条件。名称归一化后 token 内容或结构仍变化的编辑、跨 grammar 的改名移动、同文件移动暂不建立此关联；缺少候选不能解释为没有移动。解析不支持时明确报告关联不完整，继续保留单文件保守检查。候选组合或另一侧关联展开总数超过 10000 时明确失败，不截断输出报告。

## 必须明确的限制

**JSON 不能迫使 Git 调用 driver。** 双方产生同一 blob、删除、文件 rename 等情况可能走 Git 的其它路径。prepare 能提前发现其中一些风险，但如果忽略其退出 `1`，Git 仍可能直接提交。全局分析结果不等于“所有风险已转为未解决 index conflicts”。

本地三份文本的指纹匹配，也不证明整个操作对应报告中的三个 tree。另一次操作可能在该路径有完全相同的文本，却有不同的其它文件。调用方必须保证版本、方向和操作步骤一致；此校验只防止可观察的局部输入错配。报告没有授权任何自动解决，因此陈旧信息不会放宽单文件判定，但跨文件说明可能不适用。

Git 递归合成的 base、renormalize/filters 转换、文件 rename 后的路径映射可能造成校验失败；初版明确拒绝，不猜测。若未来要支持它们，应先定义有效的输入映射。rebase 每一步、merge commit cherry-pick 的 mainline、stash 的工作区/index 合并阶段不能共享未经验证的三方上下文。

完整操作级“不漏报实体冲突”仍是未解决的集成约束。本阶段不通过新增 Git 操作包装命令规避这个问题。普通单文件行级冲突始终由 driver 的独立基线保护。
