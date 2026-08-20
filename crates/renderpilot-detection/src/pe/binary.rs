pub(super) fn read_utf16_null_terminated(
    bytes: &[u8],
    offset: usize,
    limit: usize,
) -> Option<(String, usize)> {
    let mut cursor = offset;

    while cursor.checked_add(2)? <= limit {
        let unit = read_u16(bytes, cursor)?;
        cursor = cursor.checked_add(2)?;

        if unit == 0 {
            return bytes
                .get(offset..cursor - 2)
                .and_then(|value| String::from_utf16le(value).ok())
                .map(|text| (text, cursor));
        }
    }

    None
}

pub(super) fn read_utf16_value(bytes: &[u8], offset: usize, units: usize) -> Option<String> {
    let raw = checked_range(bytes, offset, units.checked_mul(2)?)?;
    let mut end = raw.len();
    while end >= 2 && raw[end - 2..end] == [0, 0] {
        end -= 2;
    }
    String::from_utf16le(&raw[..end]).ok()
}

pub(super) fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let value: &[u8; 2] = checked_range(bytes, offset, 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(*value))
}

pub(super) fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let value: &[u8; 4] = checked_range(bytes, offset, 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(*value))
}

pub(super) fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let value: &[u8; 8] = checked_range(bytes, offset, 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(*value))
}

pub(super) fn checked_range(bytes: &[u8], offset: usize, len: usize) -> Option<&[u8]> {
    bytes.get(offset..offset.checked_add(len)?)
}

pub(super) fn align4(offset: usize) -> Option<usize> {
    offset.checked_add(3).map(|value| value & !3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_u16_le_at_exact_end() {
        let bytes = [0x34, 0x12];
        assert_eq!(read_u16(&bytes, 0), Some(0x1234));
    }

    #[test]
    fn read_u16_rejects_one_byte_short() {
        let bytes = [0x34];
        assert_eq!(read_u16(&bytes, 0), None);
    }

    #[test]
    fn read_u32_le_and_rejects_short_buffer() {
        let bytes = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(read_u32(&bytes, 0), Some(0x1234_5678));
        assert_eq!(read_u32(&bytes[..3], 0), None);
    }

    #[test]
    fn read_at_offset_and_overflow_returns_none() {
        let bytes = [0, 1, 2, 3, 4, 5];
        assert_eq!(read_u16(&bytes, 4), Some(0x0504));
        assert_eq!(read_u16(&bytes, 5), None);
        assert_eq!(read_u32(&bytes, 3), None);
        // offset + width overflow / past end
        assert_eq!(read_u16(&bytes, usize::MAX), None);
    }

    #[test]
    fn unaligned_offset_is_allowed() {
        let bytes = [0x00, 0x78, 0x56, 0x34, 0x12];
        assert_eq!(read_u32(&bytes, 1), Some(0x1234_5678));
    }

    #[test]
    fn read_utf16_value_at_mid_location() {
        // "AB" encoded as UTF-16 LE (4 bytes), null-terminated.
        let data = [b'A', 0x00, b'B', 0x00, 0x00, 0x00];
        assert_eq!(read_utf16_value(&data, 0, 3), Some(String::from("AB")));
    }

    #[test]
    fn read_utf16_null_terminated_respects_bound_and_returns_cursor_after_terminator() {
        let data = [b'A', 0x00, b'B', 0x00, 0x00, 0x00, b'X', 0x00];
        assert_eq!(
            read_utf16_null_terminated(&data, 0, 6),
            Some((String::from("AB"), 6))
        );
    }

    #[test]
    fn read_utf16_null_terminated_rejects_terminator_outside_bound() {
        let data = [b'A', 0x00, 0x00, 0x00];
        assert_eq!(read_utf16_null_terminated(&data, 0, 2), None);
    }

    #[test]
    fn utf16_readers_reject_lone_surrogates() {
        let null_terminated = [0x00, 0xD8, 0x00, 0x00];
        assert_eq!(
            read_utf16_null_terminated(&null_terminated, 0, null_terminated.len()),
            None
        );

        let fixed = [0x00, 0xD8];
        assert_eq!(read_utf16_value(&fixed, 0, 1), None);
    }

    #[test]
    fn read_utf16_value_rejects_short_buffer() {
        let data = [b'A', 0x00];
        // 2 units need 4 bytes but we give only 2.
        assert_eq!(read_utf16_value(&data, 0, 2), None);
    }
}
