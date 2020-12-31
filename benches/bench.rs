use cbor_data::{Cbor, CborBuilder, CborOwned};
use criterion::{criterion_group, criterion_main, Criterion};
use rand::{random, thread_rng, Rng};

fn name() -> String {
    let mut arr = [0 as char; 8];
    thread_rng().fill(&mut arr[..]);
    let mut s = String::new();
    s.extend(arr.iter());
    s
}

fn create_cbor() -> CborOwned {
    CborBuilder::default()
        .write_dict_rec(None, |mut b| {
            b.write_str("type", "WorkStopped", None);
            b.write_str("byWhom", &*name(), None);
            b.write_bool("pause", false, None);
            b.write_array_rec("workers", None, |mut b| {
                b.write_str(&*name(), None);
                b.write_str(&*name(), None);
                b.write_str(&*name(), None);
            });
            b.write_pos("started", random(), None);
            b.write_pos("stopped", random(), None);
        })
        .0
}

fn make_new_object(obj: Cbor) -> CborOwned {
    CborBuilder::default()
        .write_dict_rec(None, |mut b| {
            b.write_pos(
                "start",
                obj.index("started").unwrap().as_u64().unwrap(),
                None,
            );
            b.write_str("who", obj.index("byWhom").unwrap().as_str().unwrap(), None);
            b.write_pos(
                "duration",
                obj.index("stopped").unwrap().as_u64().unwrap()
                    - obj.index("started").unwrap().as_u64().unwrap(),
                None,
            );
        })
        .0
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
            |o| o.value().unwrap().as_object().unwrap().depth(),
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, extract);
criterion_main!(benches);
