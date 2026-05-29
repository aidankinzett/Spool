import {
  callable,
  definePlugin,
  toaster,
  addEventListener,
  removeEventListener,
} from "@decky/api";
import {
  ButtonItem,
  PanelSection,
  PanelSectionRow,
  staticClasses,
} from "@decky/ui";
import { useEffect, useState } from "react";
import { FaFloppyDisk } from "react-icons/fa6";

// Steam's game lifecycle notification payload. `unAppID` is the (CRC-based)
// app id — for Spool's non-Steam shortcuts it equals the `steam_appid` Spool
// writes into active-session.json. `bRunning` is false on a stop event.
interface LifetimeNotification {
  unAppID: number;
  nInstanceID: number;
  bRunning: boolean;
}

// Minimal typing for the SteamClient global we rely on.
declare global {
  interface Window {
    SteamClient?: {
      GameSessions: {
        RegisterForAppLifetimeNotifications: (
          cb: (n: LifetimeNotification) => void,
        ) => { unregister: () => void };
      };
    };
  }
  const SteamClient: NonNullable<Window["SteamClient"]>;
}

const onAppStop = callable<[appid: number], { acted: boolean; game?: string }>(
  "on_app_stop",
);
const backupNow = callable<[], { acted: boolean; game?: string; reason?: string }>(
  "backup_now",
);
const getStatus = callable<
  [],
  { hasSession: boolean; game?: string; backedUp?: boolean; startedAt?: string }
>("get_status");

function Content() {
  const [status, setStatus] = useState<Awaited<ReturnType<typeof getStatus>> | null>(
    null,
  );
  const [busy, setBusy] = useState(false);

  const refresh = async () => setStatus(await getStatus());
  useEffect(() => {
    void refresh();
  }, []);

  return (
    <PanelSection title="Spool Backup">
      <PanelSectionRow>
        {status?.hasSession ? (
          <div style={{ fontSize: "0.8rem", opacity: 0.85 }}>
            Last session: <strong>{status.game}</strong>
            <br />
            {status.backedUp ? "Backed up ✓" : "Not yet backed up"}
          </div>
        ) : (
          <div style={{ fontSize: "0.8rem", opacity: 0.7 }}>
            No active Spool session recorded.
          </div>
        )}
      </PanelSectionRow>
      <PanelSectionRow>
        <ButtonItem
          layout="below"
          disabled={busy || !status?.hasSession}
          onClick={async () => {
            setBusy(true);
            const r = await backupNow();
            toaster.toast({
              title: "Spool Backup",
              body: r.acted ? `Backing up ${r.game}…` : "Nothing to back up",
            });
            setBusy(false);
            void refresh();
          }}
        >
          Back up now
        </ButtonItem>
      </PanelSectionRow>
    </PanelSection>
  );
}

export default definePlugin(() => {
  // Register the game-stop listener ONCE at plugin load (not inside the panel,
  // which unmounts when the QAM closes). On a stop, let the backend decide
  // whether a forced-close fallback backup is needed.
  const sub = SteamClient.GameSessions.RegisterForAppLifetimeNotifications(
    (n: LifetimeNotification) => {
      if (!n.bRunning) {
        void onAppStop(n.unAppID);
      }
    },
  );

  const onBackupStarted = (game: string) => {
    toaster.toast({ title: "Spool Backup", body: `Backing up ${game}…` });
  };
  addEventListener("spool_backup_started", onBackupStarted);

  return {
    name: "Spool Backup",
    titleView: <div className={staticClasses.Title}>Spool Backup</div>,
    content: <Content />,
    icon: <FaFloppyDisk />,
    onDismount() {
      sub.unregister();
      removeEventListener("spool_backup_started", onBackupStarted);
    },
  };
});
