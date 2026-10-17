// Passed to addInitScript, so keep this initializer self-contained.
export function extendWorkspaceMock({ mockName }) {
  const mock = window[mockName];
  const fallback = window.__mockDispatch;
  const prototype = structuredClone(mock.overview.tunnels[0]);
  const groups = [
    { id: "group-work", name: "Work", sort: 0 },
    { id: "group-personal", name: "Personal", sort: 1 },
    { id: "group-empty", name: "Ideas", sort: 2 },
  ];
  const project = (id, name, groupId, kind = "config", inspect = true) => ({
    ...structuredClone(prototype),
    cloudflare_tunnel_id: kind === "config" ? `synthetic-${id}` : null,
    config_path: kind === "config" ? `/synthetic/${id}.yml` : null,
    group_id: groupId,
    group_name: groups.find((group) => group.id === groupId)?.name ?? "",
    has_token: kind === "token",
    id,
    inspect,
    kind,
    name,
    origin_url: kind === "quick" ? "http://localhost:4200" : null,
    separate_tunnel_pending: false,
    separating_tunnel: false,
    setup_needed: false,
    state: {
      ...structuredClone(prototype.state),
      id,
      inspecting: inspect,
      mappings: prototype.state.mappings.map((mapping) => ({
        ...mapping,
        inspected: inspect && mapping.hostname !== null,
      })),
    },
  });
  mock.overview.groups = groups;
  mock.overview.tunnels = [
    project("work-api", "Work API", "group-work"),
    project("work-token", "Remote Token", "group-work", "token", false),
    project("personal-web", "Personal Web", "group-personal"),
    project("quick-preview", "Quick Preview", null, "quick"),
  ];
  mock.overview.active_count = mock.overview.tunnels.length;
  mock.requests = mock.overview.tunnels
    .filter((tunnel) => tunnel.inspect)
    .map((tunnel, index) => ({
      duration_ms: 12,
      error: null,
      host: "synthetic.example.test",
      id: `request-${tunnel.id}`,
      kind: "http",
      method: "GET",
      path: `/${tunnel.id}/initial`,
      resp_size: 42,
      status: 200,
      ts: Date.now() - index * 1000,
      tunnel_id: tunnel.id,
    }));
  mock.logs = mock.overview.tunnels.map((tunnel, index) => ({
    fields: "{}",
    id: index + 1,
    level: "info",
    message: `${tunnel.name} synthetic log`,
    ts: Date.now() - index * 1000,
    tunnel_id: tunnel.id,
  }));
  mock.paused = {};
  mock.emit = (event, payload) =>
    window.__mockListeners[event]?.({ event, payload });
  const changed = () => setTimeout(() => mock.emit("tunnels-changed", null), 0);
  // biome-ignore lint/performance/useTopLevelRegex: Init scripts cannot capture module constants.
  const normalize = (name) => name.trim().split(/\s+/u).join(" ");
  const ensureGroup = (name) => {
    const normalized = normalize(name);
    let group = groups.find(
      (candidate) => candidate.name.toLowerCase() === normalized.toLowerCase()
    );
    if (!group) {
      group = {
        id: `created-group-${groups.length}`,
        name: normalized,
        sort: groups.length,
      };
      groups.push(group);
    }
    return group;
  };
  const handlers = {
    clear_logs: ({ tunnelId }) => {
      mock.logs = mock.logs.filter(
        (line) => tunnelId && line.tunnel_id !== tunnelId
      );
      return null;
    },
    clear_requests: ({ tunnelId }) => {
      mock.requests = mock.requests.filter(
        (request) => tunnelId && request.tunnel_id !== tunnelId
      );
      return null;
    },
    create_group: ({ name }) => {
      const group = ensureGroup(name);
      changed();
      return group.id;
    },
    create_project: ({ name, group, configPath }) => {
      const destination = group ? ensureGroup(group) : null;
      const id = `created-project-${mock.overview.tunnels.length}`;
      const tunnel = project(
        id,
        name,
        destination?.id ?? null,
        "config",
        false
      );
      tunnel.config_path = configPath ?? `/synthetic/${id}.yml`;
      mock.overview.tunnels.push(tunnel);
      changed();
      return id;
    },
    delete_group: async ({ id }) => {
      await new Promise((resolve) => setTimeout(resolve, mock.groupDelay ?? 0));
      if (mock.failDeleteGroup) {
        throw new Error("Couldn't delete the group");
      }
      if (mock.overview.tunnels.some((tunnel) => tunnel.group_id === id)) {
        throw new Error("Only empty groups can be deleted.");
      }
      const index = groups.findIndex((group) => group.id === id);
      if (index === -1) {
        throw new Error("Group not found.");
      }
      groups.splice(index, 1);
      changed();
      return null;
    },
    delete_tunnel: ({ id }) => {
      mock.overview.tunnels = mock.overview.tunnels.filter(
        (tunnel) => tunnel.id !== id
      );
      mock.requests = mock.requests.filter(
        (request) => request.tunnel_id !== id
      );
      mock.logs = mock.logs.filter((line) => line.tunnel_id !== id);
      changed();
      return null;
    },
    list_requests: ({
      tunnelId,
      search,
      statuses = [],
      methods = [],
      limit = 500,
    }) =>
      structuredClone(
        mock.requests
          .filter(
            (request) =>
              (!tunnelId || request.tunnel_id === tunnelId) &&
              (!search || `${request.host}${request.path}`.includes(search)) &&
              (!statuses.length || statuses.includes(request.status)) &&
              (!methods.length || methods.includes(request.method))
          )
          .slice(0, limit)
      ),
    overview: () => {
      if (mock.failOverview) {
        throw new Error("Couldn't refresh projects");
      }
      return structuredClone(mock.overview);
    },
    query_logs: ({ args }) =>
      structuredClone(
        mock.logs.filter(
          (line) =>
            (!args.tunnelId || line.tunnel_id === args.tunnelId) &&
            (!args.search || line.message.includes(args.search)) &&
            (!args.levels?.length || args.levels.includes(line.level))
        )
      ),
    rename_group: ({ id, name }) => {
      const group = groups.find((candidate) => candidate.id === id);
      if (!group) {
        throw new Error("Group not found");
      }
      group.name = normalize(name);
      for (const tunnel of mock.overview.tunnels.filter(
        (item) => item.group_id === id
      )) {
        tunnel.group_name = group.name;
      }
      changed();
      return null;
    },
    request_capture_paused: ({ tunnelId }) => mock.paused[tunnelId] ?? false,
    set_request_capture_paused: ({ tunnelId, paused }) => {
      mock.paused[tunnelId] = paused;
      mock.emit("request-capture-paused", { paused, tunnel_id: tunnelId });
      return null;
    },
    update_tunnel: async ({ id, patch }) => {
      await new Promise((resolve) => setTimeout(resolve, mock.groupDelay ?? 0));
      if (mock.failMove) {
        throw new Error("Couldn't move the project");
      }
      const tunnel = mock.overview.tunnels.find((item) => item.id === id);
      if (!tunnel) {
        throw new Error("Project not found");
      }
      const { group, ...values } = patch;
      Object.assign(tunnel, values);
      if (Object.hasOwn(patch, "group")) {
        const destination = group ? ensureGroup(group) : null;
        tunnel.group_id = destination?.id ?? null;
        tunnel.group_name = destination?.name ?? "";
      }
      changed();
      return null;
    },
  };
  window.__mockDispatch = (cmd, args = {}) => {
    if (!Object.hasOwn(handlers, cmd)) {
      return fallback(cmd, args);
    }
    mock.calls.push({ args: structuredClone(args), cmd });
    return handlers[cmd](args);
  };
}
