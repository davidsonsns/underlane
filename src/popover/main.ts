import "../styles/base.css";
import { mount } from "svelte";
import Popover from "./Popover.svelte";

mount(Popover, { target: document.getElementById("app")! });
