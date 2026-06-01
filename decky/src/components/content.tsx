import { toaster } from "@decky/api";
import {
  ButtonItem,
  Navigation,
  PanelSection,
  PanelSectionRow,
  TextField,
  ToggleField,
} from "@decky/ui";
import { useEffect, useState } from "react";
import { SPOOL_ROUTE } from "../constants";
import type { Settings } from "../types";
import { backupNow, getSettings, getStatus, setSettings } from "../api/callables";

export function Content() {
  const [status, setStatus] = useState<Awaited<ReturnType<typeof getStatus>> | null>(
    null,
  );
  const [settings, setLocalSettings] = useState<Settings | null>(null);
  const [spoolCommand, setSpoolCommand] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = async () => setStatus(await getStatus());
  useEffect(() => {
    void refresh();
    void getSettings().then((s) => {
      setLocalSettings(s);
      setSpoolCommand(s.spool_command);
    });
  }, []);

  const save = async (patch: Partial<Settings>) => {
    const next = { ...(settings ?? { spool_command: "", notify: true }), ...patch };
    setLocalSettings(next);
    setLocalSettings(await setSettings(next.spool_command, next.notify));
  };

  return (
    <>
      <PanelSection title="Library">
        <PanelSectionRow>
          <ButtonItem
            layout="below"
            onClick={() => {
              Navigation.Navigate(SPOOL_ROUTE);
              Navigation.CloseSideMenus();
            }}
          >
            Browse Library
          </ButtonItem>
        </PanelSectionRow>
      </PanelSection>

      <PanelSection title="Spool">
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
                  title: "Spool",
                  body: `Backing up ${status.game}…`,
                });
              }
              const r = await backupNow();
              toaster.toast({
                title: "Spool",
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
            description="Show a toast when a backup finishes."
            checked={settings?.notify ?? true}
            onChange={(value) => void save({ notify: value })}
          />
        </PanelSectionRow>
        <PanelSectionRow>
          <TextField
            label="Spool command"
            description="Override the auto-detected spool / spool-launcher.sh path."
            value={spoolCommand}
            onChange={(e) => setSpoolCommand(e.target.value)}
            onBlur={() => void save({ spool_command: spoolCommand })}
          />
        </PanelSectionRow>
      </PanelSection>
    </>
  );
}
