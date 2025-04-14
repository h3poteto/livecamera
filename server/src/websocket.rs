use std::{
    collections::HashMap,
    env,
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
};

use actix::{Actor, AsyncContext, Handler, Message, StreamHandler};
use actix_web::web::Data;
use actix_web_actors::ws;
use rheomesh::{self, publisher::Publisher, subscriber::Subscriber, transport::Transport};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use webrtc::{
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer},
    peer_connection::sdp::session_description::RTCSessionDescription,
};

use crate::room;

pub struct WebSocket {
    owner: Data<Mutex<room::RoomOwner>>,
    room: Arc<room::Room>,
    publish_transport: Arc<rheomesh::publish_transport::PublishTransport>,
    subscribe_transport: Arc<rheomesh::subscribe_transport::SubscribeTransport>,
    publishers: Arc<Mutex<HashMap<String, Arc<Mutex<Publisher>>>>>,
    subscribers: Arc<Mutex<HashMap<String, Arc<Mutex<Subscriber>>>>>,
}

impl WebSocket {
    // This function is called when a new user connect to this server.
    pub async fn new(room: Arc<room::Room>, owner: Data<Mutex<room::RoomOwner>>) -> Self {
        tracing::info!("Starting WebSocket");
        let r = room.router.clone();
        let router = r.lock().await;

        let mut config = rheomesh::config::WebRTCTransportConfig::default();
        // Public IP address of your server.
        let ip = env::var("PUBLIC_IP").expect("PUBLIC_IP is required");
        let ipv4 = ip
            .parse::<Ipv4Addr>()
            .expect("failed to parse public IP address");
        config.announced_ips = vec![IpAddr::V4(ipv4)];
        config.configuration.ice_servers = vec![
            RTCIceServer {
                urls: vec!["stun:ice.home.h3poteto.dev:3478".to_owned()],
                username: "root".to_owned(),
                credential: "homecluster".to_owned(),
            },
            RTCIceServer {
                urls: vec!["turn:ice.home.h3poteto.dev:3478?transport=udp".to_owned()],
                username: "root".to_owned(),
                credential: "homecluster".to_owned(),
            },
            RTCIceServer {
                urls: vec!["turns:ice.home.h3poteto.dev:5349?transport=tcp".to_owned()],
                username: "root".to_owned(),
                credential: "homecluster".to_owned(),
            },
        ];
        // Port range of your server.
        config.port_range = Some(rheomesh::config::PortRange {
            min: 31300,
            max: 31331,
        });

        let publish_transport = router.create_publish_transport(config.clone()).await;
        let subscribe_transport = router.create_subscribe_transport(config).await;
        Self {
            owner,
            room,
            publish_transport: Arc::new(publish_transport),
            subscribe_transport: Arc::new(subscribe_transport),
            publishers: Arc::new(Mutex::new(HashMap::new())),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Actor for WebSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("New WebSocket connection is started");
        let address = ctx.address();
        self.room.add_user(address.clone());
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        tracing::info!("The WebSocket connection is stopped");
        let address = ctx.address();
        let subscribe_transport = self.subscribe_transport.clone();
        let publish_transport = self.publish_transport.clone();
        actix::spawn(async move {
            subscribe_transport
                .close()
                .await
                .expect("failed to close subscribe_transport");
            publish_transport
                .close()
                .await
                .expect("failed to close publish_transport");
        });
        let users = self.room.remove_user(address);
        if users == 0 {
            let owner = self.owner.clone();
            let router = self.room.router.clone();
            let room_id = self.room.id.clone();
            actix::spawn(async move {
                let mut owner = owner.lock().await;
                owner.remove_room(room_id);
                let router = router.lock().await;
                router.close();
            });
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => tracing::info!("Pong received"),
            Ok(ws::Message::Text(text)) => match serde_json::from_str::<ReceivedMessage>(&text) {
                Ok(message) => {
                    ctx.address().do_send(message);
                }
                Err(error) => {
                    tracing::error!("Failed to parse client message: {}\n{}", error, text);
                }
            },
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(reason)) => ctx.close(reason),
            _ => (),
        }
    }
}
impl Handler<ReceivedMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, msg: ReceivedMessage, ctx: &mut Self::Context) -> Self::Result {
        let address = ctx.address();
        tracing::debug!("received message: {:?}", msg);

