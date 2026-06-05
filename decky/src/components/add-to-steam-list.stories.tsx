import type { Meta, StoryObj } from "@storybook/react";
import { AddToSteamList } from "./add-to-steam-list";
import { makeGame, registerDeckyCallables, installFetchMock } from "../../.storybook/mocks/fixtures";
import { clearCallables } from "../../.storybook/mocks/registry";

// `existingShortcutAppId` checks SteamClient (absent here), so every game reads
// as "not yet added" — the selectable state. Clicking calls launch.ts, which
// needs Game Mode and will surface a "needs Game Mode" toast in the console;
// the list rendering itself is the point.
const list = (games: ReturnType<typeof makeGame>[]) => {
  clearCallables();
  registerDeckyCallables();
  installFetchMock({ "/library": games });
  return (
    <div style={{ width: 320, background: "#0c0f14", padding: "8px 12px", color: "#fff" }}>
      <AddToSteamList />
    </div>
  );
};

const meta: Meta<typeof AddToSteamList> = {
  title: "Components/AddToSteamList",
  component: AddToSteamList,
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof AddToSteamList>;

export const Default: Story = {
  render: () =>
    list([
      makeGame({ id: "g1", game_name: "Hollow Knight" }),
      makeGame({ id: "g2", game_name: "Celeste" }),
      makeGame({ id: "g3", game_name: "Stardew Valley" }),
      makeGame({ id: "g4", game_name: "A Game With A Rather Long Title Here" }),
    ]),
};

export const Empty: Story = {
  render: () => list([]),
};
