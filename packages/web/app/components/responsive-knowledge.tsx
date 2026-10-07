import { useEffect, useRef, useState, type ReactNode } from "react";

/** Keep one mounted knowledge view: a desktop sidebar or a mobile modal drawer. */
export function ResponsiveKnowledge({
  open,
  onDismiss,
  labelledBy,
  children,
}: {
  open: boolean;
  onDismiss: () => void;
  labelledBy: string;
  children: ReactNode;
}) {
  const [mobile, setMobile] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const retained = useRef(children);
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  if (open) retained.current = children;
  useEffect(() => {
    const query = matchMedia("(max-width: 960px)");
    const update = () => setMobile(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (mobile && open) {
      if (!element.open) element.showModal();
      element.removeAttribute("data-closing");
    } else if (element.open) {
      if (!mobile || matchMedia("(prefers-reduced-motion: reduce)").matches) {
        element.close();
        return;
      }
      element.setAttribute("data-closing", "true");
      const finish = () => {
        element.removeAttribute("data-closing");
        element.close();
      };
      const ended = (event: AnimationEvent) => {
        if (
          event.target === element &&
          event.animationName === "knowledge-leave"
        )
          finish();
      };
      element.addEventListener("animationend", ended);
      const timer = setTimeout(finish, 280);
      return () => {
        clearTimeout(timer);
        element.removeEventListener("animationend", ended);
        element.removeAttribute("data-closing");
      };
    }
  }, [mobile, open]);
  return (
    <>
      <aside className="knowledge" aria-labelledby={labelledBy}>
        {!mobile && children}
      </aside>
      <dialog
        ref={dialog}
        className="knowledge knowledge-sheet"
        aria-labelledby={labelledBy}
        onCancel={(event) => {
          event.preventDefault();
          dismiss.current();
        }}
        onClose={() => {
          if (mobile && open && !dialog.current?.open) onDismiss();
        }}
        onClick={(event) => {
          const element = dialog.current;
          if (!element || event.target !== element) return;
          const rect = element.getBoundingClientRect();
          if (
            event.clientX < rect.left ||
            event.clientX > rect.right ||
            event.clientY < rect.top ||
            event.clientY > rect.bottom
          )
            dismiss.current();
        }}
      >
        {mobile && (open ? children : retained.current)}
      </dialog>
    </>
  );
}