        match msg {
            ReceivedMessage::Ping => {
                address.do_send(SendingMessage::Pong);
            }
            ReceivedMessage::PublisherInit => {
                let publish_transport = self.publish_transport.clone();
                tokio::spawn(async move {
                    let addr = address.clone();
                    publish_transport
                        .on_ice_candidate(Box::new(move |candidate| {
                            let init = candidate.to_json().expect("failed to parse candidate");
                            addr.do_send(SendingMessage::PublisherIce { candidate: init });
                        }))
                        .await;
                });
            }
            ReceivedMessage::SubscriberInit => {
                let subscribe_transport = self.subscribe_transport.clone();
                let room = self.room.clone();
                tokio::spawn(async move {
                    let addr = address.clone();
                    let addr2 = address.clone();
                    subscribe_transport
                        .on_ice_candidate(Box::new(move |candidate| {
                            let init = candidate.to_json().expect("failed to parse candidate");
                            addr.do_send(SendingMessage::SubscriberIce { candidate: init });
                        }))
                        .await;
                    subscribe_transport
                        .on_negotiation_needed(Box::new(move |offer| {
                            addr2.do_send(SendingMessage::Offer { sdp: offer });
                        }))
                        .await;

                    let router = room.router.lock().await;
                    let ids = router.publisher_ids();
                    tracing::info!("router publisher ids {:#?}", ids);
                    address.do_send(SendingMessage::Published { publisher_ids: ids });
                });
            }

            ReceivedMessage::RequestPublish => address.do_send(SendingMessage::StartAsPublisher),
            ReceivedMessage::PublisherIce { candidate } => {
                let publish_transport = self.publish_transport.clone();
                actix::spawn(async move {
                    let _ = publish_transport
                        .add_ice_candidate(candidate)
                        .await
                        .expect("failed to add ICE candidate");
                });
            }
            ReceivedMessage::SubscriberIce { candidate } => {
                let subscribe_transport = self.subscribe_transport.clone();
                actix::spawn(async move {
                    let _ = subscribe_transport
                        .add_ice_candidate(candidate)
                        .await
                        .expect("failed to add ICE candidate");
                });
            }
            ReceivedMessage::Offer { sdp } => {
                let publish_transport = self.publish_transport.clone();
                actix::spawn(async move {
                    let answer = publish_transport
                        .get_answer(sdp)
                        .await
                        .expect("failed to connect publish_transport");

                    address.do_send(SendingMessage::Answer { sdp: answer });
                });
            }
            ReceivedMessage::Subscribe {
                publisher_id: track_id,
            } => {
                let subscribe_transport = self.subscribe_transport.clone();
                let subscribers = self.subscribers.clone();
                actix::spawn(async move {
                    let (subscriber, offer) = subscribe_transport
                        .subscribe(track_id)
                        .await
                        .expect("failed to connect subscribe_transport");

                    #[allow(unused)]
                    let mut id = "".to_owned();
                    {
                        let guard = subscriber.lock().await;
                        id = guard.id.clone();
                    }
                    let mut s = subscribers.lock().await;
                    s.insert(id.clone(), subscriber);
                    address.do_send(SendingMessage::Offer { sdp: offer });
                    address.do_send(SendingMessage::Subscribed { subscriber_id: id })
                });
            }
            ReceivedMessage::Answer { sdp } => {
                let subscribe_transport = self.subscribe_transport.clone();
                actix::spawn(async move {
                    let _ = subscribe_transport
                        .set_answer(sdp)
                        .await
                        .expect("failed to set answer");
                });
            }
            ReceivedMessage::Publish { track_id } => {
                let room = self.room.clone();
                let publish_transport = self.publish_transport.clone();
                let publishers = self.publishers.clone();
                actix::spawn(async move {
                    match publish_transport.publish(track_id).await {
                        Ok(publisher) => {
                            #[allow(unused)]
                            let mut track_id = "".to_owned();
                            {
                                let publisher = publisher.lock().await;
                                track_id = publisher.track_id.clone();
                            }
                            tracing::debug!("published a track: {}", track_id);
                            // address.do_send(SendingMessage::Published {
                            //     track_id: id.clone(),
                            // });
                            let mut p = publishers.lock().await;
                            p.insert(track_id.clone(), publisher.clone());
                            room.get_peers(&address).iter().for_each(|peer| {
                                peer.do_send(SendingMessage::Published {
                                    publisher_ids: vec![track_id.clone()],
                                });
                            });
                        }
                        Err(err) => {
                            tracing::error!("{}", err);
                        }
                    }
                });
            }
            ReceivedMessage::StopPublish { publisher_id } => {
                let publishers = self.publishers.clone();
                actix::spawn(async move {
                    let mut p = publishers.lock().await;
                    if let Some(publisher) = p.remove(&publisher_id) {
                        let publisher = publisher.lock().await;
                        publisher.close().await;
                    }
                });
            }
            ReceivedMessage::StopSubscribe { subscriber_id } => {
                let subscribers = self.subscribers.clone();
                actix::spawn(async move {
                    let mut s = subscribers.lock().await;
                    if let Some(subscriber) = s.remove(&subscriber_id) {
                        let subscriber = subscriber.lock().await;
                        subscriber.close().await;
                    }
                });
            }
            ReceivedMessage::SetPreferredLayer {
                subscriber_id,
                sid,
                tid,
            } => {
                let subscribers = self.subscribers.clone();
                actix::spawn(async move {
                    let s = subscribers.lock().await;
                    if let Some(subscriber) = s.get(&subscriber_id) {
                        let mut subscriber = subscriber.lock().await;
                        if let Err(err) = subscriber.set_preferred_layer(sid, tid).await {
                            tracing::error!("Failed to set preferred layer: {}", err);
                        }
                    }
                });
            }
            ReceivedMessage::RestartICE {} => {
                let subscribe_transport = self.subscribe_transport.clone();
                actix::spawn(async move {
                    match subscribe_transport.restart_ice().await {
                        Ok(offer) => {
                            address.do_send(SendingMessage::Offer { sdp: offer });
                        }
                        Err(err) => {
                            tracing::error!("Failed to restart ICE: {}", err);
                        }
                    }
                });
            }
        }
    }
}

