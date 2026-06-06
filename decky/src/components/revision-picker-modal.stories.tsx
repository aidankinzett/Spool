import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import { RevisionPickerModal } from "./revision-picker-modal";
import type { LibraryGame, SaveRevision } from "../types";
import { setCallable, clearCallables } from "../../.storybook/mocks/registry";

// Revisions relative to the real clock so the "Xh/Xd ago" labels read naturally
// whenever the story is opened. The tip ("." — is_current) can't be rolled back
// to; the older ones can.
const now = Date.now();
const iso = (msAgo: number) => new Date(now - msAgo).toISOString();
const HOUR = 3_600_000;
const DAY = 24 * HOUR;

const REVISIONS: SaveRevision[] = [
  { name: ".", when: iso(2 * HOUR), is_current: true },
  { name: "20260605T093000", when: iso(26 * HOUR), is_current: false },
  { name: "20260601T193000", when: iso(5 * DAY), is_current: false },
  { name: "20260520T120000", when: iso(20 * DAY), is_current: false },
];

const game = (overrides: Partial<LibraryGame> = {}): LibraryGame => ({
  id: "game-1",
  game_name: "Hollow Knight",
  exe_path: "C:/Games/HollowKnight/hollow_knight.exe",
  cover_image_path: null,
  accent_color: null,
  steam_id: null,
  playtime_minutes: 0,
  shortcut_app_id: null,
  last_played_at: null,
  sync_badge: null,
  game_folder_path: "C:/Games/HollowKnight",
  save_backup_count: 4,
  save_last_backed_up_at: null,
  save_backup_size_mb: 12,
  save_last_backer_device: null,
  save_cloud_revision_at: null,
  proton_version_path: null,
  ...overrides,
});

// Render the modal on a dimmed backdrop, the way showModal presents it.
const Backdrop = (node: ReactNode) => (
  <div
    style={{
      position: "fixed",
      inset: 0,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      background: "rgba(0,0,0,0.5)",
    }}
  >
    {node}
  </div>
);

const meta: Meta<typeof RevisionPickerModal> = {
  title: "Components/RevisionPickerModal",
  component: RevisionPickerModal,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof RevisionPickerModal>;

// The tip plus three older revisions. Select an older one and Restore opens the
// destructive ConfirmModal.
export const Default: Story = {
  render: () => {
    clearCallables();
    setCallable("list_save_revisions", async () => ({ ok: true, revisions: REVISIONS }));
    setCallable("restore_save_revision", async (_id: string, name: string) => {
      console.log("restore_save_revision", name);
      return { ok: true, game_count: 1 };
    });
    return Backdrop(
      <RevisionPickerModal
        game={game()}
        onRestored={() => console.log("onRestored")}
        closeModal={() => console.log("closeModal")}
      />,
    );
  },
};

// Only the current tip exists — nothing earlier to roll back to.
export const OnlyCurrent: Story = {
  render: () => {
    clearCallables();
    setCallable("list_save_revisions", async () => ({
      ok: true,
      revisions: [REVISIONS[0]],
    }));
    return Backdrop(<RevisionPickerModal game={game({ save_backup_count: 1 })} closeModal={() => {}} />);
  },
};

// No backups captured yet.
export const Empty: Story = {
  render: () => {
    clearCallables();
    setCallable("list_save_revisions", async () => ({ ok: true, revisions: [] }));
    return Backdrop(<RevisionPickerModal game={game({ save_backup_count: 0 })} closeModal={() => {}} />);
  },
};

// The list never resolves — the modal holds its loading state.
export const Loading: Story = {
  render: () => {
    clearCallables();
    setCallable("list_save_revisions", () => new Promise(() => {}));
    return Backdrop(<RevisionPickerModal game={game()} closeModal={() => {}} />);
  },
};

// The server is unreachable — the load error shows in place of the list.
export const LoadError: Story = {
  render: () => {
    clearCallables();
    setCallable("list_save_revisions", async () => ({ ok: false, reason: "server unavailable" }));
    return Backdrop(<RevisionPickerModal game={game()} closeModal={() => {}} />);
  },
};

// Restore never resolves, so the modal stays in its "Restoring…" spinner state
// (pre-selects an older revision and kicks it off on mount for the snapshot).
export const Restoring: Story = {
  render: () => {
    clearCallables();
    setCallable("list_save_revisions", async () => ({ ok: true, revisions: REVISIONS }));
    setCallable("restore_save_revision", () => new Promise(() => {}));
    return Backdrop(<RevisionPickerModal game={game()} closeModal={() => {}} />);
  },
};
