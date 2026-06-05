import type { Meta, StoryObj } from "@storybook/react";
import { Content } from "./content";
import { makeGame, registerDeckyCallables, installFetchMock } from "../../.storybook/mocks/fixtures";
import { clearCallables } from "../../.storybook/mocks/registry";

// The QAM panel content. Renders inside Decky's ~320px Quick Access flyout, so
// constrain the width to match.
const inQam = (Story: () => React.ReactElement, serverRunning = true) => {
  clearCallables();
  registerDeckyCallables({ serverRunning });
  installFetchMock({
    "/library": [
      makeGame({ id: "g1", game_name: "Hollow Knight", shortcut_app_id: 111 }),
      makeGame({ id: "g2", game_name: "Celeste", shortcut_app_id: null, steam_id: null }),
      makeGame({ id: "g3", game_name: "Stardew Valley", shortcut_app_id: null, steam_id: null }),
    ],
  });
  return (
    <div style={{ width: 320, background: "#0c0f14", padding: "8px 12px", color: "#fff" }}>
      {Story()}
    </div>
  );
};

const meta: Meta<typeof Content> = {
  title: "Components/Content (QAM)",
  component: Content,
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof Content>;

export const Default: Story = {
  render: () => inQam(() => <Content />),
};

// Server not running — useServerBase reports the "launch Spool" hint and the
// Add-to-Steam list shows nothing to load.
export const ServerOffline: Story = {
  render: () => inQam(() => <Content />, false),
};
