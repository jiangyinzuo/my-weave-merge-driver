# diff3 与 zdiff3：区别及本项目的展示策略

`diff3` 和 `zdiff3` 都使用 `<<<<<<<`、`|||||||`、`=======`、`>>>>>>>` 标记，展示 ours、base、theirs 三份内容。

主要区别是：**zdiff3 会将冲突块首尾处 ours 和 theirs 相同的行移到块外，让冲突更紧凑。** 即使这些行与 base 不同，也可能被移出去。它不会任意抽走冲突块中间所有相同的行。

## 对比示例

base：

```text
start
old value
end
```

ours：

```text
start
shared addition
ours value
end
```

theirs：

```text
start
shared addition
theirs value
end
```

使用 Git 2.53.0，通过 `git merge-file -p --diff3 -L ours -L base -L theirs ours base theirs` 得到：

```text
start
<<<<<<< ours
shared addition
ours value
||||||| base
old value
=======
shared addition
theirs value
>>>>>>> theirs
end
```

将命令中的 `--diff3` 换为 `--zdiff3`，得到：

```text
start
shared addition
<<<<<<< ours
ours value
||||||| base
old value
=======
theirs value
>>>>>>> theirs
end
```

两边共同新增的 `shared addition` 被放到了冲突块外，只保留一份。这个示例中，两种命令都返回冲突；变化的是展示边界。

## 对本项目的影响

本项目要求：同一实体相对 base 被双方修改，就必须人工处理，即使 ours 和 theirs 的结果完全相同。例如双方分别新增文件，却都把同一个计数从 2 改为 5，结果相同仍可能掩盖丢失更新。

因此，必须区分三个职责：

| 职责 | 本项目要求 |
| --- | --- |
| 判定是否冲突 | 执行严格实体规则，保留 Git 行级冲突和 weave-core 明确拒绝合并的结果；不能由展示时的相同行消除来决定 |
| 决定展示范围 | 由我们的 driver 根据实体三方范围控制，必要时扩展到完整函数／方法，处理重叠、顺序及移动 |
| 渲染冲突文本 | 在选定范围内使用三方标记、中文说明和版本身份；紧凑展示不能取消必须人工处理的冲突 |

**选择 diff3 并不自动意味着展示完整函数。** Git 的行级合并不负责识别函数边界；完整实体展示需要我们的 driver 显式选择范围，并保留对应的三方原文。即便采用 diff3 标记，也不能直接把 Git 默认输出当作完整实体展示。

**选择 diff3 也不会让 Git 对相同修改报冲突。** 若将完全相同的两侧内容直接交给普通行级合并，它们可能被自动接受。我们的严格规则必须单独构造和保留这类冲突。Git 跳过 driver 时的补充检查约束见 [requirement.md 第 8 节](requirement.md#8-整个文件内容相同时的需求约束)。

## 建议的展示策略

- **实体规则触发的冲突：** 默认用 diff3 标记展示完整三方实体，保留两侧相同的修改，不采用 zdiff3 的边界压缩。即使同时触发行级冲突，也优先采用完整实体展示。
- **纯行级冲突：** 可以支持 zdiff3，让冲突更紧凑；这是可选展示能力，不改变必须人工处理的结论。
- **范围无法安全扩展：** 保留原行级冲突块，在报告中关联实体；不能为了完整展示而复制、遗漏或重排代码。

这些是本项目的目标行为，不是设置 Git 的 `merge.conflictStyle` 就能自动获得的能力。自定义 driver 必须显式实现或读取相应展示配置；全局分析结果触发 driver 冲突时，也应使用同一套判定与渲染逻辑。

## 当前可选实现

`strict-weave driver ... --zdiff3` 显式开启紧凑行级展示，默认仍采用原有 diff3 / 完整实体展示，不自动跟随 Git 的全局 `merge.conflictStyle`。严格判定仍以原始三份文本的普通行级基线和实体规则为准。

纯行级冲突可用 Git zdiff3 渲染。对于原本的实体布局整文件回退，首版只在确认 base 原文未变、双方仅在文件末尾新增不同实体及空白、没有其它严格冲突原因时允许紧凑展示。`adjacent.ts` 属于此类；同名实体的双方新增、同一实体双方修改、无法确认的布局变化仍保留完整三方冲突，不承诺全部冲突都能压缩。

测试框架通过可选的 `.output-zdiff3` 单独验证该模式，见 [testing.md](testing.md)。
