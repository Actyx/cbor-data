use crate::{constants::*, Cbor, CborBuilder, CborOwned, ItemKind::*, PathElement, Writer};
use std::{
    borrow::Cow,
    str::{from_utf8, Split},
};

pub struct IndexIter<'a> {
    iter: Split<'a, char>,
}

impl<'a> IndexIter<'a> {
    pub fn new(s: &'a str) -> Self {
        Self { iter: s.split('.') }
    }
}

impl<'a> Iterator for IndexIter<'a> {
    type Item = PathElement<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let s = self.iter.next()?;
        s.parse::<u64>()
            .map(PathElement::Number)
            .ok()
            .or(Some(PathElement::String(Cow::Borrowed(s))))
    }
}

fn i(s: &str) -> IndexIter<'_> {
    IndexIter::new(s)
}

macro_rules! eq {
    ($i:ident, $($t:expr);*, $p:pat_param $(if $guard:expr)?) => {{
        let ti = $i.tagged_item();
        let tags = ti.tags().collect::<Vec<_>>();
        let tags_exp: Vec<u64> = vec![$($t),*];
        assert!(matches!(ti.kind(), $p $(if $guard)?), " with item {:?}", ti);
        assert_eq!(tags, tags_exp);
    }};
    ($i:expr, None) => {{
        assert_eq!($i, None);
    }};
    ($i:expr, $($t:expr);*, $p:pat_param $(if $guard:expr)?) => {{
        let i = $i.unwrap();
        let ti = i.tagged_item();
        let tags = ti.tags().collect::<Vec<_>>();
        let tags_exp: Vec<u64> = vec![$($t),*];
        assert!(matches!(ti.kind(), $p $(if $guard)?), " with item {:?}", ti);
        assert_eq!(tags, tags_exp);
    }};
}

#[test]
fn roundtrip_simple() {
    let pos = CborBuilder::new().write_pos(42, Some(56));
    eq!(pos, 56, Pos(42));

    let neg = CborBuilder::new().write_neg(42, Some(56));
    eq!(neg, 56, Neg(42));

    let bool = CborBuilder::new().write_bool(true, None);
    eq!(bool, , Bool(true));

    let null = CborBuilder::new().write_null(Some(314));
    eq!(null, 314, Null);

    let string = CborBuilder::new().write_str("huhu", vec![TAG_CBOR_MARKER, 42]);
    eq!(string, 55799;42, Str(s) if s == "huhu");

    let bytes = CborBuilder::new().write_bytes(b"abcd", None);
    eq!(bytes, , Bytes(b) if b == b"abcd");
}

#[test]
fn roundtrip_complex() {
    let complex = CborBuilder::new().write_array(Some(TAG_BIGDECIMAL), |b| {
        b.write_pos(5, None);
        b.write_dict(None, |b| {
            b.with_key("a", |b| b.write_neg(666, None));
            b.with_key("b", |b| b.write_bytes(b"defdef", None));
        });
        b.write_array(None, |b| {
            b.write_bool(false, None);
            b.write_str("hello", None);
        });
        b.write_null(Some(12345));
    });

    assert_eq!(
        complex.to_string(),
        r#"4([5, {"a": -667, "b": h'646566646566'}, [false, "hello"], 12345(null)])"#
    );

    eq!(complex.index([]), TAG_BIGDECIMAL, Array(_));
    eq!(complex.index(i("a")), None);
    eq!(complex.index(i("0")), , Pos(5));
    eq!(complex.index(i("1")), , Dict(_));
    eq!(complex.index(i("1.a")), , Neg(666));
    eq!(complex.index(i("1.b")), , Bytes(b) if b == b"defdef");
    eq!(complex.index(i("2")), , Array(_));
    eq!(complex.index(i("2.0")), , Bool(false));
    eq!(complex.index(i("2.1")), , Str(s) if s == "hello");
    eq!(complex.index(i("3")), 12345, Null);
}

