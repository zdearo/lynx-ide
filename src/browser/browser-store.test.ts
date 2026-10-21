import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands, type BrowserState, type EngineStatus } from "@/bindings";
import { STOPPED, useBrowserStore } from "@/browser/browser-store";
import { failure, ok } from "@/test-support";

vi.mock("@/bindings", () => ({
  commands: {
    browserOpen: vi.fn(),
    browserNavigate: vi.fn(),
    browserDetach: vi.fn(),
    browserAttach: vi.fn(),
    browserEngineStatus: vi.fn(),
    browserEngineInstall: vi.fn(),
    browserEngineRemove: vi.fn(),
  },
}));

const api = vi.mocked(commands);
const initialState = useBrowserStore.getState();

const running: BrowserState = {
  ...STOPPED,
  status: "running",
  url: "https://example.com/",
  title: "Example",
};

beforeEach(() => {
  vi.resetAllMocks();
  useBrowserStore.setState(initialState, true);
});

describe("opening the view", () => {
  it("starts the engine the first time it is shown", async () => {
    api.browserOpen.mockResolvedValue(ok(running));

    useBrowserStore.getState().setOpen(true);
    await vi.waitFor(() =>
      expect(useBrowserStore.getState().state.status).toBe("running"),
    );

    expect(api.browserOpen).toHaveBeenCalledWith(null);
    expect(useBrowserStore.getState().open).toBe(true);
  });

  it("does not start a second engine when it already runs", () => {
    useBrowserStore.setState({ state: running });

    useBrowserStore.getState().setOpen(true);

    expect(api.browserOpen).not.toHaveBeenCalled();
  });

  it("does not start twice while a start is still in flight", async () => {
    let finish: (value: ReturnType<typeof ok<BrowserState>>) => void = () => {};
    api.browserOpen.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );

    const first = useBrowserStore.getState().start();
    void useBrowserStore.getState().start();
    expect(api.browserOpen).toHaveBeenCalledTimes(1);

    finish(ok(running));
    await first;
    expect(useBrowserStore.getState().starting).toBe(false);
  });

  it("closes without touching the engine", () => {
    useBrowserStore.setState({ open: true, state: running });
    useBrowserStore.getState().toggle();
    expect(useBrowserStore.getState().open).toBe(false);
    expect(api.browserOpen).not.toHaveBeenCalled();
  });
});

describe("a missing engine", () => {
  it("keeps the kind so the view can say what to do", async () => {
    api.browserOpen.mockResolvedValue(
      failure("browserEngineMissing", "not installed"),
    );

    await useBrowserStore.getState().start();

    expect(useBrowserStore.getState().failure).toEqual({
      kind: "browserEngineMissing",
      message: "not installed",
    });
    expect(useBrowserStore.getState().starting).toBe(false);
  });

  it("clears the failure when a start works", async () => {
    useBrowserStore.setState({
      failure: { kind: "browser", message: "old" },
    });
    api.browserOpen.mockResolvedValue(ok(running));

    await useBrowserStore.getState().start();

    expect(useBrowserStore.getState().failure).toBeNull();
  });
});

describe("the address bar", () => {
  it("sends what was typed while the engine runs", async () => {
    useBrowserStore.setState({ state: running });
    api.browserNavigate.mockResolvedValue(ok(null));

    await useBrowserStore.getState().navigate("example.org");

    expect(api.browserNavigate).toHaveBeenCalledWith("example.org");
    expect(api.browserOpen).not.toHaveBeenCalled();
  });

  it("starts the engine with the address when it is not running", async () => {
    api.browserOpen.mockResolvedValue(ok(running));

    await useBrowserStore.getState().navigate("example.org");

    expect(api.browserOpen).toHaveBeenCalledWith("example.org");
    expect(api.browserNavigate).not.toHaveBeenCalled();
  });

  it("shows why an address was refused", async () => {
    useBrowserStore.setState({ state: running });
    api.browserNavigate.mockResolvedValue(
      failure("browser", "enter an http or https address"),
    );

    await useBrowserStore.getState().navigate("file:///etc/passwd");

    expect(useBrowserStore.getState().failure?.message).toBe(
      "enter an http or https address",
    );
  });
});

describe("state from the engine", () => {
  it("replaces the state the interface shows", () => {
    useBrowserStore.getState().apply(running);
    expect(useBrowserStore.getState().state.title).toBe("Example");
  });
});

