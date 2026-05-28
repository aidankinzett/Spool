/* Right pane — cinematic game detail view.
   - Full-bleed hero with title + Play CTA
   - Stats strip
   - Action toolbar
   - About / Saves / Details cards
   Plus an empty-state "library overview" when no game is selected. */

const { useState: useStateD } = React;

/* ============================== Hero ============================== */
function Hero({ game, accent, onPlay }) {
  return (
    <div style={{
      position: "relative",
      height: 300,
      overflow: "hidden",
      borderBottom: "1px solid rgba(255,255,255,0.05)",
    }}>
      {/* Layered gradient backdrop */}
      <div style={{
        position: "absolute",
        inset: 0,
        background: `linear-gradient(135deg, ${game.art.from} 0%, ${game.art.to} 100%)`,
      }}/>
      {/* Accent radial */}
      <div style={{
        position: "absolute",
        top: -120,
        right: -120,
        width: 600,
        height: 600,
        borderRadius: "50%",
        background: `radial-gradient(circle, ${game.art.accent}40, transparent 65%)`,
      }}/>
      {/* Decorative streaks — simulates art texture */}
      <div style={{
        position: "absolute",
        inset: 0,
        opacity: 0.18,
        backgroundImage: `repeating-linear-gradient(115deg, ${game.art.accent}15 0 2px, transparent 2px 28px)`,
      }}/>
      {/* Bottom-up scrim for legibility */}
      <div style={{
        position: "absolute",
        inset: 0,
        background: "linear-gradient(180deg, transparent 30%, rgba(0,0,0,0.55) 100%)",
      }}/>
      {/* placeholder mark — subtle, signals "drop hero art here" */}
      <div style={{
        position: "absolute",
        top: 14,
        right: 18,
        fontSize: 9,
        fontFamily: `"JetBrains Mono","Cascadia Code",ui-monospace,monospace`,
        color: "rgba(255,255,255,0.30)",
        letterSpacing: "0.12em",
        textTransform: "uppercase",
        pointerEvents: "none",
      }}>
        hero · {game.art.mood}
      </div>

      {/* Cover art card floating bottom-left */}
      <div style={{
        position: "absolute",
        left: 32,
        bottom: 28,
        display: "flex",
        alignItems: "flex-end",
        gap: 20,
      }}>
        <CoverThumb game={game} size="xl" />
        <div style={{ marginBottom: 4 }}>
          <div style={{
            fontFamily: "var(--font-display)",
            fontSize: 32,
            fontWeight: 700,
            letterSpacing: "-0.022em",
            lineHeight: 1.05,
            textShadow: "0 2px 12px rgba(0,0,0,0.4)",
            maxWidth: 520,
          }}>{game.name}</div>
          <div style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            marginTop: 10,
            fontSize: 12,
            color: "rgba(255,255,255,0.78)",
          }}>
            <span>{game.developer}</span>
            <span style={{ opacity: 0.4 }}>·</span>
            <span>{new Date(game.releaseDate).getFullYear()}</span>
            <span style={{ opacity: 0.4 }}>·</span>
            <div style={{ display: "flex", gap: 6 }}>
              {game.genres.slice(0, 3).map((g) => (
                <span key={g} style={{
                  fontSize: 10.5,
                  padding: "2px 8px",
                  background: "rgba(255,255,255,0.08)",
                  border: "1px solid rgba(255,255,255,0.08)",
                  borderRadius: 10,
                  color: "rgba(255,255,255,0.85)",
                  letterSpacing: "0.01em",
                }}>{g}</span>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Play button — bottom right */}
      <div style={{
        position: "absolute",
        right: 28,
        bottom: 28,
        display: "flex",
        gap: 8,
        alignItems: "center",
      }}>
        <PlayButton accent={accent} onClick={onPlay} />
      </div>
    </div>
  );
}

