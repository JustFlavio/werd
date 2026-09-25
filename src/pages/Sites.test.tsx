import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Project } from "../api";
import { I18nProvider, type Locale } from "../i18n";
import { Sites } from "./Sites";

const noop = () => {};

function renderSites(projects: Project[], overrides: Partial<Parameters<typeof Sites>[0]> = {}, locale: Locale = "en") {
  return render(
    <I18nProvider locale={locale}>
      <Sites
        projects={projects}
        busy={null}
        selectedId={null}
        onSelect={noop}
        onAdd={async () => true}
        onToggle={noop}
        onOpenSite={noop}
        onOpenUrl={noop}
        onResetPorts={noop}
        onShowLogs={noop}
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
  services: ["postgres"],
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

  it("shows details and a start action for the selected stopped site", () => {
    const onToggle = vi.fn();
    renderSites([blog], { selectedId: "blog", onToggle });
    expect(screen.getByText("PostgreSQL")).toBeInTheDocument();
    expect(screen.getByText("Available once started")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Start/ }));
    expect(onToggle).toHaveBeenCalledWith(blog);
  });

  it("submits the trimmed folder path from the add dialog", () => {
    const onAdd = vi.fn(async () => true);
    renderSites([], { onAdd });
    fireEvent.click(screen.getByRole("button", { name: /Add site/ }));
    const dialog = screen.getByRole("dialog", { name: "Add site" });
    fireEvent.change(dialog.querySelector("input") as HTMLInputElement, { target: { value: "  /work/shop  " } });
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
