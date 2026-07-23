// Installs a seedable, in-page fake of the Tauri backend before the app loads. main.js reads
// globalThis.__TAURI__ at call time, so setting it here is all that's needed — the real UI runs
// unchanged. The fake persists its store in localStorage so scan history, unread, and per-item
// pin/flag survive page.reload() (a "relaunch"), exactly like the real on-disk store.
//
// `config` is a plain, JSON-serializable object (passed into the page):
//   { report, runs, failing, telemetryEnabled, telemetrySuppressed, telemetryPreview,
//     diagnostics, exportPath }

export async function installFakeBackend(page, config) {
  await page.addInitScript((cfg) => {
    const KEY = "__pd_fake_store";

    const humanSize = (b) => {
      b = Number(b) || 0;
      if (b < 1024) return `${b} B`;
      const u = ["KB", "MB", "GB", "TB"];
      let v = b / 1024;
      let i = 0;
      while (v >= 1024 && i < u.length - 1) { v /= 1024; i += 1; }
      return `${v.toFixed(1)} ${u[i]}`;
    };
    const headlineOf = (report) => {
      const by = {};
      for (const f of report.findings || []) by[f.tool] = (by[f.tool] || 0) + (Number(f.size_bytes) || 0);
      let top = null;
      for (const t in by) if (!top || by[t] > top.b) top = { t, b: by[t] };
      return top ? `${humanSize(top.b)} · ${top.t}` : "No traces";
    };
    const entryOf = (runId, report, unread) => ({
      run_id: runId,
      ran_at: report.generated_at || "2026-07-17T14:12:00Z",
      exposure: report.exposure,
      confidence: report.assurance,
      headline: headlineOf(report),
      trace_count: report.summary?.total_findings || 0,
      unread,
    });

    // Seed once per browser context; later navigations reuse the persisted store.
    if (!localStorage.getItem(KEY)) {
      const runs = cfg.runs || [];
      const store = {
        index: runs.map((r) => entryOf(r.run_id, r.report, r.unread ?? false)),
        runs: Object.fromEntries(runs.map((r) => [r.run_id, { run_id: r.run_id, report: r.report, item_state: r.item_state || {} }])),
      };
      localStorage.setItem(KEY, JSON.stringify(store));
    }
    const load = () => JSON.parse(localStorage.getItem(KEY));
    const save = (s) => localStorage.setItem(KEY, JSON.stringify(s));
    let seq = 0;
    let telemetry = !!cfg.telemetryEnabled;

    const impl = {
      list_scans: () => JSON.stringify(load().index),
      load_scan: ({ runId }) => JSON.stringify(load().runs[runId]),
      run_scan: () => JSON.stringify(cfg.report),
      save_scan: ({ reportJson }) => {
        const report = JSON.parse(reportJson);
        const id = (Date.now().toString(16) + (seq++) + "0".repeat(32)).slice(0, 32);
        const entry = entryOf(id, report, true);
        const s = load();
        s.index.unshift(entry);
        s.runs[id] = { run_id: id, report, item_state: {} };
        save(s);
        return JSON.stringify(entry);
      },
      mark_scan_read: ({ runId }) => {
        const s = load();
        const e = s.index.find((x) => x.run_id === runId);
        if (e) e.unread = false;
        save(s);
      },
      set_finding_state: ({ runId, path, patch }) => {
        const s = load();
        const r = s.runs[runId];
        if (r) {
          const st = r.item_state[path] || { read: false, pinned: false, flagged: false };
          Object.assign(st, patch);
          r.item_state[path] = st;
          save(s);
        }
      },
      telemetry_status: () => JSON.stringify({ enabled: telemetry, suppressed_by_env: !!cfg.telemetrySuppressed }),
      telemetry_set_enabled: ({ enabled }) => { telemetry = enabled; },
      telemetry_preview: () => cfg.telemetryPreview || '{\n  "kind": "promptdust-telemetry",\n  "run_id": "deadbeef1234"\n}',
      diagnostics: () => cfg.diagnostics || '{\n  "kind": "promptdust-diagnostics"\n}',
      reveal: () => { globalThis.__pd_revealed = true; },
      export_report: () => cfg.exportPath || "/Users/x/Downloads/promptdust-report.md",
      share: () => { if (cfg.shareFails) throw new Error("the native share sheet is available on macOS only"); globalThis.__pd_shared = true; },
    };

    const failing = cfg.failing || {};
    globalThis.__TAURI__ = {
      core: {
        invoke: (cmd, args) => {
          if (failing[cmd]) return Promise.reject(new Error(failing[cmd]));
          try {
            return Promise.resolve(impl[cmd] ? impl[cmd](args) : undefined);
          } catch (e) {
            return Promise.reject(e);
          }
        },
      },
      // The opt-in self-updater API. Default: up to date (cfg.update === null).
      updater: { check: () => Promise.resolve(cfg.update || null) },
    };
  }, config);
}