function PlayButton({ accent, onClick }) {
  const [hover, setHover] = useStateD(false);
  return (
    <button
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 10,
        padding: "0 24px",
        height: 46,
        background: hover ? shade(accent, -6) : accent,
        color: "#000",
        border: "none",
        borderRadius: 6,
        fontFamily: "inherit",
        fontSize: 15,
        fontWeight: 600,
        cursor: "pointer",
        letterSpacing: "0.01em",
        boxShadow: hover
          ? `0 6px 18px ${accent}66, 0 0 0 1px ${accent}55`
          : `0 4px 12px ${accent}33`,
        transition: "background 120ms ease, box-shadow 120ms ease",
      }}
    >
      <IconPlay size={16} />
      Play
    </button>
  );
}

function IconButton({ children, title, onClick, danger }) {
  const [hover, setHover] = useStateD(false);
  return (
    <button
      onClick={onClick}
      title={title}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        width: 46,
        height: 46,
        background: hover
          ? (danger ? "rgba(255,95,95,0.18)" : "rgba(255,255,255,0.10)")
          : "rgba(255,255,255,0.06)",
        color: danger && hover ? "#ff8a8a" : "#fff",
        border: "1px solid rgba(255,255,255,0.10)",
        borderRadius: 6,
        cursor: "pointer",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        transition: "background 120ms ease",
      }}
    >
      {children}
    </button>
  );
}

/* ============================== Stats Strip ============================== */
function StatsStrip({ game }) {
  const items = [
    {
      icon: <IconClock size={14} />,
      label: "Last played",
      value: game.lastPlayed ? relativeDate(game.lastPlayed) : "Never",
      sub: game.lastPlayed ? absoluteDateTime(game.lastPlayed) : "—",
    },
    {
      icon: <IconGamepad size={14} />,
      label: "Playtime",
      value: formatPlaytime(game.playtime),
      sub: game.playtime > 0 ? `over ${Math.max(1, Math.ceil(game.playtime / 600))} sessions` : "—",
    },
    {
      icon: <IconHardDrive size={14} />,
      label: "Install size",
      value: formatSize(game.installSize),
      sub: "on D:\\",
    },
    {
      icon: <IconWifi size={14} />,
      label: "LAN share",
      value: game.lanShared ? "Shared" : "Local only",
      sub: game.lanShared ? "Visible to peers" : "Not shared",
    },
  ];

  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: "repeat(4, 1fr)",
      gap: 0,
      padding: "18px 32px",
      borderBottom: "1px solid rgba(255,255,255,0.04)",
    }}>
      {items.map((it, i) => (
        <div
          key={it.label}
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 4,
            padding: "0 16px",
            borderLeft: i === 0 ? "none" : "1px solid rgba(255,255,255,0.05)",
          }}
        >
          <div style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontSize: 10.5,
            color: "rgba(255,255,255,0.5)",
            textTransform: "uppercase",
            letterSpacing: "0.08em",
            fontWeight: 500,
          }}>
            {it.icon}
            {it.label}
          </div>
          <div style={{ fontSize: 17, fontWeight: 600, letterSpacing: "-0.01em" }}>
            {it.value}
          </div>
          <div style={{ fontSize: 11, color: "rgba(255,255,255,0.45)" }}>
            {it.sub}
          </div>
        </div>
      ))}
    </div>
  );
}

/* ============================== Action Toolbar ============================== */
function ActionToolbar({ game, accent }) {
  const actions = [
    { icon: <IconFolder size={14} />, label: "Open folder" },
    { icon: <IconSparkle size={14} />, label: "Armoury Crate" },
    { icon: <IconExternal size={14} />, label: "Add to Steam" },
  ];

  return (
    <div style={{
      display: "flex",
      alignItems: "center",
      gap: 8,
      padding: "16px 32px",
      borderBottom: "1px solid rgba(255,255,255,0.04)",
      flexWrap: "wrap",
    }}>
      {actions.map((a) => (
        <Button key={a.label} variant="secondary" accent={accent} icon={a.icon} size="md">
          {a.label}
        </Button>
      ))}
      <div style={{ flex: 1 }} />
      <Button variant="danger" accent={accent} icon={<IconTrash size={13} />} size="md">
        Remove
      </Button>
    </div>
  );
}

