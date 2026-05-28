/* Library window — tokens-driven so we can render Desktop and Touch
   variants from the exact same code. Same data, same shapes, same
   visual hierarchy; only sizes change. */

const { useState: useStateLT, useMemo: useMemoLT } = React;

/* ---------------- Cover thumb (token-aware) ---------------- */
function LTCover({ game, size }) {
  const w = size;
  const h = Math.round(w * 1.45);
  const big = w >= 80;
  const words = game.name.replace(/[:.]/g, " ").split(/\s+/).filter(Boolean);
  const initials = words.length >= 2
    ? (words[0][0] + words[1][0]).toUpperCase()
    : game.name.slice(0, 2).toUpperCase();

  return (
    <div style={{
      width: w, height: h, borderRadius: 4,
      background: `linear-gradient(155deg, ${game.art.from} 0%, ${game.art.to} 100%)`,
      position: "relative", overflow: "hidden", flexShrink: 0,
      border: "1px solid rgba(255,255,255,0.06)",
      boxShadow: "0 1px 3px rgba(0,0,0,0.35)",
    }}>
      <div style={{
        position: "absolute", top: -h * 0.2, right: -w * 0.3,
        width: w * 0.9, height: w * 0.9, borderRadius: "50%",
        background: `radial-gradient(circle, ${game.art.accent}80, transparent 70%)`,
      }}/>
      <div style={{
        position: "absolute", inset: 0,
        background: "linear-gradient(180deg, transparent 40%, rgba(0,0,0,0.4) 100%)",
      }}/>
      <div style={{
        position: "absolute", bottom: big ? 6 : 3, left: big ? 8 : 4, right: big ? 8 : 4,
        color: "#fff", fontWeight: 700,
        fontSize: big ? Math.round(w * 0.18) : Math.round(w * 0.30),
        letterSpacing: "-0.02em", lineHeight: 1,
        textShadow: "0 1px 2px rgba(0,0,0,0.6)",
        whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
      }}>
        {big ? (game.short || game.name) : initials}
      </div>
    </div>
  );
}

/* ---------------- Sidebar row ---------------- */
function LTRow({ game, active, accent, t, onSelect }) {
  const [hover, setHover] = useStateLT(false);
  return (
    <button
      onClick={onSelect}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        position: "relative",
        display: "flex", alignItems: "center", gap: t.rowGap,
        padding: `${t.rowPadY}px ${t.cardPad - 4}px ${t.rowPadY}px ${t.cardPad - 2}px`,
        background: active ? "rgba(255,255,255,0.07)"
                  : hover ? "rgba(255,255,255,0.03)" : "transparent",
        border: "none", borderRadius: 6, cursor: "pointer",
        textAlign: "left", width: "100%", color: "#fff",
        fontFamily: "inherit", transition: "background 100ms ease",
        minHeight: t.rowH,
      }}
    >
      {active && (
        <span style={{
          position: "absolute", left: 0, top: "50%",
          transform: "translateY(-50%)",
          width: 3, height: t.rowH * 0.5,
          background: accent, borderRadius: 2,
        }}/>
      )}
      <LTCover game={game} size={t.thumbSm} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: t.base, fontWeight: active ? 500 : 400,
          overflow: "hidden", textOverflow: "ellipsis",
          whiteSpace: "nowrap", lineHeight: 1.3,
        }}>{game.name}</div>
        <div style={{
          fontSize: t.sm, color: "rgba(255,255,255,0.5)",
          marginTop: 2,
          display: "flex", alignItems: "center", gap: 6,
        }}>
          <IconClock size={Math.round(t.sm * 0.95)} />
          {game.lastPlayed ? relativeDate(game.lastPlayed) : "Never played"}
          {game.lanShared && (
            <span style={{ color: "#7ee2a4", display: "flex", marginLeft: 2 }}>
              <IconWifi size={Math.round(t.sm * 0.95)} />
            </span>
          )}
        </div>
      </div>
    </button>
  );
}

/* ---------------- Filter chip ---------------- */
function LTChip({ label, active, accent, t, onClick }) {
  return (
    <button
      onClick={onClick}
      style={{
        padding: `0 ${t.chipPadX}px`,
        height: t.chipH,
        fontSize: t.sm + 0.5,
        fontWeight: active ? 500 : 400,
        background: active ? `${accent}26` : "transparent",
        color: active ? accent : "rgba(255,255,255,0.75)",
        border: `1px solid ${active ? `${accent}44` : "rgba(255,255,255,0.08)"}`,
        borderRadius: t.chipH / 2,
        cursor: "pointer", fontFamily: "inherit",
        whiteSpace: "nowrap",
      }}
    >{label}</button>
  );
}

