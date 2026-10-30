import React from "react";
import { createRoot } from "react-dom/client";
import Overlay from "./Overlay";
import Settings from "./Settings";
import "./styles.css";

const isOverlay = new URLSearchParams(window.location.search).get("window") === "overlay";

// The overlay window is transparent: the desktop is its backdrop. Only the
// settings window is allowed to paint a background.
if (!isOverlay) document.body.classList.add("settings");

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isOverlay ? <Overlay /> : <Settings />}</React.StrictMode>
);
