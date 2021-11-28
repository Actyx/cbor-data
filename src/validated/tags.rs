use crate::reader::integer;
use std::fmt::{Display, Formatter, Write};

/// Iterable view onto the CBOR tags applied to an item.
///
/// Equality is based on the actual encoding, not on the sequence of extracted tags;
/// this difference is only relevant if tags violate the recommendation of optimal
/// integer encoding.
///
/// ```
/// use cbor_data::CborOwned;
///
/// // FALSE tagged with 1, 11, 11*256+22
/// let cbor = CborOwned::canonical([0xc1, 0xd8, 11, 0xd9, 11, 22, 0xf4], false).unwrap();
/// let item = cbor.tagged_item();
/// assert_eq!(item.tags().collect::<Vec<_>>(), vec![1, 11, 11 * 256 + 22]);
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Tags<'a> {
    bytes: &'a [u8],
}

impl<'a> Display for Tags<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tags(")?;
        let mut first = true;
        for t in *self {
            if first {
                first = false;
            } else {
                f.write_char(',')?;
            }
            write!(f, "{}", t)?;
        }
        f.write_str(")")?;
        Ok(())
    }
}

impl<'a> Tags<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// outermost / first tag
    pub fn first(mut self) -> Option<u64> {
        self.next()
    }

    /// single tag. If there is more than one tag, this will return None
    pub fn single(&self) -> Option<u64> {
        let mut iter = *self;
        iter.next().filter(|_| iter.next().is_none())
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn empty() -> Self {
        Self { bytes: &[] }
    }
}

impl<'a> Iterator for Tags<'a> {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            None
        } else {
            let (tag, _, remaining) = integer(self.bytes)?;
            self.bytes = remaining;
            Some(tag)
        }
    }
}
