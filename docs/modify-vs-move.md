# 一方修改，另一方移动

**需求：一方修改 function，另一方移动它，必须共同审核。** 即使 Git 能沿文件 rename 自动应用修改，也不能据此自动接受。尽可能展示疑似目标、匹配依据和另一侧状态；不能推断正确的最终位置或内容。

当前已支持跨文件精确移动及名称归一化、格式无关的疑似重命名移动。额外正文编辑、同文件位置/作用域移动仍未覆盖；规则见 [analysis.md](analysis.md#跨文件规则目录)。

## Git 通常如何处理

base 的 `old.ts`：

```typescript
export function target() {
  return 1;
}

export function stay() {
  return 0;
}
```

A 将 target 的返回值改为 2；B 从 old.ts 删除 target，原样放入 new.ts，保留 stay。Git rename 检测针对文件，不能保证识别这种函数移动。

Git 2.53.0 关闭 rename 检测的对应实验中，old.ts 为 `UU`，new.ts 为 `A`。原文件发生修改/删除冲突，新文件仍是返回 1 的旧实现，不会自动获得 A 的修改。若只处理旧文件，可能遗漏跨文件关联；应由人决定最终位置与文本。

这不是所有移动的固定结果：整个旧文件删除时可能是文件级 modify/delete；文件 rename 被识别时可能沿新路径合并。严格检查不能依赖 Git 恰好报告哪一种冲突。

## 当前提示与展示

[move-source.go](../tests/fixtures/moves/move-source.go.base) 及 [move-target.go](../tests/fixtures/moves/move-target.go.theirs) 对应的诊断示例：

```text
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：function calculate 疑似移动与另一侧变化冲突
  依据 [analyze]：MOVE_CANDIDATE：theirs 疑似移动 function calculate · source.go:1 → target.go:1；源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；sources=1，destinations=1；另一侧=modified
```

候选是文本证据，不证明语义身份。多个来源/目标全部保留；另一侧也移动时，保留源位置 deleted 的事实，并补充该侧的疑似目标。重命名报告另显示旧名、新名、归一化方法和替换次数，完整预期见 [rename-modify](../tests/fixtures/multi-file/rename-modify.stderr-details/target.go)。

文件 marker 可附上关联位置，但原位置删除侧仍为空；不能把目标文件的函数伪装成源文件 theirs 正文。源、目标文件的双侧标签规则见 [conflict-rendering.md](conflict-rendering.md#全局移动的双侧展示)。

## 必须保持的边界

- 目标既可能是新文件，也可能是既有文件中的新增 entity；不能只搜索新增文件。
- 保留原定义的复制不满足 deleted 条件；普通行号漂移也不能直接视为移动。
- 没找到候选时，保留本地修改/删除及其它严格冲突；不能把“未识别”解释成没有移动或允许丢弃修改。
- 匹配只增加说明或冲突，不自动迁移修改、恢复旧定义、覆盖目标或改写调用点。没有完成跨文件引用/语义验证。
- 有报告不等于有 index conflict。prepare 只读；原生 Git 跳过 driver 时，报告本身不能安装冲突 stages。尚未完成的审核体验见 [需求第 8 节](requirement.md#8-整个文件内容相同时的需求约束)。

## 验证与剩余目标

| 场景 | 当前覆盖 |
| --- | --- |
| 修改 vs 原样移到新文件/既有文件 | 生成候选，源与目标均审核 |
| 移动并改名、格式化 | 名称归一化后的原文或语法表示相同才关联 |
| 多来源/目标、双方都移动 | 保留全部候选，不强行一对一配对 |
| 复制且保留原 entity | 不作为移动候选；本地严格规则继续检查 |
| 文件 rename 后 Git 不调用 driver | 预分析可报告命中项，包装停止；不自动生成 index stages |
| 移动并额外编辑正文、同文件作用域/位置移动 | 未实现可靠关联；不能据此放宽现有冲突 |

文本与报告预期位于 [multi-file/](../tests/fixtures/multi-file/)，真实仓库流程位于 [tests/integration/](../tests/integration/)。测试检查停止状态和源/目标内容，不宣称能证明所有解析后代码没有重复定义或引用错误。独立人工检视功能保留在 [智能 diff 草案](smart-diff.md)。
