import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Settings from "./Settings";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

type Command = string;

const state = {
  configPath: "C:\\Users\\me\\threshold\\cues.toml",
  changedOnDisk: false,
  floorMs: 600,
  volume: 0.8,
  overlay: "permanent",
  cues: [
    {
      text: "GO DRINK WATER",
      at: "every 50m",
      repeat: "",
      late: "",
      seconds: 4,
      sting: 0,
      enabled: true,
    },
  ],
  next: [new Date(Date.now() + 10 * 60_000).toISOString()],
  trouble: null,
};

function mockCommands(overrides: Partial<Record<Command, unknown>> = {}) {
  invoke.mockImplementation(async (command: Command) => {
    if (command in overrides) return overrides[command];
    if (command === "get_state") return state;
    if (command === "save_state" || command === "reload_config" || command === "preview_cue") return undefined;
    if (command === "starter_cues") return [state.cues[0]];
    throw new Error(`unexpected command ${command}`);
  });
}

describe("Settings", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("renders the loaded overlay mode instead of a blank select", async () => {
    mockCommands();
    render(<Settings />);

    expect(await screen.findByDisplayValue("GO DRINK WATER")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: /overlay/i })).toHaveValue("permanent");
  });

  it("saves the overlay mode it was told about", async () => {
    mockCommands();
    const user = userEvent.setup();
    render(<Settings />);

    await screen.findByDisplayValue("GO DRINK WATER");
    await user.click(screen.getByRole("button", { name: "save" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_state",
        expect.objectContaining({
          overlay: "permanent",
          floorMs: 600,
          volume: 0.8,
          force: false,
        }),
      ),
    );
    expect(invoke.mock.calls.find(([c]) => c === "save_state")?.[1].cues[0].text).toBe(
      "GO DRINK WATER",
    );
  });

  it("cannot save before the state has loaded", async () => {
    invoke.mockImplementation(() => new Promise(() => {}));
    render(<Settings />);

    expect(screen.getByRole("button", { name: "save" })).toBeDisabled();
  });

  it("appends starter cues to the draft without saving over it", async () => {
    mockCommands({
      starter_cues: [
        { text: "THRESHOLD", at: "08:00", repeat: "weekdays", late: "", seconds: 4, sting: 0, enabled: true },
        { text: "GO DRINK WATER", at: "every 50m", repeat: "", late: "", seconds: 4, sting: 0, enabled: true },
      ],
    });
    const user = userEvent.setup();
    render(<Settings />);

    await screen.findByDisplayValue("GO DRINK WATER");
    await user.click(screen.getByRole("button", { name: "examples" }));

    expect(await screen.findByDisplayValue("THRESHOLD")).toBeInTheDocument();
    expect(screen.getAllByDisplayValue("GO DRINK WATER")).toHaveLength(1);
    expect(invoke).not.toHaveBeenCalledWith("save_state", expect.anything());
  });

  it("shows the conflict banner and retries with force when asked", async () => {
    invoke.mockImplementation(async (command: Command, args?: { force?: boolean }) => {
      if (command === "get_state") return state;
      if (command === "save_state") {
        if (!args?.force) throw "cues.toml changed on disk since it was loaded";
        return undefined;
      }
      return undefined;
    });
    const user = userEvent.setup();
    render(<Settings />);

    await screen.findByDisplayValue("GO DRINK WATER");
    await user.click(screen.getByRole("button", { name: "save" }));
    const alerts = await screen.findAllByRole("alert");
    expect(alerts.some((a) => a.textContent?.includes("changed on disk"))).toBe(true);

    await user.click(screen.getByRole("button", { name: /overwrite/i }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_state",
        expect.objectContaining({ force: true }),
      ),
    );
  });
});
