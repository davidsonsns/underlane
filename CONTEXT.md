# Underlane

Underlane manages local Cloudflare tunnels and their traffic.

## Language

**Group**:
A named collection of projects and tunnels used to organize the dashboard and menu bar. Members may also be ungrouped.

**Project**:
A named workspace for one locally managed tunnel and its config. It can begin with a new config or use an existing one, and may belong to a group.

**Quick tunnel**:
A temporary public link to a local service, with a hostname assigned when it connects.

**Connector connected**:
The local connector has a connection to Cloudflare. This status alone does not establish that a public hostname reaches its intended local service.

**Public connection verified**:
A hostname has passed the checks for its intended tunnel and public HTTP endpoint from this Mac. Application flows such as signing in remain separate checks.
