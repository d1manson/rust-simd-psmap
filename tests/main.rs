
use simd_psmap::SimdPerfectScanMap;

#[derive(Debug, PartialEq)]
struct DummyVal(usize);

#[test]
fn test_simple_example(){
    let kvs: Vec<(String, DummyVal)> = vec![
        ("key1".into(), DummyVal(1001)),
        ("key1longer".into(), DummyVal(1002)),
        ("key".into(), DummyVal(1003)),
        ("now4".into(), DummyVal(1004))
    ];

    let m = SimdPerfectScanMap::<DummyVal, 16, 16>::try_from(kvs); 
    assert!(m.is_ok());
    let m = m.unwrap();
    assert_eq!(m.len(), 4);

    assert_eq!(m.get(&"key1".into()), Some(&DummyVal(1001)));
    assert!(m.get(&"key1 continued".into()).is_none());
    assert_eq!(m.get(&"key1longer".into()), Some(&(DummyVal(1002))));
    assert!(m.get(&"kon1".into()).is_none());
    assert_eq!(m.get(&"now4".into()),  Some(&DummyVal(1004)));
}


#[test]
fn test_invalid_example(){
    let kvs: Vec<(String, DummyVal)> = vec![
        ("aaaa".into(), DummyVal(1001)),
        ("abaa".into(), DummyVal(1002)),
        ("aaca".into(), DummyVal(1003)),
        ("aaad".into(), DummyVal(1004)),
    ];

    // you need 3 tests to distinguish between these 4 keys, but we only allow 2 below, which will be a failure
    let m = SimdPerfectScanMap::<DummyVal, 2, 16>::try_from(kvs); 
    assert!(m.is_err());
    let (err_msg, kvs) = m.unwrap_err(); // note how we regain ownership of kvs within the error payload
    assert_eq!(err_msg, "Unable to 'solve' with a sufficiently small number of scans"); 

    // with 3 it's ok
    let m = SimdPerfectScanMap::<DummyVal, 3, 16>::try_from(kvs); 
    assert!(m.is_ok());
}