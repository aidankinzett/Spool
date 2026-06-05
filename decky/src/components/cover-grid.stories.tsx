import type { Meta, StoryObj } from "@storybook/react";
import { CoverGrid, type Tile } from "./cover-grid";

// A solid-colour SVG data URL so cover tiles render offline without a network
// fetch.
const cover = (label: string, color: string) =>
  `data:image/svg+xml;utf8,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="300" height="450"><rect width="300" height="450" fill="${color}"/><text x="150" y="225" fill="white" font-family="sans-serif" font-size="28" text-anchor="middle">${label}</text></svg>`,
  )}`;

const tiles: Tile[] = [
  { key: "1", name: "Hollow Knight", coverUrl: cover("Hollow Knight", "#2b3a55"), accentColor: "#2b3a55" },
  { key: "2", name: "Celeste", coverUrl: cover("Celeste", "#7b3f6b"), accentColor: "#7b3f6b" },
  { key: "3", name: "Hades", coverUrl: cover("Hades", "#a13b2f"), accentColor: "#a13b2f" },
  // No cover art — falls back to the name label on the accent background.
  { key: "4", name: "An Unidentified Game With A Long Name", coverUrl: null, accentColor: "#1f6f5c" },
  { key: "5", name: "No Accent Either", coverUrl: null },
];

const meta: Meta<typeof CoverGrid> = {
  title: "Components/CoverGrid",
  component: CoverGrid,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof CoverGrid>;

export const Default: Story = {
  args: {
    tiles,
    onActivate: (key: string) => console.log("activate", key),
  },
};

export const SingleTile: Story = {
  args: {
    tiles: [tiles[0]],
  },
};

export const MissingArt: Story = {
  args: {
    tiles: tiles.filter((t) => t.coverUrl === null),
  },
};
