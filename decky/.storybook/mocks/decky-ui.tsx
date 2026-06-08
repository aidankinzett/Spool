// Storybook mock for "@decky/ui".
//
// The real @decky/ui components resolve to Steam's internal React components
// at runtime (via findModuleChild), so they can't render outside Steam. These
// are thin DOM shims with the same prop surface the plugin uses, styled to
// roughly evoke Game Mode so layout/state regressions are visible. They are
// NOT pixel-faithful to Steam — focus rings, gamepad nav, and exact theming
// differ. Treat Storybook as a layout/prop harness, not a visual oracle.
import React from "react";
import { createRoot } from "react-dom/client";

type AnyProps = Record<string, any>;

// Focusable — a controller-focusable container. `onActivate` fires on the
// gamepad A button in Steam; here it maps to click and Enter.
export const Focusable = React.forwardRef<HTMLDivElement, AnyProps>(
  function Focusable({ children, onActivate, onClick, style, ...rest }, ref) {
    const activate = (e: React.SyntheticEvent) => {
      onClick?.(e);
      onActivate?.(e);
    };
    return (
      <div
        ref={ref}
        tabIndex={onActivate || onClick ? 0 : -1}
        onClick={activate}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") activate(e);
        }}
        style={{ outline: "none", ...style }}
        {...rest}
      >
        {children}
      </div>
    );
  },
);

// DialogButton — the standard Game-Mode "Secondary" action button. Metrics
// read from live Game Mode via the CEF debugger: bg #23262e, padding 10px 24px,
// 2px radius, 160px min-width, white text at 16px, 0.4 opacity when disabled.
//
// `width: 100%` is part of SteamUI's real `_DialogLayout` class, so a bare
// DialogButton fills its container on-device. Reproduce it here (before
// `...style`, so a caller's `width` still wins) — otherwise the mock renders a
// content-sized button that Storybook makes look fine while it overflows its
// row on the Deck. Callers that want it content-sized must set `width: "auto"`.
export function DialogButton({ children, style, disabled, ...rest }: AnyProps) {
  return (
    <button
      disabled={disabled}
      style={{
        background: "rgb(35, 38, 46)",
        color: "#fff",
        border: "none",
        borderRadius: "2px",
        padding: "10px 24px",
        width: "100%",
        minWidth: "160px",
        fontSize: "16px",
        cursor: disabled ? "default" : "pointer",
        opacity: disabled ? 0.4 : 1,
        ...style,
      }}
      {...rest}
    >
      {children}
    </button>
  );
}

// ButtonItem — a Field row with a button. Metrics read from live Game Mode via
// the CEF debugger. Two layouts:
//   - layout="inline": label on the LEFT (flex 1, muted #67707b @16px), a
//     DialogButton holding `children` on the RIGHT (flex 0 0 auto, 160px
//     min-width). This is the QAM "Add to Steam" list row.
//   - layout="below" (default): a full-width button whose content is `children`,
//     below the label/description if any.
//
// Field row: flex row, padding 10px 16px, gap 12px, 2px radius. (Decky also
// applies margin 0 -16px to bleed to the QAM edge — container-specific, omitted.)
const FIELD_LABEL_COLOR = "rgb(103, 112, 123)"; // #67707b — Decky's muted label

export function ButtonItem({
  children,
  onClick,
  disabled,
  label,
  description,
  layout,
  bottomSeparator,
  ...rest
}: AnyProps) {
  const rowBorder =
    bottomSeparator !== "none" ? "1px solid rgba(255,255,255,0.08)" : "none";

  const labelBlock = (label || description) && (
    <div style={{ display: "flex", flexDirection: "column", minWidth: 0, flex: "1 1 auto" }}>
      {label && (
        <span
          style={{
            fontSize: "16px",
            color: FIELD_LABEL_COLOR,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {label}
        </span>
      )}
      {description && (
        <span style={{ color: FIELD_LABEL_COLOR, opacity: 0.7, fontSize: "0.8rem" }}>{description}</span>
      )}
    </div>
  );

  if (layout === "inline") {
    // Name left (flex 1), DialogButton right (flex 0 0 auto). Matches the Deck.
    return (
      <div
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
          padding: "10px 16px",
          borderRadius: 2,
          borderBottom: rowBorder,
        }}
        {...rest}
      >
        {labelBlock}
        <DialogButton
          onClick={onClick}
          disabled={disabled}
          style={{ width: "auto", flex: "0 0 auto", display: "inline-flex", alignItems: "center", justifyContent: "center" }}
        >
          {children}
        </DialogButton>
      </div>
    );
  }

  // layout="below" / default — full-width button.
  return (
    <div style={{ padding: "10px 16px", borderRadius: 2, borderBottom: rowBorder }}>
      {labelBlock}
      <DialogButton
        onClick={onClick}
        disabled={disabled}
        style={{ width: "100%", marginTop: label || description ? 6 : 0 }}
        {...rest}
      >
        {children}
      </DialogButton>
    </div>
  );
}