/* ============================== About card ============================== */
function AboutCard({ game }) {
  const [expanded, setExpanded] = useStateD(false);
  const truncated = game.description.length > 220;
  const shown = expanded || !truncated
    ? game.description
    : game.description.slice(0, 220).trimEnd() + "…";

  return (
    <DetailCard title="About">
      <p style={{
        fontSize: 13,
        lineHeight: 1.6,
        color: "rgba(255,255,255,0.85)",
        margin: 0,
        textWrap: "pretty",
      }}>{shown}</p>
      {truncated && (
        <button
          onClick={() => setExpanded(!expanded)}
          style={{
            background: "transparent",
            border: "none",
            color: "rgba(255,255,255,0.55)",
            fontSize: 12,
            fontFamily: "inherit",
            cursor: "pointer",
            padding: 0,
            marginTop: 8,
          }}
        >
          {expanded ? "Show less" : "Read more"}
        </button>
      )}
    </DetailCard>
  );
}

/* ============================== Saves card ============================== */
function SavesCard({ game, accent }) {
  const sb = game.saveBackup;
  const hasBackup = sb.count > 0;

  return (
    <DetailCard
      title="Save management"
      titleAccent={<StatusPill
        kind={hasBackup ? "ok" : "off"}
        text={hasBackup ? `${sb.count} backups` : "No backups yet"}
      />}
      headerAction={
        <div style={{ display: "flex", gap: 6 }}>
          <Button variant="ghost" accent={accent} size="sm" icon={<IconRestore size={12} />}>
            Restore
          </Button>
          <Button variant="secondary" accent={accent} size="sm" icon={<IconSave size={12} />}>
            Back up now
          </Button>
        </div>
      }
    >
      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(3, 1fr)",
        gap: 16,
        marginBottom: hasBackup ? 14 : 0,
      }}>
        <StatCell
          label="Last backup"
          value={hasBackup ? relativeDate(sb.lastBackedUp) : "—"}
          sub={hasBackup ? absoluteDateTime(sb.lastBackedUp) : "Saves back up automatically on game exit."}
        />
        <StatCell
          label="Saves found"
          value={hasBackup ? `${sb.count}` : "0"}
          sub={hasBackup ? "across all profiles" : "Run the game once to detect."}
        />
        <StatCell
          label="Total size"
          value={hasBackup ? formatSize(sb.size) : "—"}
          sub={hasBackup ? "compressed on disk" : ""}
        />
      </div>
      {hasBackup && (
        <div style={{
          padding: "10px 12px",
          background: "rgba(126,226,164,0.06)",
          border: "1px solid rgba(126,226,164,0.15)",
          borderRadius: 4,
          fontSize: 12,
          color: "rgba(255,255,255,0.78)",
          display: "flex",
          alignItems: "center",
          gap: 8,
        }}>
          <span style={{ color: "#7ee2a4", display: "flex" }}>
            <IconCheck size={14} />
          </span>
          Saves restore automatically before launch and back up on exit via Ludusavi.
        </div>
      )}
    </DetailCard>
  );
}

function StatCell({ label, value, sub }) {
  return (
    <div>
      <div style={{
        fontSize: 10.5,
        color: "rgba(255,255,255,0.45)",
        textTransform: "uppercase",
        letterSpacing: "0.08em",
        fontWeight: 500,
        marginBottom: 4,
      }}>{label}</div>
      <div style={{ fontSize: 15, fontWeight: 500 }}>{value}</div>
      {sub && (
        <div style={{ fontSize: 11, color: "rgba(255,255,255,0.42)", marginTop: 2 }}>
          {sub}
        </div>
      )}
    </div>
  );
}

