//! This module is experimental!

use super::TypeError;
use crate::{value::Number, Cbor, CborOwned, Encoder, ItemKind, TaggedItem, Writer};
use std::{
    any::TypeId,
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    convert::TryInto,
    error::Error,
    hash::Hash,
};

#[cfg(feature = "derive")]
pub use cbor_data_derive::{ReadCbor, WriteCbor};

#[derive(Debug)]
pub enum CodecError {
    TypeError(TypeError),
    WrongNumber(&'static str),
    TupleSize {
        expected: usize,
        found: usize,
    },
    NoKnownVariant {
        known: &'static [&'static str],
        present: Vec<String>,
    },
    MissingField(&'static str),
    Custom(Box<dyn Error + Send + Sync>),
    String(String),
    WithContext(String, Box<CodecError>),
}

impl PartialEq for CodecError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::TypeError(l0), Self::TypeError(r0)) => l0 == r0,
            (
                Self::TupleSize {
                    expected: l_expected,
                    found: l_found,
                },
                Self::TupleSize {
                    expected: r_expected,
                    found: r_found,
                },
            ) => l_expected == r_expected && l_found == r_found,
            (Self::WrongNumber(l0), Self::WrongNumber(r0)) => l0 == r0,
            (
                Self::NoKnownVariant {
                    known: l_known,
                    present: l_present,
                },
                Self::NoKnownVariant {
                    known: r_known,
                    present: r_present,
                },
            ) => l_known == r_known && l_present == r_present,
            (Self::MissingField(l0), Self::MissingField(r0)) => l0 == r0,
            (Self::Custom(l0), Self::Custom(r0)) => l0.to_string() == r0.to_string(),
            (Self::String(l0), Self::String(r0)) => l0 == r0,
            (Self::WithContext(l0, l1), Self::WithContext(r0, r1)) => l0 == r0 && l1 == r1,
            _ => false,
        }
    }
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

    pub fn with_ctx(self, f: impl FnOnce(&mut String)) -> Self {
        match self {
            Self::WithContext(mut ctx, err) => {
                ctx.push_str(" <- ");
                f(&mut ctx);
                Self::WithContext(ctx, err)
            }
            err => {
                let mut ctx = String::new();
                f(&mut ctx);
                Self::WithContext(ctx, Box::new(err))
            }
        }
    }
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::TypeError(e) => write!(f, "{}", e),
            CodecError::WrongNumber(s) => write!(f, "wrong number format (found {})", s),
            CodecError::TupleSize { expected, found } => write!(
                f,
                "wrong tuple size: expected {}, found {}",
                expected, found
            ),
            CodecError::NoKnownVariant { known, present } => {
                write!(f, "unknown variant: known [")?;
                for (idx, k) in known.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", k)?;
                }
                write!(f, "], present [")?;
                for (idx, p) in present.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, "]")?;
                Ok(())
            }
            CodecError::MissingField(name) => write!(f, "missing field `{}`", name),
            CodecError::Custom(err) => write!(f, "codec error: {}", err),
            CodecError::String(err) => write!(f, "codec error: {}", err),
            CodecError::WithContext(ctx, err) => write!(f, "error decoding {}: {}", ctx, err),
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
        Self: Sized,
    {
        Self::read_cbor_impl(cbor).map_err(|err| {
            err.with_ctx(|ctx| {
                Self::fmt(ctx).ok();
            })
        })
    }

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
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
                item.write_cbor(&mut b);
            }
        })
    }
}

impl<T: WriteCbor> WriteCbor for [T] {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_array(|mut w| {
            for item in self {
                item.write_cbor(&mut w);
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

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        let a = cbor.try_array()?;
        let mut v = Vec::with_capacity(a.len());
        for item in a {
            v.push(T::read_cbor(item.as_ref())?);
        }
        Ok(v)
    }
}

#[repr(transparent)]
pub struct AsByteString<T>(pub T);

impl<T: AsRef<[u8]>> WriteCbor for AsByteString<T> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_bytes(self.0.as_ref())
    }
}

impl<'a> ReadCborBorrowed<'a> for [u8] {
    fn read_cbor_borrowed(cbor: &'a Cbor) -> Result<Cow<'a, Self>> {
        cbor.try_bytes().map_err(Into::into)
    }
}

impl<T: for<'a> From<&'a [u8]> + 'static> ReadCbor for AsByteString<T> {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "AsByteString({:?})", TypeId::of::<T>())
    }

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(AsByteString(T::from(
            <[u8]>::read_cbor_borrowed(cbor)?.as_ref(),
        )))
    }
}

impl<'a, T: ToOwned + WriteCbor> WriteCbor for Cow<'a, T> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        self.as_ref().write_cbor(w)
    }
}

impl<'a, T: ToOwned> ReadCbor for Cow<'a, T>
where
    T::Owned: ReadCbor,
{
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "Cow<")?;
        T::Owned::fmt(f)?;
        write!(f, ">")?;
        Ok(())
    }

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Cow::Owned(ReadCbor::read_cbor(cbor)?))
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
        cbor.try_str().map_err(Into::into)
    }
}

