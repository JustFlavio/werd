import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { en, type Messages } from "./en";
import { I18nProvider, initialLocale, useI18n } from "./index";
import { it as italian } from "./it";

/** Every leaf key of a messages tree, e.g. "sites.add". */
function keys(tree: object, prefix = ""): string[] {
  return Object.entries(tree).flatMap(([key, value]) =>
    typeof value === "object" && value !== null ? keys(value, `${prefix}${key}.`) : [`${prefix}${key}`],
  );
}

describe("dictionaries", () => {
  it("Italian covers exactly the English keys", () => {
    expect(keys(italian).sort()).toEqual(keys(en).sort());
  });

  it("interpolates values", () => {
    const messages: Messages = italian;
    expect(messages.dashboard.sitesSummary(1, 0)).toBe("1 sito collegato, 0 in esecuzione.");
    expect(en.dashboard.sitesSummary(3, 2)).toBe("3 linked sites, 2 running.");
  });
});

describe("locale selection", () => {
  afterEach(() => localStorage.clear());

  it("prefers the saved choice, then falls back to English", () => {
    localStorage.setItem("werd.locale", "it");
    expect(initialLocale()).toBe("it");
    localStorage.setItem("werd.locale", "klingon");
    expect(initialLocale()).toBe(navigator.language.startsWith("it") ? "it" : "en");
  });

  it("switches language and remembers it", () => {
    function Probe() {
      const { t, setLocale } = useI18n();
      return (
        <button type="button" onClick={() => setLocale("it")}>
          {t.nav.sites}
        </button>
      );
    }
    render(
      <I18nProvider locale="en">
        <Probe />
      </I18nProvider>,
    );
    const button = screen.getByRole("button", { name: "Sites" });
    act(() => button.click());
    expect(screen.getByRole("button", { name: "Siti" })).toBeInTheDocument();
    expect(localStorage.getItem("werd.locale")).toBe("it");
    expect(document.documentElement.lang).toBe("it");
  });
});