/* ============================== Details card ============================== */
function DetailsCard({ game }) {
  const rows = [
    { label: "Developer", value: game.developer },
    { label: "Publisher", value: game.publisher },
    { label: "Release date", value: absoluteDate(game.releaseDate) },
    { label: "Genres", value: game.genres.join(", ") },
    { label: "Added to library", value: absoluteDate(game.addedAt) },
    { label: "Executable", value: game.executable, mono: true },
    { label: "Install path", value: game.installPath, mono: true, copy: true },
  ];

  return (
    <DetailCard title="Details">
      <div style={{ display: "flex", flexDirection: "column" }}>
        {rows.map((r, i) => (
          <div
            key={r.label}
            style={{
              display: "grid",
              gridTemplateColumns: "140px 1fr",
              gap: 16,
              padding: "10px 0",
              borderBottom: i < rows.length - 1 ? "1px solid rgba(255,255,255,0.04)" : "none",
              alignItems: "center",
            }}
          >
            <div style={{ fontSize: 12, color: "rgba(255,255,255,0.55)" }}>{r.label}</div>
            <div style={{
              fontSize: 12.5,
              color: "rgba(255,255,255,0.92)",
              fontFamily: r.mono ? `"JetBrains Mono","Cascadia Code",ui-monospace,monospace` : "inherit",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: r.mono ? "nowrap" : "normal",
            }}>{r.value}</div>
          </div>
        ))}
      </div>
    </DetailCard>
  );
}

/* ============================== Detail card shell ============================== */
function DetailCard({ title, children, headerAction, titleAccent }) {
  return (
    <section style={{
      background: "rgba(255,255,255,0.022)",
      border: "1px solid rgba(255,255,255,0.05)",
      borderRadius: 8,
      padding: "16px 18px",
    }}>
      <header style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        marginBottom: 12,
      }}>
        <div style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          fontSize: 13,
          fontWeight: 600,
          color: "#fff",
          letterSpacing: "-0.005em",
        }}>
          {title}
          {titleAccent}
        </div>
        {headerAction}
      </header>
      {children}
    </section>
  );
}

/* ============================== Empty State (library overview) ============================== */
function EmptyState({ games, accent }) {
  const totalPlaytime = games.reduce((s, g) => s + g.playtime, 0);
  const totalBackups = games.reduce((s, g) => s + g.saveBackup.count, 0);
  const totalSize = games.reduce((s, g) => s + g.installSize, 0);
  const recent = [...games]
    .filter(g => g.lastPlayed)
    .sort((a, b) => new Date(b.lastPlayed) - new Date(a.lastPlayed))
    .slice(0, 5);

  return (
    <div style={{
      flex: 1,
      minHeight: 0,
      overflowY: "auto",
      padding: "32px 32px 40px",
    }}>
      <div style={{
        fontFamily: "var(--font-display)",
        fontSize: 28,
        fontWeight: 700,
        letterSpacing: "-0.022em",
        marginBottom: 4,
      }}>Your Library</div>
      <div style={{ fontSize: 13, color: "rgba(255,255,255,0.55)", marginBottom: 28 }}>
        Pick a game on the left, or get a quick snapshot of where things stand.
      </div>

      <div style={{
        display: "grid",
        gridTemplateColumns: "repeat(4, 1fr)",
        gap: 12,
        marginBottom: 28,
      }}>
        <OverviewStat label="Games" value={games.length} accent={accent} />
        <OverviewStat label="Total playtime" value={formatPlaytime(totalPlaytime)} accent={accent} />
        <OverviewStat label="Save backups" value={totalBackups} accent={accent} />
        <OverviewStat label="On disk" value={formatSize(totalSize)} accent={accent} />
      </div>

      <DetailCard title="Recently played">
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          {recent.map((g) => (
            <div
              key={g.id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 12,
                padding: "8px 4px",
              }}
            >
              <CoverThumb game={g} size="sm" />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, fontWeight: 500 }}>{g.name}</div>
                <div style={{ fontSize: 11, color: "rgba(255,255,255,0.5)" }}>
                  {relativeDate(g.lastPlayed)} · {formatPlaytime(g.playtime)} played
                </div>
              </div>
            </div>
          ))}
        </div>
      </DetailCard>
    </div>
  );
}

