use crate::{ItemKind, TaggedItem};

/// Visitor for the structure of a CBOR item.
///
/// The visit is guided by the visitor in the sense that uninteresting arrays, dicts,
/// or some of their values can be skipped by returning `false` from the respective
/// methods.
///
/// **Note the different lifetimes needed on the `impl Visitor`!** The first lifetime
/// `'a` describes how long the underlying `Cbor` value lives, i.e. how long the items
/// passed into the visitor’s methods stay available. The second lifetime `'b` denotes
/// the mutable borrow of the `String` we’re writing into, which needs to be more
/// short-lived since we want to move the `String` before the `Cbor` expires.
///
/// Example:
///
/// ```
/// use std::fmt::{Error, Formatter, Write};
/// use cbor_data::{Cbor, CborOwned, TaggedItem, Visitor};
///
/// fn pretty_print(value: &Cbor) -> Result<String, Error> {
///     struct X<'a>(&'a mut String);
///     impl<'a, 'b> Visitor<'a, Error> for X<'b> {
///         fn visit_simple(&mut self, item: TaggedItem<'a>) -> Result<(), Error> {
///             write!(self.0, "{}", item)
///         }
///         fn visit_array_begin(&mut self, item: TaggedItem<'a>, size: Option<u64>) -> Result<bool, Error> {
///             write!(self.0, "[")?;
///             Ok(true)
///         }
///         fn visit_array_index(&mut self, item: TaggedItem<'a>, idx: u64) -> Result<bool, Error> {
///             if idx > 0 {
///                 write!(self.0, ", ")?;
///             }
///             Ok(true)
///         }
///         fn visit_array_end(&mut self, item: TaggedItem<'a>) -> Result<(), Error> {
///             write!(self.0, "]")
///         }
///         fn visit_dict_begin(&mut self, item: TaggedItem<'a>, size: Option<u64>) -> Result<bool, Error> {
///             write!(self.0, "{{")?;
///             Ok(true)
///         }
///         fn visit_dict_key(&mut self, item: TaggedItem<'a>, key: TaggedItem<'a>, is_first: bool) -> Result<bool, Error> {
///             if !is_first {
///                 write!(self.0, ", ")?;
///             }
///             write!(self.0, "{}: ", key)?;
///             Ok(true)
///         }
///         fn visit_dict_end(&mut self, item: TaggedItem<'a>) -> Result<(), Error> {
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
/// let pretty = pretty_print(cbor.as_ref()).expect("should always be able to write to a String …");
///
/// assert_eq!(pretty, r#"[5, {"a": -667, "b": h'646566646566'}, [false, "hello"], 12345(null)]"#);
/// ```
#[allow(unused_variables)]
pub trait Visitor<'a, Err> {
    /// Visit a simple item, i.e. `item.kind` will neither be `Array` nor `Dict`.
    fn visit_simple(&mut self, item: TaggedItem<'a>) -> Result<(), Err> {
        Ok(())
    }
    /// Visit the beginning of an array. `size` is None for indefinite size encoding.
    /// Return `false` to skip this array entirely, meaning that `visit_array_index`
    /// and `visit_array_end` will NOT be called for it.
    fn visit_array_begin(&mut self, array: TaggedItem<'a>, size: Option<u64>) -> Result<bool, Err> {
        Ok(true)
    }
    /// Visit an array element at the given index. Return `false` to skip over the element’s
    /// contents, otherwise nested items (simple or otherwise) will be visited before visiting
    /// the next element or the array’s end.
    fn visit_array_index(&mut self, array: TaggedItem<'a>, index: u64) -> Result<bool, Err> {
        Ok(true)
    }
    /// Visit the end of the current array.
    fn visit_array_end(&mut self, array: TaggedItem<'a>) -> Result<(), Err> {
        Ok(())
    }
    /// Visit the beginning of an dict. `size` is None for indefinite size encoding.
    /// Return `false` to skip this dict entirely, meaning that `visit_dict_key`
    /// and `visit_dict_end` will NOT be called for it.
    fn visit_dict_begin(&mut self, dict: TaggedItem<'a>, size: Option<u64>) -> Result<bool, Err> {
        Ok(true)
    }
    /// Visit a dict value at the given key. Return `false` to skip over the value’s
    /// contents, otherwise nested items (simple or otherwise) will be visited before visiting
    /// the next key or the dict’s end.
    ///
    /// In most cases the key will be a string or an integer. In the rare case where a key is a
    /// complex struct, you can visit it manually.
    fn visit_dict_key(
        &mut self,
        dict: TaggedItem<'a>,
        key: TaggedItem<'a>,
        is_first: bool,
    ) -> Result<bool, Err> {
        Ok(true)
    }
    /// Visit the end of the current dict.
    fn visit_dict_end(&mut self, dict: TaggedItem<'a>) -> Result<(), Err> {
        Ok(())
    }
}

pub fn visit<'a, 'b, Err, V: Visitor<'b, Err>>(v: &'a mut V, c: TaggedItem<'b>) -> Result<(), Err> {
    match c.kind() {
        ItemKind::Array(iter) => {
            if v.visit_array_begin(c, iter.size())? {
                for (idx, item) in iter.enumerate() {
                    if v.visit_array_index(c, idx as u64)? {
                        visit(v, item.tagged_item())?;
                    }
                }
            }

            v.visit_array_end(c)
        }
        ItemKind::Dict(iter) => {
            if v.visit_dict_begin(c, iter.size())? {
                let mut is_first = true;
                for (key, item) in iter {
                    if v.visit_dict_key(c, key.tagged_item(), is_first)? {
                        visit(v, item.tagged_item())?;
                    }
                    is_first = false;
                }
            }

            v.visit_dict_end(c)
        }
        _ => v.visit_simple(c),
    }
}

