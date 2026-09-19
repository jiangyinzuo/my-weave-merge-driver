# 显式三方全局分析与 driver

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

报告按路径排序，包含每个变化文件三侧 SHA-256 指纹、严格冲突原因、移动关联、解析降级提示。缺失文件以 null 指纹表示，空文件有正常指纹。首次写入使用临时文件原子安装，拒绝覆盖已有结果，并设置只读权限。只读权限防止意外修改，不是防篡改认证。

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

## 第一版移动规则

分别比较 base → ours 与 base → theirs。在一侧，源路径的某 entity 消失，另一条路径新增类型、名称、原始实体区域文本完全相同的 entity，产生“疑似移动”候选。附着注释也在原文比较内。多个源/目标全部保留，输出候选数，不通过遍历顺序选一个“正确”关联。

若另一侧改变了该 base entity，包括无法确认其未改变的解析失败，则源路径和目标路径都记录 `GLOBAL_MODIFY_VS_MOVE`。例如：

```text
MOVE_CANDIDATE：theirs 疑似移动 function calculate
source.go:1 → target.go:1
依据：源 deleted、目标 added；类型/名称/原始实体区域文本相同
sources=1，destinations=1
GLOBAL_MODIFY_VS_MOVE：另一侧改变了 base 实体或无法确认未改变，需共同审核
```

这是确定性文本证据，不证明语义身份。复制后仍保留源 entity 不满足 deleted 条件。名称变化、移动同时编辑、同文件移动暂不建立此关联；缺少候选不能解释为没有移动。解析不支持时明确报告关联不完整，继续保留单文件保守检查。

## 必须明确的限制

**JSON 不能迫使 Git 调用 driver。** 双方产生同一 blob、删除、文件 rename 等情况可能走 Git 的其它路径。prepare 能提前发现其中一些风险，但如果忽略其退出 `1`，Git 仍可能直接提交。全局分析结果不等于“所有风险已转为未解决 index conflicts”。

本地三份文本的指纹匹配，也不证明整个操作对应报告中的三个 tree。另一次操作可能在该路径有完全相同的文本，却有不同的其它文件。调用方必须保证版本、方向和操作步骤一致；此校验只防止可观察的局部输入错配。报告没有授权任何自动解决，因此陈旧信息不会放宽单文件判定，但跨文件说明可能不适用。

Git 递归合成的 base、renormalize/filters 转换、文件 rename 后的路径映射可能造成校验失败；初版明确拒绝，不猜测。若未来要支持它们，应先定义有效的输入映射。rebase 每一步、merge commit cherry-pick 的 mainline、stash 的工作区/index 合并阶段不能共享未经验证的三方上下文。

完整操作级“不漏报实体冲突”仍是未解决的集成约束。本阶段不通过新增 Git 操作包装命令规避这个问题。普通单文件行级冲突始终由 driver 的独立基线保护。
