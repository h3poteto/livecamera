use rheomesh;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use crate::websocket::WebSocket;
use actix::Addr;

pub struct RoomOwner {
    pub rooms: HashMap<String, Arc<Room>>,
    worker: Arc<Mutex<rheomesh::worker::Worker>>,
}

impl RoomOwner {
    pub async fn new() -> Self {
        let worker = rheomesh::worker::Worker::new(rheomesh::config::WorkerConfig {
            relay_sender_port: 9441,
            relay_server_udp_port: 9442,
            relay_server_tcp_port: 9443,
        })
        .await
        .unwrap();
        RoomOwner {
            rooms: HashMap::<String, Arc<Room>>::new(),
            worker,
        }
    }

    pub fn find_by_id(&self, id: String) -> Option<Arc<Room>> {
        self.rooms.get(&id).cloned()
    }

    pub async fn create_new_room(
        &mut self,
        id: String,
        config: rheomesh::config::MediaConfig,
    ) -> Arc<Room> {
        let mut worker = self.worker.lock().await;
        let router = worker.new_router(config);
        let room = Room::new(id.clone(), router);
        let a = Arc::new(room);
        self.rooms.insert(id.clone(), a.clone());
        a
    }

    pub fn remove_room(&mut self, room_id: String) {
        self.rooms.remove(&room_id);
    }
}

pub struct Room {
    pub id: String,
    pub router: Arc<Mutex<rheomesh::router::Router>>,
    users: std::sync::Mutex<Vec<Addr<WebSocket>>>,
}

impl Room {
    pub fn new(id: String, router: Arc<Mutex<rheomesh::router::Router>>) -> Self {
        Self {
            id,
            router,
            users: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn add_user(&self, user: Addr<WebSocket>) {
        let mut users = self.users.lock().unwrap();
        users.push(user);
    }

    pub fn remove_user(&self, user: Addr<WebSocket>) -> usize {
        let mut users = self.users.lock().unwrap();
        users.retain(|u| u != &user);
        users.len()
    }

    pub fn get_peers(&self, user: &Addr<WebSocket>) -> Vec<Addr<WebSocket>> {
        let users = self.users.lock().unwrap();
        users.iter().filter(|u| u != &user).cloned().collect()
    }
}