impl Handler<SendingMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, msg: SendingMessage, ctx: &mut Self::Context) -> Self::Result {
        tracing::debug!("sending message: {:?}", msg);
        ctx.text(serde_json::to_string(&msg).expect("failed to parse SendingMessage"));
    }
}

impl Handler<InternalMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, _msg: InternalMessage, _ctx: &mut Self::Context) -> Self::Result {}
}

#[derive(Deserialize, Message, Debug)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum ReceivedMessage {
    #[serde(rename_all = "camelCase")]
    Ping,
    #[serde(rename_all = "camelCase")]
    PublisherInit,
    #[serde(rename_all = "camelCase")]
    SubscriberInit,
    #[serde(rename_all = "camelCase")]
    RequestPublish,
    // Seems like client-side (JS) RTCIceCandidate struct is equal RTCIceCandidateInit.
    #[serde(rename_all = "camelCase")]
    PublisherIce { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    SubscriberIce { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    Offer { sdp: RTCSessionDescription },
    #[serde(rename_all = "camelCase")]
    Subscribe { publisher_id: String },
    #[serde(rename_all = "camelCase")]
    Answer { sdp: RTCSessionDescription },
    #[serde(rename_all = "camelCase")]
    Publish { track_id: String },
    #[serde(rename_all = "camelCase")]
    StopPublish { publisher_id: String },
    #[serde(rename_all = "camelCase")]
    StopSubscribe { subscriber_id: String },
    #[serde(rename_all = "camelCase")]
    SetPreferredLayer {
        subscriber_id: String,
        sid: u8,
        tid: Option<u8>,
    },
    #[serde(rename_all = "camelCase")]
    RestartICE,
}

#[derive(Serialize, Message, Debug)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum SendingMessage {
    #[serde(rename_all = "camelCase")]
    Pong,
    #[serde(rename_all = "camelCase")]
    StartAsPublisher,
    #[serde(rename_all = "camelCase")]
    Answer { sdp: RTCSessionDescription },
    #[serde(rename_all = "camelCase")]
    Offer { sdp: RTCSessionDescription },
    #[serde(rename_all = "camelCase")]
    PublisherIce { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    SubscriberIce { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    Published { publisher_ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Subscribed { subscriber_id: String },
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
enum InternalMessage {}
