use super::prelude::*;

use futures::executor::{ThreadPool, ThreadPoolBuilder};

const VDISKS_COUNT: u32 = 10;

fn get_pool() -> ThreadPool {
    ThreadPoolBuilder::new().pool_size(1).create().unwrap()
}

#[test]
fn test_mem_put_wrong_disk() {
    let backend = MemBackend::new_direct(&["name".to_string()], VDISKS_COUNT);
    let mut reactor = get_pool();

    let retval = reactor.run(
        backend
            .put(
                BackendOperation::new_local(VDiskId::new(0), DiskPath::new("invalid name", "")),
                BobKey { key: 1 },
                BobData {
                    data: vec![0],
                    meta: BobMeta::new_stub(),
                },
            )
            .0,
    );
    assert_eq!(retval.err().unwrap(), Error::Internal)
}

#[test]
fn test_mem_put_get() {
    let backend = MemBackend::new_direct(&["name".to_string()], VDISKS_COUNT);
    let mut reactor = get_pool();

    reactor
        .run(
            backend
                .put(
                    BackendOperation::new_local(VDiskId::new(0), DiskPath::new("name", "")),
                    BobKey { key: 1 },
                    BobData {
                        data: vec![1],
                        meta: BobMeta::new_stub(),
                    },
                )
                .0,
        )
        .unwrap();
    let retval = reactor
        .run(
            backend
                .get(
                    BackendOperation::new_local(VDiskId::new(0), DiskPath::new("name", "")),
                    BobKey { key: 1 },
                )
                .0,
        )
        .unwrap();
    assert_eq!(retval.data.data, vec![1]);
}

#[test]
fn test_mem_get_wrong_disk() {
    let backend = MemBackend::new_direct(&["name".to_string()], VDISKS_COUNT);
    let mut reactor = get_pool();

    reactor
        .run(
            backend
                .put(
                    BackendOperation::new_local(VDiskId::new(0), DiskPath::new("name", "")),
                    BobKey { key: 1 },
                    BobData {
                        data: vec![1],
                        meta: BobMeta::new_stub(),
                    },
                )
                .0,
        )
        .unwrap();
    let retval = reactor.run(
        backend
            .get(
                BackendOperation::new_local(VDiskId::new(0), DiskPath::new("invalid name", "")),
                BobKey { key: 1 },
            )
            .0,
    );
    assert_eq!(retval.err().unwrap(), Error::Internal)
}

#[test]
fn test_mem_get_no_data() {
    let backend = MemBackend::new_direct(&["name".to_string()], VDISKS_COUNT);
    let mut reactor = get_pool();

    let retval = reactor.run(
        backend
            .get(
                BackendOperation::new_local(VDiskId::new(0), DiskPath::new("name", "")),
                BobKey { key: 1 },
            )
            .0,
    );
    assert_eq!(retval.err().unwrap(), Error::KeyNotFound)
}
