import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { I18nProvider } from "./i18n";

const actions = vi.hoisted(() => ({ setup: vi.fn(), openSite: vi.fn(), startProject: vi.fn() }));

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
    projectSetup: (id: string) => actions.setup(id) ?? demoRpc("sites.setup", { id }),
    openSite: actions.openSite,
    startProject: actions.startProject,
    syncHosts: async () => {
      await demoRpc("hosts.sync", {});
    },
    systemInfo: () => demoRpc("system.info", {}),
    certificateStatus: async () => ({ exists: true, trusted: true }),
    setTrayLabels: async () => {},
    launchAtLogin: async () => false,
  };
});

beforeEach(() => vi.resetAllMocks());

describe("workspace navigation", () => {
  it("rechecks preparation before opening even if the displayed project was ready", async () => {
    const ready = {
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
    actions.setup.mockResolvedValue(ready);
    render(
      <I18nProvider locale="en">
        <App />
      </I18nProvider>,
    );
    fireEvent.click(await screen.findByRole("button", { name: /shop.*Open project/ }));
    const open = await screen.findByRole("button", { name: "Open" });
    actions.setup.mockResolvedValue({ ...ready, node_installed: false });
    fireEvent.click(open);
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("Complete project preparation before opening"),
    );
    expect(actions.openSite).not.toHaveBeenCalled();
  });
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
    expect(await screen.findByRole("button", { name: "Continue to project" })).toBeInTheDocument();
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
