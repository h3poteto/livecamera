import {
  DtlsParameters,
  Transport,
  TransportOptions,
} from "mediasoup-client/lib/Transport";
import WebSocketClient from "./client";
import {
  MediaKind,
  RtpCapabilities,
  RtpParameters,
} from "mediasoup-client/lib/RtpParameters";
import { Device } from "mediasoup-client";
import { Consumer, ConsumerOptions } from "mediasoup-client/lib/Consumer";
import { Producer } from "mediasoup-client/lib/types";

const iceServers: Array<RTCIceServer> = [];

export default class SgnalHandler {
  private socket: WebSocketClient | null;
  private url: string;
  private producerTransport: Transport | null;
  private consumerTransport: Transport | null;
  private device: Device;
  private producer: Producer | null;
  private consumers: { [producerId: string]: Consumer };
  private consumed: (producerId: string, track: MediaStreamTrack) => void;

  constructor(
    url: string,
    consumed: (producerId: string, track: MediaStreamTrack) => void,
  ) {
    this.socket = null;
    this.url = url;
    this.producerTransport = null;
    this.consumerTransport = null;
    this.device = new Device();
    this.producer = null;
    this.consumers = {};
    this.consumed = consumed;
  }

  connect() {
    this.socket = new WebSocketClient(this.url, (e) => this.handler(e));
    this.socket.connect(() => {
      this.socket?.send({
        action: "Init",
      } as ClientInit);
    });
  }

  close() {
    if (this.producer) {
      this.producer.close();
      this.socket?.send({
        action: "ProducerClosed",
        id: this.producer.id,
      } as ProducerClosed);
    }
    if (this.producerTransport) {
      this.producerTransport.close();
    }
    if (this.consumerTransport) {
      this.consumerTransport.close();
    }
    if (this.socket) {
      this.socket.disconnect();
    }
  }

  getConsumers() {
    return this.consumers;
  }

  async produce(track: MediaStreamTrack) {
    if (this.producer) {
      this.producer.close();
      this.socket?.send({
        action: "ProducerClosed",
        id: this.producer.id,
      } as ProducerClosed);
    }
    if (!this.producerTransport) {
      console.error("Producer transport not ready");
      return;
    }

    const p = await this.producerTransport.produce({
      track,
    });
    console.debug("Produced", p.id);
    this.producer = p;
  }

  closeProducer() {
    if (this.producer) {
      this.producer.close();
      this.socket?.send({
        action: "ProducerClosed",
        id: this.producer.id,
      } as ProducerClosed);
    }
  }

