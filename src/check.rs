use crate::{
    constants::*,
    error::InternalError,
    reader::{indefinite, integer, major},
    validated::iterators::BytesIter,
    Cbor,
    ErrorKind::*,
    ParseError,
    WhileParsing::*,
};
use std::borrow::Cow;

pub(crate) trait MkErr {
    type Out;
    fn header_value(self, bytes: &[u8]) -> Result<Self::Out, InternalError<'_>>;
}
impl<T> MkErr for Option<T> {
    type Out = T;
    fn header_value(self, bytes: &[u8]) -> Result<Self::Out, InternalError<'_>> {
        self.ok_or_else(|| {
            if bytes[0] & 31 > 27 {
                InternalError::new(bytes, InvalidInfo)
            } else {
                InternalError::new(&bytes[bytes.len()..], UnexpectedEof(HeaderValue))
            }
        })
    }
}

fn frag_err(position: &[u8], is_string: bool) -> InternalError {
    let w = if is_string {
        StringFragment
    } else {
        BytesFragment
    };
    InternalError::new(position, UnexpectedEof(w))
}

pub(crate) fn value_bytes(
    bytes: &[u8],
    get_bytes: bool,
    check_str: bool,
) -> Result<(Cow<[u8]>, &[u8]), InternalError> {
    let m = major(bytes).unwrap();
    let (len, _, mut rest) = integer(bytes)
        .or_else(|| indefinite(bytes))
        .header_value(bytes)?;
    if len == u64::MAX {
        // since an item takes at least 1 byte, u64::MAX is an impossible size
        let mut b = Vec::new();
        while *rest.get(0).ok_or_else(|| frag_err(rest, check_str))? != STOP_BYTE {
            if major(rest).unwrap() != m {
                return Err(InternalError::new(rest, InvalidStringFragment));
            }
            let (len, _, r) = integer(rest).header_value(rest)?;
            let len = len as usize;
            if len > r.len() {
                return Err(frag_err(r, check_str));
            }
            let s = &r[..len];
            if check_str {
                std::str::from_utf8(s).map_err(|e| InternalError::new(s, InvalidUtf8(e)))?;
            }
            if get_bytes {
                b.extend_from_slice(s);
            }
            rest = &r[len..];
        }
        Ok((Cow::Owned(b), &rest[1..]))
    } else {
        let len = len as usize;
        if rest.len() >= len {
            let s = &rest[..len];
            if check_str {
                std::str::from_utf8(s).map_err(|e| InternalError::new(s, InvalidUtf8(e)))?;
            }
            Ok((Cow::Borrowed(s), &rest[len..]))
        } else {
            let w = if check_str { StringValue } else { BytesValue };
            Err(InternalError::new(rest, UnexpectedEof(w)))
        }
    }
}