/* ---------------- Sidebar ---------------- */
function LTSidebar({ games, activeId, accent, t, onSelect, query, setQuery, filter, setFilter }) {
  return (
    <aside style={{
      width: t.sidebarW, flexShrink: 0,
      display: "flex", flexDirection: "column",
      borderRight: "1px solid rgba(255,255,255,0.05)",
      background: "rgba(0,0,0,0.18)", minHeight: 0,
    }}>
      <div style={{ padding: `${t.cardPad - 4}px ${t.cardPad - 6}px ${t.rowPadY}px` }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 10,
          height: t.btnH + (t.pointer === "coarse" ? 4 : 0),
          background: "rgba(255,255,255,0.04)",
          border: "1px solid rgba(255,255,255,0.10)",
          borderRadius: 6, padding: `0 ${t.chipPadX}px`,
        }}>
          <IconSearch size={Math.round(t.base * 1.05)} />
          <input
            value={query} onChange={(e) => setQuery(e.target.value)}
            placeholder="Search library…"
            style={{
              flex: 1, background: "transparent", border: "none", outline: "none",
              color: "#fff", fontSize: t.base, fontFamily: "inherit",
            }}
          />
        </div>
      </div>

      <div style={{
        display: "flex", alignItems: "center", gap: 6,
        padding: `0 ${t.cardPad - 6}px ${t.rowPadY + 2}px`,
        flexWrap: "wrap",
      }}>
        {[
          { id: "all", label: "All" },
          { id: "recent", label: "Recent" },
          { id: "shared", label: "On LAN" },
          { id: "unplayed", label: "Unplayed" },
        ].map(f => (
          <LTChip key={f.id} {...f}
            active={filter === f.id} onClick={() => setFilter(f.id)}
            accent={accent} t={t} />
        ))}
      </div>

      <div style={{
        flex: 1, minHeight: 0, overflowY: "auto",
        padding: `0 ${t.rowPadY}px ${t.cardPad - 4}px`,
      }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          {games.map(g => (
            <LTRow key={g.id} game={g}
              active={g.id === activeId} accent={accent} t={t}
              onSelect={() => onSelect(g.id)} />
          ))}
        </div>
      </div>

      <div style={{
        padding: `${t.rowPadY + 2}px ${t.cardPad - 6}px ${t.cardPad - 4}px`,
        borderTop: "1px solid rgba(255,255,255,0.05)",
        display: "flex", flexDirection: "column", gap: 8,
      }}>
        <LTButton variant="primary" accent={accent} t={t} fullWidth
          icon={<IconPlus size={Math.round(t.base * 1.1)} />}>Add Game</LTButton>
        <LTButton variant="ghost" accent={accent} t={t} fullWidth
          icon={<IconSearch size={Math.round(t.base * 1.05)} />}>Browse Games</LTButton>
      </div>
    </aside>
  );
}

/* ---------------- Button (tokenized) ---------------- */
function LTButton({ children, variant = "secondary", accent, t, icon, fullWidth, danger, style, size = "md" }) {
  const [hover, setHover] = useStateLT(false);
  const h = size === "primary" ? t.primaryH
          : size === "sm" ? t.btnHsm : t.btnH;
  const palette = {
    primary: { bg: accent, fg: "#000", border: "transparent",
               hover: shade(accent, -8) },
    secondary: { bg: "rgba(255,255,255,0.06)", fg: "#fff",
                 border: "rgba(255,255,255,0.09)",
                 hover: "rgba(255,255,255,0.10)" },
    ghost: { bg: "transparent", fg: "#fff", border: "transparent",
             hover: "rgba(255,255,255,0.06)" },
    danger: { bg: "rgba(255,95,95,0.10)", fg: "#ff8a8a",
              border: "rgba(255,95,95,0.20)",
              hover: "rgba(255,95,95,0.18)" },
  }[variant];
  return (
    <button
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: "inline-flex", alignItems: "center", justifyContent: "center",
        gap: 8, padding: `0 ${Math.round(t.cardPad * 0.8)}px`,
        height: h, minHeight: h,
        width: fullWidth ? "100%" : undefined,
        background: hover ? palette.hover : palette.bg,
        color: palette.fg,
        border: `1px solid ${palette.border}`,
        borderRadius: 6, fontFamily: "inherit",
        fontSize: size === "primary" ? t.h3 : t.base,
        fontWeight: variant === "primary" ? 600 : 400,
        cursor: "pointer", transition: "background 100ms ease",
        whiteSpace: "nowrap",
        boxShadow: variant === "primary"
          ? `0 4px 12px ${accent}33` : "none",
        ...style,
      }}
    >
      {icon}
      {children}
    </button>
  );
}

