use std::str::from_utf8;

use maplit::btreemap;

use crate::{
    constants::*,
    value::{CborObject, CborValue, ValueKind::*},
    CborBuilder, CborOwned, Tags, ValueKind, Writer,
};

#[test]
fn roundtrip_simple() {
    let pos = CborBuilder::new().write_pos(42, Some(56));
    assert_eq!(pos.value(), Some(CborValue::fake(Some(56), Pos(42))));

    let neg = CborBuilder::new().write_neg(42, Some(56));
    assert_eq!(neg.value(), Some(CborValue::fake(Some(56), Neg(42))));

    let bool = CborBuilder::new().write_bool(true, None);
    assert_eq!(bool.value(), Some(CborValue::fake(None, Bool(true))));

    let null = CborBuilder::new().write_null(Some(314));
    assert_eq!(null.value(), Some(CborValue::fake(Some(314), Null)));

    let string = CborBuilder::new().write_str("huhu", Some(TAG_CBOR_MARKER));
    assert_eq!(
        string.value(),
        Some(CborValue::fake(Some(55799), Str("huhu")))
    );

    let bytes = CborBuilder::new().write_bytes(b"abcd", None);
    assert_eq!(bytes.value(), Some(CborValue::fake(None, Bytes(b"abcd"))));
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
        "4|[5, {\"a\": -667, \"b\": 0x646566646566}, [false, \"hello\"], 12345|null]"
    );

    assert_eq!(
        complex.index(""),
        Some(CborValue::fake(Some(TAG_BIGDECIMAL), Array))
    );
    assert_eq!(complex.index("a"), None);
    assert_eq!(complex.index("0"), Some(CborValue::fake(None, Pos(5))));
    assert_eq!(complex.index("1"), Some(CborValue::fake(None, Dict)));
    assert_eq!(complex.index("1.a"), Some(CborValue::fake(None, Neg(666))));
    assert_eq!(
        complex.index("1.b"),
        Some(CborValue::fake(None, Bytes(b"defdef")))
    );
    assert_eq!(complex.index("2"), Some(CborValue::fake(None, Array)));
    assert_eq!(
        complex.index("2.0"),
        Some(CborValue::fake(None, Bool(false)))
    );
    assert_eq!(
        complex.index("2.1"),
        Some(CborValue::fake(None, Str("hello")))
    );
    assert_eq!(complex.index("3"), Some(CborValue::fake(Some(12345), Null)));
}

#[test]
fn canonical() {
    let bytes = vec![
        0xc4u8, 0x84, 5, 0xa2, 0x61, b'a', 0x39, 2, 154, 0x61, b'b', 0x46, b'd', b'e', b'f', b'd',
        b'e', b'f', 0x82, 0xf4, 0x65, b'h', b'e', b'l', b'l', b'o', 0xd9, 48, 57, 0xf6,
    ];
    let complex = CborOwned::canonical(&*bytes).unwrap();

    assert_eq!(
        complex.to_string(),
        "4|[5, {\"a\": -667, \"b\": 0x646566646566}, [false, \"hello\"], 12345|null]"
    );

    assert_eq!(complex.index("a"), None);
    assert_eq!(complex.index("0"), Some(CborValue::fake(None, Pos(5))));
    assert_eq!(complex.index("1"), Some(CborValue::fake(None, Dict)));
    assert_eq!(complex.index("1.a"), Some(CborValue::fake(None, Neg(666))));
    assert_eq!(
        complex.index("1.b"),
        Some(CborValue::fake(None, Bytes(b"defdef")))
    );
    assert_eq!(complex.index("2"), Some(CborValue::fake(None, Array)));
    assert_eq!(
        complex.index("2.0"),
        Some(CborValue::fake(None, Bool(false)))
    );
    assert_eq!(
        complex.index("2.1"),
        Some(CborValue::fake(None, Str("hello")))
    );
    assert_eq!(complex.index("3"), Some(CborValue::fake(Some(12345), Null)));
}

