pub(super) fn read_utf16_null_terminated(
    bytes: &[u8],
    offset: usize,
    limit: usize,
) -> Option<(String, usize)> {
    let mut cursor = offset;
    let mut value = Vec::new();

    while cursor.checked_add(2)? <= limit {
        let unit = read_u16(bytes, cursor)?;
        cursor = cursor.checked_add(2)?;

        if unit == 0 {
            return String::from_utf16(&value).ok().map(|text| (text, cursor));
        }

        value.push(unit);
    }

    None
}

pub(super) fn read_utf16_value(bytes: &[u8], offset: usize, units: usize) -> Option<String> {
    let raw = checked_range(bytes, offset, units.checked_mul(2)?)?;
    let mut value = Vec::with_capacity(units);

    for chunk in raw.chunks_exact(2) {
        let unit: [u8; 2] = chunk.try_into().ok()?;
        value.push(u16::from_le_bytes(unit));
    }

    while value.last() == Some(&0) {
        value.pop();
    }

    String::from_utf16(&value).ok()
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
    fn read_utf16_value_rejects_short_buffer() {
        let data = [b'A', 0x00];
        // 2 units need 4 bytes but we give only 2.
        assert_eq!(read_utf16_value(&data, 0, 2), None);
    }
}
