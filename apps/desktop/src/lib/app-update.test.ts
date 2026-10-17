import { check, Update } from "@tauri-apps/plugin-updater";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { AppUpdate } from "./app-update.svelte";

vi.mock("@tauri-apps/plugin-updater", async (original) => ({
  ...(await original<typeof import("@tauri-apps/plugin-updater")>()),
  check: vi.fn(),
}));
vi.mock("./api", () => ({ api: { restartApp: vi.fn() } }));

function release() {
  const update = new Update({
    currentVersion: "0.1.0",
    rawJson: {},
    rid: 1,
    version: "0.2.0",
  });
  vi.spyOn(update, "download").mockResolvedValue();
  vi.spyOn(update, "install").mockResolvedValue();
  vi.spyOn(update, "close").mockResolvedValue();
  vi.mocked(check).mockResolvedValue(update);
  return update;
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.mocked(check).mockReset();
  vi.mocked(api.restartApp).mockReset().mockResolvedValue();
  vi.spyOn(console, "info").mockImplementation(() => undefined);
});

describe("app updates", () => {
  it("stays hidden without an update and retries a failed background check", async () => {
    const updater = new AppUpdate();
    vi.mocked(check)
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(null);
    await updater.check();
    expect(updater.status).toBe("idle");
    await updater.check();
    expect(updater.status).toBe("idle");
    expect(check).toHaveBeenCalledTimes(2);
  });

  it("downloads with progress, then waits for an explicit install before restarting", async () => {
    const update = release();
    const updater = new AppUpdate();
    await updater.check();
    expect(updater.status).toBe("available");
    expect(updater.version).toBe("0.2.0");
    vi.mocked(update.download).mockImplementation((onEvent) => {
      onEvent?.({ data: { contentLength: 100 }, event: "Started" });
      onEvent?.({ data: { chunkLength: 25 }, event: "Progress" });
      expect(updater.progress).toBe(25);
      return Promise.resolve();
    });
    await updater.download();
    expect(updater.status).toBe("ready");
    expect(update.install).not.toHaveBeenCalled();
    expect(api.restartApp).not.toHaveBeenCalled();
    await updater.install();
    expect(update.install).toHaveBeenCalledOnce();
    expect(api.restartApp).toHaveBeenCalledOnce();
  });

  it("supports downloads without a content length and allows failed downloads to retry", async () => {
    const update = release();
    const updater = new AppUpdate();
    await updater.check();
    vi.mocked(update.download).mockRejectedValueOnce(
      new Error("bad signature")
    );
    await expect(updater.download()).rejects.toThrow("bad signature");
    expect(updater.status).toBe("available");
    vi.mocked(update.download).mockImplementation((onEvent) => {
      onEvent?.({ data: {}, event: "Started" });
      onEvent?.({ data: { chunkLength: 100 }, event: "Progress" });
      expect(updater.progress).toBeNull();
      return Promise.resolve();
    });
    await updater.download();
    expect(updater.status).toBe("ready");
    expect(api.restartApp).not.toHaveBeenCalled();
  });

  it("allows a failed install to retry, and never restarts on failure", async () => {
    const update = release();
    const updater = new AppUpdate();
    await updater.check();
    await updater.download();
    vi.mocked(update.install).mockRejectedValueOnce(new Error("not writable"));
    await expect(updater.install()).rejects.toThrow("not writable");
    expect(updater.status).toBe("ready");
    expect(api.restartApp).not.toHaveBeenCalled();
    await updater.install();
    expect(update.download).toHaveBeenCalledOnce();
    expect(api.restartApp).toHaveBeenCalledOnce();
  });

  it("retries a failed restart without reinstalling consumed bytes", async () => {
    const update = release();
    const updater = new AppUpdate();
    await updater.check();
    await updater.download();
    vi.mocked(api.restartApp).mockRejectedValueOnce(
      new Error("restart failed")
    );
    await expect(updater.install()).rejects.toThrow("restart failed");
    expect(updater.status).toBe("ready");
    await updater.install();
    expect(update.install).toHaveBeenCalledOnce();
    expect(api.restartApp).toHaveBeenCalledTimes(2);
  });

  it("releases an update returned after the sidebar unmounts", async () => {
    const update = release();
    const updater = new AppUpdate();
    const checking = updater.check();
    updater.dispose();
    expect(update.close).not.toHaveBeenCalled();
    await checking;
    expect(update.close).toHaveBeenCalledOnce();
    await updater.download();
    expect(update.download).not.toHaveBeenCalled();
  });

  it("waits for an in-flight download before releasing its resources", async () => {
    const update = release();
    const updater = new AppUpdate();
    await updater.check();
    const downloading = updater.download();
    updater.dispose();
    expect(update.close).not.toHaveBeenCalled();
    await downloading;
    expect(update.close).toHaveBeenCalledOnce();
  });
});
