const BASE_URL = process.env.E2E_BASE_URL ?? "http://127.0.0.1:1420";

export async function browserPage({
  webkit = false,
  height = Number(process.env.HEIGHT ?? 760),
  timeout = 5000,
} = {}) {
  const playwright = await import(
    process.env.PLAYWRIGHT_MODULE ?? "playwright"
  );
  const browser = await (webkit
    ? playwright.webkit
    : playwright.chromium
  ).launch({ headless: true });
  const page = await browser.newPage({
    reducedMotion: "no-preference",
    viewport: { height, width: 1280 },
  });
  const errors = [];
  page.setDefaultTimeout(timeout);
  page.on("pageerror", (error) => errors.push(String(error)));
  return {
    baseUrl: BASE_URL,
    browser,
    errors,
    page,
  };
}
export function installHostnameMock({ delay, mockName }) {
  window.__loadDelay = delay;
  const hosts = [
    "app.example.com",
    "api.example.com",
    "docs.example.com",
    "checkout.example.com",
  ];
  const routes = hosts.map((hostname, index) => ({
    hostname,
    index,
    path: null,
    service: `http://localhost:${[4000, 3000, 4330, 4000][index]}`,
  }));
  const config = {
    fallback_service: "http_status:404",
    revision: "synthetic-r1",
    routes,
  };
  const state = {
    config_revision: 1,
    connections: 4,
    detail: null,
    failure: null,
    hint: null,
    id: "synthetic",
    inspecting: false,
    mappings: [
      ...routes.map((r) => ({ ...r, inspected: false, note: null })),
      {
        hostname: null,
        inspected: false,
        note: null,
        path: null,
        service: "http_status:404",
      },
    ],
    metrics_addr: null,
    pid: 123,
    public_url: null,
    restarts: 0,
    run: 1,
    started_at: Date.now(),
    status: "connected",
    supervised: true,
  };
  const tunnel = {
    autostart: false,
    config_path: "/synthetic/hostname-ui.yml",
    created_at: Date.now(),
    group_id: "group",
    group_name: "254",
    has_token: false,
    id: "synthetic",
    inspect: false,
    kind: "config",
    loglevel: "info",
    name: "example",
    origin_url: null,
    sort: 0,
    state,
  };
  const overview = {
    active_count: 1,
    cloudflared: {
      path: "/synthetic/cloudflared",
      source: "mock",
      version: "2026.9.1",
    },
    data_dir: "/synthetic",
    dropped_captures: 0,
    groups: [{ id: "group", name: "254", sort: 0 }],
    last_launch: null,
    settings: {
      cloudflared_path: null,
      default_loglevel: "info",
      log_ttl_days: 7,
      max_body_kb: 1024,
      max_db_mb: 100,
      redact_headers: [],
      redact_query: [],
      theme: "dark",
    },
    tunnels: [tunnel],
  };

  window[mockName] = { calls: [], config, overview };
  window.__mockDispatch = (cmd, args) => {
    const m = window[mockName];
    m.calls.push({ args, cmd });

    const handlers = {
      check_config_change: () => ({
        config_revision: 1,
        message: null,
        run: 1,
        source_path: "/synthetic/hostname-ui.yml",
        status: "unchanged",
      }),
      load_hostnames: async () => {
        await new Promise((r) =>
          setTimeout(r, Number(window.__loadDelay ?? 100))
        );
        return structuredClone(m.config);
      },
      login_item_status: () => ({
        enabled: false,
        message: null,
        status: "unsupported",
      }),
      overview: () => structuredClone(m.overview),

      probe_services: () =>
        args.services.map((s) => (s.startsWith("http_status:") ? null : false)),
      project_directory: () => "/synthetic/.cloudflared/underlane",
      rename_group: async () => {
        await new Promise((r) => setTimeout(r, m.renameDelay ?? 80));
        if (m.failRename) {
          return Promise.reject("Could not save project name");
        }
        m.overview.groups[0].name = args.name;
        m.overview.tunnels[0].group_name = args.name;
        return null;
      },
      scan_default: () => [],
      update_tunnel: () => {
        m.overview.tunnels[0].name = args.patch.name;
        return null;
      },
    };
    const handler = handlers[cmd];
    if (handler) {
      return handler();
    }
    if (!cmd.startsWith("plugin:")) {
      console.warn("Unmocked Tauri command", cmd);
    }
    return null;
  };
}

function installTauriBridge() {
  const callbacks = {},
    listeners = {};
  let next = 1;
  const nextId = () => {
    const id = next;
    next += 1;
    return id;
  };
  window.__mockListeners = listeners;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener() {
      /* The fixture page owns listener lifetime. */
    },
  };
  window.__TAURI_INTERNALS__ = {
    async invoke(cmd, args) {
      if (cmd === "plugin:event|listen") {
        listeners[args.event] = callbacks[args.handler];
        return nextId();
      }
      return await window.__mockDispatch(cmd, args);
    },
    metadata: {
      currentWebview: { label: "dashboard" },
      currentWindow: { label: "dashboard" },
    },
    transformCallback(callback) {
      const id = nextId();
      callbacks[id] = callback;
      return id;
    },
    unregisterCallback(id) {
      delete callbacks[id];
    },
  };
}
export function installPageMock(page, initializer, options) {
  return page.addInitScript({
    content: `(${installTauriBridge})(); (${initializer})(${JSON.stringify(options)});`,
  });
}
