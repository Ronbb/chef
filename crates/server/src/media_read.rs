//! Bound both queued reads and blocking hash/file workers. No unbounded waiting.
use crate::AppError;
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

static ADMISSION: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(20)));

pub(crate) struct ReadPermit {
    _admission: OwnedSemaphorePermit,
    _worker: OwnedSemaphorePermit,
}

pub(crate) async fn acquire(workers: Arc<Semaphore>) -> Result<ReadPermit, AppError> {
    acquire_with(ADMISSION.clone(), workers, Duration::from_secs(2)).await
}

async fn acquire_with(
    admission: Arc<Semaphore>,
    workers: Arc<Semaphore>,
    timeout: Duration,
) -> Result<ReadPermit, AppError> {
    let admitted = admission
        .try_acquire_owned()
        .map_err(|_| AppError::Unavailable)?;
    let worker = tokio::time::timeout(timeout, workers.acquire_owned())
        .await
        .map_err(|_| AppError::Unavailable)?
        .map_err(|_| AppError::Unavailable)?;
    Ok(ReadPermit {
        _admission: admitted,
        _worker: worker,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn admitted(admission: &Semaphore) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while admission.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn short_burst_waits_without_exceeding_worker_or_queue_limits() {
        let admission = Arc::new(Semaphore::new(2));
        let workers = Arc::new(Semaphore::new(1));
        let first = acquire_with(admission.clone(), workers.clone(), Duration::from_secs(1))
            .await
            .unwrap();
        let waiting = tokio::spawn(acquire_with(
            admission.clone(),
            workers.clone(),
            Duration::from_secs(1),
        ));
        admitted(&admission).await;
        assert_eq!(workers.available_permits(), 0);
        assert!(
            acquire_with(admission.clone(), workers.clone(), Duration::from_secs(1))
                .await
                .is_err()
        );
        drop(first);
        let second = waiting.await.unwrap().unwrap();
        assert_eq!(workers.available_permits(), 0);
        drop(second);
        assert_eq!(admission.available_permits(), 2);
        assert_eq!(workers.available_permits(), 1);
    }

    #[tokio::test]
    async fn timeout_and_closed_workers_release_admission() {
        let admission = Arc::new(Semaphore::new(2));
        let workers = Arc::new(Semaphore::new(0));
        assert!(
            acquire_with(
                admission.clone(),
                workers.clone(),
                Duration::from_millis(10)
            )
            .await
            .is_err()
        );
        assert_eq!(admission.available_permits(), 2);
        workers.close();
        assert!(
            acquire_with(admission.clone(), workers, Duration::from_secs(1))
                .await
                .is_err()
        );
        assert_eq!(admission.available_permits(), 2);
    }

    #[tokio::test]
    async fn cancelled_waiter_releases_its_admission() {
        let admission = Arc::new(Semaphore::new(1));
        let workers = Arc::new(Semaphore::new(0));
        let waiting = tokio::spawn(acquire_with(
            admission.clone(),
            workers,
            Duration::from_secs(1),
        ));
        admitted(&admission).await;
        waiting.abort();
        assert!(matches!(waiting.await, Err(error) if error.is_cancelled()));
        assert_eq!(admission.available_permits(), 1);
    }
}
