use nomt::{hasher::Sha2Hasher, KeyReadWrite, Nomt, Options, SessionParams};
use tempfile::tempdir;

#[test]
fn complete_export_checks_root_count_order_and_overflow_values() {
    let directory = tempdir().unwrap();
    let options = || {
        let mut options = Options::new();
        options.path(directory.path().join("db"));
        options.hashtable_buckets(1024);
        options.preallocate_ht(false);
        options.io_workers(1);
        options.commit_concurrency(1);
        options
    };
    let database = Nomt::<Sha2Hasher>::open(options()).unwrap();
    let values = vec![
        ([1; 32], vec![1]),
        ([2; 32], vec![2; 20_000]),
        ([3; 32], vec![]),
    ];
    database
        .begin_session(SessionParams::default())
        .finish(
            values
                .iter()
                .map(|(key, value)| (*key, KeyReadWrite::Write(Some(value.clone()))))
                .collect(),
        )
        .unwrap()
        .commit(&database)
        .unwrap();
    let usage = database.storage_usage().unwrap();
    assert_eq!(usage.bucket_capacity, 1024);
    assert!(usage.resident_bucket_metadata_bytes >= 1024);
    assert!(usage.resident_branch_count > 0);
    assert_eq!(usage.resident_branch_page_bytes, usage.resident_branch_count * 4096);
    assert!(usage.pool_mapped_bytes >= usage.resident_branch_page_bytes);
    assert!(usage.leaf_next_page > 1);
    assert!(usage.branch_next_page > 1);
    let root = database.root().into_inner();
    drop(database);
    let database = Nomt::<Sha2Hasher>::open(options()).unwrap();
    let mut exported = Vec::new();
    database
        .export_sorted(root, 3, |key, value| {
            exported.push((key, value.to_vec()));
            Ok(())
        })
        .unwrap();
    assert_eq!(exported, values);
    assert!(database.export_sorted([9; 32], 3, |_, _| Ok(())).is_err());
    assert!(database.export_sorted(root, 2, |_, _| Ok(())).is_err());
    assert!(database.export_sorted(root, 4, |_, _| Ok(())).is_err());
    assert!(database
        .export_sorted(root, 3, |_, _| anyhow::bail!("cancelled"))
        .is_err());
    database
        .begin_session(SessionParams::default())
        .finish(vec![([2; 32], KeyReadWrite::Write(None))])
        .unwrap()
        .commit(&database)
        .unwrap();
    let mut exported = Vec::new();
    database
        .export_sorted(database.root().into_inner(), 2, |key, value| {
            exported.push((key, value.to_vec()));
            Ok(())
        })
        .unwrap();
    assert_eq!(exported, vec![values[0].clone(), values[2].clone()]);
    let root = database.root().into_inner();
    drop(database);
    let mut growth = options();
    growth.hashtable_buckets(2048);
    nomt::grow_hashtable(&growth).unwrap();
    nomt::validate_hashtable(&growth).unwrap();
    let database = Nomt::<Sha2Hasher>::open(options()).unwrap();
    assert_eq!(database.storage_usage().unwrap().bucket_capacity, 2048);
    assert_eq!(database.root().into_inner(), root);
    let mut grown = Vec::new();
    database.export_sorted(root, 2, |key, value| { grown.push((key, value.to_vec())); Ok(()) }).unwrap();
    assert_eq!(grown, exported);
}