impl ReadCbor for String {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "String")
    }

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
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
    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
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
    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        let mut map = BTreeMap::new();
        for (k, v) in cbor.try_dict()? {
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

impl<K: WriteCbor, V: WriteCbor> WriteCbor for HashMap<K, V> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_dict(|w| {
            for (k, v) in self {
                w.with_cbor_key(|w| k.write_cbor(w), |w| v.write_cbor(w));
            }
        })
    }
}

impl<K: ReadCbor + Hash + Eq, V: ReadCbor> ReadCbor for HashMap<K, V> {
    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        let mut map = HashMap::new();
        for (k, v) in cbor.try_dict()? {
            map.insert(K::read_cbor(k.as_ref())?, V::read_cbor(v.as_ref())?);
        }
        Ok(map)
    }

    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "HashMap<")?;
        K::fmt(f)?;
        write!(f, ", ")?;
        V::fmt(f)?;
        write!(f, ">")?;
        Ok(())
    }
}

impl<K: WriteCbor> WriteCbor for BTreeSet<K> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_array(|mut w| {
            for k in self {
                k.write_cbor(&mut w);
            }
        })
    }
}

impl<K: ReadCbor + Ord> ReadCbor for BTreeSet<K> {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "BTreeSet<")?;
        K::fmt(f)?;
        write!(f, ">")?;
        Ok(())
    }

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        let mut set = Self::new();
        for k in cbor.try_array()? {
            set.insert(K::read_cbor(k.as_ref())?);
        }
        Ok(set)
    }
}

impl<K: WriteCbor> WriteCbor for HashSet<K> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_array(|mut w| {
            for k in self {
                k.write_cbor(&mut w);
            }
        })
    }
}

impl<K: ReadCbor + Hash + Eq> ReadCbor for HashSet<K> {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "HashSet<")?;
        K::fmt(f)?;
        write!(f, ">")?;
        Ok(())
    }

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        let mut set = Self::new();
        for k in cbor.try_array()? {
            set.insert(K::read_cbor(k.as_ref())?);
        }
        Ok(set)
    }
}

impl WriteCbor for i128 {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_number(&Number::Int(*self))
    }
}

impl ReadCbor for i128 {
    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        match cbor.try_number()? {
            Number::Int(x) => Ok(x),
            x => Err(CodecError::WrongNumber(x.get_type())),
        }
    }

    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "i128")
    }
}

impl WriteCbor for f64 {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_number(&Number::IEEE754(*self))
    }
}

impl ReadCbor for f64 {
    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        match cbor.try_number()? {
            Number::IEEE754(x) => Ok(x),
            x => Err(CodecError::WrongNumber(x.get_type())),
        }
    }

    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "f64")
    }
}

impl WriteCbor for Number<'_> {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.encode_number(self)
    }
}

impl ReadCbor for Number<'static> {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "Number")
    }

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(cbor.try_number()?.make_static())
    }
}

impl WriteCbor for Cbor {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.write_trusting(self.as_slice())
    }
}

impl WriteCbor for CborOwned {
    fn write_cbor<W: Writer>(&self, w: W) -> W::Output {
        w.write_trusting(self.as_slice())
    }
}

impl<'a> ReadCborBorrowed<'a> for Cbor {
    fn read_cbor_borrowed(cbor: &'a Cbor) -> Result<Cow<'a, Self>> {
        Ok(Cow::Borrowed(cbor))
    }
}

impl ReadCbor for CborOwned {
    fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "Cbor")
    }

    fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(<Cbor>::read_cbor_borrowed(cbor)?.into_owned())
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
            fn read_cbor_impl(cbor: &Cbor) -> Result<Self> {
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
            fn read_cbor_impl(cbor: &$crate::Cbor) -> $crate::codec::Result<Self>
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

cbor_via!(u64 => i128: |x| -> i128::from(*x), |x| -> x.try_into().map_err(CodecError::custom));
cbor_via!(i64 => i128: |x| -> i128::from(*x), |x| -> x.try_into().map_err(CodecError::custom));
cbor_via!(u32 => i128: |x| -> i128::from(*x), |x| -> x.try_into().map_err(CodecError::custom));
cbor_via!(i32 => i128: |x| -> i128::from(*x), |x| -> x.try_into().map_err(CodecError::custom));
cbor_via!(u16 => i128: |x| -> i128::from(*x), |x| -> x.try_into().map_err(CodecError::custom));
cbor_via!(i16 => i128: |x| -> i128::from(*x), |x| -> x.try_into().map_err(CodecError::custom));
cbor_via!(f32 => f64: |x| -> f64::from(*x), |x| -> Ok(x as f32));

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

        fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
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
            (self.cid(), AsByteString(self.data())).write_cbor(w)
        }
    }

    impl<S: StoreParams> ReadCbor for Block<S> {
        fn fmt(f: &mut impl std::fmt::Write) -> std::fmt::Result {
            write!(f, "Block")
        }

        fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
        where
            Self: Sized,
        {
            let (cid, data) = <(Cid, AsByteString<Vec<u8>>)>::read_cbor(cbor)?;
            Self::new(cid, data.0).map_err(|err| CodecError::str(err.to_string()))
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

        fn read_cbor_impl(cbor: &Cbor) -> Result<Self>
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
