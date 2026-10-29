import "@fontsource-variable/instrument-sans/wdth.css";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });
