import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { CrashBoundary, installUiErrorHandlers } from "./lib/crash";

installUiErrorHandlers();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <CrashBoundary>
      <App />
    </CrashBoundary>
  </React.StrictMode>,
);
