import "./styles.css"
import React from "react"
import ReactDOM from "react-dom/client"

import App from "./App"
import { installGlobalErrorLogging } from "./adapters/errorLog"
import { installTauriApi } from "./host/tauriApi"

// First, before anything reads it: the Electron build had a preload script put
// window.api there before the document ran, and nothing in the renderer checks
// whether it is present.
installTauriApi()

// Before the first render, so a throw while the tree is mounting is logged too.
installGlobalErrorLogging()

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
)
