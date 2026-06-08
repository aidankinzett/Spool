import { useEffect, useState } from "react";

// Returns a timestamp that advances on a fixed interval so a component
// rendering relative-time labels (e.g. "5m ago" / "just now") re-renders and
// recomputes them, instead of freezing the label until some other state change
// forces a re-render. One tick a minute matches formatRelativeTime's
// granularity (its smallest live step is "just now" → "1h ago").
export function useNow(intervalMs = 60_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
