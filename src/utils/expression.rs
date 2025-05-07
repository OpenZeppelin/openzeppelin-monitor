/// Splits an expression into a tuple of (left, operator, right)
///
/// # Arguments
/// * `expr` - The expression to split
///
/// # Returns
/// An Option containing the split expression if successful, None otherwise
pub fn split_expression(expr: &str) -> Option<(&str, &str, &str)> {
	// Find the operator position while respecting quotes
	let mut in_quotes = false;
	let mut operator_start = None;
	let mut operator_end = None;

	let operators = [
		"==",
		"!=",
		">=",
		"<=",
		">",
		"<",
		"contains",
		"starts_with",
		"ends_with",
	];

	// First pass - find operator position
	for (i, c) in expr.char_indices() {
		if c == '\'' || c == '"' {
			in_quotes = !in_quotes;
			continue;
		}

		if !in_quotes {
			// Check each operator
			for op in operators {
				if expr[i..].starts_with(op) {
					operator_start = Some(i);
					operator_end = Some(i + op.len());
					break;
				}
			}
			if operator_start.is_some() {
				break;
			}
		}
	}

	// Split based on operator position
	if let (Some(op_start), Some(op_end)) = (operator_start, operator_end) {
		let left = expr[..op_start].trim();
		let operator = expr[op_start..op_end].trim();
		let right = expr[op_end..].trim();

		// Remove surrounding quotes from right side if present
		let right = right.trim_matches(|c| c == '\'' || c == '"');

		Some((left, operator, right))
	} else {
		None
	}
}
