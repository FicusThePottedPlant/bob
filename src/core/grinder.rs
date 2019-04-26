use tokio::prelude::Future;

//use futures::future::;
use crate::core::backend::{
    Backend, BackendError, BackendGetResult, BackendResult,
};
use crate::core::backend::stub_backend::StubBackend;
use crate::core::backend::mem_backend::MemBackend;
use crate::core::data::{BobData, BobError, BobGetResult, BobKey, BobOptions, ClusterResult};
use crate::core::sprinkler::{Sprinkler, SprinklerError, SprinklerResult};
use futures::future::Either;
use crate::core::configs::node::{NodeConfig, BackendType};
use crate::core::data::VDiskMapper;

#[derive(Debug)]
pub enum ServeTypeOk<CT, BT> {
    Cluster(CT),
    Local(BT),
}

impl<CT, BT> ServeTypeOk<CT, BT> {
    pub fn is_cluster(&self) -> bool {
        match *self {
            ServeTypeOk::Cluster(_) => true,
            ServeTypeOk::Local(_) => false,
        }
    }
    pub fn is_local(&self) -> bool {
        !self.is_cluster()
    }
}

#[derive(Debug)]
pub enum ServeTypeError<CT, BT> {
    Cluster(CT),
    Local(BT),
}

impl<CT, BT> ServeTypeError<CT, BT> {
    pub fn is_cluster(&self) -> bool {
        match *self {
            ServeTypeError::Cluster(_) => true,
            ServeTypeError::Local(_) => false,
        }
    }
    pub fn is_local(&self) -> bool {
        !self.is_cluster()
    }
}

pub struct Grinder {
    pub backend: Box<dyn Backend + Send + Sync>,
    pub sprinkler: Sprinkler,
    mapper: VDiskMapper,
}

impl Grinder {
    pub fn new(mapper: VDiskMapper, config: &NodeConfig) -> Grinder {
        let backend: Box<Backend + Send + Sync +'static> = match config.backend_type() {
            BackendType::InMemory => Box::new(MemBackend::new(&mapper)),
            BackendType::Stub => Box::new(StubBackend{}),
        };
        Grinder {
            backend: backend,
            sprinkler: Sprinkler::new(&mapper, config),
            mapper: mapper
        }
    }
    pub fn put(
        &self,
        key: BobKey,
        data: BobData,
        opts: BobOptions,
    ) -> impl Future<
        Item = ServeTypeOk<SprinklerResult, BackendResult>,
        Error = ServeTypeError<SprinklerError, BackendError>,
    >
                 + 'static
                 + Send {
        Box::new(if opts.contains(BobOptions::FORCE_NODE) {
            let op = self.mapper.get_operation(key);
            debug!(
                "PUT[{}] flag FORCE_NODE is on - will handle it by local node. Put params: {}",
                key, op
            );

            Either::A(
                self.backend
                    .put(&op, key, data)
                    .map(|r| ServeTypeOk::Local(r))
                    .map_err(|err| ServeTypeError::Local(err)),
            )
        } else {
            debug!("PUT[{}] will route to cluster", key);
            Either::B(
                self.sprinkler
                    .put_clustered(key, data)
                    .map(|r| ServeTypeOk::Cluster(r))
                    .map_err(|err| ServeTypeError::Cluster(err)),
            )
        })
    }

    pub fn get(
        &self,
        key: BobKey,
        opts: BobOptions,
    ) -> impl Future<
        Item = ServeTypeOk<ClusterResult<BobGetResult>, BackendGetResult>,
        Error = ServeTypeError<BobError, BackendError>,
    >
                 + 'static
                 + Send {
        Box::new(if opts.contains(BobOptions::FORCE_NODE) {
            let op = self.mapper.get_operation(key);
            debug!(
                "GET[{}] flag FORCE_NODE is on - will handle it by local node. Get params: {}",
                key, op
            );
            Either::A(
                self.backend
                    .get(&op, key) // TODO need vdisk and disk path
                    .map(|r| ServeTypeOk::Local(r))
                    .map_err(|err| ServeTypeError::Local(err)),
            )
        } else {
            debug!("GET[{}] will route to cluster", key);
            Either::B(
                self.sprinkler
                    .get_clustered(key)
                    .map(|r| ServeTypeOk::Cluster(r))
                    .map_err(|err| ServeTypeError::Cluster(err)),
            )
        })
    }

    pub fn get_periodic_tasks(
        &self,
        ex: tokio::runtime::TaskExecutor,
    ) -> Box<impl Future<Item = (), Error = ()>> {
        self.sprinkler.get_periodic_tasks(ex)
    }
}
