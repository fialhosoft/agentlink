#!/usr/bin/env node
// Locates the prebuilt binary for this platform and hands control to it.
//
// npm installs exactly one of the `@fialhosoft/agentlink-*` optional
// dependencies — whichever matches the host's `os` and `cpu` — so resolution
// here is a lookup, not a download.

import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

const PLATFORM = `${process.platform}-${process.arch}`;
const EXECUTABLE = process.platform === "win32" ? "agentlink.exe" : "agentlink";

function resolveBinary() {
  try {
    return require.resolve(`@fialhosoft/agentlink-${PLATFORM}/${EXECUTABLE}`);
  } catch {
    return null;
  }
}

const binary = resolveBinary();

if (!binary) {
  console.error(
    [
      `agentlink does not ship a prebuilt binary for ${PLATFORM}.`,
      "",
      "If your platform should be supported, this usually means the optional",
      "dependency was skipped during install. Try:",
      "",
      "  npm install @fialhosoft/agentlink --force",
      "",
      "Otherwise, build from source:",
      "",
      "  cargo install agentlink-cli",
      "",
      "and please open an issue at https://github.com/fialhosoft/agentlink/issues",
    ].join("\n"),
  );
  process.exit(1);
}

// `stdio: "inherit"` keeps the binary's colour detection and terminal handling
// intact, and forwarding the exit code preserves `status --check`'s contract
// with CI.
const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  console.error(`agentlink failed to start: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