///////////////////////////////////////////////////////////////////////////////////////////////////
// Test cases below taken from [RFC 7049 Appendix A](https://tools.ietf.org/html/rfc7049#appendix-A)
///////////////////////////////////////////////////////////////////////////////////////////////////

fn str_to_bytes(s: &str) -> Vec<u8> {
    assert_eq!(&s[..2], "0x");
    let mut v = Vec::new();
    for b in s.as_bytes()[2..].chunks(2) {
        v.push(u8::from_str_radix(from_utf8(b).unwrap(), 16).unwrap());
    }
    v
}

fn str_to_cbor(s: &str, trusting: bool) -> CborOwned {
    let v = str_to_bytes(s);
    if trusting {
        CborOwned::trusting(v)
    } else {
        CborOwned::canonical(v).unwrap()
    }
}

fn string(s: &str) -> String {
    str_to_cbor(s, false).to_string()
}

#[test]
#[allow(clippy::float_cmp)]
#[allow(clippy::excessive_precision)]
fn numbers() {
    fn float(s: &str) -> f64 {
        str_to_cbor(s, false).value().unwrap().as_f64().unwrap()
    }

    assert_eq!(float("0x00"), 0f64);
    assert_eq!(string("0x00"), "0");
    assert_eq!(float("0x01"), 1f64);
    assert_eq!(string("0x01"), "1");
    assert_eq!(float("0x0a"), 10f64);
    assert_eq!(string("0x0a"), "10");
    assert_eq!(float("0x17"), 23f64);
    assert_eq!(string("0x17"), "23");
    assert_eq!(float("0x1818"), 24f64);
    assert_eq!(string("0x1818"), "24");
    assert_eq!(float("0x1819"), 25f64);
    assert_eq!(string("0x1819"), "25");
    assert_eq!(float("0x1864"), 100f64);
    assert_eq!(string("0x1864"), "100");
    assert_eq!(float("0x1903e8"), 1000f64);
    assert_eq!(string("0x1903e8"), "1000");
    assert_eq!(float("0x1a000f4240"), 1000000f64);
    assert_eq!(string("0x1a000f4240"), "1000000");
    assert_eq!(float("0x1b000000e8d4a51000"), 1000000000000f64);
    assert_eq!(string("0x1b000000e8d4a51000"), "1000000000000");
    assert_eq!(float("0x1bffffffffffffffff"), 18446744073709551615f64);
    assert_eq!(string("0x1bffffffffffffffff"), "18446744073709551615");
    assert_eq!(float("0xc249010000000000000000"), 18446744073709551616f64);
    // assert_eq!(string("0xc249010000000000000000"), "18446744073709551616");
    assert_eq!(float("0x3bffffffffffffffff"), -18446744073709551616f64);
    // assert_eq!(string("0x3bffffffffffffffff"), "-18446744073709551616");
    assert_eq!(float("0xc349010000000000000000"), -18446744073709551617f64);
    // assert_eq!(string("0xc349010000000000000000"), "-18446744073709551617");
    assert_eq!(float("0x20"), -1f64);
    assert_eq!(string("0x20"), "-1");
    assert_eq!(float("0x29"), -10f64);
    assert_eq!(string("0x29"), "-10");
    assert_eq!(float("0x3863"), -100f64);
    assert_eq!(string("0x3863"), "-100");
    assert_eq!(float("0x3903e7"), -1000f64);
    assert_eq!(string("0x3903e7"), "-1000");
    assert_eq!(float("0xf90000"), 0.0f64);
    assert_eq!(string("0xf90000"), "0.0");
    assert_eq!(float("0xf98000"), -0.0f64);
    assert_eq!(string("0xf98000"), "-0.0");
    assert_eq!(float("0xf93c00"), 1.0f64);
    assert_eq!(string("0xf93c00"), "1.0");
    assert_eq!(float("0xfb3ff199999999999a"), 1.1f64);
    assert_eq!(string("0xfb3ff199999999999a"), "1.1");
    assert_eq!(string("0xf93e00"), "1.5");
    assert_eq!(float("0xf97bff"), 65504.0f64);
    assert_eq!(string("0xf97bff"), "65504.0");
    assert_eq!(float("0xfa47c35000"), 100000.0f64);
    assert_eq!(string("0xfa47c35000"), "100000.0");
    assert_eq!(float("0xfa7f7fffff"), 3.4028234663852886e+38f64);
    assert_eq!(
        string("0xfa7f7fffff"),
        "340282346638528860000000000000000000000.0"
    );
    assert_eq!(float("0xfb7e37e43c8800759c"), 1.0e+300f64);
    assert_eq!(
        string("0xfb7e37e43c8800759c"),
        "10000000000000000000000000000000000000000000000000000000000000000000000\
    00000000000000000000000000000000000000000000000000000000\
    00000000000000000000000000000000000000000000000000000000\
    00000000000000000000000000000000000000000000000000000000\
    00000000000000000000000000000000000000000000000000000000000000.0"
    );
    assert_eq!(float("0xf90001"), 5.960464477539063e-8f64);
    assert_eq!(string("0xf90001"), "0.00000005960464477539063");
    assert_eq!(float("0xf90400"), 0.00006103515625f64);
    assert_eq!(string("0xf90400"), "0.00006103515625");
    assert_eq!(float("0xf9c400"), -4.0f64);
    assert_eq!(string("0xf9c400"), "-4.0");
    assert_eq!(float("0xfbc010666666666666"), -4.1f64);
    assert_eq!(string("0xfbc010666666666666"), "-4.1");
    assert_eq!(float("0xf97c00"), f64::INFINITY);
    assert_eq!(string("0xf97c00"), "inf");
    assert!(f64::is_nan(float("0xf97e00")));
    assert_eq!(string("0xf97e00"), "NaN");
    assert_eq!(float("0xf9fc00"), f64::NEG_INFINITY);
    assert_eq!(string("0xf9fc00"), "-inf");
    assert_eq!(float("0xfa7f800000"), f64::INFINITY);
    assert_eq!(string("0xfa7f800000"), "inf");
    assert!(f64::is_nan(float("0xfa7fc00000")));
    assert_eq!(string("0xfa7fc00000"), "NaN");
    assert_eq!(float("0xfaff800000"), f64::NEG_INFINITY);
    assert_eq!(string("0xfaff800000"), "-inf");
    assert_eq!(float("0xfb7ff0000000000000"), f64::INFINITY);
    assert_eq!(string("0xfb7ff0000000000000"), "inf");
    assert!(f64::is_nan(float("0xfb7ff8000000000000")));
    assert_eq!(string("0xfb7ff8000000000000"), "NaN");
    assert_eq!(float("0xfbfff0000000000000"), f64::NEG_INFINITY);
    assert_eq!(string("0xfbfff0000000000000"), "-inf");
}

