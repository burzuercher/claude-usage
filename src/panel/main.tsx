// Entry for the iTerm2 toolbelt panel (panel.html). It runs in iTerm's own web
// view, not in Tauri, so it reads data over HTTP from the app's local server
// instead of `invoke`.
import React from "react";
import ReactDOM from "react-dom/client";
import { Panel } from "./Panel";
import "./panel.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Panel />
  </React.StrictMode>
);
