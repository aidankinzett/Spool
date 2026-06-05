import type { Meta, StoryObj } from "@storybook/react";
import { LanPage } from "./lan-view";
import { PEERS, registerDeckyCallables, installFetchMock } from "../../.storybook/mocks/fixtures";
import { clearCallables } from "../../.storybook/mocks/registry";

// Full-screen route; render on a Game-Mode-sized dark surface.
const fullscreen = (setup: () => void) => {
  clearCallables();
  registerDeckyCallables();
  setup();
  return (
    <div style={{ minHeight: 600, background: "#0c0f14", color: "#fff" }}>
      <LanPage />
    </div>
  );
};

const meta: Meta<typeof LanPage> = {
  title: "LAN/PeersList",
  component: LanPage,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof LanPage>;

export const WithPeers: Story = {
  render: () => fullscreen(() => installFetchMock({ "/lan/peers": PEERS })),
};

export const NoPeers: Story = {
  render: () => fullscreen(() => installFetchMock({ "/lan/peers": [] })),
};

// Server not running → the hook reports the "launch Spool" error before any
// fetch happens.
export const ServerOffline: Story = {
  render: () => {
    clearCallables();
    registerDeckyCallables({ serverRunning: false });
    return (
      <div style={{ minHeight: 600, background: "#0c0f14", color: "#fff" }}>
        <LanPage />
      </div>
    );
  },
};
