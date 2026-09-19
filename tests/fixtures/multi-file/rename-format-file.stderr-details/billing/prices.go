冲突 · billing/prices.go
ours   : feature/ours
base   : base-commit
theirs : feature/theirs
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：function calculateTotal 疑似重命名并移动（calculateTotal → calculateTotals），与另一侧变化需共同审核
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 function calculateTotal → calculateTotals · legacy/pricing.go:3 → billing/prices.go:3；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，忽略 token 间空白后语法结构及有序 token 完全相同（19 个 token，保留注释和字面量原文）；原文存在格式差异；替换次数=1/1；sources=1，destinations=1；另一侧=modified；仅为文本候选，未证明语义等价
  依据 [analyze]：方法：复用 weave::binding::replace_at_word_boundaries 和 sem-core::parse_tree，比较名称归一化后的完整语法结构及 token 序列；相同才匹配，不使用模糊阈值或 hash 判等
  依据 [analyze]：格式：只忽略语法 token 间的空白；保留节点类型、字段、顺序、嵌套及注释/字面量原文，不把缩进造成的结构变化当作格式化；不是语义等价证明
  依据 [analyze]：范围：整段 entity 文本；替换可能涉及定义、自引用、注释及字符串，不代表只修改定义名；未检查其它文件中的调用是否更新
  依据 [analyze]：歧义：sources 为此目标的候选来源数，destinations 为此来源的候选目标数（含同名移动）；全部保留，不选择唯一配对
需人工处理：选择一侧 / 编辑合并结果。