/* ---------------- Hero ---------------- */
function LTHero({ game, accent, t }) {
  const heroH = t.pointer === "coarse" ? 340 : 300;
  return (
    <div style={{
      position: "relative", height: heroH, overflow: "hidden",
      borderBottom: "1px solid rgba(255,255,255,0.05)",
    }}>
      <div style={{
        position: "absolute", inset: 0,
        background: `linear-gradient(135deg, ${game.art.from} 0%, ${game.art.to} 100%)`,
      }}/>
      <div style={{
        position: "absolute", top: -120, right: -120,
        width: 600, height: 600, borderRadius: "50%",
        background: `radial-gradient(circle, ${game.art.accent}40, transparent 65%)`,
      }}/>
      <div style={{
        position: "absolute", inset: 0, opacity: 0.18,
        backgroundImage: `repeating-linear-gradient(115deg, ${game.art.accent}15 0 2px, transparent 2px 28px)`,
      }}/>
      <div style={{
        position: "absolute", inset: 0,
        background: "linear-gradient(180deg, transparent 30%, rgba(0,0,0,0.55) 100%)",
      }}/>
      <div style={{
        position: "absolute", top: 14, right: 18,
        fontSize: 9,
        fontFamily: `"JetBrains Mono","Cascadia Code",ui-monospace,monospace`,
        color: "rgba(255,255,255,0.30)", letterSpacing: "0.12em",
        textTransform: "uppercase", pointerEvents: "none",
      }}>hero · {game.art.mood}</div>

      <div style={{
        position: "absolute", left: t.pageGutter, bottom: t.pageGutter,
        display: "flex", alignItems: "flex-end", gap: 22,
      }}>
        <LTCover game={game} size={t.thumbXl} />
        <div style={{ marginBottom: 4 }}>
          <div style={{
            fontFamily: "var(--font-display, inherit)",
            fontSize: t.h1, fontWeight: 700,
            letterSpacing: "-0.022em", lineHeight: 1.05,
            textShadow: "0 2px 12px rgba(0,0,0,0.4)",
            maxWidth: 520,
          }}>{game.name}</div>
          <div style={{
            display: "flex", alignItems: "center",
            gap: 12, marginTop: 10,
            fontSize: t.sm + 1, color: "rgba(255,255,255,0.78)",
          }}>
            <span>{game.developer}</span>
            <span style={{ opacity: 0.4 }}>·</span>
            <span>{new Date(game.releaseDate).getFullYear()}</span>
            <span style={{ opacity: 0.4 }}>·</span>
            <div style={{ display: "flex", gap: 6 }}>
              {game.genres.slice(0, 2).map(g => (
                <span key={g} style={{
                  fontSize: t.xs, padding: `2px ${Math.max(8, t.chipPadX - 4)}px`,
                  background: "rgba(255,255,255,0.08)",
                  border: "1px solid rgba(255,255,255,0.08)",
                  borderRadius: 12, color: "rgba(255,255,255,0.85)",
                }}>{g}</span>
              ))}
            </div>
          </div>
        </div>
      </div>

      <div style={{
        position: "absolute", right: t.pageGutter, bottom: t.pageGutter,
        display: "flex", gap: 10, alignItems: "center",
      }}>
        <LTButton variant="primary" accent={accent} t={t} size="primary"
          icon={<IconPlay size={Math.round(t.h3 * 1.05)} />}
          style={{ padding: `0 ${Math.round(t.pageGutter * 0.85)}px`, letterSpacing: "0.01em" }}
        >Play</LTButton>
      </div>
    </div>
  );
}

