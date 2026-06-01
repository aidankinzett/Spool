import { useEffect, useState } from "react";
import { getServerBase } from "../api/callables";

// Resolve the headless server base URL once. The whole full-screen UI talks
// to the server over loopback HTTP directly (not the Decky callable bridge):
// `http://127.0.0.1` is a secure origin, so `<img>` covers aren't blocked as
// mixed content from the https://steamloopback.host page.
export function useServerBase(): { base: string | null; error: string | null } {
  const [base, setBase] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const { baseUrl } = await getServerBase();
        if (cancelled) return;
        if (baseUrl) setBase(baseUrl);
        else setError("Spool isn’t running. Launch Spool, then try again.");
      } catch (e) {
        if (!cancelled)
          setError(`Couldn’t reach Spool: ${e instanceof Error ? e.message : String(e)}`);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);
  return { base, error };
}
