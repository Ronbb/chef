import test from "node:test";
import assert from "node:assert/strict";
import { accountReturnPath } from "../app/lib/account-return.ts";

test("login returns to pending confirmation and existing private destinations", () => {
  for (const path of [
    "/pending-saves",
    "/reviews",
    "/library",
    "/review-history",
    "/learning/session-123",
    "/lessons/a1-bakery-buy-breakfast",
  ])
    assert.equal(accountReturnPath(path), path);
});

test("login return rejects external, encoded, nested and unrelated destinations", () => {
  for (const path of [
    null,
    "",
    "https://example.test",
    "//example.test",
    "/\\example.test",
    "%2F%2Fexample.test",
    "/%2fexample.test",
    "javascript:alert(1)",
    "/pending-saves/other",
    "/pending-saves?next=//example.test",
    "/learning/../profile",
    "/lessons/a%2fb",
    "/author-preview",
    "/api/v1/me",
  ])
    assert.equal(accountReturnPath(path), "/profile");
});