#[test]
fn simple() {
    fn kind(s: &str) -> ValueKind {
        match str_to_cbor(s, false).value().unwrap().kind {
            Bool(b) => Bool(b),
            Null => Null,
            Undefined => Undefined,
            Simple(x) => Simple(x),
            _ => panic!(),
        }
    }

    assert_eq!(kind("0xf4"), Bool(false));
    assert_eq!(string("0xf4"), "false");
    assert_eq!(kind("0xf5"), Bool(true));
    assert_eq!(string("0xf5"), "true");
    assert_eq!(kind("0xf6"), Null);
    assert_eq!(string("0xf6"), "null");
    assert_eq!(kind("0xf7"), Undefined);
    assert_eq!(string("0xf7"), "undefined");
    assert_eq!(kind("0xf0"), Simple(16));
    assert_eq!(string("0xf0"), "simple(16)");
    assert_eq!(kind("0xf818"), Simple(24));
    assert_eq!(string("0xf818"), "simple(24)");
    assert_eq!(kind("0xf8ff"), Simple(255));
    assert_eq!(string("0xf8ff"), "simple(255)");
}

#[test]
fn tags() {
    assert_eq!(
        str_to_cbor("0xc074323031332d30332d32315432303a30343a30305a", false)
            .value()
            .unwrap(),
        CborValue::fake(Some(0), Str("2013-03-21T20:04:00Z"))
    );
    assert_eq!(
        string("0xc074323031332d30332d32315432303a30343a30305a"),
        "0|\"2013-03-21T20:04:00Z\""
    );
    assert_eq!(
        str_to_cbor("0xc11a514b67b0", false).value().unwrap(),
        CborValue::fake(Some(1), Pos(1363896240))
    );
    assert_eq!(string("0xc11a514b67b0"), "1|1363896240");
    assert_eq!(
        str_to_cbor("0xc1fb41d452d9ec200000", false)
            .value()
            .unwrap(),
        CborValue::fake(Some(1), Float(1363896240.5))
    );
    assert_eq!(string("0xc1fb41d452d9ec200000"), "1|1363896240.5");
    assert_eq!(
        str_to_cbor("0xd74401020304", false).value().unwrap(),
        CborValue::fake(Some(23), Bytes(&[1, 2, 3, 4]))
    );
    assert_eq!(string("0xd74401020304"), "23|0x01020304");
    assert_eq!(
        str_to_cbor("0xd818456449455446", true).value().unwrap(),
        CborValue::fake(Some(24), Bytes(b"dIETF"))
    );
    assert_eq!(
        str_to_cbor("0xd818456449455446", true).to_string(),
        "24|0x6449455446"
    );
    assert_eq!(
        str_to_cbor(
            "0xd82076687474703a2f2f7777772e6578616d706c652e636f6d",
            false
        )
        .value()
        .unwrap(),
        CborValue::fake(Some(32), Str("http://www.example.com"))
    );
    assert_eq!(
        string("0xd82076687474703a2f2f7777772e6578616d706c652e636f6d",),
        "32|\"http://www.example.com\""
    );
}

