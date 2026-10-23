import type { Meta, StoryObj } from "@storybook/react-vite";
import { PostGameSummary } from "./post-game-summary";
import { ACHIEVEMENTS, SESSIONS, THIS_DEVICE } from "../../../.storybook/mocks/activity-fixtures";

// Sits between the Play row and the tab bar on the game page.
const onPage = (Story: () => React.ReactElement) => {
  try {
    localStorage.removeItem("spool:post-game-dismissed:story-game");
  } catch {
    /* storage unavailable */
  }
  return <div style={{ width: 900, maxWidth: "100%", padding: 24, background: "#1a1d23" }}>{Story()}</div>;
};

const meta: Meta<typeof PostGameSummary> = {
  title: "Activity/PostGameSummary",
  component: PostGameSummary,
  decorators: [onPage],
  parameters: { layout: "fullscreen" },
  args: {
    base: "http://127.0.0.1:47650",
    gameId: "story-game",
    sessions: SESSIONS,
    deviceId: THIS_DEVICE,
    accent: "#a8804a",
  },
};
export default meta;

type Story = StoryObj<typeof PostGameSummary>;

export const WithUnlocks: Story = { args: { data: ACHIEVEMENTS } };

export const NoUnlocksThisSession: Story = {
  args: {
    data: { ...ACHIEVEMENTS, items: ACHIEVEMENTS.items.map((a) => ({ ...a, live_at: null })) },
  },
};

export const NoSchema: Story = {
  args: {
    data: {
      ...ACHIEVEMENTS,
      schema_source: null,
      total: 0,
      items: ACHIEVEMENTS.items.filter((a) => a.unlocked_at !== null),
    },
  },
};
