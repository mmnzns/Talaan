import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { meetingStore, settingsStore } from "./data";
import { StoreProvider } from "./data/StoreProvider";
import { ToastProvider } from "./ui";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <StoreProvider meetings={meetingStore} settings={settingsStore}>
      <ToastProvider>
        <App />
      </ToastProvider>
    </StoreProvider>
  </React.StrictMode>,
);