#[test]
fn bytes() {
    fn b(s: &str) -> Vec<u8> {
        str_to_cbor(s, false)
            .value()
            .unwrap()
            .as_bytes()
            .unwrap()
            .to_vec()
    }

    assert_eq!(b("0x40"), Vec::<u8>::new());
    assert_eq!(string("0x40"), "0x");
    assert_eq!(b("0x4401020304"), vec![1u8, 2, 3, 4]);
    assert_eq!(string("0x4401020304"), "0x01020304");
    assert_eq!(b("0x5f42010243030405ff"), vec![1u8, 2, 3, 4, 5]);
    assert_eq!(string("0x5f42010243030405ff"), "0x0102030405");
}

#[test]
fn strings() {
    fn s(s: &str) -> String {
        str_to_cbor(s, false)
            .value()
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }

    assert_eq!(s("0x60"), "");
    assert_eq!(string("0x60"), "\"\"");
    assert_eq!(s("0x6161"), "a");
    assert_eq!(string("0x6161"), "\"a\"");
    assert_eq!(s("0x6449455446"), "IETF");
    assert_eq!(string("0x6449455446"), "\"IETF\"");
    assert_eq!(s("0x62225c"), "\"\\");
    assert_eq!(string("0x62225c"), "\"\\\"\\\\\"");
    assert_eq!(s("0x62c3bc"), "\u{00fc}");
    assert_eq!(string("0x62c3bc"), "\"\u{00fc}\"");
    assert_eq!(s("0x63e6b0b4"), "\u{6c34}");
    assert_eq!(string("0x63e6b0b4"), "\"\u{6c34}\"");
    assert_eq!(s("0x64f0908591"), "\u{10151}");
    assert_eq!(string("0x64f0908591"), "\"\u{10151}\"");
    assert_eq!(s("0x7f657374726561646d696e67ff"), "streaming");
    assert_eq!(string("0x7f657374726561646d696e67ff"), "\"streaming\"");
}

