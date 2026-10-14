import { invoke } from "@tauri-apps/api/core";
import type {
  Binary,
  Candidate,
  ConfigChange,
  HostnameConfig,
  HostnameDns,
  HostnameRoute,
  HostnameSaveResult,
  ImportReport,
  LoginItem,
  LogLine,
  OnboardingStatus,
  Overview,
  ProjectConfigPreview,
  RequestDetail,
  RequestSummary,
  SeparateTunnelPreview,
  SeparateTunnelResult,
  Settings,
  TunnelSetupResult,
  Validation,
} from "./types";

export const api = {
  addQuick: (name: string, origin: string, group: string, inspect: boolean) =>
    invoke<string>("add_quick_tunnel", { group, inspect, name, origin }),
  addToken: (name: string, token: string, group: string) =>
    invoke<string>("add_token_tunnel", { group, name, token }),
  cancelOnboardingLogin: (requestId: string) =>
    invoke<void>("cancel_onboarding_login", { requestId }),
  cancelProjectSetup: (id: string, requestId: string) =>
    invoke<void>("cancel_project_setup", { id, requestId }),
  checkConfigChange: (id: string) =>
    invoke<ConfigChange | null>("check_config_change", { id }),
  checkHostnameDns: (id: string, hostname: string) =>
    invoke<HostnameDns>("check_hostname_dns", { hostname, id }),
  clearLogs: (tunnelId: string | null) =>
    invoke<void>("clear_logs", { tunnelId }),
  clearRequests: (tunnelId: string | null) =>
    invoke<void>("clear_requests", { tunnelId }),
  completeOnboarding: () => invoke<void>("complete_onboarding"),
  connectCloudflare: (requestId: string) =>
    invoke<void>("connect_cloudflare", { requestId }),
  connectHostnameDns: (id: string, hostname: string, token: string) =>
    invoke<HostnameDns>("connect_hostname_dns", { hostname, id, token }),
  createHostnameDns: (id: string, hostname: string) =>
    invoke<HostnameDns>("create_hostname_dns", { hostname, id }),
  createProject: (
    name: string,
    group: string,
    configPath: string | null,
    directory: string | null
  ) => invoke<string>("create_project", { configPath, directory, group, name }),
  createSeparateTunnel: (id: string, revision: string, requestId: string) =>
    invoke<SeparateTunnelResult>("create_separate_tunnel", {
      id,
      requestId,
      revision,
    }),
  deleteTunnel: (id: string, deleteConfig = false) =>
    invoke<void>("delete_tunnel", { deleteConfig, id }),
  detectCloudflared: () => invoke<Binary | null>("detect_cloudflared"),
  e2eReport: (nonce: string, text: string) =>
    invoke<void>("e2e_report", { nonce, text }),
  getRequest: (id: string) =>
    invoke<RequestDetail | null>("get_request", { id }),
  hidePopover: () => invoke<void>("hide_popover"),
  importPaths: (paths: string[]) =>
    invoke<ImportReport>("import_paths", { paths }),
  installCloudflared: () => invoke<Binary>("install_cloudflared"),
  listRequests: (
    tunnelId: string | null,
    search?: string,
    limit = 500,
    statuses: number[] = [],
    methods: string[] = []
  ) =>
    invoke<RequestSummary[]>("list_requests", {
      limit,
      methods,
      search: search || null,
      statuses,
      tunnelId,
    }),
  loadHostnames: (id: string) =>
    invoke<HostnameConfig>("load_hostnames", { id }),
  loginItem: () => invoke<LoginItem>("login_item_status"),
  onboardingStatus: () => invoke<OnboardingStatus>("onboarding_status"),
  openConfigInEditor: (id: string) =>
    invoke<void>("open_config_in_editor", { id }),
  openDashboard: (view?: string) =>
    invoke<void>("open_dashboard", { view: view ?? null }),
  overview: () => invoke<Overview>("overview"),
  previewProjectConfig: (path: string) =>
    invoke<ProjectConfigPreview>("preview_project_config", { path }),
  previewSeparateTunnel: (id: string) =>
    invoke<SeparateTunnelPreview>("preview_separate_tunnel", { id }),
  probeServices: (services: string[]) =>
    invoke<(boolean | null)[]>("probe_services", { services }),
  projectDirectory: () => invoke<string>("project_directory"),
  queryLogs: (args: {
    tunnelId?: string | null;
    levels?: string[];
    search?: string;
    beforeId?: number;
    afterId?: number;
    limit?: number;
  }) => invoke<LogLine[]>("query_logs", { args }),
  quit: () => invoke<void>("quit"),
  renameGroup: (id: string, name: string) =>
    invoke<void>("rename_group", { id, name }),
  requestCapturePaused: (tunnelId: string) =>
    invoke<boolean>("request_capture_paused", { tunnelId }),
  restart: (id: string) => invoke<void>("restart_tunnel", { id }),
  restartAll: () => invoke<void>("restart_all"),
  saveHostnames: (
    id: string,
    revision: string,
    routes: HostnameRoute[],
    fallbackService: string
  ) =>
    invoke<HostnameSaveResult>("save_hostnames", {
      fallbackService,
      id,
      revision,
      routes,
    }),
  saveSettings: (settings: Settings) =>
    invoke<void>("save_settings", { settings }),
  scanDefault: () => invoke<Candidate[]>("scan_default"),
  selectCloudflared: (path: string | null) =>
    invoke<Binary | null>("select_cloudflared", { path }),
  setLaunchAtLogin: (enabled: boolean) =>
    invoke<LoginItem>("set_launch_at_login", { enabled }),
  setRequestCapturePaused: (tunnelId: string, paused: boolean) =>
    invoke<void>("set_request_capture_paused", { paused, tunnelId }),
  setupProjectTunnel: (id: string, requestId: string) =>
    invoke<TunnelSetupResult>("setup_project_tunnel", { id, requestId }),
  start: (id: string) => invoke<void>("start_tunnel", { id }),
  startAll: () => invoke<void>("start_all"),
  stop: (id: string) => invoke<void>("stop_tunnel", { id }),
  stopAll: () => invoke<void>("stop_all"),
  updateTunnel: (
    id: string,
    patch: Partial<{
      name: string;
      group: string | null;
      inspect: boolean;
      autostart: boolean;
      loglevel: string;
      originUrl: string;
      token: string;
    }>
  ) => invoke<void>("update_tunnel", { id, patch }),
  validate: (id: string) => invoke<Validation>("validate_tunnel", { id }),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") {
    return e;
  }
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
