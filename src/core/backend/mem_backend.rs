use crate::core::backend::*;
use crate::core::data::{BobData, BobKey, VDiskId, VDiskMapper, WriteOption};
use futures::future::{err, ok};
use futures_locks::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

struct VDisk {
    repo: HashMap<BobKey, BobData>,
}
 
impl  VDisk {
    pub fn new() -> VDisk {
        VDisk{
            repo: HashMap::<BobKey, BobData>::new()
        }
    }

    pub fn put(&mut self, key: BobKey, data: BobData) {
        self.repo.insert(key, data);
    }

    pub fn get(&self, key: BobKey) -> Option<BobData> {
        match self.repo.get(&key) {
            Some(data) => Some(data.clone()),
            None => None,
        }
    }
}

struct MemDisk {
    name: String,
    vdisks: HashMap<VDiskId, VDisk>,
}

impl MemDisk {
    pub fn new(name: String, vdisks_count: u32) -> MemDisk {
        let mut b: HashMap<VDiskId, VDisk> = HashMap::new();
        for i in 0..vdisks_count {
            b.insert(VDiskId::new(i), VDisk::new());
        }
        MemDisk {
            name: name.clone(),
            vdisks: b,
        }
    }

    pub fn put(&mut self, vdisk_id: VDiskId, key: BobKey, data: BobData) {
        match self.vdisks.get_mut(&vdisk_id) {
            Some(vdisk) => vdisk.put(key, data),
            None => (), // TODO log
        }
    }

    pub fn get(&self, vdisk_id: VDiskId, key: BobKey) -> Option<BobData> {
        match self.vdisks.get(&vdisk_id) {
            Some(vdisk) => vdisk.get(key),
            None => None, // TODO log
        }
    }
}

#[derive(Clone)]
pub struct MemBackend {
    disks: Arc<RwLock<HashMap<String, MemDisk>>>,
}

impl MemBackend {
    pub fn new(paths: &[String], vdisks_count: u32) -> MemBackend {
        let b = paths.iter()
            .map(|p|(p.clone(), MemDisk::new(p.clone(), vdisks_count)))
            .collect::<HashMap<String, MemDisk>>();
        MemBackend {
            disks: Arc::new(RwLock::new(b)),
        }
    }

    pub fn new2(mapper: &VDiskMapper) -> MemBackend {
        let b = mapper.local_disks().iter()
            .map(|p|(p.name.clone(), MemDisk::new(p.name.clone(), 10)))   //TODO usereal vdisks
            .collect::<HashMap<String, MemDisk>>();
        MemBackend {
            disks: Arc::new(RwLock::new(b)),
        }
    }
}

impl Backend for MemBackend {
    fn put(&self, op: &WriteOption, key: BobKey, data: BobData) -> BackendPutFuture {
        let disk = op.disk_name.clone();
        let id = op.vdisk_id.clone();

        debug!("PUT[{}][{}]", key, disk);
        Box::new(self
            .disks
            .write()
            .then(move |disks_lock_res| match disks_lock_res {
                Ok(mut disks) => match disks.get_mut(&disk) {
                    Some(mem_disk) => {
                        mem_disk.put(id, key, data);
                        ok(BackendResult {})
                    }
                    None => {
                        error!("PUT[{}][{}] Can't find disk {}", key, disk, disk);
                        err(BackendError::NotFound)
                    }
                },
                Err(_) => err(BackendError::Other),
            }))
    }
    fn get(&self, op: &WriteOption, key: BobKey) -> BackendGetFuture {
        let disk = op.disk_name.clone();
        let id = op.vdisk_id.clone();

        debug!("GET[{}][{}]", key, disk);
        Box::new(self
            .disks
            .read()
            .then(move |disks_lock_res| match disks_lock_res {
                Ok(disks) => match disks.get(&disk) {
                    Some(mem_disk) => match mem_disk.get(id, key) {
                                Some(data) => ok(BackendGetResult { data }),
                                None => err(BackendError::NotFound),
                            }
                    None => {
                        error!("GET[{}][{}] Can't find disk {}", key, disk, disk);
                        err(BackendError::NotFound)
                    }
                },
                Err(_) => err(BackendError::Other),
            }))
    }
}
