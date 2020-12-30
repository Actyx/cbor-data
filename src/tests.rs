use std::str::from_utf8;

use maplit::btreemap;

use crate::{
    builder::{WriteToArray, WriteToDict},
    constants::*,
    value::{CborObject, CborValue, ValueKind::*},
    CborBuilder, CborOwned, ValueKind,
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
    let mut array = CborBuilder::new().write_array(Some(TAG_BIGDECIMAL));
    array.write_pos(5, None);

    let mut dict = array.write_dict(None);
    dict.write_neg("a", 666, None);
    dict.write_bytes("b", b"defdef", None);
    let array = dict.finish();

    let mut array2 = array.write_array(None);
    array2.write_bool(false, None);
    array2.write_str("hello", None);
    let mut array = array2.finish();

    array.write_null(Some(12345));

    let complex = array.finish();

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
    let complex = CborOwned::canonical(&*bytes, None).unwrap();

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
// Test cases below taken from [RFC7049 Appendix A](https://tools.ietf.org/html/rfc7049#appendix-A)
///////////////////////////////////////////////////////////////////////////////////////////////////

fn str_to_cbor(s: &str, trusting: bool) -> CborOwned {
    assert_eq!(&s[..2], "0x");
    let mut v = Vec::new();
    for b in s.as_bytes()[2..].chunks(2) {
        v.push(u8::from_str_radix(from_utf8(b).unwrap(), 16).unwrap());
    }
    if trusting {
        CborOwned::trusting(v)
    } else {
        CborOwned::canonical(v, None).unwrap()
    }
}

#[test]
#[allow(clippy::float_cmp)]
#[allow(clippy::excessive_precision)]
fn numbers() {
    fn float(s: &str) -> f64 {
        str_to_cbor(s, false).value().unwrap().as_f64().unwrap()
    }

    assert_eq!(float("0x00"), 0f64);
    assert_eq!(float("0x01"), 1f64);
    assert_eq!(float("0x0a"), 10f64);
    assert_eq!(float("0x17"), 23f64);
    assert_eq!(float("0x1818"), 24f64);
    assert_eq!(float("0x1819"), 25f64);
    assert_eq!(float("0x1864"), 100f64);
    assert_eq!(float("0x1903e8"), 1000f64);
    assert_eq!(float("0x1a000f4240"), 1000000f64);
    assert_eq!(float("0x1b000000e8d4a51000"), 1000000000000f64);
    assert_eq!(float("0x1bffffffffffffffff"), 18446744073709551615f64);
    assert_eq!(float("0xc249010000000000000000"), 18446744073709551616f64);
    assert_eq!(float("0x3bffffffffffffffff"), -18446744073709551616f64);
    assert_eq!(float("0xc349010000000000000000"), -18446744073709551617f64);
    assert_eq!(float("0x20"), -1f64);
    assert_eq!(float("0x29"), -10f64);
    assert_eq!(float("0x3863"), -100f64);
    assert_eq!(float("0x3903e7"), -1000f64);
    assert_eq!(float("0xf90000"), 0.0f64);
    assert_eq!(float("0xf98000"), -0.0f64);
    assert_eq!(float("0xf93c00"), 1.0f64);
    assert_eq!(float("0xfb3ff199999999999a"), 1.1f64);
    assert_eq!(float("0xf93e00"), 1.5f64);
    assert_eq!(float("0xf97bff"), 65504.0f64);
    assert_eq!(float("0xfa47c35000"), 100000.0f64);
    assert_eq!(float("0xfa7f7fffff"), 3.4028234663852886e+38f64);
    assert_eq!(float("0xfb7e37e43c8800759c"), 1.0e+300f64);
    assert_eq!(float("0xf90001"), 5.960464477539063e-8f64);
    assert_eq!(float("0xf90400"), 0.00006103515625f64);
    assert_eq!(float("0xf9c400"), -4.0f64);
    assert_eq!(float("0xfbc010666666666666"), -4.1f64);
    assert_eq!(float("0xf97c00"), f64::INFINITY);
    assert!(f64::is_nan(float("0xf97e00")));
    assert_eq!(float("0xf9fc00"), f64::NEG_INFINITY);
    assert_eq!(float("0xfa7f800000"), f64::INFINITY);
    assert!(f64::is_nan(float("0xfa7fc00000")));
    assert_eq!(float("0xfaff800000"), f64::NEG_INFINITY);
    assert_eq!(float("0xfb7ff0000000000000"), f64::INFINITY);
    assert!(f64::is_nan(float("0xfb7ff8000000000000")));
    assert_eq!(float("0xfbfff0000000000000"), f64::NEG_INFINITY);
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
    assert_eq!(kind("0xf5"), Bool(true));
    assert_eq!(kind("0xf6"), Null);
    assert_eq!(kind("0xf7"), Undefined);
    assert_eq!(kind("0xf0"), Simple(16));
    assert_eq!(kind("0xf818"), Simple(24));
    assert_eq!(kind("0xf8ff"), Simple(255));
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
        str_to_cbor("0xc11a514b67b0", false).value().unwrap(),
        CborValue::fake(Some(1), Pos(1363896240))
    );
    assert_eq!(
        str_to_cbor("0xc1fb41d452d9ec200000", false)
            .value()
            .unwrap(),
        CborValue::fake(Some(1), Float(1363896240.5))
    );
    assert_eq!(
        str_to_cbor("0xd74401020304", false).value().unwrap(),
        CborValue::fake(Some(23), Bytes(&[1, 2, 3, 4]))
    );
    assert_eq!(
        str_to_cbor("0xd818456449455446", true).value().unwrap(),
        CborValue::fake(Some(24), Bytes(b"dIETF"))
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
    assert_eq!(b("0x4401020304"), vec![1u8, 2, 3, 4]);
    assert_eq!(b("0x5f42010243030405ff"), vec![1u8, 2, 3, 4, 5]);
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

    assert_eq!(s("0x60"), "".to_owned());
    assert_eq!(s("0x6161"), "a".to_owned());
    assert_eq!(s("0x6449455446"), "IETF".to_owned());
    assert_eq!(s("0x62225c"), "\"\\".to_owned());
    assert_eq!(s("0x62c3bc"), "\u{00fc}".to_owned());
    assert_eq!(s("0x63e6b0b4"), "\u{6c34}".to_owned());
    assert_eq!(s("0x64f0908591"), "\u{10151}".to_owned());
    assert_eq!(s("0x7f657374726561646d696e67ff"), "streaming".to_owned());
}

#[test]
fn object() {
    use CborObject::*;
    fn o(c: &CborOwned) -> CborObject {
        c.value().unwrap().as_object().unwrap()
    }

    let bytes = str_to_cbor("0x80", false);
    assert_eq!(o(&bytes), Array(vec![]));

    let bytes = str_to_cbor("0x83010203", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(None, Pos(1)),
            Value(None, Pos(2)),
            Value(None, Pos(3))
        ])
    );

    let bytes = str_to_cbor("0x8301820203820405", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(None, Pos(1)),
            Array(vec![Value(None, Pos(2)), Value(None, Pos(3))]),
            Array(vec![Value(None, Pos(4)), Value(None, Pos(5))]),
        ])
    );

    let bytes = str_to_cbor(
        "0x98190102030405060708090a0b0c0d0e0f101112131415161718181819",
        false,
    );
    assert_eq!(
        o(&bytes),
        Array((1u64..26).map(|i| Value(None, Pos(i))).collect())
    );

    let bytes = str_to_cbor("0xa0", false);
    assert_eq!(o(&bytes), Dict(btreemap! {}));

    // original test uses integer keys which we don’t support (0xa201020304)
    let bytes = str_to_cbor("0xa2613102613304", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "1" => Value(None, Pos(2)),
            "3" => Value(None, Pos(4))
        })
    );

    let bytes = str_to_cbor("0xa26161016162820203", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "a" => Value(None, Pos(1)),
            "b" => Array(vec![Value(None, Pos(2)), Value(None, Pos(3))])
        })
    );

    let bytes = str_to_cbor("0x826161a161626163", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(None, Str("a")),
            Dict(btreemap! {
                "b" => Value(None, Str("c"))
            })
        ])
    );

    let bytes = str_to_cbor("0xa56161614161626142616361436164614461656145", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "a" => Value(None, Str("A")),
            "b" => Value(None, Str("B")),
            "c" => Value(None, Str("C")),
            "d" => Value(None, Str("D")),
            "e" => Value(None, Str("E")),
        })
    );

    let bytes = str_to_cbor("0x9fff", false);
    assert_eq!(o(&bytes), Array(vec![]));

    let bytes = str_to_cbor("0x9f018202039f0405ffff", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(None, Pos(1)),
            Array(vec![Value(None, Pos(2)), Value(None, Pos(3))]),
            Array(vec![Value(None, Pos(4)), Value(None, Pos(5))]),
        ])
    );

    let bytes = str_to_cbor("0x9f01820203820405ff", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(None, Pos(1)),
            Array(vec![Value(None, Pos(2)), Value(None, Pos(3))]),
            Array(vec![Value(None, Pos(4)), Value(None, Pos(5))]),
        ])
    );

    let bytes = str_to_cbor("0x83018202039f0405ff", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(None, Pos(1)),
            Array(vec![Value(None, Pos(2)), Value(None, Pos(3))]),
            Array(vec![Value(None, Pos(4)), Value(None, Pos(5))]),
        ])
    );

    let bytes = str_to_cbor("0x83019f0203ff820405", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(None, Pos(1)),
            Array(vec![Value(None, Pos(2)), Value(None, Pos(3))]),
            Array(vec![Value(None, Pos(4)), Value(None, Pos(5))]),
        ])
    );

    let bytes = str_to_cbor(
        "0x9f0102030405060708090a0b0c0d0e0f101112131415161718181819ff",
        false,
    );
    assert_eq!(
        o(&bytes),
        Array((1u64..26).map(|i| Value(None, Pos(i))).collect())
    );

    let bytes = str_to_cbor("0xbf61610161629f0203ffff", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "a" => Value(None, Pos(1)),
            "b" => Array(vec![Value(None, Pos(2)), Value(None, Pos(3))])
        })
    );

    let bytes = str_to_cbor("0x826161bf61626163ff", false);
    assert_eq!(
        o(&bytes),
        Array(vec![
            Value(None, Str("a")),
            Dict(btreemap! {
                "b" => Value(None, Str("c"))
            })
        ])
    );

    let bytes = str_to_cbor("0xbf6346756ef563416d7421ff", false);
    assert_eq!(
        o(&bytes),
        Dict(btreemap! {
            "Fun" => Value(None, Bool(true)),
            "Amt" => Value(None, Neg(1))
        })
    );
}
