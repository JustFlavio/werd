import { describe, expect, it } from "vitest";
import type { Project, ProjectSetup, RuntimeLine } from "./api";
import { needsPreparationBeforeStart, preparationIssues } from "./projectPreparation";

const project: Project = {
  id: "app",
  name: "app",
  path: "/app",
  php: "8.4",
  status: "stopped",
  vite: { available: true, status: "stopped", autostart: false },
};
const setup: ProjectSetup = {
  env_pending: 0,
  env_exists: true,
  env_error: null,
  vendor_installed: true,
  app_key_present: true,
  configuration_cached: false,
  frontend: true,
  frontend_installed: true,
  frontend_built: true,
  node_required: "24",
  node_installed: true,
};
const rows = [{ product: "php", line: "8.4", installed: "8.4.1" }] as RuntimeLine[];
const certificate = { exists: true, trusted: true };
describe("shared project preparation", () => {
  it.each([
    [{ vendor_installed: false }, "composer"],
    [{ env_exists: false }, "environment"],
    [{ env_pending: 2 }, "environment"],
    [{ env_error: "Unrecognized file" }, "environment"],
    [{ app_key_present: false }, "key"],
    [{ configuration_cached: true }, "config"],
    [{ node_installed: false }, "node"],
    [{ frontend_installed: false }, "frontend"],
  ] as const)("requires preparation for %j", (change, issue) => {
    const issues = preparationIssues(project, { ...setup, ...change }, certificate, rows);
    expect(issues).toContain(issue);
    expect(needsPreparationBeforeStart(issues, certificate)).toBe(true);
  });
  it("lets the first server start generate the HTTPS certificate", () => {
    const ca = { exists: false, trusted: false };
    const issues = preparationIssues(project, setup, ca, rows);
    expect(issues).toEqual(["https"]);
    expect(needsPreparationBeforeStart(issues, ca)).toBe(false);
    expect(needsPreparationBeforeStart(issues, { exists: true, trusted: false })).toBe(true);
  });
  it("accepts live assets scheduled for startup but verifies them again after the server starts", () => {
    const site: Project = { ...project, vite: { available: true, status: "stopped", autostart: true } };
    expect(preparationIssues(site, { ...setup, frontend_built: false }, certificate, rows)).toEqual([]);
    expect(
      preparationIssues({ ...site, status: "running" }, { ...setup, frontend_built: false }, certificate, rows),
    ).toContain("assets");
  });
});
