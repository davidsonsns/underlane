// UI lab entry (dev only). ?theme=light|dark|system pins the theme the way the app does
// (applyTheme sets or removes html[data-theme]); no param = system.
import "../styles/app.css";
import { mount } from "svelte";
import { applyTheme, installFocusModality } from "$lib/theme";
import Lab from "./Lab.svelte";

const t = new URLSearchParams(location.search).get("theme");
applyTheme(t === "light" || t === "dark" ? t : "system");
installFocusModality();
mount(Lab, { target: document.getElementById("app")! });
