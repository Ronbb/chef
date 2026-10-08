import { productNamespace } from "../lib/product-runtime";
import { useCallback, useEffect, useSyncExternalStore } from "react";
import { draftsChangedEventFor } from "../lib/learning-draft";
import { pendingOwned } from "../lib/owned-draft";

const draftsChangedEvent = draftsChangedEventFor(productNamespace);
function subscribe(notify: () => void) {
  window.addEventListener(draftsChangedEvent, notify);
  window.addEventListener("storage", notify);
  return () => {
    window.removeEventListener(draftsChangedEvent, notify);
    window.removeEventListener("storage", notify);
  };
}
const serverSnapshot = () => false;

// Read validated, owner-scoped requests instead of mounted control state. A
// collapsed card can unmount its writer while its original request is pending.
export function usePendingOwnedWrites(userId?: string) {
  const snapshot = useCallback(
    () => !!userId && pendingOwned(userId, productNamespace).length > 0,
    [userId],
  );
  const active = useSyncExternalStore(subscribe, snapshot, serverSnapshot);
  useEffect(() => {
    if (!active) return;
    function warn(event: BeforeUnloadEvent) {
      event.preventDefault();
      event.returnValue = "";
    }
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [active]);
  return active;
}
