import { invoke } from "@tauri-apps/api/core";
import type {
  Binary, Candidate, ConfigChange, HostnameConfig, HostnameRoute, HostnameSaveResult, ImportReport, LogLine, LoginItem, Overview, ProjectConfigPreview, RequestDetail, RequestSummary, Settings, TunnelSetupResult, Validation,
} from "./types";

export const api = {
  overview: () => invoke<Overview>("overview"),
  renameGroup: (id: string, name: string) => invoke<void>("rename_group", { id, name }),
  scanDefault: () => invoke<Candidate[]>("scan_default"),
  importPaths: (paths: string[]) => invoke<ImportReport>("import_paths", { paths }),
  projectDirectory: () => invoke<string>("project_directory"),
  previewProjectConfig: (path: string) => invoke<ProjectConfigPreview>("preview_project_config", { path }),
  createProject: (name: string, group: string, configPath: string | null, directory: string | null) =>
    invoke<string>("create_project", { name, group, configPath, directory }),
  setupProjectTunnel: (id: string, requestId: string) => invoke<TunnelSetupResult>("setup_project_tunnel", { id, requestId }),
  cancelProjectSetup: (id: string, requestId: string) => invoke<void>("cancel_project_setup", { id, requestId }),
  addQuick: (name: string, origin: string, group: string, inspect: boolean) =>
    invoke<string>("add_quick_tunnel", { name, origin, group, inspect }),
  addToken: (name: string, token: string, group: string) => invoke<string>("add_token_tunnel", { name, token, group }),
  updateTunnel: (id: string, patch: Partial<{ name: string; group: string | null; inspect: boolean; autostart: boolean; loglevel: string; originUrl: string; token: string }>) =>
    invoke<void>("update_tunnel", { id, patch }),
  deleteTunnel: (id: string) => invoke<void>("delete_tunnel", { id }),
  start: (id: string) => invoke<void>("start_tunnel", { id }),
  stop: (id: string) => invoke<void>("stop_tunnel", { id }),
  restart: (id: string) => invoke<void>("restart_tunnel", { id }),
  startAll: () => invoke<void>("start_all"),
  stopAll: () => invoke<void>("stop_all"),
  restartAll: () => invoke<void>("restart_all"),
  validate: (id: string) => invoke<Validation>("validate_tunnel", { id }),
  checkConfigChange: (id: string) => invoke<ConfigChange | null>("check_config_change", { id }),
  openConfigInEditor: (id: string) => invoke<void>("open_config_in_editor", { id }),
  loadHostnames: (id: string) => invoke<HostnameConfig>("load_hostnames", { id }),
  saveHostnames: (id: string, revision: string, routes: HostnameRoute[], fallbackService: string) =>
    invoke<HostnameSaveResult>("save_hostnames", { id, revision, routes, fallbackService }),
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
