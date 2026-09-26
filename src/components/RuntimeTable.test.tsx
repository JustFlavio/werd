import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Job, RuntimeLine } from "../api";
import { I18nProvider } from "../i18n";
import type { Runtimes } from "../runtimes";
import { RuntimeTable } from "./RuntimeTable";

const line = (overrides: Partial<RuntimeLine>): RuntimeLine => ({
  product: "php",
  label: "PHP",
  kind: "runtime",
  line: "8.4",
  latest: "8.4.26",
  installed: null,
  update_available: false,
  is_default: false,
  lts: false,
  eol: null,
  ...overrides,
});

function fakeRuntimes(jobs: Job[] = []): Runtimes {
  return {
    rows: [],
    jobs,
    jobFor: (product, version) => jobs.find((job) => job.product === product && job.line === version),
    install: vi.fn(),
    update: vi.fn(),
    uninstall: vi.fn(),
    setDefault: vi.fn(),
    refresh: vi.fn(async () => {}),
  };
}

const renderTable = (rows: RuntimeLine[], runtimes: Runtimes) =>
  render(
    <I18nProvider locale="en">
      <RuntimeTable rows={rows} runtimes={runtimes} showDefault />
    </I18nProvider>,
  );

describe("RuntimeTable", () => {
  it("offers install for missing lines and update when a newer patch exists", () => {
    const runtimes = fakeRuntimes();
    const missing = line({ line: "8.3", latest: "8.3.35" });
    const outdated = line({ installed: "8.4.25", update_available: true });
    renderTable([missing, outdated], runtimes);

    expect(screen.getByText("(8.4.25)")).toBeInTheDocument();
    expect(screen.getByText("Update available: 8.4.26")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Install" }));
    expect(runtimes.install).toHaveBeenCalledWith(missing);
    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    expect(runtimes.update).toHaveBeenCalledWith(outdated);
    fireEvent.click(screen.getByRole("button", { name: "Make default" }));
    expect(runtimes.setDefault).toHaveBeenCalledWith(outdated);
  });

  it("shows progress while a job runs and disables actions", () => {
    const job: Job = {
      id: "j1",
      product: "php",
      line: "8.3",
      action: "install",
      state: "running",
      downloaded: 50,
      total: 200,
      step: "Downloading",
      error: null,
      started_at: 1,
    };
    renderTable([line({ line: "8.3" })], fakeRuntimes([job]));
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "25");
    expect(screen.getByRole("button", { name: "Install" })).toBeDisabled();
  });

  it("marks the default line and hides its make-default action", () => {
    renderTable([line({ installed: "8.4.26", is_default: true })], fakeRuntimes());
    expect(screen.getByText("Default")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Make default" })).not.toBeInTheDocument();
    expect(screen.getByText("Installed")).toBeInTheDocument();
  });
});
