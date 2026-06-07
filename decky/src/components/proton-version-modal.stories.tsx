import type { Meta, StoryObj } from "@storybook/react-vite";
import { ProtonVersionModal } from "./proton-version-modal";
import type { LibraryGame, ProtonVersion } from "../types";
import { setCallable, clearCallables } from "../../.storybook/mocks/registry";

const VERSIONS: ProtonVersion[] = [
  { name: "GE-Proton9-20", path: "/home/deck/.steam/root/compatibilitytools.d/GE-Proton9-20", source: "GE-Proton" },
  { name: "Proton 9.0", path: "/home/deck/.steam/root/steamapps/common/Proton 9.0", source: "Steam" },
  { name: "UMU-Proton-9.0", path: "/home/deck/.local/share/umu/UMU-Proton-9.0", source: "UMU-Proton" },
];

const game = (overrides: Partial<LibraryGame> = {}): LibraryGame => ({
  id: "game-1",
  game_name: "Elden Ring",
  exe_path: "C:/Games/EldenRing/eldenring.exe",
  cover_image_path: null,
  accent_color: null,
  steam_id: null,
  playtime_minutes: 0,
  shortcut_app_id: null,
  last_played_at: null,
  sync_badge: null,
  game_folder_path: null,
  installed: true,
  save_backup_count: 0,
  save_last_backed_up_at: null,
  save_backup_size_mb: 0,
  save_last_backer_device: null,
  save_cloud_revision_at: null,
  proton_version_path: null,
  ...overrides,
});

// Render the modal on a dimmed backdrop, the way showModal presents it.
const Backdrop = (node: React.ReactNode) => (
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

const meta: Meta<typeof ProtonVersionModal> = {
  title: "Components/ProtonVersionModal",
  component: ProtonVersionModal,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof ProtonVersionModal>;

export const Default: Story = {
  render: () => {
    clearCallables();
    setCallable("list_proton_versions", async () => VERSIONS);
    setCallable("set_proton_version", async (_id: string, path: string) => {
      console.log("set_proton_version", path);
      return { ok: true };
    });
    return Backdrop(
      <ProtonVersionModal
        game={game()}
        onSaved={() => console.log("onSaved")}
        closeModal={() => console.log("closeModal")}
      />,
    );
  },
};

// A game already pinned to a specific build — that row shows the check.
export const Preselected: Story = {
  render: () => {
    clearCallables();
    setCallable("list_proton_versions", async () => VERSIONS);
    setCallable("set_proton_version", async () => ({ ok: true }));
    return Backdrop(
      <ProtonVersionModal
        game={game({ proton_version_path: VERSIONS[0].path })}
        closeModal={() => console.log("closeModal")}
      />,
    );
  },
};

// The list never resolves, so the modal stays in its loading state.
export const Loading: Story = {
  render: () => {
    clearCallables();
    setCallable("list_proton_versions", () => new Promise(() => {}));
    return Backdrop(<ProtonVersionModal game={game()} closeModal={() => {}} />);
  },
};

// Saving fails — the toast (console-logged by the mock) reports the reason.
export const SaveError: Story = {
  render: () => {
    clearCallables();
    setCallable("list_proton_versions", async () => VERSIONS);
    setCallable("set_proton_version", async () => ({
      ok: false,
      reason: "server unavailable",
    }));
    return Backdrop(<ProtonVersionModal game={game()} closeModal={() => {}} />);
  },
};