describe("a window of its own", () => {
  it("asks the backend to move the page, and does not guess the result", async () => {
    useBrowserStore.setState({ state: running });
    api.browserDetach.mockResolvedValue(ok(null));

    await useBrowserStore.getState().detach();

    expect(api.browserDetach).toHaveBeenCalledOnce();
    // Where the page is shown arrives as an event, from the backend that knows.
    expect(useBrowserStore.getState().state.detached).toBe(false);
  });

  it("follows the backend when the page is detached and brought back", () => {
    useBrowserStore.getState().apply({ ...running, detached: true });
    expect(useBrowserStore.getState().state.detached).toBe(true);

    useBrowserStore.getState().apply({ ...running, detached: false });
    expect(useBrowserStore.getState().state.detached).toBe(false);
  });

  it("shows why the page could not be moved", async () => {
    api.browserDetach.mockResolvedValue(
      failure("browser", "the browser is not running"),
    );

    await useBrowserStore.getState().detach();

    expect(useBrowserStore.getState().failure?.message).toBe(
      "the browser is not running",
    );
  });

  it("brings the page back through the backend", async () => {
    api.browserAttach.mockResolvedValue(ok(null));

    await useBrowserStore.getState().attach();

    expect(api.browserAttach).toHaveBeenCalledOnce();
  });
});

describe("an agent using the browser", () => {
  it("says who is in control and what they do, without opening the browser", () => {
    useBrowserStore.setState({ state: { ...running, page: 1 } });

    useBrowserStore
      .getState()
      .noteAgentAction(1, "Claude Code", "Opening example.com");

    expect(useBrowserStore.getState().open).toBe(false);
    expect(useBrowserStore.getState().agentAction).toEqual({
      agent: "Claude Code",
      summary: "Opening example.com",
    });
  });

  it("stops saying it when the backend lets go", () => {
    useBrowserStore.setState({ state: { ...running, page: 1 } });
    useBrowserStore.getState().noteAgentAction(1, "Codex", "Typing");

    useBrowserStore.getState().releaseAgent();

    expect(useBrowserStore.getState().agentAction).toBeNull();
  });

  it("clears the mark by itself if the release never comes", () => {
    vi.useFakeTimers();
    useBrowserStore.setState({ state: { ...running, page: 1 } });

    useBrowserStore.getState().noteAgentAction(1, "Codex", "Typing");
    vi.advanceTimersByTime(41_000);

    expect(useBrowserStore.getState().agentAction).toBeNull();
    vi.useRealTimers();
  });
});

describe("the browser engine", () => {
  const ready: EngineStatus = {
    state: "ready",
    version: "154.0.34-p2",
    path: "/home/dev/.lynx/engine/current/lynx-browser-host",
  };
  const missing: EngineStatus = { ...ready, state: "missing", version: null };

  it("reads what is installed", async () => {
    api.browserEngineStatus.mockResolvedValue(ok(missing));

    await useBrowserStore.getState().loadEngine();

    expect(useBrowserStore.getState().engine?.state).toBe("missing");
  });

  it("installs, clears the progress and starts the browser the person was waiting for", async () => {
    useBrowserStore.setState({
      open: true,
      engineProgress: { phase: "unpacking", received: 0, total: 0 },
    });
    api.browserEngineInstall.mockResolvedValue(ok(ready));
    api.browserOpen.mockResolvedValue(ok(running));

    await useBrowserStore.getState().installEngine();

    const state = useBrowserStore.getState();
    expect(state.engine).toEqual(ready);
    expect(state.engineProgress).toBeNull();
    expect(state.engineBusy).toBe(false);
    expect(api.browserEngineInstall).toHaveBeenCalledWith(false);
    await vi.waitFor(() => expect(api.browserOpen).toHaveBeenCalled());
  });

  it("does not start a browser nobody opened", async () => {
    api.browserEngineInstall.mockResolvedValue(ok(ready));

    await useBrowserStore.getState().installEngine(true);

    expect(api.browserEngineInstall).toHaveBeenCalledWith(true);
    expect(api.browserOpen).not.toHaveBeenCalled();
  });

  it("keeps the reason when the download fails", async () => {
    api.browserEngineInstall.mockResolvedValue(
      failure("browser", "could not reach github.com"),
    );

    await useBrowserStore.getState().installEngine();

    const state = useBrowserStore.getState();
    expect(state.engineError).toBe("could not reach github.com");
    expect(state.engineBusy).toBe(false);
  });

  it("ignores a second install while one runs", async () => {
    let finish: (value: ReturnType<typeof ok<EngineStatus>>) => void = () => {};
    api.browserEngineInstall.mockReturnValue(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );

    const first = useBrowserStore.getState().installEngine();
    await useBrowserStore.getState().installEngine();
    finish(ok(ready));
    await first;

    expect(api.browserEngineInstall).toHaveBeenCalledTimes(1);
  });

  it("removes the engine", async () => {
    useBrowserStore.setState({ engine: ready });
    api.browserEngineRemove.mockResolvedValue(ok(missing));

    await useBrowserStore.getState().removeEngine();

    expect(useBrowserStore.getState().engine?.state).toBe("missing");
  });

  it("shows the progress it is given", () => {
    useBrowserStore
      .getState()
      .applyEngineProgress({ phase: "downloading", received: 5, total: 10 });

    expect(useBrowserStore.getState().engineProgress?.received).toBe(5);
  });
});
