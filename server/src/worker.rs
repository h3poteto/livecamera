use std::sync::Arc;

use mediasoup::{
    data_structures::{ListenInfo, Protocol},
    webrtc_server::{WebRtcServer, WebRtcServerListenInfos, WebRtcServerOptions},
    worker::{Worker, WorkerLogLevel, WorkerLogTag, WorkerSettings},
    worker_manager::WorkerManager,
};
use num_cpus;

pub struct WorkerOwner {
    pub workers: Vec<Arc<WorkerSet>>,
}

pub struct WorkerSet {
    pub worker: Worker,
    pub server: WebRtcServer,
}

impl WorkerOwner {
    pub async fn new() -> Self {
        let worker_manager = WorkerManager::new();
        let core = num_cpus::get_physical();
        let mut workers: Vec<Arc<WorkerSet>> = Vec::new();

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
                Ok(worker) => {
                    let server = worker
                        .clone()
                        .create_webrtc_server(WebRtcServerOptions::new(
                            WebRtcServerListenInfos::new(ListenInfo {
                                protocol: Protocol::Udp,
                                ip: "0.0.0.0".parse().unwrap(),
                                announced_address: Some("192.168.10.12".to_string()),
                                port: None,
                                flags: None,
                                send_buffer_size: None,
                                recv_buffer_size: None,
                            }),
                        ))
                        .await
                        .expect("Failed to create WebRtcServer");
                    workers.push(Arc::new(WorkerSet { worker, server }));
                }
                Err(err) => {
                    tracing::error!("Failed to create worker: {:?}", err);
                }
            }
        }

        WorkerOwner { workers }
    }

    pub fn choose_worker(&self) -> Option<Arc<WorkerSet>> {
        // TODO: get random worker
        self.workers.first().cloned()
    }
}
