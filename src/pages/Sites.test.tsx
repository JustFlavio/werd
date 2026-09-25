import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Project } from "../api";
import { Sites } from "./Sites";

const noop = () => {};

function renderSites(projects: Project[], overrides: Partial<Parameters<typeof Sites>[0]> = {}) {
  return render(
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
    />,
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
    expect(screen.getByRole("heading", { name: "Nessun sito" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Aggiungi sito/ })).toBeInTheDocument();
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
    fireEvent.click(screen.getByRole("button", { name: /Avvia/ }));
    expect(onToggle).toHaveBeenCalledWith(blog);
  });

  it("submits the trimmed folder path from the add dialog", async () => {
    const onAdd = vi.fn(async () => true);
    renderSites([], { onAdd });
    fireEvent.click(screen.getByRole("button", { name: /Aggiungi sito/ }));
    const dialog = screen.getByRole("dialog", { name: "Aggiungi sito" });
    fireEvent.change(dialog.querySelector("input") as HTMLInputElement, { target: { value: "  /work/shop  " } });
    fireEvent.click(screen.getByRole("button", { name: "Aggiungi" }));
    expect(onAdd).toHaveBeenCalledWith("/work/shop");
  });
});
