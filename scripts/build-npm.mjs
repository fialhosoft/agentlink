#!/usr/bin/env node
// Assembles the npm packages for a release.
//
// agentlink is a Rust binary, but its audience discovers tools through `npx`.
// The established way to serve both is the pattern esbuild, swc, Biome and
// Rolldown use: one thin wrapper package that declares an optional dependency
// per platform, each carrying a prebuilt binary. npm installs only the one
// matching the host, so `npm i -g @fialhosoft/agentlink` downloads a single
// binary and no Rust toolchain is involved. The package is scoped under the
// `@fialhosoft` npm org rather than a standalone `agentlink` name; the `bin`
// field below still exposes a bare `agentlink` command once installed.
//
// Usage: node scripts/build-npm.mjs <version> <binaries-dir>

import { chmodSync, cpSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [version, binariesDir] = process.argv.slice(2);
if (!version || !binariesDir) {
  console.error("usage: build-npm.mjs <version> <binaries-dir>");
  process.exit(1);
}

/** Platform packages, keyed by the artifact directory name from CI. */
const PLATFORMS = [
  { key: "linux-x64", os: "linux", cpu: "x64", exe: "agentlink" },
  { key: "linux-arm64", os: "linux", cpu: "arm64", exe: "agentlink" },
  { key: "darwin-x64", os: "darwin", cpu: "x64", exe: "agentlink" },
  { key: "darwin-arm64", os: "darwin", cpu: "arm64", exe: "agentlink" },
  { key: "win32-x64", os: "win32", cpu: "x64", exe: "agentlink.exe" },
  { key: "win32-arm64", os: "win32", cpu: "arm64", exe: "agentlink.exe" },
];

const SHARED = {
  version,
  license: "Apache-2.0",
  homepage: "https://github.com/fialhosoft/agentlink",
  repository: { type: "git", url: "git+https://github.com/fialhosoft/agentlink.git" },
  bugs: { url: "https://github.com/fialhosoft/agentlink/issues" },
};

const out = "npm/dist";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

const available = new Set(readdirSync(binariesDir).map((d) => d.replace(/^bin-/, "")));
const optionalDependencies = {};

for (const platform of PLATFORMS) {
  if (!available.has(platform.key)) {
    // A missing target must not silently ship a wrapper that cannot resolve it.
    console.error(`missing binary artifact for ${platform.key}`);
    process.exit(1);
  }

  const name = `@fialhosoft/agentlink-${platform.key}`;
  const dir = join(out, `agentlink-${platform.key}`);
  mkdirSync(dir, { recursive: true });

  const source = join(binariesDir, `bin-${platform.key}`, platform.exe);
  const target = join(dir, platform.exe);
  cpSync(source, target);
  // npm preserves the mode recorded in the tarball, so this is what makes the
  // binary executable after install on Unix.
  chmodSync(target, 0o755);

  writeFileSync(
    join(dir, "package.json"),
    `${JSON.stringify(
      {
        name,
        ...SHARED,
        description: `agentlink binary for ${platform.os}-${platform.cpu}`,
        os: [platform.os],
        cpu: [platform.cpu],
        files: [platform.exe],
      },
      null,
      2,
    )}\n`,
  );

  optionalDependencies[name] = version;
}

// The wrapper package everyone actually installs.
const wrapper = join(out, "agentlink");
mkdirSync(join(wrapper, "bin"), { recursive: true });
cpSync("npm/agentlink/bin/agentlink.js", join(wrapper, "bin/agentlink.js"));
cpSync("README.md", join(wrapper, "README.md"));
cpSync("LICENSE", join(wrapper, "LICENSE"));

writeFileSync(
  join(wrapper, "package.json"),
  `${JSON.stringify(
    {
      name: "@fialhosoft/agentlink",
      ...SHARED,
      description:
        "One brain for every AI coding agent — shared rules and skills with zero file duplication",
      keywords: ["ai", "agents", "claude", "cursor", "copilot", "codex", "skills", "agents-md"],
      bin: { agentlink: "bin/agentlink.js" },
      files: ["bin"],
      optionalDependencies,
      engines: { node: ">=18" },
    },
    null,
    2,
  )}\n`,
);

console.log(`assembled ${PLATFORMS.length + 1} packages for v${version} in ${out}`);
