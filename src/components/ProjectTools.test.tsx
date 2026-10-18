import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project, ServiceInstance, ServiceOffering } from "../api";
import { I18nProvider } from "../i18n";
import type { Runtimes } from "../runtimes";
import { ProjectTools } from "./ProjectTools";

const api = vi.hoisted(() => ({
  createService: vi.fn(),
  linkProject: vi.fn(),
  serviceCatalog: vi.fn(),
  startService: vi.fn(),
  prepareService: vi.fn(),
  openUrl: vi.fn(),
}));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), ...api }));
const pg: ServiceInstance = {
  id: "pg",
  name: "Shared PostgreSQL",
  product: "postgresql",
  line: "18",
  port: 5432,
  autostart: false,
  status: "running",
};
const redis: ServiceInstance = { ...pg, id: "redis", name: "Redis", product: "redis", line: "8" };
const project: Project = {
  id: "shop",
  name: "shop",
  path: "/work/shop",
  php: "8.4",
  status: "stopped",
  links: { database: { instance: "pg", database: "shop" } },
};
const catalog: ServiceOffering[] = [
  {
    product: "postgresql",
    label: "PostgreSQL",
    categories: ["database"],
    default_port: 5432,
    extensions: ["pgvector"],
    lines: [
      { line: "18", latest: "18.1", lts: false, eol: null, installed: null },
      { line: "17", latest: "17.4", lts: false, eol: null, installed: null },
    ],
  },
  {
    product: "redis",
    label: "Redis",
    categories: ["cache", "queue"],
    default_port: 6379,
    extensions: [],
    lines: [{ line: "8", latest: "8.0", lts: false, eol: null, installed: null }],
  },
];
const runtimes: Runtimes = {
  rows: [],
  jobs: [],
  jobFor: () => undefined,
  install: vi.fn(),
  update: vi.fn(),
  uninstall: vi.fn(),
  setDefault: vi.fn(),
  track: vi.fn(),
  refresh: async () => {},
};
const onChanged = vi.fn(async () => {});
function show(site = project, others: Project[] = [], engines = [pg, redis]) {
  render(
    <I18nProvider locale="en">
      <ProjectTools
        project={site}
        projects={[site, ...others]}
        instances={engines}
        runtimes={runtimes}
        onChanged={onChanged}
        onError={vi.fn()}
      />
    </I18nProvider>,
  );
}
async function add() {
  fireEvent.click(screen.getByRole("button", { name: "Add to project" }));
  const dialog = screen.getByRole("dialog");
  await waitFor(() => expect(within(dialog).queryByText("Loading…")).not.toBeInTheDocument());
  return within(dialog);
}
beforeEach(() => {
  vi.resetAllMocks();
  api.serviceCatalog.mockResolvedValue(catalog);
  api.createService.mockResolvedValue({ instance: pg, job: null });
  api.linkProject.mockResolvedValue(project);
});

describe("project tools", () => {
  it("retries preparation on the same engine instead of creating another instance", async () => {
    api.prepareService.mockResolvedValue({ instance: pg, job: { id: "retry-download" } });
    show(project, [], [{ ...pg, status: "stopped" }]);
    fireEvent.click(screen.getByRole("button", { name: "Start engine" }));
    await waitFor(() => expect(api.prepareService).toHaveBeenCalledWith("pg"));
    expect(api.createService).not.toHaveBeenCalled();
    expect(runtimes.track).toHaveBeenCalledWith({ id: "retry-download" });
  });
  it("does not clear an incompatible Redis requirement by reusing the other connection", async () => {
    show({
      ...project,
      links: { queue: { instance: "redis" } },
      requirements: [{ category: "cache", product: "redis", line: "9" }],
    });
    const dialog = await add();
    fireEvent.change(dialog.getByRole("combobox", { name: "Category" }), { target: { value: "cache" } });
    expect(dialog.getByText(/does not satisfy werd.yml/)).toBeInTheDocument();
    expect(dialog.getByRole("button", { name: "Add to project" })).toBeDisabled();
    expect(api.linkProject).not.toHaveBeenCalled();
    expect(api.createService).not.toHaveBeenCalled();
  });
  it("shows configured tools and warns about sharing without exposing destructive engine actions", () => {
    show(project, [{ ...project, id: "blog", name: "blog" }]);
    expect(screen.getByRole("heading", { name: "Database" })).toBeInTheDocument();
    expect(screen.queryByText("Redis")).not.toBeInTheDocument();
    expect(screen.getByText(/Shared with blog/)).toBeInTheDocument();
    expect(screen.getByText("shop", { selector: ".tool-database" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Stop|Remove|Delete/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Database backups" })).toBeInTheDocument();
  });

  it("creates an engine for the project and retries a failed connection without duplicating it", async () => {
    api.linkProject.mockRejectedValueOnce(new Error("Connection failed"));
    show({ ...project, links: {} });
    const dialog = await add();
    fireEvent.click(dialog.getByRole("button", { name: "Add to project" }));
    expect(await dialog.findByRole("alert")).toHaveTextContent("Connection failed");
    expect(api.createService).toHaveBeenCalledWith({
      product: "postgresql",
      line: "18",
      name: "shop · Database",
      autostart: false,
      extensions: [],
    });
    fireEvent.click(dialog.getByRole("button", { name: "Retry connection" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.createService).toHaveBeenCalledOnce();
    expect(api.linkProject).toHaveBeenLastCalledWith("shop", "database", "pg");
    expect(onChanged).toHaveBeenCalledOnce();
  });

  it("reuses this project's cache engine for its queue", async () => {
    show({ ...project, links: { ...project.links, cache: { instance: "redis" } } });
    const dialog = await add();
    fireEvent.change(dialog.getByRole("combobox", { name: "Category" }), { target: { value: "queue" } });
    expect(dialog.getByText(/same Redis engine/)).toBeInTheDocument();
    fireEvent.click(dialog.getByRole("button", { name: "Add to project" }));
    await waitFor(() => expect(api.linkProject).toHaveBeenCalledWith("shop", "queue", "redis"));
    expect(api.createService).not.toHaveBeenCalled();
  });

  it("honors the version and extensions requested by werd.yml", async () => {
    show({
      ...project,
      links: {},
      requirements: [{ category: "database", product: "postgresql", line: "17", extensions: ["pgvector"] }],
    });
    const dialog = await add();
    fireEvent.click(dialog.getByRole("button", { name: "Add to project" }));
    await waitFor(() =>
      expect(api.createService).toHaveBeenCalledWith(expect.objectContaining({ line: "17", extensions: ["pgvector"] })),
    );
  });
});
