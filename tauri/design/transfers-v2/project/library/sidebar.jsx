/* Left pane — Steam-style compact list:
   - Search + filter pills at top
   - Scrollable list with small cover thumb + name + meta
   - Footer with "Add Game" CTA */

const { useState: useStateSB, useMemo: useMemoSB } = React;

/* ---------- Cover Art Thumb (placeholder using gradient + initials) ---------- */
function CoverThumb({ game, size = "sm" }) {
  const w = { sm: 30, md: 44, lg: 64, xl: 96 }[size];
  const h = Math.round(w * 1.45);
  const fontSize = { sm: 9, md: 12, lg: 16, xl: 22 }[size];

  // Initials from name — first 2 distinct words, fallback to 2 chars
  const words = game.name.replace(/[:.]/g, " ").split(/\s+/).filter(Boolean);
  const initials = words.length >= 2
    ? (words[0][0] + words[1][0]).toUpperCase()
    : game.name.slice(0, 2).toUpperCase();

  return (
    <div style={{
      width: w,
      height: h,
      borderRadius: 3,
      background: `linear-gradient(155deg, ${game.art.from} 0%, ${game.art.to} 100%)`,
      position: "relative",
      overflow: "hidden",
      flexShrink: 0,
      border: "1px solid rgba(255,255,255,0.06)",
      boxShadow: "0 1px 2px rgba(0,0,0,0.3)",
    }}>
      {/* accent shape — a glowing circle in upper-right giving each cover its own identity */}
      <div style={{
        position: "absolute",
        top: -h * 0.2,
        right: -w * 0.3,
        width: w * 0.9,
        height: w * 0.9,
        borderRadius: "50%",
        background: `radial-gradient(circle, ${game.art.accent}80, transparent 70%)`,
      }}/>
      {/* lower scrim for "title" */}
      <div style={{
        position: "absolute",
        inset: 0,
        background: "linear-gradient(180deg, transparent 40%, rgba(0,0,0,0.35) 100%)",
      }}/>
      {/* initials/text — sized for thumb */}
      <div style={{
        position: "absolute",
        bottom: size === "sm" ? 2 : 4,
        left: size === "sm" ? 3 : 6,
        right: size === "sm" ? 3 : 6,
        color: "#fff",
        fontSize,
        fontWeight: 700,
        letterSpacing: "-0.02em",
        lineHeight: 1,
        textShadow: "0 1px 2px rgba(0,0,0,0.6)",
        whiteSpace: "nowrap",
        overflow: "hidden",
        textOverflow: "ellipsis",
      }}>
        {size === "sm" ? initials : game.short || game.name}
      </div>
    </div>
  );
}

/* ---------- Sidebar list item ---------- */
function GameRow({ game, active, onSelect, accent, density }) {
  const [hover, setHover] = useStateSB(false);
  const padY = density === "compact" ? 5 : 8;

  const meta = (() => {
    if (game.lastPlayed) {
      return relativeDate(game.lastPlayed);
    }
    return "Never played";
  })();

  return (
    <button
      onClick={onSelect}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        position: "relative",
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: `${padY}px 12px ${padY}px 14px`,
        background: active
          ? "rgba(255,255,255,0.06)"
          : hover ? "rgba(255,255,255,0.025)" : "transparent",
        border: "none",
        borderRadius: 4,
        cursor: "pointer",
        textAlign: "left",
        width: "100%",
        color: "#fff",
        fontFamily: "inherit",
        transition: "background 100ms ease",
      }}
    >
      {active && (
        <span style={{
          position: "absolute",
          left: 0,
          top: "50%",
          transform: "translateY(-50%)",
          width: 3,
          height: 22,
          background: accent,
          borderRadius: 2,
        }}/>
      )}
      <CoverThumb game={game} size="sm" />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: 13,
          fontWeight: active ? 500 : 400,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          lineHeight: 1.3,
        }}>{game.name}</div>
        <div style={{
          fontSize: 11,
          color: "rgba(255,255,255,0.5)",
          marginTop: 1,
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}>
          <span style={{ display: "flex" }}><IconClock size={10} /></span>
          {meta}
          {game.lanShared && (
            <span style={{
              color: "#7ee2a4", display: "flex", marginLeft: 2,
            }} title="Shared on LAN">
              <IconWifi size={10} />
            </span>
          )}
        </div>
      </div>
    </button>
  );
}

/* ---------- Sidebar ---------- */
function LibrarySidebar({
  games, activeId, onSelect, accent, density,
  query, setQuery, filter, setFilter, sort, setSort,
}) {
  return (
    <aside style={{
      width: 320,
      flexShrink: 0,
      display: "flex",
      flexDirection: "column",
      borderRight: "1px solid rgba(255,255,255,0.05)",
      background: "rgba(0,0,0,0.18)",
      minHeight: 0,
    }}>
      {/* Search */}
      <div style={{ padding: "14px 12px 8px" }}>
        <TextField
          value={query}
          onChange={setQuery}
          placeholder="Search library…"
          accent={accent}
          prefix={<IconSearch size={13} />}
        />
      </div>

      {/* Filter pills + sort */}
      <div style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "0 12px 10px",
      }}>
        {[
          { id: "all", label: "All" },
          { id: "recent", label: "Recent" },
          { id: "shared", label: "On LAN" },
          { id: "unplayed", label: "Unplayed" },
        ].map((f) => (
          <FilterChip
            key={f.id}
            label={f.label}
            active={filter === f.id}
            onClick={() => setFilter(f.id)}
            accent={accent}
          />
        ))}
        <div style={{ flex: 1 }} />
        <SortMenu sort={sort} setSort={setSort} accent={accent} />
      </div>

      {/* List */}
      <div style={{
        flex: 1,
        minHeight: 0,
        overflowY: "auto",
        padding: "0 8px 12px 8px",
      }}>
        {games.length === 0 ? (
          <div style={{
            padding: "24px 12px",
            fontSize: 12,
            color: "rgba(255,255,255,0.45)",
            textAlign: "center",
          }}>
            No games match.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
            {games.map((g) => (
              <GameRow
                key={g.id}
                game={g}
                active={g.id === activeId}
                onSelect={() => onSelect(g.id)}
                accent={accent}
                density={density}
              />
            ))}
          </div>
        )}
      </div>

      {/* Footer */}
      <div style={{
        padding: "10px 12px 12px",
        borderTop: "1px solid rgba(255,255,255,0.05)",
        display: "flex",
        flexDirection: "column",
        gap: 6,
      }}>
        <Button
          variant="primary"
          accent={accent}
          icon={<IconPlus size={13} />}
          fullWidth
        >
          Add Game
        </Button>
        <Button
          variant="ghost"
          accent={accent}
          icon={<IconSearch size={13} />}
          fullWidth
        >
          Browse Games
        </Button>
        <div style={{
          fontSize: 11,
          color: "rgba(255,255,255,0.4)",
          textAlign: "center",
          marginTop: 4,
        }}>
          {games.length} {games.length === 1 ? "game" : "games"} in library
        </div>
      </div>
    </aside>
  );
}

