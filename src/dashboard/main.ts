import "../styles/base.css";
import { mount } from "svelte";
import { installFocusModality } from "../lib/theme";
import App from "./App.svelte";

installFocusModality();
mount(App, { target: document.getElementById("app")! });
