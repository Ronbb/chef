import test from "node:test";
import assert from "node:assert/strict";
import { productSessionCookie } from "../app/lib/product-session.ts";

test("SSR forwards only the configured product's exact session cookie", () => {
  const headers = new Headers({
    cookie:
      "tracking=omit; brioche.sid=b; hargow.sid=h; __Host-hargow.sid.extra=omit",
  });
  assert.equal(productSessionCookie(headers, "brioche"), "brioche.sid=b");
  assert.equal(productSessionCookie(headers, "hargow"), "hargow.sid=h");
  assert.equal(
    productSessionCookie(new Headers({ cookie: "brioche.sid=b" }), "hargow"),
    "",
  );
  assert.equal(
    productSessionCookie(
      new Headers({ cookie: "__Host-hargow.sid=secure" }),
      "hargow",
    ),
    "__Host-hargow.sid=secure",
  );
  assert.equal(productSessionCookie(new Headers(), "brioche"), "");
  assert.equal(
    productSessionCookie(
      new Headers({
        cookie:
          "hargow.sidX; __Host-hargow.sidX; hargow.sid.extra=omit; hargow.sid =omit",
      }),
      "hargow",
    ),
    "",
  );
});

test("ambiguous or invalid product sessions are rejected before service forwarding", () => {
  for (const cookie of [
    "hargow.sid=a; hargow.sid=b",
    "hargow.sid=a; __Host-hargow.sid=b",
    "hargow.sid=",
    'hargow.sid="quoted"',
    "hargow.sid=space inside",
    `hargow.sid=${"a".repeat(4097)}`,
  ]) {
    assert.throws(
      () => productSessionCookie(new Headers({ cookie }), "hargow"),
      (error: unknown) => error instanceof Response && error.status === 401,
    );
  }
  assert.throws(
    () => productSessionCookie(new Headers(), "other" as "brioche"),
    /Invalid product/,
  );
});
