import type {
  ChunkEvent,
  ContentRequest,
  ContentResponse,
  Request,
  Response,
  Session,
} from '../shared/types.js';

const NATIVE_HOST = 'com.browser_cli.relay';
const RECONNECT_BASE_DELAY_MS = 1_000;
const RECONNECT_MAX_DELAY_MS = 30_000;
const CLICK_TARGET_GRACE_MS = 250;

const sessions = new Map<string, Session>();
let port: chrome.runtime.Port | null = null;
let reconnectDelay = RECONNECT_BASE_DELAY_MS;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

connectNative();

chrome.runtime.onMessage.addListener((message: unknown) => {
  if (!isChunkEvent(message)) {
    return;
  }
  ensureNativePort();
  port?.postMessage(message.chunk);
});

function connectNative(): void {
  if (reconnectTimer !== null) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }

  port = chrome.runtime.connectNative(NATIVE_HOST);

  port.onMessage.addListener((message: Request) => {
    reconnectDelay = RECONNECT_BASE_DELAY_MS;
    handleRequest(message).then((response) => {
      port?.postMessage(response);
    });
  });

  port.onDisconnect.addListener(() => {
    const errorMsg = chrome.runtime.lastError?.message ?? 'unknown';
    console.warn(
      `Native messaging disconnected: ${errorMsg}. Reconnecting in ${reconnectDelay}ms`,
    );
    port = null;
    scheduleReconnect();
  });
}

function scheduleReconnect(): void {
  if (reconnectTimer !== null) {
    return;
  }
  const currentDelay = reconnectDelay;
  reconnectDelay = Math.min(reconnectDelay * 2, RECONNECT_MAX_DELAY_MS);
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectNative();
  }, currentDelay);
}

function ensureNativePort(): void {
  if (!port && reconnectTimer === null) {
    connectNative();
  }
}

