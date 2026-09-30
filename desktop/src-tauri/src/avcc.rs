//! Pure AVCC (length-prefixed H.264) parsing shared by platform encoders.

/// Splits complete AVCC NAL units. A truncated tail or empty NAL is ignored.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn split_avcc(data: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut i = 0;
    while i + 4 <= data.len() {
        let n = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        if n == 0 {
            i += 4;
            continue;
        }
        let Some(nal) = data.get(i + 4..i + 4 + n) else {
            break;
        };
        nals.push(nal);
        i += 4 + n;
    }
    nals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_complete_nals_and_ignores_malformed_tail() {
        let data = [0, 0, 0, 2, 0x65, 0xaa, 0, 0, 0, 1, 0x41, 0, 0, 0, 9];
        assert_eq!(split_avcc(&data), vec![&[0x65, 0xaa][..], &[0x41][..]]);
    }

    #[test]
    fn skips_empty_nals_and_truncated_prefixes_without_panicking() {
        assert!(split_avcc(&[0, 0, 0, 0, 0, 0, 0]).is_empty());
        assert!(split_avcc(&[0, 0, 0]).is_empty());
        assert!(split_avcc(&[0, 0, 0, 3, 0x65]).is_empty());
    }
}
