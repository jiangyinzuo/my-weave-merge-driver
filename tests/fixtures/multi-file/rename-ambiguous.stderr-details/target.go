冲突 · target.go
feature/ours
base-commit
feature/theirs
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculate 疑似重命名并移动（calculate → renamed），与另一侧变化冲突
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculate → ƒ renamed · source.go:1 → target.go:1；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，区域文本逐 byte 相同（含附着注释）；替换次数=1/1；sources=1，destinations=3；另一侧=modified；仅为文本候选，未证明语义等价
  依据 [analyze]：方法：复用 weave::binding::replace_at_word_boundaries，将各自名称替换为 __ENTITY__ 后直接比较原文；不使用相似度或 hash 判等
  依据 [analyze]：范围：整段 entity 文本；替换可能涉及定义、自引用、注释及字符串，不代表只修改定义名；未检查其它文件中的调用是否更新
  依据 [analyze]：歧义：sources 为此目标的候选来源数，destinations 为此来源的候选目标数（含同名移动）；全部保留，不选择唯一配对
