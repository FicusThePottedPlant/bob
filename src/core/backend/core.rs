use crate::core::{
    backend::{
        mem_backend::MemBackend, pearl::core::PearlBackend, stub_backend::StubBackend, Error,
    },
    configs::node::{BackendType, NodeConfig},
    data::{BobData, BobKey, DiskPath, VDiskId, BobOptions},
    mapper::VDiskMapper,
};
use futures03::{
    future::FutureExt,
    task::Spawn,
    Future,
};
use std::{pin::Pin, sync::Arc};

#[derive(Debug, Clone)]
pub struct BackendOperation {
    vdisk_id: VDiskId,
    disk_path: Option<DiskPath>,
    remote_node_name: Option<String>, // save data to alien/<remote_node_name>
}

impl std::fmt::Display for BackendOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.disk_path.clone() {
            Some(path) => write!(
                f,
                "#{}-{}-{}-{}",
                self.vdisk_id, path.name, path.path, self.is_data_alien()
            ),
            None => write!(f, "#{}-{}", self.vdisk_id, self.is_data_alien()),
        }
    }
}

impl BackendOperation {
    pub fn new_alien(vdisk_id: VDiskId) -> BackendOperation {
        BackendOperation {
            vdisk_id,
            disk_path: None,
            remote_node_name: None,
        }
    }
    pub fn new_local(vdisk_id: VDiskId, path: DiskPath) -> BackendOperation {
        BackendOperation {
            vdisk_id,
            disk_path: Some(path),
            remote_node_name: None,
        }
    }

    pub fn set_remote_folder(&mut self, name: &str) {
        self.remote_node_name = Some(name.to_string())
    }
    pub fn is_data_alien(&self) -> bool {
        self.disk_path.is_none() || self.remote_node_name.is_some()
    }
    pub fn disk_name_local(&self) -> String {
        self.disk_path.clone().unwrap().name.clone()
    }
}

#[derive(Debug)]
pub struct BackendPutResult {}

#[derive(Debug)]
pub struct BackendGetResult {
    pub data: BobData,
}

#[derive(Debug)]
pub struct BackendPingResult {}

pub type GetResult = Result<BackendGetResult, Error>;
pub struct Get(pub Pin<Box<dyn Future<Output = GetResult> + Send>>);

pub type PutResult = Result<BackendPutResult, Error>;
pub struct Put(pub Pin<Box<dyn Future<Output = PutResult> + Send>>);

pub type RunResult = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

pub trait BackendStorage {
    fn run_backend(&self) -> RunResult;

    fn put(&self, disk_name: String, vdisk: VDiskId, key: BobKey, data: BobData) -> Put;
    fn put_alien(&self, vdisk: VDiskId, key: BobKey, data: BobData) -> Put;

    fn get(&self, disk_name: String, vdisk: VDiskId, key: BobKey) -> Get;
    fn get_alien(&self, vdisk: VDiskId, key: BobKey) -> Get;
}

pub struct Backend {
    backend: Arc<dyn BackendStorage + Send + Sync>,
    mapper: Arc<VDiskMapper>,
}

impl Backend {
    pub fn new<TSpawner: Spawn + Clone + Send + 'static + Unpin + Sync>(
        mapper: Arc<VDiskMapper>,
        config: &NodeConfig,
        spawner: TSpawner,
    ) -> Self {
        let backend: Arc<dyn BackendStorage + Send + Sync + 'static> = match config.backend_type() {
            BackendType::InMemory => Arc::new(MemBackend::new(mapper.clone())),
            BackendType::Stub => Arc::new(StubBackend {}),
            BackendType::Pearl => Arc::new(PearlBackend::new(mapper.clone(), config, spawner)),
        };
        Backend { backend, mapper }
    }

    pub async fn run_backend(&self) -> Result<(), String> {
        self.backend.run_backend().boxed().await
    }

    pub async fn put(&self, key: BobKey, data: BobData, options: BobOptions) -> PutResult {
        let (vdisk_id, disk_path) = self.mapper.get_operation(key);
        if options.have_remote_node() {
            // write to all remote_nodes
            for node_name in options.remote_nodes.iter(){
                let mut op = BackendOperation::new_alien(vdisk_id.clone());
                op.set_remote_folder(node_name);

                //TODO make it parallel?
                if let Err(err) = Self::put_single(self.backend.clone(), key, data.clone(), op).await {
                    //TODO stop after first error?
                    return Err(err);
                }
            }
            return Ok(BackendPutResult{});
        }
        else if let Some(path) = disk_path { //TODO no case for local and alien write?
            return self.put_local(key, data, BackendOperation::new_local(vdisk_id, path)).await;
        }
        else {
            //todo some cluster put mistake ? 
            return Err(Error::Other);
        }
    }

    pub async fn put_local(&self, key: BobKey, data: BobData, op: BackendOperation) -> PutResult {
        Self::put_single(self.backend.clone(), key, data, op).await
    }

    async fn put_single(backend: Arc<dyn BackendStorage + Send + Sync>, key: BobKey, data: BobData, operation: BackendOperation) -> PutResult {
        if !operation.is_data_alien() {
            debug!("PUT[{}][{}] to backend", key, operation.disk_name_local());
            let result = backend
                .put(
                    operation.disk_name_local(),
                    operation.vdisk_id.clone(),
                    key,
                    data.clone(),
                )
                .0.boxed().await;
            match result {
                Err(err) => {
                    error!(
                        "PUT[{}][{}] to backend. Error: {:?}",
                        key,
                        operation.disk_name_local(),
                        err
                    );
                    backend.put_alien(operation.vdisk_id.clone(), key, data).0.boxed().await
                },
                _ => result,
            }    
        } else {
            debug!(
                "PUT[{}] to backend, alien data for {}",
                key,
                operation.vdisk_id.clone()
            );
            backend.put_alien(operation.vdisk_id.clone(), key, data).0.boxed().await
        }
    }

    pub async fn get(&self, key: BobKey, options: BobOptions) -> GetResult {
        let (vdisk_id, disk_path) = self.mapper.get_operation(key);
        //TODO how read from all alien folders?
        if options.get_alien() {  //TODO check is alien? how? add field to grpc
            trace!("GET[{}] try read alien", key);
            let op = BackendOperation::new_alien(vdisk_id.clone());
            return Self::get_single(self.backend.clone(), key, op).await;
        }
        // we cannot write data to alien if it belong this node
        else if let Some(path) = disk_path.clone() { 
            if options.get_normal() {
                trace!("GET[{}] try read normal", key);
                return self.get_local(key, BackendOperation::new_local(vdisk_id, path)).await;
            }
        }
        error!("we cannot read data from anywhere. path: {:?}, options: {:?}", disk_path, options);
        Err(Error::Failed(format!("we cannot read data from anywhere. path: {:?}, options: {:?}", disk_path, options)))
    }

    pub async fn get_local(&self, key: BobKey, op: BackendOperation) -> GetResult {
        Self::get_single(self.backend.clone(), key, op).await
    }

    async fn get_single(backend: Arc<dyn BackendStorage + Send + Sync>, key: BobKey, operation: BackendOperation) -> GetResult {
        if !operation.is_data_alien() {
            debug!("GET[{}][{}] to backend", key, operation.disk_name_local());
            backend
                .get(operation.disk_name_local(), operation.vdisk_id.clone(), key)
                .0.boxed().await
        } else {
            debug!("GET[{}] to backend, foreign data", key);
            backend.get_alien(operation.vdisk_id.clone(), key).0.boxed().await
        }
    }
}
