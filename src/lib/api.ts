import { invoke } from "@tauri-apps/api/core";
import type {
  Binary, Candidate, ImportReport, LogLine, LoginItem, Overview, RequestDetail, RequestSummary, Settings, Validation,
} from "./types";

export const api = {
  overview: () => invoke<Overview>("overview"),
  scanDefault: () => invoke<Candidate[]>("scan_default"),
  importPaths: (paths: string[]) => invoke<ImportReport>("import_paths", { paths }),
  addQuick: (name: string, origin: string, project: string, inspect: boolean) =>
    invoke<string>("add_quick_tunnel", { name, origin, project, inspect }),
  addToken: (name: string, token: string, project: string) => invoke<string>("add_token_tunnel", { name, token, project }),
  updateTunnel: (id: string, patch: Partial<{ name: string; project: string; inspect: boolean; autostart: boolean; loglevel: string; originUrl: string; token: string }>) =>
    invoke<void>("update_tunnel", { id, patch }),
  deleteTunnel: (id: string) => invoke<void>("delete_tunnel", { id }),
  renameProject: (id: string, name: string) => invoke<void>("rename_project", { id, name }),
  start: (id: string) => invoke<void>("start_tunnel", { id }),
  stop: (id: string) => invoke<void>("stop_tunnel", { id }),
  restart: (id: string) => invoke<void>("restart_tunnel", { id }),
  startAll: () => invoke<void>("start_all"),
  stopAll: () => invoke<void>("stop_all"),
  restartAll: () => invoke<void>("restart_all"),
  validate: (id: string) => invoke<Validation>("validate_tunnel", { id }),
  probeServices: (services: string[]) => invoke<(boolean | null)[]>("probe_services", { services }),
  queryLogs: (args: { tunnelId?: string | null; levels?: string[]; search?: string; beforeId?: number; afterId?: number; limit?: number }) =>
    invoke<LogLine[]>("query_logs", { args }),
  clearLogs: (tunnelId: string | null) => invoke<void>("clear_logs", { tunnelId }),
  listRequests: (tunnelId: string | null, search?: string, limit = 500) =>
    invoke<RequestSummary[]>("list_requests", { tunnelId, search: search || null, limit }),
  getRequest: (id: string) => invoke<RequestDetail | null>("get_request", { id }),
  clearRequests: (tunnelId: string | null) => invoke<void>("clear_requests", { tunnelId }),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  detectCloudflared: () => invoke<Binary | null>("detect_cloudflared"),
  loginItem: () => invoke<LoginItem>("login_item_status"),
  setLaunchAtLogin: (enabled: boolean) => invoke<LoginItem>("set_launch_at_login", { enabled }),
  openDashboard: (view?: string) => invoke<void>("open_dashboard", { view: view ?? null }),
  hidePopover: () => invoke<void>("hide_popover"),
  quit: () => invoke<void>("quit"),
  e2eReport: (nonce: string, text: string) => invoke<void>("e2e_report", { nonce, text }),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}
