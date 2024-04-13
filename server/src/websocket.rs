use std::sync::Arc;

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web_actors::ws;
use mediasoup::{
    consumer::{Consumer, ConsumerId, ConsumerOptions},
    data_structures::{DtlsParameters, IceCandidate, IceParameters},
    producer::{Producer, ProducerId, ProducerOptions},
    rtp_parameters::{MediaKind, RtpCapabilities, RtpCapabilitiesFinalized, RtpParameters},
    transport::{Transport, TransportId},
    webrtc_transport::{WebRtcTransport, WebRtcTransportOptions, WebRtcTransportRemoteParameters},
};
use serde::{Deserialize, Serialize};

use crate::room;

#[derive(Debug)]
struct Transports {
    producer: WebRtcTransport,
    consumer: WebRtcTransport,
}

pub struct WebSocket {
    pub room: Arc<room::Room>,
    transports: Transports,
    producers: Vec<Arc<Producer>>,
    consumers: Vec<Arc<Consumer>>,
    client_rtp_capabilities: Option<RtpCapabilities>,
}

impl WebSocket {
    pub async fn new(room: Arc<room::Room>) -> Self {
        let mut transport_options =
            WebRtcTransportOptions::new_with_server(room.worker.server.clone());
        transport_options.enable_tcp = true;
        transport_options.enable_udp = true;

        let producer_transport = room
            .router
            .create_webrtc_transport(transport_options.clone())
            .await
            .expect("Failed to create producer transport");
        let consumer_transport = room
            .router
            .create_webrtc_transport(transport_options)
            .await
            .expect("Failed to create consumer transport");
        WebSocket {
            room,
            transports: Transports {
                producer: producer_transport,
                consumer: consumer_transport,
            },
            producers: [].to_vec(),
            consumers: [].to_vec(),
            client_rtp_capabilities: None,
        }
    }
}

impl Actor for WebSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("WebSocket started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("WebSocket stopped");
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

    fn handle(&mut self, msg: ReceivedMessage, ctx: &mut Self::Context) {
        let address = ctx.address();

        tracing::debug!("Received message: {:?}", msg);

        match msg {
            ReceivedMessage::Init => {
                let router_rtp_capabilities = self.room.router.rtp_capabilities().clone();
                let message = SendingMessage::Init {
                    consumer_transport_options: TransportOptions {
                        id: self.transports.consumer.id(),
                        dtls_parameters: self.transports.consumer.dtls_parameters(),
                        ice_candidates: self.transports.consumer.ice_candidates().clone(),
                        ice_parameters: self.transports.consumer.ice_parameters().clone(),
                    },
                    producer_transport_options: TransportOptions {
                        id: self.transports.producer.id(),
                        dtls_parameters: self.transports.producer.dtls_parameters(),
                        ice_candidates: self.transports.producer.ice_candidates().clone(),
                        ice_parameters: self.transports.producer.ice_parameters().clone(),
                    },
                    router_rtp_capabilities,
                };
                address.do_send(message);
            }
            ReceivedMessage::SendRtpCapabilities { rtp_capabilities } => {
                self.client_rtp_capabilities.replace(rtp_capabilities);
            }
            ReceivedMessage::ConnectProducerTransport { dtls_parameters } => {
                let transport = self.transports.producer.clone();
                actix::spawn(async move {
                    match transport
                        .connect(WebRtcTransportRemoteParameters { dtls_parameters })
                        .await
                    {
                        Ok(_) => {
                            tracing::info!("Producer transport connected");
                            address.do_send(SendingMessage::ConnectedProducerTransport);
                        }
                        Err(err) => {
                            tracing::error!("Failed to connect producer transport: {:?}", err);
                            address.do_send(InternalMessage::Stop);
                        }
                    }
                });
            }
            ReceivedMessage::Produce {
                kind,
                rtp_parameters,
            } => {
                let transport = self.transports.producer.clone();

                actix::spawn(async move {
                    match transport
                        .produce(ProducerOptions::new(kind, rtp_parameters))
                        .await
                    {
                        Ok(producer) => {
                            let id = producer.id();
                            address.do_send(SendingMessage::Produced { id });
                            address.do_send(InternalMessage::SaveProducer(producer));
                            tracing::info!("{:?} producer created: {:?}", kind, id);
                        }
                        Err(err) => {
                            tracing::error!("Failed to produce {:?}: {:?}", kind, err);
                            address.do_send(InternalMessage::Stop)
                        }
                    }
                });
            }
            ReceivedMessage::ConnectConsumerTransport { dtls_parameters } => {
                let transport = self.transports.consumer.clone();
                actix::spawn(async move {
                    match transport
                        .connect(WebRtcTransportRemoteParameters { dtls_parameters })
                        .await
                    {
                        Ok(_) => {
                            tracing::info!("Consumer transport connected");
                            address.do_send(SendingMessage::ConnectedConsumerTransport);
                        }
                        Err(err) => {
                            tracing::error!("Failed to connect consumer transport: {:?}", err);
                            address.do_send(InternalMessage::Stop);
                        }
                    }
                });
            }
            ReceivedMessage::Consume { producer_id } => {
                let transport = self.transports.consumer.clone();
                let Some(rtp_capabilities) = self.client_rtp_capabilities.clone() else {
                    tracing::error!("RTP capabilities not set");
                    address.do_send(InternalMessage::Stop);
                    return;
                };

                actix::spawn(async move {
                    let mut options = ConsumerOptions::new(producer_id, rtp_capabilities);
                    options.paused = true;
                    match transport.consume(options).await {
                        Ok(consumer) => {
                            let id = consumer.id();
                            let producer_id = consumer.producer_id();
                            let kind = consumer.kind();
                            let rtp_parameters = consumer.rtp_parameters().clone();
                            address.do_send(SendingMessage::Consumed {
                                id,
                                producer_id,
                                kind,
                                rtp_parameters,
                            });
                            address.do_send(InternalMessage::SaveConsumer(consumer));
                            tracing::info!("{:?} consumer created: {:?}", kind, id);
                        }
                        Err(err) => {
                            tracing::error!("Failed to consume {:?}: {:?}", producer_id, err);
                            address.do_send(InternalMessage::Stop);
                        }
                    }
                });
            }
            ReceivedMessage::Resume { consumer_id } => {
                let Some(consumer) = self.consumers.iter().find(|c| c.id() == consumer_id) else {
                    tracing::error!("Consumer not found: {:?}", consumer_id);
                    address.do_send(InternalMessage::Stop);
                    return;
                };
                let consumer = consumer.clone();

                actix::spawn(async move {
                    match consumer.resume().await {
                        Ok(_) => {
                            tracing::info!(
                                "{:?} consumer resumed: {:?}",
                                consumer.kind(),
                                consumer_id
                            );
                        }
                        Err(err) => {
                            tracing::error!(
                                "Failed to resume {:?} consumer {}: {:?}",
                                consumer.kind(),
                                consumer_id,
                                err
                            );
                        }
                    }
                });
            }
        }
    }
}

