冲突 · billing/prices.go
feature/ours
base-commit
feature/theirs
原因 [analyze]：GLOBAL_MODIFY_VS_MOVE：ƒ calculateTotal 疑似重命名并移动（calculateTotal → calculateTotals），与另一侧变化冲突
  依据 [analyze]：RENAME_MOVE_CANDIDATE：theirs 疑似重命名并移动 ƒ calculateTotal → ƒ calculateTotals · legacy/pricing.go:3 → billing/prices.go:3；源 deleted、目标 added；grammar=go、类型相同；按词边界替换各自名称后，忽略 token 间空白后语法结构及有序 token 完全相同（19 个 token，保留注释和字面量原文）；原文存在格式差异；替换次数=1/1；sources=1，destinations=1；另一侧=modified；仅为文本候选，未证明语义等价
