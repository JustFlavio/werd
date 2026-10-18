import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Job, RuntimeLine } from "../api";
import { I18nProvider } from "../i18n";
import type { Runtimes } from "../runtimes";
import { AddSiteWizard } from "./AddSiteWizard";

const api = vi.hoisted(() => ({
  listRuntimes: vi.fn(),
  listJobs: vi.fn(),
  installRuntime: vi.fn(),
  createProject: vi.fn(),
  getSettings: vi.fn(),
  domainsStatus: vi.fn(),
  certificateStatus: vi.fn(),
  trustCa: vi.fn(),
  startProject: vi.fn(),
  openSite: vi.fn(),
}));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), ...api }));
const php: RuntimeLine = {
  product: "php",
  label: "PHP",
  kind: "runtime",
  line: "8.4",
  installed: "8.4.1",
  latest: "8.4.1",
  is_default: true,
  update_available: false,
  lts: false,
  eol: null,
};
const node: RuntimeLine = {
  ...php,
  product: "node",
  label: "Node.js",
  line: "24",
  installed: null,
  latest: "24.0.0",
  is_default: false,
  lts: true,
};
const runtimes: Runtimes = {
  rows: [php, node],
  jobs: [],
  jobFor: () => undefined,
  install: vi.fn(),
  update: vi.fn(),
  uninstall: vi.fn(),
  setDefault: vi.fn(),
  track: vi.fn(),
  refresh: async () => {},
};
const job: Job = {
  id: "node-download",
  product: "node",
  line: "24",
  action: "install",
  state: "done",
  step: "Done",
  started_at: 0,
  downloaded: 0,
  total: null,
  error: null,
};
async function wizard(onClose = vi.fn()) {
  render(
    <I18nProvider locale="en">
      <AddSiteWizard runtimes={runtimes} onClose={onClose} onDone={async () => {}} />
    </I18nProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: /New Laravel project/ }));
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), { target: { value: "my-app" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Location" }), { target: { value: "/work" } });
  return within(screen.getByRole("dialog"));
}
beforeEach(() => {
  vi.resetAllMocks();
  api.getSettings.mockResolvedValue({ parked: [] });
  api.domainsStatus.mockResolvedValue({ missing: [] });
  api.listRuntimes.mockResolvedValue([php, node]);
  api.listJobs.mockResolvedValue([]);
  localStorage.clear();
});
describe("first project wizard", () => {
  it("continues to project preparation without starting or opening the app from the wizard", async () => {
    api.listRuntimes.mockResolvedValue([php, { ...node, installed: "24.0.0" }]);
    api.listJobs.mockResolvedValue([{ ...job, id: "app-created", product: "laravel", result: "app" }]);
    api.createProject.mockResolvedValue({ ...job, id: "app-created", product: "laravel", result: "app" });
    api.startProject.mockResolvedValue({ id: "app", status: "running" });
    api.certificateStatus
      .mockResolvedValueOnce({ exists: true, trusted: false })
      .mockResolvedValueOnce({ exists: true, trusted: false })
      .mockResolvedValue({ exists: true, trusted: true });
    api.trustCa.mockResolvedValue("Trusted");
    api.openSite.mockResolvedValue("https://app.test");
    const onClose = vi.fn();
    const dialog = await wizard(onClose);
    fireEvent.click(dialog.getByRole("button", { name: "Create project" }));
    fireEvent.click(await dialog.findByRole("button", { name: "Continue to project" }));
    expect(api.trustCa).not.toHaveBeenCalled();
    expect(api.openSite).not.toHaveBeenCalled();
    expect(api.startProject).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledOnce();
  });
  it("installs Node before creating the app and keeps advanced choices collapsed", async () => {
    api.installRuntime.mockResolvedValue(job);
    api.listJobs.mockImplementation(async () =>
      api.installRuntime.mock.calls.length
        ? [job, { ...job, id: "app-created", product: "laravel", result: "app" }]
        : [],
    );
    api.createProject.mockResolvedValue({ ...job, id: "app-created", product: "laravel", result: "app" });
    const dialog = await wizard();
    expect(dialog.getByText("Customize").closest("details")).not.toHaveAttribute("open");
    fireEvent.click(dialog.getByRole("button", { name: "Create project" }));
    await dialog.findByText("Your application was created. Build something great!");
    expect(api.installRuntime).toHaveBeenCalledWith("node", "24");
    expect(api.createProject).toHaveBeenCalledWith(
      expect.objectContaining({ name: "my-app", directory: "/work", npm: true, php: "8.4" }),
    );
  });
  it("preserves choices after a failed Node download and allows retry", async () => {
    api.installRuntime.mockRejectedValue(new Error("Connection unavailable"));
    const dialog = await wizard();
    fireEvent.click(dialog.getByRole("button", { name: "Create project" }));
    fireEvent.click(await dialog.findByRole("button", { name: "Previous" }));
    expect(dialog.getByRole("textbox", { name: "Project name" })).toHaveValue("my-app");
    expect(dialog.getByRole("textbox", { name: "Location" })).toHaveValue("/work");
    fireEvent.click(dialog.getByRole("button", { name: "Create project" }));
    await waitFor(() => expect(api.installRuntime).toHaveBeenCalledTimes(2));
    expect(api.createProject).not.toHaveBeenCalled();
  });
});
