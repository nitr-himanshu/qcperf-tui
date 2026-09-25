pub fn is_percent(unit: &str) -> bool {
    let unit = unit.trim();
    unit == "%" || unit.to_ascii_lowercase().contains("percent")
}

pub fn is_frequency(unit: &str) -> bool {
    let unit = unit.trim();
    unit.eq_ignore_ascii_case("mhz") || unit.eq_ignore_ascii_case("hz")
}
