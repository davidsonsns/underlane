// Synthetic workspace regression. Run with Vite on port 1420.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import {
  browserPage,
  installHostnameMock,
  installPageMock,
} from "./lib/browser.mjs";
import { extendWorkspaceMock } from "./lib/workspace-fixture.mjs";

const { baseUrl, browser, page, errors } = await browserPage({
  webkit: process.env.BROWSER === "webkit",
});
const external = [];
await page.route("**/*", (route) => {
  if (new URL(route.request().url()).origin === new URL(baseUrl).origin) {
    return route.continue();
  }
  external.push(route.request().url());
  return route.abort();
});
await installPageMock(
  page,
  installHostnameMock,
  { delay: 0, mockName: "__workspaceMock" },
  extendWorkspaceMock
);

const project = (name) =>
  page
    .getByRole("navigation", { exact: true, name: "Projects" })
    .getByRole("button", { name: new RegExp(`^${name},`) });
const activity = (kind) =>
  page.locator("aside").getByRole("button", {
    exact: true,
    name: kind === "requests" ? "Requests" : "Logs",
  });
async function checkOverview() {
  await main.getByRole("heading", { exact: true, name: "Source" }).waitFor();
  await main.getByRole("heading", { exact: true, name: "Behaviour" }).waitFor();
  assert.equal(
    await main.getByRole("tablist", { name: "Project views" }).count(),
    0,
    "Project settings are directly visible without nested navigation"
  );
  assert.deepEqual(
    await Promise.all(
      ["Overview", "Requests", "Logs"].map((name) =>
        main.getByRole("tab", { exact: true, name }).count()
      )
    ),
    [0, 0, 0]
  );
  assert.deepEqual(
    await Promise.all(
      ["View requests", "View logs"].map((name) =>
        main.getByRole("button", { exact: true, name }).count()
      )
    ),
    [0, 0]
  );
}
async function checkGlobalActivity(kind) {
  await activity(kind).and(page.locator('[aria-current="page"]')).waitFor();
  assert.equal(await activity(kind).getAttribute("aria-current"), "page");
  assert.equal(
    await page
      .getByRole("navigation", { exact: true, name: "Projects" })
      .locator('[aria-current="page"]')
      .count(),
    0,
    "Global activity does not select a project in the sidebar"
  );
  assert.equal(
    await main.getByRole("tablist", { name: "Project views" }).count(),
    0
  );
}
const main = page.getByRole("main");
const lastCall = (cmd) =>
  page.evaluate(
    (command) =>
      window.__workspaceMock.calls
        .filter((call) => call.cmd === command)
        .at(-1),
    cmd
  );
const queryCount = (cmd) =>
  page.evaluate(
    (command) =>
      window.__workspaceMock.calls.filter((call) => call.cmd === command)
        .length,
    cmd
  );
async function waitQuery(cmd, count, id) {
  await page.waitForFunction(
    ({ command, previous, expected }) => {
      const calls = window.__workspaceMock.calls.filter(
        (call) => call.cmd === command
      );
      const latest = calls.at(-1);
      return (
        latest !== undefined &&
        calls.length > previous &&
        (command === "query_logs"
          ? latest.args.args.tunnelId
          : latest.args.tunnelId) === expected
      );
    },
    { command: cmd, expected: id, previous: count }
  );
}
async function navigate(target) {
  await page.evaluate(
    (value) => window.__workspaceMock.emit("navigate", value),
    target
  );
}

