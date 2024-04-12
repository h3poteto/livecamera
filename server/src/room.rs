use std::{
    collections::HashMap,
    num::{NonZeroU32, NonZeroU8},
    sync::Arc,
};

use mediasoup::{
    router::{Router, RouterOptions},
    rtp_parameters::{RtcpFeedback, RtpCodecCapability, RtpCodecParametersParameters},
};

use crate::worker::WorkerSet;

pub struct RoomOwner {
    pub rooms: HashMap<String, Arc<Room>>,
}

impl RoomOwner {
    pub fn new() -> Self {
        RoomOwner {
            rooms: HashMap::<String, Arc<Room>>::new(),
        }
    }

    pub fn find_by_id(&self, id: String) -> Option<Arc<Room>> {
        self.rooms.get(&id).cloned()
    }

    pub async fn create_new_room(&mut self, id: String, worker: Arc<WorkerSet>) -> Arc<Room> {
        let room = Room::new(id.clone(), worker).await;
        let a = Arc::new(room);
        self.rooms.insert(id.clone(), a.clone());
        a
    }
}

pub struct Room {
    pub id: String,
    pub router: Router,
    pub worker: Arc<WorkerSet>,
}

impl Room {
    pub async fn new(room_id: String, worker: Arc<WorkerSet>) -> Self {
        let router = worker
            .worker
            .create_router(RouterOptions::new(media_codecs()))
            .await
            .expect("Failed to create router");

        tracing::info!("Room created: {}", room_id);
        Room {
            id: room_id,
            router,
            worker,
        }
    }
}

fn media_codecs() -> Vec<RtpCodecCapability> {
    vec![RtpCodecCapability::Audio {
        mime_type: mediasoup::rtp_parameters::MimeTypeAudio::Opus,
        preferred_payload_type: None,
        clock_rate: NonZeroU32::new(48000).unwrap(),
        channels: NonZeroU8::new(2).unwrap(),
        parameters: RtpCodecParametersParameters::from([("useinbandfec", 1_u32.into())]),
        rtcp_feedback: vec![RtcpFeedback::TransportCc],
    }]
}
