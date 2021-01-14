use crate::{
    reader::{integer, tagged_value, Iter},
    value::Tags,
    CborValue,
    ValueKind::*,
};

/// Visitor for the structure of a CBOR item.
///
/// The visit is guided by the visitor in the sense that uninteresting arrays, dicts,
/// or some of their values can be skipped by returning `false` from the respective
/// methods.
///
/// Example:
///
/// ```
/// use std::fmt::{Error, Formatter, Write};
/// use cbor_data::{Cbor, CborOwned, CborValue, Visitor, Tags};
///
/// fn pretty_print(value: Cbor) -> Result<String, Error> {
///     struct X<'a>(&'a mut String);
///     impl<'a> Visitor<'a, Error> for X<'a> {
///         fn visit_simple(&mut self, item: CborValue) -> Result<(), Error> {
///             write!(self.0, "{}", item.kind)
///         }
///         fn visit_array_begin(&mut self, size: Option<u64>, tags: Tags<'a>) -> Result<bool, Error> {
///             write!(self.0, "[")?;
///             Ok(true)
///         }
///         fn visit_array_index(&mut self, idx: u64) -> Result<bool, Error> {
///             if idx > 0 {
///                 write!(self.0, ", ")?;
///             }
///             Ok(true)
///         }
///         fn visit_array_end(&mut self) -> Result<(), Error> {
///             write!(self.0, "]")
///         }
///         fn visit_dict_begin(&mut self, size: Option<u64>, tags: Tags<'a>) -> Result<bool, Error> {
///             write!(self.0, "{{")?;
///             Ok(true)
///         }
///         fn visit_dict_key(&mut self, key: &str, is_first: bool) -> Result<bool, Error> {
///             if !is_first {
///                 write!(self.0, ", ")?;
///             }
///             write!(self.0, "\"{}\": ", key.escape_debug())?;
///             Ok(true)
///         }
///         fn visit_dict_end(&mut self) -> Result<(), Error> {
///             write!(self.0, "}}")
///         }
///     }
///     let mut s = String::new();
///     value.visit(&mut X(&mut s))?;
///     Ok(s)
/// }
///
/// let bytes = vec![
///     0xc4u8, 0x84, 5, 0xa2, 0x61, b'a', 0x39, 2, 154, 0x61, b'b', 0x46, b'd', b'e', b'f', b'd',
///     b'e', b'f', 0x82, 0xf4, 0x65, b'h', b'e', b'l', b'l', b'o', 0xd9, 48, 57, 0xf6,
/// ];
/// let cbor = CborOwned::canonical(bytes).expect("invalid CBOR");
///
/// let pretty = pretty_print(cbor.borrow()).expect("should always be able to write to a String …");
///
/// assert_eq!(pretty, r#"[5, {"a": -667, "b": 0x646566646566}, [false, "hello"], null]"#);
/// ```
#[allow(unused_variables)]
pub trait Visitor<'a, Err> {
    /// Visit a simple item, i.e. `item.kind` will neither be `Array` nor `Dict`.
    fn visit_simple(&mut self, item: CborValue<'a>) -> Result<(), Err> {
        Ok(())
    }
    /// Visit the beginning of an array. `size` is None for indefinite size encoding.
    /// Return `false` to skip this array entirely, meaning that `visit_array_index`
    /// and `visit_array_end` will NOT be called for it.
    fn visit_array_begin(&mut self, size: Option<u64>, tags: Tags<'a>) -> Result<bool, Err> {
        Ok(true)
    }
    /// Visit an array element at the given index. Return `false` to skip over the element’s
    /// contents, otherwise nested items (simple or otherwise) will be visited before visiting
    /// the next element or the array’s end.
    fn visit_array_index(&mut self, index: u64) -> Result<bool, Err> {
        Ok(true)
    }
    /// Visit the end of the current array.
    fn visit_array_end(&mut self) -> Result<(), Err> {
        Ok(())
    }
    /// Visit the beginning of an dict. `size` is None for indefinite size encoding.
    /// Return `false` to skip this dict entirely, meaning that `visit_dict_key`
    /// and `visit_dict_end` will NOT be called for it.
    fn visit_dict_begin(&mut self, size: Option<u64>, tags: Tags<'a>) -> Result<bool, Err> {
        Ok(true)
    }
    /// Visit a dict value at the given key. Return `false` to skip over the value’s
    /// contents, otherwise nested items (simple or otherwise) will be visited before visiting
    /// the next key or the dict’s end.
    fn visit_dict_key(&mut self, key: &str, is_first: bool) -> Result<bool, Err> {
        Ok(true)
    }
    /// Visit the end of the current dict.
    fn visit_dict_end(&mut self) -> Result<(), Err> {
        Ok(())
    }
}

pub fn visit<'a, Err, V: Visitor<'a, Err>>(v: &mut V, c: CborValue<'a>) -> Result<(), Err> {
    match c.kind {
        Array => {
            let info = integer(c.bytes);
            let bytes = info.map(|x| x.2).unwrap_or_else(|| &c.bytes[1..]);
            let length = info.map(|x| x.0);

            if !v.visit_array_begin(length, c.tags())? {
                return Ok(());
            }

            let iter = Iter::new(bytes, length);
            for (idx, item) in iter.enumerate() {
                if v.visit_array_index(idx as u64)? {
                    if let Some(item) = tagged_value(item.as_slice()).and_then(|i| i.decoded()) {
                        visit(v, item)?;
                    }
                }
            }

            v.visit_array_end()
        }
        Dict => {
            let info = integer(c.bytes);
            let bytes = info.map(|x| x.2).unwrap_or_else(|| &c.bytes[1..]);
            let length = info.map(|x| x.0);

            if !v.visit_dict_begin(length, c.tags())? {
                return Ok(());
            }

            let mut iter = Iter::new(bytes, length.map(|x| x * 2));
            let mut is_first = true;
            while let Some(key) = iter.next() {
                if let Some(Str(key)) = tagged_value(key.as_slice()).map(|x| x.kind) {
                    if v.visit_dict_key(key, is_first)? {
                        if let Some(item) = iter
                            .next()
                            .and_then(|item| tagged_value(item.as_slice()))
                            .and_then(|item| item.decoded())
                        {
                            visit(v, item)?;
                        }
                    }
                    is_first = false;
                }
            }

            v.visit_dict_end()
        }
        _ => v.visit_simple(c),
    }
}
