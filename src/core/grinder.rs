use super::prelude::*;

/// Struct for cooperation backend, link manager and cluster
pub struct Grinder {
    backend: Arc<Backend>,
    link_manager: Arc<LinkManager>,
    cluster: Arc<dyn Cluster + Send + Sync>,
}

impl Grinder {
    /// Creates new instance of the Grinder
    pub fn new(mapper: Virtual, config: &NodeConfig) -> Grinder {
        let link_manager = Arc::new(LinkManager::new(
            mapper.nodes().to_vec(),
            config.check_interval(),
        ));
        let mapper = Arc::new(mapper);
        let backend = Arc::new(Backend::new(mapper.clone(), config));
        Grinder {
            backend: backend.clone(),
            link_manager,
            cluster: get_cluster(mapper, config, backend),
        }
    }

    pub(crate) fn backend(&self) -> &Arc<Backend> {
        &self.backend
    }

    pub(crate) async fn run_backend(&self) -> Result<()> {
        self.backend.run_backend().await
    }

    pub(crate) async fn put(
        &self,
        key: BobKey,
        data: BobData,
        opts: BobOptions,
    ) -> Result<(), Error> {
        let sw = Stopwatch::start_new();
        if opts.flags().contains(BobFlags::FORCE_NODE) {
            trace!(">>>- - - - - GRINDER PUT START - - - - -");
            debug!(
                "PUT[{}] FORCE_NODE=true - will handle it by local node. Put params: {:?}",
                key, opts
            );
            CLIENT_PUT_COUNTER.count(1);
            let time = CLIENT_PUT_TIMER.start();

            let result = self.backend.put(key, data, opts).await;
            trace!(
                "backend processed put, /{:.3}ms/",
                sw.elapsed().as_secs_f64() * 1000.0
            );
            if result.is_err() {
                CLIENT_PUT_ERROR_COUNT_COUNTER.count(1);
            }

            CLIENT_PUT_TIMER.stop(time);
            trace!("<<<- - - - - GRINDER PUT FINISH - - - - -");
            result
        } else {
            debug!("PUT[{}] will route to cluster", key);
            GRINDER_PUT_COUNTER.count(1);
            let time = GRINDER_PUT_TIMER.start();

            let result = self.cluster.put(key, data).await;
            if result.is_err() {
                GRINDER_PUT_ERROR_COUNT_COUNTER.count(1);
            }

            GRINDER_PUT_TIMER.stop(time);
            trace!(">>>- - - - - GRINDER PUT FINISH - - - - -");
            result
        }
    }

    pub(crate) async fn get(&self, key: BobKey, opts: &BobOptions) -> Result<BobData, Error> {
        trace!(">>>- - - - - GRINDER GET START - - - - -");
        let sw = Stopwatch::start_new();
        if opts.flags().contains(BobFlags::FORCE_NODE) {
            trace!(
                "pass request to backend, /{:.3}ms/",
                sw.elapsed().as_secs_f64() * 1000.0
            );
            CLIENT_GET_COUNTER.count(1);
            let time = CLIENT_GET_TIMER.start();

            debug!(
                "GET[{}] flag FORCE_NODE is on - will handle it by local node. Get params: {:?}",
                key, opts
            );
            let result = self.backend.get(key, opts).await;
            trace!(
                "backend processed get, /{:.3}ms/",
                sw.elapsed().as_secs_f64() * 1000.0
            );
            if result.is_err() {
                CLIENT_GET_ERROR_COUNT_COUNTER.count(1);
            }

            CLIENT_GET_TIMER.stop(time);
            trace!(">>>- - - - - GRINDER PUT FINISHED - - - - -");
            result
        } else {
            trace!(
                "pass request to cluster, /{:.3}ms/",
                sw.elapsed().as_secs_f64() * 1000.0
            );
            GRINDER_GET_COUNTER.count(1);
            let time = GRINDER_GET_TIMER.start();
            debug!("GET[{}] will route to cluster", key);
            let result = self.cluster.get(key).await;
            trace!(
                "cluster processed get, /{:.3}ms/",
                sw.elapsed().as_secs_f64() * 1000.0
            );
            if result.is_err() {
                GRINDER_GET_ERROR_COUNT_COUNTER.count(1);
            }
            GRINDER_GET_TIMER.stop(time);
            trace!(">>>- - - - - GRINDER PUT FINISHED - - - - -");
            result
        }
    }

    pub(crate) async fn exist(
        &self,
        keys: &[BobKey],
        opts: &BobOptions,
    ) -> Result<Vec<bool>, Error> {
        if opts.flags().contains(BobFlags::FORCE_NODE) {
            self.backend.exist(keys, opts).await
        } else {
            self.cluster.exist(keys).await
        }
    }

    #[inline]
    pub(crate) fn run_periodic_tasks(&self, client_factory: Factory) {
        self.link_manager.spawn_checker(client_factory);
    }
}

impl Debug for Grinder {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Grinder")
            .field("backend", &self.backend)
            .field("link_manager", &self.link_manager)
            .field("cluster", &"..")
            .finish()
    }
}