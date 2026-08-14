//! Generated, presentation-free command error identifiers.
//!
//! This neutral module is deliberately below both the command boundary and the
//! diagnostics bridge, so a durable diagnostic can carry only the generated
//! error kind without creating a dependency cycle.

include!(concat!(env!("OUT_DIR"), "/desktop_error_kinds.rs"));

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{CommandErrorKind, CommandErrorSeverity};

    #[test]
    fn generated_error_codes_are_unique_and_valid() {
        let mut seen = HashSet::new();

        for &kind in CommandErrorKind::ALL {
            let code = kind.code();
            assert!(!code.is_empty());
            assert!(code.len() <= 64);
            assert!(code.chars().all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '_'));
            assert!(seen.insert(code), "duplicate command error code: {code}");
        }
    }

    #[test]
    fn generated_severity_matches_the_manifest() {
        assert_eq!(
            CommandErrorKind::ConfirmationTokenMismatch.severity(),
            CommandErrorSeverity::Warning
        );
        assert_eq!(
            CommandErrorKind::StorageFailed.severity(),
            CommandErrorSeverity::Error
        );
    }
}
