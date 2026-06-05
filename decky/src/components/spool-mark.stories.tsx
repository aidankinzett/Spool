import type { Meta, StoryObj } from "@storybook/react";
import { SpoolMark } from "./spool-mark";

const meta: Meta<typeof SpoolMark> = {
  title: "Components/SpoolMark",
  component: SpoolMark,
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof SpoolMark>;

export const Default: Story = {
  args: { size: 64, color: "#f4f4f5" },
};

export const WithAccentTape: Story = {
  args: { size: 64, color: "#f4f4f5", tape: "#d7c9a0" },
};

export const Dim: Story = {
  args: { size: 64, color: "#f4f4f5", tape: "#7ec6ff", dim: true },
};

export const Sizes: StoryObj = {
  render: () => (
    <div style={{ display: "flex", alignItems: "flex-end", gap: 20, color: "#fff" }}>
      {[17, 24, 40, 64].map((s) => (
        <SpoolMark key={s} size={s} color="#f4f4f5" tape="#d7c9a0" />
      ))}
    </div>
  ),
};