function FilterChip({ label, active, onClick, accent }) {
  const [hover, setHover] = useStateSB(false);
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        padding: "4px 10px",
        height: 24,
        fontSize: 11,
        fontWeight: active ? 500 : 400,
        background: active ? `${accent}26` : hover ? "rgba(255,255,255,0.04)" : "transparent",
        color: active ? accent : "rgba(255,255,255,0.75)",
        border: `1px solid ${active ? `${accent}44` : "rgba(255,255,255,0.08)"}`,
        borderRadius: 12,
        cursor: "pointer",
        fontFamily: "inherit",
      }}
    >
      {label}
    </button>
  );
}

function SortMenu({ sort, setSort, accent }) {
  const [open, setOpen] = useStateSB(false);
  const ref = React.useRef(null);
  React.useEffect(() => {
    if (!open) return;
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    document.addEventListener("mousedown", h);
    return () => document.removeEventListener("mousedown", h);
  }, [open]);

  const options = [
    { value: "recent", label: "Recently played" },
    { value: "name", label: "Name (A–Z)" },
    { value: "added", label: "Date added" },
    { value: "playtime", label: "Playtime" },
    { value: "size", label: "Install size" },
  ];

  return (
    <div ref={ref} style={{ position: "relative" }}>
      <button
        onClick={() => setOpen(!open)}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
          padding: "4px 8px",
          height: 24,
          background: open ? "rgba(255,255,255,0.06)" : "transparent",
          color: "rgba(255,255,255,0.7)",
          border: "1px solid rgba(255,255,255,0.08)",
          borderRadius: 4,
          fontFamily: "inherit",
          fontSize: 11,
          cursor: "pointer",
        }}
        title="Sort"
      >
        <IconSortDesc size={12} />
      </button>
      {open && (
        <div style={{
          position: "absolute",
          top: "calc(100% + 4px)",
          right: 0,
          width: 180,
          background: "#2c2c2c",
          border: "1px solid rgba(255,255,255,0.10)",
          borderRadius: 6,
          padding: 4,
          zIndex: 100,
          boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
        }}>
          {options.map((o) => (
            <button
              key={o.value}
              onClick={() => { setSort(o.value); setOpen(false); }}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                width: "100%",
                padding: "7px 10px",
                background: o.value === sort ? "rgba(255,255,255,0.06)" : "transparent",
                border: "none",
                borderRadius: 4,
                color: "#fff",
                fontFamily: "inherit",
                fontSize: 12,
                cursor: "pointer",
                textAlign: "left",
              }}
              onMouseEnter={(e) => { if (o.value !== sort) e.currentTarget.style.background = "rgba(255,255,255,0.03)"; }}
              onMouseLeave={(e) => { if (o.value !== sort) e.currentTarget.style.background = "transparent"; }}
            >
              <span>{o.label}</span>
              {o.value === sort && (
                <span style={{ color: accent, display: "flex" }}>
                  <IconCheck size={13} />
                </span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/* ---------- date helpers ---------- */
function relativeDate(iso) {
  if (!iso) return "Never";
  const t = new Date(iso).getTime();
  const now = new Date("2026-05-25T12:00:00").getTime();
  const diff = (now - t) / 1000;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
  if (diff < 86400 * 7) return `${Math.round(diff / 86400)}d ago`;
  if (diff < 86400 * 30) return `${Math.round(diff / 86400 / 7)}w ago`;
  if (diff < 86400 * 365) return `${Math.round(diff / 86400 / 30)}mo ago`;
  return `${Math.round(diff / 86400 / 365)}y ago`;
}

function formatPlaytime(minutes) {
  if (!minutes) return "Never played";
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m}m`;
  if (h < 100) return `${h}h ${m}m`;
  return `${h}h`;
}

function formatSize(mb) {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${(mb * 1024).toFixed(0)} KB`;
}

function absoluteDate(iso) {
  if (!iso) return "—";
  const d = new Date(iso);
  return d.toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric" });
}

function absoluteDateTime(iso) {
  if (!iso) return "—";
  const d = new Date(iso);
  return d.toLocaleString("en-US", {
    month: "short", day: "numeric",
    hour: "numeric", minute: "2-digit",
  });
}

Object.assign(window, {
  LibrarySidebar, CoverThumb, GameRow, FilterChip,
  relativeDate, formatPlaytime, formatSize, absoluteDate, absoluteDateTime,
});
