import { createTheme } from "@mui/material";

export const hd2Theme = createTheme({
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
