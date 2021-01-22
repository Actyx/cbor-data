pub enum Path<'a> {
    Num(u64, &'a Path<'a>),
    Str(&'a str, &'a Path<'a>),
    Bytes(&'a [u8], &'a Path<'a>),
    Done,
}

pub fn path() -> Path<'static> {
    Path::Done
}

impl<'a> Path<'a> {
    pub fn num(&'a self, n: u64) -> Self {
        Path::Num(n, self)
    }

    pub fn str(&'a self, s: &'a str) -> Self {
        Path::Str(s, self)
    }

    pub fn bytes(&'a self, b: &'a [u8]) -> Self {
        Path::Bytes(b, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CborBuilder, Encoder};

    #[test]
    fn must_index() {
        let cbor = CborBuilder::new().encode_array(|b| {
            b.encode_dict(|b| {
                b.with_cbor_key(|b| b.encode_str("42"), |b| b.encode_str("buh"));
            });
        });

        let found = cbor.index_path(path().num(0).str("42")).unwrap();

        assert_eq!(found.as_str(), Some("buh"));
    }
}