#[test]
fn canonical() {
    let bytes = vec![
        0xc4u8, 0x84, 5, 0xa2, 0x61, b'a', 0x39, 2, 154, 0x61, b'b', 0x46, b'd', b'e', b'f', b'd',
        b'e', b'f', 0x82, 0xf4, 0x65, b'h', b'e', b'l', b'l', b'o', 0xd9, 48, 57, 0xf6,
    ];
    let complex = CborOwned::canonical(bytes).unwrap();

    assert_eq!(
        complex.to_string(),
        r#"4([5, {"a": -667, "b": h'646566646566'}, [false, "hello"], 12345(null)])"#
    );

    eq!(complex.index(i("a")), None);
    eq!(complex.index(i("0")), , Pos(5));
    eq!(complex.index(i("1")), , Dict(_));
    eq!(complex.index(i("1.a")), , Neg(666));
    eq!(complex.index(i("1.b")), , Bytes(b) if b == b"defdef");
    eq!(complex.index(i("2")), , Array(_));
    eq!(complex.index(i("2.0")), , Bool(false));
    eq!(complex.index(i("2.1")), , Str(s) if s == "hello");
    eq!(complex.index(i("3")), 12345, Null);
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Test cases below taken from [RFC 8949 Appendix A](https://www.rfc-editor.org/rfc/rfc8949#appendix-A)
///////////////////////////////////////////////////////////////////////////////////////////////////

fn hex(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    for b in s.as_bytes().chunks(2) {
        v.push(u8::from_str_radix(from_utf8(b).unwrap(), 16).unwrap());
    }
    v
}

fn str_to_cbor(s: &str, trusting: bool) -> CborOwned {
    let v = hex(s);
    if trusting {
        Cbor::checked(&*v).unwrap().to_owned()
    } else {
        CborOwned::canonical(v).unwrap()
    }
}

macro_rules! c {
    ($bytes:literal => ($($t:expr),*) Bytes($b:expr) => $s:literal $(($canonical:literal))?) => {
        c!($bytes => ($($t),*) Bytes(b) if b == $b => $s $(($canonical))?)
    };
    ($bytes:literal => ($($t:expr),*) Str($b:expr) => $s:literal $(($canonical:literal))?) => {
        c!($bytes => ($($t),*) Str(b) if b == $b => $s $(($canonical))?)
    };
    ($bytes:literal => ($($t:expr),*) Float(NaN) => $s:literal $(($canonical:literal))?) => {
        c!($bytes => ($($t),*) Float(b) if b.is_nan() => $s $(($canonical))?)
    };
    ($bytes:literal => ($($t:expr),*) Float($n:expr) => $s:literal $(($canonical:literal))?) => {
        c!($bytes => ($($t),*) Float(b) if b == $n => $s $(($canonical))?)
    };
    ($bytes:literal => ($($t:expr),*) $p:pat_param $(if $guard:expr)? => $s:literal $(($canonical:literal))?) => {{
        let cbor = str_to_cbor($bytes, true);
        let item = cbor.tagged_item();
        let tags_exp: Vec<u64> = vec![$($t),*];
        assert!(matches!(item.kind(), $p $(if $guard)?), " with item {:?}", item);
        assert_eq!(item.tags().collect::<Vec<_>>(), tags_exp);
        assert_eq!(cbor.to_string(), $s);
        assert_eq!(str_to_cbor($bytes, false).to_string(), c!(@ $($canonical)? $s));
    }};
    (@ $one:literal $($two:literal)?) => { $one }
}

#[test]
#[allow(clippy::float_cmp)]
#[allow(clippy::excessive_precision)]
fn numbers() {
    c!("00" => () Pos(0) => "0");
    c!("01" => () Pos(1) => "1");
    c!("0a" => () Pos(10) => "10");
    c!("17" => () Pos(23) => "23");
    c!("1818" => () Pos(24) => "24");
    c!("1819" => () Pos(25) => "25");
    c!("1864" => () Pos(100) => "100");
    c!("1903e8" => () Pos(1000) => "1000");
    c!("1a000f4240" => () Pos(1000000) => "1000000");
    c!("1b000000e8d4a51000" => () Pos(1000000000000) => "1000000000000");
    c!("1bffffffffffffffff" => () Pos(18446744073709551615) => "18446744073709551615");
    c!("c249010000000000000000" => (2) Bytes(hex("010000000000000000")) => "2(h'010000000000000000')");
    c!("3bffffffffffffffff" => () Neg(18446744073709551615) => "-18446744073709551616");
    c!("c349010000000000000000" => (3) Bytes(hex("010000000000000000")) => "3(h'010000000000000000')");
    c!("20" => () Neg(0) => "-1");
    c!("29" => () Neg(9) => "-10");
    c!("3863" => () Neg(99) => "-100");
    c!("3903e7" => () Neg(999) => "-1000");
    c!("f90000" => () Float(0.0) => "0.0");
    c!("f98000" => () Float(-0.0) => "-0.0");
    c!("f93c00" => () Float(1.0) => "1.0");
    c!("fb3ff199999999999a" => () Float(1.1) => "1.1");
    c!("f93e00" => () Float(1.5) => "1.5");
    c!("f97bff" => () Float(65504.0) => "65504.0");
    c!("fa47c35000" => () Float(100000.0) => "100000.0");
    // https://github.com/rust-lang/rust/pull/86479/files
    c!("fa7f7fffff" => () Float(3.4028234663852886e+38) => "3.4028234663852886e38");
    c!("fb7e37e43c8800759c" => () Float(1.0e+300) => "1.0e300");
    c!("f90001" => () Float(5.960464477539063e-8) => "5.960464477539063e-8");
    c!("f90400" => () Float(0.00006103515625) => "0.00006103515625");
    c!("f9c400" => () Float(-4.0) => "-4.0");
    c!("fbc010666666666666" => () Float(-4.1) => "-4.1");
    c!("f97c00" => () Float(f64::INFINITY) => "Infinity");
    c!("f97e00" => () Float(NaN) => "NaN");
    c!("f9fc00" => () Float(f64::NEG_INFINITY) => "-Infinity");
    c!("fa7f800000" => () Float(f64::INFINITY) => "Infinity");
    c!("fa7fc00000" => () Float(NaN) => "NaN");
    c!("faff800000" => () Float(f64::NEG_INFINITY) => "-Infinity");
    c!("fb7ff0000000000000" => () Float(f64::INFINITY) => "Infinity");
    c!("fb7ff8000000000000" => () Float(NaN) => "NaN");
    c!("fbfff0000000000000" => () Float(f64::NEG_INFINITY) => "-Infinity");
}

#[test]
fn simple() {
    c!("f4" => () Bool(false) => "false");
    c!("f5" => () Bool(true) => "true");
    c!("f6" => () Null => "null");
    c!("f7" => () Undefined => "undefined");
    c!("f0" => () Simple(16) => "simple(16)");
    c!("f818" => () Simple(24) => "simple(24)");
    c!("f8ff" => () Simple(255) => "simple(255)");
}

#[test]
#[allow(clippy::float_cmp)]
fn tags() {
    c!("c074323031332d30332d32315432303a30343a30305a" => (0) Str("2013-03-21T20:04:00Z") => "0(\"2013-03-21T20:04:00Z\")");
    c!("c11a514b67b0" => (1) Pos(1363896240) => "1(1363896240)");
    c!("c1fb41d452d9ec200000" => (1) Float(1363896240.5) => "1(1363896240.5)");
    c!("d74401020304" => (23) Bytes([1, 2, 3, 4]) => "23(h'01020304')");
    c!("d818456449455446" => (24) Bytes(b"dIETF") => "<\"IETF\">" ("\"IETF\""));
    c!("d82076687474703a2f2f7777772e6578616d706c652e636f6d" => (32) Str("http://www.example.com") => "32(\"http://www.example.com\")");
}

#[test]
fn bytes() {
    c!("40" => () Bytes(b"") => "h''");
    c!("4401020304" => () Bytes([1, 2, 3, 4]) => "h'01020304'");
    c!("5f42010243030405ff" => () Bytes([1, 2, 3, 4, 5]) => "(_ h'0102', h'030405')" ("h'0102030405'"));
}

#[test]
fn strings() {
    c!("60" => () Str("") => "\"\"");
    c!("6161" => () Str("a") => "\"a\"");
    c!("6449455446" => () Str("IETF") => "\"IETF\"");
    c!("62225c" => () Str("\"\\") => r#""\"\\""#);
    c!("62c3bc" => () Str("\u{00fc}") => "\"\u{00fc}\"");
    c!("63e6b0b4" => () Str("\u{6c34}") => "\"\u{6c34}\"");
    c!("64f0908591" => () Str("\u{10151}") => "\"\u{10151}\"");
    c!("7f657374726561646d696e67ff" => () Str("streaming") => "(_ \"strea\", \"ming\")" ("\"streaming\""));
}

#[test]
fn object() {
    c!("80" => () Array(a) if a.count() == 0 => "[]");
    c!("80" => () Array(a) if a.count() == 0 => "[]");
    c!("83010203" => () Array(a) if a.count() == 3 => "[1, 2, 3]");
    c!("8301820203820405" => () Array(a) if a.count() == 3 => "[1, [2, 3], [4, 5]]");
    c!("98190102030405060708090a0b0c0d0e0f101112131415161718181819" => () Array(a) if a.count() == 25 =>
        "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]"
        ("[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]"));

    c!("a0" => () Dict(d) if d.count() == 0 => "{}");
    c!("a201020304" => () Dict(d) if d.count() == 2 => r#"{1: 2, 3: 4}"#);
    c!("a26161016162820203" => () Dict(d) if d.count() == 2 => r#"{"a": 1, "b": [2, 3]}"#);
    c!("826161a161626163" => () Array(d) if d.count() == 2 => r#"["a", {"b": "c"}]"#);
    c!("a56161614161626142616361436164614461656145" => () Dict(d) if d.count() == 5 =>
        r#"{"a": "A", "b": "B", "c": "C", "d": "D", "e": "E"}"#);

    c!("9fff" => () Array(a) if a.count() == 0 => "[_ ]" ("[]"));
    c!("9f018202039f0405ffff" => () Array(a) if a.count() == 3 => "[_ 1, [2, 3], [_ 4, 5]]" ("[1, [2, 3], [4, 5]]"));
    c!("9f01820203820405ff" => () Array(a) if a.count() == 3 => "[_ 1, [2, 3], [4, 5]]" ("[1, [2, 3], [4, 5]]"));
    c!("83018202039f0405ff" => () Array(a) if a.count() == 3 => "[1, [2, 3], [_ 4, 5]]" ("[1, [2, 3], [4, 5]]"));
    c!("83019f0203ff820405" => () Array(a) if a.count() == 3 => "[1, [_ 2, 3], [4, 5]]" ("[1, [2, 3], [4, 5]]"));
    c!("9f0102030405060708090a0b0c0d0e0f101112131415161718181819ff" => () Array(a) if a.count() == 25 =>
        "[_ 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]"
        ("[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]"));

    c!("bf61610161629f0203ffff" => () Dict(d) if d.count() == 2 => r#"{_ "a": 1, "b": [_ 2, 3]}"# (r#"{"a": 1, "b": [2, 3]}"#));
    c!("826161bf61626163ff" => () Array(d) if d.count() == 2 => r#"["a", {_ "b": "c"}]"# (r#"["a", {"b": "c"}]"#));
    c!("bf6346756ef563416d7421ff" => () Dict(d) if d.count() == 2 => r#"{_ "Fun": true, "Amt": -2}"# (r#"{"Fun": true, "Amt": -2}"#));
}

#[test]
fn roundtrip_non_string_map() {
    let complex = CborBuilder::new().write_array(Some(TAG_BIGDECIMAL), |b| {
        b.write_pos(5, None);
        b.write_dict(None, |b| {
            b.with_cbor_key(|b| b.write_str("1", None), |b| b.write_neg(666, None));
            b.with_cbor_key(
                |b| b.write_pos(2, Some(1)),
                |b| b.write_bytes(b"defdef", None),
            );
        });
        b.write_array(None, |b| {
            b.write_bool(false, None);
            b.write_str("hello", None);
        });
        b.write_null(Some(12345));
    });
    assert_eq!(
        complex.to_string(),
        "4([5, {\"1\": -667, 1(2): h'646566646566'}, [false, \"hello\"], 12345(null)])"
    );
}
