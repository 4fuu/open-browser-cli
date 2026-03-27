import type {
  ChunkEvent,
  ContentRequest,
  ContentResponse,
  PageChunk,
  RawNode,
  RawSnapshot,
  Rect,
} from '../shared/types.js';

const elementMap = new Map<string, WeakRef<Element>>();
let cursorOverlay: HTMLElement | null = null;

const SKIPPED_TAGS = new Set(['script', 'style', 'noscript', 'svg', 'path']);
const CHUNK_SIZE = 100;
const CHUNK_DELAY_MS = 8;
const STABILITY_WINDOW_MS = 500;
const DEFAULT_WAIT_TIMEOUT_MS = 30_000;
const TYPE_DELAY_MIN_MS = 40;
const TYPE_DELAY_MAX_MS = 120;
const MOUSE_APPROACH_STEPS_MIN = 3;
const MOUSE_APPROACH_STEPS_MAX = 6;

let refCounter = 0;

interface WaitResult {
  selectorFound: boolean;
  changed: boolean;
  waitedMs: number;
}

chrome.runtime.onMessage.addListener(
  (
    msg: ContentRequest,
    _sender: chrome.runtime.MessageSender,
    sendResponse: (response: ContentResponse) => void,
  ) => {
    handleMessage(msg).then(sendResponse);
    return true;
  },
);

