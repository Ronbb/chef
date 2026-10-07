// The notice contains no account, token or lesson data. Other tabs reload their
// server-authorized route rather than trying to reuse another tab's identity.
export const identityNoticeKey = "brioche.identity-change.v1";
export function announceIdentityChange() {
  try {
    localStorage.setItem(identityNoticeKey, crypto.randomUUID());
  } catch {
    // Focus/visibility checks still work when browser storage is unavailable.
  }
}
export type Identity = { id: string; role: string } | null;
export function sameIdentity(a: Identity, b: Identity) {
  return a?.id === b?.id && a?.role === b?.role;
}

export function watchIdentity(options: {
  identity: Identity;
  window: EventTarget;
  document: EventTarget;
  visible: () => boolean;
  read: (signal: AbortSignal) => Promise<Identity>;
  invalidate: () => void;
  stopPlayback: () => void;
  every: (callback: () => void) => () => void;
}) {
  let disposed = false,
    invalidated = false;
  let pending: AbortController | null = null;
  function invalidate() {
    if (disposed || invalidated) return;
    invalidated = true;
    pending?.abort();
    options.stopPlayback();
    options.invalidate();
  }
  async function check() {
    if (disposed || invalidated || pending || !options.visible()) return;
    const request = new AbortController();
    pending = request;
    try {
      const identity = await options.read(request.signal);
      if (
        !disposed &&
        !request.signal.aborted &&
        !sameIdentity(identity, options.identity)
      )
        invalidate();
    } catch {
      // A transport failure does not prove a change of account. Retry on the
      // next visible check; private operations still enforce server permission.
    } finally {
      if (pending === request) pending = null;
    }
  }
  function storage(event: Event) {
    const notice = event as StorageEvent;
    if (notice.key === identityNoticeKey && notice.newValue) invalidate();
  }
  function pageShow(event: Event) {
    if ((event as PageTransitionEvent).persisted) invalidate();
    else void check();
  }
  const pageHide = () => options.stopPlayback();
  const refresh = () => void check();
  options.window.addEventListener("storage", storage);
  options.window.addEventListener("focus", refresh);
  options.window.addEventListener("pageshow", pageShow);
  options.window.addEventListener("pagehide", pageHide);
  options.document.addEventListener("visibilitychange", refresh);
  const cancelTimer = options.every(refresh);
  // Covers an auth change between SSR and effect subscription.
  void check();
  return () => {
    disposed = true;
    pending?.abort();
    cancelTimer();
    options.window.removeEventListener("storage", storage);
    options.window.removeEventListener("focus", refresh);
    options.window.removeEventListener("pageshow", pageShow);
    options.window.removeEventListener("pagehide", pageHide);
    options.document.removeEventListener("visibilitychange", refresh);
  };
}
