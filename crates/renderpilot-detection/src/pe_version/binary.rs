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
        value.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }

    while value.last() == Some(&0) {
        value.pop();
    }

    String::from_utf16(&value).ok()
}

pub(super) fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let value = checked_range(bytes, offset, 2)?;
    Some(u16::from_le_bytes([value[0], value[1]]))
}

pub(super) fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let value = checked_range(bytes, offset, 4)?;
    Some(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

pub(super) fn checked_range(bytes: &[u8], offset: usize, len: usize) -> Option<&[u8]> {
    bytes.get(offset..offset.checked_add(len)?)
}

pub(super) fn align4(offset: usize) -> Option<usize> {
    offset.checked_add(3).map(|value| value & !3)
}