#[test]
fn object() {
    use CborObject::*;
    fn o(c: &CborOwned) -> CborObject {
        c.value().unwrap().as_object().unwrap()
    }

    let bytes = str_to_cbor("0x80", false);
    assert_eq!(o(&bytes), Array(vec![]));
    assert_eq!(bytes.to_string(), "[]");
    assert_eq!(bytes.as_slice(), str_to_bytes("0x80"));

    let bytes = str_to_cbor("0x83010203", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(Tags::empty(), Pos(1)),
            Value(Tags::empty(), Pos(2)),
            Value(Tags::empty(), Pos(3))
        ])
    );
    assert_eq!(bytes.to_string(), "[1, 2, 3]");
    assert_eq!(bytes.as_slice(), str_to_bytes("0x83010203"));

    let bytes = str_to_cbor("0x8301820203820405", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(Tags::empty(), Pos(1)),
            Array(vec![
                Value(Tags::empty(), Pos(2)),
                Value(Tags::empty(), Pos(3))
            ]),
            Array(vec![
                Value(Tags::empty(), Pos(4)),
                Value(Tags::empty(), Pos(5))
            ]),
        ])
    );
    assert_eq!(bytes.to_string(), "[1, [2, 3], [4, 5]]");
    assert_eq!(bytes.as_slice(), str_to_bytes("0x8301820203820405"));

    let bytes = str_to_cbor(
        "0x98190102030405060708090a0b0c0d0e0f101112131415161718181819",
        false,
    );
    assert_eq!(
        o(&bytes),
        Array((1u64..26).map(|i| Value(Tags::empty(), Pos(i))).collect())
    );
    assert_eq!(bytes.to_string(), "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]");
    assert_eq!(
        bytes.as_slice(),
        str_to_bytes("0x98190102030405060708090a0b0c0d0e0f101112131415161718181819")
    );

    let bytes = str_to_cbor("0xa0", false);
    assert_eq!(o(&bytes), Dict(btreemap! {}));
    assert_eq!(bytes.to_string(), "{}");
    assert_eq!(bytes.as_slice(), str_to_bytes("0xa0"));

    let bytes = str_to_cbor("0xa201020304", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "1" => Value(Tags::empty(), Pos(2)),
            "3" => Value(Tags::empty(), Pos(4))
        })
    );
    // note that canonicalisation turns all dict keys into strings
    assert_eq!(bytes.to_string(), r#"{"1": 2, "3": 4}"#);
    assert_eq!(bytes.as_slice(), str_to_bytes("0xa2613102613304"));

    let bytes = str_to_cbor("0xa26161016162820203", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "a" => Value(Tags::empty(), Pos(1)),
            "b" => Array(vec![Value(Tags::empty(), Pos(2)), Value(Tags::empty(), Pos(3))])
        })
    );
    assert_eq!(bytes.to_string(), r#"{"a": 1, "b": [2, 3]}"#);
    assert_eq!(bytes.as_slice(), str_to_bytes("0xa26161016162820203"));

    let bytes = str_to_cbor("0x826161a161626163", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(Tags::empty(), Str("a")),
            Dict(btreemap! {
                "b" => Value(Tags::empty(), Str("c"))
            })
        ])
    );
    assert_eq!(bytes.to_string(), r#"["a", {"b": "c"}]"#);
    assert_eq!(bytes.as_slice(), str_to_bytes("0x826161a161626163"));

    let bytes = str_to_cbor("0xa56161614161626142616361436164614461656145", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "a" => Value(Tags::empty(), Str("A")),
            "b" => Value(Tags::empty(), Str("B")),
            "c" => Value(Tags::empty(), Str("C")),
            "d" => Value(Tags::empty(), Str("D")),
            "e" => Value(Tags::empty(), Str("E")),
        })
    );
    assert_eq!(
        bytes.to_string(),
        r#"{"a": "A", "b": "B", "c": "C", "d": "D", "e": "E"}"#
    );
    assert_eq!(
        bytes.as_slice(),
        str_to_bytes("0xa56161614161626142616361436164614461656145")
    );

    let bytes = str_to_cbor("0x9fff", false);
    assert_eq!(o(&bytes), Array(vec![]));
    assert_eq!(bytes.to_string(), "[]");
    assert_eq!(bytes.as_slice(), str_to_bytes("0x80"));

    let bytes = str_to_cbor("0x9f018202039f0405ffff", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(Tags::empty(), Pos(1)),
            Array(vec![
                Value(Tags::empty(), Pos(2)),
                Value(Tags::empty(), Pos(3))
            ]),
            Array(vec![
                Value(Tags::empty(), Pos(4)),
                Value(Tags::empty(), Pos(5))
            ]),
        ])
    );
    assert_eq!(bytes.to_string(), "[1, [2, 3], [4, 5]]");
    assert_eq!(bytes.as_slice(), str_to_bytes("0x8301820203820405"));

    let bytes = str_to_cbor("0x9f01820203820405ff", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(Tags::empty(), Pos(1)),
            Array(vec![
                Value(Tags::empty(), Pos(2)),
                Value(Tags::empty(), Pos(3))
            ]),
            Array(vec![
                Value(Tags::empty(), Pos(4)),
                Value(Tags::empty(), Pos(5))
            ]),
        ])
    );
    assert_eq!(bytes.to_string(), "[1, [2, 3], [4, 5]]");
    assert_eq!(bytes.as_slice(), str_to_bytes("0x8301820203820405"));

    let bytes = str_to_cbor("0x83018202039f0405ff", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(Tags::empty(), Pos(1)),
            Array(vec![
                Value(Tags::empty(), Pos(2)),
                Value(Tags::empty(), Pos(3))
            ]),
            Array(vec![
                Value(Tags::empty(), Pos(4)),
                Value(Tags::empty(), Pos(5))
            ]),
        ])
    );
    assert_eq!(bytes.to_string(), "[1, [2, 3], [4, 5]]");
    assert_eq!(bytes.as_slice(), str_to_bytes("0x8301820203820405"));

    let bytes = str_to_cbor("0x83019f0203ff820405", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(Tags::empty(), Pos(1)),
            Array(vec![
                Value(Tags::empty(), Pos(2)),
                Value(Tags::empty(), Pos(3))
            ]),
            Array(vec![
                Value(Tags::empty(), Pos(4)),
                Value(Tags::empty(), Pos(5))
            ]),
        ])
    );
    assert_eq!(bytes.to_string(), "[1, [2, 3], [4, 5]]");
    assert_eq!(bytes.as_slice(), str_to_bytes("0x8301820203820405"));

    let bytes = str_to_cbor(
        "0x9f0102030405060708090a0b0c0d0e0f101112131415161718181819ff",
        false,
    );
    assert_eq!(
        o(&bytes),
        Array((1u64..26).map(|i| Value(Tags::empty(), Pos(i))).collect())
    );
    assert_eq!(bytes.to_string(), "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]");
    assert_eq!(
        bytes.as_slice(),
        str_to_bytes("0x98190102030405060708090a0b0c0d0e0f101112131415161718181819")
    );

    let bytes = str_to_cbor("0xbf61610161629f0203ffff", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "a" => Value(Tags::empty(), Pos(1)),
            "b" => Array(vec![Value(Tags::empty(), Pos(2)), Value(Tags::empty(), Pos(3))])
        })
    );
    assert_eq!(bytes.to_string(), r#"{"a": 1, "b": [2, 3]}"#);
    assert_eq!(bytes.as_slice(), str_to_bytes("0xa26161016162820203"));

    let bytes = str_to_cbor("0x826161bf61626163ff", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(Tags::empty(), Str("a")),
            Dict(btreemap! {
                "b" => Value(Tags::empty(), Str("c"))
            })
        ])
    );
    assert_eq!(bytes.to_string(), r#"["a", {"b": "c"}]"#);
    assert_eq!(bytes.as_slice(), str_to_bytes("0x826161a161626163"));

    let bytes = str_to_cbor("0xbf6346756ef563416d7421ff", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "Fun" => Value(Tags::empty(), Bool(true)),
            "Amt" => Value(Tags::empty(), Neg(1))
        })
    );
    assert_eq!(bytes.to_string(), r#"{"Fun": true, "Amt": -2}"#);
    assert_eq!(bytes.as_slice(), str_to_bytes("0xa26346756ef563416d7421"));
}
