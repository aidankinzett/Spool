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
  TextField,
  ToggleField,
  staticClasses,
} from "@decky/ui";
import { useEffect, useState } from "react";
import { FaFloppyDisk } from "react-icons/fa6";

// `SteamClient` (incl. GameSessions.RegisterForAppLifetimeNotifications and the
// LifetimeNotification payload) is provided as an ambient global by @decky/ui.
// The stop callback's `n` has `unAppID` (CRC app id — for Spool's non-Steam
// shortcuts it equals the `steam_appid` in active-session.json) and `bRunning`
// (false on a stop event).

const onAppStop = callable<[appid: number], { acted: boolean; game?: string }>(
  "on_app_stop",
);
const backupNow = callable<
  [],
  { acted: boolean; ok: boolean; game?: string; reason?: string }
>("backup_now");
const getStatus = callable<
  [],
  { hasSession: boolean; game?: string; backedUp?: boolean; startedAt?: string }
>("get_status");

interface Settings {
  spool_command: string;
  session_file: string;
  notify: boolean;
}
const getSettings = callable<[], Settings>("get_settings");
const setSettings = callable<
  [spool_command: string, session_file: string, notify: boolean],
  Settings
>("set_settings");

function Content() {
  const [status, setStatus] = useState<Awaited<ReturnType<typeof getStatus>> | null>(
    null,
  );
  const [settings, setLocalSettings] = useState<Settings | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = async () => setStatus(await getStatus());
  useEffect(() => {
    void refresh();
    void getSettings().then(setLocalSettings);
  }, []);

  // Persist a single changed field, optimistically updating local state.
  const save = async (patch: Partial<Settings>) => {
    const next = { ...(settings ?? { spool_command: "", session_file: "", notify: true }), ...patch };
    setLocalSettings(next);
    setLocalSettings(await setSettings(next.spool_command, next.session_file, next.notify));
  };

  return (
    <>
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
              if (status?.game) {
                toaster.toast({
                  title: "Spool Backup",
                  body: `Backing up ${status.game}…`,
                });
              }
              const r = await backupNow();
              toaster.toast({
                title: "Spool Backup",
                body: !r.acted
                  ? "Nothing to back up"
                  : r.ok
                    ? `Backed up ${r.game} ✓`
                    : `Backup failed: ${r.reason ?? "unknown error"}`,
              });
              setBusy(false);
              void refresh();
            }}
          >
            Back up now
          </ButtonItem>
        </PanelSectionRow>
      </PanelSection>

      <PanelSection title="Settings">
        <PanelSectionRow>
          <ToggleField
            label="Notify on backup"
            description="Show a toast when a backup starts and finishes."
            checked={settings?.notify ?? true}
            onChange={(value) => void save({ notify: value })}
          />
        </PanelSectionRow>
        <PanelSectionRow>
          <TextField
            label="Spool command"
            description="Override the auto-detected spool / spool-launcher.sh path."
            value={settings?.spool_command ?? ""}
            onChange={(e) => void save({ spool_command: e.target.value })}
          />
        </PanelSectionRow>
        <PanelSectionRow>
          <TextField
            label="Session file"
            description="Override the auto-detected active-session.json path."
            value={settings?.session_file ?? ""}
            onChange={(e) => void save({ session_file: e.target.value })}
          />
        </PanelSectionRow>
      </PanelSection>
    </>
  );
}

export default definePlugin(() => {
  // Register the game-stop listener ONCE at plugin load (not inside the panel,
  // which unmounts when the QAM closes). On a stop, let the backend decide
  // whether a forced-close fallback backup is needed.
  const sub = SteamClient.GameSessions.RegisterForAppLifetimeNotifications(
    (n) => {
      if (!n.bRunning) {
        void onAppStop(n.unAppID);
      }
    },
  );

  const onBackupStarted = (game: string) => {
    toaster.toast({ title: "Spool Backup", body: `Backing up ${game}…` });
  };
  const onBackupFinished = (game: string, ok: boolean, reason: string) => {
    toaster.toast({
      title: "Spool Backup",
      body: ok ? `Backed up ${game} ✓` : `Backup failed: ${reason || "unknown error"}`,
    });
  };
  addEventListener("spool_backup_started", onBackupStarted);
  addEventListener("spool_backup_finished", onBackupFinished);

  return {
    name: "Spool Backup",
    titleView: <div className={staticClasses.Title}>Spool Backup</div>,
    content: <Content />,
    icon: <FaFloppyDisk />,
    onDismount() {
      sub.unregister();
      removeEventListener("spool_backup_started", onBackupStarted);
      removeEventListener("spool_backup_finished", onBackupFinished);
    },
  };
});
