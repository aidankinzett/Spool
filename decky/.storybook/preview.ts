import type { Preview } from "@storybook/react-vite";

// A free near-match for Steam's proprietary "Motiva Sans" (see
// preview-head.html), with a system fallback if the webfont is unreachable.
// Applied to the canvas so @decky/ui mock components — which in Steam inherit
// the UI font — read consistently here.
const STEAM_FONT =
  `"Open Sans", "Segoe UI", system-ui, -apple-system, sans-serif`;

if (typeof document !== "undefined") {
  const style = document.createElement("style");
  style.textContent = `body, .sb-show-main { font-family: ${STEAM_FONT}; }`;
  document.head.appendChild(style);
}

// Game Mode renders on a dark surface; default the canvas to match so the
// plugin's light-on-dark styling reads correctly.
const preview: Preview = {
  parameters: {
    backgrounds: {
      options: {
        steam: { name: "steam", value: "#1a1d23" },
        black: { name: "black", value: "#000000" }
      }
    },
    controls: {
      matchers: { color: /(background|color)$/i, date: /Date$/i },
    },
  },

  initialGlobals: {
    backgrounds: {
      value: "steam"
    }
  }
};

export default preview;
