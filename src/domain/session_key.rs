//! Session keys are user-chosen, 8 alphanumeric characters. Canonical form is
//! uppercase with no separators; display form adds a dash: `AB7K-X92P`.

/// Normalize user input: strip separators/whitespace, uppercase.
/// `"ab7k-x92p"` -> `"AB7KX92P"`.
pub fn normalize_key(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

/// Display form with a dash: `"AB7KX92P"` -> `"AB7K-X92P"`.
pub fn format_key(key: &str) -> String {
    match key.len() {
        8 => format!("{}-{}", &key[0..4], &key[4..8]),
        _ => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_and_format() {
        assert_eq!(normalize_key(" ab7k-x92p "), "AB7KX92P");
        assert_eq!(format_key("AB7KX92P"), "AB7K-X92P");
    }
}
