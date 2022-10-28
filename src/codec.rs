//! This module is experimental!

use super::TypeError;
use crate::{Cbor, Encoder, ItemKind, TaggedItem, Writer};
use std::{borrow::Cow, collections::BTreeMap, error::Error};

#[derive(Debug)]
pub enum CodecError {
    TypeError(TypeError),
    TupleSize { expected: usize, found: usize },
    Custom(Box<dyn Error + Send + Sync>),
    String(String),
}

impl CodecError {
    pub fn type_error(target: &'static str, item: &TaggedItem<'_>) -> Self {
        Self::TypeError(TypeError {
            target,
            kind: item.kind().into(),
            tags: item.tags().into(),
        })
    }

    pub fn tuple_size(expected: usize, found: usize) -> Self {
        Self::TupleSize { expected, found }
    }

    pub fn custom(err: impl Error + Send + Sync + 'static) -> Self {
        Self::Custom(Box::new(err))
    }

    pub fn str(err: impl Into<String>) -> Self {
        Self::String(err.into())
    }
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::TypeError(e) => write!(f, "{}", e),
            CodecError::TupleSize { expected, found } => write!(
                f,
                "wrong tuple size: expected {}, found {}",
                expected, found
            ),
            CodecError::Custom(err) => write!(f, "codec error: {}", err),
            CodecError::String(err) => write!(f, "codec error: {}", err),
        }
    }
}
impl Error for CodecError {}

impl From<TypeError> for CodecError {
    fn from(te: TypeError) -> Self {
        Self::TypeError(te)
    }
}

pub type Result<T> = std::result::Result<T, CodecError>;

pub trait WriteCbor {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output;
}

pub trait ReadCbor {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result;

    fn name() -> String {
        let mut s = String::new();
        Self::fmt(&mut s).unwrap();
        s
    }

    fn read_cbor(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized;
}

pub trait ReadCborBorrowed<'a>: ToOwned + 'a {
    fn read_cbor_borrowed(cbor: &'a Cbor) -> Result<Cow<'a, Self>>;
}

impl<T: WriteCbor> WriteCbor for Vec<T> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_array(|mut b| {
            for item in self {
                b = item.write_cbor(b);
            }
        })
    }
}

impl<T: ReadCbor> ReadCbor for Vec<T> {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "Vec<")?;
        T::fmt(f)?;
        write!(f, ">")?;
        Ok(())
    }

    fn read_cbor(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        let d = cbor.decode();
        let a = d
            .as_array()
            .ok_or_else(|| CodecError::type_error("Vec", &cbor.tagged_item()))?;
        let mut v = Vec::with_capacity(a.len());
        for item in a {
            v.push(T::read_cbor(item.as_ref())?);
        }
        Ok(v)
    }
}

impl WriteCbor for Vec<u8> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_bytes(self)
    }
}

impl WriteCbor for [u8] {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_bytes(self)
    }
}

impl<'a> ReadCborBorrowed<'a> for [u8] {
    fn read_cbor_borrowed(cbor: &'a Cbor) -> Result<Cow<'a, Self>> {
        cbor.decode()
            .to_bytes()
            .ok_or_else(|| CodecError::type_error("byte slice", &cbor.tagged_item()))
    }
}

impl ReadCbor for Vec<u8> {
    fn read_cbor(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(<[u8]>::read_cbor_borrowed(cbor)?.into_owned())
    }

    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "Vec<u8>")
    }
}

impl WriteCbor for String {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_str(self)
    }
}

impl<'a> WriteCbor for &'a str {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_str(self)
    }
}

impl<'a> ReadCborBorrowed<'a> for str {
    fn read_cbor_borrowed(cbor: &'a Cbor) -> Result<Cow<'a, Self>> {
        cbor.decode()
            .to_str()
            .ok_or_else(|| CodecError::type_error("String", &cbor.tagged_item()))
    }
}

impl ReadCbor for String {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "String")
    }

    fn read_cbor(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(str::read_cbor_borrowed(cbor)?.into_owned())
    }
}

impl<T: WriteCbor> WriteCbor for Option<T> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        if let Some(this) = self {
            this.write_cbor(w)
        } else {
            w.encode_null()
        }
    }
}

impl<T: ReadCbor> ReadCbor for Option<T> {
    fn read_cbor(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        if let ItemKind::Null = cbor.tagged_item().kind() {
            Ok(None)
        } else {
            Ok(Some(T::read_cbor(cbor)?))
        }
    }

    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "Option<")?;
        T::fmt(f)?;
        write!(f, ">")?;
        Ok(())
    }
}

impl<K: WriteCbor, V: WriteCbor> WriteCbor for BTreeMap<K, V> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_dict(|w| {
            for (k, v) in self {
                w.with_cbor_key(|w| k.write_cbor(w), |w| v.write_cbor(w));
            }
        })
    }
}

