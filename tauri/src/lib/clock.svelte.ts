/**
 * Shared, lazily-started reactive clock for periodic relative-time labels.
 *
 * Relative-time formatters like `relDate` ("just now" / "5m ago") are pure
 * functions of the current time, so a label rendered once goes stale until
 * something else forces the view to re-render. Reading `clock.now` inside the
 * formatter turns that read into a reactive dependency: every tick re-runs the
 * surrounding Svelte effect / `$derived`, so the label advances on its own.
 *
 * The interval only starts the first time `now` is read (lazy), so SSR and
 * unit tests don't pay for a timer nobody observes. One tick a minute is
 * enough for relDate's granularity (its smallest live step is "just now" →
 * "1m ago"); the toast store keeps its own 1 s ticker for second-level chips.
 */
class Clock {
  /** Bumps every minute. Read via the `now` getter to subscribe. */
  #tick = $state(0);
  #started = false;

  /**
   * Current tick. Reading this inside a Svelte effect / `$derived` (e.g.
   * through `relDate`) registers a dependency, so the reader re-runs each
   * tick. Outside a reactive context it's a plain number read — harmless.
   */
  get now(): number {
    this.#ensureTicker();
    return this.#tick;
  }

  #ensureTicker(): void {
    if (this.#started || typeof window === 'undefined') return; // SSR/test-safe
    this.#started = true;
    setInterval(() => this.#tick++, 60_000);
  }
}

export const clock = new Clock();
