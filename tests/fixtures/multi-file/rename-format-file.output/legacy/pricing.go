<<<<<<< ours: feature/ours | LINE_CONFLICT：Git 行级冲突; ENTITY_CONFLICT：function calculateTotal 需人工审核; ENTITY_LAYOUT_CHANGED：实体增删或顺序变化
package invoice

func calculateTotal(price int,count int) int {
    subtotal:=price*count
    return subtotal-5
}
||||||| base: base-commit
package invoice

func calculateTotal(price int,count int) int {
    subtotal:=price*count
    return subtotal
}
=======
>>>>>>> theirs: feature/theirs | 文件关联 [analyze]：疑似重命名并移动 function calculateTotal · legacy/pricing.go:3 → function calculateTotals · billing/prices.go:3
