// Only known account destinations are accepted, never an arbitrary URL.
export function accountReturnPath(next: string | null): string {
  return next &&
    (["/reviews", "/library", "/review-history", "/pending-saves"].includes(
      next,
    ) ||
      /^\/(?:learning|lessons)\/[a-zA-Z0-9_-]+$/.test(next))
    ? next
    : "/profile";
}
