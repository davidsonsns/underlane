import { describe, expect, it } from "vitest";
import { projectFilename } from "./project-filename";

describe("project config destination", () => {
  it.each([
    ["Storefront", "storefront.yml"],
    ["  My  App  ", "my-app.yml"],
    ["../../outside", "outside.yml"],
    ["💻", "project.yml"],
    ["Équipe Web", "équipe-web.yml"],
    ["ΟΣ", "οσ.yml"],
    ["CON", "project-con.yml"],
  ])("uses a safe filename for %s", (name, filename) => {
    expect(projectFilename(name)).toBe(filename);
  });

  it("truncates at a UTF-8 character boundary", () => {
    expect(projectFilename("界".repeat(50))).toBe(`${"界".repeat(40)}.yml`);
  });
});