  async handler(e: MessageEvent) {
    const message: ServerMessage = JSON.parse(e.data);
    console.debug("Message received", message);

    switch (message.action) {
      case "Init":
        await this.device.load({
          routerRtpCapabilities: message.routerRtpCapabilities,
        });

        this.socket?.send({
          action: "SendRtpCapabilities",
          rtpCapabilities: this.device.rtpCapabilities,
        } as SendRtpCapabilities);

        this.producerTransport = this.device.createSendTransport({
          ...message.producerTransportOptions,
          iceServers: iceServers,
        });
        this.consumerTransport = this.device.createRecvTransport({
          ...message.consumerTransportOptions,
          iceServers: iceServers,
        });

        this.producerTransport
          .on("connect", async ({ dtlsParameters }, success, failed) => {
            this.socket
              ?.invoke(
                {
                  action: "ConnectProducerTransport",
                  dtlsParameters,
                } as ConnectProducerTransport,
                "ConnectedProducerTransport",
              )
              .then((e: MessageEvent) => {
                const m = JSON.parse(e.data) as ConnectedProducerTransport;
                if (m.action === "ConnectedProducerTransport") {
                  console.debug("Producer transport connected");
                  success();
                } else {
                  console.error("Invalid response", m);
                  failed(new Error("Invalid response"));
                }
              })
              .catch((e) => {
                console.error("Failed to connect producer transport", e);
                failed(e);
              });
          })
          .on("produce", ({ kind, rtpParameters }, success, failed) => {
            this.socket
              ?.invoke(
                {
                  action: "Produce",
                  kind,
                  rtpParameters,
                } as Produce,
                "Produced",
              )
              .then((e: MessageEvent) => {
                const m = JSON.parse(e.data) as Produced;
                if (m.action === "Produced") {
                  console.debug("Produced", m.id);
                  success({ id: m.id });
                } else {
                  console.error("Invalid response", m);
                  failed(new Error("Invalid response"));
                }
              })
              .catch((e) => {
                console.error("Failed to produce", e);
                failed(e);
              });
          });

        this.consumerTransport.on(
          "connect",
          ({ dtlsParameters }, success, failed) => {
            this.socket
              ?.invoke(
                {
                  action: "ConnectConsumerTransport",
                  dtlsParameters,
                } as ConnectConsumerTransport,
                "ConnectedConsumerTransport",
              )
              .then((e: MessageEvent) => {
                const m = JSON.parse(e.data) as ConnectedConsumerTransport;
                if (m.action === "ConnectedConsumerTransport") {
                  console.debug("Consumer transport connected");
                  success();
                } else {
                  console.error("Invalid response", m);
                  failed(new Error("Invalid response"));
                }
              })
              .catch((e) => {
                console.error("Failed to connect consumer transport", e);
                failed(e);
              });
          },
        );

        break;
      case "NewProducers": {
        const ids = message.ids;
        ids.forEach(async (id) => {
          const e: MessageEvent = await this.socket?.invoke(
            {
              action: "Consume",
              producerId: id,
            } as Consume,
            "Consumed",
          );
          const m = JSON.parse(e.data) as Consumed;
          if (m.action !== "Consumed") {
            console.error("Invalid response", m);
            return;
          }
          const consumer = await this.consumerTransport?.consume({
            id: m.id,
            producerId: m.producerId,
            rtpParameters: m.rtpParameters,
            kind: m.kind,
          } as ConsumerOptions);
          if (!consumer) {
            console.error("Failed to consume", m);
            return;
          }

          console.debug("Consumed", consumer.id);

          this.socket?.send({
            action: "Resume",
            consumerId: consumer.id,
          } as Resume);

          this.consumers[id] = consumer;

          this.consumed(m.producerId, consumer.track);
        });
        break;
      }
      case "ProducerClosed":
        const id = message.id;
        const consumer = this.consumers[id];
        if (consumer) {
          consumer.close();
          delete this.consumers[id];
        }
        break;
    }
  }
}

type ServerInit = {
  action: "Init";
  consumerTransportOptions: TransportOptions;
  producerTransportOptions: TransportOptions;
  routerRtpCapabilities: RtpCapabilities;
};

type ConnectedProducerTransport = {
  action: "ConnectedProducerTransport";
};

type Produced = {
  action: "Produced";
  id: string;
};

type NewProducers = {
  action: "NewProducers";
  ids: Array<string>;
};

type ProducerClosed = {
  action: "ProducerClosed";
  id: string;
};

type ConnectedConsumerTransport = {
  action: "ConnectedConsumerTransport";
};

type Consumed = {
  action: "Consumed";
  id: string;
  producerId: string;
  kind: MediaKind;
  rtpParameters: RtpParameters;
};

type ServerMessage =
  | ServerInit
  | ConnectedProducerTransport
  | Produced
  | NewProducers
  | ProducerClosed
  | ConnectedConsumerTransport
  | Consumed;

type ClientInit = {
  action: "Init";
};

type SendRtpCapabilities = {
  action: "SendRtpCapabilities";
  rtpCapabilities: RtpCapabilities;
};

type ConnectProducerTransport = {
  action: "ConnectProducerTransport";
  dtlsParameters: DtlsParameters;
};

type Produce = {
  action: "Produce";
  kind: MediaKind;
  rtpParameters: RtpParameters;
};

type ConnectConsumerTransport = {
  action: "ConnectConsumerTransport";
  dtlsParameters: DtlsParameters;
};

type Consume = {
  action: "Consume";
  producerId: string;
};

type Resume = {
  action: "Resume";
  consumerId: string;
};

type ClientMessage =
  | ClientInit
  | SendRtpCapabilities
  | ConnectProducerTransport
  | Produce
  | ProducerClosed
  | ConnectConsumerTransport
  | Consume
  | Resume;
