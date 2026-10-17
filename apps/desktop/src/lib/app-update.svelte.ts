import { check, type Update } from "@tauri-apps/plugin-updater";
import { api } from "./api";

type Status =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "installing";

export class AppUpdate {
  status = $state<Status>("idle");
  version = $state("");
  progress = $state<number | null>(null);
  private update: Update | null = null;
  private disposed = $state(false);
  private installed = $state(false);

  async check() {
    if (this.disposed || this.status !== "idle") {
      return;
    }
    this.status = "checking";
    try {
      const update = await check({ timeout: 10_000 });
      this.update = update;
      this.version = update?.version ?? "";
      this.status = update === null ? "idle" : "available";
    } catch (error) {
      // A missing first release, offline Mac, or failed check must not interrupt work.
      console.info("Underlane update check failed", error);
      this.status = "idle";
    } finally {
      this.releaseIfDisposed();
    }
  }

  async download() {
    if (this.disposed || this.status !== "available" || !this.update) {
      return;
    }
    this.status = "downloading";
    this.progress = null;
    let total = 0;
    let received = 0;
    try {
      await this.update.download((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          received = 0;
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          this.progress =
            total > 0
              ? Math.min(100, Math.round((received / total) * 100))
              : null;
        }
      });
      this.status = "ready";
    } catch (error) {
      this.status = "available";
      throw error;
    } finally {
      this.releaseIfDisposed();
    }
  }

  async install() {
    if (this.disposed || this.status !== "ready" || !this.update) {
      return;
    }
    this.status = "installing";
    try {
      if (!this.installed) {
        await this.update.install();
        this.installed = true;
      }
      await api.restartApp();
    } catch (error) {
      this.status = "ready";
      throw error;
    } finally {
      this.releaseIfDisposed();
    }
  }

  dispose() {
    this.disposed = true;
    if (!["checking", "downloading", "installing"].includes(this.status)) {
      this.releaseIfDisposed();
    }
  }

  private releaseIfDisposed() {
    if (!this.disposed) {
      return;
    }
    const { update } = this;
    this.update = null;
    update
      ?.close()
      .catch((error) => console.info("Could not release update", error));
  }
}
