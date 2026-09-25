# 智能 diff：暂缓实现的需求

当前没有独立 diff 命令。此处仅保留未来人工检视功能的需求；已实现的移动/重命名匹配见 [analysis.md](analysis.md#跨文件规则目录)，原生 Git 操作和可选 prepare 见 [workflows.md](workflows.md)。

## Git 已有能力

```sh
git diff --color-moved=dimmed-zebra --color-moved-ws=allow-indentation-change OLD NEW
```

| 选项 | 能提供什么 | 边界 |
| --- | --- | --- |
| `--color-moved` | 匹配增删文本的移动着色，可跨文件 | 不建立可靠函数身份；短小、同质或移动后修改的代码可能无法完整关联 |
| `--color-moved-ws=allow-indentation-change` | 辅助识别整体缩进变化 | 不等于忽略任意语法变化 |
| `-M` / `-C` | 文件 rename/copy 检测 | 不是函数级匹配 |
| `-W` / `--function-context` | 扩大函数上下文 | 不提供跨文件关系 |
| `--histogram` / `--patience` | 改善部分文本重排的匹配 | 不生成结构化 entity 关系 |

路径过滤会限制可见源/目标，普通工作区 diff 不含未跟踪文件。external diff/textconv 通常按文件调用，也缺少天然全仓库上下文。修改 diff 展示不会改变 merge 判定；不应通过解析 ANSI 颜色建立实体关系。

## 未来接口应满足的要求

- 接收明确的两份快照，输出只读检视结果，与严格合并共用解析和候选规则。命令拼写、工作区/index 模式及未跟踪文件策略尚未确定。
- 分开展示位置与内容变化，覆盖新增、删除、修改、重命名、移动和移动并修改；区分复制、普通行号漂移和真实作用域变化。
- 提供路径、版本、范围、匹配方法和歧义候选；不确定时明确写“疑似”，分数不能冒充正确率。
- 展示移动摘要后，仍能检视完整实体和原始增删文本，包括格式、注释及非实体区域。报告不默认宣称可被 `git apply` 使用。
- 先确定分析范围，再过滤展示；解析失败或范围受限时保留原始 diff，不将“未找到候选”写成“没有移动”。

例如未来识别移动并修改时，可以展示：

```text
疑似移动并修改 · function target()
OLD old.ts:1 → NEW new.ts:1
依据：源 deleted、目标 added；类型、名称、声明行相同；候选目标数=1
候选文本变化：
-  return 1;
+  return 2;
```

这是目标示例，不是当前匹配器的能力承诺。当前跨文件规则要求原文或名称归一化后的文本/语法表示相同，尚不匹配这种额外正文编辑。类型、名称等筛选事实也不能单独证明移动关系。

Tree-sitter 提取语法，不维护跨版本身份。weave 的重命名匹配和可复用 API 见[重命名调研](weave-rename-research.md)。即使将来 diff 能识别更多移动，也不能据此取消[修改与移动](modify-vs-move.md)必须共同审核的要求。