/* ---------------- Stats strip ---------------- */
function LTStats({ game, t }) {
  const items = [
    { icon: <IconClock size={t.base + 1} />, label: "Last played",
      value: game.lastPlayed ? relativeDate(game.lastPlayed) : "Never",
      sub: game.lastPlayed ? absoluteDateTime(game.lastPlayed) : "—" },
    { icon: <IconGamepad size={t.base + 1} />, label: "Playtime",
      value: formatPlaytime(game.playtime),
      sub: `over ${Math.max(1, Math.ceil(game.playtime / 600))} sessions` },
    { icon: <IconHardDrive size={t.base + 1} />, label: "Install size",
      value: formatSize(game.installSize), sub: "on D:\\" },
    { icon: <IconWifi size={t.base + 1} />, label: "LAN share",
      value: game.lanShared ? "Shared" : "Local only",
      sub: game.lanShared ? "Visible to peers" : "Not shared" },
  ];
  return (
    <div style={{
      display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 0,
      padding: `${t.statsPadY}px ${t.pageGutter}px`,
      borderBottom: "1px solid rgba(255,255,255,0.04)",
    }}>
      {items.map((it, i) => (
        <div key={it.label} style={{
          display: "flex", flexDirection: "column", gap: 4,
          padding: "0 16px",
          borderLeft: i === 0 ? "none" : "1px solid rgba(255,255,255,0.05)",
        }}>
          <div style={{
            display: "flex", alignItems: "center", gap: 6,
            fontSize: t.xs, color: "rgba(255,255,255,0.5)",
            textTransform: "uppercase", letterSpacing: "0.08em", fontWeight: 500,
          }}>{it.icon}{it.label}</div>
          <div style={{ fontSize: t.statSize, fontWeight: 600, letterSpacing: "-0.01em" }}>{it.value}</div>
          <div style={{ fontSize: t.sm, color: "rgba(255,255,255,0.45)" }}>{it.sub}</div>
        </div>
      ))}
    </div>
  );
}

/* ---------------- Action toolbar ---------------- */
function LTActions({ accent, t }) {
  const actions = [
    { icon: <IconFolder size={t.base + 1} />, label: "Open folder" },
    { icon: <IconSparkle size={t.base + 1} />, label: "Armoury Crate" },
    { icon: <IconExternal size={t.base + 1} />, label: "Add to Steam" },
  ];
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 10,
      padding: `${Math.round(t.cardPad * 0.9)}px ${t.pageGutter}px`,
      borderBottom: "1px solid rgba(255,255,255,0.04)",
      flexWrap: "wrap",
    }}>
      {actions.map(a => (
        <LTButton key={a.label} variant="secondary" accent={accent} t={t} icon={a.icon}>
          {a.label}
        </LTButton>
      ))}
      <div style={{ flex: 1 }} />
      <LTButton variant="danger" accent={accent} t={t} icon={<IconTrash size={t.base} />}>Remove</LTButton>
    </div>
  );
}

/* ---------------- About / Saves / Details cards ---------------- */
function LTCard({ title, children, headerAction, t }) {
  return (
    <section style={{
      background: "rgba(255,255,255,0.022)",
      border: "1px solid rgba(255,255,255,0.05)",
      borderRadius: 10, padding: `${t.cardPad - 2}px ${t.cardPad}px`,
    }}>
      <header style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        gap: 12, marginBottom: t.cardPad - 6,
      }}>
        <div style={{
          fontSize: t.h3 - 1, fontWeight: 600, color: "#fff",
          letterSpacing: "-0.005em",
        }}>{title}</div>
        {headerAction}
      </header>
      {children}
    </section>
  );
}

function LTAbout({ game, t }) {
  return (
    <LTCard title="About" t={t}>
      <p style={{
        fontSize: t.base, lineHeight: 1.6,
        color: "rgba(255,255,255,0.85)",
        margin: 0, textWrap: "pretty",
      }}>
        {game.description.slice(0, 220).trimEnd() + "…"}
      </p>
    </LTCard>
  );
}

function LTGameSettings({ game, accent, t, onUpdate }) {
  return (
    <LTCard title="Game settings" t={t}>
      <LTSetting
        icon={<IconShield size={Math.round(t.h3 * 1.1)} />}
        title="Run as administrator"
        description="Always elevate this game on launch."
        accent={accent} t={t}
        control={<LTToggle checked={!!game.runAsAdmin} onChange={(v) => onUpdate({ runAsAdmin: v })} accent={accent} t={t} />}
      />
      <LTSetting
        icon={<IconWifi size={Math.round(t.h3 * 1.1)} />}
        title="Share on LAN"
        description="Make this install available to other Spool devices."
        accent={accent} t={t}
        control={<LTToggle checked={!!game.lanShared} onChange={(v) => onUpdate({ lanShared: v })} accent={accent} t={t} />}
        last
      />
    </LTCard>
  );
}

