import type { Meta, StoryObj } from "@storybook/react-vite";
import { LanPage } from "./lan-view";
import { PEERS, registerDeckyCallables, installFetchMock } from "../../.storybook/mocks/fixtures";
import { clearCallables } from "../../.storybook/mocks/registry";
import { withSteamChrome } from "../../.storybook/steam-chrome";
import { SafeArea } from "./safe-area";

// Full-screen route; render on a Game-Mode-sized dark surface. minHeight 100vh
// so the fixed Steam footer (added by `withSteamChrome`) overlays real content.
// SafeArea matches how the route is wrapped in index.tsx, insetting content
// clear of the header/footer bars.
const fullscreen = (setup: () => void) => {
  clearCallables();
  registerDeckyCallables();
  setup();
  return (
    <div style={{ minHeight: "100vh", background: "#0c0f14", color: "#fff" }}>
      <SafeArea>
        <LanPage />
      </SafeArea>
    </div>
  );
};

const meta: Meta<typeof LanPage> = {
  title: "LAN/PeersList",
  component: LanPage,
  parameters: { layout: "fullscreen" },
  decorators: [withSteamChrome],
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
      <div style={{ minHeight: "100vh", background: "#0c0f14", color: "#fff" }}>
        <SafeArea>
          <LanPage />
        </SafeArea>
      </div>
    );
  },
};