function OverviewStat({ label, value, accent }) {
  return (
    <div style={{
      padding: "14px 16px",
      background: "rgba(255,255,255,0.024)",
      border: "1px solid rgba(255,255,255,0.05)",
      borderRadius: 8,
    }}>
      <div style={{
        fontSize: 10.5,
        color: "rgba(255,255,255,0.5)",
        textTransform: "uppercase",
        letterSpacing: "0.08em",
        fontWeight: 500,
        marginBottom: 6,
      }}>{label}</div>
      <div style={{
        fontSize: 22,
        fontWeight: 700,
        letterSpacing: "-0.015em",
      }}>{value}</div>
    </div>
  );
}

/* ============================== Main detail pane ============================== */
function GameSettingsCard({ game, accent, onUpdate }) {
  return (
    <DetailCard title="Game settings">
      <div style={{ display: "flex", flexDirection: "column" }}>
        <SettingRow
          icon={<IconShield size={16} />}
          title="Run as administrator"
          description="Always elevate this game on launch — needed by anti-cheat or installers writing outside the install dir."
          control={
            <ToggleSwitch
              checked={!!game.runAsAdmin}
              onChange={(v) => onUpdate({ runAsAdmin: v })}
              accent={accent}
            />
          }
        />
        <SettingRow
          icon={<IconWifi size={16} />}
          title="Share on LAN"
          description="Make this install available to other Spool devices on your network."
          control={
            <ToggleSwitch
              checked={!!game.lanShared}
              onChange={(v) => onUpdate({ lanShared: v })}
              accent={accent}
            />
          }
          expanded={game.lanShared}
          expandedContent={
            <div style={{ display: "grid", gridTemplateColumns: "140px 1fr", gap: 12, alignItems: "center" }}>
              <div style={{ fontSize: 12, color: "rgba(255,255,255,0.65)" }}>
                Shared folder
                <div style={{ fontSize: 10.5, color: "rgba(255,255,255,0.4)", marginTop: 1 }}>
                  Defaults to install dir
                </div>
              </div>
              <div style={{ display: "flex", gap: 6 }}>
                <TextField
                  value={game.lanShareFolder || game.installPath}
                  onChange={(v) => onUpdate({ lanShareFolder: v })}
                  placeholder="Root folder shared to peers…"
                  readOnly
                  monospace
                  accent={accent}
                />
                <Button accent={accent} size="md">Browse</Button>
              </div>
            </div>
          }
        />
      </div>
    </DetailCard>
  );
}

function SettingRow({ icon, title, description, control, expanded, expandedContent }) {
  return (
    <div style={{
      borderBottom: expanded && expandedContent ? "1px solid rgba(255,255,255,0.04)" : "none",
    }}>
      <div style={{
        display: "flex",
        alignItems: "center",
        gap: 14,
        padding: "10px 0",
      }}>
        <div style={{
          width: 32,
          height: 32,
          borderRadius: 4,
          background: "rgba(255,255,255,0.04)",
          border: "1px solid rgba(255,255,255,0.05)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "rgba(255,255,255,0.85)",
          flexShrink: 0,
        }}>{icon}</div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 500 }}>{title}</div>
          <div style={{ fontSize: 12, color: "rgba(255,255,255,0.6)", marginTop: 2 }}>
            {description}
          </div>
        </div>
        {control}
      </div>
      {expanded && expandedContent && (
        <div style={{ padding: "4px 0 14px 46px" }}>
          {expandedContent}
        </div>
      )}
    </div>
  );
}

function GameDetail({ game, accent, onUpdate }) {
  return (
    <div style={{
      flex: 1,
      minHeight: 0,
      display: "flex",
      flexDirection: "column",
      background: "rgba(0,0,0,0.12)",
    }}>
      <Hero game={game} accent={accent} />
      <div style={{
        flex: 1,
        minHeight: 0,
        overflowY: "auto",
      }}>
        <StatsStrip game={game} />
        <ActionToolbar game={game} accent={accent} />
        <div style={{
          padding: "20px 32px 32px",
          display: "flex",
          flexDirection: "column",
          gap: 16,
        }}>
          <AboutCard game={game} />
          <GameSettingsCard game={game} accent={accent} onUpdate={onUpdate} />
          <SavesCard game={game} accent={accent} />
          <DetailsCard game={game} />
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  GameDetail, EmptyState, DetailCard,
});
