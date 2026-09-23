冲突 · new.py
⎇ feature/ours
base-commit
⎇ feature/theirs
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculate 疑似重命名并移动（calculate → calculates），与另一侧变化需共同审核
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculate → ƒ calculates · old.py:1 → new.py:1；源 deleted、目标 added；grammar=python、类型相同；按词边界替换各自名称后，忽略 token 间空白后语法结构及有序 token 完全相同（15 个 token，保留注释和字面量原文）；原文存在格式差异；替换次数=1/1；sources=1，destinations=1；另一侧=modified；仅为文本候选，未证明语义等价
需人工处理：选择一侧 / 编辑合并结果。