function LTSetting({ icon, title, description, control, t, last }) {
  const iconBox = t.pointer === "coarse" ? 44 : 32;
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: t.rowGap,
      padding: `${t.rowPadY}px 0`,
      borderBottom: last ? "none" : "1px solid rgba(255,255,255,0.04)",
    }}>
      <div style={{
        width: iconBox, height: iconBox, borderRadius: 6,
        background: "rgba(255,255,255,0.04)",
        border: "1px solid rgba(255,255,255,0.05)",
        display: "flex", alignItems: "center", justifyContent: "center",
        color: "rgba(255,255,255,0.85)", flexShrink: 0,
      }}>{icon}</div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: t.base, fontWeight: 500 }}>{title}</div>
        <div style={{ fontSize: t.sm + 1, color: "rgba(255,255,255,0.6)", marginTop: 2 }}>{description}</div>
      </div>
      {control}
    </div>
  );
}

function LTToggle({ checked, onChange, accent, t }) {
  const w = t.pointer === "coarse" ? 56 : 40;
  const h = t.pointer === "coarse" ? 30 : 20;
  const knob = checked ? h - 8 : h - 10;
  return (
    <button
      onClick={() => onChange(!checked)}
      style={{
        position: "relative", width: w, height: h, borderRadius: h / 2,
        border: checked ? `1px solid ${accent}` : "1px solid rgba(255,255,255,0.55)",
        background: checked ? accent : "transparent",
        cursor: "pointer", padding: 0, flexShrink: 0,
        transition: "background 120ms ease, border-color 120ms ease",
      }}
    >
      <span style={{
        position: "absolute", top: "50%",
        left: checked ? w - knob - 4 : 4,
        width: knob, height: knob, borderRadius: "50%",
        background: checked ? "#000" : "rgba(255,255,255,0.78)",
        transform: "translateY(-50%)",
        transition: "left 140ms cubic-bezier(.2,.9,.3,1.2)",
      }}/>
    </button>
  );
}

/* ---------------- Title bar ---------------- */
function LTTitleBar({ accent, t, peers = 3 }) {
  return (
    <div style={{
      height: t.titleBar,
      display: "flex", alignItems: "center", justifyContent: "space-between",
      padding: "0 0 0 16px", flexShrink: 0, position: "relative", zIndex: 2,
      borderBottom: "1px solid rgba(255,255,255,0.04)",
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <SpoolMark size={t.pointer === "coarse" ? 22 : 18} fg="rgba(255,255,255,0.92)" />
        <SpoolWordmark size={t.pointer === "coarse" ? 16 : 13} fg="rgba(255,255,255,0.92)" />
      </div>

      <div style={{
        position: "absolute", left: "50%", top: "50%",
        transform: "translate(-50%, -50%)",
        display: "flex", alignItems: "center", gap: 8,
        fontSize: t.xs + 0.5, color: "rgba(255,255,255,0.55)",
      }}>
        <span style={{
          display: "inline-flex", alignItems: "center", gap: 6,
          padding: `${t.pointer === "coarse" ? 6 : 3}px 12px`,
          borderRadius: 12,
          background: "rgba(255,255,255,0.04)",
          border: "1px solid rgba(255,255,255,0.06)",
        }}>
          <span style={{
            width: 6, height: 6, borderRadius: 3,
            background: peers > 0 ? "#7ee2a4" : "rgba(255,255,255,0.3)",
          }}/>
          <IconWifi size={t.xs + 1} />
          {peers} peers on LAN
        </span>
      </div>

      <div style={{ display: "flex", alignItems: "center" }}>
        <LTTitleBtn t={t}><IconGeneral size={t.titleBtnIcon} /></LTTitleBtn>
        <LTTitleBtn t={t}><IconMinimize size={t.titleBtnIcon} /></LTTitleBtn>
        <LTTitleBtn t={t}><IconMaximize size={t.titleBtnIcon} /></LTTitleBtn>
        <LTTitleBtn t={t} danger><IconClose size={t.titleBtnIcon} /></LTTitleBtn>
      </div>
    </div>
  );
}

function LTTitleBtn({ children, danger, t }) {
  const [hover, setHover] = useStateLT(false);
  return (
    <button
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        width: t.titleBtnW, height: t.titleBar,
        background: hover ? (danger ? "#c42b1c" : "rgba(255,255,255,0.06)") : "transparent",
        color: hover && danger ? "#fff" : "rgba(255,255,255,0.78)",
        border: "none", cursor: "pointer",
        display: "flex", alignItems: "center", justifyContent: "center",
        transition: "background 100ms ease",
      }}
    >{children}</button>
  );
}

