基于[weave](https://github.com/Ataraxy-Labs/weave)的weave-core实现严格的、同时基于entity和行级的git merge driver。

## 功能需求

### 当两个分支同时修改同一个函数的不同内容时，产生merge conflict

例如base commit: 保留xyz
```go
func a() int {
    x := 1
    y := 2
    z := 3
    return y
}
```

update1 commit: 保留xy
```go
func a() int {
    x := 1
    y := 2
    return y
}
```

update2 commit: 保留yz
```go
func a() int {
    y := 2
    z := 3
    return y
}
```

行级diff中，update1和update2不会报merge conflict，但我认为它们修改了相同函数，理应让人类进一步判断其中的逻辑是否实现正确，故需要报merge conflict。

### 保留行级diff产生的merge conflict，但产生更加人类友好的merge conflict信息

当 Git 行级合并实际产生 conflict 时，即使 weave 能自动合并，也必须保留 conflict，并提供实体层级的原因说明。具体示例见 [requirement.md](requirement.md) 中的示例 B。

值得注意的是，[weave的README](https://github.com/Ataraxy-Labs/weave/blob/main/README.md#weave-vs-git-merge)
中列举了很多line-based merger会报错，weave会自动合并的场景，我希望保持对它们报merge conflict，但给出友好的、方便人类阅读的merge conflict保持信息。

### 人类友好的merge conflict 信息增强

- 传统merge conflict信息中，很难分清谁是ours，谁是theirs。我希望标明具体的branch/cherry-pick信息。
- 在merge conflict信息中尽量支持中文。
- 信息保持简洁，function、branch、ours、theirs、base 等基础术语保留英文，不必逐一翻译；中文用于解释冲突原因和必要的操作上下文。
- 提示由确定性的实体分析、文本 diff、Git 上下文及固定模板生成，不推断业务意图或正确结果。启发式匹配应提供依据并标注不确定性；人工处理建议不代表程序已验证语义。
- 尽量给出基于entity的merge conflict信息，而不是基于行的。
