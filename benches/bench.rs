use std::{borrow::Borrow, cell::RefCell};

use cbor_data::{
    constants::TAG_EPOCH, index_str, Cbor, CborBuilder, CborOwned, Encoder, ItemKind, Visitor,
    Writer,
};
use criterion::{criterion_group, criterion_main, Criterion};
use rand::{random, thread_rng, Rng};

fn name() -> String {
    let mut arr = [0 as char; 8];
    thread_rng().fill(&mut arr[..]);
    let mut s = String::new();
    s.extend(arr.iter());
    s
}

thread_local! {
    static SCRATCH: RefCell<Vec<u8>> = Vec::new().into();
}

fn create_cbor() -> CborOwned {
    SCRATCH.with(|v| {
        let mut v = v.borrow_mut();
        CborBuilder::with_scratch_space(v.as_mut()).encode_dict(|b| {
            b.with_key("type", |b| b.write_str("WorkStopped", None));
            b.with_key("byWhom", |b| b.write_str(&*name(), None));
            b.with_key("pause", |b| b.write_bool(false, None));
            b.with_key("workers", |b| {
                b.write_array(None, |b| {
                    b.write_str(&*name(), None);
                    b.write_str(&*name(), None);
                    b.write_str(&*name(), None);
                })
            });
            b.with_key("started", |b| b.write_pos(random(), Some(TAG_EPOCH)));
            b.with_key("stopped", |b| b.write_pos(random(), Some(TAG_EPOCH)));
        })
    })
}

fn make_new_object(obj: &Cbor) -> CborOwned {
    CborBuilder::default().write_dict(None, |b| {
        let mut started = 0;
        if let ItemKind::Pos(x) = obj.index(index_str("started").unwrap()).unwrap().item() {
            started = x;
            b.with_key("start", |b| b.write_pos(x, None));
        }
        if let ItemKind::Str(s) = obj.index(index_str("byWhom").unwrap()).unwrap().item() {
            b.with_key("who", |b| b.write_str(s.as_cow().as_ref(), None));
        }
        if let ItemKind::Pos(stopped) = obj.index(index_str("stopped").unwrap()).unwrap().item() {
            b.with_key("duration", |b| b.write_pos(stopped - started, None));
        }
    })
}

struct Depth {
    curr: usize,
    max: usize,
}

impl Depth {
    pub fn new() -> Self {
        Self { curr: 1, max: 1 }
    }
}

impl<'a> Visitor<'a, ()> for Depth {
    fn visit_array_begin(
        &mut self,
        _array: cbor_data::TaggedItem<'a>,
        _size: Option<u64>,
    ) -> Result<bool, ()> {
        self.curr += 1;
        self.max = self.curr.max(self.max);
        Ok(true)
    }

    fn visit_array_end(&mut self, _array: cbor_data::TaggedItem<'a>) -> Result<(), ()> {
        self.curr -= 1;
        Ok(())
    }

    fn visit_dict_begin(
        &mut self,
        _dict: cbor_data::TaggedItem<'a>,
        _size: Option<u64>,
    ) -> Result<bool, ()> {
        self.curr += 1;
        self.max = self.curr.max(self.max);
        Ok(true)
    }

    fn visit_dict_end(&mut self, _dict: cbor_data::TaggedItem<'a>) -> Result<(), ()> {
        self.curr -= 1;
        Ok(())
    }
}

fn extract(c: &mut Criterion) {
    c.bench_function("make object", |b| b.iter(create_cbor));
    c.bench_function("transform object", |b| {
        b.iter_batched_ref(
            create_cbor,
            |o| make_new_object((*o).borrow()),
            criterion::BatchSize::SmallInput,
        )
    });
    c.bench_function("as_object", |b| {
        b.iter_batched_ref(
            create_cbor,
            |o| {
                let mut d = Depth::new();
                o.visit(&mut d).unwrap();
                d.max
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, extract);
criterion_main!(benches);
