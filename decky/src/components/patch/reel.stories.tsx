import type { Meta, StoryObj } from "@storybook/react-vite";
import { Reel, TapeMeter, ensureReelKeyframes } from "./reel";

// The spin/sheen animations reference keyframes that the live code injects into
// the capsule's document. Inject them into Storybook's document so the
// animations actually run here.
const withKeyframes = (Story: () => React.ReactElement) => {
  ensureReelKeyframes(document);
  return Story();
};

const meta: Meta<typeof Reel> = {
  title: "Patch/Reel",
  component: Reel,
  decorators: [withKeyframes],
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof Reel>;

export const Static: Story = {
  args: { size: 48, color: "#d7c9a0", spinning: false },
};

export const Spinning: Story = {
  args: { size: 48, color: "#7ee2a4", spinning: true },
};

// The four save-state tones the SpoolBar uses, at the bar's real 14px size.
export const Tones: StoryObj = {
  render: () => {
    ensureReelKeyframes(document);
    const tones = [
      ["#7ee2a4", "Synced"],
      ["#f4b66c", "Not uploaded"],
      ["#7ec6ff", "Cloud newer"],
      ["rgba(244,244,245,0.40)", "No backup"],
    ] as const;
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 12, color: "#fff" }}>
        {tones.map(([color, label]) => (
          <div key={label} style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Reel size={14} color={color} />
            <span style={{ fontSize: 14, color }}>{label}</span>
          </div>
        ))}
      </div>
    );
  },
};

export const Tape: StoryObj = {
  render: () => {
    ensureReelKeyframes(document);
    return (
      <div style={{ display: "flex", alignItems: "center", gap: 8, color: "#fff" }}>
        <Reel size={14} color="#d7c9a0" spinning />
        <span style={{ fontSize: 14 }}>Backing up…</span>
        <TapeMeter accent="#d7c9a0" />
      </div>
    );
  },
};
