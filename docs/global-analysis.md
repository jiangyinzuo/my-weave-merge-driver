# 显式三方全局分析与 driver

全局分析读取明确的 base / ours / theirs，生成只读报告，由原生 Git 调用的各文件 driver 校验并读取。匹配规则集中在 [analysis.md](analysis.md)，自动确定三方并执行 Git 的入口见 [workflows.md](workflows.md)。Git 没有可由 merge driver 注册的通用操作前 hook，直接执行原生 Git 不会自动启动 prepare。

## 手动调用

```sh
strict-weave driver prepare BASE OURS THEIRS \
  --output /absolute/path/analysis.json --explain-reasons
```

三个位置参数均可使用明确的 commit/tree。prepare 通过 `rev-parse`、`ls-tree --full-tree`、`cat-file` 读取整个 tree 的变化文本，即使从子目录启动也不缩小范围。它不把当前工作区当作版本，不推导 merge base，不调用 external diff、filters 或 hooks，不修改 refs/index。

输出必须是新路径，报告原子写入并设为只读；不会覆盖已有文件。`--explain-reasons` 仅展开诊断，不改变报告或判定。

| 退出码 | 含义 |
| --- | --- |
| `0` | 报告已生成，未发现当前规则要求审核的项 |
| `1` | 报告已生成，有审核项；stderr 列出原因 |
| `129` | 读取、分析或写入失败，不生成可被误认作成功的新报告 |
| `2` | CLI 参数错误 |

prepare 不创建冲突文件或 index stages。只有返回 `0` 才继续的手动调用可使用 `&&`，例如已确认是单个共同祖先的 merge：

```sh
# BASE_COMMIT 必须是本次 Git merge 实际采用的唯一基准。
strict-weave driver prepare BASE_COMMIT HEAD feature/example \
  --output /absolute/path/analysis.json &&
STRICT_WEAVE_ANALYSIS=/absolute/path/analysis.json git merge feature/example
```

每次分析使用新报告路径。报告生成后输入版本不能改变；rebase/cherry-pick/stash 必须按实际步骤确定三方，不能直接照搬这个 merge 示例。常规操作推荐使用自动包装入口。

## 报告格式

当前 schema 为 **v5**，engine 为 **`strict-weave-global-v9`**。完整数据定义位于 [analysis/global.rs](../src/analysis/global.rs) 和 [reason.rs](../src/reason.rs)。

| 字段 | 内容 |
| --- | --- |
| `trees` | 按 base / ours / theirs 顺序保存解析后的 tree ID |
| `files` | 按路径排序；保存三侧原文 SHA-256 指纹、`reasons` 和 `related_moves` |
| `reasons` | 完整的 `kind / subject / evidence` 父子原因，见[原因模型](conflict-reasons.md) |
| `move_candidates` | 全部疑似移动关系，保留多来源/目标歧义 |
| `warnings` | 解析降级等非阻断说明，不伪装成冲突原因 |

缺失文件的指纹为 null，存在的空文件有正常指纹。移动依据的 `matched_by.kind` 为 `exact` 或 `name_normalized`；后者用 `comparison.kind=text/syntax` 区分原文与格式无关语法比较。`opposite_moves` 保存同一 base entity 在另一侧的全部移动候选。详细字段约束只在[跨文件子依据](conflict-reasons.md#跨文件子依据)维护。

## driver 消费协议

```sh
strict-weave driver BASE_FILE OURS_FILE THEIRS_FILE SOURCE_PATH 7 \
  --analysis /absolute/path/analysis.json
```

显式 `--analysis` 优先于环境变量 `STRICT_WEAVE_ANALYSIS`。不提供报告时，driver 独立执行单文件规则。提供报告时，先验证 schema/engine、父子结构、路径和三侧指纹；文件缺失、损坏、旧版、缺少路径或输入不匹配，均在写回 ours 前退出 `129`。

校验成功后仍执行本地严格检查。全局结果只能增加冲突和说明，不能取消行级冲突或 weave 拒绝。局部本来 clean、全局却要求审核时，构造自生成三方冲突块并返回 `1`；已有冲突则保留并附加关联，见[渲染规则](conflict-rendering.md#全局移动的双侧展示)。driver 不向报告追加状态，多个文件可并发读取同一份报告。

## 限制

- 单文件输入必须为 UTF-8、无 NUL、无已有 conflict marker，不超过 1,000,000 bytes。预分析拒绝非 UTF-8 路径、symlink/submodule；变化文本总量和报告各限 64 MiB。候选组合及另一侧关联展开总数分别限 10000，超限报错而非截断。
- 指纹只验证当前文件输入，不证明整个 Git 操作与报告 tree 一致；调用方负责版本、方向和步骤一致。driver 的空输入不能区分缺失文件与空文件，报告本身则区分两者。
- 虚拟 base、filters/renormalize 转换、文件 rename 路径映射可能导致输入失配；当前明确拒绝，不猜测映射。包装命令的上下文限制见 [workflows.md](workflows.md)。
- **报告不能迫使 Git 调用 driver。** 包装入口会在 prepare 有审核项时停止；手动调用若忽略 `1`，Git 仍可能跳过 driver 并提交。预分析停止不等于所有风险已成为 index conflicts。
- 只读权限用于防止意外修改，不是防篡改认证。移动候选只表示可解释的文本证据；未发现候选不代表不存在移动，能力范围见[跨文件规则](analysis.md#跨文件规则目录)。
