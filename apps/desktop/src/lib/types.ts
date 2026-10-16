export type Status =
  | "stopped"
  | "starting"
  | "connected"
  | "degraded"
  | "crashed";
type TunnelKind = "config" | "token" | "quick";

export interface RouteMapping {
  hostname: string | null;
  inspected: boolean;
  note: string | null;
  path: string | null;
  service: string;
}

export interface HostnameRoute {
  hostname: string;
  index: number | null;
  path: string | null;
  service: string;
}

export interface HostnameConfig {
  fallback_service: string;
  revision: string;
  routes: HostnameRoute[];
}

export interface HostnameSaveResult {
  restart_error: string | null;
}

export interface FailureExplanation {
  action: string | null;
  cause: string;
  raw: string;
}

export interface TunnelState {
  cloudflare_tunnel_id?: string | null;
  config_revision: number;
  connections: number;
  detail: string | null;
  failure: FailureExplanation | null;
  hint: string | null;
  id: string;
  inspecting: boolean;
  mappings: RouteMapping[];
  metrics_addr: string | null;
  pid: number | null;
  public_url: string | null;
  restarts: number;
  run: number;
  started_at: number | null;
  status: Status;
  supervised: boolean;
  tunnel_conflict?: {
    tunnel_id: string;
    project_id: string;
    project_name: string;
  } | null;
}

export interface ConfigChange {
  config_revision: number;
  message: string | null;
  run: number;
  source_path: string;
  status: "unchanged" | "changed" | "unreadable" | "not_running";
}

export interface Tunnel {
  autostart: boolean;
  /** Resolved source identity; active runs carry their pinned identity in state. */
  cloudflare_tunnel_id?: string | null;
  config_path: string | null;
  created_at: number;
  group_id: string | null;
  group_name: string;
  has_token: boolean;
  id: string;
  inspect: boolean;
  kind: TunnelKind;
  loglevel: string;
  name: string;
  origin_url: string | null;
  separate_tunnel_pending?: boolean;
  separating_tunnel?: boolean;
  /** A config project without a Cloudflare tunnel identity yet. */
  setup_needed?: boolean;
  sort: number;
  state: TunnelState;
}

interface Group {
  id: string;
  name: string;
  sort: number;
}

export interface Binary {
  path: string;
  source: string;
  version: string | null;
}

export interface OnboardingStatus {
  account_connected: boolean;
  complete: boolean;
}

export interface Settings {
  cloudflared_path: string | null;
  default_loglevel: string;
  log_ttl_days: number;
  max_body_kb: number;
  max_db_mb: number;
  redact_headers: string[];
  redact_query: string[];
  theme: "system" | "light" | "dark";
}

export interface Overview {
  active_count: number;
  cloudflared: Binary | null;
  data_dir: string;
  dropped_captures: number;
  groups: Group[];
  last_launch: { at: number; reason: "login_item" | "normal" } | null;
  settings: Settings;
  tunnels: Tunnel[];
}

export interface LogLine {
  fields: string;
  id: number;
  level: "debug" | "info" | "warn" | "error" | "fatal";
  message: string;
  ts: number;
  tunnel_id: string;
}

export interface RequestSummary {
  duration_ms: number | null;
  error: string | null;
  host: string;
  id: string;
  kind: "http" | "websocket" | "sse";
  method: string;
  path: string;
  resp_size: number;
  status: number | null;
  ts: number;
  tunnel_id: string;
}

export interface BodyView {
  base64: string;
  content_type: string | null;
  decode_partial: boolean;
  decoded_from: string | null;
  is_text: boolean;
  size: number;
  stored: number;
  text: string | null;
  truncated: boolean;
}

export interface RequestDetail extends Omit<RequestSummary, "resp_size"> {
  req: BodyView;
  req_headers: [string, string][];
  resp: BodyView;
  resp_headers: [string, string][];
}

export interface Candidate {
  group: string;
  hostnames: string[];
  imported: boolean;
  name: string;
  path: string;
}

export interface ProjectConfigPreview {
  name: string;
  path: string;
  routes: { hostname: string | null; path: string | null; service: string }[];
}

export interface TunnelSetupProgress {
  id: string;
  request_id: string;
  stage: "checking" | "login" | "creating" | "linking";
}

export interface TunnelSetupResult {
  credentials_file: string;
  tunnel_id: string;
}

export interface SeparateTunnelPreview {
  config_path: string;
  hostnames: string[];
  old_tunnel_id: string;
  resuming: boolean;
  revision: string;
}

export interface SeparateTunnelResult {
  config_path: string;
  moved_hostnames: string[];
  start_error: string | null;
  tunnel_id: string;
}

export interface SeparateTunnelProgress {
  id: string;
  request_id: string;
  stage: "checking" | "creating" | "dns" | "linking" | "starting";
}

export interface ImportReport {
  added: string[];
  existing: string[];
  failed: [string, string][];
}

export interface Validation {
  failure: FailureExplanation | null;
  hint: string | null;
  ok: boolean;
  raw: string;
}

export interface LoginItem {
  enabled: boolean;
  message: string | null;
  status:
    | "enabled"
    | "not_registered"
    | "requires_approval"
    | "not_found"
    | "unsupported";
}

export interface HostnameDns {
  can_create: boolean;
  message: string;
  needs_dns_access: boolean;
  status: "connected" | "missing" | "conflict" | "unknown";
}
