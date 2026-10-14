import "../styles/app.css";
import { mount } from "svelte";
import { installFocusModality } from "../lib/theme";
import Popover from "./Popover.svelte";

installFocusModality();
const target = document.getElementById("app");
if (!target) {
  throw new Error("Missing popover mount element");
}
mount(Popover, { target });
