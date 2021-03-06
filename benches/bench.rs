use std::cell::RefCell;

use cbor_data::{constants::TAG_EPOCH, Cbor, CborBuilder, CborOwned, Encoder, Writer};
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

fn make_new_object(obj: Cbor) -> CborOwned {
    CborBuilder::default().write_dict(None, |b| {
        b.with_key("start", |b| {
            b.write_pos(obj.index("started").unwrap().as_u64().unwrap(), None)
        });
        b.with_key("who", |b| {
            b.write_str(obj.index("byWhom").unwrap().as_str().unwrap(), None)
        });
        b.with_key("duration", |b| {
            b.write_pos(
                obj.index("stopped").unwrap().as_u64().unwrap()
                    - obj.index("started").unwrap().as_u64().unwrap(),
                None,
            )
        });
    })
}

fn extract(c: &mut Criterion) {
    c.bench_function("make object", |b| b.iter(create_cbor));
    c.bench_function("transform object", |b| {
        b.iter_batched_ref(
            create_cbor,
            |o| make_new_object(o.borrow()),
            criterion::BatchSize::SmallInput,
        )
    });
    c.bench_function("as_object", |b| {
        b.iter_batched_ref(
            create_cbor,
            |o| Some(o.value()?.as_object()?.depth()),
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, extract);
criterion_main!(benches);
