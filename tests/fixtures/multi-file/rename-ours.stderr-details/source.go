冲突 · source.go
ours   : feature/ours
base   : base-commit
theirs : feature/theirs
原因 [git]：LINE_CONFLICT：Git 行级冲突
  依据 [git]：Git 行级合并返回冲突
原因 [weave]：ENTITY_CONFLICT：function calculate 需人工审核
  依据 [weave]：拒绝：modify_delete；modified in theirs
原因 [analyze]：ENTITY_LAYOUT_CHANGED：实体增删或顺序变化
  依据 [analyze]：三方实体或非实体区域序列不同
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：function calculate 疑似重命名并移动（calculate → renamed），与另一侧变化需共同审核
  依据 [analyze]：RENAME_MOVE_CANDIDATE：ours 疑似重命名并移动 function calculate → renamed · source.go:1 → target.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，区域文本逐 byte 相同（含附着注释）；替换次数=1/1；sources=1，destinations=1；另一侧=modified；仅为文本候选，未证明语义等价
  依据 [analyze]：方法：复用 weave::binding::replace_at_word_boundaries，将各自名称替换为 __ENTITY__ 后直接比较原文；不使用相似度或 hash 判等
  依据 [analyze]：范围：整段 entity 文本；替换可能涉及定义、自引用、注释及字符串，不代表只修改定义名；未检查其它文件中的调用是否更新
  依据 [analyze]：歧义：sources 为此目标的候选来源数，destinations 为此来源的候选目标数（含同名移动）；全部保留，不选择唯一配对
需人工处理：选择一侧 / 编辑合并结果。

