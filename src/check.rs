use crate::{
    constants::*,
    error::ErrorKind,
    reader::{indefinite, integer, major},
    validated::iterators::BytesIter,
    Cbor, Error,
};
use std::borrow::Cow;

pub(crate) trait MkErr {
    type Out;
    fn header_value(self, bytes: &[u8]) -> Result<Self::Out, Error<'_>>;
}
impl<T> MkErr for Option<T> {
    type Out = T;
    fn header_value(self, bytes: &[u8]) -> Result<Self::Out, Error<'_>> {
        self.ok_or_else(|| {
            // in case of 31 it can’t be EOF
            if bytes[0] & 31 > 27 {
                Error::AtSlice(bytes, ErrorKind::InvalidInfo)
            } else {
                Error::UnexpectedEof("header value")
            }
        })
    }
}

pub(crate) fn value_bytes(
    bytes: &[u8],
    get_bytes: bool,
    check_str: bool,
) -> Result<(Cow<[u8]>, &[u8]), Error> {
    let m = major(bytes).unwrap();
    let (len, _, mut rest) = integer(bytes)
        .or_else(|| indefinite(bytes))
        .header_value(bytes)?;
    if len == u64::MAX {
        // since an item takes at least 1 byte, u64::MAX is an impossible size
        let mut b = Vec::new();
        while *rest.get(0).ok_or(Error::UnexpectedEof("string fragment"))? != STOP_BYTE {
            if major(rest).unwrap() != m {
                return Err(Error::AtSlice(rest, ErrorKind::InvalidStringFragment));
            }
            let (len, _, r) = integer(rest).header_value(rest)?;
            let len = len as usize;
            if len > r.len() {
                return Err(Error::UnexpectedEof("string fragment"));
            }
            let s = &r[..len];
            if check_str {
                std::str::from_utf8(s).map_err(|e| Error::AtSlice(s, ErrorKind::InvalidUtf8(e)))?;
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
                std::str::from_utf8(s).map_err(|e| Error::AtSlice(s, ErrorKind::InvalidUtf8(e)))?;
            }
            Ok((Cow::Borrowed(s), &rest[len..]))
        } else {
            Err(Error::UnexpectedEof("string value"))
        }
    }
}

pub fn validate(bytes: &[u8], tag: Option<u64>) -> Result<(&Cbor, &[u8]), Error> {
    let m = major(bytes).ok_or(Error::UnexpectedEof("item header"))?;
    match m {
        MAJOR_POS | MAJOR_NEG | MAJOR_LIT => integer(bytes)
            .map(|(_, b, r)| (Cbor::unchecked(b), r))
            .header_value(bytes),
        MAJOR_BYTES | MAJOR_STR => {
            let check = m == MAJOR_BYTES && tag == Some(TAG_CBOR_ITEM);
            let (value, rest) = value_bytes(bytes, check, m == MAJOR_STR)?;
            if check {
                if let Err(e) = validate(value.as_ref(), None) {
                    if let Some(mut offset) = e.offset(value.as_ref()) {
                        let iter = if bytes[0] & 31 == INDEFINITE_SIZE {
                            BytesIter::new(&bytes[1..], None)
                        } else {
                            BytesIter::new(bytes, Some(1))
                        };
                        for slice in iter {
                            if offset < slice.len() {
                                return Err(e.with_location(&slice[offset..]));
                            }
                            offset -= slice.len();
                        }
                        return Err(e.with_location(rest));
                    } else {
                        return Err(e.with_location(bytes));
                    }
                }
            }
            Ok((Cbor::unchecked(bytes), rest))
        }
        MAJOR_TAG => {
            let (t, _, rest) = integer(bytes).header_value(bytes)?;
            let tag = match tag {
                Some(_) => Some(u64::MAX),
                None => Some(t),
            };
            let (_cbor, rest) = validate(rest, tag)?;
            Ok((Cbor::unchecked(bytes), rest))
        }
        MAJOR_ARRAY | MAJOR_DICT => {
            let (len, _, mut rest) = integer(bytes)
                .or_else(|| indefinite(bytes))
                .header_value(bytes)?;
            if len == u64::MAX {
                while *rest.get(0).ok_or(Error::UnexpectedEof("array item"))? != STOP_BYTE {
                    rest = validate(rest, None).map(|x| x.1)?;
                    if m == MAJOR_DICT {
                        rest = validate(rest, None).map(|x| x.1)?;
                    }
                }
                let size = bytes.len() - rest.len() + 1;
                Ok((Cbor::unchecked(&bytes[..size]), &rest[1..]))
            } else {
                for _ in 0..len {
                    rest = validate(rest, None).map(|x| x.1)?;
                    if m == MAJOR_DICT {
                        rest = validate(rest, None).map(|x| x.1)?;
                    }
                }
                let size = bytes.len() - rest.len();
                Ok((Cbor::unchecked(&bytes[..size]), rest))
            }
        }
        _ => unreachable!(),
    }
}
