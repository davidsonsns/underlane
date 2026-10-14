export type Status = "stopped" | "starting" | "connected" | "degraded" | "crashed";
type TunnelKind = "config" | "token" | "quick";

export interface RouteMapping {
  hostname: string | null;
  path: string | null;
  service: string;
  inspected: boolean;
  note: string | null;
}

export interface HostnameRoute {
  index: number | null;
  hostname: string;
  service: string;
  path: string | null;
}

export interface HostnameConfig {
  revision: string;
  routes: HostnameRoute[];
  fallback_service: string;
}

export interface HostnameSaveResult {
  restart_error: string | null;
}

export interface FailureExplanation {
  cause: string;
  action: string | null;
  raw: string;
}

export interface TunnelState {
  id: string;
  status: Status;
  detail: string | null;
  hint: string | null;
  failure: FailureExplanation | null;
  cloudflare_tunnel_id?: string | null;
  tunnel_conflict?: { tunnel_id: string; project_id: string; project_name: string } | null;
  public_url: string | null;
  mappings: RouteMapping[];
  pid: number | null;
  metrics_addr: string | null;
  connections: number;
  restarts: number;
  started_at: number | null;
  inspecting: boolean;
  supervised: boolean;
  run: number;
  config_revision: number;
}

export interface ConfigChange {
  run: number;
  config_revision: number;
  source_path: string;
  status: "unchanged" | "changed" | "unreadable" | "not_running";
  message: string | null;
}

export interface Tunnel {
  id: string;
  group_id: string | null;
  group_name: string;
  name: string;
  kind: TunnelKind;
  config_path: string | null;
  origin_url: string | null;
  inspect: boolean;
  autostart: boolean;
  loglevel: string;
  created_at: number;
  sort: number;
  state: TunnelState;
  has_token: boolean;
  /** A config project without a Cloudflare tunnel identity yet. */
  setup_needed?: boolean;
  /** Resolved source identity; active runs carry their pinned identity in state. */
  cloudflare_tunnel_id?: string | null;
  separating_tunnel?: boolean;
  separate_tunnel_pending?: boolean;
}

interface Group {
  id: string;
  name: string;
  sort: number;
}

export interface Binary {
  path: string;
  version: string | null;
  source: string;
}

export interface Settings {
  cloudflared_path: string | null;
  log_ttl_days: number;
  max_db_mb: number;
  max_body_kb: number;
  redact_headers: string[];
  redact_query: string[];
  theme: "system" | "light" | "dark";
  default_loglevel: string;
}

export interface Overview {
  groups: Group[];
  tunnels: Tunnel[];
  cloudflared: Binary | null;
  settings: Settings;
  active_count: number;
  dropped_captures: number;
  data_dir: string;
  last_launch: { at: number; reason: "login_item" | "normal" } | null;
}

export interface LogLine {
  id: number;
  tunnel_id: string;
  ts: number;
  level: "debug" | "info" | "warn" | "error" | "fatal";
  message: string;
  fields: string;
}

export interface RequestSummary {
  id: string;
  tunnel_id: string;
  ts: number;
  duration_ms: number | null;
  kind: "http" | "websocket" | "sse";
  method: string;
  host: string;
  path: string;
  status: number | null;
  resp_size: number;
  error: string | null;
}

export interface BodyView {
  size: number;
  stored: number;
  truncated: boolean;
  content_type: string | null;
  decoded_from: string | null;
  decode_partial: boolean;
  is_text: boolean;
  text: string | null;
  base64: string;
}

export interface RequestDetail extends Omit<RequestSummary, "resp_size"> {
  req_headers: [string, string][];
  resp_headers: [string, string][];
  req: BodyView;
  resp: BodyView;
}

export interface Candidate {
  path: string;
  name: string;
  group: string;
  hostnames: string[];
  imported: boolean;
}

export interface ProjectConfigPreview {
  path: string;
  routes: { hostname: string | null; path: string | null; service: string }[];
}

export interface TunnelSetupProgress {
  id: string;
  request_id: string;
  stage: "checking" | "login" | "creating" | "linking";
}

export interface TunnelSetupResult {
  tunnel_id: string;
  credentials_file: string;
}

export interface SeparateTunnelPreview {
  revision: string;
  old_tunnel_id: string;
  hostnames: string[];
  config_path: string;
  resuming: boolean;
}

export interface SeparateTunnelResult {
  tunnel_id: string;
  config_path: string;
  moved_hostnames: string[];
  start_error: string | null;
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
  ok: boolean;
  raw: string;
  hint: string | null;
  failure: FailureExplanation | null;
}

export interface LoginItem {
  status: "enabled" | "not_registered" | "requires_approval" | "not_found" | "unsupported";
  enabled: boolean;
  message: string | null;
}

export interface HostnameDns {
  status: "connected" | "missing" | "conflict" | "unknown";
  message: string;
  can_create: boolean;
  needs_dns_access: boolean;
}
