# strict-weave

基于 [weave-core](https://github.com/Ataraxy-Labs/weave) 的严格 Git merge driver。它保留 Git 行级冲突，并在双方修改同一 function/entity 时要求人工审核，包括修改不同行或得到相同结果的情况。冲突提示使用中文，保留 function、ours、theirs、base 等术语。

只需配置一次 Git merge driver 和 `.gitattributes`，之后继续使用原生 Git 的全部命令。driver 在 Git 实际要求合并文件时执行严格检查；Git 仍完全负责 merge、rebase、cherry-pick、stash、pull 以及 `--continue`、`--abort`、`--quit` 等状态流程。

## 安装

需要 Rust/Cargo 工具链、Git，以及编译 Tree-sitter 依赖所需的 C/C++ 编译环境。在本项目源码目录执行：

```sh
cargo install --path . --locked
strict-weave --help
```

确保 Cargo 的安装目录（默认 `$HOME/.cargo/bin`）在 `PATH` 中，且从终端或编辑器启动的 Git 都能找到 `strict-weave`。首次构建需要下载依赖。

## 在仓库中启用

进入需要使用 driver 的 Git 仓库，执行：

```sh
git config --local merge.strict-weave.name 'Strict entity and line merge'
git config --local merge.strict-weave.driver 'strict-weave driver %O %A %B %P %L --ours-label %X --base-label %S --theirs-label %Y'
```

`%O/%A/%B` 分别是 Git 提供的 base/ours/theirs 临时文件，`%P` 是源码路径，`%L` 是 conflict marker 宽度；这些占位符应原样保留。`%X/%S/%Y` 提供版本标签，便于识别双方来源。上述配置已在 Git 2.53.0 验证；如果所用 Git 不支持版本标签占位符，可使用基础配置：

```sh
git config --local merge.strict-weave.driver 'strict-weave driver %O %A %B %P %L'
```

基础配置会将版本标签显示为“来源未识别”。

在仓库根目录的 `.gitattributes` 中追加需要启用的文件类型，例如：

```gitattributes
*.go  merge=strict-weave
*.ts  merge=strict-weave
*.tsx merge=strict-weave
*.cpp merge=strict-weave
*.hpp merge=strict-weave
*.py  merge=strict-weave
```

按实际需要选择扩展名，完整支持范围见 [语言支持](docs/languages.md)。`.gitattributes` 可以随仓库提交；Git 的本地配置不会随 clone 分发，其他使用者也需要安装工具并配置 driver。

之后正常执行 Git 操作即可。出现冲突时，查看文件中的 conflict marker 和终端原因提示，编辑解决后使用 `git add`，再按 Git 提示完成或继续操作。

## 展示选项

在 driver 配置命令末尾添加所需选项：

| 选项 | 效果 |
| --- | --- |
| `--zdiff3` | 使用 Git 的 zdiff3 行级展示；默认是 diff3，不改变严格冲突判定 |
| `--explain-reasons` | 展开全部检查依据，包括移动候选的匹配方法和限制 |

例如，同时启用两者：

```sh
git config --local merge.strict-weave.driver 'strict-weave driver %O %A %B %P %L --ours-label %X --base-label %S --theirs-label %Y --zdiff3 --explain-reasons'
```

## 直接使用原生 Git

完成配置后，直接执行平时的 Git 命令：

```sh
git merge feature/payment
git rebase main
git rebase -i --rebase-merges main
git cherry-pick <commit>
git pull --rebase
git stash pop --index
```

merge/rebase/pull 要求干净的工作区和 index（包括未跟踪文件）。stash 允许已有 tracked 修改，但暂不支持当前工作区有未跟踪文件。预分析通过才执行原生 Git；发现审核项就停止并显示报告路径，没有忽略审核继续的开关。

冲突时仍使用 Git 原生命令继续、跳过或中止：

```sh
git rebase --continue
git rebase --skip
git rebase --abort
```

driver 返回冲突时，编辑文件并执行 `git add`，再按 Git 的提示完成当前操作。strict-weave 不保存自己的操作状态，也不提供 `--continue`、`--abort` 或 `--quit` 包装命令。

## 手动三方预分析

需要自行集成 Git 操作时，可提供明确的 base / ours / theirs commit/tree：

```sh
strict-weave prepare BASE_TREE OURS_TREE THEIRS_TREE \
  --output /absolute/path/analysis.json --explain-reasons
```

报告路径必须尚不存在；prepare 只读取 Git 对象，返回 `0` 才适合继续对应步骤。driver 可通过 `STRICT_WEAVE_ANALYSIS` 或 `--analysis` 读取报告，并校验路径与三方指纹。报告不能跨操作或 rebase 步骤随意复用。旧版 `strict-weave driver prepare ...` 仍可兼容使用。输入含义、调用示例及限制见 [全局分析协议](docs/global-analysis.md)。

## 退出码与使用边界

| 退出码 | driver | driver prepare |
| --- | --- | --- |
| `0` | 合并完成，无冲突 | 报告已生成，未发现审核项 |
| `1` | 写回带冲突的结果，需人工处理 | 报告已生成，存在审核项 |
| `129` | 处理失败，未写回 ours | 处理失败，未生成新报告 |
| `2` | CLI 参数错误 | CLI 参数错误 |

**Git 可能跳过 driver**，例如双方产生相同文件内容、部分删除或文件 rename。仅配置 driver 或忽略手动 prepare 的非零退出码仍有盲区。prepare 不会创建 index conflict，也不证明代码语义正确。

当前按顶层 entity 分析，class/namespace 内部成员不独立匹配；无法可靠分析时保守冲突或明确报错。详细限制见 [当前实现状态](WIP.md)。

## 文档索引

| 文档 | 内容 |
| --- | --- |
| [需求与示例](docs/requirement.md) | 严格策略、目标提示、尚未完成的体验要求 |
| [操作命令](docs/workflows.md) | 原生 Git 操作、driver 边界及可选 prepare |
| [分析规则](docs/analysis.md) | 已实现规则、执行顺序与代码入口 |
| [原因模型](docs/conflict-reasons.md) | 全部父原因、子依据和归并规则，同时用作 rustdoc |
| [全局分析协议](docs/global-analysis.md) | prepare、报告格式、driver 校验及输入限制 |
| [冲突块渲染](docs/conflict-rendering.md) | 展示选择、共同文本裁剪与移动标签 |
| [diff3 与 zdiff3](docs/diff3-vs-zdiff3.md) | 输出对比、Git 冲突下限的源码依据 |
| [语言支持](docs/languages.md) | 上游 grammar 覆盖与解析边界 |
| [Entity type](docs/entity-types.md) | 上游 entity type、归一化规则与跨语言 fixtures |
| [测试说明](docs/testing.md) | 添加/运行 fixtures、端到端测试及人工检查索引 |
| [修改与移动](docs/modify-vs-move.md) | Git 行为、跨文件审核需求与当前覆盖 |
| [重命名匹配调研](docs/weave-rename-research.md) | 上游匹配依据及公共 API 的复用边界 |
| [成员粒度取舍](docs/member-conflicts-research.md) | 顶层判定与共同文本裁剪的选型依据 |
| [智能 diff 草案](docs/smart-diff.md) | 暂缓实现的独立检视功能 |
