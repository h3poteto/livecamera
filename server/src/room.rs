use std::{
    collections::HashMap,
    num::{NonZeroU32, NonZeroU8},
    sync::{Arc, Mutex},
};

use actix::Addr;
use mediasoup::{
    producer::{Producer, ProducerId},
    router::{Router, RouterOptions},
    rtp_parameters::{RtcpFeedback, RtpCodecCapability, RtpCodecParametersParameters},
};

use crate::{
    websocket::{SendingMessage, WebSocket},
    worker::WorkerSet,
};

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
    users: Mutex<Vec<Addr<WebSocket>>>,
    pdocuers: Mutex<HashMap<ProducerId, Arc<Producer>>>,
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
            users: Mutex::new(Vec::new()),
            pdocuers: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_user(&self, user: Addr<WebSocket>) {
        let mut users = self.users.lock().unwrap();
        users.push(user);
    }

    pub fn remove_user(&self, user: Addr<WebSocket>) {
        let mut users = self.users.lock().unwrap();
        users.retain(|u| u != &user);
    }

    pub fn add_producer(&self, producer_id: ProducerId, producer: Arc<Producer>) {
        let mut producers = self.pdocuers.lock().unwrap();
        producers.insert(producer_id, producer);
    }

    pub fn remove_producer(&self, producer_id: ProducerId) {
        let mut producers = self.pdocuers.lock().unwrap();
        producers.remove(&producer_id);
    }

    pub fn broadcast_producer(&self, producer_id: ProducerId) {
        let ids = vec![producer_id];
        let users = self.users.lock().unwrap();
        for user in users.iter() {
            user.do_send(SendingMessage::NewProducers { ids: ids.clone() });
        }
    }
}

fn media_codecs() -> Vec<RtpCodecCapability> {
    vec![
        RtpCodecCapability::Audio {
            mime_type: mediasoup::rtp_parameters::MimeTypeAudio::Opus,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(48000).unwrap(),
            channels: NonZeroU8::new(2).unwrap(),
            parameters: RtpCodecParametersParameters::from([("useinbandfec", 1_u32.into())]),
            rtcp_feedback: vec![RtcpFeedback::TransportCc],
        },
        RtpCodecCapability::Video {
            mime_type: mediasoup::rtp_parameters::MimeTypeVideo::Vp8,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(90000).unwrap(),
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ],
        },
    ]
}
