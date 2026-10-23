import type { Meta, StoryObj } from "@storybook/react-vite";
import { ActivityTimeline } from "./activity-timeline";
import { ACHIEVEMENTS, SESSIONS, THIS_DEVICE } from "../../../.storybook/mocks/activity-fixtures";

// Top of the game page's Activity tab.
const onPage = (Story: () => React.ReactElement) => (
  <div style={{ width: 900, maxWidth: "100%", padding: 24, background: "#1a1d23" }}>{Story()}</div>
);

const meta: Meta<typeof ActivityTimeline> = {
  title: "Activity/ActivityTimeline",
  component: ActivityTimeline,
  decorators: [onPage],
  parameters: { layout: "fullscreen" },
  args: {
    base: "http://127.0.0.1:47650",
    sessions: SESSIONS,
    data: ACHIEVEMENTS,
    deviceId: THIS_DEVICE,
    accent: "#a8804a",
  },
};
export default meta;

type Story = StoryObj<typeof ActivityTimeline>;

export const Default: Story = {};

export const SessionsOnly: Story = { args: { data: null } };
