use super::prelude::*;

pub struct LinkManager {
    repo: Arc<Vec<Node>>,
    check_interval: Duration,
}

pub type ClusterCallType<T> = Result<ClusterResult<T>, ClusterResult<BackendError>>;
pub type ClusterCallFuture<T> = Pin<Box<dyn Future<Output = ClusterCallType<T>> + Send>>;

impl LinkManager {
    pub fn new(nodes: &[Node], check_interval: Duration) -> LinkManager {
        LinkManager {
            repo: Arc::new(nodes.to_vec()),
            check_interval,
        }
    }

    pub async fn get_checker_future(&self, client_factory: BobClientFactory) -> Result<(), ()> {
        let local_repo = self.repo.clone();
        Interval::new_interval(self.check_interval)
            .map(move |_| {
                local_repo.iter().for_each(|v| {
                    let q = v.clone().check(client_factory.clone()).map(|_| {});
                    tokio::spawn(q);
                });
            })
            .collect::<Vec<_>>()
            .boxed()
            .await;
        Ok(())
    }

    pub fn call_nodes<F, T>(nodes: &[Node], mut f: F) -> Vec<ClusterCallFuture<T>>
    where
        F: FnMut(BobClient) -> ClusterCallFuture<T> + Send,
        T: 'static + Send,
    {
        nodes
            .iter()
            .map(move |nl| {
                let nl_clone = nl.clone();
                let client = nl.get_connection();
                match client {
                    Some(conn) => f(conn)
                        .map_err(move |e| {
                            if e.result.is_service() {
                                trace!("clean connection: {}", e.result);
                                nl_clone.clear_connection();
                            }
                            e
                        })
                        .boxed(),
                    None => future::err(ClusterResult {
                        result: BackendError::Failed(format!("No active connection {:?}", nl)),
                        node: nl.clone(),
                    })
                    .boxed(),
                }
            })
            .collect()
    }

    pub fn call_node<F, T>(node: &Node, mut f: F) -> ClusterCallFuture<T>
    where
        F: FnMut(BobClient) -> ClusterCallFuture<T>,
        T: 'static + Send,
    {
        match node.get_connection() {
            Some(conn) => {
                let nl_node = node.clone();
                f(conn)
                    .boxed()
                    .map_err(move |e| {
                        if e.result.is_service() {
                            trace!("clean connection: {}", e.result);
                            nl_node.clear_connection();
                        }
                        e
                    })
                    .boxed()
            }
            None => future::err(ClusterResult {
                result: BackendError::Failed(format!("No active connection {:?}", node)),
                node: node.clone(),
            })
            .boxed(),
        }
    }
}