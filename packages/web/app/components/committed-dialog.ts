import { useLayoutEffect, useState, type RefObject } from "react";

// Commit operation-specific fields before native dialog autofocus runs. Opening
// inside the click handler can focus an old field that React then replaces.
export function useCommittedDialog(
  dialog: RefObject<HTMLDialogElement | null>,
) {
  const [sequence, setSequence] = useState(0);
  useLayoutEffect(() => {
    const element = dialog.current;
    if (sequence && element && !element.open) element.showModal();
  }, [sequence, dialog]);
  return () => setSequence((current) => current + 1);
}
