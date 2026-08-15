import React from "react";
import ReactDOM from "react-dom/client";
import { CssBaseline, ThemeProvider, createTheme } from "@mui/material";
import { I18nProvider } from "../../web_ui/src/i18n";
import "../../web_ui/src/styles.css";
import App from "./App";

const hd2Theme = createTheme({
  palette: {
    mode: "dark",
    primary: { main: "#fee70f", contrastText: "#111820" },
    warning: { main: "#f59e0b" },
    background: { default: "#111820", paper: "#0c0c0c" },
    text: { primary: "#fdfdfd", secondary: "#aaaaaa" },
  },
  shape: { borderRadius: 0 },
  typography: {
    fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, sans-serif",
  },
});

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
