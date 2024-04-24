import WebSocketClient from "@/socket/client";
import {
  Consumer,
  ConsumerOptions,
  Device,
  DtlsParameters,
  MediaKind,
  Producer,
  RtpCapabilities,
  RtpParameters,
  Transport,
  TransportOptions,
} from "mediasoup-client/lib/types";
import { useRouter } from "next/router";
import { useEffect, useRef, useState } from "react";

export default function Room() {
  const router = useRouter();

  const [room, setRoom] = useState("");
  const [stream, setStream] = useState<MediaStream | null>(null);
  const [producer, setProducer] = useState<Producer | null>(null);
  const [consumers, setConsumers] = useState<{ [key: string]: Consumer }>({});

  const socket = useRef<WebSocketClient | null>(null);
  const device = useRef<Device | null>(null);
  const producerTransport = useRef<Transport | null>(null);
  const consumerTransport = useRef<Transport | null>(null);
  const sendingVideoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    return () => {
      producerTransport.current?.close();
      consumerTransport.current?.close();
      socket.current?.disconnect();
    };
  }, []);

  useEffect(() => {
    if (router.query.room) {
      setRoom(router.query.room as string);
      const url = process.env.NEXT_PUBLIC_WS_URL as string;
      const ws = new WebSocketClient(
        `${url}?room=${router.query.room}`,
        messageHandler,
      );
      socket.current = ws;
      socket.current.connect(() => {
        console.debug("Connected to server");
        device.current = new Device();
        socket.current?.send({ action: "Init" } as ClientInit);
      });
    }
  }, [router.query.room]);

  const messageHandler = async (e: MessageEvent) => {
    const message: ServerMessage = JSON.parse(e.data);
    console.debug("Message received", message);

    switch (message.action) {
      case "Init": {
        if (!device.current) {
          return;
        }
        if (!device.current.loaded) {
          await device.current.load({
            routerRtpCapabilities: message.routerRtpCapabilities,
          });
        }
        socket.current?.send({
          action: "SendRtpCapabilities",
          rtpCapabilities: device.current.rtpCapabilities,
        });

        producerTransport.current = device.current.createSendTransport(
          message.producerTransportOptions,
        );
        consumerTransport.current = device.current.createRecvTransport(
          message.consumerTransportOptions,
        );

        producerTransport.current
          .on("connect", ({ dtlsParameters }, success, failed) => {
            socket.current
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
                  failed(new Error("Invalid response"));
                }
              })
              .catch((err) => failed(err));
          })
          .on("produce", ({ kind, rtpParameters }, success, failed) => {
            socket.current
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
                  failed(new Error("Invalid response"));
                }
              })
              .catch((err) => failed(err));
          });

        consumerTransport.current.on(
          "connect",
          ({ dtlsParameters }, success, failed) => {
            socket.current
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
                  failed(new Error("Invalid response"));
                }
              })
              .catch((err) => failed(err));
          },
        );

        break;
      }
      case "NewProducers": {
        const ids = message.ids;
        ids.forEach(async (id) => {
          const e: MessageEvent = await socket.current?.invoke(
            {
              action: "Consume",
              producerId: id,
            } as Consume,
            "Consumed",
          );
          const m = JSON.parse(e.data) as Consumed;
          if (m.action !== "Consumed") {
            return;
          }
          const consumer = await consumerTransport.current?.consume({
            id: m.id,
            producerId: m.producerId,
            rtpParameters: m.rtpParameters,
            kind: m.kind,
          } as ConsumerOptions);
          if (!consumer) {
            console.error("Failed to consume");
            return;
          }
          console.debug("Consumed", m.id);
          consumer.on("transportclose", () => {
            console.debug("Consumer transport closed");
          });
          consumer.on("trackended", () => {
            console.debug("Consumer track ended");
          });

          socket.current?.send({
            action: "Resume",
            consumerId: consumer.id,
          } as Resume);

          setConsumers((prev) => {
            return Object.assign({}, prev, {
              [m.producerId]: consumer,
            });
          });
        });
        break;
      }
      case "ProducerClosed": {
        const id = message.id;
        setConsumers((prev) => {
          const consumer = prev[id];
          if (consumer) {
            consumer.close();
          }

          return Object.assign({}, prev, {
            [id]: undefined,
          });
        });
        break;
      }
    }
  };

  const getScreenCapture = async () => {
    const s = await navigator.mediaDevices.getDisplayMedia({
      video: true,
      audio: true,
    });
    setStream(s);
    if (sendingVideoRef.current) {
      sendingVideoRef.current.srcObject = s;
    }

    if (!producerTransport) {
      return;
    }
    const p = await producerTransport.current?.produce({
      track: s.getVideoTracks()[0],
    });
    if (p) {
      setProducer(p);
    }
  };

  const stop = () => {
    if (producer) {
      producer.close();
      setProducer(null);
      socket.current?.send({ action: "ProducerClosed", id: producer.id });
    }
    stream?.getTracks().forEach((track) => track.stop());
    setStream(null);
    if (sendingVideoRef.current) {
      sendingVideoRef.current.srcObject = null;
    }
  };

  return (
    <div className="px-4">
      <h1>Room: {room}</h1>
      <div className="mt-2">
        <button
          onClick={getScreenCapture}
          className="bg-blue-500 text-white px-4 py-1 rounded-md hover:bg-blue-600"
        >
          Get Screen Capture
        </button>
      </div>
      <div className="mt-2">
        <button
          onClick={stop}
          className="bg-red-500 text-white px-4 py-1 rounded-md hover:bg-red-600"
        >
          Stop
        </button>
      </div>
      <h3>My Screen</h3>
      <video autoPlay muted ref={sendingVideoRef} width={480}></video>
      <h3>Receiving</h3>
      {Object.keys(consumers).map((key) => (
        <div key={key}>
          {consumers[key] && (
            <video
              id={key}
              autoPlay
              ref={(video) => {
                if (video && consumers[key]) {
                  video.srcObject = new MediaStream([consumers[key].track]);
                } else {
                  console.warn("video element or track is null");
                }
              }}
              width={480}
            ></video>
          )}
        </div>
      ))}
    </div>
  );
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
