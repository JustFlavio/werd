import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Project, ProjectInfo, RuntimeLine, ServiceInstance } from "../api";
import { I18nProvider, type Locale } from "../i18n";
import type { Runtimes } from "../runtimes";
import { Sites, type SitesProps } from "./Sites";

const info: ProjectInfo = {
  path: "/work/fleet-desk",
  name: "fleet-desk",
  laravel: true,
  php_constraint: "^8.2",
  suggested_php: "8.4",
  suggested_php_installed: true,
  php_packages: [
    { name: "laravel/framework", label: "Laravel", version: "v12.30.1" },
    { name: "filament/filament", label: "Filament", version: "v4.1.0" },
  ],
  js_packages: [],
  node: null,
  werd_yml: false,
  env_file: true,
};

const api = vi.hoisted(() => ({
  addProject: vi.fn(),
  inspectFolder: vi.fn(),
  siteInfo: vi.fn(),
  domainsStatus: vi.fn(),
}));

vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  ...api,
}));

const noop = () => {};

const php = (line: string): RuntimeLine => ({
  product: "php",
  label: "PHP",
  kind: "runtime",
  line,
  latest: `${line}.0`,
  installed: `${line}.0`,
  update_available: false,
  is_default: false,
  lts: false,
  eol: null,
});

const runtimes: Runtimes = {
  rows: [php("8.5"), php("8.4")],
  jobs: [],
  jobFor: () => undefined,
  install: noop,
  update: noop,
  uninstall: noop,
  setDefault: noop,
  track: noop,
  refresh: async () => {},
};

const postgres: ServiceInstance = {
  id: "pg",
  name: "PostgreSQL 18",
  product: "postgresql",
  line: "18",
  port: 5432,
  autostart: true,
  status: "running",
};

function renderSites(projects: Project[], overrides: Partial<SitesProps> = {}, locale: Locale = "en") {
  return render(
    <I18nProvider locale={locale}>
      <Sites
        projects={projects}
        instances={[postgres]}
        runtimes={runtimes}
        busy={null}
        selectedId={null}
        onSelect={noop}
        onAdded={async () => {}}
        onToggle={noop}
        onOpenSite={noop}
        onResetPorts={noop}
        onShowLogs={noop}
        onChanged={async () => {}}
        onError={noop}
        {...overrides}
      />
    </I18nProvider>,
  );
}

const blog: Project = {
  id: "blog",
  name: "blog",
  path: "/work/blog",
  php: "8.5",
  links: { database: { instance: "pg", database: "blog" } },
  status: "stopped",
};

describe("Sites", () => {
  it("shows an empty state with a call to action when there are no sites", () => {
    renderSites([]);
    expect(screen.getByRole("heading", { name: "No sites" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Add site/ })).toBeInTheDocument();
  });

  it("lists sites and reports the selection", () => {
    const onSelect = vi.fn();
    renderSites([blog], { onSelect });
    fireEvent.click(screen.getByRole("button", { name: /blog/ }));
    expect(onSelect).toHaveBeenCalledWith("blog");
  });

  it("shows the PHP version, linked services and a start action", () => {
    const onToggle = vi.fn();
    renderSites([blog], { selectedId: "blog", onToggle });
    expect(screen.getByRole("combobox", { name: "PHP" })).toHaveValue("8.5");
    fireEvent.click(screen.getByRole("tab", { name: "Services" }));
    expect(screen.getByRole("combobox", { name: "Database" })).toHaveValue("pg");
    expect(screen.getByRole("combobox", { name: "Cache" })).toHaveValue("");
    expect(screen.getByText("blog", { selector: "td" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Start/ }));
    expect(onToggle).toHaveBeenCalledWith(blog);
  });

  it("blocks starting while werd.yml asks for missing services", () => {
    const pending: Project = { ...blog, requirements: [{ category: "search", product: "meilisearch" }] };
    renderSites([pending], { selectedId: "blog" });
    expect(screen.getByText(/asks for services you do not have yet/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create missing services" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Start/ })).toBeDisabled();
  });

  it("links an existing project with the name and PHP read from it", async () => {
    api.inspectFolder.mockResolvedValue(info);
    api.addProject.mockResolvedValue({ ...blog, id: "fleet", name: "fleet-desk", domain: "fleet-desk.test" });
    api.domainsStatus.mockResolvedValue({ missing: [] });
    const onAdded = vi.fn(async () => {});
    renderSites([], { onAdded });
    fireEvent.click(screen.getByRole("button", { name: /Add site/ }));
    fireEvent.click(screen.getByRole("button", { name: /Link existing project/ }));

    const dialog = screen.getByRole("dialog", { name: "Link existing project" });
    const path = within(dialog).getByRole("textbox", { name: "Project path" });
    fireEvent.change(path, { target: { value: "/work/fleet-desk" } });
    fireEvent.blur(path);
    await waitFor(() => expect(within(dialog).getByRole("textbox", { name: "Site name" })).toHaveValue("fleet-desk"));
    expect(within(dialog).getByRole("combobox", { name: "PHP" })).toHaveValue("8.4");
    expect(within(dialog).getByText("composer.json requires PHP ^8.2")).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Add site" }));
    await waitFor(() => expect(onAdded).toHaveBeenCalledWith("fleet"));
    expect(api.addProject).toHaveBeenCalledWith("/work/fleet-desk", "fleet-desk", "8.4", true);
    expect(await within(dialog).findByText("Your site is linked and ready to start.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /Open in browser/ })).toBeInTheDocument();
  });

  it("offers the starter kits for a new project", () => {
    renderSites([]);
    fireEvent.click(screen.getByRole("button", { name: /Add site/ }));
    fireEvent.click(screen.getByRole("button", { name: /New Laravel project/ }));
    for (const kit of ["No starter kit", "React", "Vue", "Svelte", "Livewire", "Custom starter kit"]) {
      expect(screen.getByRole("button", { name: kit })).toBeInTheDocument();
    }
  });

  it("shows the project stack on the Information tab", async () => {
    api.siteInfo.mockResolvedValue(info);
    renderSites([blog], { selectedId: "blog" });
    fireEvent.click(screen.getByRole("tab", { name: "Information" }));
    expect(await screen.findByText("v12.30.1")).toBeInTheDocument();
    expect(screen.getByText("Filament")).toBeInTheDocument();
  });

  it("closes the add dialog with Escape", () => {
    renderSites([]);
    fireEvent.click(screen.getByRole("button", { name: /Add site/ }));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("renders in Italian", () => {
    renderSites([blog], { selectedId: "blog" }, "it");
    expect(screen.getByRole("heading", { name: "Siti" })).toBeInTheDocument();
    expect(screen.getByText("Disponibile dopo l’avvio")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Avvia/ })).toBeInTheDocument();
  });
});
