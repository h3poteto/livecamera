import { useRouter } from "next/router";
import { useEffect, useState } from "react";

export default function Room() {
  const [room, setRoom] = useState("");

  const router = useRouter();

  useEffect(() => {
    if (router.query.room) {
      setRoom(router.query.room as string);
    }
  }, [router.query.room]);

  return (
    <div>
      <h1>Room: {room}</h1>
    </div>
  );
}
