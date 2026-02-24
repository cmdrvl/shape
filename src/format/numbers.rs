pub fn format_count(value: u64) -> String {
    let s = value.to_string();
    let mut out = String::with_capacity(s.len() + (s.len() / 3));
    for (idx, ch) in s.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

pub fn format_ratio_as_percent(ratio: f64) -> String {
    format!("{}%", (ratio * 100.0).round() as i64)
}

pub fn format_coverage(coverage: f64) -> String {
    let mut rendered = format!("{coverage:.2}");
    while rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.push('0');
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::{format_count, format_coverage, format_ratio_as_percent};

    #[test]
    fn format_count_adds_thousands_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(12), "12");
        assert_eq!(format_count(3_214), "3,214");
        assert_eq!(format_count(1_234_567), "1,234,567");
    }

    #[test]
    fn format_ratio_rounds_to_nearest_integer_percent() {
        assert_eq!(format_ratio_as_percent(1.0), "100%");
        assert_eq!(format_ratio_as_percent(0.88235), "88%");
        assert_eq!(format_ratio_as_percent(0.885), "89%");
    }

    #[test]
    fn format_coverage_emits_one_or_two_decimals() {
        assert_eq!(format_coverage(1.0), "1.0");
        assert_eq!(format_coverage(0.85), "0.85");
        assert_eq!(format_coverage(0.9), "0.9");
        assert_eq!(format_coverage(0.0), "0.0");
    }
}
