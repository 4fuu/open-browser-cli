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
const INTERACTIVE_SELECTOR = [
  'a[href]',
  'button',
  'input:not([type="hidden"])',
  'textarea',
  'select',
  '[role="button"]',
  '[onclick]',
  '[contenteditable="true"]',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

const CHUNK_SIZE = 100;
const CHUNK_DELAY_MS = 8;
const STABILITY_WINDOW_MS = 500;
const DEFAULT_WAIT_TIMEOUT_MS = 30_000;
const TYPE_DELAY_MIN_MS = 40;
const TYPE_DELAY_MAX_MS = 120;
const IDLE_DECISION_INTERVAL_MS = 1000;
const IDLE_MOVE_MIN_GAP_MS = 2000;
const IDLE_FORCE_START_MS = 10_000;
const IDLE_INACTIVITY_STOP_MS = 30_000;
// Speed in pixels per second — used to compute duration from distance
const MOVE_SPEED_MIN_PPS = 280;
const MOVE_SPEED_MAX_PPS = 700;
// Minimum/maximum duration regardless of distance
const MOVE_DURATION_MIN_MS = 180;
const MOVE_DURATION_MAX_MS = 2400;

let refCounter = 0;

interface WaitResult {
  selectorFound: boolean;
  changed: boolean;
  waitedMs: number;
}

interface Point {
  x: number;
  y: number;
}

interface Bounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

class CursorAgent {
  private enabled = false;
  private sessionId: string | null = null;
  private taskMode = false;
  private currentX = Math.max(window.innerWidth * 0.35, 24);
  private currentY = Math.max(window.innerHeight * 0.22, 24);
  private idleLoopToken = 0;
  private motionToken = 0;
  private hoveredElement: Element | null = null;
  private lastCliActivityAt = Date.now();
  private lastIdleMoveFinishedAt = 0;

  constructor() {
    document.addEventListener('visibilitychange', () => {
      if (!this.enabled) {
        return;
      }
      if (document.hidden) {
        this.cancelMotion();
        return;
      }
      this.moveInstant(this.currentX, this.currentY);
      if (!this.taskMode) {
        this.startIdleLoop();
      }
    });

    window.addEventListener('resize', () => {
      if (!this.enabled) {
        return;
      }
      const point = this.clampPoint({ x: this.currentX, y: this.currentY });
      this.moveInstant(point.x, point.y);
    });
  }

  start(sessionId: string): void {
    this.enabled = true;
    this.sessionId = sessionId;
    this.lastCliActivityAt = Date.now();
    this.lastIdleMoveFinishedAt = Date.now();
    this.ensureCursor();
    const point = this.clampPoint({ x: this.currentX, y: this.currentY });
    this.moveInstant(point.x, point.y);
    this.setTaskMode(false);
    this.startIdleLoop();
  }

  stop(sessionId?: string): void {
    if (sessionId && this.sessionId && sessionId !== this.sessionId) {
      return;
    }

    this.enabled = false;
    this.sessionId = null;
    this.taskMode = false;
    this.cancelMotion();
    this.hoveredElement = null;

    if (cursorOverlay && document.documentElement.contains(cursorOverlay)) {
      cursorOverlay.remove();
    }
    cursorOverlay = null;
  }

  beginTask(): void {
    if (!this.enabled) {
      this.start('implicit');
    }
    this.cancelMotion();
    this.setTaskMode(true);
  }

  endTask(): void {
    if (!this.enabled) {
      return;
    }
    this.setTaskMode(false);
    this.lastIdleMoveFinishedAt = Date.now();
    this.startIdleLoop();
  }

  noteCliActivity(): void {
    this.lastCliActivityAt = Date.now();
  }

  async moveTo(point: Point): Promise<boolean> {
    if (!this.enabled || document.hidden) {
      return false;
    }

    const end = this.clampPoint(point);
    const totalDistance = Math.hypot(end.x - this.currentX, end.y - this.currentY);
    if (totalDistance < 1) {
      this.moveInstant(end.x, end.y);
      return true;
    }

    const motionToken = ++this.motionToken;

    // Decide whether to insert a detour mid-flight.
    // Longer distances get a higher chance; short hops almost never detour.
    const detourChance = clamp(totalDistance / 1200, 0.08, 0.4);
    const doDetour = Math.random() < detourChance;

    if (doDetour) {
      // Pick a random progress point (30-70% along the straight line) to break away
      const breakT = jitter(0.3, 0.7);
      const breakPoint = {
        x: this.currentX + (end.x - this.currentX) * breakT,
        y: this.currentY + (end.y - this.currentY) * breakT,
      };

      // Offset the break point perpendicular to the path — the "wrong" direction
      const detourSpread = clamp(totalDistance * 0.25, 30, 180);
      const detour = this.clampPoint({
        x: breakPoint.x + (Math.random() - 0.5) * detourSpread * 2,
        y: breakPoint.y + (Math.random() - 0.5) * detourSpread * 2,
      });

      // First leg: move toward the detour point
      if (!await this.moveSegment(detour, motionToken)) {
        return false;
      }

      // Brief hesitation at the detour point
      await delay(jitter(60, 350));
      if (!this.enabled || motionToken !== this.motionToken) {
        return false;
      }

      // Second leg: correct back to the real target
      if (!await this.moveSegment(end, motionToken)) {
        return false;
      }
    } else {
      if (!await this.moveSegment(end, motionToken)) {
        return false;
      }
    }

    this.moveInstant(end.x, end.y);
    return motionToken === this.motionToken;
  }

  /** Low-level cubic-bezier segment with real mouse events on every frame. */
  private async moveSegment(target: Point, motionToken: number): Promise<boolean> {
    const start = { x: this.currentX, y: this.currentY };
    const distance = Math.hypot(target.x - start.x, target.y - start.y);
    if (distance < 1) {
      this.moveInstant(target.x, target.y);
      return motionToken === this.motionToken;
    }

    const speed = jitter(MOVE_SPEED_MIN_PPS, MOVE_SPEED_MAX_PPS);
    const durationMs = clamp((distance / speed) * 1000, MOVE_DURATION_MIN_MS, MOVE_DURATION_MAX_MS);
    const steps = Math.max(10, Math.round(durationMs / 16));

    const perpX = (target.y - start.y) / distance;
    const perpY = -(target.x - start.x) / distance;
    const drift1 = (Math.random() - 0.5) * Math.min(120, distance * 0.3);
    const drift2 = (Math.random() - 0.5) * Math.min(80, distance * 0.2);
    const ctrl1 = {
      x: start.x + (target.x - start.x) * 0.33 + perpX * drift1,
      y: start.y + (target.y - start.y) * 0.33 + perpY * drift1,
    };
    const ctrl2 = {
      x: start.x + (target.x - start.x) * 0.67 + perpX * drift2,
      y: start.y + (target.y - start.y) * 0.67 + perpY * drift2,
    };
    const wobblePhase = Math.random() * Math.PI * 2;
    const wobbleAmplitude = Math.min(14, Math.max(2, distance * 0.03));
    const hesitationStep = steps > 18 ? Math.floor(jitter(steps * 0.25, steps * 0.72)) : -1;

    let hovered = this.hoveredElement;

    for (let index = 1; index <= steps; index++) {
      if (!this.enabled || motionToken !== this.motionToken) {
        return false;
      }

      const t = easeInOutCubic(index / steps);
      const wobble =
        Math.sin(t * Math.PI * jitter(2.2, 4.4) + wobblePhase) * wobbleAmplitude * (1 - t) * 0.35;
      const x = cubicBezier(start.x, ctrl1.x, ctrl2.x, target.x, t) + perpX * wobble;
      const y = cubicBezier(start.y, ctrl1.y, ctrl2.y, target.y, t) + perpY * wobble;
      this.moveInstant(x, y);
      hovered = dispatchCursorMoveEvent(hovered, x, y);

      if (index === hesitationStep) {
        await delay(jitter(32, 96));
      } else {
        await delay(jitter(10, 22));
      }
    }

    this.hoveredElement = hovered;
    return motionToken === this.motionToken;
  }

  async clickPulse(): Promise<void> {
    if (!this.enabled) {
      return;
    }

    const host = this.ensureCursor();
    host.style.setProperty('--cursor-scale', '0.82');
    await delay(70);
    if (!this.enabled) {
      return;
    }
    host.style.setProperty('--cursor-scale', '1');
    await delay(110);
  }

  private ensureCursor(): HTMLElement {
    if (cursorOverlay && document.documentElement.contains(cursorOverlay)) {
      this.syncAppearance(cursorOverlay);
      return cursorOverlay;
    }

    const el = document.createElement('div');
    el.setAttribute('id', '__browser_cli_cursor__');
    el.style.cssText =
      'position:fixed;top:0;left:0;width:0;height:0;overflow:visible;' +
      'pointer-events:none;z-index:2147483647;margin:0;padding:0;border:none;';

    const shadow = el.attachShadow({ mode: 'open' });
    shadow.innerHTML =
      '<style>' +
      ':host{all:initial;--cursor-fill:#111111;--cursor-stroke:#ffffff;--cursor-scale:1;--cursor-idle:0;}' +
      '.wrap{position:relative;display:inline-block;}' +
      'svg{display:block;transform:scale(var(--cursor-scale));transform-origin:2px 2px;' +
      'transition:transform 90ms ease,filter 120ms ease;' +
      'filter:drop-shadow(0 1px 2px rgba(0,0,0,0.45));}' +
      'path{fill:var(--cursor-fill);stroke:var(--cursor-stroke);stroke-width:1.5;stroke-linejoin:round;}' +
      '.zzz{position:absolute;left:14px;top:-2px;font:bold 10px/1 sans-serif;' +
      'color:#444;letter-spacing:2px;opacity:var(--cursor-idle);' +
      'transition:opacity 400ms ease;pointer-events:none;' +
      'text-shadow:0 1px 0 rgba(255,255,255,0.85);' +
      'animation:zf 2s ease-in-out infinite;}' +
      '@keyframes zf{0%,100%{transform:translateY(0)}50%{transform:translateY(-3px)}}' +
      '</style>' +
      '<div class="wrap">' +
      '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="22" viewBox="0 0 14 24" aria-hidden="true">' +
      '<path d="M 1 1 L 1 19 L 5 14 L 8 22 L 11 21 L 8 13 L 13 13 Z"/>' +
      '</svg>' +
      '<div class="zzz">zzz</div>' +
      '</div>';

    document.documentElement.appendChild(el);
    cursorOverlay = el;
    this.syncAppearance(el);
    return el;
  }

  private syncAppearance(host: HTMLElement): void {
    host.style.setProperty('--cursor-fill', this.taskMode ? '#f5cb23' : '#111111');
    host.style.setProperty('--cursor-stroke', this.taskMode ? '#6c5600' : '#ffffff');
    host.style.setProperty('--cursor-scale', this.taskMode ? '1' : '0.65');
    host.style.setProperty('--cursor-idle', this.taskMode ? '0' : '1');
  }

  private setTaskMode(value: boolean): void {
    this.taskMode = value;
    this.syncAppearance(this.ensureCursor());
  }

  private moveInstant(x: number, y: number): void {
    const host = this.ensureCursor();
    this.currentX = x;
    this.currentY = y;
    host.style.left = `${Math.round(x)}px`;
    host.style.top = `${Math.round(y)}px`;
  }

  private startIdleLoop(): void {
    if (!this.enabled || this.taskMode || document.hidden) {
      return;
    }

    const loopToken = ++this.idleLoopToken;
    void this.runIdleLoop(loopToken);
  }

  private async runIdleLoop(loopToken: number): Promise<void> {
    while (this.enabled && !this.taskMode && !document.hidden && loopToken === this.idleLoopToken) {
      const now = Date.now();
      if (now - this.lastCliActivityAt >= IDLE_INACTIVITY_STOP_MS) {
        this.stop(this.sessionId ?? undefined);
        return;
      }

      const elapsedSinceMove = now - this.lastIdleMoveFinishedAt;
      if (elapsedSinceMove < IDLE_MOVE_MIN_GAP_MS) {
        await delay(IDLE_DECISION_INTERVAL_MS);
        continue;
      }

      const secondsSinceMove = Math.floor(elapsedSinceMove / IDLE_DECISION_INTERVAL_MS);
      const shouldStartMove =
        elapsedSinceMove >= IDLE_FORCE_START_MS || Math.random() < secondsSinceMove / 10;

      if (!shouldStartMove) {
        await delay(IDLE_DECISION_INTERVAL_MS);
        continue;
      }

      const point = pickIdlePoint();
      await this.moveTo(point);
      this.lastIdleMoveFinishedAt = Date.now();

      if (!this.enabled || this.taskMode || document.hidden || loopToken !== this.idleLoopToken) {
        return;
      }
    }
  }

  private cancelMotion(): void {
    this.idleLoopToken += 1;
    this.motionToken += 1;
  }

  private clampPoint(point: Point): Point {
    return {
      x: clamp(point.x, 8, Math.max(8, window.innerWidth - 8)),
      y: clamp(point.y, 8, Math.max(8, window.innerHeight - 8)),
    };
  }
}

const cursorAgent = new CursorAgent();

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
    cursorAgent.noteCliActivity();
    switch (req.type) {
      case 'snapshot':
        return await handleSnapshot(req);
      case 'click':
        return await handleClick(req);
      case 'type':
        return await handleType(req);
      case 'wait':
        return await handleWait(req);
      case 'presence_start':
        return handlePresenceStart(req);
      case 'presence_stop':
        return handlePresenceStop(req);
      default:
        return {
          ok: false,
          error: `Unknown message type: ${String((req as { type?: unknown }).type)}`,
        };
    }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

function handlePresenceStart(req: ContentRequest): ContentResponse {
  const sessionId = requireString(req.params.session_id, 'session_id');
  cursorAgent.start(sessionId);
  return {
    ok: true,
    data: {
      session_id: sessionId,
      cursor: 'started',
    },
  };
}

function handlePresenceStop(req: ContentRequest): ContentResponse {
  const sessionId = optionalString(req.params.session_id);
  cursorAgent.stop(sessionId);
  return {
    ok: true,
    data: {
      session_id: sessionId,
      cursor: 'stopped',
    },
  };
}

async function handleSnapshot(req: ContentRequest): Promise<ContentResponse> {
  const sessionId = requireString(req.params.session_id, 'session_id');
  const requestId = requireString(req.params.request_id, 'request_id');
  const waitAfterLoad = optionalNumber(req.params.wait_after_load);

  if (waitAfterLoad !== undefined && waitAfterLoad > 0) {
    try {
      await waitForPageStability({ timeoutMs: waitAfterLoad });
    } catch {
      // Timed out waiting for stability — proceed with current DOM state
    }
  }

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

  cursorAgent.beginTask();

  try {
    const point = await prepareInteractionPoint(target);
    await cursorAgent.moveTo(point);
    await delay(jitter(30, 90));
    await performClickSequence(target, point);

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
  } finally {
    cursorAgent.endTask();
  }
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

  cursorAgent.beginTask();

  try {
    const point = await prepareInteractionPoint(target);
    await cursorAgent.moveTo(point);
    await delay(jitter(30, 90));
    await performFocusSequence(target, point);

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
  } finally {
    cursorAgent.endTask();
  }
}

async function handleWait(req: ContentRequest): Promise<ContentResponse> {
  const selector = optionalString(req.params.selector);
  const timeout = optionalNumber(req.params.timeout) ?? DEFAULT_WAIT_TIMEOUT_MS;

  cursorAgent.beginTask();
  try {
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
  } finally {
    cursorAgent.endTask();
  }
}

async function prepareInteractionPoint(target: Element): Promise<Point> {
  if (target instanceof HTMLElement) {
    target.scrollIntoView({ block: 'center', inline: 'center' });
  }

  await nextFrame();
  await nextFrame();

  const point = chooseInteractionPoint(target);
  if (!point) {
    throw new Error('Target is not visible');
  }
  return point;
}

async function performClickSequence(target: Element, point: Point): Promise<void> {
  const dispatchTarget = resolveDispatchTarget(target, point);
  const eventInit = mouseEventInitFromPoint(point);

  dispatchHoverTransition(dispatchTarget, eventInit);
  await delay(jitter(25, 80));
  void cursorAgent.clickPulse();
  dispatchTarget.dispatchEvent(new MouseEvent('mousedown', eventInit));
  if (dispatchTarget instanceof HTMLElement) {
    dispatchTarget.focus();
  }
  await delay(jitter(55, 135));
  dispatchTarget.dispatchEvent(new MouseEvent('mouseup', eventInit));
  await delay(jitter(8, 20));
  dispatchTarget.dispatchEvent(new MouseEvent('click', eventInit));
}

async function performFocusSequence(target: Element, point: Point): Promise<void> {
  const dispatchTarget = resolveDispatchTarget(target, point);
  const eventInit = mouseEventInitFromPoint(point);

  dispatchHoverTransition(dispatchTarget, eventInit);
  await delay(jitter(20, 60));
  void cursorAgent.clickPulse();
  dispatchTarget.dispatchEvent(new MouseEvent('mousedown', eventInit));
  if (dispatchTarget instanceof HTMLElement) {
    dispatchTarget.focus();
  }
  await delay(jitter(30, 90));
  dispatchTarget.dispatchEvent(new MouseEvent('mouseup', eventInit));
  await delay(jitter(6, 16));
  dispatchTarget.dispatchEvent(new MouseEvent('click', eventInit));
}

function dispatchHoverTransition(target: Element, eventInit: MouseEventInit): void {
  target.dispatchEvent(new MouseEvent('mouseover', eventInit));
  target.dispatchEvent(new MouseEvent('mousemove', eventInit));
}

function resolveDispatchTarget(target: Element, point: Point): Element {
  const hit = resolvePointElement(point.x, point.y);
  if (hit && isRelatedTarget(target, hit)) {
    return hit;
  }
  return target;
}

function chooseInteractionPoint(target: Element): Point | null {
  const bounds = getVisibleBounds(target);
  if (!bounds) {
    return null;
  }

  const maxInsetX = Math.max(0, bounds.width / 2 - 1);
  const maxInsetY = Math.max(0, bounds.height / 2 - 1);
  const insetX = clamp(bounds.width * 0.18, 2, Math.max(2, maxInsetX));
  const insetY = clamp(bounds.height * 0.18, 2, Math.max(2, maxInsetY));
  const left = bounds.left + insetX;
  const right = bounds.right - insetX;
  const top = bounds.top + insetY;
  const bottom = bounds.bottom - insetY;

  const candidates: Point[] = [
    { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 },
    { x: left, y: top },
    { x: right, y: top },
    { x: left, y: bottom },
    { x: right, y: bottom },
    { x: (left + right) / 2, y: top },
    { x: (left + right) / 2, y: bottom },
  ];

  for (let index = 0; index < 4; index++) {
    candidates.push({
      x: left + Math.random() * Math.max(1, right - left),
      y: top + Math.random() * Math.max(1, bottom - top),
    });
  }

  for (const point of candidates) {
    if (pointHitsTarget(target, point)) {
      return point;
    }
  }

  return candidates[0] ?? null;
}

function pointHitsTarget(target: Element, point: Point): boolean {
  const hit = resolvePointElement(point.x, point.y);
  return hit ? isRelatedTarget(target, hit) : false;
}

function isRelatedTarget(target: Element, hit: Element): boolean {
  return target === hit || target.contains(hit) || hit.contains(target);
}

function resolvePointElement(clientX: number, clientY: number): Element | null {
  const x = clamp(clientX, 0, Math.max(0, window.innerWidth - 1));
  const y = clamp(clientY, 0, Math.max(0, window.innerHeight - 1));
  return document.elementFromPoint(x, y);
}

function pickIdlePoint(): Point {
  const anchors = collectVisibleAnchors();
  if (anchors.length > 0 && Math.random() < 0.72) {
    return anchors[Math.floor(Math.random() * anchors.length)] ?? randomViewportPoint();
  }
  return randomViewportPoint();
}

function collectVisibleAnchors(): Point[] {
  const points: Point[] = [];
  const candidates = Array.from(document.querySelectorAll(INTERACTIVE_SELECTOR));

  for (const element of candidates) {
    if (points.length >= 48) {
      break;
    }
    if (!isVisible(element)) {
      continue;
    }
    const bounds = getVisibleBounds(element);
    if (!bounds) {
      continue;
    }

    points.push({
      x: bounds.left + bounds.width * (0.25 + Math.random() * 0.5),
      y: bounds.top + bounds.height * (0.25 + Math.random() * 0.5),
    });
  }

  return points;
}

function randomViewportPoint(): Point {
  return {
    x: 20 + Math.random() * Math.max(20, window.innerWidth - 40),
    y: 20 + Math.random() * Math.max(20, window.innerHeight - 40),
  };
}

function getVisibleBounds(element: Element): Bounds | null {
  const rect = element.getBoundingClientRect();
  const left = clamp(rect.left, 0, window.innerWidth);
  const right = clamp(rect.right, 0, window.innerWidth);
  const top = clamp(rect.top, 0, window.innerHeight);
  const bottom = clamp(rect.bottom, 0, window.innerHeight);
  const width = right - left;
  const height = bottom - top;

  if (width <= 0 || height <= 0) {
    return null;
  }

  return { left, top, right, bottom, width, height };
}

function dispatchCursorMoveEvent(previous: Element | null, clientX: number, clientY: number): Element | null {
  const target = resolvePointElement(clientX, clientY);
  const eventInit = mouseEventInitFromPoint({ x: clientX, y: clientY });

  if (previous && previous !== target) {
    previous.dispatchEvent(new MouseEvent('mouseout', eventInit));
  }
  if (target && previous !== target) {
    target.dispatchEvent(new MouseEvent('mouseover', eventInit));
  }
  (target ?? document.documentElement).dispatchEvent(new MouseEvent('mousemove', eventInit));
  return target;
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
  maybeSet('class', element.getAttribute('class'));
  maybeSet('tabindex', element.getAttribute('tabindex'));
  maybeSet('aria-label', element.getAttribute('aria-label'));
  maybeSet('aria-selected', element.getAttribute('aria-selected'));
  maybeSet('aria-pressed', element.getAttribute('aria-pressed'));
  maybeSet('aria-current', element.getAttribute('aria-current'));
  maybeSet('aria-expanded', element.getAttribute('aria-expanded'));
  maybeSet('title', element.getAttribute('title'));
  const maybeOnclick = (element as HTMLElement & { onclick?: unknown }).onclick;
  if (element.hasAttribute('onclick') || typeof maybeOnclick === 'function') {
    attrs.onclick = 'true';
  }
  if (
    !element.matches(INTERACTIVE_SELECTOR) &&
    element instanceof HTMLElement &&
    getComputedStyle(element).cursor === 'pointer'
  ) {
    attrs.cursor = 'pointer';
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

function mouseEventInitFromPoint(point: Point): MouseEventInit {
  return {
    bubbles: true,
    cancelable: true,
    composed: true,
    clientX: point.x,
    clientY: point.y,
    button: 0,
  };
}

function jitter(min: number, max: number): number {
  return min + Math.random() * (max - min);
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function cubicBezier(p0: number, p1: number, p2: number, p3: number, t: number): number {
  const inverse = 1 - t;
  return inverse * inverse * inverse * p0
    + 3 * inverse * inverse * t * p1
    + 3 * inverse * t * t * p2
    + t * t * t * p3;
}

function easeInOutCubic(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - ((-2 * t + 2) ** 3) / 2;
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
