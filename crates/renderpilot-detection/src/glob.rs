pub(crate) fn glob_matches(pattern: &str, file_name: &str) -> bool {
    let pattern = pattern.as_bytes();
    let file_name = file_name.as_bytes();
    let (mut pattern_index, mut file_index) = (0, 0);
    let mut star_index = None;
    let mut star_file_index = 0;

    while file_index < file_name.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == file_name[file_index])
        {
            pattern_index += 1;
            file_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_file_index = file_index;
        } else if let Some(index) = star_index {
            pattern_index = index + 1;
            star_file_index += 1;
            file_index = star_file_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::glob_matches;

    #[test]
    fn supports_star_and_question_mark() {
        assert!(glob_matches("sl.*.dll", "sl.interposer.dll"));
        assert!(glob_matches("a?c*.dll", "abc123.dll"));
        assert!(!glob_matches("a?c.dll", "ac.dll"));
    }
}
