use std::fmt;

pub type Result<T> = std::result::Result<T, PortableRuntimeError>;

#[derive(Debug)]
pub struct PortableRuntimeError {
    code: &'static str,
    detail: String,
}

impl PortableRuntimeError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for PortableRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for PortableRuntimeError {}

impl From<std::io::Error> for PortableRuntimeError {
    fn from(error: std::io::Error) -> Self {
        Self::new("portable_runtime_io", error.to_string())
    }
}
