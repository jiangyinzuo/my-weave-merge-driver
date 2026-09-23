<<<<<<< ⎇ feature/ours | LINE_CONFLICT：Git 行级冲突; ENTITY_CONFLICT：ƒ calculateTotal 需人工审核; ENTITY_LAYOUT_CHANGED：实体增删或顺序变化
package invoice

func calculateTotal(price int,count int) int {
    subtotal:=price*count
    return subtotal-5
}
||||||| base-commit
package invoice

func calculateTotal(price int,count int) int {
    subtotal:=price*count
    return subtotal
}
=======
>>>>>>> ⎇ feature/theirs | 文件关联 [analyze]：疑似重命名并移动 ƒ calculateTotal · legacy/pricing.go:3 → ƒ calculateTotals · billing/prices.go:3
