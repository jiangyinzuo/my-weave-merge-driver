package invoice

func calculateTotals(price int, count int) int {
	subtotal := price * count
	return subtotal
}
