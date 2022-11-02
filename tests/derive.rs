use cbor_data::{
    codec::{CodecError, ReadCbor, WriteCbor},
    Cbor, CborBuilder,
};

fn b(mut s: &str) -> Vec<u8> {
    let mut ret = vec![];
    while !s.is_empty() {
        let space = s.find(' ').unwrap_or(s.len());
        ret.push(u8::from_str_radix(&s[..space], 16).unwrap());
        s = &s[(space + 1).min(s.len())..];
    }
    ret
}

#[test]
fn named_struct() {
    #[derive(ReadCbor, WriteCbor, PartialEq, Debug)]
    struct X {
        x: String,
        y: u64,
    }

    impl X {
        fn new(x: impl Into<String>, y: u64) -> Self {
            Self { x: x.into(), y }
        }
    }

    let bytes = X::new("hello", 42).write_cbor(CborBuilder::default());
    assert_eq!(
        bytes.as_slice(),
        b("a2 61 78 65 68 65 6c 6c 6f 61 79 18 2a")
    );
    let x = X::read_cbor(bytes.as_ref()).unwrap();
    assert_eq!(x, X::new("hello", 42));
    let x =
        X::read_cbor(Cbor::checked(&*b("a2 61 77 64 68 65 6c 6c 61 41 00")).unwrap()).unwrap_err();
    assert_eq!(x, CodecError::MissingField("x"));
    let x =
        X::read_cbor(Cbor::checked(&*b("a3 61 78 64 68 65 6c 6c 61 79 18 2a 61 41 00")).unwrap())
            .unwrap();
    assert_eq!(x, X::new("hell", 42));
    assert_eq!(X::name(), "X");
}

#[test]
fn tuple_struct() {
    #[derive(Debug, PartialEq, WriteCbor, ReadCbor)]
    struct X(u64, String);

    let s = "str".to_string();
    let bytes = X(42, s.clone()).write_cbor(CborBuilder::default());
    assert_eq!(bytes.as_slice(), b("82 18 2a 63 73 74 72"));
    let x = X::read_cbor(bytes.as_ref()).unwrap();
    assert_eq!(x, X(42, s));
    let e = X::read_cbor(Cbor::checked(&*b("81 18 2a")).unwrap()).unwrap_err();
    assert_eq!(
        e,
        CodecError::TupleSize {
            expected: 2,
            found: 1
        }
    );
    let x = X::read_cbor(Cbor::checked(&*b("83 17 60 00")).unwrap()).unwrap();
    assert_eq!(x, X(23, String::new()));
}

#[test]
fn single_struct() {
    #[derive(Debug, PartialEq, WriteCbor, ReadCbor)]
    struct X(u64);

    assert_eq!(
        X(3).write_cbor(CborBuilder::default()).as_slice(),
        b("81 03")
    );
    assert_eq!(
        X::read_cbor(Cbor::checked(&*b("82 13 00")).unwrap()).unwrap(),
        X(19)
    );

    #[derive(Debug, PartialEq, WriteCbor, ReadCbor)]
    #[cbor(transparent)]
    struct Y(u64);

    assert_eq!(Y(3).write_cbor(CborBuilder::default()).as_slice(), b("03"));
    assert_eq!(
        Y::read_cbor(Cbor::checked(&*b("13")).unwrap()).unwrap(),
        Y(19)
    );
}

#[test]
fn enums() {
    #[derive(Debug, PartialEq, WriteCbor, ReadCbor)]
    #[cbor(x)]
    enum X {
        Unit,
        One(u64),
        #[cbor(transparent)]
        OnePrime(u64),
        Two(u64, u64),
        Rec {
            a: u64,
            b: u64,
        },
    }

    let bytes = X::Unit.write_cbor(CborBuilder::default());
    assert_eq!(bytes.as_slice(), b("a1 64 55 6e 69 74 f6"));
    assert_eq!(X::read_cbor(bytes.as_ref()).unwrap(), X::Unit);

    let bytes = X::One(1).write_cbor(CborBuilder::default());
    assert_eq!(bytes.as_slice(), b("a1 63 4f 6e 65 81 01"));
    assert_eq!(X::read_cbor(bytes.as_ref()).unwrap(), X::One(1));

    let bytes = X::OnePrime(2).write_cbor(CborBuilder::default());
    assert_eq!(bytes.as_slice(), b("a1 68 4f 6e 65 50 72 69 6d 65 02"));
    assert_eq!(X::read_cbor(bytes.as_ref()).unwrap(), X::OnePrime(2));

    let bytes = X::Two(3, 4).write_cbor(CborBuilder::default());
    assert_eq!(bytes.as_slice(), b("a1 63 54 77 6f 82 03 04"));
    assert_eq!(X::read_cbor(bytes.as_ref()).unwrap(), X::Two(3, 4));

    let bytes = X::Rec { a: 5, b: 6 }.write_cbor(CborBuilder::default());
    assert_eq!(bytes.as_slice(), b("a1 63 52 65 63 a2 61 61 05 61 62 06"));
    assert_eq!(X::read_cbor(bytes.as_ref()).unwrap(), X::Rec { a: 5, b: 6 });
}