// PanelSectionRow / PanelSection — layout wrappers.
export function PanelSectionRow({ children }: AnyProps) {
  return <div style={{ padding: "4px 0" }}>{children}</div>;
}

export function PanelSection({ title, children }: AnyProps) {
  return (
    <div style={{ padding: "8px 0" }}>
      {title && (
        <div style={{ fontSize: "0.75rem", textTransform: "uppercase", opacity: 0.5, padding: "0 0 4px" }}>
          {title}
        </div>
      )}
      {children}
    </div>
  );
}

// TextField — label/description + text input. onChange receives a DOM event
// (the plugin reads e.target.value), onBlur commits.
export function TextField({ label, description, value, onChange, onBlur, ...rest }: AnyProps) {
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: 4, padding: "6px 0" }}>
      {label && <span style={{ fontSize: "0.9rem" }}>{label}</span>}
      {description && <span style={{ opacity: 0.55, fontSize: "0.8rem" }}>{description}</span>}
      <input
        value={value ?? ""}
        onChange={onChange}
        onBlur={onBlur}
        style={{
          background: "rgba(0,0,0,0.3)",
          color: "#fff",
          border: "1px solid rgba(255,255,255,0.15)",
          borderRadius: 3,
          padding: "8px 10px",
          fontSize: "0.9rem",
        }}
        {...rest}
      />
    </label>
  );
}

// ToggleField — label + switch row.
export function ToggleField({ label, description, checked, onChange, disabled }: AnyProps) {
  return (
    <label
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: "1rem",
        padding: "8px 0",
        opacity: disabled ? 0.5 : 1,
      }}
    >
      <div style={{ display: "flex", flexDirection: "column" }}>
        <span>{label}</span>
        {description && <span style={{ opacity: 0.55, fontSize: "0.8rem" }}>{description}</span>}
      </div>
      <input
        type="checkbox"
        checked={!!checked}
        disabled={disabled}
        onChange={(e) => onChange?.(e.target.checked)}
      />
    </label>
  );
}

// ModalRoot — centered dialog panel. The real one is rendered by showModal;
// in stories we render it directly inside a backdrop.
export function ModalRoot({ children, closeModal }: AnyProps) {
  return (
    <div
      style={{
        background: "#23262e",
        color: "#fff",
        borderRadius: "8px",
        padding: "1.5rem",
        width: "min(560px, 90vw)",
        boxShadow: "0 10px 40px rgba(0,0,0,0.6)",
      }}
      data-close-modal={typeof closeModal === "function" ? "available" : "none"}
    >
      {children}
    </div>
  );
}

// --- Overlay rendering for showModal / showContextMenu --------------------
// The real Decky versions mount into Steam's portal layer. Here we mount into a
// detached root on document.body via react-dom so modals and context menus
// opened from a story (e.g. the BadgeMenuButton ⋮ menu) are actually
// interactive, not just console logs.
function mountOverlay(render: (close: () => void) => React.ReactNode) {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = createRoot(host);
  const close = () => {
    root.unmount();
    host.remove();
  };
  root.render(render(close) as React.ReactElement);
  return close;
}

// showModal — renders the node in a centered backdrop, injecting `closeModal`
// the way Decky does. Clicking the backdrop dismisses it.
export function showModal(node: React.ReactElement<any>) {
  const close = mountOverlay((closeModal) => (
    <div
      onClick={(e) => {
        if (e.target === e.currentTarget) closeModal();
      }}
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.5)",
        zIndex: 9999,
      }}
    >
      {React.cloneElement(node, { closeModal })}
    </div>
  ));
  return { Close: close };
}