async function checkGroupManagement() {
  const group = (name) =>
    page.getByRole("region", { exact: true, name: `${name} projects` });
  const groupButton = (name) =>
    page.getByRole("button", { name: new RegExp(`^${name}, \\d+ projects`) });
  const actions = (name) =>
    page.getByRole("button", { exact: true, name: `${name} actions` });
  const remove = () =>
    page.getByRole("menuitem", { exact: true, name: "Delete group" });
  const move = (name, destination) => project(name).dragTo(group(destination));
  const inGroup = (id, name) => group(name).locator(`[data-tunnel-id="${id}"]`);

  await project("Work API").click();
  await checkOverview();
  await groupButton("Ideas").click();
  await move("Work API", "Ideas");
  await inGroup("work-api", "Ideas").waitFor();
  assert.equal(
    await groupButton("Ideas").getAttribute("aria-expanded"),
    "true"
  );
  assert.equal(await project("Work API").getAttribute("aria-current"), "page");
  await checkOverview();
  assert.deepEqual((await lastCall("update_tunnel")).args, {
    id: "work-api",
    patch: { group: "Ideas" },
  });
  await move("Work API", "Work");
  await inGroup("work-api", "Work").waitFor();

  let mutations = await queryCount("update_tunnel");
  await move("Work API", "Work");
  await project("Work API").dragTo(main);
  assert.equal(
    await queryCount("update_tunnel"),
    mutations,
    "Same-group and cancelled drags do not write"
  );
  assert.equal(await page.locator("[data-drop-target]").count(), 0);

  await move("Quick Preview", "Personal");
  await inGroup("quick-preview", "Personal").waitFor();
  assert.equal(await group("Ungrouped").count(), 0);
  // Drag first so the temporary Ungrouped target becomes available.
  const box = await project("Work API").boundingBox();
  await project("Work API").hover();
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 12, box.y + box.height / 2, {
    steps: 5,
  });
  await group("Ungrouped").waitFor();
  await group("Ungrouped").hover();
  await group("Ungrouped").hover();
  await page
    .locator('[data-project-group="ungrouped"][data-drop-target]')
    .waitFor();
  await mkdir("e2e/out", { recursive: true });
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/group-drag.png",
  });
  await page.mouse.up();
  await inGroup("work-api", "Ungrouped").waitFor();
  assert.deepEqual((await lastCall("update_tunnel")).args.patch, {
    group: null,
  });
  await move("Work API", "Work");
  await inGroup("work-api", "Work").waitFor();

  await page.evaluate(() => {
    window.__workspaceMock.failMove = true;
  });
  await move("Work API", "Ideas");
  await page
    .getByRole("alert")
    .filter({ hasText: "Couldn't move the project" })
    .waitFor();
  await inGroup("work-api", "Work").waitFor();
  assert.equal(await inGroup("work-api", "Ideas").count(), 0);
  await page.evaluate(() => {
    window.__workspaceMock.failMove = false;
    window.__workspaceMock.failOverview = true;
  });
  await move("Work API", "Ideas");
  await page.getByRole("alert").filter({ hasText: "Change saved." }).waitFor();
  mutations = await queryCount("update_tunnel");
  await page.evaluate(() => {
    window.__workspaceMock.failOverview = false;
  });
  await page
    .getByRole("button", { exact: true, name: "Retry refresh" })
    .click();
  await inGroup("work-api", "Ideas").waitFor();
  assert.equal(
    await queryCount("update_tunnel"),
    mutations,
    "Refresh retry never repeats a move"
  );
  await move("Work API", "Work");
  await inGroup("work-api", "Work").waitFor();

  await actions("Work").click();
  assert.equal(
    await remove().count(),
    0,
    "Populated groups cannot be deleted in the UI"
  );
  await page.keyboard.press("Escape");
  await page.evaluate(() => {
    window.__workspaceMock.failDeleteGroup = true;
  });
  await actions("Ideas").click();
  await remove().click();
  await page
    .getByRole("alert")
    .filter({ hasText: "Couldn't delete the group" })
    .waitFor();
  await group("Ideas").waitFor();

  await groupButton("Ideas").click();
  await page.evaluate(() => {
    window.__workspaceMock.failDeleteGroup = false;
    window.__workspaceMock.failOverview = true;
  });
  await actions("Ideas").click();
  await remove().click();
  await page.getByRole("alert").filter({ hasText: "Change saved." }).waitFor();
  const deletions = await queryCount("delete_group");
  await page.evaluate(() => {
    window.__workspaceMock.failOverview = false;
  });
  await page
    .getByRole("button", { exact: true, name: "Retry refresh" })
    .click();
  await group("Ideas").waitFor({ state: "hidden" });
  assert.equal(
    await queryCount("delete_group"),
    deletions,
    "Refresh retry never repeats a deletion"
  );
  assert.equal(await project("Work API").getAttribute("aria-current"), "page");
  await checkOverview();
  assert.equal(
    await page.evaluate(
      () =>
        localStorage
          .getItem("underlane.projects.collapsed-groups")
          ?.includes("group-empty") ?? false
    ),
    false
  );

  await page.reload();
  await project("Work API").waitFor();
}
const projectFilterName = /^Projects \/ tunnels/;
const filtersButtonName = /^Filters(?:, \d+ active)?$/;
async function chooseProjectFilter(kind, name, id) {
  const cmd = kind === "requests" ? "list_requests" : "query_logs";
  const count = await queryCount(cmd);
  await page.getByRole("button", { name: filtersButtonName }).click();
  await page.getByRole("menuitem", { name: projectFilterName }).click();
  await page.getByRole("menuitemradio", { exact: true, name }).click();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await waitQuery(cmd, count, id);
}
async function resetScope(kind, id) {
  const cmd = kind === "requests" ? "list_requests" : "query_logs";
  const count = await queryCount(cmd);
  await page
    .getByRole("button", { exact: true, name: `Search ${kind}` })
    .click();
  await page
    .getByRole("textbox", { exact: true, name: `Search ${kind}` })
    .fill("initial");
  await waitQuery(cmd, count, id);
  const filtered = await queryCount(cmd);
  await page
    .getByRole("button", { exact: true, name: "Reset filters" })
    .click();
  await waitQuery(cmd, filtered, null);
  await checkGlobalActivity(kind);
  // Deep-link scope is an ordinary filter: reset removes it and another
  // project can be chosen before restoring the scope for history clearing.
  await chooseProjectFilter(kind, "Personal Web", "personal-web");
  await chooseProjectFilter(kind, "Work API", id);
}
async function clearHistory(kind, id) {
  await page
    .getByRole("button", { exact: true, name: "Columns and actions" })
    .click();
  await page
    .getByRole("button", {
      exact: true,
      name:
        kind === "requests" ? "Clear request history…" : "Clear log history…",
    })
    .click();
  const dialog = page.getByRole("alertdialog");
  await dialog
    .getByRole("button", { exact: true, name: "Clear history" })
    .click();
  await dialog.waitFor({ state: "hidden" });
  assert.equal(
    (await lastCall(`clear_${kind}`)).args.tunnelId,
    id,
    "Clear history stays within the selected project"
  );
}

