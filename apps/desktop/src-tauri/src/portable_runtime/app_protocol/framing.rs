use std::{
    collections::HashSet,
    fmt,
    io::{BufRead, BufReader, Read, Write},
};

use serde::{
    Deserialize, Serialize,
    de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor},
};

use crate::portable_runtime::error::{PortableRuntimeError, Result};

pub const MAX_FRAME_BYTES: usize = 64 * 1024;
const FRAME_READ_LIMIT: usize = MAX_FRAME_BYTES + 1;

struct DuplicateKeyPreflight;

impl<'de> DeserializeSeed<'de> for DuplicateKeyPreflight {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(DuplicateKeyVisitor)
    }
}

struct DuplicateKeyVisitor;

impl<'de> Visitor<'de> for DuplicateKeyVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, _: bool) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_borrowed_str<E>(self, _: &'de str) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_string<E>(self, _: String) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        DuplicateKeyPreflight.deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element_seed(DuplicateKeyPreflight)?.is_some() {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut fields = HashSet::new();
        while let Some(field) = map.next_key::<String>()? {
            if !fields.insert(field) {
                return Err(A::Error::custom("JSON object contains a duplicate key"));
            }
            map.next_value_seed(DuplicateKeyPreflight)?;
        }
        Ok(())
    }
}

fn preflight_json_without_duplicate_keys(
    frame: &[u8],
) -> std::result::Result<(), serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(frame);
    DuplicateKeyPreflight.deserialize(&mut deserializer)?;
    deserializer.end()
}

pub fn write_message(writer: &mut impl Write, message: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *writer, message).map_err(|error| {
        PortableRuntimeError::new("portable_protocol_encode", error.to_string())
    })?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

pub fn read_message_or_eof<T: for<'de> Deserialize<'de>>(
    reader: &mut impl BufRead,
) -> Result<Option<T>> {
    let mut frame = Vec::with_capacity(FRAME_READ_LIMIT);
    let bytes_read = reader
        .take(FRAME_READ_LIMIT as u64)
        .read_until(b'\n', &mut frame)?;
    if bytes_read == 0 {
        return Ok(None);
    }
    if frame.len() > MAX_FRAME_BYTES || frame.last() != Some(&b'\n') {
        return Err(PortableRuntimeError::new(
            "portable_protocol_invalid",
            "oversized or unterminated protocol message",
        ));
    }
    preflight_json_without_duplicate_keys(&frame).map_err(|error| {
        PortableRuntimeError::new("portable_protocol_invalid", error.to_string())
    })?;
    serde_json::from_slice(&frame)
        .map(Some)
        .map_err(|error| PortableRuntimeError::new("portable_protocol_invalid", error.to_string()))
}

pub fn read_message<T: for<'de> Deserialize<'de>>(reader: &mut impl BufRead) -> Result<T> {
    read_message_or_eof(reader)?.ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_protocol_closed",
            "protocol channel closed before the required message",
        )
    })
}

pub fn reader(file: std::fs::File) -> BufReader<std::fs::File> {
    BufReader::new(file)
}
