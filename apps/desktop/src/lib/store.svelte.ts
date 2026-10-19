// Reactive app state shared by a window's components, fed by backend events.
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "./api";
import { columnPrefs } from "./columns.svelte";
import { clearDnsCache } from "./dns-cache";
import { sectionPrefs } from "./sections.svelte";
import { applyBackgroundOpacity, applyTheme } from "./theme";
import { notify } from "./toast";
import { preserveConfigMappings } from "./tunnel-state";
import type { Overview, TunnelState } from "./types";

class AppStore {
  overview = $state<Overview | null>(null);
  loading = $state(true);
  error = $state<string | null>(null);
  /** Bumps when a request flows through a tunnel (drives the route pulse). */
  pulses = $state<Record<string, number>>({});
  private generation = 0;

  get tunnels() {
    return this.overview?.tunnels ?? [];
  }
  get activeCount() {
    return this.tunnels.filter(
      (t) => t.state.status === "connected" || t.state.status === "degraded"
    ).length;
  }

  async refresh() {
    const { generation } = this;
    try {
      const o = await api.overview();
      if (generation !== this.generation) {
        return;
      }
      this.overview = o;
      this.error = null;
      applyTheme(o.settings.theme);
      applyBackgroundOpacity(o.settings.background_opacity);
    } catch (e) {
      if (generation === this.generation) {
        this.error = errorText(e);
      }
    } finally {
      if (generation === this.generation) {
        this.loading = false;
      }
    }
  }

  private refreshTimer: ReturnType<typeof setTimeout> | undefined;
  /** Coalesce bursts of change events into one refresh. */
  scheduleRefresh() {
    clearTimeout(this.refreshTimer);
    this.refreshTimer = setTimeout(() => this.refresh(), 60);
  }

  applyState(s: TunnelState) {
    const tunnel = this.overview?.tunnels.find((item) => item.id === s.id);
    if (!tunnel) {
      return this.scheduleRefresh();
    }
    if (s.run < (tunnel.state.run ?? 0)) {
      return; // late event from a replaced run
    }
    tunnel.state = preserveConfigMappings(tunnel, s);
  }

  async init() {
    // Subscribe first so nothing that happens during the initial load is missed.
    await listen<TunnelState>("tunnel-state", (e) =>
      this.applyState(e.payload)
    );
    await listen("tunnels-changed", () => this.scheduleRefresh());
    await listen("app-reset", () => {
      this.generation += 1;
      clearTimeout(this.refreshTimer);
      this.overview = null;
      this.loading = true;
      this.error = null;
      this.pulses = {};
      for (const storage of ["localStorage", "sessionStorage"] as const) {
        try {
          window[storage].clear();
        } catch {
          // Browser storage can be unavailable; still reset the live window.
        }
      }
      columnPrefs.hidden = [];
      sectionPrefs.collapsed = [];
      clearDnsCache();
      notify.dismiss();
      this.refresh();
    });
    await listen("settings-changed", (e) => {
      const s = e.payload as Overview["settings"];
      if (this.overview) {
        this.overview.settings = s;
      }
      applyTheme(s.theme);
      applyBackgroundOpacity(s.background_opacity);
    });
    await listen<{ tunnel_id: string }>("request", (e) => {
      const id = e.payload.tunnel_id;
      this.pulses[id] = (this.pulses[id] ?? 0) + 1;
    });
    await this.refresh();
  }
}

export const store = new AppStore();
