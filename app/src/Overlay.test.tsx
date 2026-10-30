import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Overlay from "./Overlay";

const invoke = vi.hoisted(() => vi.fn());
const handlers = vi.hoisted(() => new Map<string, (event: { payload: unknown }) => void>());

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "overlay-0" }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
    handlers.set(name, handler);
    return () => handlers.delete(name);
  },
}));

const card = {
  target: "overlay-0",
  lines: ["GO DRINK", "WATER"],
  sizePx: 180,
  seconds: 4,
  floorMs: 600,
};

describe("Overlay", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invoke.mockReset();
    invoke.mockResolvedValue(undefined);
    handlers.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("draws only the card addressed to its own layer", async () => {
    render(<Overlay />);
    await Promise.resolve();
    await Promise.resolve();

    act(() => handlers.get("card")?.({ payload: { ...card, target: "overlay-1" } }));
    expect(screen.queryByText("GO DRINK")).not.toBeInTheDocument();

    act(() => handlers.get("card")?.({ payload: card }));
    expect(screen.getByText("GO DRINK")).toBeInTheDocument();
    expect(screen.getByText("WATER")).toHaveStyle({ fontSize: "180px" });
  });

  it("ends the card after its Window and gives input back", async () => {
    render(<Overlay />);
    await Promise.resolve();
    await Promise.resolve();

    act(() => handlers.get("card")?.({ payload: card }));
    act(() => vi.advanceTimersByTime(3999));
    expect(invoke).not.toHaveBeenCalledWith("card_end", expect.anything());

    act(() => vi.advanceTimersByTime(1));
    expect(invoke).toHaveBeenCalledWith("card_end", { label: "overlay-0" });
    expect(screen.queryByText("GO DRINK")).not.toBeInTheDocument();
  });

  it("clears on the watchdog event without calling back", async () => {
    render(<Overlay />);
    await Promise.resolve();
    await Promise.resolve();

    act(() => handlers.get("card")?.({ payload: card }));
    act(() => handlers.get("card_clear")?.({ payload: undefined }));

    expect(screen.queryByText("GO DRINK")).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("card_end", expect.anything());
  });
});
