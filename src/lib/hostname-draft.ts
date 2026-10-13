import type { HostnameConfig, HostnameRoute } from "./types";

// Keep rule indices, paths and schemes intact; only new rules have a null index.
export function hostnamePayload(routes: HostnameRoute[], return404: boolean, customFallback: string) {
  return {
    routes: routes.map(({ index, hostname, service, path }) => ({ index, hostname: hostname.trim(), service: service.trim(), path })),
    fallbackService: return404 ? "http_status:404" : customFallback.trim(),
  };
}

export function hostnameChanged(config: HostnameConfig, payload: ReturnType<typeof hostnamePayload>) {
  return config.fallback_service !== payload.fallbackService
    || config.routes.length !== payload.routes.length
    || config.routes.some((row, i) => {
      const draft = payload.routes[i];
      return row.index !== draft.index || row.hostname !== draft.hostname || row.service !== draft.service || row.path !== draft.path;
    });
}