async function handleMessage(req: ContentRequest): Promise<ContentResponse> {
  try {
    switch (req.type) {
      case 'snapshot':
        return await handleSnapshot(req);
      case 'click':
        return await handleClick(req);
      case 'type':
        return await handleType(req);
      case 'wait':
        return await handleWait(req);
      default:
        return { ok: false, error: `Unknown message type: ${String((req as { type?: unknown }).type)}` };
    }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function handleSnapshot(req: ContentRequest): Promise<ContentResponse> {
  const sessionId = requireString(req.params.session_id, 'session_id');
  const requestId = requireString(req.params.request_id, 'request_id');
  const snapshot = collectSnapshot();
  await streamSnapshot(sessionId, requestId, snapshot);
  return {
    ok: true,
    data: {
      streamed: true,
      count: snapshot.nodes.length,
      url: snapshot.url,
      title: snapshot.title,
    },
  };
}

async function handleClick(req: ContentRequest): Promise<ContentResponse> {
  const sessionId = requireString(req.params.session_id, 'session_id');
  const requestId = requireString(req.params.request_id, 'request_id');
  const refId = requireString(req.params.ref, 'ref');
  const target = resolveTarget(refId);
  if (!target) {
    return { ok: false, error: `Element not found: ${refId}` };
  }

  const beforeUrl = location.href;
  const beforeTitle = document.title;

  if (target instanceof HTMLElement) {
    target.scrollIntoView({ block: 'center', inline: 'center' });
  }
  await nextFrame();

  const eventInit = mouseEventInit(target);
  await simulateMouseApproach(target, eventInit);
  moveCursor(eventInit.clientX ?? 0, eventInit.clientY ?? 0);
  target.dispatchEvent(new MouseEvent('mouseover', eventInit));
  await delay(jitter(5, 20));
  target.dispatchEvent(new MouseEvent('mousedown', eventInit));
  await delay(jitter(50, 150));
  target.dispatchEvent(new MouseEvent('mouseup', eventInit));
  await delay(jitter(5, 20));
  target.dispatchEvent(new MouseEvent('click', eventInit));

  const stability = await waitForPageStability({ timeoutMs: 5_000 });
  const snapshot = collectSnapshot();
  await streamSnapshot(sessionId, requestId, snapshot);

  return {
    ok: true,
    data: {
      action: 'click',
      changed: stability.changed || beforeUrl !== location.href || beforeTitle !== document.title,
      navigated: beforeUrl !== location.href,
      url: location.href,
      title: document.title,
    },
  };
}

async function handleType(req: ContentRequest): Promise<ContentResponse> {
  const sessionId = requireString(req.params.session_id, 'session_id');
  const requestId = requireString(req.params.request_id, 'request_id');
  const refId = requireString(req.params.ref, 'ref');
  const text = requireString(req.params.text, 'text');
  const target = resolveTarget(refId);
  if (!target) {
    return { ok: false, error: `Element not found: ${refId}` };
  }
  if (!isEditable(target)) {
    return { ok: false, error: 'Element is not editable' };
  }

  const beforeUrl = location.href;
  const beforeTitle = document.title;

  focusElement(target);
  clearEditableValue(target);

  for (const char of text) {
    dispatchKeyboardEvent(target, 'keydown', char);
    insertText(target, char);
    dispatchInputEvent(target, char);
    dispatchKeyboardEvent(target, 'keyup', char);
    await delay(jitter(TYPE_DELAY_MIN_MS, TYPE_DELAY_MAX_MS));
  }

  target.dispatchEvent(new Event('change', { bubbles: true }));
  if (target instanceof HTMLElement) {
    target.blur();
  }

  const stability = await waitForPageStability({ timeoutMs: 5_000 });
  const snapshot = collectSnapshot();
  await streamSnapshot(sessionId, requestId, snapshot);

  return {
    ok: true,
    data: {
      action: 'type',
      changed: stability.changed || beforeUrl !== location.href || beforeTitle !== document.title,
      navigated: beforeUrl !== location.href,
      url: location.href,
      title: document.title,
    },
  };
}

async function handleWait(req: ContentRequest): Promise<ContentResponse> {
  const selector = optionalString(req.params.selector);
  const timeout = optionalNumber(req.params.timeout) ?? DEFAULT_WAIT_TIMEOUT_MS;
  const result = await waitForPageStability({ selector, timeoutMs: timeout });
  return {
    ok: true,
    data: {
      selector,
      selector_found: result.selectorFound,
      changed: result.changed,
      waited_ms: result.waitedMs,
    },
  };
}

function collectSnapshot(): RawSnapshot {
  elementMap.clear();
  refCounter = 0;

  const snapshot: RawSnapshot = {
    url: location.href,
    title: document.title,
    viewport: {
      width: Math.max(window.innerWidth, 1),
      height: Math.max(window.innerHeight, 1),
    },
    scroll: {
      top: window.scrollY,
      height: Math.max(document.documentElement.scrollHeight, window.innerHeight),
    },
    nodes: [],
  };

  if (document.body) {
    walkElement(document.body, undefined, snapshot.nodes);
  }

  return snapshot;
}

function walkElement(
  element: Element,
  parentRef: string | undefined,
  nodes: RawNode[],
): void {
  const tag = element.tagName.toLowerCase();
  if (SKIPPED_TAGS.has(tag) || element.getAttribute('aria-hidden') === 'true') {
    return;
  }
  if (!isVisible(element)) {
    return;
  }

  const rect = toAbsoluteRect(element.getBoundingClientRect());
  if (rect.w <= 0 || rect.h <= 0) {
    return;
  }

  const refId = `r${++refCounter}`;
  elementMap.set(refId, new WeakRef(element));

  nodes.push({
    ref: refId,
    parent: parentRef,
    tag,
    text: element.textContent ?? '',
    attrs: extractAttrs(element),
    rect,
  });

  for (const child of Array.from(element.children)) {
    walkElement(child, refId, nodes);
  }

  if (element.shadowRoot) {
    for (const child of Array.from(element.shadowRoot.children)) {
      walkElement(child, refId, nodes);
    }
  }
}

async function streamSnapshot(
  sessionId: string,
  requestId: string,
  snapshot: RawSnapshot,
): Promise<void> {
  if (snapshot.nodes.length === 0) {
    await postChunk({
      type: 'page_chunk',
      session_id: sessionId,
      request_id: requestId,
      meta: {
        url: snapshot.url,
        title: snapshot.title,
        viewport: snapshot.viewport,
        scroll: snapshot.scroll,
      },
      nodes: [],
      chunk_index: 0,
      done: true,
    });
    return;
  }

  for (let index = 0; index < snapshot.nodes.length; index += CHUNK_SIZE) {
    const chunkIndex = Math.floor(index / CHUNK_SIZE);
    const chunk: PageChunk = {
      type: 'page_chunk',
      session_id: sessionId,
      request_id: requestId,
      meta:
        chunkIndex === 0
          ? {
              url: snapshot.url,
              title: snapshot.title,
              viewport: snapshot.viewport,
              scroll: snapshot.scroll,
            }
          : undefined,
      nodes: snapshot.nodes.slice(index, index + CHUNK_SIZE),
      chunk_index: chunkIndex,
      done: index + CHUNK_SIZE >= snapshot.nodes.length,
    };
    await postChunk(chunk);
    if (!chunk.done) {
      await delay(CHUNK_DELAY_MS);
    }
  }
}

async function postChunk(chunk: PageChunk): Promise<void> {
  const message: ChunkEvent = { type: 'page_chunk', chunk };
  await chrome.runtime.sendMessage(message);
}

async function waitForPageStability(options: {
  selector?: string;
  timeoutMs?: number;
}): Promise<WaitResult> {
  const selector = options.selector;
  const timeoutMs = options.timeoutMs ?? DEFAULT_WAIT_TIMEOUT_MS;
  const startedAt = Date.now();

  if (selector && document.querySelector(selector)) {
    return { selectorFound: true, changed: false, waitedMs: 0 };
  }

  return await new Promise<WaitResult>((resolve, reject) => {
    let settled = false;
    let sawMutation = false;
    let quietTimer: number | undefined;

    const cleanup = (): void => {
      observer.disconnect();
      window.clearTimeout(timeoutTimer);
      if (quietTimer !== undefined) {
        window.clearTimeout(quietTimer);
      }
    };

    const finish = (selectorFound: boolean): void => {
      if (settled) {
        return;
      }
      settled = true;
      cleanup();
      resolve({
        selectorFound,
        changed: sawMutation,
        waitedMs: Date.now() - startedAt,
      });
    };

    const observer = new MutationObserver(() => {
      sawMutation = true;
      if (selector && document.querySelector(selector)) {
        finish(true);
        return;
      }
      if (quietTimer !== undefined) {
        window.clearTimeout(quietTimer);
      }
      quietTimer = window.setTimeout(() => finish(false), STABILITY_WINDOW_MS);
    });

    observer.observe(document.documentElement, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
    });

    quietTimer = window.setTimeout(() => finish(false), STABILITY_WINDOW_MS);
    const timeoutTimer = window.setTimeout(() => {
      cleanup();
      if (!settled) {
        reject(new Error(`wait timed out after ${timeoutMs}ms`));
      }
    }, timeoutMs);
  });
}

function resolveTarget(refId: string): Element | null {
  return elementMap.get(refId)?.deref() ?? null;
}

function extractAttrs(element: Element): Record<string, string> {
  const attrs: Record<string, string> = {};
  const maybeSet = (name: string, value: string | null | undefined): void => {
    if (value !== null && value !== undefined && value !== '') {
      attrs[name] = value;
    }
  };

  maybeSet('href', element.getAttribute('href'));
  maybeSet('type', element.getAttribute('type'));
  maybeSet('placeholder', element.getAttribute('placeholder'));
  maybeSet('name', element.getAttribute('name'));
  maybeSet('role', element.getAttribute('role'));
  maybeSet('aria-label', element.getAttribute('aria-label'));
  if (element.hasAttribute('onclick')) {
    attrs.onclick = 'true';
  }
  if (element.hasAttribute('disabled')) {
    attrs.disabled = 'true';
  }
  if (element.hasAttribute('checked')) {
    attrs.checked = 'true';
  }
  if (element.hasAttribute('selected')) {
    attrs.selected = 'true';
  }

  if (
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement ||
    element instanceof HTMLSelectElement
  ) {
    maybeSet('value', element.value);
  }

  return attrs;
}

function isVisible(element: Element): boolean {
  const htmlElement = element as HTMLElement;
  if (htmlElement.offsetParent !== null) {
    return true;
  }
  // Fallback for position:fixed/sticky elements which have null offsetParent.
  // getBoundingClientRect is cheaper than getComputedStyle.
  const rect = element.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}

function toAbsoluteRect(rect: DOMRect): Rect {
  return {
    x: rect.left + window.scrollX,
    y: rect.top + window.scrollY,
    w: rect.width,
    h: rect.height,
  };
}

function mouseEventInit(target: Element): MouseEventInit {
  const rect = target.getBoundingClientRect();
  // Land within the central 40% of the element to avoid edges.
  const clientX = rect.left + rect.width * (0.3 + Math.random() * 0.4);
  const clientY = rect.top + rect.height * (0.3 + Math.random() * 0.4);
  return {
    bubbles: true,
    cancelable: true,
    composed: true,
    clientX,
    clientY,
    button: 0,
  };
}

async function simulateMouseApproach(
  target: Element,
  finalInit: MouseEventInit,
): Promise<void> {
  const finalX = finalInit.clientX ?? 0;
  const finalY = finalInit.clientY ?? 0;
  const startX = finalX + (Math.random() - 0.5) * 300;
  const startY = finalY + (Math.random() - 0.5) * 200;
  const steps =
    MOUSE_APPROACH_STEPS_MIN +
    Math.floor(Math.random() * (MOUSE_APPROACH_STEPS_MAX - MOUSE_APPROACH_STEPS_MIN + 1));

  for (let i = 1; i <= steps; i++) {
    const t = i / steps;
    const x = startX + (finalX - startX) * t;
    const y = startY + (finalY - startY) * t;
    moveCursor(x, y);
    target.dispatchEvent(
      new MouseEvent('mousemove', {
        bubbles: true,
        cancelable: true,
        composed: true,
        clientX: x,
        clientY: y,
        button: 0,
      }),
    );
    await delay(jitter(10, 30));
  }
}

function getOrCreateCursor(): HTMLElement {
  if (cursorOverlay && document.documentElement.contains(cursorOverlay)) {
    return cursorOverlay;
  }
  const el = document.createElement('div');
  el.setAttribute('id', '__browser_cli_cursor__');
  // position:fixed on a direct child of <html> is unaffected by transforms on <body>.
  // No transform/filter on the host itself to avoid creating a new containing block.
  // All layout via top/left so the element never clips itself.
  el.style.cssText =
    'position:fixed;top:0;left:0;width:0;height:0;overflow:visible;' +
    'pointer-events:none;z-index:2147483647;margin:0;padding:0;border:none;';

  // Shadow DOM isolates the SVG from page stylesheets.
  const shadow = el.attachShadow({ mode: 'open' });
  shadow.innerHTML =
    '<style>:host{all:initial}svg{display:block;filter:drop-shadow(0 1px 2px rgba(0,0,0,0.5));' +
    'transition:transform 25ms ease-out;}</style>' +
    '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="22" viewBox="0 0 14 24">' +
    '<path d="M 1 1 L 1 19 L 5 14 L 8 22 L 11 21 L 8 13 L 13 13 Z"' +
    ' fill="black" stroke="white" stroke-width="1.5" stroke-linejoin="round"/>' +
    '</svg>';

  document.documentElement.appendChild(el);
  cursorOverlay = el;
  return el;
}

function moveCursor(clientX: number, clientY: number): void {
  const el = getOrCreateCursor();
  el.style.left = `${clientX}px`;
  el.style.top = `${clientY}px`;
}

function jitter(min: number, max: number): number {
  return min + Math.random() * (max - min);
}

function isEditable(element: Element): boolean {
  return (
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement ||
    element instanceof HTMLSelectElement ||
    element instanceof HTMLElement && element.isContentEditable
  );
}

function focusElement(element: Element): void {
  if (element instanceof HTMLElement) {
    element.focus();
  }
}

function clearEditableValue(element: Element): void {
  if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
    const setter =
      Object.getOwnPropertyDescriptor(
        Object.getPrototypeOf(element),
        'value',
      )?.set;
    setter?.call(element, '');
    return;
  }
  if (element instanceof HTMLSelectElement) {
    element.selectedIndex = -1;
    return;
  }
  if (element instanceof HTMLElement && element.isContentEditable) {
    element.textContent = '';
  }
}

function insertText(element: Element, char: string): void {
  if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
    const setter =
      Object.getOwnPropertyDescriptor(
        Object.getPrototypeOf(element),
        'value',
      )?.set;
    setter?.call(element, `${element.value}${char}`);
    return;
  }
  if (element instanceof HTMLElement && element.isContentEditable) {
    element.textContent = `${element.textContent ?? ''}${char}`;
  }
}

function dispatchKeyboardEvent(
  element: Element,
  type: 'keydown' | 'keyup',
  key: string,
): void {
  element.dispatchEvent(
    new KeyboardEvent(type, {
      key,
      bubbles: true,
      cancelable: true,
      composed: true,
    }),
  );
}

function dispatchInputEvent(element: Element, data: string): void {
  element.dispatchEvent(
    new InputEvent('input', {
      data,
      bubbles: true,
      cancelable: false,
      composed: true,
      inputType: 'insertText',
    }),
  );
}

function requireString(value: unknown, name: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}
