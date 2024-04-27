import SignalHandler from "@/socket/handler";
import { useRouter } from "next/router";
import { useEffect, useRef, useState } from "react";

export default function Room() {
  const router = useRouter();

  const [room, setRoom] = useState("");
  const [stream, setStream] = useState<MediaStream | null>(null);
  const [signalHandler, setSignalHandler] = useState<SignalHandler>();
  const [recevingStream, setRecevingStream] = useState<{
    [producerId: string]: MediaStream;
  }>({});

  const sendingVideoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    if (router.query.room) {
      setRoom(router.query.room as string);
      const url = process.env.NEXT_PUBLIC_WS_URL as string;

      const handler = new SignalHandler(
        `${url}?room=${router.query.room}`,
        (producerId: string, track: MediaStreamTrack) => {
          const stream = new MediaStream([track]);
          setRecevingStream((prev) => ({
            ...prev,
            [producerId]: stream,
          }));
        },
        (producerId: string) => {
          setRecevingStream((prev) => {
            const stream = prev[producerId];
            if (stream) {
              stream.getTracks().forEach((track) => track.stop());
            }
            const newStream = { ...prev };
            delete newStream[producerId];
            return newStream;
          });
        },
      );
      setSignalHandler(handler);
      handler.connect();
    }
    return () => {
      signalHandler?.close();
      setSignalHandler(undefined);
    };
  }, [router.query.room]);

  const getScreenCapture = async () => {
    const s = await navigator.mediaDevices.getDisplayMedia({
      video: true,
      audio: true,
    });
    setStream(s);
    if (sendingVideoRef.current) {
      sendingVideoRef.current.srcObject = s;
    }

    signalHandler!.produce(s.getVideoTracks()[0]);
  };

  const stop = () => {
    signalHandler?.closeProducer();
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
      {Object.keys(recevingStream).map((key) => (
        <div key={key}>
          {recevingStream[key] && (
            <video
              id={key}
              muted
              autoPlay
              ref={(video) => {
                if (video && recevingStream[key]) {
                  video.srcObject = recevingStream[key];
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