async function handleRequest(req: Request): Promise<Response> {
  try {
    switch (req.action) {
      case 'open':
        return await handleOpen(req);
      case 'close':
        return await handleClose(req);
      case 'list':
        return handleList(req);
      case 'get_page':
        return await handleSnapshotRequest(req);
      case 'click':
      case 'type':
      case 'wait':
        return await forwardToContent(req);
      case 'screenshot':
        return await handleScreenshot(req);
      default:
        return { id: req.id, ok: false, error: `Unknown action: ${req.action}` };
    }
  } catch (error) {
    return {
      id: req.id,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function handleOpen(req: Request): Promise<Response> {
  const url = req.params.url;
  if (typeof url !== 'string' || url.length === 0) {
    return { id: req.id, ok: false, error: 'url is required' };
  }

  const tab = await chrome.tabs.create({ url });
  if (!tab.id) {
    return { id: req.id, ok: false, error: 'Failed to create tab' };
  }

  await waitForTabLoad(tab.id);

  const session = await createTrackedSession(tab.id, req.id, optionalSessionId(req.params.session_id));
  const snapshotResult = await requestSessionSnapshot(session, req.id, req.params.wait_after_load);
  if (!snapshotResult.ok) {
    sessions.delete(session.session_id);
    return { id: req.id, ok: false, error: snapshotResult.error };
  }

  const updatedTab = await chrome.tabs.get(tab.id);
  session.url = updatedTab.url ?? session.url;
  session.title = updatedTab.title ?? session.title;

  return { id: req.id, ok: true, data: session as unknown as Record<string, unknown> };
}

async function handleClose(req: Request): Promise<Response> {
  if (req.params.all === true) {
    const activeSessions = Array.from(sessions.values());
    const tabIds = activeSessions.map((session) => session.tab_id);
    const closed = tabIds.length;
    await Promise.all(
      activeSessions.map((session) =>
        sendToContent(session.tab_id, {
          type: 'presence_stop',
          params: {
            session_id: session.session_id,
            request_id: req.id,
          },
        }).catch(() => ({ ok: false })),
      ),
    );
    if (tabIds.length > 0) {
      await chrome.tabs.remove(tabIds);
    }
    sessions.clear();
    return { id: req.id, ok: true, data: { closed } };
  }

  const sessionId = req.params.session_id;
  if (typeof sessionId !== 'string' || sessionId.length === 0) {
    return { id: req.id, ok: false, error: 'session_id is required' };
  }

  const session = sessions.get(sessionId);
  if (!session) {
    return { id: req.id, ok: false, error: `Session not found: ${sessionId}` };
  }

  await sendToContent(session.tab_id, {
    type: 'presence_stop',
    params: {
      session_id: session.session_id,
      request_id: req.id,
    },
  });
  await chrome.tabs.remove(session.tab_id);
  sessions.delete(sessionId);
  return { id: req.id, ok: true };
}

function handleList(req: Request): Response {
  return {
    id: req.id,
    ok: true,
    data: { sessions: Array.from(sessions.values()) },
  };
}

async function handleSnapshotRequest(req: Request): Promise<Response> {
  const session = sessionFromRequest(req);
  if (!session.ok) {
    return { id: req.id, ok: false, error: session.error };
  }

  await ensureTabLoaded(session.value.tab_id);

  const result = await sendToContent(session.value.tab_id, {
    type: 'snapshot',
    params: {
      session_id: session.value.session_id,
      request_id: req.id,
      wait_after_load: req.params.wait_after_load,
    },
  });

  if (!result.ok) {
    return { id: req.id, ok: false, error: result.error };
  }

  return {
    id: req.id,
    ok: true,
    data: {
      session_id: session.value.session_id,
      url: session.value.url,
      title: session.value.title,
    },
  };
}

async function forwardToContent(req: Request): Promise<Response> {
  const session = sessionFromRequest(req);
  if (!session.ok) {
    return { id: req.id, ok: false, error: session.error };
  }

  const contentReq: ContentRequest = {
    type: req.action as ContentRequest['type'],
    params: {
      ...req.params,
      session_id: session.value.session_id,
      request_id: req.id,
    },
  };

  const result =
    req.action === 'click'
      ? await raceClickWithContextChange(session.value.tab_id, () =>
          sendToContent(session.value.tab_id, contentReq),
        )
      : await raceWithNavigation(session.value.tab_id, () =>
          sendToContent(session.value.tab_id, contentReq),
        );
  if (!result.ok) {
    return { id: req.id, ok: false, error: result.error };
  }

  if (req.action === 'click' && result.data?.opened_new_target === true) {
    const targetTabId = result.data.target_tab_id;
    if (typeof targetTabId !== 'number') {
      return { id: req.id, ok: false, error: 'Missing target tab for click-opened session' };
    }

    await ensureTabLoaded(targetTabId);

    const newSession = await createTrackedSession(targetTabId, req.id);
    const snapshotResult = await requestSessionSnapshot(newSession, req.id);
    if (!snapshotResult.ok) {
      sessions.delete(newSession.session_id);
      return { id: req.id, ok: false, error: snapshotResult.error };
    }

    const updatedTargetTab = await chrome.tabs.get(targetTabId);
    newSession.url = updatedTargetTab.url ?? newSession.url;
    newSession.title = updatedTargetTab.title ?? newSession.title;

    return {
      id: req.id,
      ok: true,
      data: {
        action: 'click',
        changed: true,
        navigated: false,
        opened_new_session: true,
        new_session_id: newSession.session_id,
        url: newSession.url,
        title: newSession.title,
      },
    };
  }

  const navigated = result.data && (result.data as Record<string, unknown>).navigated === true;

  // If navigation occurred, the old content script's snapshot is gone.
  // Request a fresh snapshot from the new page's content script.
  if (navigated) {
    await sendToContent(session.value.tab_id, {
      type: 'snapshot',
      params: {
        session_id: session.value.session_id,
        request_id: req.id,
      },
    });
  }

  const updatedTab = await chrome.tabs.get(session.value.tab_id);
  session.value.url = updatedTab.url ?? session.value.url;
  session.value.title = updatedTab.title ?? session.value.title;

  return {
    id: req.id,
    ok: true,
    data: result.data,
  };
}

async function handleScreenshot(req: Request): Promise<Response> {
  const session = sessionFromRequest(req);
  if (!session.ok) {
    return { id: req.id, ok: false, error: session.error };
  }

  await ensureTabLoaded(session.value.tab_id);

  const quality = typeof req.params.quality === 'number' ? req.params.quality : undefined;
  const format: 'png' | 'jpeg' = quality !== undefined ? 'jpeg' : 'png';
  const options: Parameters<typeof chrome.tabs.captureVisibleTab>[1] = { format };
  if (quality !== undefined) {
    options.quality = quality;
  }

  // Get the window ID for the session's tab
  const tab = await chrome.tabs.get(session.value.tab_id);
  if (!tab.windowId) {
    return { id: req.id, ok: false, error: 'Could not determine window for tab' };
  }

  // Record the currently active tab so we can restore focus after capture
  const [previousTab] = await chrome.tabs.query({ active: true, windowId: tab.windowId });

  // Ensure the tab is active in its window before capturing
  await chrome.tabs.update(session.value.tab_id, { active: true });

  const dataUrl = await chrome.tabs.captureVisibleTab(tab.windowId, options);
  const base64Data = dataUrl.split(',')[1];

  // Restore previously active tab if it was different
  if (previousTab && previousTab.id !== undefined && previousTab.id !== tab.id) {
    await chrome.tabs.update(previousTab.id, { active: true });
  }

  return {
    id: req.id,
    ok: true,
    data: {
      image: base64Data,
      format: format,
    },
  };
}

function sessionFromRequest(
  req: Request,
): { ok: true; value: Session } | { ok: false; error: string } {
  const sessionId = req.params.session_id;
  if (typeof sessionId !== 'string' || sessionId.length === 0) {
    return { ok: false, error: 'session_id is required' };
  }
  const session = sessions.get(sessionId);
  if (!session) {
    return { ok: false, error: `Session not found: ${sessionId}` };
  }
  return { ok: true, value: session };
}

function optionalSessionId(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function generateSessionId(): string {
  return `s${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

async function createTrackedSession(
  tabId: number,
  requestId: string,
  sessionId = generateSessionId(),
): Promise<Session> {
  const tab = await chrome.tabs.get(tabId);
  const session: Session = {
    session_id: sessionId,
    tab_id: tabId,
    url: tab.url ?? '',
    title: tab.title ?? '',
    created_at: Date.now(),
    status: tab.status === 'loading' ? 'loading' : 'active',
  };
  sessions.set(sessionId, session);

  await sendToContent(tabId, {
    type: 'presence_start',
    params: {
      session_id: sessionId,
      request_id: requestId,
    },
  });

  return session;
}

function requestSessionSnapshot(
  session: Session,
  requestId: string,
  waitAfterLoad?: unknown,
): Promise<ContentResponse> {
  return sendToContent(session.tab_id, {
    type: 'snapshot',
    params: {
      session_id: session.session_id,
      request_id: requestId,
      wait_after_load: waitAfterLoad,
    },
  });
}

async function sendToContent(
  tabId: number,
  request: ContentRequest,
): Promise<ContentResponse> {
  try {
    return await chrome.tabs.sendMessage(tabId, request);
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

function waitForTabLoad(tabId: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      chrome.tabs.onUpdated.removeListener(listener);
      reject(new Error('Tab load timeout'));
    }, 30_000);

    const listener = (
      updatedTabId: number,
      changeInfo: chrome.tabs.OnUpdatedInfo,
    ) => {
      if (updatedTabId === tabId && changeInfo.status === 'complete') {
        clearTimeout(timeout);
        chrome.tabs.onUpdated.removeListener(listener);
        resolve();
      }
    };

    chrome.tabs.onUpdated.addListener(listener);
  });
}

// Waits for tab to reach 'complete' status, checking current state first to
// avoid missing the event if the tab is already loaded.
function ensureTabLoaded(tabId: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      chrome.tabs.onUpdated.removeListener(listener);
      reject(new Error('Tab load timeout'));
    }, 30_000);

    const listener = (updatedTabId: number, changeInfo: chrome.tabs.OnUpdatedInfo) => {
      if (updatedTabId === tabId && changeInfo.status === 'complete') {
        clearTimeout(timeout);
        chrome.tabs.onUpdated.removeListener(listener);
        resolve();
      }
    };

    chrome.tabs.onUpdated.addListener(listener);

    chrome.tabs.get(tabId).then((tab) => {
      if (tab.status === 'complete') {
        clearTimeout(timeout);
        chrome.tabs.onUpdated.removeListener(listener);
        resolve();
      }
    });
  });
}

// Races a content script call against tab navigation. If the tab starts loading
// a new URL before the content script responds (e.g. bfcache freeze), resolves
// immediately with navigated:true instead of hanging indefinitely.
function raceWithNavigation(
  tabId: number,
  fn: () => Promise<ContentResponse>,
): Promise<ContentResponse> {
  return new Promise((resolve) => {
    let settled = false;

    const onUpdated = (updatedTabId: number, changeInfo: chrome.tabs.OnUpdatedInfo) => {
      if (updatedTabId !== tabId || settled) return;
      if (changeInfo.status === 'loading' || changeInfo.url) {
        settled = true;
        chrome.tabs.onUpdated.removeListener(onUpdated);
        // Wait for the new page to finish loading, then resolve.
        // The caller (forwardToContent) will see navigated:true but NO
        // snapshot from the old page.  The Relay cache is invalidated and
        // the CLI will get a fresh snapshot on the next get_page.
        waitForTabLoad(tabId)
          .then(() => chrome.tabs.get(tabId))
          .then((tab) => {
            resolve({
              ok: true,
              data: {
                navigated: true,
                changed: true,
                url: tab.url ?? changeInfo.url ?? '',
                title: tab.title ?? '',
              },
            });
          })
          .catch(() => {
            resolve({
              ok: true,
              data: { navigated: true, changed: true, url: changeInfo.url ?? '' },
            });
          });
      }
    };

    chrome.tabs.onUpdated.addListener(onUpdated);

    fn().then((result) => {
      if (!settled) {
        settled = true;
        chrome.tabs.onUpdated.removeListener(onUpdated);
        resolve(result);
      }
    }).catch((err: unknown) => {
      if (!settled) {
        settled = true;
        chrome.tabs.onUpdated.removeListener(onUpdated);
        resolve({ ok: false, error: err instanceof Error ? err.message : String(err) });
      }
    });
  });
}

function raceClickWithContextChange(
  sourceTabId: number,
  fn: () => Promise<ContentResponse>,
): Promise<ContentResponse> {
  return new Promise((resolve) => {
    let settled = false;
    let responseTimer: ReturnType<typeof setTimeout> | null = null;

    const cleanup = () => {
      chrome.tabs.onUpdated.removeListener(onUpdated);
      chrome.webNavigation.onCreatedNavigationTarget.removeListener(onCreatedTarget);
      if (responseTimer !== null) {
        clearTimeout(responseTimer);
        responseTimer = null;
      }
    };

    const settle = (result: ContentResponse) => {
      if (settled) {
        return;
      }
      settled = true;
      cleanup();
      resolve(result);
    };

    const onUpdated = (updatedTabId: number, changeInfo: chrome.tabs.OnUpdatedInfo) => {
      if (updatedTabId !== sourceTabId || settled) return;
      if (changeInfo.status === 'loading' || changeInfo.url) {
        waitForTabLoad(sourceTabId)
          .then(() => chrome.tabs.get(sourceTabId))
          .then((tab) => {
            settle({
              ok: true,
              data: {
                navigated: true,
                changed: true,
                url: tab.url ?? changeInfo.url ?? '',
                title: tab.title ?? '',
              },
            });
          })
          .catch(() => {
            settle({
              ok: true,
              data: { navigated: true, changed: true, url: changeInfo.url ?? '' },
            });
          });
      }
    };

    const onCreatedTarget = (details: chrome.webNavigation.WebNavigationSourceCallbackDetails) => {
      if (settled || details.sourceTabId !== sourceTabId) {
        return;
      }
      settle({
        ok: true,
        data: {
          changed: true,
          navigated: false,
          opened_new_target: true,
          target_tab_id: details.tabId,
          url: details.url,
        },
      });
    };

    chrome.tabs.onUpdated.addListener(onUpdated);
    chrome.webNavigation.onCreatedNavigationTarget.addListener(onCreatedTarget);

    fn()
      .then((result) => {
        if (settled) {
          return;
        }
        responseTimer = setTimeout(() => settle(result), CLICK_TARGET_GRACE_MS);
      })
      .catch((err: unknown) => {
        settle({ ok: false, error: err instanceof Error ? err.message : String(err) });
      });
  });
}

function isChunkEvent(message: unknown): message is ChunkEvent {
  if (!message || typeof message !== 'object') {
    return false;
  }
  const value = message as { type?: unknown; chunk?: unknown };
  return value.type === 'page_chunk' && typeof value.chunk === 'object';
}

chrome.tabs.onRemoved.addListener((tabId) => {
  for (const [sessionId, session] of sessions) {
    if (session.tab_id === tabId) {
      sessions.delete(sessionId);
      break;
    }
  }
});

chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  for (const session of sessions.values()) {
    if (session.tab_id !== tabId) {
      continue;
    }
    if (changeInfo.url) {
      session.url = changeInfo.url;
    }
    if (changeInfo.title) {
      session.title = changeInfo.title;
    }
    if (changeInfo.status === 'loading') {
      session.status = 'loading';
    }
    if (changeInfo.status === 'complete') {
      session.status = 'active';
      void sendToContent(session.tab_id, {
        type: 'presence_start',
        params: {
          session_id: session.session_id,
          request_id: `presence-${Date.now()}`,
        },
      });
    }
  }
});