impl<K: ReadCbor + Ord, V: ReadCbor> ReadCbor for BTreeMap<K, V> {
    fn read_cbor(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        let mut map = BTreeMap::new();
        for (k, v) in cbor
            .decode()
            .to_dict()
            .ok_or_else(|| CodecError::type_error("BTreeMap", &cbor.tagged_item()))?
        {
            map.insert(K::read_cbor(k.as_ref())?, V::read_cbor(v.as_ref())?);
        }
        Ok(map)
    }

    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "BTreeMap<")?;
        K::fmt(f)?;
        write!(f, ", ")?;
        V::fmt(f)?;
        write!(f, ">")?;
        Ok(())
    }
}

impl WriteCbor for u64 {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_u64(*self)
    }
}

impl ReadCbor for u64 {
    fn read_cbor(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        let item = cbor.tagged_item();
        match item.kind() {
            ItemKind::Pos(x) => Ok(x),
            _ => Err(CodecError::type_error("u64", &item)),
        }
    }

    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "u64")
    }
}

macro_rules! tuple {
    ($($t:ident),+) => {
        impl<$($t: WriteCbor),*> WriteCbor for ($($t),*) {
            #[allow(unused_assignments, non_snake_case)]
            fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
                w.encode_array(|mut w| {
                    let ($($t),*) = self;
                    $(w = $t.write_cbor(w);)*
                })
            }
        }
        impl<$($t: ReadCbor),*> ReadCbor for ($($t),*) {
            #[allow(unused_assignments, non_snake_case)]
            fn read_cbor(cbor: &Cbor) -> Result<Self> {
                let d = cbor.decode().to_array().ok_or_else(|| CodecError::type_error("tuple", &cbor.tagged_item()))?;
                let len = $({const $t: usize = 1; $t}+)* 0;
                if d.len() < len {
                    return Err(CodecError::tuple_size(len, d.len()));
                }
                let mut idx = 0;
                $(
                    let $t = $t::read_cbor(d[idx].as_ref())?;
                    idx += 1;
                )*
                Ok(($($t),*))
            }

            fn fmt(f: &mut impl ::std::fmt::Write) -> std::fmt::Result {
                write!(f, "(")?;
                $(
                    $t::fmt(f)?;
                    write!(f, ", ")?;
                )*
                write!(f, ")")?;
                Ok(())
            }
        }
    };
}

tuple!(T0, T1);
tuple!(T0, T1, T2);
tuple!(T0, T1, T2, T3);
tuple!(T0, T1, T2, T3, T4);
tuple!(T0, T1, T2, T3, T4, T5);
tuple!(T0, T1, T2, T3, T4, T5, T6);
tuple!(T0, T1, T2, T3, T4, T5, T6, T7);
tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8);
tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);

impl<T: ?Sized + WriteCbor> WriteCbor for &T {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        (*self).write_cbor(w)
    }
}

#[macro_export]
macro_rules! cbor_via {
    ($t:ty => $u:ty: |$x:pat| -> $xx:expr, |$y:pat| -> $yy:expr) => {
        impl $crate::codec::WriteCbor for $t {
            fn write_cbor<W: $crate::Writer>(&self, w: W) -> W::Output {
                let $x: &$t = self;
                let u = $xx;
                $crate::codec::WriteCbor::write_cbor(&u, w)
            }
        }
        impl $crate::codec::ReadCbor for $t {
            fn read_cbor(cbor: &$crate::Cbor) -> $crate::codec::Result<Self>
            where
                Self: Sized,
            {
                let $y = <$u>::read_cbor(cbor)?;
                $yy
            }

            fn fmt(f: &mut impl ::std::fmt::Write) -> std::fmt::Result {
                write!(f, stringify!($t))
            }
        }
    };
    ($t:ty => $u:ty: INTO, $($rest:tt)*) => {
        cbor_via!($t => $u: |x| -> <$u>::from(x), $($rest)*);
    };
    ($t:ty => $u:ty: |$x:ident| -> $xx:expr, FROM) => {
        cbor_via!($t => $u: |$x| -> $xx, |x| -> Ok(x.into()));
    };
    ($t:ty => $u:ty) => {
        cbor_via!($t => $u: INTO, FROM);
    };
}

#[cfg(feature = "libipld14")]
mod impl_libipld14 {
    use super::*;
    use libipld14::{
        cbor::DagCborCodec,
        prelude::{Codec, Encode},
        store::StoreParams,
        Block, Cid, Ipld,
    };
    use smallvec::SmallVec;

