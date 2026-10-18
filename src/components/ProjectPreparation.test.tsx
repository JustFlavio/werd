import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project, ProjectSetup } from "../api";
import { I18nProvider } from "../i18n";
import { useProjectPreparation } from "../projectPreparation";
import type { Runtimes } from "../runtimes";
import { ProjectPreparation } from "./ProjectPreparation";

const api = vi.hoisted(() => ({
  projectSetup: vi.fn(),
  certificateStatus: vi.fn(),
  previewProjectEnv: vi.fn(),
  applyProjectEnv: vi.fn(),
  installProjectDependencies: vi.fn(),
  installRuntime: vi.fn(),
  trustCa: vi.fn(),
  openSite: vi.fn(),
}));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), ...api }));
const project: Project = { id: "shop", name: "shop", path: "/work/shop", php: "8.4", status: "stopped" };
const status: ProjectSetup = {
  env_pending: 0,
  env_exists: true,
  env_error: null,
  vendor_installed: true,
  app_key_present: true,
  configuration_cached: false,
  frontend: false,
  frontend_installed: true,
  frontend_built: true,
  node_required: null,
  node_installed: true,
};
const runtimes: Runtimes = {
  rows: [
    {
      product: "php",
      label: "PHP",
      kind: "runtime",
      line: "8.4",
      latest: "8.4.1",
      installed: "8.4.1",
      is_default: true,
      update_available: false,
      lts: false,
      eol: null,
    },
  ],
  jobs: [],
  jobFor: () => undefined,
  install: vi.fn(),
  update: vi.fn(),
  uninstall: vi.fn(),
  setDefault: vi.fn(),
  track: vi.fn(),
  refresh: async () => {},
};
function Preparation() {
  const checks = useProjectPreparation(project, runtimes);
  return <ProjectPreparation project={project} runtimes={runtimes} checks={checks} onChanged={async () => {}} />;
}
const show = () =>
  render(
    <I18nProvider locale="en">
      <Preparation />
    </I18nProvider>,
  );
beforeEach(() => {
  vi.resetAllMocks();
  api.projectSetup.mockResolvedValue(status);
  api.certificateStatus.mockResolvedValue({ exists: true, trusted: true });
  api.previewProjectEnv.mockResolvedValue({
    revision: "first",
    exists: true,
    changes: [
      { key: "DB_HOST", before: "remote", after: "127.0.0.1" },
      { key: "DB_PASSWORD", before: "old-secret", after: "new-secret" },
    ],
  });
});
describe("project preparation", () => {
  it("never writes environment values before the preview is approved and masks credentials", async () => {
    api.projectSetup.mockResolvedValue({ ...status, env_pending: 2 });
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Review .env changes" }));
    const dialog = within(await screen.findByRole("dialog"));
    expect(api.applyProjectEnv).not.toHaveBeenCalled();
    expect(dialog.getByText("127.0.0.1")).toBeInTheDocument();
    expect(dialog.queryByText("new-secret")).not.toBeInTheDocument();
    fireEvent.click(dialog.getByRole("button", { name: "Cancel" }));
    expect(api.applyProjectEnv).not.toHaveBeenCalled();
  });
  it("requires a fresh preview after concurrent changes and applies only the approved revision", async () => {
    api.projectSetup.mockResolvedValue({ ...status, env_pending: 1 });
    api.applyProjectEnv
      .mockRejectedValueOnce(new Error("Environment values changed since the preview"))
      .mockResolvedValue({ backup: "/private/original.env" });
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Review .env changes" }));
    const dialog = within(await screen.findByRole("dialog"));
    fireEvent.click(dialog.getByRole("button", { name: "Apply these changes" }));
    expect(await dialog.findByRole("alert")).toHaveTextContent("changed since the preview");
    expect(api.applyProjectEnv).toHaveBeenCalledWith("shop", "first");
    api.previewProjectEnv.mockResolvedValue({
      revision: "second",
      exists: true,
      changes: [{ key: "DB_HOST", before: "edited", after: "127.0.0.1" }],
    });
    fireEvent.click(dialog.getByRole("button", { name: "Refresh preview" }));
    await dialog.findByText("edited");
    fireEvent.click(dialog.getByRole("button", { name: "Apply these changes" }));
    await waitFor(() => expect(api.applyProjectEnv).toHaveBeenLastCalledWith("shop", "second"));
    expect(await screen.findByText("/private/original.env")).toBeInTheDocument();
  });
  it("offers missing dependency installation without exposing a terminal command", async () => {
    api.projectSetup.mockResolvedValue({ ...status, vendor_installed: false });
    api.installProjectDependencies.mockResolvedValue({ id: "dependencies", state: "running" });
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Install application dependencies" }));
    await waitFor(() => expect(api.installProjectDependencies).toHaveBeenCalledWith("shop", "composer"));
    expect(runtimes.track).toHaveBeenCalledWith(expect.objectContaining({ id: "dependencies" }));
  });
});
