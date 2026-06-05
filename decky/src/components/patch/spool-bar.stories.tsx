import type { Meta, StoryObj } from "@storybook/react";
import { SpoolBar } from "./spool-bar";
import { ensureReelKeyframes } from "./reel";
import { makeGame, registerDeckyCallables } from "../../../.storybook/mocks/fixtures";
import { clearCallables } from "../../../.storybook/mocks/registry";

// The bar normally overlays the hero capsule art; render it on a dark surface
// at a representative width so the layout reads the way it does in Game Mode.
const onHero = (Story: () => React.ReactElement) => {
  ensureReelKeyframes(document);
  clearCallables();
  registerDeckyCallables();
  return (
    <div style={{ width: 900, maxWidth: "100%", padding: 24, background: "#0c0f14" }}>
      {Story()}
    </div>
  );
};

const meta: Meta<typeof SpoolBar> = {
  title: "Patch/SpoolBar",
  component: SpoolBar,
  decorators: [onHero],
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof SpoolBar>;

export const Synced: Story = {
  args: { appid: 3000123456, backingUp: false, game: makeGame({ sync_badge: "synced" }) },
};

export const NotUploaded: Story = {
  args: {
    appid: 3000123456,
    backingUp: false,
    game: makeGame({ sync_badge: "local-newer" }),
  },
};

export const CloudNewer: Story = {
  args: {
    appid: 3000123456,
    backingUp: false,
    game: makeGame({
      sync_badge: "cloud-newer",
      save_last_backer_device: "Desktop-PC",
      save_cloud_revision_at: "2026-06-05T07:00:00Z",
    }),
  },
};

export const NoBackupYet: Story = {
  args: {
    appid: 3000123456,
    backingUp: false,
    game: makeGame({ sync_badge: null, save_last_backed_up_at: null, save_backup_count: 0 }),
  },
};

export const BackingUp: Story = {
  args: { appid: 3000123456, backingUp: true, game: makeGame() },
};

// A native Linux game (no .exe) trims the Proton/deps menu items; a game with
// no install folder also drops Delete. (Open the ⋮ menu to see the difference.)
export const NativeLinuxGame: Story = {
  args: {
    appid: 3000123456,
    backingUp: false,
    game: makeGame({ exe_path: "/games/celeste/Celeste", game_name: "Celeste" }),
  },
};
