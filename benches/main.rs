#![feature(portable_simd)]
use std::collections::HashMap;
use rustc_hash::FxHashMap;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::Rng;
use rand::seq::SliceRandom;
use simd_psmap::SimdPerfectScanMap;

#[derive(Debug, PartialEq, Clone)]
struct DummyVal(usize);

const N_LANES: usize = 16;

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
    
    let m = SimdPerfectScanMap::<DummyVal, 8, N_LANES>::try_from(kvs.clone()).unwrap(); // we clone here so we can reuse the kvs for other benchmarks
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
    

    let h = FxHashMap::<String, DummyVal>::from_iter(kvs.into_iter());
    assert_eq!(h.get("key1"), Some(&DummyVal(1001)));
    assert_eq!(h.get("another".into()), Some(&DummyVal(1004)));
    group.bench_function("FxHashMap", |b| b.iter(|| h.get(value_iter.next().unwrap())));
    
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
    
    let m = SimdPerfectScanMap::<DummyVal, 8, N_LANES>::try_from(kvs.clone()).unwrap(); // we clone here so we can reuse the kvs for other benchmarks
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
    
    let h = FxHashMap::<String, DummyVal>::from_iter(kvs.into_iter());
    assert_eq!(h.get("key1"), Some(&DummyVal(1001)));
    assert_eq!(h.get("key1longer".into()), Some(&DummyVal(1002)));
    group.bench_function("FxHashMap", |b| b.iter(|| h.get(value_iter.next().unwrap())));

    group.finish();


    let mut group = c.benchmark_group("num of keys");
    // key list taken from a real-world api here: https://data.crunchbase.com/reference/get_data-entities-organizations-entity-id
    let mut sample_keys = [ "acquirer_identifier","aliases","categories","category_groups","closed_on","company_type","contact_email","created_at","delisted_on","demo_days","description","entity_def_id","exited_on","facebook","facet_ids","founded_on","founder_identifiers","hub_tags","identifier","image_id","image_url","layout_id","legal_name","linkedin","listed_stock_symbol","location_group_identifiers","location_identifiers","name","num_alumni","num_current_advisor_positions","num_current_positions","num_employees_enum","num_enrollments","num_event_appearances","num_portfolio_organizations","num_sub_organizations","operating_status","owner_identifier","permalink","permalink_aliases","phone_number","program_application_deadline","program_duration","program_type","rank","rank_org","school_method","school_program","school_type","short_description","status","stock_exchange_symbol","stock_symbol","twitter","updated_at","uuid","website","website_url","went_public_on"];
    for size in (3..=sample_keys.len()).step_by(3) {
        sample_keys.shuffle(&mut rng);
        let kvs : Vec<(String, DummyVal)>= sample_keys[..size].iter().enumerate().map(|(idx, k)| (k.to_string(), DummyVal(idx+1000))).collect();

        let mut values: Vec<String> = Vec::with_capacity(10000);
        for _ in 0..values.capacity() {
            values.push(kvs[rng.random_range(0..size)].0.clone());
        }
        let mut value_iter = values.iter().cycle(); 

        let m = SimdPerfectScanMap::<DummyVal, 32, N_LANES>::try_from(kvs.clone()).unwrap(); 
        assert_eq!(m.get(&kvs[0].0), Some(&kvs[0].1));
        assert_eq!(m.get(&kvs[2].0), Some(&kvs[2].1));
        group.bench_with_input(BenchmarkId::new("SimdPerfectScanMap", size), &size, |b, _| {
            b.iter(||  m.get( value_iter.next().unwrap()));
        });

        let h = FxHashMap::<String, DummyVal>::from_iter(kvs.clone().into_iter());
        assert_eq!(h.get(&kvs[0].0), Some(&kvs[0].1));
        assert_eq!(h.get(&kvs[2].0), Some(&kvs[2].1));
        group.bench_with_input(BenchmarkId::new("FxHashMap", size), &size, |b, _| {
            b.iter(||  h.get(value_iter.next().unwrap()))
        });
    }
    group.finish();




}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

