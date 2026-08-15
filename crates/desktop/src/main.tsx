import React from "react";
import ReactDOM from "react-dom/client";
import { CssBaseline, ThemeProvider } from "@mui/material";
import { I18nProvider, hd2Theme } from "@hd2-mod-tools/migrator-ui";
import "@hd2-mod-tools/migrator-ui/styles.css";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider theme={hd2Theme}>
      <CssBaseline />
      <I18nProvider>
        <App />
      </I18nProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
