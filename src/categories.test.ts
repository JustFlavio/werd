import { describe, expect, it } from "vitest";
import type { ServiceInstance } from "./api";
import { CATEGORIES, CATEGORY_PRODUCTS, instancesFor } from "./categories";

const instance = (product: string): ServiceInstance => ({
  id: product,
  name: product,
  product,
  line: "1",
  port: 1,
  autostart: false,
  status: "stopped",
});

describe("categories", () => {
  it("every category has at least one product", () => {
    for (const category of CATEGORIES) expect(CATEGORY_PRODUCTS[category].length).toBeGreaterThan(0);
  });

  it("filters instances by what the category accepts", () => {
    const all = ["postgresql", "redis", "mailpit", "mysql"].map(instance);
    expect(instancesFor("database", all).map((item) => item.product)).toEqual(["postgresql", "mysql"]);
    expect(instancesFor("queue", all).map((item) => item.product)).toEqual(["redis"]);
    expect(instancesFor("search", all)).toEqual([]);
  });
});
