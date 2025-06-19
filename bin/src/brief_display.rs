use cbor_data::{Cbor, ItemKind, TaggedItem, Visitor};
use std::fmt::{Display, Formatter};

pub struct BriefDisplay<'a> {
    pub cbor: &'a Cbor,
    pub max_depth: usize,
    pub array_length: usize,
    pub censored_properties: &'a [String],
    pub text_length: usize,
}

impl<'a> Display for BriefDisplay<'a> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
        self.cbor.visit(&mut BriefDisplayVisitor {
            fmt,
            depth: 0,
            max_depth: self.max_depth,
            array_length: self.array_length as u64,
            censored_properties: self.censored_properties,
            text_length: self.text_length,
        })
    }
}

struct BriefDisplayVisitor<'a, 'b> {
    fmt: &'a mut Formatter<'b>,
    depth: usize,
    max_depth: usize,
    array_length: u64,
    censored_properties: &'a [String],
    text_length: usize,
}

impl<'a, 'b> Visitor<'a, std::fmt::Error> for BriefDisplayVisitor<'a, 'b> {
    fn visit_simple(&mut self, item: TaggedItem<'a>) -> Result<(), std::fmt::Error> {
        match item.kind() {
            ItemKind::Str(s) if s.len() > self.text_length => {
                let s = s.as_cow();
                let end = s.char_indices().nth(self.text_length).unwrap_or_default().0;
                if end == s.len() {
                    write!(self.fmt, "\"{}\"", s.escape_debug())?;
                } else {
                    write!(self.fmt, "\"{}\"…", s[..end].escape_debug())?;
                }
                Ok(())
            }
            _ => self.fmt.visit_simple(item),
        }
    }

    fn visit_array_begin(
        &mut self,
        array: TaggedItem<'a>,
        size: Option<u64>,
    ) -> Result<bool, std::fmt::Error> {
        self.depth += 1;

        if self.depth > self.max_depth {
            write!(self.fmt, "[…]")?;
            Ok(false)
        } else {
            self.fmt.visit_array_begin(array, size)
        }
    }

    fn visit_array_index(
        &mut self,
        _array: TaggedItem<'a>,
        index: u64,
    ) -> Result<bool, std::fmt::Error> {
        if index == self.array_length {
            write!(self.fmt, "…")?;
        } else if index > 0 && index < self.array_length {
            write!(self.fmt, ", ")?;
        }
        Ok(index < self.array_length)
    }

    fn visit_array_end(&mut self, array: TaggedItem<'a>) -> Result<(), std::fmt::Error> {
        if self.depth <= self.max_depth {
            self.fmt.visit_array_end(array)?;
        }
        self.depth -= 1;
        Ok(())
    }

    fn visit_dict_begin(
        &mut self,
        dict: TaggedItem<'a>,
        size: Option<u64>,
    ) -> Result<bool, std::fmt::Error> {
        self.depth += 1;

        if self.depth > self.max_depth {
            write!(self.fmt, "{{…}}")?;
            Ok(false)
        } else {
            self.fmt.visit_dict_begin(dict, size)
        }
    }

    fn visit_dict_key(
        &mut self,
        dict: TaggedItem<'a>,
        key: TaggedItem<'a>,
        is_first: bool,
    ) -> Result<bool, std::fmt::Error> {
        if let Some(key) = key.decode().as_str() {
            if self.censored_properties.iter().any(|k| k == key.as_ref()) {
                write!(self.fmt, "…")?;
                return Ok(false);
            }
        }
        self.fmt.visit_dict_key(dict, key, is_first)
    }

    fn visit_dict_end(&mut self, dict: TaggedItem<'a>) -> Result<(), std::fmt::Error> {
        if self.depth <= self.max_depth {
            self.fmt.visit_dict_end(dict)?;
        }
        self.depth -= 1;
        Ok(())
    }
}
