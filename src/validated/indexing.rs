use crate::{constants::TAG_CBOR_ITEM, Cbor, CborOwned, ItemKind, TaggedItem, Visitor};
use std::borrow::Cow;

pub struct IndexVisitor<'a, I: Iterator> {
    iter: Option<I>,
    arr_idx: Option<u64>,
    dict_idx: Option<PathElement<'a>>,
}

impl<'a, I: Iterator<Item = PathElement<'a>>> IndexVisitor<'a, I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter: Some(iter),
            arr_idx: None,
            dict_idx: None,
        }
    }

    fn iter(&mut self) -> &mut I {
        self.iter.as_mut().unwrap()
    }
}

impl<'a, 'b, I: Iterator<Item = PathElement<'b>>> Visitor<'a, Option<Cow<'a, Cbor>>>
    for IndexVisitor<'b, I>
{
    fn visit_simple(&mut self, item: TaggedItem<'a>) -> Result<(), Option<Cow<'a, Cbor>>> {
        if let (Some(TAG_CBOR_ITEM), ItemKind::Bytes(bytes)) = (item.tags().single(), item.kind()) {
            return if let Some(cbor) = bytes.as_slice() {
                Cbor::unchecked(cbor).visit(self)
            } else {
                let cbor = CborOwned::unchecked(bytes.to_vec());
                cbor.visit(self)
                    .map_err(|res| res.map(|cbor| Cow::Owned(cbor.into_owned())))
            };
        }

        if self.iter().next().is_some() {
            Err(None)
        } else {
            Err(Some(Cow::Borrowed(item.cbor())))
        }
    }

    fn visit_array_begin(
        &mut self,
        item: TaggedItem<'a>,
        size: Option<u64>,
    ) -> Result<bool, Option<Cow<'a, Cbor>>> {
        if let Some(idx) = self.iter().next() {
            let idx = match idx {
                PathElement::String(_) => return Err(None),
                PathElement::Number(x) => x,
                PathElement::Item(_) => return Err(None),
            };
            if let Some(size) = size {
                if size <= idx {
                    return Err(None);
                }
            }
            self.arr_idx = Some(idx);
            self.dict_idx = None;
            Ok(true)
        } else {
            Err(Some(Cow::Borrowed(item.cbor())))
        }
    }

    fn visit_array_index(
        &mut self,
        _item: TaggedItem<'a>,
        index: u64,
    ) -> Result<bool, Option<Cow<'a, Cbor>>> {
        Ok(index == self.arr_idx.unwrap())
    }

    fn visit_array_end(&mut self, _item: TaggedItem<'a>) -> Result<(), Option<Cow<'a, Cbor>>> {
        // exhausted the indefinite length array without success
        Err(None)
    }

    fn visit_dict_begin(
        &mut self,
        item: TaggedItem<'a>,
        _size: Option<u64>,
    ) -> Result<bool, Option<Cow<'a, Cbor>>> {
        if let Some(idx) = self.iter().next() {
            self.arr_idx = None;
            self.dict_idx = Some(idx);
            Ok(true)
        } else {
            Err(Some(Cow::Borrowed(item.cbor())))
        }
    }

    fn visit_dict_key(
        &mut self,
        _item: TaggedItem<'a>,
        key: TaggedItem<'a>,
        _is_first: bool,
    ) -> Result<bool, Option<Cow<'a, Cbor>>> {
        Ok(match self.dict_idx.as_ref().unwrap() {
            PathElement::String(idx) => matches!(key.kind(), ItemKind::Str(s) if s == idx),
            PathElement::Number(idx) => matches!(key.kind(), ItemKind::Pos(p) if p == *idx),
            PathElement::Item(idx) => &**idx == key.cbor(),
        })
    }

    fn visit_dict_end(&mut self, _item: TaggedItem<'a>) -> Result<(), Option<Cow<'a, Cbor>>> {
        // exhausted the dict without success
        Err(None)
    }
}

/// Path elements for indexing into CBOR structures
#[derive(Debug, Clone, PartialEq)]
pub enum PathElement<'a> {
    /// matches only text string dictionary keys encoded as major type 3
    String(Cow<'a, str>),
    /// matches only array indices or numeric dictionary keys encoded as major type 0
    Number(u64),
    /// matches only dictionary keys of exactly the byte sequence as this element
    Item(Cow<'a, Cbor>),
}

/// Iterator returned by [`index_str`](fn.index_str.html)
#[derive(Debug, Clone, PartialEq)]
pub struct IndexStr<'a>(&'a str);

impl<'a> IndexStr<'a> {
    pub fn new(s: &'a str) -> Option<Self> {
        let mut test = Self(s);
        (&mut test).count();
        if test.0.is_empty() {
            Some(Self(s))
        } else {
            None
        }
    }
}

impl<'a> Iterator for IndexStr<'a> {
    type Item = PathElement<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.0.starts_with('.') {
            self.0 = &self.0[1..];
        }
        if self.0.is_empty() {
            return None;
        }

        if self.0.starts_with('[') {
            let end = self.0.find(']')?;
            let idx: u64 = self.0[1..end].parse().ok()?;
            self.0 = &self.0[end + 1..];
            Some(PathElement::Number(idx))
        } else {
            let mut pos = self.0.len();
            for (p, ch) in self.0.char_indices() {
                if ch == '.' || ch == '[' {
                    pos = p;
                    break;
                }
            }
            let ret = PathElement::String(Cow::Borrowed(&self.0[..pos]));
            self.0 = &self.0[pos..];
            Some(ret)
        }
    }
}
