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
import { SPOOL_ROUTE, SPOOL_GAME_ROUTE, SPOOL_LAN_ROUTE, SPOOL_LAN_PEER_ROUTE, SPOOL_LAN_GAME_ROUTE } from "./constants";
import { onAppStop } from "./api/callables";
import { Content } from "./components/content";
import { GameDetailPage } from "./components/game-detail-panel";
import { LanPage } from "./components/lan-view";
import { PeerGamesPage } from "./components/peer-games";
import { PeerGameDetailPage } from "./components/peer-game-detail-panel";
import { PatchWrapper } from "./components/patch/patch-wrapper";
import { SpoolPage } from "./components/spool-page";

export default definePlugin(() => {
  // Register the full-screen route (Library | LAN). The QAM "Browse Library"
  // button navigates to it; we remove it on dismount to avoid duplicate
  // patches across hot-reloads.
  // `/spool` must be exact, otherwise it prefix-matches `/spool/game/:id` and
  // shadows the detail page (first matching <Route> in the Switch wins).
  routerHook.addRoute(SPOOL_ROUTE, SpoolPage, { exact: true });
  routerHook.addRoute(SPOOL_GAME_ROUTE, GameDetailPage);
  routerHook.addRoute(SPOOL_LAN_ROUTE, LanPage, { exact: true });
  routerHook.addRoute(SPOOL_LAN_PEER_ROUTE, PeerGamesPage, { exact: true });
  routerHook.addRoute(SPOOL_LAN_GAME_ROUTE, PeerGameDetailPage);

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
          container.props.children.splice(1, 0, createElement(PatchWrapper, null));

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
        void onAppStop(n.unAppID >>> 0);
      }
    },
  );

  const onBackupFinished = (game: string, ok: boolean, reason: string) => {
    toaster.toast({
      title: "Spool",
      body: ok ? `Backed up ${game} ✓` : `Backup failed: ${reason || "unknown error"}`,
    });
  };
  addEventListener("spool_backup_finished", onBackupFinished);

  return {
    name: "Spool",
    titleView: <div className={staticClasses.Title}>Spool</div>,
    content: <Content />,
    icon: <FaFloppyDisk />,
    onDismount() {
      sub.unregister();
      removeEventListener("spool_backup_finished", onBackupFinished);
      routerHook.removeRoute(SPOOL_ROUTE);
      routerHook.removeRoute(SPOOL_GAME_ROUTE);
      routerHook.removeRoute(SPOOL_LAN_ROUTE);
      routerHook.removeRoute(SPOOL_LAN_PEER_ROUTE);
      routerHook.removeRoute(SPOOL_LAN_GAME_ROUTE);
      routerHook.removePatch("/library/app/:appid", playtimePatch);
    },
  };
});
