import WebSocketClient from "@/socket/client";
import {
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
  const [room, setRoom] = useState("");
  const [stream, setStream] = useState<MediaStream | null>(null);
  const [device, setDevice] = useState<Device | null>(null);
  const [producerTransport, setProducerTransport] = useState<Transport | null>(
    null,
  );
  const [consumerTransport, setConsumerTransport] = useState<Transport | null>(
    null,
  );
  const [producer, setProducer] = useState<Producer | null>(null);

  const socket = useRef<WebSocketClient | null>(null);

  const router = useRouter();
  const videoRef = useRef<any>();

  useEffect(() => {
    const d = new Device();
    setDevice(d);
    return () => {
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
        socket.current?.send({ action: "Init" } as ClientInit);
      });
    }
  }, [router.query.room]);

  const messageHandler = async (e: MessageEvent) => {
    const message: ServerMessage = JSON.parse(e.data);
    console.debug("Message received", message);

    switch (message.action) {
      case "Init": {
        if (!device) {
          return;
        }
        await device.load({
          routerRtpCapabilities: message.routerRtpCapabilities,
        });
        socket.current?.send({
          action: "SendRtpCapabilities",
          rtpCapabilities: device.rtpCapabilities,
        });

        const pt = device.createSendTransport(message.producerTransportOptions);
        setProducerTransport(pt);
        const ct = device.createRecvTransport(message.consumerTransportOptions);
        setConsumerTransport(ct);

        pt.on("connect", ({ dtlsParameters }, success, failed) => {
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
        }).on("produce", ({ kind, rtpParameters }, success, failed) => {
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

        ct.on("connect", ({ dtlsParameters }, success, failed) => {
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
    videoRef.current.srcObject = s;

    if (!producerTransport) {
      return;
    }
    const p = await producerTransport.produce({
      track: s.getVideoTracks()[0],
    });
    setProducer(p);
  };

  const stop = () => {
    if (producer) {
      producer.close();
      setProducer(null);
    }
    stream?.getTracks().forEach((track) => track.stop());
    setStream(null);
    videoRef.current.srcObject = null;
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
      <video autoPlay muted ref={videoRef} width={480}></video>
      <h3>Receiving</h3>
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
  | ConnectConsumerTransport
  | Consume
  | Resume;
