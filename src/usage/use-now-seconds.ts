import { useEffect, useState } from "react";

const TICK_MS = 30_000;

/** The current time in unix seconds, updated every half minute so reset countdowns stay true. */
export function useNowSeconds(): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const timer = window.setInterval(() => {
      setNow(Math.floor(Date.now() / 1000));
    }, TICK_MS);
    return () => window.clearInterval(timer);
  }, []);
  return now;
}
