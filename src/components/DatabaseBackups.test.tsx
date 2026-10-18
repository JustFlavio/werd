import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DatabaseBackup, Job, ServiceInstance } from "../api";
import { I18nProvider } from "../i18n";
import { DatabaseBackups } from "./DatabaseBackups";

const api = vi.hoisted(() => ({
  backupDatabase: vi.fn(),
  restoreDatabase: vi.fn(),
  databaseBackups: vi.fn(),
  listJobs: vi.fn(),
}));
vi.mock("../api", async (original) => ({ ...(await original<typeof import("../api")>()), ...api }));

const instance: ServiceInstance = {
  id: "pg",
  name: "PostgreSQL",
  product: "postgresql",
  line: "18",
  port: 5432,
  autostart: false,
  status: "running",
};
const backup: DatabaseBackup = {
  schema: 1,
  id: "snapshot",
  instance: "pg",
  service_name: "PostgreSQL",
  product: "postgresql",
  line: "18",
  database: "shop",
  bytes: 1024,
  created_at: 1000,
  sha256: "a".repeat(64),
};
const job: Job = {
  id: "job",
  product: "postgresql",
  line: "18",
  action: "backup",
  state: "running",
  downloaded: 0,
  total: null,
  step: "Exporting database",
  error: null,
  started_at: 1000,
};
const renderBackups = (service = instance) =>
  render(
    <I18nProvider locale="en">
      <DatabaseBackups instance={service} onClose={() => {}} />
    </I18nProvider>,
  );

beforeEach(() => {
  vi.resetAllMocks();
  api.listJobs.mockResolvedValue([]);
  api.databaseBackups.mockResolvedValue([backup]);
});

describe("DatabaseBackups", () => {
  it("limits a project backup to its linked database and engine", async () => {
    api.databaseBackups.mockResolvedValue([
      backup,
      { ...backup, id: "other-db", database: "billing" },
      { ...backup, id: "other-engine", instance: "pg-other" },
    ]);
    api.listJobs.mockResolvedValue([]);
    api.backupDatabase.mockResolvedValue({ ...job, state: "done" });
    render(
      <I18nProvider locale="en">
        <DatabaseBackups instance={instance} projectDatabase="shop" onClose={() => {}} />
      </I18nProvider>,
    );
    await screen.findByRole("option", { name: /shop/ });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(screen.getByLabelText("Database to back up")).toHaveValue("shop");
    expect(screen.getByLabelText("Database to back up")).toHaveAttribute("readonly");
    expect(screen.queryByText(/operation continues/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Create backup" }));
    await waitFor(() => expect(api.backupDatabase).toHaveBeenCalledWith("pg", "shop"));
  });

  it("blocks concurrent engine work without attributing another project's operation to this project", async () => {
    api.listJobs.mockResolvedValue([job]);
    render(
      <I18nProvider locale="en">
        <DatabaseBackups instance={instance} projectDatabase="shop" onClose={() => {}} />
      </I18nProvider>,
    );
    await screen.findByText(/busy with another operation/);
    expect(screen.getByRole("button", { name: "Create backup" })).toBeDisabled();
    expect(screen.queryByText(/operation continues/)).not.toBeInTheDocument();
    api.listJobs.mockResolvedValue([]);
    await waitFor(() => expect(screen.getByRole("button", { name: "Create backup" })).toBeEnabled());
  });

  it("shows a new backup only after its job has completed", async () => {
    api.databaseBackups.mockResolvedValueOnce([]).mockResolvedValue([backup]);
    api.backupDatabase.mockImplementation(async () => {
      api.listJobs.mockResolvedValue([job]);
      return job;
    });
    renderBackups();
    await screen.findByText("No compatible backups yet.");
    fireEvent.change(screen.getByLabelText("Database to back up"), { target: { value: "shop" } });
    fireEvent.click(screen.getByRole("button", { name: "Create backup" }));
    await screen.findByText(/operation continues/);
    expect(screen.queryByText("Backup completed.")).not.toBeInTheDocument();
    api.listJobs.mockResolvedValue([{ ...job, state: "done", result: backup.id }]);
    await screen.findByText("Backup completed.");
    await screen.findByRole("option", { name: /shop/ });
    expect(api.backupDatabase).toHaveBeenCalledWith("pg", "shop");
  });

  it("keeps an existing-target refusal visible and does not report success", async () => {
    api.restoreDatabase.mockRejectedValue(new Error("database already exists"));
    renderBackups();
    await screen.findByRole("option", { name: /shop/ });
    fireEvent.change(screen.getByLabelText("New database name"), { target: { value: "existing" } });
    fireEvent.click(screen.getByRole("button", { name: "Restore into a new database" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("database already exists");
    expect(api.restoreDatabase).toHaveBeenCalledWith("pg", "snapshot", "existing");
    expect(screen.queryByText(/Restored into existing/)).not.toBeInTheDocument();
  });

  it("filters incompatible engines/lines and disables operations on stopped services", async () => {
    api.databaseBackups.mockResolvedValue([
      backup,
      { ...backup, id: "old", line: "17", database: "old_db" },
      { ...backup, id: "mysql", product: "mysql", database: "mysql_db" },
    ]);
    renderBackups({ ...instance, status: "stopped" });
    await screen.findByRole("option", { name: /shop/ });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    fireEvent.change(screen.getByLabelText("Database to back up"), { target: { value: "shop" } });
    fireEvent.change(screen.getByLabelText("New database name"), { target: { value: "recovered" } });
    expect(screen.getByRole("button", { name: "Create backup" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Restore into a new database" })).toBeDisabled();
  });

  it("rediscovers a running job when reopened and shows its failure", async () => {
    api.listJobs.mockResolvedValue([job]);
    renderBackups();
    await screen.findByText(/operation continues/);
    expect(screen.getByRole("button", { name: "Create backup" })).toBeDisabled();
    api.listJobs.mockResolvedValue([{ ...job, state: "failed", error: "Database tool failed" }]);
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Database tool failed"));
  });

  it("explains a lost job after a daemon restart instead of waiting forever", async () => {
    api.listJobs.mockResolvedValue([job]);
    renderBackups();
    await screen.findByText(/operation continues/);
    api.listJobs.mockResolvedValue([]);
    expect(await screen.findByRole("alert")).toHaveTextContent("no longer reported by the daemon");
    expect(screen.queryByText(/operation continues/)).not.toBeInTheDocument();
  });
});