// showContextMenu — renders the menu anchored to the element that opened it,
// dismissing on an outside click. Mirrors how Decky positions the menu near
// the trigger.
export function showContextMenu(node: React.ReactElement<any>, anchor?: Element) {
  const rect = anchor?.getBoundingClientRect();
  mountOverlay((close) => (
    <div
      onClick={close}
      style={{ position: "fixed", inset: 0, zIndex: 9999 }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          position: "absolute",
          top: rect ? rect.bottom + 4 : 80,
          left: rect ? Math.max(8, rect.right - 220) : 80,
          minWidth: 200,
          background: "#1c1f26",
          border: "1px solid rgba(255,255,255,0.1)",
          borderRadius: 6,
          padding: 6,
          boxShadow: "0 8px 30px rgba(0,0,0,0.6)",
        }}
      >
        {React.cloneElement(node, { closeMenu: close })}
      </div>
    </div>
  ));
}

// Menu / MenuItem / MenuSeparator — context-menu primitives. MenuItem calls
// `onSelected` and closes the menu (closeMenu is injected by showContextMenu).
export function Menu({ children }: AnyProps) {
  return <div role="menu">{children}</div>;
}

export function MenuItem({ children, onSelected, tone, closeMenu }: AnyProps) {
  return (
    <button
      role="menuitem"
      onClick={() => {
        onSelected?.();
        closeMenu?.();
      }}
      style={{
        display: "block",
        width: "100%",
        textAlign: "left",
        background: "transparent",
        color: tone === "destructive" ? "#ff7a7a" : "#fff",
        border: "none",
        padding: "8px 10px",
        borderRadius: 4,
        cursor: "pointer",
        fontSize: "0.9rem",
      }}
    >
      {children}
    </button>
  );
}

export function MenuSeparator() {
  return <hr style={{ border: 0, borderTop: "1px solid rgba(255,255,255,0.1)", margin: "4px 0" }} />;
}

// ConfirmModal — title/description + OK/Cancel. Used by the delete flow.
export function ConfirmModal({
  strTitle,
  strDescription,
  strOKButtonText = "OK",
  strCancelButtonText = "Cancel",
  bDestructiveWarning,
  onOK,
  onCancel,
  closeModal,
}: AnyProps) {
  return (
    <ModalRoot closeModal={closeModal}>
      <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.2rem" }}>{strTitle}</h2>
      <div style={{ opacity: 0.75, fontSize: "0.9rem", whiteSpace: "pre-wrap", marginBottom: "1rem" }}>
        {strDescription}
      </div>
      <div style={{ display: "flex", gap: "0.75rem" }}>
        <DialogButton
          style={{ flex: 1, background: bDestructiveWarning ? "#a13b2f" : undefined }}
          onClick={() => {
            onOK?.();
            closeModal?.();
          }}
        >
          {strOKButtonText}
        </DialogButton>
        <DialogButton
          style={{ flex: 1 }}
          onClick={() => {
            onCancel?.();
            closeModal?.();
          }}
        >
          {strCancelButtonText}
        </DialogButton>
      </div>
    </ModalRoot>
  );
}

// Navigation / ReactRouter — no-op routing surface.
export const Navigation = {
  Navigate: (path: string) => console.log("[Navigation.Navigate]", path),
  NavigateBack: () => console.log("[Navigation.NavigateBack]"),
  NavigateToLibraryTab: () => console.log("[Navigation.NavigateToLibraryTab]"),
  CloseSideMenus: () => {},
};

// Route params surfaced to components via lib/steam's `useParams`. lib/steam
// finds the param hook by scanning ReactRouter's values for a function whose
// source matches `/return (\w)\?\1\.params:{}/`, so `useParamsImpl` carries a
// crafted toString to be picked. A story sets the active params with
// setRouteParams() before render (e.g. { peerAddr, peerPort, gameId }).
let routeParams: Record<string, string> = {};
export function setRouteParams(p: Record<string, string>): void {
  routeParams = p;
}
const useParamsImpl = function useParams() {
  return routeParams;
};
useParamsImpl.toString = () => "function(n){return n?n.params:{}}";

export const ReactRouter = {
  RouterContext: React.createContext({}),
  useParams: useParamsImpl,
};

// Steam minified CSS class objects — every property access returns a stable
// dummy class string so reads never come back undefined.
const classProxy = new Proxy(
  {},
  { get: (_t, key) => `mock-${String(key)}` },
) as Record<string, string>;
export const appDetailsClasses = classProxy;
export const appDetailsHeaderClasses = classProxy;

// Misc primitives the plugin may import.
export function Spinner({ style }: AnyProps) {
  return <div style={{ width: 18, height: 18, ...style }}>⏳</div>;
}
export const Field = ({ label, children }: AnyProps) => (
  <div style={{ display: "flex", justifyContent: "space-between", padding: "6px 0" }}>
    <span>{label}</span>
    {children}
  </div>
);
