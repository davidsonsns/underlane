import type { RequestSummary } from "./types";

export const REQUEST_STATUSES = [
  [200, "OK"],
  [201, "Created"],
  [202, "Accepted"],
  [204, "No Content"],
  [301, "Moved Permanently"],
  [302, "Found"],
  [304, "Not Modified"],
  [307, "Temporary Redirect"],
  [308, "Permanent Redirect"],
  [400, "Bad Request"],
  [401, "Unauthorized"],
  [403, "Forbidden"],
  [404, "Not Found"],
  [409, "Conflict"],
  [422, "Unprocessable Content"],
  [429, "Too Many Requests"],
  [500, "Internal Server Error"],
  [502, "Bad Gateway"],
  [503, "Service Unavailable"],
  [504, "Gateway Timeout"],
] as const;

export const REQUEST_METHODS = [
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
  "OPTIONS",
  "CONNECT",
  "TRACE",
  "QUERY",
];
// Named methods from the IANA HTTP Method Registry; keep uncommon extensions out of the first group.
export const OTHER_REQUEST_METHODS = [
  "ACL",
  "BASELINE-CONTROL",
  "BIND",
  "CHECKIN",
  "CHECKOUT",
  "COPY",
  "LABEL",
  "LINK",
  "LOCK",
  "MERGE",
  "MKACTIVITY",
  "MKCALENDAR",
  "MKCOL",
  "MKREDIRECTREF",
  "MKWORKSPACE",
  "MOVE",
  "ORDERPATCH",
  "PRI",
  "PROPFIND",
  "PROPPATCH",
  "REBIND",
  "REPORT",
  "SEARCH",
  "UNBIND",
  "UNCHECKOUT",
  "UNLINK",
  "UNLOCK",
  "UPDATE",
  "UPDATEREDIRECTREF",
  "VERSION-CONTROL",
];

/** Match live events using the same OR-within / AND-between rules as the history query. */
export function matchesRequestFilters(
  r: Pick<RequestSummary, "status" | "method">,
  statuses: readonly number[],
  methods: readonly string[]
): boolean {
  return (
    (!statuses.length || (r.status !== null && statuses.includes(r.status))) &&
    (!methods.length || methods.includes(r.method))
  );
}
