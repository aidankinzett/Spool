import type { Meta, StoryObj } from "@storybook/react-vite";
import { PeerGameDetailPage } from "./peer-game-detail-panel";
import { PEERS, PEER_GAMES, registerDeckyCallables, installFetchMock, installCoverArtMock } from "../../.storybook/mocks/fixtures";
import { clearCallables } from "../../.storybook/mocks/registry";
import { setRouteParams } from "../../.storybook/mocks/decky-ui";
import { withSteamChrome } from "../../.storybook/steam-chrome";

const page = (gameId: string) => {
  clearCallables();
  registerDeckyCallables();
  installCoverArtMock();
  setRouteParams({ peerAddr: "192.168.1.20", peerPort: "47632", gameId });
  installFetchMock({ "/games": PEER_GAMES, "/lan/peers": PEERS });
  return (
    <div style={{ height: "100vh", background: "#0c0f14", color: "#fff" }}>
      <PeerGameDetailPage />
    </div>
  );
};

const meta: Meta<typeof PeerGameDetailPage> = {
  title: "LAN/PeerGameDetail",
  component: PeerGameDetailPage,
  parameters: { layout: "fullscreen" },
  decorators: [withSteamChrome],
};
export default meta;

type Story = StoryObj<typeof PeerGameDetailPage>;

// A shareable game — Download button enabled.
export const Shareable: Story = {
  render: () => page("pg1"),
};

// A game flagged not-shared — Download disabled, "Not available".
export const NotShared: Story = {
  render: () => page("pg4"),
};
