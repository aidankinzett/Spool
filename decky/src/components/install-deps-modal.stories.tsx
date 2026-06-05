import type { Meta, StoryObj } from "@storybook/react-vite";
import { InstallDepsModal } from "./install-deps-modal";
import { makeGame, registerDeckyCallables } from "../../.storybook/mocks/fixtures";
import { clearCallables, setCallable } from "../../.storybook/mocks/registry";

const backdrop = (node: React.ReactNode) => (
  <div
    style={{
      position: "fixed",
      inset: 0,
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      background: "rgba(0,0,0,0.5)",
    }}
  >
    {node}
  </div>
);

const meta: Meta<typeof InstallDepsModal> = {
  title: "Components/InstallDepsModal",
  component: InstallDepsModal,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof InstallDepsModal>;

export const Default: Story = {
  render: () => {
    clearCallables();
    registerDeckyCallables();
    return backdrop(
      <InstallDepsModal game={makeGame()} closeModal={() => console.log("closeModal")} />,
    );
  },
};

// Install fails — the failure toast is console-logged by the mock.
export const InstallFails: Story = {
  render: () => {
    clearCallables();
    registerDeckyCallables();
    // Override just the install handler to fail.
    setCallable("install_deps", async () => ({ ok: false, reason: "no Proton set" }));
    return backdrop(<InstallDepsModal game={makeGame()} closeModal={() => {}} />);
  },
};
