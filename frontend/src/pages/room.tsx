import { useRouter } from "next/router";
import { useEffect, useRef, useState } from "react";
import {
  PublishTransport,
  SubscribeTransport,
  simulcastEncodings,
} from "rheomesh";

const peerConnectionConfig: RTCConfiguration = {
  iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
};

export default function Room() {
  const router = useRouter();

  const [room, setRoom] = useState("");
  const [recevingVideo, setRecevingVideo] = useState<{
    [publisherId: string]: MediaStream;
  }>({});
  const [recevingAudio, setRecevingAudio] = useState<{
    [publisherId: string]: MediaStream;
  }>({});
  const [connected, setConnected] = useState(false);
  const [localVideo, setLocalVideo] = useState<MediaStream>();
  const [localAudio, setLocalAudio] = useState<MediaStream>();
  const [subscriberIds, setSubscriberIds] = useState<Array<string>>([]);
  const [sid, setSid] = useState<number>(2);
  const [tid, setTid] = useState<number>(2);

  const ws = useRef<WebSocket | null>(null);
  const sendingVideoRef = useRef<HTMLVideoElement>(null);
  const publishTransport = useRef<PublishTransport | null>(null);
  const subscribeTransport = useRef<SubscribeTransport | null>(null);

  useEffect(() => {
    if (router.query.room) {
      setRoom(router.query.room as string);
    }
  }, [router.query.room]);

  const connect = () => {
    const url = process.env.NEXT_PUBLIC_WS_URL as string;
    ws.current = new WebSocket(`${url}/socket?room=${room}`);
    ws.current.onopen = () => {
      console.debug("Connected websocket server");
      startPublishPeer();
      startSubscribePeer();
      setConnected(true);
    };
    ws.current.onclose = () => {
      console.debug("Disconnected from websocket server");
      setConnected(false);
    };
    ws.current.onerror = (e) => {
      console.error(e);
    };
    ws.current.onmessage = messageHandler;
    setInterval(() => {
      if (ws.current && ws.current.readyState === WebSocket.OPEN) {
        ws.current.send(JSON.stringify({ action: "Ping" }));
      }
    }, 5000);
  };

  const startPublishPeer = () => {
    if (!publishTransport.current) {
      publishTransport.current = new PublishTransport(peerConnectionConfig);
      ws.current!.send(JSON.stringify({ action: "PublisherInit" }));
      publishTransport.current.on("icecandidate", (candidate) => {
        ws.current!.send(
          JSON.stringify({
            action: "PublisherIce",
            candidate: candidate,
          }),
        );
      });
      publishTransport.current.on("negotiationneeded", (offer) => {
        ws.current!.send(
          JSON.stringify({
            action: "Offer",
            sdp: offer,
          }),
        );
      });
    }
  };

  const startSubscribePeer = () => {
    if (!subscribeTransport.current) {
      subscribeTransport.current = new SubscribeTransport(peerConnectionConfig);
      ws.current!.send(JSON.stringify({ action: "SubscriberInit" }));
      subscribeTransport.current.on("icecandidate", (candidate) => {
        ws.current!.send(
          JSON.stringify({
            action: "SubscriberIce",
            candidate: candidate,
          }),
        );
      });
    }
  };

  const messageHandler = (event: MessageEvent) => {
    console.debug("Received message: ", event.data);
    const message = JSON.parse(event.data);
    switch (message.action) {
      case "Offer":
        subscribeTransport.current!.setOffer(message.sdp).then((answer) => {
          ws.current!.send(JSON.stringify({ action: "Answer", sdp: answer }));
        });
        break;
      case "Answer":
        publishTransport.current!.setAnswer(message.sdp);
        break;
      case "SubscriberIce":
        subscribeTransport.current!.addIceCandidate(message.candidate);
        break;
      case "PublisherIce":
        publishTransport.current!.addIceCandidate(message.candidate);
        break;
      case "Published":
        message.publisherIds.forEach((publisherId: string) => {
          ws.current!.send(
            JSON.stringify({
              action: "Subscribe",
              publisherId: publisherId,
            }),
          );
          subscribeTransport.current!.subscribe(publisherId).then((track) => {
            const stream = new MediaStream([track]);
            if (track.kind === "audio") {
              setRecevingAudio((prev) => ({
                ...prev,
                [publisherId]: stream,
              }));
            } else {
              setRecevingVideo((prev) => ({
                ...prev,
                [publisherId]: stream,
              }));
            }
          });
        });

        break;
      case "Subscribed":
        setSubscriberIds((prev) => [...prev, message.subscriberId]);
        break;
      case "Pong":
        console.debug("pong");
        break;
      default:
        console.error("Unknown message type: ", message);
        break;
    }
  };

  const capture = async () => {
    const stream = await navigator.mediaDevices.getDisplayMedia({
      video: true,
      audio: false,
    });

    if (sendingVideoRef.current) {
      sendingVideoRef.current.srcObject = stream;
    }
    await publish(stream);
    setLocalVideo(stream);
  };

  const publish = async (stream: MediaStream) => {
    stream.getTracks().forEach(async (track) => {
      const offer = await publishTransport.current!.publish(
        track,
        simulcastEncodings(),
      );
      ws.current!.send(
        JSON.stringify({
          action: "Offer",
          sdp: offer,
        }),
      );
      ws.current!.send(
        JSON.stringify({ action: "Publish", trackId: track.id }),
      );
    });
  };

  const restart = async () => {
    await restartPublish();
    await restartSubscribe();
  };

  const restartPublish = async () => {
    if (!publishTransport.current) return;
    try {
      const offer = await publishTransport.current.restartIce();
      ws.current!.send(
        JSON.stringify({
          action: "Offer",
          sdp: offer,
        }),
      );
    } catch (err) {
      console.error(err);
    }
  };

  const restartSubscribe = async () => {
    if (!subscribeTransport.current) return;
    ws.current!.send(
      JSON.stringify({
        action: "RestartICE",
      }),
    );
  };

  const stop = async () => {
    localVideo?.getTracks().forEach((track) => {
      ws.current!.send(
        JSON.stringify({ action: "StopPublish", publisherId: track.id }),
      );
      track.stop();
    });
    setLocalVideo(undefined);
    localAudio?.getTracks().forEach((track) => {
      ws.current!.send(
        JSON.stringify({ action: "StopPublish", publisherId: track.id }),
      );
      track.stop();
    });
    setLocalAudio(undefined);

    subscriberIds.forEach((id) => {
      ws.current!.send(
        JSON.stringify({
          action: "StopSubscribe",
          subscriberId: id,
        }),
      );
    });
    setSubscriberIds([]);
    publishTransport.current?.close();
    publishTransport.current = null;
    subscribeTransport.current?.close();
    subscribeTransport.current = null;
    ws.current?.close();
    ws.current = null;
    setConnected(false);
  };

  const setPrefferedLayer = (sid: number, tid: number) => {
    subscriberIds.forEach((id) => {
      ws.current!.send(
        JSON.stringify({
          action: "SetPreferredLayer",
          subscriberId: id,
          sid: sid,
          tid: tid,
        }),
      );
    });
  };

  const updateSid = (sid: number) => {
    setSid(sid);
    setPrefferedLayer(sid, tid);
  };

  const updateTid = (tid: number) => {
    setTid(tid);
    setPrefferedLayer(sid, tid);
  };

  return (
    <div className="px-4">
      <h1>Room: {room}</h1>
      <div className="mt-2">
        <button
          id="connect"
          onClick={connect}
          disabled={connected}
          className="bg-blue-500 text-white px-4 py-1 rounded-md hover:bg-blue-600"
        >
          Connect
        </button>
      </div>
      <div className="mt-2">
        <button
          id="capture"
          onClick={capture}
          disabled={localVideo !== undefined || !connected}
          className="bg-blue-500 text-white px-4 py-1 rounded-md hover:bg-blue-600"
        >
          Capture
        </button>
      </div>
      <div className="mt-2">
        <select onChange={(e) => updateSid(parseInt(e.target.value))}>
          <option value="2">High</option>
          <option value="1">Middle</option>
          <option value="0">Low</option>
        </select>
        <select onChange={(e) => updateTid(parseInt(e.target.value))}>
          <option value="2">2</option>
          <option value="1">1</option>
          <option value="0">0</option>
        </select>
      </div>
      <div className="mt-2">
        <button
          id="restart"
          onClick={restart}
          className="bg-yellow-500 text-white px-4 py-1 rounded-md hover:bg-yellow-600"
        >
          RestartICE
        </button>
      </div>
      <div className="mt-2">
        <button
          id="stop"
          onClick={stop}
          disabled={!connected}
          className="bg-red-500 text-white px-4 py-1 rounded-md hover:bg-red-600"
        >
          Stop
        </button>
      </div>
      <h3>My Screen</h3>
      <video
        autoPlay
        muted
        id="sending-video"
        ref={sendingVideoRef}
        width={480}
      ></video>
      <h3>Receving</h3>
      {Object.keys(recevingVideo).map((key) => (
        <div key={key}>
          {recevingVideo[key] && (
            <video
              id={key}
              muted
              className="receiving-video"
              autoPlay
              ref={(video) => {
                if (video && recevingVideo[key]) {
                  video.srcObject = recevingVideo[key];
                } else {
                  console.warn("video element or track is null");
                }
              }}
              width={480}
            ></video>
          )}
        </div>
      ))}
      {Object.keys(recevingAudio).map((key) => (
        <div key={key}>
          {recevingAudio[key] && (
            <audio
              id={key}
              autoPlay
              controls
              ref={(audio) => {
                if (audio && recevingAudio[key]) {
                  audio.srcObject = recevingAudio[key];
                } else {
                  console.warn("audio element or track is null");
                }
              }}
            ></audio>
          )}
        </div>
      ))}
    </div>
  );
}
