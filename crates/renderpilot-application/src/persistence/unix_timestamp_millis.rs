use crate::{AppError, AppResult};

/// Unix timestamp in milliseconds.
///
/// Used for persisted journal and backup records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixTimestampMillis(i64);

impl UnixTimestampMillis {
    /// Creates a non-negative Unix timestamp in milliseconds.
    pub fn new(value: i64) -> AppResult<Self> {
        if value < 0 {
            return Err(AppError::invalid_input(
                "unix timestamp in milliseconds must not be negative",
            ));
        }

        Ok(Self(value))
    }

    /// Returns the raw timestamp value.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for UnixTimestampMillis {
    type Error = AppError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<UnixTimestampMillis> for i64 {
    fn from(value: UnixTimestampMillis) -> Self {
        value.as_i64()
    }
}

#[cfg(test)]
mod tests {
    use crate::{AppErrorKind, UnixTimestampMillis};

    #[test]
    fn accepts_non_negative_timestamp() {
        let timestamp = UnixTimestampMillis::new(1_700_000_000_000).unwrap();

        assert_eq!(timestamp.as_i64(), 1_700_000_000_000);
    }

    #[test]
    fn rejects_negative_timestamp() {
        let error = UnixTimestampMillis::new(-1).unwrap_err();

        assert_eq!(error.kind(), AppErrorKind::InvalidInput);
    }
}
