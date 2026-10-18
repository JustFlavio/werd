import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Project } from "../api";
import { I18nProvider } from "../i18n";
import type { Runtimes } from "../runtimes";
import { Dashboard } from "./Dashboard";

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
const project: Project = {
  id: "shop",
  name: "shop",
  path: "/work/shop",
  php: "8.4",
  domain: "shop.test",
  status: "running",
  links: { database: { instance: "pg", database: "shop" } },
};

describe("project home", () => {
  it("opens the selected project and shows only its configured capabilities", () => {
    const onSelect = vi.fn();
    render(
      <I18nProvider locale="en">
        <Dashboard projects={[project]} runtimes={runtimes} onSelect={onSelect} onAdd={vi.fn()} onError={vi.fn()} />
      </I18nProvider>,
    );
    expect(screen.getByText("1 project · 1 running")).toBeInTheDocument();
    expect(screen.getByText("Database")).toBeInTheDocument();
    expect(screen.queryByText("Cache")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Stop all/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /shop.*Open project/ }));
    expect(onSelect).toHaveBeenCalledWith(project);
  });

  it("offers project creation from the empty state", () => {
    const onAdd = vi.fn();
    render(
      <I18nProvider locale="it">
        <Dashboard projects={[]} runtimes={runtimes} onSelect={vi.fn()} onAdd={onAdd} onError={vi.fn()} />
      </I18nProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Aggiungi progetto" }));
    expect(onAdd).toHaveBeenCalledOnce();
  });

  it("keeps stopped status visible when configuration is incomplete", () => {
    render(
      <I18nProvider locale="en">
        <Dashboard
          projects={[{ ...project, status: "stopped", requirements: [{ category: "cache", product: "redis" }] }]}
          runtimes={runtimes}
          onSelect={vi.fn()}
          onAdd={vi.fn()}
          onError={vi.fn()}
        />
      </I18nProvider>,
    );
    expect(screen.getByText("Setup required")).toBeInTheDocument();
    expect(screen.getByText("Stopped")).toBeInTheDocument();
    expect(screen.queryByText("Running")).not.toBeInTheDocument();
  });
});
