import React from "react";
import { createRoot } from "react-dom/client";
import Overlay from "./Overlay";
import Settings from "./Settings";
import "./styles.css";

const windowKind = new URLSearchParams(window.location.search).get("window");
const isOverlay = windowKind === "overlay";
const isBoard = windowKind === "board";

// The overlay window is transparent: the desktop is its backdrop. The settings
// window paints its own paper. The Board is the one opaque white surface.
if (!isOverlay && !isBoard) document.body.classList.add("settings");
if (isBoard) document.body.classList.add("board");

// three.js is only for the Board; keep it out of the cue overlay's bundle.
const BoardHotline = React.lazy(() => import("./BoardHotline"));

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isOverlay ? (
      <Overlay />
    ) : isBoard ? (
      <React.Suspense fallback={null}>
        <BoardHotline />
      </React.Suspense>
    ) : (
      <Settings />
    )}
  </React.StrictMode>
);
