import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const openUrl = vi.fn<(url: string) => Promise<void>>(() => Promise.resolve());
vi.mock('$lib/api', () => ({ api: { openUrl: (u: string) => openUrl(u) } }));

const { externalUrlFor, startExternalLinks } = await import('$lib/externalLinks');

// The app's origin varies by platform and build (dev server vs. `tauri://`),
// so it is always passed in explicitly — here, the jsdom document's own origin,
// which is what relative hrefs below resolve against.
const ORIGIN = window.location.origin;

function anchor(href: string, attrs: Record<string, string> = {}) {
  const a = document.createElement('a');
  a.setAttribute('href', href);
  for (const [k, v] of Object.entries(attrs)) a.setAttribute(k, v);
  a.textContent = 'link';
  document.body.appendChild(a);
  return a;
}

describe('externalUrlFor', () => {
  it('returns the absolute URL for off-origin http(s) links', () => {
    expect(
      externalUrlFor(
        anchor('https://www.steamgriddb.com/profile/preferences/api'),
        ORIGIN,
      ),
    ).toBe('https://www.steamgriddb.com/profile/preferences/api');
    expect(externalUrlFor(anchor('http://example.com/x'), ORIGIN)).toBe(
      'http://example.com/x',
    );
  });

  it('leaves in-app SvelteKit navigation alone', () => {
    // Relative hrefs resolve against the document, so they are same-origin.
    for (const href of ['/settings', 'edit', '#section', '?tab=cloud']) {
      expect(externalUrlFor(anchor(href), ORIGIN)).toBeNull();
    }
    // Including one written out in full.
    expect(externalUrlFor(anchor(`${ORIGIN}/settings`), ORIGIN)).toBeNull();
  });

  it('ignores non-web schemes', () => {
    for (const href of ['mailto:a@example.com', 'tel:+1234', 'file:///etc/passwd']) {
      expect(externalUrlFor(anchor(href), ORIGIN)).toBeNull();
    }
  });

  it('ignores an anchor with no href', () => {
    const a = document.createElement('a');
    document.body.appendChild(a);
    expect(externalUrlFor(a, ORIGIN)).toBeNull();
  });
});

describe('startExternalLinks', () => {
  let stop: () => void;

  beforeEach(() => {
    openUrl.mockClear();
    document.body.innerHTML = '';
    stop = startExternalLinks();
  });
  afterEach(() => stop());

  const click = (el: Element, init: MouseEventInit = {}) =>
    el.dispatchEvent(
      new MouseEvent('click', { bubbles: true, cancelable: true, ...init }),
    );

  it('opens an external link in the browser and cancels the navigation', () => {
    const a = anchor('https://example.com/docs', { target: '_blank' });
    const ev = new MouseEvent('click', { bubbles: true, cancelable: true });
    a.dispatchEvent(ev);
    expect(openUrl).toHaveBeenCalledWith('https://example.com/docs');
    expect(ev.defaultPrevented).toBe(true);
  });

  it('opens a link clicked through a child element', () => {
    const a = anchor('https://example.com/docs');
    const icon = document.createElement('span');
    a.appendChild(icon);
    click(icon);
    expect(openUrl).toHaveBeenCalledWith('https://example.com/docs');
  });

  it('opens through a container that stops propagation', () => {
    // Modals swallow clicks to keep them off the backdrop; the listener is on
    // the capture phase so it still sees them.
    const box = document.createElement('div');
    box.addEventListener('click', (e) => e.stopPropagation());
    document.body.appendChild(box);
    const a = document.createElement('a');
    a.href = 'https://example.com/docs';
    box.appendChild(a);
    click(a);
    expect(openUrl).toHaveBeenCalledWith('https://example.com/docs');
  });

  it('opens on a modifier-click too — there are no tabs to open in', () => {
    const a = anchor('https://example.com/docs');
    click(a, { ctrlKey: true });
    click(a, { shiftKey: true });
    click(a, { metaKey: true });
    expect(openUrl).toHaveBeenCalledTimes(3);
  });

  it('does not touch internal links, non-web schemes or non-links', () => {
    click(anchor('/settings'));
    click(anchor('#top'));
    click(anchor('mailto:a@example.com'));
    const button = document.createElement('button');
    document.body.appendChild(button);
    click(button);
    expect(openUrl).not.toHaveBeenCalled();
  });

  it('leaves a download link to the browser', () => {
    click(anchor('https://example.com/save.zip', { download: '' }));
    expect(openUrl).not.toHaveBeenCalled();
  });

  it('ignores non-left buttons and already-handled clicks', () => {
    const a = anchor('https://example.com/docs');
    click(a, { button: 1 });
    expect(openUrl).not.toHaveBeenCalled();

    const ev = new MouseEvent('click', { bubbles: true, cancelable: true });
    ev.preventDefault();
    a.dispatchEvent(ev);
    expect(openUrl).not.toHaveBeenCalled();
  });

  it('stops listening after teardown', () => {
    const a = anchor('https://example.com/docs');
    stop();
    click(a);
    expect(openUrl).not.toHaveBeenCalled();
    stop = () => {};
  });
});
