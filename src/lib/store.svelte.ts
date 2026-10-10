// Reactive app state shared by a window's components, fed by backend events.
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "./api";
import { applyTheme } from "./theme";
import type { Overview, TunnelState } from "./types";

class AppStore {
  overview = $state<Overview | null>(null);
  loading = $state(true);
  error = $state<string | null>(null);
  /** Bumps when a request flows through a tunnel (drives the route pulse). */
  pulses = $state<Record<string, number>>({});

  get tunnels() {
    return this.overview?.tunnels ?? [];
  }
  get activeCount() {
    return this.tunnels.filter((t) => t.state.status === "connected" || t.state.status === "degraded").length;
  }

  async refresh() {
    try {
      const o = await api.overview();
      this.overview = o;
      this.error = null;
      applyTheme(o.settings.theme, false);
    } catch (e) {
      this.error = errorText(e);
    } finally {
      this.loading = false;
    }
  }

  applyState(s: TunnelState) {
    const t = this.overview?.tunnels.find((t) => t.id === s.id);
    if (t) t.state = s;
  }

  async init() {
    await this.refresh();
    await listen<TunnelState>("tunnel-state", (e) => this.applyState(e.payload));
    await listen("settings-changed", (e) => {
      if (this.overview) this.overview.settings = e.payload as Overview["settings"];
      applyTheme(this.overview!.settings.theme);
    });
    await listen<{ tunnel_id: string }>("request", (e) => {
      const id = e.payload.tunnel_id;
      this.pulses[id] = (this.pulses[id] ?? 0) + 1;
    });
  }
}

export const store = new AppStore();
