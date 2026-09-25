import { describe, expect, it } from "vitest";
import type { Project } from "./api";
import { SERVICE_NAMES, SERVICES, serviceUrl } from "./services";

const project = (overrides: Partial<Project> = {}): Project => ({
  id: "shop",
  name: "shop",
  path: "/work/shop",
  php: "8.5",
  services: ["mailpit", "rustfs", "redis"],
  status: "running",
  ports: { mailpit_ui: 52016, rustfs_console: 52018, redis: 52014 },
  ...overrides,
});

describe("serviceUrl", () => {
  it("returns the loopback web UI of a running service", () => {
    expect(serviceUrl(project(), "mailpit")).toBe("http://127.0.0.1:52016");
    expect(serviceUrl(project(), "rustfs")).toBe("http://127.0.0.1:52018");
  });

  it("returns null for services without a web UI", () => {
    expect(serviceUrl(project(), "redis")).toBeNull();
  });

  it("returns null when the site is not running or the port is unknown", () => {
    expect(serviceUrl(project({ status: "stopped" }), "mailpit")).toBeNull();
    expect(serviceUrl(project({ ports: undefined }), "mailpit")).toBeNull();
  });
});

describe("SERVICES", () => {
  it("declares a UI port that is part of each service's ports", () => {
    for (const name of SERVICE_NAMES) {
      const { ui, ports } = SERVICES[name];
      if (ui) expect(ports).toContain(ui);
    }
  });
});
