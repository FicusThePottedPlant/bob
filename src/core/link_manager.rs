use crate::core::bob_client::{BobClient, BobClientFactory};
use crate::core::data::{BobError, ClusterResult, Node};
use futures::future::Either;
use futures::future::Future;
use futures::future::*;
use futures::stream::Stream;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::timer::Interval;

use futures03::future::err as err2;
use futures03::Future as NewFuture;
use futures03::future::{TryFutureExt, FutureExt};
use std::pin::Pin;
use futures03::stream::FuturesUnordered as unordered;

pub struct NodeLink {
    pub node: Node,
    pub conn: Option<BobClient>,
}

#[derive(Clone)]
pub struct NodeLinkHolder {
    node: Node,
    conn: Arc<Mutex<Option<BobClient>>>,
}

impl NodeLinkHolder {
    pub fn new(node: Node) -> NodeLinkHolder {
        NodeLinkHolder {
            node,
            conn: Arc::new(Mutex::new(None)), // TODO: consider to use RwLock
        }
    }

    pub fn get_connection(&self) -> NodeLink {
        NodeLink {
            node: self.node.clone(),
            conn: self.conn.lock().unwrap().clone(),
        }
    }

    pub fn set_connection(&self, client: BobClient) {
        *self.conn.lock().unwrap() = Some(client);
    }

    pub fn clear_connection(&self) {
        *self.conn.lock().unwrap() = None;
    }

    fn check(&self, client_fatory: BobClientFactory) -> impl Future<Item = (), Error = ()> {
        match self.get_connection().conn {
            Some(mut conn) => {
                let nlh = self.clone();
                Either::A(conn.ping().then(move |r| {
                    match r {
                        Ok(_) => debug!("All good with pinging node {:?}", nlh.node),
                        Err(_) => {
                            debug!("Got broken connection to node {:?}", nlh.node);
                            nlh.clear_connection();
                        }
                    };
                    Ok(())
                }))
            }
            None => {
                let nlh = self.clone();
                debug!("will connect to {:?}", nlh.node);
                Either::B(client_fatory.produce(nlh.node.clone()).map(move |client| {
                    nlh.set_connection(client);
                }))
            }
        }
    }
}

pub struct LinkManager {
    repo: Arc<HashMap<Node, NodeLinkHolder>>,
    check_interval: Duration,
    timeout: Duration,
}

impl LinkManager {
    pub fn new(nodes: Vec<Node>, check_interval: Duration, timeout: Duration) -> LinkManager {
        LinkManager {
            repo: {
                let mut hm = HashMap::new();
                for node in nodes {
                    hm.insert(node.clone(), NodeLinkHolder::new(node));
                }
                Arc::new(hm)
            },
            check_interval,
            timeout,
        }
    }

    pub fn get_checker_future(
        &self,
        ex: tokio::runtime::TaskExecutor,
    ) -> Box<impl Future<Item = (), Error = ()>> {
        let local_repo = self.repo.clone();
        let client_factory = BobClientFactory {
            executor: ex,
            timeout: self.timeout,
        };
        Box::new(
            Interval::new_interval(self.check_interval)
                .for_each(move |_| {
                    local_repo.values().for_each(|v| {
                        tokio::spawn(v.check(client_factory.clone()));
                    });

                    Ok(())
                })
                .map_err(|e| panic!("can't make to work timer {:?}", e)),
        )
    }

    pub fn get_link(&self, node: &Node) -> NodeLink {
        self.repo
            .get(node)
            .expect("No such node in repo. Check config and cluster setup")
            .get_connection()
    }

    pub fn get_connections(&self, nodes: &[Node]) -> Vec<NodeLink> {
        nodes.iter().map(|n| self.get_link(n)).collect()
    }
    pub fn call_nodes<F, T: 'static + Send>(
        &self,
        nodes: &[Node],
        mut f: F,
    ) -> Vec<Box<dyn Future<Item = ClusterResult<T>, Error = ClusterResult<BobError>> + 'static + Send>>
    where
        F: FnMut(
            &mut BobClient,
        ) -> (Box< dyn
            Future<Item = ClusterResult<T>, Error = ClusterResult<BobError>> + 'static + Send,
        >),
    {
        let links = &mut self.get_connections(nodes);
        let t: Vec<_> = links
            .iter_mut()
            .map(move |nl| {
                let node = nl.node.clone();
                match &mut nl.conn {
                    Some(conn) => f(conn),
                    None => Box::new(err(ClusterResult {
                        result: BobError::Other(format!("No active connection {:?}", node)),
                        node,
                    })),
                }
            })
            .collect();
        t
    }

    pub fn call_nodes2<F, T: 'static + Send>(
        &self,
        nodes: &[Node],
        mut f: F,
    ) -> Vec<Pin<Box<dyn NewFuture<Output = Result<ClusterResult<T>, ClusterResult<BobError>>> + 'static + Send>>>
    where
        F: FnMut(
            &mut BobClient,
        ) -> (Pin<Box<dyn NewFuture<Output = Result<ClusterResult<T>, ClusterResult<BobError>>> + 'static + Send>>),
    {
        let links = &mut self.get_connections(nodes);
        let t: Vec<_> = links
            .iter_mut()
            .map(move |nl| {
                let node = nl.node.clone();
                match &mut nl.conn {
                    Some(conn) => f(conn),
                    None => err2(ClusterResult {
                        result: BobError::Other(format!("No active connection {:?}", node)),
                        node,
                    }).boxed(),
                }
            })
            .collect();
        t
    }

    pub fn call_nodes3<F, T: 'static + Send>(
        &self,
        nodes: &[Node],
        mut f: F,
    ) -> unordered<Pin<Box<dyn NewFuture<Output = Result<ClusterResult<T>, ClusterResult<BobError>>> + 'static + Send>>>
    where
        F: FnMut(
            &mut BobClient,
        ) -> (Pin<Box<dyn NewFuture<Output = Result<ClusterResult<T>, ClusterResult<BobError>>> + 'static + Send>>),
    {
        let links = &mut self.get_connections(nodes);
        let t: unordered<_> = links
            .iter_mut()
            .map(move |nl| {
                let node = nl.node.clone();
                match &mut nl.conn {
                    Some(conn) => f(conn),
                    None => err2(ClusterResult {
                        result: BobError::Other(format!("No active connection {:?}", node)),
                        node,
                    }).boxed(),
                }
            })
            .collect();
        t
    }
}
