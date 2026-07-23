// WebdriverIO config for the real-app smoke suite. tauri-driver is the WebDriver intermediary
// that launches the built app and proxies to the platform webview driver (WebKitWebDriver on
// Linux, Edge driver on Windows). There is no macOS equivalent, so this layer is CI-only.
//
// The app is pointed at a synthetic home (PROMPTDUST_HOME) and a throwaway config dir
// (PROMPTDUST_CONFIG_DIR) so the real scan is deterministic and side-effect-free.
import { spawn } from "node:child_process";
import { makeFixtureHome } from "./fixture.mjs";

let tauriDriver;

export const config = {
  runner: "local",
  specs: ["./features/**/*.feature"],
  maxInstances: 1,
  capabilities: [
    {
      // Launch the built desktop binary through tauri-driver.
      "tauri:options": {
        application:
          process.env.PD_APP_BINARY || "../../src-tauri/target/release/promptdust-desktop",
      },
    },
  ],
  logLevel: "warn",
  waitforTimeout: 20000,
  framework: "cucumber",
  reporters: ["spec"],
  cucumberOpts: {
    require: ["./steps/**/*.js"],
    timeout: 60000,
  },

  // Isolate the scan + storage before any session starts.
  onPrepare: async () => {
    process.env.PROMPTDUST_TELEMETRY = process.env.PROMPTDUST_TELEMETRY || "0";
    if (process.env.PROMPTDUST_HOME) await makeFixtureHome(process.env.PROMPTDUST_HOME);
  },
  // tauri-driver must be running for the whole session; start it before, kill it after.
  beforeSession: () => {
    tauriDriver = spawn("tauri-driver", [], { stdio: [null, process.stdout, process.stderr] });
  },
  afterSession: () => {
    tauriDriver?.kill();
  },
};
