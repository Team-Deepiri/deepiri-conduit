/// Sanitize a name for `docker compose -p` (project name).
pub fn sanitize_compose_project(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_lowercase();
    if s.is_empty() {
        "conduit-app".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize() {
        assert_eq!(sanitize_compose_project("MyApp"), "myapp");
        assert_eq!(
            sanitize_compose_project("deepiri-platform"),
            "deepiri-platform"
        );
        assert_eq!(sanitize_compose_project("a..b"), "a--b");
    }
}