impl Handler<SendingMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, message: SendingMessage, ctx: &mut Self::Context) {
        ctx.text(serde_json::to_string(&message).unwrap());
    }
}

impl Handler<InternalMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, message: InternalMessage, ctx: &mut Self::Context) {
        match message {
            InternalMessage::SaveProducer(producer) => {
                let producer = Arc::new(producer);
                self.producers.push(producer);
            }
            InternalMessage::SaveConsumer(consumer) => {
                let consumer = Arc::new(consumer);
                self.consumers.push(consumer);
            }
            InternalMessage::Stop => {
                ctx.stop();
            }
        }
    }
}

// messages
#[derive(Deserialize, Message, Debug)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum ReceivedMessage {
    #[serde(rename_all = "camelCase")]
    Init,
    #[serde(rename_all = "camelCase")]
    SendRtpCapabilities { rtp_capabilities: RtpCapabilities },
    #[serde(rename_all = "camelCase")]
    ConnectProducerTransport { dtls_parameters: DtlsParameters },
    #[serde(rename_all = "camelCase")]
    Produce {
        kind: MediaKind,
        rtp_parameters: RtpParameters,
    },
    #[serde(rename_all = "camelCase")]
    ConnectConsumerTransport { dtls_parameters: DtlsParameters },
    #[serde(rename_all = "camelCase")]
    Consume { producer_id: ProducerId },
    #[serde(rename_all = "camelCase")]
    Resume { consumer_id: ConsumerId },
}

#[derive(Serialize, Message, Debug)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum SendingMessage {
    #[serde(rename_all = "camelCase")]
    Init {
        consumer_transport_options: TransportOptions,
        producer_transport_options: TransportOptions,
        router_rtp_capabilities: RtpCapabilitiesFinalized,
    },
    #[serde(rename_all = "camelCase")]
    ConnectedProducerTransport,
    #[serde(rename_all = "camelCase")]
    Produced { id: ProducerId },
    #[serde(rename_all = "camelCase")]
    ConnectedConsumerTransport,
    #[serde(rename_all = "camelCase")]
    Consumed {
        id: ConsumerId,
        producer_id: ProducerId,
        kind: MediaKind,
        rtp_parameters: RtpParameters,
    },
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TransportOptions {
    id: TransportId,
    dtls_parameters: DtlsParameters,
    ice_candidates: Vec<IceCandidate>,
    ice_parameters: IceParameters,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
enum InternalMessage {
    SaveProducer(Producer),
    SaveConsumer(Consumer),
    Stop,
}