#[cfg(test)]
mod tests {
    use crate::{constants::TAG_CBOR_ITEM, CborBuilder, ItemKind, PathElement, TaggedItem, Writer};
    use pretty_assertions::assert_eq;

    #[test]
    fn smoke() {
        let x = CborBuilder::new().write_array([1, 2], |b| {
            b.write_bool(true, [3, 4]);
            b.write_dict([5, 6], |b| {
                b.with_key("k", |b| b.write_pos(5, [7, 8]));
                b.with_cbor_key(|b| b.write_neg(42, [9, 10]), |b| b.write_null([11, 12]));
            });
            b.write_bytes(
                CborBuilder::new()
                    .write_array([], |b| {
                        b.write_bool(false, [0]);
                        b.write_undefined([42]);
                    })
                    .as_slice(),
                [TAG_CBOR_ITEM],
            );
        });

        struct Visitor<'b>(&'b mut Vec<String>, bool);
        impl<'a, 'b> super::Visitor<'a, &'static str> for Visitor<'b> {
            fn visit_simple(&mut self, item: TaggedItem<'a>) -> Result<(), &'static str> {
                self.0.push(format!("simple {:?}", item));
                if self.1 {
                    Err("buh")
                } else {
                    Ok(())
                }
            }

            fn visit_array_begin(
                &mut self,
                array: TaggedItem<'a>,
                size: Option<u64>,
            ) -> Result<bool, &'static str> {
                self.0.push(format!("array_begin {:?} {:?}", array, size));
                Ok(true)
            }

            fn visit_array_index(
                &mut self,
                array: TaggedItem<'a>,
                index: u64,
            ) -> Result<bool, &'static str> {
                self.0.push(format!("array_index {:?} {}", array, index));
                Ok(true)
            }

            fn visit_array_end(&mut self, array: TaggedItem<'a>) -> Result<(), &'static str> {
                self.0.push(format!("array_end {:?}", array));
                Ok(())
            }

            fn visit_dict_begin(
                &mut self,
                dict: TaggedItem<'a>,
                size: Option<u64>,
            ) -> Result<bool, &'static str> {
                self.0.push(format!("dict_begin {:?} {:?}", dict, size));
                Ok(true)
            }

            fn visit_dict_key(
                &mut self,
                dict: TaggedItem<'a>,
                key: TaggedItem<'a>,
                is_first: bool,
            ) -> Result<bool, &'static str> {
                self.0
                    .push(format!("dict_key {:?} {:?} {}", dict, key, is_first));
                Ok(true)
            }

            fn visit_dict_end(&mut self, dict: TaggedItem<'a>) -> Result<(), &'static str> {
                self.0.push(format!("dict_end {:?}", dict));
                Ok(())
            }
        }

        let mut trace = Vec::new();
        x.visit(&mut Visitor(trace.as_mut(), false)).unwrap();
        assert_eq!(
            trace,
            vec![
                "array_begin TaggedItem(Tags(1,2), Array(Some(3))) Some(3)",
                "array_index TaggedItem(Tags(1,2), Array(Some(3))) 0",
                "simple TaggedItem(Tags(3,4), Bool(true))",
                "array_index TaggedItem(Tags(1,2), Array(Some(3))) 1",
                "dict_begin TaggedItem(Tags(5,6), Dict(Some(2))) Some(2)",
                "dict_key TaggedItem(Tags(5,6), Dict(Some(2))) TaggedItem(Tags(), Str(k)) true",
                "simple TaggedItem(Tags(7,8), Pos(5))",
                "dict_key TaggedItem(Tags(5,6), Dict(Some(2))) TaggedItem(Tags(9,10), Neg(42)) false",
                "simple TaggedItem(Tags(11,12), Null)",
                "dict_end TaggedItem(Tags(5,6), Dict(Some(2)))",
                "array_index TaggedItem(Tags(1,2), Array(Some(3))) 2",
                "simple TaggedItem(Tags(24), Bytes(82c0f4d82af7))",
                "array_end TaggedItem(Tags(1,2), Array(Some(3)))",
            ]
        );

        trace.clear();
        assert_eq!(
            x.visit(&mut Visitor(trace.as_mut(), true)).unwrap_err(),
            "buh"
        );
        assert_eq!(
            trace,
            vec![
                "array_begin TaggedItem(Tags(1,2), Array(Some(3))) Some(3)",
                "array_index TaggedItem(Tags(1,2), Array(Some(3))) 0",
                "simple TaggedItem(Tags(3,4), Bool(true))",
            ]
        );

        trace.clear();
        x.index([PathElement::Number(2)])
            .unwrap()
            .visit(&mut Visitor(trace.as_mut(), false))
            .unwrap();
        assert_eq!(
            trace,
            vec![
                "array_begin TaggedItem(Tags(), Array(Some(2))) Some(2)",
                "array_index TaggedItem(Tags(), Array(Some(2))) 0",
                "simple TaggedItem(Tags(0), Bool(false))",
                "array_index TaggedItem(Tags(), Array(Some(2))) 1",
                "simple TaggedItem(Tags(42), Undefined)",
                "array_end TaggedItem(Tags(), Array(Some(2)))",
            ]
        );

        assert_eq!(
            x.index([PathElement::Number(2), PathElement::Number(1)])
                .unwrap()
                .kind(),
            ItemKind::Undefined
        );
    }
}
