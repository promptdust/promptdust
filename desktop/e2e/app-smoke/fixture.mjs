// Builds a synthetic home tree the real scan will detect, so the real-app smoke run is
// deterministic and writes nothing to the real machine. Verified: this fixture yields Claude
// Code findings under `PROMPTDUST_HOME`. Reused by wdio.conf (onPrepare) and runnable directly
// (`node fixture.mjs`) before the suite in CI.
import { mkdir, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";

export async function makeFixtureHome(home) {
  await rm(home, { recursive: true, force: true });
  const project = join(home, ".claude", "projects", "demo-project");
  await mkdir(project, { recursive: true });
  await writeFile(
    join(project, "session.jsonl"),
    '{"role":"user","content":"x"}\n{"role":"assistant","content":"y"}\n',
  );
  await writeFile(join(home, ".claude.json"), '{"numStartups":3,"account":{"email":"x"}}\n');
  return home;
}

// Direct run (`node fixture.mjs`): build the fixture at $PROMPTDUST_HOME (or argv[2]).
if (import.meta.url === `file://${process.argv[1]}`) {
  const home = process.env.PROMPTDUST_HOME || process.argv[2];
  if (!home) {
    console.error("set PROMPTDUST_HOME (or pass a path) to build the fixture home");
    process.exit(1);
  }
  await makeFixtureHome(home);
  console.log(`fixture home created at ${home}`);
}
