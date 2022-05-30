use async_trait::async_trait;
use semver::Version;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[async_trait]
pub trait Fetcher {
    type Error: std::error::Error;
    async fn fetch(&self, ver: &Version) -> Result<PathBuf, Self::Error>;
}

pub struct DownloadCache<D> {
    cache: parking_lot::Mutex<HashMap<Version, Arc<tokio::sync::RwLock<Option<PathBuf>>>>>,
    fetcher: D,
}

impl<D> DownloadCache<D> {
    pub fn new(fetcher: D) -> Self {
        DownloadCache {
            cache: Default::default(),
            fetcher,
        }
    }

    async fn try_get(&self, ver: &Version) -> Option<PathBuf> {
        let entry = {
            let cache = self.cache.lock();
            cache.get(ver).cloned()
        };
        match entry {
            Some(lock) => {
                let file = lock.read().await;
                file.as_ref().cloned()
            }
            None => None,
        }
    }
}

impl<D: Fetcher> DownloadCache<D> {
    pub async fn get(&self, ver: &Version) -> Result<PathBuf, D::Error> {
        match self.try_get(ver).await {
            Some(file) => Ok(file),
            None => self.fetch(ver).await,
        }
    }

    async fn fetch(&self, ver: &Version) -> Result<PathBuf, D::Error> {
        let lock = {
            let mut cache = self.cache.lock();
            Arc::clone(cache.entry(ver.clone()).or_default())
        };
        let mut entry = lock.write().await;
        match entry.as_ref() {
            Some(file) => Ok(file.clone()),
            None => {
                log::info!(target: "compiler_cache", "installing file version {}", ver);
                let file = self.fetcher.fetch(ver).await?;
                *entry = Some(file.clone());
                Ok(file)
            }
        }
    }
}

impl<D: Default> Default for DownloadCache<D> {
    fn default() -> Self {
        DownloadCache::new(D::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{executor::block_on, join, pin_mut};
    use std::time::Duration;
    use thiserror::Error;
    use tokio::{spawn, task::yield_now, time::timeout};

    #[derive(Error, Debug)]
    enum MockError {}

    /// Tests, that caching works, meaning that cache downloads each version only once
    #[test]
    fn caches() {
        #[derive(Default)]
        struct MockFetcher {
            counter: parking_lot::Mutex<HashMap<Version, u32>>,
        }

        #[async_trait]
        impl Fetcher for MockFetcher {
            type Error = MockError;
            async fn fetch(&self, ver: &Version) -> Result<PathBuf, Self::Error> {
                *self.counter.lock().entry(ver.clone()).or_default() += 1;
                Ok(PathBuf::from(ver.to_string()))
            }
        }

        let fetcher = MockFetcher::default();
        let cache = DownloadCache::new(fetcher);

        let vers: Vec<_> = vec![(0, 1, 2), (1, 2, 3), (3, 3, 3)]
            .into_iter()
            .map(|ver| Version::new(ver.0, ver.1, ver.2))
            .collect();

        block_on(cache.get(&vers[0])).unwrap();
        block_on(cache.get(&vers[1])).unwrap();
        block_on(cache.get(&vers[0])).unwrap();
        block_on(cache.get(&vers[0])).unwrap();
        block_on(cache.get(&vers[1])).unwrap();
        block_on(cache.get(&vers[1])).unwrap();
        block_on(cache.get(&vers[2])).unwrap();
        block_on(cache.get(&vers[2])).unwrap();
        block_on(cache.get(&vers[1])).unwrap();
        block_on(cache.get(&vers[0])).unwrap();

        let counter = cache.fetcher.counter.lock();
        assert_eq!(counter.len(), 3);
        for (_, count) in counter.iter() {
            assert_eq!(*count, 1);
        }
    }

    /// Tests, that cache will not block requests for already downloaded values,
    /// while it downloads others
    #[tokio::test]
    async fn not_blocking() {
        const TIMEOUT: Duration = Duration::from_secs(10);

        struct MockBlockingFetcher {
            sync: Arc<tokio::sync::Mutex<()>>,
        }

        #[async_trait]
        impl Fetcher for MockBlockingFetcher {
            type Error = MockError;
            async fn fetch(&self, ver: &Version) -> Result<PathBuf, Self::Error> {
                self.sync.lock().await;
                Ok(PathBuf::from(ver.to_string()))
            }
        }

        let sync = Arc::<tokio::sync::Mutex<()>>::default();
        let fetcher = MockBlockingFetcher { sync: sync.clone() };
        let cache = Arc::new(DownloadCache::new(fetcher));

        let vers: Vec<_> = (0..3).map(|x| Version::new(x, 0, 0)).collect();

        // fill the cache
        cache.get(&vers[1]).await.unwrap();

        // lock the fetcher
        let guard = sync.lock().await;

        // try to download (it will block on mutex)
        let handle = {
            let cache = cache.clone();
            let vers = vers.clone();
            spawn(async move { join!(cache.get(&vers[0]), cache.get(&vers[2])) })
        };
        // so we could rerun future after timeout
        pin_mut!(handle);
        // give the thread to the scheduler so it could run "handle" task
        yield_now().await;

        // check, that while we're downloading we don't block the cache
        timeout(TIMEOUT, cache.get(&vers[1]))
            .await
            .expect("should not block")
            .expect("expected value not error");

        // check, that we're blocked on downloading
        timeout(Duration::from_millis(100), &mut handle)
            .await
            .expect_err("should block");

        // release the lock
        std::mem::drop(guard);

        // now we can finish downloading
        let vals = timeout(TIMEOUT, handle)
            .await
            .expect("should not block")
            .unwrap();
        vals.0.expect("expected value got error");
        vals.1.expect("expected value got error");
    }
}
