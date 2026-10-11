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
      applyTheme(o.settings.theme);
    } catch (e) {
      this.error = errorText(e);
    } finally {
      this.loading = false;
    }
  }

  private refreshTimer: ReturnType<typeof setTimeout> | undefined;
  /** Coalesce bursts of change events into one refresh. */
  scheduleRefresh() {
    clearTimeout(this.refreshTimer);
    this.refreshTimer = setTimeout(() => this.refresh(), 60);
  }

  applyState(s: TunnelState) {
    const t = this.overview?.tunnels.find((t) => t.id === s.id);
    if (!t) return this.scheduleRefresh();
    if (s.run < (t.state.run ?? 0)) return; // late event from a replaced run
    // A stopped config tunnel keeps showing the routes parsed from its file.
    if (s.mappings.length === 0 && t.state.mappings.length && t.kind === "config") s = { ...s, mappings: t.state.mappings };
    t.state = s;
  }

  async init() {
    // Subscribe first so nothing that happens during the initial load is missed.
    await listen<TunnelState>("tunnel-state", (e) => this.applyState(e.payload));
    await listen("tunnels-changed", () => this.scheduleRefresh());
    await listen("settings-changed", (e) => {
      const s = e.payload as Overview["settings"];
      if (this.overview) this.overview.settings = s;
      applyTheme(s.theme);
    });
    await listen<{ tunnel_id: string }>("request", (e) => {
      const id = e.payload.tunnel_id;
      this.pulses[id] = (this.pulses[id] ?? 0) + 1;
    });
    await this.refresh();
  }
}

export const store = new AppStore();
