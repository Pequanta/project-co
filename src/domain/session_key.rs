use rand::Rng;

/// Cryptographically secure, human-friendly session key.
///
/// Canonical form is 8 chars, no separators, from an alphabet that avoids
/// ambiguous characters (0/O, 1/I/L). Display form adds a dash: `AB7K-X92P`.
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const KEY_LEN: usize = 8;

/// Generate a new 8-char session key (canonical, no dash).
pub fn generate_session_key() -> String {
    let mut rng = rand::thread_rng();
    let mut out = String::with_capacity(KEY_LEN);
    for _ in 0..KEY_LEN {
        let idx = rng.gen_range(0..ALPHABET.len());
        out.push(ALPHABET[idx] as char);
    }
    out
}

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
    fn key_has_expected_shape_and_length() {
        for _ in 0..100 {
            let k = generate_session_key();
            assert_eq!(k.len(), 8);
            assert!(k
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
            assert!(!k.contains('O') && !k.contains('0') && !k.contains('I') && !k.contains('1'));
        }
    }

    #[test]
    fn keys_are_unique_in_practice() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..10_000 {
            assert!(seen.insert(generate_session_key()));
        }
    }

    #[test]
    fn normalize_and_format() {
        assert_eq!(normalize_key(" ab7k-x92p "), "AB7KX92P");
        assert_eq!(format_key("AB7KX92P"), "AB7K-X92P");
    }
}
