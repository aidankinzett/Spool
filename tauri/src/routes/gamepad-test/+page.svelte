<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';

  // Standalone Gamepad API smoke test. The whole point is to answer one
  // question before we build controller navigation: does the webview actually
  // see a connected controller? WebView2 (Windows) is expected to; WebKitGTK
  // (the Linux AppImage) only exposes the Gamepad API when libmanette is
  // present, so this page is how we confirm the Linux path before investing in
  // a focus/navigation layer. If pads never show up here on Linux, the input
  // half has to move to Rust (gilrs) instead.
  //
  // Deliberately self-contained — no $lib/api, no Tauri IPC — so it behaves the
  // same in `bun run tauri dev` and in a packaged build. Reach it at /gamepad-test.

  type PadSnapshot = {
    index: number;
    id: string;
    mapping: string;
    connected: boolean;
    timestamp: number;
    axes: number[];
    buttons: { pressed: boolean; touched: boolean; value: number }[];
  };

  let supported = $state(true);
  let pads = $state<PadSnapshot[]>([]);
  let lastEvent = $state<string>('—');
  let frames = $state(0);

  // Events from the Rust gilrs bridge (gamepad:input). This is the path that
  // actually matters on Linux: the webview Gamepad API above stays empty in the
  // AppImage / Steam Game Mode, so if controller input works at all it shows up
  // here. In Game Mode, press dpad / A / B and confirm entries appear.
  type BridgeEvent = {
    kind: string;
    button?: string;
    axis?: string;
    value: number;
    gamepad: string;
  };
  type BridgeLogEntry = BridgeEvent & { t: string };
  let bridgeLog = $state<BridgeLogEntry[]>([]);
  let bridgeCount = $state(0);

  // Standard mapping button names, for readability when mapping === "standard".
  const STANDARD_BUTTONS = [
    'A', 'B', 'X', 'Y',
    'LB', 'RB', 'LT', 'RT',
    'Back', 'Start', 'L3', 'R3',
    'D-Up', 'D-Down', 'D-Left', 'D-Right',
    'Guide',
  ];

  function buttonLabel(mapping: string, i: number): string {
    if (mapping === 'standard' && i < STANDARD_BUTTONS.length) {
      return `${STANDARD_BUTTONS[i]} (${i})`;
    }
    return `Button ${i}`;
  }

  function snapshot(): PadSnapshot[] {
    // navigator.getGamepads() returns live snapshots; you must re-read every
    // frame. Entries can be null for empty slots.
    const raw = navigator.getGamepads ? navigator.getGamepads() : [];
    const out: PadSnapshot[] = [];
    for (const p of raw) {
      if (!p) continue;
      out.push({
        index: p.index,
        id: p.id,
        mapping: p.mapping || '(none)',
        connected: p.connected,
        timestamp: p.timestamp,
        axes: Array.from(p.axes),
        buttons: p.buttons.map((b) => ({
          pressed: b.pressed,
          touched: b.touched,
          value: b.value,
        })),
      });
    }
    return out;
  }

  onMount(() => {
    // Rust gilrs bridge listener — attach this regardless of Gamepad API
    // support, since on Linux the API is empty but the bridge is what works.
    let unlisten: UnlistenFn | undefined;
    listen<BridgeEvent>('gamepad:input', (e) => {
      const t = new Date().toLocaleTimeString(undefined, { hour12: false });
      bridgeCount += 1;
      bridgeLog = [{ t, ...e.payload }, ...bridgeLog].slice(0, 40);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => console.error('[gamepad-test] listen failed:', err));

    // Browser Gamepad API path (works on Windows/WebView2; empty on Linux).
    const gamepadApi = typeof navigator !== 'undefined' && !!navigator.getGamepads;
    supported = gamepadApi;

    let raf = 0;
    let onConnect: ((e: GamepadEvent) => void) | undefined;
    let onDisconnect: ((e: GamepadEvent) => void) | undefined;

    if (gamepadApi) {
      onConnect = (e: GamepadEvent) => {
        lastEvent = `connected: [${e.gamepad.index}] ${e.gamepad.id} (mapping: ${e.gamepad.mapping || 'none'})`;
      };
      onDisconnect = (e: GamepadEvent) => {
        lastEvent = `disconnected: [${e.gamepad.index}] ${e.gamepad.id}`;
      };
      window.addEventListener('gamepadconnected', onConnect);
      window.addEventListener('gamepaddisconnected', onDisconnect);

      const loop = () => {
        pads = snapshot();
        frames += 1;
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);
    }

    return () => {
      unlisten?.();
      if (raf) cancelAnimationFrame(raf);
      if (onConnect) window.removeEventListener('gamepadconnected', onConnect);
      if (onDisconnect) window.removeEventListener('gamepaddisconnected', onDisconnect);
    };
  });

  // Some browsers only start reporting a pad after the first input ("button
  // ghosting" privacy mitigation), so a freshly plugged controller can be
  // invisible until you press something. Say so explicitly.
  const noPads = $derived(supported && pads.length === 0);
</script>

<svelte:head>
  <title>Gamepad smoke test</title>
</svelte:head>

<main>
  <header>
    <button class="back" onclick={() => goto('/')}>← Back to library</button>
    <h1>Gamepad smoke test</h1>
    <p class="sub">
      Confirms whether this webview can see a controller via the browser Gamepad
      API. Frames polled: <strong>{frames}</strong>
    </p>
    <p class="event">Last event: <code>{lastEvent}</code></p>
  </header>

  <section class="bridge">
    <h2>Rust bridge (gilrs) — <code>gamepad:input</code></h2>
    <p class="sub">
      The path that matters on Linux. Events here come from the Rust gilrs reader,
      not the webview. In Steam Game Mode (gamepad emulation), press dpad / A / B
      and confirm entries appear below. Events received: <strong>{bridgeCount}</strong>
    </p>
    {#if bridgeLog.length === 0}
      <div class="banner warn">
        <strong>No bridge events yet.</strong>
        <p>
          Press buttons on the controller. If nothing appears here even in Game
          Mode with gamepad emulation, gilrs can't see the (virtual) pad — check
          the app log for "gamepad bridge" lines.
        </p>
      </div>
    {:else}
      <ul class="evlog">
        {#each bridgeLog as ev, i (i)}
          <li class:down={ev.kind === 'button-down'}>
            <span class="ev-t">{ev.t}</span>
            <span class="ev-kind">{ev.kind}</span>
            <span class="ev-name">{ev.button ?? ev.axis ?? '—'}</span>
            <span class="ev-val">{ev.value.toFixed(2)}</span>
            <span class="ev-pad">{ev.gamepad}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <h2 class="api-heading">Browser Gamepad API</h2>

  {#if !supported}
    <div class="banner bad">
      <strong>navigator.getGamepads is unavailable in this webview.</strong>
      <p>
        The Gamepad API isn't exposed here. On the Linux AppImage this usually
        means WebKitGTK was built/loaded without libmanette support — the input
        layer would need to come from Rust (gilrs) instead.
      </p>
    </div>
  {:else if noPads}
    <div class="banner warn">
      <strong>No controllers detected yet.</strong>
      <p>
        Plug in / wake a controller and <em>press any button</em>. Many webviews
        hide a pad until its first input. If nothing ever appears here, the
        webview can't see the controller.
      </p>
    </div>
  {/if}

  {#each pads as pad (pad.index)}
    <section class="pad">
      <div class="pad-head">
        <span class="badge">#{pad.index}</span>
        <span class="pad-id">{pad.id}</span>
        <span class="mapping" class:standard={pad.mapping === 'standard'}>
          mapping: {pad.mapping}
        </span>
        <span class="conn" class:on={pad.connected}>
          {pad.connected ? 'connected' : 'disconnected'}
        </span>
      </div>

      <h3>Axes</h3>
      <div class="axes">
        {#each pad.axes as axis, i (i)}
          <div class="axis">
            <span class="axis-label">Axis {i}</span>
            <div class="bar">
              <div class="fill" style="left: {((axis + 1) / 2) * 100}%;"></div>
            </div>
            <span class="val">{axis.toFixed(2)}</span>
          </div>
        {/each}
      </div>

      <h3>Buttons</h3>
      <div class="buttons">
        {#each pad.buttons as btn, i (i)}
          <div class="btn" class:pressed={btn.pressed} class:touched={btn.touched}>
            <span class="btn-name">{buttonLabel(pad.mapping, i)}</span>
            <span class="btn-val">{btn.value.toFixed(2)}</span>
          </div>
        {/each}
      </div>
    </section>
  {/each}
</main>

<style>
  main {
    padding: 1.5rem 2rem 3rem;
    color: #e6edf3;
    background: #0d1117;
    min-height: 100vh;
    font-family:
      system-ui,
      -apple-system,
      sans-serif;
  }

  header {
    margin-bottom: 1.5rem;
  }

  .back {
    background: none;
    border: none;
    color: #58a6ff;
    cursor: pointer;
    padding: 0;
    margin-bottom: 0.75rem;
    font-size: 0.85rem;
  }
  .back:hover {
    text-decoration: underline;
  }

  h1 {
    margin: 0 0 0.25rem;
    font-size: 1.4rem;
  }

  .sub,
  .event {
    margin: 0.25rem 0;
    color: #9da7b3;
    font-size: 0.9rem;
  }

  code {
    background: #161b22;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-size: 0.85rem;
  }

  .bridge {
    background: #161b22;
    border: 1px solid #1f6feb;
    border-radius: 10px;
    padding: 1rem 1.25rem;
    margin-bottom: 1.5rem;
  }
  .bridge h2 {
    margin: 0 0 0.25rem;
    font-size: 1.05rem;
  }
  .api-heading {
    font-size: 1.05rem;
    margin: 1.5rem 0 0.75rem;
    color: #9da7b3;
  }

  .evlog {
    list-style: none;
    margin: 0.75rem 0 0;
    padding: 0;
    font-family: ui-monospace, monospace;
    font-size: 0.82rem;
    max-height: 16rem;
    overflow-y: auto;
  }
  .evlog li {
    display: grid;
    grid-template-columns: 5.5rem 6.5rem 1fr 3.5rem auto;
    gap: 0.6rem;
    padding: 0.2rem 0.4rem;
    border-radius: 4px;
  }
  .evlog li.down {
    background: rgba(31, 111, 235, 0.18);
  }
  .ev-t {
    color: #6e7681;
  }
  .ev-kind {
    color: #58a6ff;
  }
  .ev-name {
    font-weight: 600;
  }
  .ev-val {
    text-align: right;
    color: #9da7b3;
  }
  .ev-pad {
    color: #6e7681;
  }

  .banner {
    border-radius: 8px;
    padding: 1rem 1.25rem;
    margin-bottom: 1.5rem;
    border: 1px solid;
  }
  .banner p {
    margin: 0.5rem 0 0;
    font-size: 0.9rem;
    line-height: 1.45;
  }
  .banner.bad {
    background: #2d1417;
    border-color: #f85149;
  }
  .banner.warn {
    background: #2d2410;
    border-color: #d29922;
  }

  .pad {
    background: #161b22;
    border: 1px solid #21262d;
    border-radius: 10px;
    padding: 1rem 1.25rem;
    margin-bottom: 1.25rem;
  }

  .pad-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.6rem;
    margin-bottom: 0.75rem;
  }
  .badge {
    background: #1f6feb;
    color: #fff;
    border-radius: 6px;
    padding: 0.1rem 0.5rem;
    font-weight: 700;
    font-size: 0.85rem;
  }
  .pad-id {
    font-weight: 600;
    flex: 1;
    min-width: 12rem;
  }
  .mapping {
    font-size: 0.8rem;
    color: #9da7b3;
  }
  .mapping.standard {
    color: #3fb950;
  }
  .conn {
    font-size: 0.8rem;
    color: #f85149;
  }
  .conn.on {
    color: #3fb950;
  }

  h3 {
    margin: 0.75rem 0 0.5rem;
    font-size: 0.95rem;
    color: #c9d1d9;
  }

  .axes {
    display: grid;
    gap: 0.4rem;
  }
  .axis {
    display: grid;
    grid-template-columns: 4rem 1fr 3rem;
    align-items: center;
    gap: 0.6rem;
  }
  .axis-label {
    font-size: 0.8rem;
    color: #9da7b3;
  }
  .bar {
    position: relative;
    height: 8px;
    background: #21262d;
    border-radius: 4px;
  }
  .fill {
    position: absolute;
    top: -3px;
    width: 14px;
    height: 14px;
    margin-left: -7px;
    background: #58a6ff;
    border-radius: 50%;
  }
  .val {
    font-variant-numeric: tabular-nums;
    font-size: 0.8rem;
    text-align: right;
  }

  .buttons {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 0.4rem;
  }
  .btn {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    background: #21262d;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 0.4rem 0.6rem;
    font-size: 0.82rem;
    transition:
      background 0.05s,
      border-color 0.05s;
  }
  .btn.touched {
    border-color: #58a6ff;
  }
  .btn.pressed {
    background: #1f6feb;
    border-color: #58a6ff;
    color: #fff;
  }
  .btn-name {
    font-weight: 600;
  }
  .btn-val {
    font-variant-numeric: tabular-nums;
    color: #9da7b3;
  }
  .btn.pressed .btn-val {
    color: #cce0ff;
  }
</style>
