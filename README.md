A library for using CBOR as in-memory representation for working with dynamically shaped data.

For the details on the data format see [RFC 8949](https://www.rfc-editor.org/rfc/rfc8949). It is
normally meant to be used as a data interchange format that models a superset of the JSON
features while employing a more compact binary representation. As such, the data representation
is biased towards smaller in-memory size and not towards fastest data access speed.

This library presents a range of tradeoffs when using this data format. You can just use the
bits you get from the wire or from a file, without paying initial overhead beyond scanning the
bytes once for valid encoding, but then possible causing allocations when working with the data.
Or you can canonicalise the bits before using them, guaranteeing that indexing into the data
will never allocate.

Regarding performance you should keep in mind that arrays and dictionaries are encoded as flat
juxtaposition of its elements, meaning that indexing will have to decode items as it skips over
them.

Regarding the interpretation of parsed data you have the option of inspecting the particular
encoding (by pattern matching on [`ItemKind`](enum.ItemKind.html)) or inspecting the higher-level
[`CborValue`](enum.CborValue.html). In the latter case, many binary representations may yield the
same value, e.g. when asking for an integer the result may stem from a non-optimal encoding
(like writing 57 as 64-bit value) or from a BigDecimal with mantissa 570 and exponent -1.

# Example

```rust
use cbor_data::{CborBuilder, Encoder, Writer, constants::*};

// create some nonsense CBOR item
let cbor = CborBuilder::new().encode_dict(|builder| {
    builder.with_key("name", |builder| builder.encode_str("Actyx"));
    builder.with_key("founded", |b| b.write_str("2016-02-11T13:00:00+01:00", [TAG_ISO8601]));
    builder.with_key("founders", |builder| builder.encode_array(|builder| {
        builder
            .encode_str("Oliver Stollmann")
            .encode_str("Maximilian Fischer")
            .encode_str("Roland Kuhn");
    }));
});

// access properties
use cbor_data::{PathElement, index_str, CborValue};
use std::borrow::Cow::Borrowed;

let item = cbor.index(index_str("name")).unwrap();
assert_eq!(item.decode(), CborValue::Str(Borrowed("Actyx")));

// decoding tries referencing source bytes where possible, use make_static to break ties
let d = cbor.index([PathElement::String(Borrowed("founded"))]).unwrap().decode().make_static();
assert_eq!(d, CborValue::Timestamp { unix_epoch: 1455192000, nanos: 0, tz_sec_east: 3600 });

let item = cbor.index(index_str("founders[1]")).unwrap();
assert_eq!(item.decode(), CborValue::Str(Borrowed("Maximilian Fischer")));

// access low-level encoding
use cbor_data::ItemKind;

let item = cbor.index(index_str("founded")).unwrap();
assert_eq!(item.tags().collect::<Vec<_>>(), [TAG_ISO8601]);
assert!(matches!(item.kind(), ItemKind::Str(s) if s == "2016-02-11T13:00:00+01:00"));
```
