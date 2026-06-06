import type { Meta, StoryObj } from "@storybook/react-vite";
import { PeerGamesPage } from "./peer-games";
import {
  PEERS,
  PEER_GAMES,
  makeDownload,
  registerDeckyCallables,
  installFetchMock,
  installCoverArtMock,
} from "../../.storybook/mocks/fixtures";
import { clearCallables } from "../../.storybook/mocks/registry";
import { setRouteParams } from "../../.storybook/mocks/decky-ui";
import { withSteamChrome } from "../../.storybook/steam-chrome";
import { SafeArea } from "./safe-area";

// Route params drive which peer this page shows (matched against PEERS[0]).
// Route order matters: the game-list URL contains both "/lan/peers" and
// "/games", so "/games" must be listed first to win.
const page = (downloadBody: unknown = null) => {
  clearCallables();
  registerDeckyCallables();
  installCoverArtMock();
  setRouteParams({ peerAddr: "192.168.1.20", peerPort: "47632" });
  installFetchMock({
    "/games": PEER_GAMES,
    "/lan/download": downloadBody,
    "/lan/peers": PEERS,
  });
  return (
    <div style={{ minHeight: "100vh", background: "#0c0f14", color: "#fff" }}>
      <SafeArea>
        <PeerGamesPage />
      </SafeArea>
    </div>
  );
};

const meta: Meta<typeof PeerGamesPage> = {
  title: "LAN/PeerGames",
  component: PeerGamesPage,
  parameters: { layout: "fullscreen" },
  decorators: [withSteamChrome],
};
export default meta;

type Story = StoryObj<typeof PeerGamesPage>;

export const Default: Story = {
  render: () => page(),
};

// An install is in flight (Hades / pg1) — its row shows live progress + Cancel
// and a progress bar, while every other game's Download button is greyed out.
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
      <div style={{ minHeight: "100vh", background: "#0c0f14", color: "#fff" }}>
        <SafeArea>
          <PeerGamesPage />
        </SafeArea>
      </div>
    );
  },
};
