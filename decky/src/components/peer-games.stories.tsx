import type { Meta, StoryObj } from "@storybook/react";
import { PeerGamesPage } from "./peer-games";
import {
  PEERS,
  PEER_GAMES,
  makeDownload,
  registerDeckyCallables,
  installFetchMock,
} from "../../.storybook/mocks/fixtures";
import { clearCallables } from "../../.storybook/mocks/registry";
import { setRouteParams } from "../../.storybook/mocks/decky-ui";

// Route params drive which peer this page shows (matched against PEERS[0]).
// Route order matters: the game-list URL contains both "/lan/peers" and
// "/games", so "/games" must be listed first to win.
const page = (downloadBody: unknown = null) => {
  clearCallables();
  registerDeckyCallables();
  setRouteParams({ peerAddr: "192.168.1.20", peerPort: "47632" });
  installFetchMock({
    "/games": PEER_GAMES,
    "/lan/download": downloadBody,
    "/lan/peers": PEERS,
  });
  return (
    <div style={{ minHeight: 600, background: "#0c0f14", color: "#fff" }}>
      <PeerGamesPage />
    </div>
  );
};

const meta: Meta<typeof PeerGamesPage> = {
  title: "LAN/PeerGames",
  component: PeerGamesPage,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof PeerGamesPage>;

export const Default: Story = {
  render: () => page(),
};

// An install is in flight — the progress row with a live percentage shows above
// the grid.
export const Downloading: Story = {
  render: () => page(makeDownload()),
};

export const NothingShared: Story = {
  render: () => {
    clearCallables();
    registerDeckyCallables();
    setRouteParams({ peerAddr: "192.168.1.20", peerPort: "47632" });
    installFetchMock({ "/games": [], "/lan/download": null, "/lan/peers": PEERS });
    return (
      <div style={{ minHeight: 600, background: "#0c0f14", color: "#fff" }}>
        <PeerGamesPage />
      </div>
    );
  },
};
