import "../styles/app.css";
import { mount } from "svelte";
import { installFocusModality } from "../lib/theme";
import Popover from "./Popover.svelte";

installFocusModality();
mount(Popover, { target: document.getElementById("app")! });
