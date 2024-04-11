use std::sync::Arc;

use mediasoup::{
    worker::{Worker, WorkerLogLevel, WorkerLogTag, WorkerSettings},
    worker_manager::WorkerManager,
};
use num_cpus;

pub struct WorkerOwner {
    pub workers: Vec<Arc<Worker>>,
}

impl WorkerOwner {
    pub async fn new() -> Self {
        let worker_manager = WorkerManager::new();
        let core = num_cpus::get_physical();
        let mut workers: Vec<Arc<Worker>> = Vec::new();

        for _ in 0..core {
            let result = worker_manager
                .create_worker({
                    let mut settings = WorkerSettings::default();
                    settings.log_level = WorkerLogLevel::Debug;
                    settings.log_tags = vec![
                        WorkerLogTag::Info,
                        WorkerLogTag::Ice,
                        WorkerLogTag::Dtls,
                        WorkerLogTag::Rtp,
                        WorkerLogTag::Srtp,
                        WorkerLogTag::Rtcp,
                        WorkerLogTag::Rtx,
                        WorkerLogTag::Bwe,
                        WorkerLogTag::Score,
                        WorkerLogTag::Simulcast,
                        WorkerLogTag::Svc,
                        WorkerLogTag::Sctp,
                        WorkerLogTag::Message,
                    ];
                    settings
                })
                .await;
            match result {
                Ok(worker) => workers.push(Arc::new(worker)),
                Err(err) => {
                    tracing::error!("Failed to create worker: {:?}", err);
                }
            }
        }

        WorkerOwner { workers }
    }

    pub fn choose_worker(&self) -> Option<Arc<Worker>> {
        // TODO: get random worker
        self.workers.first().cloned()
    }
}
