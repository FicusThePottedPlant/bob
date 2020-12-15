use super::prelude::*;
use std::time::Duration;

pub(crate) struct Counter {
    count_interval: Duration,
}

impl Counter {
    pub(crate) fn new(count_interval: Duration) -> Self {
        Self { count_interval }
    }

    pub(crate) fn spawn_task(&self, backend: Arc<Backend>) {
        tokio::spawn(Self::task(backend, self.count_interval));
    }

    async fn task(backend: Arc<Backend>, t: Duration) {
        let mut interval = interval(t);
        loop {
            interval.tick().await;
            let (blobs_cnt, aliens_cnt) = backend.blobs_count().await;
            BLOBS_COUNT.value(blobs_cnt);
            ALIEN_BLOBS_COUNT.value(aliens_cnt);
        }
    }
}