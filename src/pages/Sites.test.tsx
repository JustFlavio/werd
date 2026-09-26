import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Project, RuntimeLine, ServiceInstance } from "../api";
import { I18nProvider, type Locale } from "../i18n";
import type { Runtimes } from "../runtimes";
import { Sites, type SitesProps } from "./Sites";

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
        onAdd={async () => true}
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

  it("submits the trimmed folder path from the add dialog", () => {
    const onAdd = vi.fn(async () => true);
    renderSites([], { onAdd });
    fireEvent.click(screen.getByRole("button", { name: /Add site/ }));
    const dialog = screen.getByRole("dialog", { name: "Add site" });
    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "  /work/shop  " } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(onAdd).toHaveBeenCalledWith("/work/shop");
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