pub fn validate(bytes: &[u8], permit_suffix: bool) -> Result<(&Cbor, &[u8]), ParseError> {
    fn rec(bytes: &[u8], tag: Option<u64>) -> Result<(&Cbor, &[u8]), InternalError> {
        let m = major(bytes).ok_or_else(|| InternalError::new(bytes, UnexpectedEof(ItemHeader)))?;
        match m {
            MAJOR_POS | MAJOR_NEG | MAJOR_LIT => integer(bytes)
                .map(|(_, b, r)| (Cbor::unchecked(b), r))
                .header_value(bytes),
            MAJOR_BYTES | MAJOR_STR => {
                let check = m == MAJOR_BYTES && tag == Some(TAG_CBOR_ITEM);
                let (value, rest) = value_bytes(bytes, check, m == MAJOR_STR)?;
                if check {
                    rec(value.as_ref(), None)
                        .and_then(|(_cbor, r)| {
                            if r.is_empty() {
                                Ok((Cbor::unchecked(&bytes[..bytes.len() - rest.len()]), rest))
                            } else {
                                Err(InternalError::new(r, TrailingGarbage))
                            }
                        })
                        .map_err(|e| {
                            let mut offset = e.offset(value.as_ref());
                            let iter = if bytes[0] & 31 == INDEFINITE_SIZE {
                                BytesIter::new(&bytes[1..], None)
                            } else {
                                BytesIter::new(bytes, Some(1))
                            };
                            for slice in iter {
                                if offset < slice.len() {
                                    return e.with_location(&slice[offset..]);
                                }
                                offset -= slice.len();
                            }
                            e.with_location(rest)
                        })
                } else {
                    Ok((Cbor::unchecked(&bytes[..bytes.len() - rest.len()]), rest))
                }
            }
            MAJOR_TAG => {
                let (t, _, rest) = integer(bytes).header_value(bytes)?;
                let tag = match tag {
                    Some(_) => Some(u64::MAX),
                    None => Some(t),
                };
                let (_cbor, rest) = rec(rest, tag)?;
                Ok((Cbor::unchecked(&bytes[..bytes.len() - rest.len()]), rest))
            }
            MAJOR_ARRAY | MAJOR_DICT => {
                let (len, _, mut rest) = integer(bytes)
                    .or_else(|| indefinite(bytes))
                    .header_value(bytes)?;
                let w = if m == MAJOR_ARRAY {
                    ArrayItem
                } else {
                    DictItem
                };
                if len == u64::MAX {
                    while *rest
                        .get(0)
                        .ok_or_else(|| InternalError::new(rest, UnexpectedEof(w)))?
                        != STOP_BYTE
                    {
                        rest = rec(rest, None).map(|x| x.1)?;
                        if m == MAJOR_DICT {
                            rest = rec(rest, None).map(|x| x.1)?;
                        }
                    }
                    let size = bytes.len() - rest.len() + 1;
                    Ok((Cbor::unchecked(&bytes[..size]), &rest[1..]))
                } else {
                    for _ in 0..len {
                        if rest.is_empty() {
                            return Err(InternalError::new(rest, UnexpectedEof(w)));
                        }
                        rest = rec(rest, None).map(|x| x.1)?;
                        if m == MAJOR_DICT {
                            rest = rec(rest, None).map(|x| x.1)?;
                        }
                    }
                    let size = bytes.len() - rest.len();
                    Ok((Cbor::unchecked(&bytes[..size]), rest))
                }
            }
            _ => unreachable!(),
        }
    }
    let (cbor, rest) = rec(bytes, None).map_err(|e| e.rebase(bytes))?;
    if rest.is_empty() || permit_suffix {
        Ok((cbor, rest))
    } else {
        Err(InternalError::new(rest, TrailingGarbage).rebase(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::{
        CborOwned,
        ErrorKind::{self, *},
        WhileParsing::*,
    };

    fn t(bytes: impl AsRef<[u8]>) -> (usize, ErrorKind) {
        let bytes = bytes.as_ref();
        let error = validate(bytes, false).unwrap_err();
        let error2 = CborOwned::canonical(bytes).unwrap_err();
        assert_eq!(error2, error, "canonical != checked");
        (error.offset(), error.kind())
    }

    fn tt(bytes: impl AsRef<[u8]>) -> (usize, usize, Option<usize>) {
        let (pos, err) = t(bytes);
        if let InvalidUtf8(e) = err {
            (pos, e.valid_up_to(), e.error_len())
        } else {
            panic!("wrong error: {}", err)
        }
    }

    #[test]
    fn invalid_info() {
        // pos
        assert_eq!(t([28, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([29, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([30, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([31, 1, 2, 3, 4]), (0, InvalidInfo));
        // neg
        assert_eq!(t([32 + 28, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([32 + 29, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([32 + 30, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([32 + 31, 1, 2, 3, 4]), (0, InvalidInfo));
        // bytes
        assert_eq!(t([64 + 28, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([64 + 29, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([64 + 30, 1, 2, 3, 4]), (0, InvalidInfo));
        // string
        assert_eq!(t([96 + 28, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([96 + 29, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([96 + 30, 1, 2, 3, 4]), (0, InvalidInfo));
        // array
        assert_eq!(t([128 + 28, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([128 + 29, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([128 + 30, 1, 2, 3, 4]), (0, InvalidInfo));
        // dict
        assert_eq!(t([160 + 28, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([160 + 29, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([160 + 30, 1, 2, 3, 4]), (0, InvalidInfo));
        // tag
        assert_eq!(t([192 + 28, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([192 + 29, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([192 + 30, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([192 + 31, 1, 2, 3, 4]), (0, InvalidInfo));
        // special
        assert_eq!(t([224 + 28, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([224 + 29, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([224 + 30, 1, 2, 3, 4]), (0, InvalidInfo));
        assert_eq!(t([224 + 31, 1, 2, 3, 4]), (0, InvalidInfo));

        // tagged
        assert_eq!(t([192 + 1, 28, 1, 2, 3]), (1, InvalidInfo));
        assert_eq!(t([192 + 24, 250, 28, 1, 2, 3]), (2, InvalidInfo));
        assert_eq!(t([192 + 25, 1, 2, 28, 1, 2, 3]), (3, InvalidInfo));
        assert_eq!(t([192 + 26, 1, 2, 3, 4, 28]), (5, InvalidInfo));
        assert_eq!(t([192 + 27, 1, 2, 3, 4, 5, 6, 7, 8, 28]), (9, InvalidInfo));

        // cbor encoded
        assert_eq!(t([0xd8, 24, 0x41, 31]), (3, InvalidInfo));
        assert_eq!(t([0xd8, 24, 0x5f, 0x41, 31, 0xff]), (4, InvalidInfo));
        assert_eq!(t([0xd8, 24, 0x5f, 0x40, 0x41, 31, 0xff]), (5, InvalidInfo));
    }

    #[test]
    fn trailing_garbage() {
        assert_eq!(t([0x01, 2]), (1, TrailingGarbage));
        assert_eq!(t([0x21, 2]), (1, TrailingGarbage));
        assert_eq!(t([0x40, 2]), (1, TrailingGarbage));
        assert_eq!(t([0x60, 2]), (1, TrailingGarbage));
        assert_eq!(t([0x80, 2]), (1, TrailingGarbage));
        assert_eq!(t([0x9f, 0xff, 2]), (2, TrailingGarbage));
        assert_eq!(t([0xa0, 2]), (1, TrailingGarbage));
        assert_eq!(t([0xbf, 0xff, 2]), (2, TrailingGarbage));
        assert_eq!(t([0xe0, 2]), (1, TrailingGarbage));

        assert_eq!(t([0xc1, 0x01, 2]), (2, TrailingGarbage));
        assert_eq!(t([0xd8, 24, 0x41, 0x01, 2]), (4, TrailingGarbage));
        assert_eq!(t([0xd8, 24, 0x42, 0x01, 2]), (4, TrailingGarbage));
        assert_eq!(
            t([0xd8, 24, 0x5f, 0x40, 0x42, 0x01, 2, 0xff]),
            (6, TrailingGarbage)
        );
        assert_eq!(
            t([0xd8, 24, 0x5f, 0x40, 0x41, 0x01, 0xff, 2]),
            (7, TrailingGarbage)
        );
    }

    #[test]
    fn invalid_fragment() {
        assert_eq!(t([0x5f, 0x61, b'a', 0xff]), (1, InvalidStringFragment));
        assert_eq!(
            t([0x5f, 0x41, b'a', 0x61, 1, 0xff]),
            (3, InvalidStringFragment)
        );
        assert_eq!(t([0x7f, 0x41, 1, 0xff]), (1, InvalidStringFragment));
        assert_eq!(
            t([0x7f, 0x61, b'a', 0x41, 1, 0xff]),
            (3, InvalidStringFragment)
        );
    }

    #[test]
    fn utf8() {
        assert_eq!(tt([0x62, 65, 128, 65]), (1, 1, Some(1)));
        assert_eq!(tt([0x62, 65, 128, 129, 65]), (1, 1, Some(1)));
        assert_eq!(
            tt([0x7f, 0x62, 0xc3, 0xbc, 0x61, 0xc3, 0x61, 0xbc, 0xff]),
            (5, 0, None)
        );
    }

    #[test]
    fn eof() {
        assert_eq!(t([]), (0, UnexpectedEof(ItemHeader)));
        assert_eq!(t([0x18]), (1, UnexpectedEof(HeaderValue)));
        assert_eq!(t([0x41]), (1, UnexpectedEof(BytesValue)));
        assert_eq!(t([0x5f]), (1, UnexpectedEof(BytesFragment)));
        assert_eq!(t([0x5f, 0x41]), (2, UnexpectedEof(BytesFragment)));
        assert_eq!(t([0x61]), (1, UnexpectedEof(StringValue)));
        assert_eq!(t([0x7f]), (1, UnexpectedEof(StringFragment)));
        assert_eq!(t([0x7f, 0x61]), (2, UnexpectedEof(StringFragment)));
        assert_eq!(t([0x81]), (1, UnexpectedEof(ArrayItem)));
        assert_eq!(t([0x9f]), (1, UnexpectedEof(ArrayItem)));
        assert_eq!(t([0xa1]), (1, UnexpectedEof(DictItem)));
        assert_eq!(t([0xbf]), (1, UnexpectedEof(DictItem)));
    }

    #[test]
    fn suffix() {
        fn t(bytes: impl AsRef<[u8]>) -> Result<usize, (usize, ErrorKind)> {
            let bytes = bytes.as_ref();
            validate(bytes, true)
                .map(|(_cbor, rest)| rest.len())
                .map_err(|error| {
                    let error2 = CborOwned::canonical(bytes).unwrap_err();
                    assert_eq!(error2, error, "canonical != checked");
                    (error.offset(), error.kind())
                })
        }

        assert_eq!(t([0xd8, 24, 0x41, 0x01, 2]), Ok(1));
        assert_eq!(t([0xd8, 24, 0x42, 0x01, 2]), Err((4, TrailingGarbage)));
    }
}
