import {
  addEventListener,
  definePlugin,
  removeEventListener,
  routerHook,
  toaster,
} from "@decky/api";
import {
  afterPatch,
  appDetailsClasses,
  createReactTreePatcher,
  findInReactTree,
  staticClasses
} from "@decky/ui";
import { createElement, type ReactElement } from "react";
import { FaFloppyDisk } from "react-icons/fa6";
import { SPOOL_LAN_ROUTE, SPOOL_LAN_PEER_ROUTE } from "./constants";
import { onAppStop } from "./api/callables";
import { backupStarted, backupFinished } from "./lib/backup-status";
import { Content } from "./components/content";
import { LanPage } from "./components/lan-view";
import { PeerGamesPage } from "./components/peer-games";
import { PatchWrapper } from "./components/patch/patch-wrapper";
import { SafeArea } from "./components/safe-area";

export default definePlugin(() => {
  // Register the full-screen LAN browse routes. The QAM "Browse LAN games"
  // button navigates to them; we remove them on dismount to avoid duplicate
  // patches across hot-reloads.
  // The header/footer chrome overlays page content, so these top-to-bottom
  // pages are wrapped in SafeArea to clear it.
  routerHook.addRoute(SPOOL_LAN_ROUTE, () => (
    <SafeArea>
      <LanPage />
    </SafeArea>
  ), { exact: true });
  routerHook.addRoute(SPOOL_LAN_PEER_ROUTE, () => (
    <SafeArea scroll>
      <PeerGamesPage />
    </SafeArea>
  ), { exact: true });

  // Patch the Steam game-detail page to inject Spool's cross-device playtime
  // badge. Uses afterPatch + findInReactTree to splice into the InnerContainer
  // of the rendered tree
  const playtimePatch = routerHook.addPatch(
    "/library/app/:appid",
    (tree: any) => {
      const routeProps = findInReactTree(tree, (x: any) => x?.renderFunc);
      if (!routeProps) return tree;

      const patchHandler = createReactTreePatcher(
        [
          (t: any) =>
            findInReactTree(t, (x: any) => x?.props?.children?.props?.overview)
              ?.props?.children,
        ],
        (_: Array<Record<string, unknown>>, ret?: ReactElement) => {
          const container = findInReactTree(
            ret,
            (x: any) =>
              Array.isArray(x?.props?.children) &&
              x?.props?.className?.includes(appDetailsClasses.InnerContainer),
          );

          if (typeof container !== "object") return ret;
          // Insert as the first child so the zero-height anchor shares the
          // InnerContainer's top edge — PatchWrapper walks from there to the
          // hero capsule and floats the bar over it (see patch-wrapper.tsx).
          container.props.children.splice(0, 0, createElement(PatchWrapper, null));

          return ret;
        },
      );

      afterPatch(routeProps, "renderFunc", patchHandler);

      return tree;
    },
  );

  // Register the game-stop listener ONCE at plugin load (not inside the panel,
  // which unmounts when the QAM closes). On a stop, let the backend decide
  // whether a forced-close fallback backup is needed.
  const sub = SteamClient.GameSessions.RegisterForAppLifetimeNotifications(
    (n) => {
      if (!n.bRunning) {
        // Spool's non-Steam shortcut appids are `crc32(...) | 0x80000000`, so
        // the high bit is set. Steam surfaces those through `unAppID` as a
        // *signed* int32 (e.g. -105595925 instead of 4189371371), which would
        // never match the unsigned `steam_appid` in active-session.json. `>>> 0`
        // coerces it back to the unsigned 32-bit value the backend compares.
        const appid = n.unAppID >>> 0;
        if (appid !== 0) {
          void onAppStop(appid);
        }
      }
    },
  );

  // Drive the game-page backup badge's spinner. `started`/`finished` carry the
  // appid and always fire (independent of the notify setting); the separate
  // `toast` event is gated by that setting on the Python side.
  const onBackupStarted = (appid: number) => backupStarted(appid);
  const onBackupFinished = (appid: number) => backupFinished(appid);
  const onBackupToast = (game: string, ok: boolean, reason: string) => {
    toaster.toast({
      title: "Spool",
      body: ok ? `Backed up ${game} ✓` : `Backup failed: ${reason || "unknown error"}`,
    });
  };
  addEventListener("spool_backup_started", onBackupStarted);
  addEventListener("spool_backup_finished", onBackupFinished);
  addEventListener("spool_backup_toast", onBackupToast);

  return {
    name: "Spool",
    titleView: <div className={staticClasses.Title}>Spool</div>,
    content: <Content />,
    icon: <FaFloppyDisk />,
    onDismount() {
      sub.unregister();
      removeEventListener("spool_backup_started", onBackupStarted);
      removeEventListener("spool_backup_finished", onBackupFinished);
      removeEventListener("spool_backup_toast", onBackupToast);
      routerHook.removeRoute(SPOOL_LAN_ROUTE);
      routerHook.removeRoute(SPOOL_LAN_PEER_ROUTE);
      routerHook.removePatch("/library/app/:appid", playtimePatch);
    },
  };
});
