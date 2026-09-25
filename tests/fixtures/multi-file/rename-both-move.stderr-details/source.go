冲突 · source.go
feature/ours
base-commit
feature/theirs
原因 [weave]：ENTITY_CONFLICT：ƒ calculate
  依据 [weave]：分类：ours=deleted, theirs=deleted
原因 [analyze]：ENTITY_LAYOUT_CHANGED：实体增删或顺序变化
  依据 [analyze]：三方实体或非实体区域序列不同
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculate 疑似移动与另一侧变化冲突
  依据 [analyze]：MOVE_CANDIDATE：ours 疑似移动 ƒ calculate · source.go:1 → ours/moved.go:1；源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；sources=1，destinations=1；另一侧=deleted（theirs：疑似重命名并移动 ƒ calculate · source.go:1 → ƒ renamed · theirs/first.go:1；疑似重命名并移动 ƒ calculate · source.go:1 → ƒ renamed · theirs/second.go:1；候选数=2）
  依据 另一侧候选 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculate → ƒ renamed · source.go:1 → theirs/first.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，忽略 token 间空白后语法结构及有序 token 完全相同（9 个 token，保留注释和字面量原文）；原文存在格式差异；替换次数=1/1；sources=1，destinations=2；另一侧=deleted；仅为文本候选，未证明语义等价
  依据 另一侧候选 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculate → ƒ renamed · source.go:1 → theirs/second.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，区域文本逐 byte 相同（含附着注释）；替换次数=1/1；sources=1，destinations=2；另一侧=deleted；仅为文本候选，未证明语义等价
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculate 疑似重命名并移动（calculate → renamed），与另一侧变化冲突
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculate → ƒ renamed · source.go:1 → theirs/first.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，忽略 token 间空白后语法结构及有序 token 完全相同（9 个 token，保留注释和字面量原文）；原文存在格式差异；替换次数=1/1；sources=1，destinations=2；另一侧=deleted（ours：疑似移动 ƒ calculate · source.go:1 → ƒ calculate · ours/moved.go:1；候选数=1）；仅为文本候选，未证明语义等价
  依据 另一侧候选 [analyze]：MOVE_CANDIDATE：ours 疑似移动 ƒ calculate · source.go:1 → ours/moved.go:1；源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；sources=1，destinations=1；另一侧=deleted
  依据 [analyze]：方法：复用 weave::binding::replace_at_word_boundaries 和 sem-core::parse_tree，比较名称归一化后的完整语法结构及 token 序列；相同才匹配，不使用模糊阈值或 hash 判等
  依据 [analyze]：格式：只忽略语法 token 间的空白；保留节点类型、字段、顺序、嵌套及注释/字面量原文，不把缩进造成的结构变化当作格式化；不是语义等价证明
  依据 [analyze]：范围：整段 entity 文本；替换可能涉及定义、自引用、注释及字符串，不代表只修改定义名；未检查其它文件中的调用是否更新
  依据 [analyze]：歧义：sources 为此目标的候选来源数，destinations 为此来源的候选目标数（含同名移动）；全部保留，不选择唯一配对
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculate 疑似重命名并移动（calculate → renamed），与另一侧变化冲突
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculate → ƒ renamed · source.go:1 → theirs/second.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，区域文本逐 byte 相同（含附着注释）；替换次数=1/1；sources=1，destinations=2；另一侧=deleted（ours：疑似移动 ƒ calculate · source.go:1 → ƒ calculate · ours/moved.go:1；候选数=1）；仅为文本候选，未证明语义等价
  依据 另一侧候选 [analyze]：MOVE_CANDIDATE：ours 疑似移动 ƒ calculate · source.go:1 → ours/moved.go:1；源 deleted、目标 added；类型/名称/原始区域文本相同（含附着注释）；sources=1，destinations=1；另一侧=deleted
  依据 [analyze]：方法：复用 weave::binding::replace_at_word_boundaries，将各自名称替换为 __ENTITY__ 后直接比较原文；不使用相似度或 hash 判等
  依据 [analyze]：范围：整段 entity 文本；替换可能涉及定义、自引用、注释及字符串，不代表只修改定义名；未检查其它文件中的调用是否更新
  依据 [analyze]：歧义：sources 为此目标的候选来源数，destinations 为此来源的候选目标数（含同名移动）；全部保留，不选择唯一配对
