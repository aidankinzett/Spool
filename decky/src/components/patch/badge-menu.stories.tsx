import type { Meta, StoryObj } from "@storybook/react";
import { BadgeMenuButton } from "./badge-menu";
import { makeGame, registerDeckyCallables } from "../../../.storybook/mocks/fixtures";
import { clearCallables } from "../../../.storybook/mocks/registry";

// The ⋮ trigger renders standalone; clicking it opens the Spool context menu
// (the mock's showContextMenu mounts it anchored to the button). Menu actions
// fire the registered callables and surface results as console toasts.
const withCallables = (Story: () => React.ReactElement) => {
  clearCallables();
  registerDeckyCallables();
  return (
    <div style={{ padding: 40, background: "#15181d", display: "inline-block" }}>
      {Story()}
    </div>
  );
};

const meta: Meta<typeof BadgeMenuButton> = {
  title: "Patch/BadgeMenuButton",
  component: BadgeMenuButton,
  decorators: [withCallables],
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof BadgeMenuButton>;

// .exe game with an install folder → full menu (Proton, Install deps, Delete).
export const FullMenu: Story = {
  args: { appid: 3000123456, game: makeGame() },
};

// Native Linux game → no Proton/deps entries.
export const NativeLinux: Story = {
  args: {
    appid: 3000123456,
    game: makeGame({ exe_path: "/games/celeste/Celeste", game_name: "Celeste" }),
  },
};

// No known install folder → no Delete entry.
export const NoInstallFolder: Story = {
  args: { appid: 3000123456, game: makeGame({ game_folder_path: null }) },
};
