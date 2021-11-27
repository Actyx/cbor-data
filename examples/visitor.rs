use cbor_data::{CborOwned, ItemKind, TaggedItem, Visitor};

struct X<'a>(Vec<&'a str>);
impl<'a> Visitor<'a, ()> for X<'a> {
    fn visit_simple(&mut self, item: TaggedItem<'a>) -> Result<(), ()> {
        if let ItemKind::Str(s) = item.kind() {
            // ignore indefinite size encoding of strings
            if let Some(s) = s.as_str() {
                self.0.push(s);
            }
        }
        Ok(())
    }
    fn visit_dict_key(
        &mut self,
        _dict: TaggedItem<'a>,
        key: TaggedItem<'a>,
        _is_first: bool,
    ) -> Result<bool, ()> {
        if let ItemKind::Str(s) = key.kind() {
            Ok(s.as_str() != Some("Fun"))
        } else {
            Ok(true)
        }
    }
}

fn main() {
    let cbor =
        CborOwned::unchecked(&[0xbf, 0x63, 0x46, 0x75, 0x6e, 0x63, 0x41, 0x6d, 0x74, 0xff][..]);
    let mut visitor = X(Vec::new());
    cbor.visit(&mut visitor).unwrap();
    println!("{:?}", visitor.0);
}
