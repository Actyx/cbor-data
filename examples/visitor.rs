use cbor_data::{CborOwned, CborValue, Visitor};

struct X<'a>(Vec<&'a str>);
impl<'a> Visitor<'a, ()> for X<'a> {
    fn visit_simple(&mut self, item: CborValue<'a>) -> Result<(), ()> {
        if let Some(s) = item.as_str() {
            if item.tags.is_empty() {
                self.0.push(s);
            }
        }
        Ok(())
    }
    fn visit_dict_key(&mut self, key: &str, _is_first: bool) -> Result<bool, ()> {
        Ok(key != "Fun")
    }
}

fn main() {
    let cbor = CborOwned::trusting([0xbf, 0x63, 0x46, 0x75, 0x6e, 0x63, 0x41, 0x6d, 0x74, 0xff]);
    let mut visitor = X(Vec::new());
    println!("{}", cbor.visit(&mut visitor).unwrap());
    println!("{:?}", visitor.0);
}
