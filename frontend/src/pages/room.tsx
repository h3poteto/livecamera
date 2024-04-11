import { useRouter } from "next/router";
import { useEffect, useRef, useState } from "react";

export default function Room() {
  const [room, setRoom] = useState("");
  const [stream, setStream] = useState<MediaStream | null>(null);

  const socket = useRef<WebSocket | null>(null);

  const router = useRouter();
  const videoRef = useRef<any>();

  useEffect(() => {
    return () => {
      socket.current?.close();
    };
  }, []);

  useEffect(() => {
    if (router.query.room) {
      setRoom(router.query.room as string);
      const url = process.env.NEXT_PUBLIC_WS_URL as string;
      const ws = new WebSocket(`${url}?room=${router.query.room}`);
      socket.current = ws;
    }
  }, [router.query.room]);

  const getScreenCapture = async () => {
    const s = await navigator.mediaDevices.getDisplayMedia({
      video: true,
      audio: false,
    });
    setStream(s);
    videoRef.current.srcObject = s;
  };

  const stop = () => {
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