/* ---------------- The window ---------------- */
function LibraryWindow({ tokens, accent = "#4cc2ff", width = 1240, height = 800 }) {
  const t = tokens;
  const [activeId, setActiveId] = useStateLT("hades-2");
  const [query, setQuery] = useStateLT("");
  const [filter, setFilter] = useStateLT("all");
  const [overrides, setOverrides] = useStateLT({});

  const games = window.LIBRARY;
  const liveGames = useMemoLT(
    () => games.map(g => ({ ...g, ...(overrides[g.id] || {}) })),
    [games, overrides]
  );
  const visibleGames = useMemoLT(() => {
    const now = new Date("2026-05-25T12:00:00").getTime();
    let out = liveGames.slice();
    if (filter === "recent")
      out = out.filter(g => g.lastPlayed && (now - new Date(g.lastPlayed).getTime()) < 14 * 86400 * 1000);
    else if (filter === "shared") out = out.filter(g => g.lanShared);
    else if (filter === "unplayed") out = out.filter(g => !g.lastPlayed || g.playtime < 60);
    if (query.trim()) {
      const q = query.toLowerCase();
      out = out.filter(g => g.name.toLowerCase().includes(q));
    }
    out.sort((a, b) => {
      const ta = a.lastPlayed ? new Date(a.lastPlayed).getTime() : 0;
      const tb = b.lastPlayed ? new Date(b.lastPlayed).getTime() : 0;
      return tb - ta;
    });
    return out;
  }, [liveGames, filter, query]);
  const activeGame = useMemoLT(() =>
    liveGames.find(g => g.id === activeId) || liveGames[0],
    [liveGames, activeId]
  );

  return (
    <div style={{
      position: "relative", width, height,
      background: "linear-gradient(180deg, rgba(28,28,28,0.92) 0%, rgba(22,22,22,0.96) 100%)",
      borderRadius: 8,
      border: "1px solid rgba(255,255,255,0.06)",
      boxShadow: "0 20px 60px rgba(0,0,0,0.55), 0 4px 16px rgba(0,0,0,0.4)",
      display: "flex", flexDirection: "column",
      overflow: "hidden", color: "#fff",
      fontFamily: `"Segoe UI Variable Text","Segoe UI Variable","Segoe UI","Inter",-apple-system,sans-serif`,
    }}>
      <div style={{
        position: "absolute", top: -300, left: -200,
        width: 700, height: 700,
        background: `radial-gradient(circle, ${accent}0e, transparent 60%)`,
        pointerEvents: "none",
      }}/>

      <LTTitleBar accent={accent} t={t} />

      <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
        <LTSidebar
          games={visibleGames} activeId={activeGame?.id}
          accent={accent} t={t} onSelect={setActiveId}
          query={query} setQuery={setQuery}
          filter={filter} setFilter={setFilter}
        />
        <div style={{
          flex: 1, minHeight: 0, display: "flex", flexDirection: "column",
          background: "rgba(0,0,0,0.12)",
        }}>
          <LTHero game={activeGame} accent={accent} t={t} />
          <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
            <LTStats game={activeGame} t={t} />
            <LTActions accent={accent} t={t} />
            <div style={{
              padding: `${t.cardPad}px ${t.pageGutter}px ${t.pageGutter}px`,
              display: "flex", flexDirection: "column", gap: t.sectionGap,
            }}>
              <LTAbout game={activeGame} t={t} />
              <LTGameSettings
                game={activeGame} accent={accent} t={t}
                onUpdate={(patch) => setOverrides(o => ({ ...o, [activeGame.id]: { ...(o[activeGame.id] || {}), ...patch } }))}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { LibraryWindow });
