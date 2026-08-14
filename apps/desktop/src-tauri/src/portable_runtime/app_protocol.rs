//! Stable App/supervisor protocol DTOs and newline-delimited JSON framing.

pub mod dto;
pub mod framing;

pub use dto::*;
pub use framing::{read_message, read_message_or_eof, reader, write_message};
