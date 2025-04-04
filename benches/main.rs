#![feature(portable_simd)]
use std::collections::HashMap;
use highway::HighwayHasher;
use std::hash::BuildHasherDefault;
use criterion::{criterion_group, criterion_main, Criterion};
use rand::Rng;

use simd_psmap::SimdPerfectScanMap;

#[derive(Debug, PartialEq, Clone)]
struct DummyVal(usize);


fn criterion_benchmark(c: &mut Criterion) {

    let mut rng = rand::rng();
    

    let mut group = c.benchmark_group("6 short keys with no major overlap");

    let kvs: Vec<(String, DummyVal)> = vec![
        ("key1".into(), DummyVal(1001)),
        ("now4".into(), DummyVal(1002)),
        ("something".into(), DummyVal(1003)),
        ("another".into(), DummyVal(1004)),
        ("interesting".into(), DummyVal(1005)),
        ("thanks".into(), DummyVal(1005)),
    ];
    
    let m = SimdPerfectScanMap::<DummyVal, 8, 16>::try_from(kvs.clone()).unwrap(); // we clone here so we can reuse the kvs for other benchmarks
    assert_eq!(m.get(&"key1".into()), Some(&DummyVal(1001)));
    assert_eq!(m.get(&"another".into()), Some(&DummyVal(1004)));

    let mut values: Vec<String> = Vec::with_capacity(10000);
    for _ in 0..values.capacity() {
        values.push(if rng.random_bool(0.5) { "key1".into() } else {  "another".into() });
    }
    let mut value_iter = values.iter().cycle(); 
    group.bench_function("SimdPerfectScanMap", |b| b.iter(|| m.get( value_iter.next().unwrap())));
    

    let h = HashMap::<String, DummyVal>::from_iter(kvs.clone().into_iter());
    assert_eq!(h.get("key1"), Some(&DummyVal(1001)));
    assert_eq!(h.get("another".into()), Some(&DummyVal(1004)));
    group.bench_function("std::HashMap", |b| b.iter(|| h.get(value_iter.next().unwrap())));
    

    let h = HashMap::<String, DummyVal, BuildHasherDefault::<HighwayHasher>>::from_iter(kvs.into_iter());
    assert_eq!(h.get("key1"), Some(&DummyVal(1001)));
    assert_eq!(h.get("another".into()), Some(&DummyVal(1004)));
    group.bench_function("std::HashMap<..HighwayHasher>", |b| b.iter(|| h.get(value_iter.next().unwrap())));
    

    group.finish();



    let mut group = c.benchmark_group("6 short keys with substantial overlap");

    let kvs: Vec<(String, DummyVal)> = vec![
        ("key1".into(), DummyVal(1001)),
        ("key1longer".into(), DummyVal(1002)),
        ("key".into(), DummyVal(1003)),
        ("now4".into(), DummyVal(1004)),
        ("something".into(), DummyVal(1005)),
        ("something_b".into(), DummyVal(1006))
    ];
    
    let m = SimdPerfectScanMap::<DummyVal, 8, 16>::try_from(kvs.clone()).unwrap(); // we clone here so we can reuse the kvs for other benchmarks
    assert_eq!(m.get(&"key1".into()), Some(&DummyVal(1001)));
    assert_eq!(m.get(&"key1longer".into()), Some(&DummyVal(1002)));

    let mut values: Vec<String> = Vec::with_capacity(10000);
    for _ in 0..values.capacity() {
        values.push(if rng.random_bool(0.5) { "key1".into() } else {  "key1longer".into() });
    }
    let mut value_iter = values.iter().cycle(); 
    group.bench_function("SimdPerfectScanMap", |b| b.iter(|| m.get( value_iter.next().unwrap())));
    

    let h = HashMap::<String, DummyVal>::from_iter(kvs.clone().into_iter());
    assert_eq!(h.get("key1"), Some(&DummyVal(1001)));
    assert_eq!(h.get("key1longer".into()), Some(&DummyVal(1002)));
    group.bench_function("std::HashMap", |b| b.iter(|| h.get(value_iter.next().unwrap())));
    
    let h = HashMap::<String, DummyVal, BuildHasherDefault::<HighwayHasher>>::from_iter(kvs.into_iter());
    assert_eq!(h.get("key1"), Some(&DummyVal(1001)));
    assert_eq!(h.get("key1longer".into()), Some(&DummyVal(1002)));
    group.bench_function("std::HashMap<..HighwayHasher>", |b| b.iter(|| h.get(value_iter.next().unwrap())));

    group.finish();

}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

