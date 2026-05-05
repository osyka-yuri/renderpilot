pub(crate) fn normalize_pattern(pattern: &str) -> Option<String> {
    normalize_file_name(pattern)
}

pub(crate) fn normalize_file_name(file_name: &str) -> Option<String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return None;
    }

    trimmed
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests {
    use super::normalize_file_name;

    #[test]
    fn normalizes_case_and_extracts_file_name() {
        assert_eq!(
            normalize_file_name(r"C:\Games\Game\NVNGX_DLSS.DLL").as_deref(),
            Some("nvngx_dlss.dll")
        );
    }

    #[test]
    fn rejects_empty_input() {
        assert_eq!(normalize_file_name(" "), None);
    }
}
