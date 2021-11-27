use crate::{constants::*, Tags};

macro_rules! check {
    ($e:expr) => {
        if !($e) {
            return None;
        }
    };
    ($e:expr, $v:expr) => {
        if !($e) {
            return None;
        } else {
            $v
        }
    };
}

/// Low-level representation of major type 7 values.
///
/// Bool, null, and undefined are represented by L0 while L2–L4 represent the underlying
/// bytes of floating-point numbers (16-, 32-, and 64-bit IEE754).
pub enum Literal {
    L0(u8),
    L1(u8),
    L2(u16),
    L4(u32),
    L8(u64),
}

#[inline]
pub(crate) fn major(bytes: &[u8]) -> Option<u8> {
    Some(*bytes.get(0)? >> 5)
}

pub(crate) fn careful_literal(bytes: &[u8]) -> Option<(Literal, &[u8])> {
    let (int, b, rest) = integer(bytes)?;
    match b.len() {
        1 => Some((Literal::L0(int as u8), rest)),
        2 => Some((Literal::L1(int as u8), rest)),
        3 => Some((Literal::L2(int as u16), rest)),
        5 => Some((Literal::L4(int as u32), rest)),
        9 => Some((Literal::L8(int as u64), rest)),
        _ => None,
    }
}

pub(crate) fn integer(bytes: &[u8]) -> Option<(u64, &[u8], &[u8])> {
    match bytes[0] & 31 {
        // fun fact: explicit bounds checks make the code a lot smaller and faster because
        // otherwise the panic’s line number dictates a separate check for each array access
        24 => check!(
            bytes.len() > 1,
            Some((bytes[1] as u64, &bytes[..2], &bytes[2..]))
        ),
        25 => check!(
            bytes.len() > 2,
            Some((
                ((bytes[1] as u64) << 8) | (bytes[2] as u64),
                &bytes[..3],
                &bytes[3..]
            ))
        ),
        26 => check!(
            bytes.len() > 4,
            Some((
                // fun fact: these expressions compile down to mov-shl-bswap
                ((bytes[1] as u64) << 24)
                    | ((bytes[2] as u64) << 16)
                    | ((bytes[3] as u64) << 8)
                    | (bytes[4] as u64),
                &bytes[..5],
                &bytes[5..],
            ))
        ),
        27 => check!(
            bytes.len() > 8,
            Some((
                ((bytes[1] as u64) << 56)
                    | ((bytes[2] as u64) << 48)
                    | ((bytes[3] as u64) << 40)
                    | ((bytes[4] as u64) << 32)
                    | ((bytes[5] as u64) << 24)
                    | ((bytes[6] as u64) << 16)
                    | ((bytes[7] as u64) << 8)
                    | (bytes[8] as u64),
                &bytes[..9],
                &bytes[9..],
            ))
        ),
        x if x < 24 => Some(((x as u64), &bytes[..1], &bytes[1..])),
        _ => None,
    }
}

// inline to reuse the bounds check already made by the caller
#[inline(always)]
pub(crate) fn indefinite(bytes: &[u8]) -> Option<(u64, &[u8], &[u8])> {
    if bytes[0] & 31 == INDEFINITE_SIZE {
        // since an item takes at least 1 byte, u64::MAX is an impossible size
        Some((u64::MAX, &bytes[..1], &bytes[1..]))
    } else {
        None
    }
}

pub(crate) fn float(bytes: &[u8]) -> Option<(f64, &[u8], &[u8])> {
    integer(bytes).and_then(|(x, b, rest)| match b.len() {
        3 => Some((half::f16::from_bits(x as u16).to_f64(), b, rest)),
        5 => Some((f32::from_bits(x as u32) as f64, b, rest)),
        9 => Some((f64::from_bits(x), b, rest)),
        _ => None,
    })
}

pub(crate) fn tags(bytes: &[u8]) -> Option<(Tags, &[u8])> {
    let mut remaining = bytes;
    while let Some(value) = remaining.get(0) {
        if (*value >> 5) != MAJOR_TAG {
            break;
        }
        let (_, _, r) = integer(remaining)?;
        remaining = r;
    }
    let len = bytes.len() - remaining.len();
    Some((Tags::new(&bytes[..len]), remaining))
}

#[cfg(test)]
mod tests {
    use crate::{index_str, Cbor, CborOwned, ItemKind};
    use serde_json::json;

    fn sample() -> CborOwned {
        CborOwned::canonical(
            serde_cbor::to_vec(&json!({
                "a": {
                    "b": 12
                },
                "c": null
            }))
            .unwrap(),
            false,
        )
        .unwrap()
    }

    #[test]
    fn must_read_serde() {
        assert_eq!(
            sample().index(index_str("a.b").unwrap()).unwrap().item(),
            ItemKind::Pos(12)
        );
        assert_eq!(
            sample().index(index_str("c").unwrap()).unwrap().item(),
            ItemKind::Null
        );
    }

    #[test]
    fn indefinite_strings() {
        let cases = vec![
            // 2 chunks (with unicode)
            (
                "exampleα≤β",
                vec![
                    0x7fu8, 0x67, 101, 120, 97, 109, 112, 108, 101, 0x67, 206, 177, 226, 137, 164,
                    206, 178, 0xff,
                ],
            ),
            // 1 chunk
            (
                "example",
                vec![0x7fu8, 0x67, 101, 120, 97, 109, 112, 108, 101, 0xff],
            ),
            // 0 chunks
            ("", vec![0x7fu8, 0xff]),
            // empty chunk
            ("", vec![0x7fu8, 0x60, 0xff]),
        ];

        for (res, bytes) in cases {
            let cbor = Cbor::unchecked(&*bytes);
            assert!(
                matches!(cbor.item(), ItemKind::Str(s) if s == res),
                "value was {:?}",
                cbor.item()
            );

            let cbor = CborOwned::canonical(bytes, false).unwrap();
            assert!(
                matches!(cbor.item(), ItemKind::Str(s) if s.as_str().unwrap() == res),
                "value was {:?}",
                cbor.item()
            );
        }
    }

    #[test]
    fn float() {
        let bytes = vec![0xfau8, 0, 0, 51, 17];
        let cbor = Cbor::unchecked(&*bytes);
        assert_eq!(cbor.item(), ItemKind::Float(1.8319174824118334e-41));
        let cbor = CborOwned::canonical(bytes, false).unwrap();
        assert_eq!(cbor.item(), ItemKind::Float(1.8319174824118334e-41));
    }
}