try {
  await page.goto(`${baseUrl}/#tunnels`);
  await project("Work API").waitFor();
  assert.equal(
    await page.locator("aside").count(),
    1,
    "One application sidebar"
  );
  assert.equal(
    await page
      .getByRole("navigation", { exact: true, name: "Projects" })
      .count(),
    1,
    "One project navigation list"
  );
  assert.equal(
    await main.locator("[data-tunnel-id]").count(),
    0,
    "The content pane does not duplicate the project list"
  );
  assert.equal(
    await page.getByRole("heading", { exact: true, name: "Projects" }).count(),
    1
  );
  await page
    .getByRole("button", { exact: true, name: "Ideas, 0 projects" })
    .waitFor();
  await page.getByText("No projects", { exact: true }).waitFor();
  await checkGroupManagement();

  await project("Work API").click();
  await project("Work API").press("ArrowDown");
  assert.equal(
    await project("Remote Token").getAttribute("aria-current"),
    "page",
    "Arrow keys select the next project"
  );
  await checkOverview();
  await navigate("requests:work-token");
  await checkGlobalActivity("requests");
  await page
    .getByText("Request inspection unavailable", { exact: true })
    .waitFor();
  assert.equal(
    await page
      .getByRole("button", { exact: true, name: "Turn on inspection" })
      .count(),
    0,
    "A token project never offers inspection"
  );

  await project("Work API").click();
  await checkOverview();
  const work = page.getByRole("button", { name: /^Work, 2 projects/ });
  await work.click();
  assert.equal(
    await work.getAttribute("aria-current"),
    "location",
    "Collapsed group indicates its selected project"
  );
  assert.equal(await work.getAttribute("aria-expanded"), "false");
  assert.match(await work.getAttribute("aria-label"), /2 running/);
  await page.reload();
  await work.waitFor();
  assert.equal(
    await work.getAttribute("aria-expanded"),
    "false",
    "Group collapse survives reload"
  );
  await work.click();

  await project("Work API").click();
  await checkOverview();
  await navigate("requests:work-api");
  await waitQuery("list_requests", -1, "work-api");
  await checkGlobalActivity("requests");
  await main
    .getByRole("option", {
      exact: true,
      name: "GET /work-api/initial, status 200",
    })
    .waitFor();
  assert.equal(
    await main.getByRole("option", { name: /personal-web\/initial/ }).count(),
    0
  );
  await page.evaluate(() => {
    const mock = window.__workspaceMock;
    const [request] = mock.requests;
    mock.emit("request", {
      ...request,
      id: "live-other",
      path: "/other-live",
      tunnel_id: "personal-web",
    });
    mock.emit("request", {
      ...request,
      id: "live-work",
      path: "/work-live",
      ts: Date.now() + 1000,
    });
  });
  await main
    .getByRole("option", { exact: true, name: "GET /work-live, status 200" })
    .waitFor();
  assert.equal(
    await main.getByRole("option", { name: /other-live/ }).count(),
    0,
    "Request live events respect project scope"
  );
  await resetScope("requests", "work-api");
  await clearHistory("requests", "work-api");

  await navigate("logs:work-api");
  await waitQuery("query_logs", -1, "work-api");
  await checkGlobalActivity("logs");
  await main.getByText("Work API synthetic log", { exact: true }).waitFor();
  assert.equal(
    await main.getByText("Personal Web synthetic log", { exact: true }).count(),
    0
  );
  await page.evaluate(() => {
    const mock = window.__workspaceMock;
    mock.emit("logs", [
      {
        ...mock.logs[0],
        id: 100,
        message: "Other live log",
        tunnel_id: "personal-web",
      },
      { ...mock.logs[0], id: 101, message: "Work live log" },
    ]);
  });
  await main.getByText("Work live log", { exact: true }).waitFor();
  assert.equal(
    await main.getByText("Other live log", { exact: true }).count(),
    0,
    "Log live events respect project scope"
  );
  await resetScope("logs", "work-api");
  await clearHistory("logs", "work-api");

  let before = await queryCount("list_requests");
  await activity("requests").click();
  await waitQuery("list_requests", before, null);
  await checkGlobalActivity("requests");
  await page.getByRole("button", { exact: true, name: "Filters" }).click();
  await page
    .getByRole("menuitem", { exact: true, name: "Projects / tunnels" })
    .waitFor();
  await page.keyboard.press("Escape");
  await main
    .getByRole("option", {
      exact: true,
      name: "GET /personal-web/initial, status 200",
    })
    .waitFor();
  await main
    .getByRole("option", {
      exact: true,
      name: "GET /quick-preview/initial, status 200",
    })
    .waitFor();
  before = await queryCount("query_logs");
  await activity("logs").click();
  await waitQuery("query_logs", before, null);
  await checkGlobalActivity("logs");
  await main.getByText("Personal Web synthetic log", { exact: true }).waitFor();
  await main.getByText("Remote Token synthetic log", { exact: true }).waitFor();

  before = await queryCount("list_requests");
  await navigate("requests:personal-web");
  await waitQuery("list_requests", before, "personal-web");
  await checkGlobalActivity("requests");
  before = await queryCount("list_requests");
  await activity("requests").click();
  await waitQuery("list_requests", before, null);
  await main
    .getByRole("option", {
      exact: true,
      name: "GET /quick-preview/initial, status 200",
    })
    .waitFor();
  before = await queryCount("query_logs");
  await navigate("logs:quick-preview");
  await waitQuery("query_logs", before, "quick-preview");
  await checkGlobalActivity("logs");
  before = await queryCount("query_logs");
  await activity("logs").click();
  await waitQuery("query_logs", before, null);
  await main.getByText("Remote Token synthetic log", { exact: true }).waitFor();

  await page
    .getByRole("button", { exact: true, name: "New project or group" })
    .click();
  await page.getByRole("menuitem", { exact: true, name: "New group" }).click();
  let dialog = page.getByRole("dialog", { exact: true, name: "New group" });
  await dialog
    .getByRole("textbox", { exact: true, name: "Group name" })
    .fill("New workspace");
  await dialog
    .getByRole("button", { exact: true, name: "Create group" })
    .click();
  await dialog.waitFor({ state: "hidden" });
  await page
    .getByRole("button", { exact: true, name: "New workspace, 0 projects" })
    .waitFor();
  assert.equal((await lastCall("create_group")).args.name, "New workspace");
  await page
    .getByRole("button", { exact: true, name: "New workspace actions" })
    .click();
  await page
    .getByRole("menuitem", { exact: true, name: "Delete group" })
    .click();
  await page
    .getByRole("region", { exact: true, name: "New workspace projects" })
    .waitFor({ state: "hidden" });
  assert.equal(
    await page
      .getByRole("button", { exact: true, name: "New project or group" })
      .evaluate((button) => button === document.activeElement),
    true,
    "Deleting a group returns focus to the project menu"
  );

  await page.getByRole("button", { exact: true, name: "Work actions" }).click();
  await page
    .getByRole("menuitem", { exact: true, name: "New project here" })
    .click();
  dialog = page.getByRole("dialog", { exact: true, name: "New" });
  await dialog
    .getByRole("button", { exact: true, name: "Group Work" })
    .waitFor();
  assert.equal(
    await dialog.getByText("group-work", { exact: true }).count(),
    0,
    "Context creation displays the group name"
  );
  await dialog
    .getByRole("textbox", { exact: true, name: "Name" })
    .fill("New API");
  await dialog
    .getByRole("button", { exact: true, name: "Create project" })
    .click();
  await dialog.waitFor({ state: "hidden" });
  assert.equal(
    (await lastCall("create_project")).args.group,
    "Work",
    "Creation sends group name, not ID"
  );
  await project("New API").waitFor();
  await checkOverview();

  await page.getByRole("button", { exact: true, name: "Work actions" }).click();
  await page
    .getByRole("menuitem", { exact: true, name: "Rename group" })
    .click();
  dialog = page.getByRole("dialog", { exact: true, name: "Rename group" });
  await dialog
    .getByRole("textbox", { exact: true, name: "Group name" })
    .fill("Team");
  await dialog
    .getByRole("button", { exact: true, name: "Save changes" })
    .click();
  await dialog.waitFor({ state: "hidden" });
  await page
    .getByRole("button", { exact: true, name: "Team, 3 projects" })
    .waitFor();
  assert.deepEqual((await lastCall("rename_group")).args, {
    id: "group-work",
    name: "Team",
  });

  await project("Personal Web").click();
  await checkOverview();
  await main
    .getByRole("button", { exact: true, name: "Remove" })
    .first()
    .click();
  dialog = page.getByRole("alertdialog");
  await dialog.getByRole("button", { exact: true, name: "Remove" }).click();
  await project("Personal Web").waitFor({ state: "hidden" });
  assert.equal(
    await project("Quick Preview").getAttribute("aria-current"),
    "page",
    "Removing the selected project selects its next neighbor"
  );
  await checkOverview();
  await page
    .getByRole("button", { exact: true, name: "Personal, 0 projects" })
    .waitFor();

  await page.goto(`${baseUrl}/#tunnels`);
  await page.reload();
  await project("Work API").waitFor();
  await checkOverview();
  await mkdir("e2e/out", { recursive: true });
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/workspace-overview.png",
  });
  await navigate("requests:work-api");
  await checkGlobalActivity("requests");
  await main
    .getByRole("option", {
      exact: true,
      name: "GET /work-api/initial, status 200",
    })
    .waitFor();
  await page.screenshot({
    animations: "disabled",
    path: "e2e/out/workspace-requests.png",
  });
  assert.deepEqual(errors, [], "No browser errors");
  assert.deepEqual(
    external,
    [],
    "All content comes from the local synthetic fixture"
  );
  console.log(
    "Workspace passed: direct project settings, single sidebar, global activity with changeable project filters and live events, scoped history, groups, drag-and-drop, empty group deletion and retries, creation, keyboard navigation, deep links, deletion"
  );
} catch (error) {
  console.error(
    await page.evaluate(() => ({
      calls: window.__workspaceMock.calls.slice(-12),
      content: document.querySelector("main")?.innerText,
    }))
  );
  throw error;
} finally {
  await browser.close();
}