    impl WriteCbor for Cid {
        fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
            let mut bytes = SmallVec::<[u8; 128]>::new();
            self.write_bytes(&mut bytes).expect("writing to SmallVec");
            w.write_bytes_chunked([&[0][..], &*bytes], [42])
        }
    }

    impl ReadCbor for Cid {
        fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
            write!(f, "Cid")
        }

        fn read_cbor(cbor: &Cbor) -> Result<Self>
        where
            Self: Sized,
        {
            let decoded = cbor.tagged_item();
            if let (Some(42), ItemKind::Bytes(b)) = (decoded.tags().single(), decoded.kind()) {
                let b = b.as_cow();
                if b.is_empty() {
                    Err(CodecError::str("Cid cannot be empty"))
                } else if b[0] != 0 {
                    Err(CodecError::str("Cid must use identity encoding"))
                } else {
                    Cid::read_bytes(&b[1..]).map_err(CodecError::custom)
                }
            } else {
                Err(CodecError::type_error("Cid", &decoded))
            }
        }
    }

    impl<S: StoreParams> WriteCbor for Block<S> {
        fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
            (self.cid(), self.data()).write_cbor(w)
        }
    }

    impl<S: StoreParams> ReadCbor for Block<S> {
        fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
            write!(f, "Block")
        }

        fn read_cbor(cbor: &Cbor) -> Result<Self>
        where
            Self: Sized,
        {
            let (cid, data) = <(Cid, Vec<u8>)>::read_cbor(cbor)?;
            Self::new(cid, data).map_err(|err| CodecError::str(err.to_string()))
        }
    }

    impl WriteCbor for Ipld {
        fn write_cbor<W: Writer>(&self, mut w: W) -> W::Output {
            w.bytes(|b| self.encode(DagCborCodec, b))
                .expect("WriteCbor for Ipld");
            w.into_output()
        }
    }

    impl ReadCbor for Ipld {
        fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
            write!(f, "Ipld")
        }

        fn read_cbor(cbor: &Cbor) -> Result<Self>
        where
            Self: Sized,
        {
            DagCborCodec
                .decode(cbor.as_slice())
                .map_err(|err| CodecError::Custom(err.into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CborBuilder;
    use std::convert::TryFrom;

    #[derive(Debug, PartialEq)]
    struct X(u64);
    impl From<u64> for X {
        fn from(x: u64) -> Self {
            X(x)
        }
    }
    impl From<&X> for u64 {
        fn from(x: &X) -> Self {
            x.0
        }
    }
    mod priv1 {
        use super::X;
        cbor_via!(X => u64);
    }

    #[derive(Debug)]
    struct Z;
    impl std::fmt::Display for Z {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Z")
        }
    }
    impl Error for Z {}
    #[derive(Debug, PartialEq)]
    struct Y(u64);
    impl TryFrom<u64> for Y {
        type Error = Z;
        fn try_from(y: u64) -> std::result::Result<Self, Z> {
            Ok(Y(y))
        }
    }
    mod priv2 {
        use crate::codec::CodecError;
        use std::convert::TryInto;

        cbor_via!(super::Y => u64: |x| -> x.0, |x| -> x.try_into().map_err(CodecError::custom));
    }

    #[test]
    fn via() {
        assert_eq!(X::name(), "X");
        let bytes = X(5).write_cbor(CborBuilder::default());
        let x = X::read_cbor(&*bytes).unwrap();
        assert_eq!(x, X(5));

        assert_eq!(Y::name(), "super::Y");
        let bytes = Y(5).write_cbor(CborBuilder::default());
        let y = Y::read_cbor(&*bytes).unwrap();
        assert_eq!(y, Y(5));
    }

    #[test]
    fn tuple() {
        assert_eq!(<(String, u64)>::name(), "(String, u64, )");
        let bytes = ("hello".to_owned(), 42u64).write_cbor(CborBuilder::default());
        let tuple = <(String, u64)>::read_cbor(&*bytes).unwrap();
        assert_eq!(tuple, ("hello".to_owned(), 42u64));
    }

    #[test]
    fn vec() {
        assert_eq!(<Vec<String>>::name(), "Vec<String>");
        let x = vec!["hello".to_owned(), "world".to_owned()];
        let bytes = x.write_cbor(CborBuilder::default());
        let vec = <Vec<String>>::read_cbor(&*bytes).unwrap();
        assert_eq!(vec, x);
    }

    #[test]
    fn option() {
        assert_eq!(<Option<String>>::name(), "Option<String>");
        let x = Some("hello".to_owned());
        let bytes = x.write_cbor(CborBuilder::default());
        let opt = <Option<String>>::read_cbor(&*bytes).unwrap();
        assert_eq!(opt, x);

        let x = None;
        let bytes = x.write_cbor(CborBuilder::default());
        let opt = <Option<String>>::read_cbor(&*bytes).unwrap();
        assert_eq!(opt, x);
    }

    #[test]
    fn int() {
        assert_eq!(u64::name(), "u64");
        let bytes = 42u64.write_cbor(CborBuilder::default());
        let x = u64::read_cbor(&*bytes).unwrap();
        assert_eq!(x, 42);
    }
}
