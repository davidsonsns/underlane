export type Status = "stopped" | "starting" | "connected" | "degraded" | "crashed";
export type TunnelKind = "config" | "token" | "quick";

export interface RouteMapping {
  hostname: string | null;
  path: string | null;
  service: string;
  inspected: boolean;
  note: string | null;
}

export interface TunnelState {
  id: string;
  status: Status;
  detail: string | null;
  hint: string | null;
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
}

export interface Tunnel {
  id: string;
  project_id: string;
  project_name: string;
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
}

export interface Project {
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
  projects: Project[];
  tunnels: Tunnel[];
  cloudflared: Binary | null;
  settings: Settings;
  active_count: number;
  dropped_captures: number;
  data_dir: string;
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
  project: string;
  hostnames: string[];
  imported: boolean;
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
}

export interface LoginItem {
  status: "enabled" | "not_registered" | "requires_approval" | "not_found" | "unsupported";
  enabled: boolean;
  message: string | null;
}
