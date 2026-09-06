/// Check if a string is a valid identifier or path (`Foo::bar`).
/// Each `::`-separated segment starts with a letter or underscore and contains
/// only letters, digits, and underscores. A lone `:` is not allowed.
pub fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.split("::").all(is_identifier_segment)
}

fn is_identifier_segment(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();

    // First character must be letter or underscore
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {},
        _ => return false,
    }

    // Remaining characters must be alphanumeric or underscore
    chars.all(|c| c.is_alphanumeric() || c == '_')
}
