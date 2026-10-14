import "../styles/app.css";
import { mount } from "svelte";
import { installFocusModality } from "../lib/theme";
import App from "./App.svelte";

installFocusModality();
const target = document.getElementById("app");
if (!target) {
  throw new Error("Dashboard mount target is missing");
}
mount(App, { target });
