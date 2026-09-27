import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n";
import { UpdateButton } from "./UpdateButton";

const api = vi.hoisted(() => ({
  checkUpdate: vi.fn(),
  downloadUpdate: vi.fn(),
  installUpdate: vi.fn(),
  onCheckUpdateRequest: vi.fn(() => () => {}),
}));

vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  ...api,
}));

function renderButton() {
  // New callbacks on every render, like App passes them.
  const view = (key: number) => (
    <I18nProvider locale="en">
      <UpdateButton key="update" onError={() => key} onUpToDate={() => key} />
    </I18nProvider>
  );
  const result = render(view(0));
  return { rerender: (key: number) => result.rerender(view(key)) };
}

describe("UpdateButton", () => {
  it("checks once, downloads with progress and installs on Restart", async () => {
    api.checkUpdate.mockResolvedValue({ version: "0.3.1", notes: "- **ui:** New", date: null });
    api.downloadUpdate.mockImplementation(async (onProgress: (progress: object) => void) => {
      onProgress({ downloaded: 50, total: 100 });
    });
    api.installUpdate.mockResolvedValue(undefined);
    const { rerender } = renderButton();

    const button = await screen.findByRole("button", { name: "Update to Werd 0.3.1" });
    rerender(1);
    rerender(2);
    expect(api.checkUpdate).toHaveBeenCalledTimes(1);

    await act(async () => fireEvent.click(button));
    const restart = await screen.findByRole("button", { name: "Restart" });
    // App re-renders every few seconds; that must not check again and drop the download.
    rerender(3);
    rerender(4);
    expect(api.checkUpdate).toHaveBeenCalledTimes(1);

    fireEvent.click(restart);
    await waitFor(() => expect(api.installUpdate).toHaveBeenCalledTimes(1));
  });
});
