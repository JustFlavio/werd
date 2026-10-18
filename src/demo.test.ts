import { describe, expect, it } from "vitest";
import type { Project, Snapshot } from "./api";
import { demoRpc } from "./demo";

describe("demo site domains", () => {
  it.each([
    [" Shop ", "shop.test"],
    ["API.my-shop.TEST", "api.my-shop.test"],
    ["a".repeat(63), `${"a".repeat(63)}.test`],
    [Array(4).fill("a".repeat(61)).join("."), `${Array(4).fill("a".repeat(61)).join(".")}.test`],
  ])("normalizes valid input %s", async (input, domain) => {
    const project = (await demoRpc("sites.domain", { id: "shop", domain: input })) as Project;
    expect(project.domain).toBe(domain);
    expect(project.url).toBe(`https://${domain}`);
  });

  it.each([
    "",
    "-shop",
    "shop-",
    "shop..api",
    "shop/api",
    "shop_api",
    "a".repeat(64),
    Array(4).fill("a".repeat(62)).join("."),
    `-a${"--".repeat(8)}!`,
  ])("rejects invalid input without changing the site: %s", async (domain) => {
    const before = (await demoRpc("sites.list", {})) as Snapshot;
    const previous = before.projects.find((project) => project.id === "shop")?.domain;
    await expect(demoRpc("sites.domain", { id: "shop", domain })).rejects.toThrow("not a valid .test domain");
    const after = (await demoRpc("sites.list", {})) as Snapshot;
    expect(after.projects.find((project) => project.id === "shop")?.domain).toBe(previous);
  });
});
