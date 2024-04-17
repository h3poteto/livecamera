use mediasoup::{
    data_structures::{ListenInfo, Protocol},
    webrtc_server::{WebRtcServer, WebRtcServerListenInfos, WebRtcServerOptions},
    worker::{Worker, WorkerLogLevel, WorkerLogTag, WorkerSettings},
    worker_manager::WorkerManager,
};
use num_cpus;
use rand::Rng;
use std::{env, sync::Arc};

const PORTS: [u16; 32] = [
    48300, 48301, 48302, 48303, 48304, 48305, 48306, 48307, 48308, 48309, 48310, 48311, 48312,
    48313, 48314, 48315, 48316, 48317, 48318, 48319, 48320, 48321, 48322, 48323, 48324, 48325,
    48326, 48327, 48328, 48329, 48330, 48331,
];

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
        let core = if let Ok(num_core) =
            env::var("NUM_CPU").map(|v| v.parse::<usize>().expect("NUM_CPU must be an integer"))
        {
            num_core
        } else {
            num_cpus::get_physical()
        };

        let mut workers: Vec<Arc<WorkerSet>> = Vec::new();

        let host = env::var("PUBLIC_IP").expect("PUBLIC_IP must be set");

        for i in 0..core {
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
                                announced_address: Some(host.clone()),
                                port: Some(PORTS[i]),
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
        let length = self.workers.len();
        if length == 0 {
            return None;
        }
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..length);
        Some(self.workers[index].clone())
    }
}
