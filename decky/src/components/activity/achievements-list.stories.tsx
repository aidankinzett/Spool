import type { Meta, StoryObj } from "@storybook/react-vite";
import { AchievementsList } from "./achievements-list";
import { ACHIEVEMENTS } from "../../../.storybook/mocks/activity-fixtures";

// Below the timeline in the game page's Activity tab.
const onPage = (Story: () => React.ReactElement) => (
  <div style={{ width: 900, maxWidth: "100%", padding: 24, background: "#1a1d23" }}>{Story()}</div>
);

const meta: Meta<typeof AchievementsList> = {
  title: "Activity/AchievementsList",
  component: AchievementsList,
  decorators: [onPage],
  parameters: { layout: "fullscreen" },
  args: { base: "http://127.0.0.1:47650", data: ACHIEVEMENTS, accent: "#a8804a" },
};
export default meta;

type Story = StoryObj<typeof AchievementsList>;

export const Default: Story = {};

// Without a schema only unlocked achievements are known, by API name.
export const NoSchema: Story = {
  args: {
    data: {
      ...ACHIEVEMENTS,
      schema_source: null,
      total: 0,
      items: ACHIEVEMENTS.items
        .filter((a) => a.unlocked_at !== null)
        .map((a) => ({ ...a, display_name: a.api_name, description: "", icon: null, icon_gray: null })),
    },
  },
};
