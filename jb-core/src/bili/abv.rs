// 代码来自 https://github.com/Colerar/abv/, 去掉了所有 unsafe blocks

const XOR_CODE: u64 = 23442827791579;
const MASK_CODE: u64 = 2251799813685247;

pub const MAX_AID: u64 = 1 << 51;
pub const MIN_AID: u64 = 1;

const BASE: u64 = 58;
const BV_LEN: usize = 12;
const PREFIX: &str = "BV1";

const ALPHABET: [u8; BASE as usize] =
    *b"FcwAPNKTMug3GV5Lj7EJnHpWsx4tb8haYeviqBz6rkCy12mUSDQX9RdoZf";

#[rustfmt::skip]
fn rev(value: u8) -> Option<u8> {
    use std::option::Option::Some as S;
    match value {
        b'F' => S(0),  b'c' => S(1),  b'w' => S(2),  b'A' => S(3),  b'P' => S(4),  b'N' => S(5),  b'K' => S(6),  b'T' => S(7),  b'M' => S(8),
        b'u' => S(9),  b'g' => S(10), b'3' => S(11), b'G' => S(12), b'V' => S(13), b'5' => S(14), b'L' => S(15), b'j' => S(16), b'7' => S(17),
        b'E' => S(18), b'J' => S(19), b'n' => S(20), b'H' => S(21), b'p' => S(22), b'W' => S(23), b's' => S(24), b'x' => S(25), b'4' => S(26),
        b't' => S(27), b'b' => S(28), b'8' => S(29), b'h' => S(30), b'a' => S(31), b'Y' => S(32), b'e' => S(33), b'v' => S(34), b'i' => S(35),
        b'q' => S(36), b'B' => S(37), b'z' => S(38), b'6' => S(39), b'r' => S(40), b'k' => S(41), b'C' => S(42), b'y' => S(43), b'1' => S(44),
        b'2' => S(45), b'm' => S(46), b'U' => S(47), b'S' => S(48), b'D' => S(49), b'Q' => S(50), b'X' => S(51), b'9' => S(52), b'R' => S(53),
        b'd' => S(54), b'o' => S(55), b'Z' => S(56), b'f' => S(57),
        _ => None
    }
}

pub fn av2bv(avid: u64) -> Option<String> {
    if avid < MIN_AID {
        return None;
    }
    if avid >= MAX_AID {
        return None;
    }

    let mut bytes: [u8; BV_LEN] = *b"BV1000000000";

    let mut bv_idx = BV_LEN - 1;
    let mut tmp = (MAX_AID | avid) ^ XOR_CODE;
    while tmp != 0 {
        let table_idx = tmp % BASE;
        // SAFETY: a positive number mod 58 is in 0..58
        bytes[bv_idx] = ALPHABET[table_idx as usize];
        tmp /= BASE;
        bv_idx -= 1;
    }

    // SAFETY, 3 < 4 < 7 < 9 < BV_LEN
    bytes.swap(3, 9);
    bytes.swap(4, 7);

    // SAFETY: bytes represent an ASCII string
    let str = String::from_utf8(bytes.to_vec()).unwrap();

    Some(str)
}

pub fn bv2av(bvid: &str) -> Option<u64> {
    if bvid.is_empty() || !bvid.is_ascii() {
        return None;
    }

    if bvid.len() != BV_LEN {
        return None;
    }

    // SAFETY: Already checked before
    if !bvid[0..3].eq_ignore_ascii_case(PREFIX) {
        return None;
    }

    let mut bytes = bvid.as_bytes().to_vec();

    bytes.swap(3, 9);
    bytes.swap(4, 7);

    let mut tmp: u64 = 0;

    for byte in &bytes[3..] {
        let idx = rev(*byte)?;
        tmp = tmp * BASE + idx as u64;
    }

    // Equivalence of: format!("{:b}", tmp).size()
    let bin_len = if tmp == 0 {
        0
    } else {
        u64::BITS - tmp.leading_zeros()
    };

    if bin_len != 52 {
        return None;
    }

    let avid = (tmp & MASK_CODE) ^ XOR_CODE;

    if avid < MIN_AID {
        return None;
    }

    Some(avid)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: [(u64, &str); 7] = [
        (117101125764901, "BV1Akb16VEhm"),
        (117216435510550, "BV1c7t16iEYe"),
        (117227810457493, "BV1E3bw6yEnp"),
        (117195178841241, "BV1p5t366EuM"),
        (115441708044072, "BV1qmsrzZENe"),
        (117218431996158, "BV1sbbj6bEut"),
        (117223330942671, "BV1zKbp6BEgj"),
    ];

    #[test]
    fn test_abv() {
        for (av, bv) in IDS {
            assert_eq!(&av2bv(av).unwrap(), bv);
            assert_eq!(bv2av(bv).unwrap(), av);
        }
    }
}
