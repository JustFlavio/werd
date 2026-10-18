import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import { I18nProvider } from "./i18n";

vi.mock("./api", async (original) => {
  const api = await original<typeof import("./api")>();
  const { demoRpc } = await import("./demo");
  return {
    ...api,
    listProjects: () => demoRpc("sites.list", {}),
    listServices: () => demoRpc("services.list", {}),
    domainsStatus: () => demoRpc("domains.status", {}),
    listRuntimes: () => demoRpc("runtimes.list", {}),
    listJobs: () => demoRpc("jobs.list", {}),
    getSettings: () => demoRpc("settings.get", {}),
    inspectFolder: (path: string) => demoRpc("sites.inspect", { path }),
    addProject: (path: string, name: string, php: string) => demoRpc("sites.add", { path, name, php }),
    syncHosts: async () => {
      await demoRpc("hosts.sync", {});
    },
    systemInfo: () => demoRpc("system.info", {}),
    certificateStatus: async () => ({ exists: true, trusted: true }),
    setTrayLabels: async () => {},
    launchAtLogin: async () => false,
  };
});

describe("workspace navigation", () => {
  it("keeps the completion screen when a project is added from Home", async () => {
    render(
      <I18nProvider locale="en">
        <App />
      </I18nProvider>,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Add project" }));
    fireEvent.click(screen.getByRole("button", { name: /Link existing project/ }));
    const dialog = within(screen.getByRole("dialog"));
    fireEvent.change(dialog.getByRole("textbox", { name: "Project path" }), { target: { value: "/work/new-project" } });
    fireEvent.blur(dialog.getByRole("textbox", { name: "Project path" }));
    await dialog.findByDisplayValue("new-project");
    fireEvent.click(dialog.getByRole("button", { name: "Add project" }));
    expect(await screen.findByRole("button", { name: "Open in browser" })).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
  it("keeps infrastructure in advanced settings while preserving runtime and global service controls", async () => {
    render(
      <I18nProvider locale="en">
        <App />
      </I18nProvider>,
    );
    await screen.findByText("Your projects");
    const nav = within(screen.getByRole("navigation", { name: "Main navigation" }));
    expect(nav.getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
      "Home",
      "Projects",
      "Settings",
    ]);
    fireEvent.click(nav.getByRole("button", { name: "Settings" }));
    await screen.findByText("C:\\Users\\dev\\AppData\\Local\\Werd");
    expect(screen.getByRole("button", { name: "Shared infrastructure" }).closest("details")).not.toHaveAttribute(
      "open",
    );
    fireEvent.click(screen.getByText("Advanced settings and diagnostics"));
    fireEvent.click(screen.getByRole("button", { name: "Shared infrastructure" }));
    expect(screen.getByRole("heading", { name: "Services" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "← Settings" })).toBeInTheDocument();
    expect(nav.getByRole("button", { name: "Settings" })).toHaveAttribute("aria-current", "page");
  });
});
