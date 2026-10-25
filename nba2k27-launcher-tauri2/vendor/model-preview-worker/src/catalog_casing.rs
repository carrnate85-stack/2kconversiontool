//! Frozen, app-owned WPF browse policies. Never used by asset resolution/export.
//! These tiny tables were generated through public .NET APIs by CasingOracle;
//! the application has no .NET dependency. See Data/WPF-CASING.txt.
const CHAR_UPPER: &[u8] = include_bytes!("../Data/wpf-char-upper.bin");
const ORDINAL_UPPER: &[u8] = include_bytes!("../Data/wpf-ordinal-upper.bin");

pub fn upper_unit(value: u16) -> u16 {
    if value < 128 {
        return (value as u8).to_ascii_uppercase() as u16;
    }
    let (mut lo, mut hi) = (0, CHAR_UPPER.len() / 4);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let p = mid * 4;
        let key = u16::from_le_bytes([CHAR_UPPER[p], CHAR_UPPER[p + 1]]);
        match key.cmp(&value) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => {
                return u16::from_le_bytes([CHAR_UPPER[p + 2], CHAR_UPPER[p + 3]]);
            }
        }
    }
    value
}
fn ordinal_upper(value: char) -> char {
    if value.is_ascii() {
        return value.to_ascii_uppercase();
    }
    let (mut lo, mut hi) = (0, ORDINAL_UPPER.len() / 8);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let p = mid * 8;
        let key = u32::from_le_bytes(ORDINAL_UPPER[p..p + 4].try_into().unwrap());
        match key.cmp(&(value as u32)) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => {
                return char::from_u32(u32::from_le_bytes(
                    ORDINAL_UPPER[p + 4..p + 8].try_into().unwrap(),
                ))
                .expect("Verified scalar casing table");
            }
        }
    }
    value
}
pub fn ordinal_fold(value: &str) -> String {
    if value.is_ascii() {
        return value.to_ascii_uppercase();
    }
    value.chars().map(ordinal_upper).collect()
}
pub fn ordinal_compare(a: &str, b: &str) -> std::cmp::Ordering {
    if a.is_ascii() && b.is_ascii() {
        return a
            .bytes()
            .map(|c| c.to_ascii_uppercase())
            .cmp(b.bytes().map(|c| c.to_ascii_uppercase()));
    }
    // The reference's ordinal-ignore-case comparer orders supplementary
    // scalars after BMP values. Its char-based natural comparer does not.
    ordinal_fold(a).chars().cmp(ordinal_fold(b).chars())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn generated_tables_are_sorted_scalar_valid_and_reference_pinned() {
        assert_eq!(
            format!("{:X}", Sha256::digest(CHAR_UPPER)),
            "F3FA361FB3342EC7C0B147B45EC0FFAD749B834E4C20074A7468AB80A105E9E2"
        );
        assert_eq!(
            format!("{:X}", Sha256::digest(ORDINAL_UPPER)),
            "97155F201CE0451B18428CBF93F12513C11FB8FECA02614417AC6EC8B21EC086"
        );
        assert_eq!(CHAR_UPPER.len(), 1189 * 4);
        assert_eq!(ORDINAL_UPPER.len(), 1448 * 8);
        let mut previous = None;
        for row in CHAR_UPPER.chunks_exact(4) {
            let key = u16::from_le_bytes(row[..2].try_into().unwrap());
            assert!(previous.is_none_or(|p| p < key));
            assert_eq!(
                upper_unit(key),
                u16::from_le_bytes(row[2..].try_into().unwrap())
            );
            previous = Some(key);
        }
        let mut previous = None;
        for row in ORDINAL_UPPER.chunks_exact(8) {
            let key = u32::from_le_bytes(row[..4].try_into().unwrap());
            let value = u32::from_le_bytes(row[4..].try_into().unwrap());
            assert!(previous.is_none_or(|p| p < key));
            assert_eq!(ordinal_upper(char::from_u32(key).unwrap()) as u32, value);
            previous = Some(key);
        }
    }
    #[test]
    fn ordinal_literals_do_not_merge_special_ascii_pairs_or_expand_letters() {
        assert_ne!(ordinal_fold("istanbul"), ordinal_fold("ıstanbul"));
        assert_ne!(ordinal_fold("sample"), ordinal_fold("ſample"));
        assert_ne!(ordinal_fold("ß"), ordinal_fold("ss"));
        assert_eq!(ordinal_fold("σςΣ"), "ΣΣΣ");
        assert_eq!(ordinal_fold("𐐨"), ordinal_fold("𐐀"));
        assert_eq!(ordinal_fold("ᾀ"), ordinal_fold("ᾈ"));
    }
    #[test]
    fn char_order_is_distinct_from_scalar_ordinal_casing() {
        assert_eq!(upper_unit(0xd801), 0xd801);
        assert_eq!(upper_unit(0xdc28), 0xdc28);
        assert_ne!(
            "𐐨".encode_utf16().map(upper_unit).collect::<Vec<_>>(),
            "𐐀".encode_utf16().map(upper_unit).collect::<Vec<_>>()
        );
        assert!(ordinal_compare("𐐨", "\u{e000}").is_gt());
    }
}
