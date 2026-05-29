/**
 * Display formatters — match the design system's voice (mono, terse,
 * UK-style absolute dates). Centralised so every detail card / sidebar
 * row / hero label uses the same rendering.
 */

/** Relative time like "3d ago", "2h ago", "just now". Returns "—" if null. */
export function relDate(iso: string | null | undefined): string {
  if (!iso) return '—';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '—';
  const diff = (Date.now() - date.getTime()) / 1000;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)}h ago`;
  const days = Math.round(diff / 86400);
  if (days < 7) return `${days}d ago`;
  if (days < 30) return `${Math.round(days / 7)}w ago`;
  return `${Math.round(days / 30)}mo ago`;
}

/** "26 May 2026" — UK format, locale-aware. */
export function absDate(iso: string | null | undefined): string {
  if (!iso) return '—';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '—';
  return date.toLocaleDateString('en-GB', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

/** "26 May 2026, 14:32" — UK format with 24-hour time. */
export function absDateTime(iso: string | null | undefined): string {
  if (!iso) return '—';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '—';
  return date.toLocaleString('en-GB', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/** Playtime in minutes → "5h 23m" / "12m" / "—". */
export function fmtPlaytime(mins: number | null | undefined): string {
  if (!mins) return '—';
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (h === 0) return `${m}m`;
  if (h < 100) return `${h}h ${m}m`;
  return `${h}h`;
}

/** Size in MB → "1.4 GB" or "423.0 MB" or "—". */
export function fmtSize(mb: number | null | undefined): string {
  if (!mb) return '—';
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

/**
 * Bytes-per-second → network-style bitrate: "98.5 Mbps" / "342 Kbps" /
 * "…" when 0. We report in bits-per-second (×8) using decimal scaling
 * (1000, not 1024) to match how ISPs, routers, and Steam display speed —
 * that's the number most people recognise for a network transfer.
 */
export function fmtRate(bytesPerSec: number | null | undefined): string {
  if (!bytesPerSec || bytesPerSec <= 0) return '…';
  const bits = bytesPerSec * 8;
  if (bits < 1000) return `${bits.toFixed(0)} bps`;
  if (bits < 1000 * 1000) return `${(bits / 1000).toFixed(1)} Kbps`;
  if (bits < 1000 * 1000 * 1000) return `${(bits / (1000 * 1000)).toFixed(1)} Mbps`;
  return `${(bits / (1000 * 1000 * 1000)).toFixed(2)} Gbps`;
}

/** Sequential catalog number → "SPL-0042". */
export function fmtCatalog(num: number): string {
  return `SPL-${num.toString().padStart(4, '0')}`;
}
