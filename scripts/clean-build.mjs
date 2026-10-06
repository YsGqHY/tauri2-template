import { lstat, readdir, rm } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import readline from "node:readline/promises";

const rootDirectory = process.cwd();

const RELEASE_CACHE_DIRECTORIES = [
  [".fingerprint", "Cargo fingerprint 缓存"],
  ["build", "Rust build.rs 中间产物"],
  ["deps", "Rust 依赖编译中间产物"],
  ["examples", "Cargo examples 编译输出"],
  ["incremental", "Rust 增量编译缓存"],
];

const RELEASE_TEMP_DIRECTORIES = [
  ["nsis", "NSIS 打包 staging 目录"],
  ["wix", "WiX 打包 staging 目录"],
];

const FRONTEND_DIRECTORIES = [
  ["dist", "Vite/Tauri 前端构建产物（可重建）"],
  ["dist-ssr", "Vite SSR 构建产物（可重建）"],
  [".vite", "Vite 缓存"],
  ["coverage", "测试覆盖率报告（可重建）"],
];

const ROOT_LOG_PATTERNS = [
  (name) => name.endsWith(".log"),
  (name) => name.startsWith("npm-debug.log"),
  (name) => name.startsWith("yarn-debug.log"),
  (name) => name.startsWith("yarn-error.log"),
  (name) => name.startsWith("pnpm-debug.log"),
  (name) => name.startsWith("lerna-debug.log"),
];

const PROTECTED_PATHS = [
  "target/release/bundle",
  "src-tauri/target/release/bundle",
  "target/release/resources",
  "src-tauri/target/release/resources",
];

const options = parseArguments(process.argv.slice(2));

if (options.help) {
  printUsage();
  process.exit(0);
}

const candidates = await collectCandidates(options);

printPlan(candidates, options);

if (candidates.length === 0) {
  console.log("\n没有发现符合当前清理范围的产物。\n");
  process.exit(0);
}

if (options.dryRun) {
  console.log("\n预览模式：未删除任何文件。\n");
  process.exit(0);
}

if (!options.yes && !process.stdin.isTTY) {
  console.error("\n当前不是交互终端，已拒绝删除。需要无人值守清理时请显式传入 --yes。\n");
  process.exitCode = 2;
} else if (!options.yes && !(await requestConfirmation())) {
  console.log("\n已取消，未删除任何文件。\n");
} else {
  await removeCandidates(candidates);
}

function parseArguments(argumentsList) {
  const parsed = {
    cleanFrontend: false,
    dryRun: false,
    help: false,
    includeDebug: false,
    includeLogs: false,
    yes: false,
  };

  for (const argument of argumentsList) {
    switch (argument) {
      case "--":
        break;
      case "--clean-frontend":
        parsed.cleanFrontend = true;
        break;
      case "--dry-run":
        parsed.dryRun = true;
        break;
      case "--help":
      case "-h":
        parsed.help = true;
        break;
      case "--include-debug":
        parsed.includeDebug = true;
        break;
      case "--include-logs":
        parsed.includeLogs = true;
        break;
      case "--yes":
      case "-y":
        parsed.yes = true;
        break;
      default:
        console.error(`未知参数：${argument}`);
        printUsage();
        process.exit(2);
    }
  }

  return parsed;
}

function printUsage() {
  console.log(`用法：node scripts/clean-build.mjs [选项]

默认行为：预览并确认清理 release 编译缓存、Cargo 中间产物、打包 staging 和 Tauri 生成 schema。

选项：
  --dry-run          只列出候选路径和大小，不删除
  --yes, -y          跳过交互确认（仍会打印完整清单，适合 CI）
  --clean-frontend   额外删除 dist、Vite 缓存、coverage、*.tsbuildinfo
  --include-debug    额外删除 target/debug，下一次调试会重新编译
  --include-logs     额外删除根目录 logs/ 和根目录日志文件
  --help, -h         显示帮助
`);
}

async function collectCandidates(cleanOptions) {
  const candidateMap = new Map();

  for (const cargoTarget of ["target", "src-tauri/target"]) {
    const releaseRoot = path.posix.join(cargoTarget, "release");

    for (const [directory, reason] of RELEASE_CACHE_DIRECTORIES) {
      await addCandidate(
        candidateMap,
        path.posix.join(releaseRoot, directory),
        "release 编译缓存/中间产物",
        reason,
      );
    }

    for (const [directory, reason] of RELEASE_TEMP_DIRECTORIES) {
      await addCandidate(
        candidateMap,
        path.posix.join(releaseRoot, directory),
        "Tauri 打包临时目录",
        reason,
      );
    }

    await collectReleaseMetadata(candidateMap, releaseRoot);

    if (cleanOptions.includeDebug) {
      await addCandidate(
        candidateMap,
        path.posix.join(cargoTarget, "debug"),
        "debug 构建产物",
        "显式选择的完整 debug 输出；后续调试会重新编译",
      );
    }
  }

  await addCandidate(
    candidateMap,
    "src-tauri/gen/schemas",
    "Tauri 生成文件",
    "能力 schema 补全文件，可由构建重新生成",
  );

  if (cleanOptions.cleanFrontend) {
    for (const [directory, reason] of FRONTEND_DIRECTORIES) {
      await addCandidate(candidateMap, directory, "前端可重建产物", reason);
    }
    await collectRootFiles(
      candidateMap,
      (name) => name.endsWith(".tsbuildinfo"),
      "前端编译缓存",
      "TypeScript 增量编译信息，可由构建重新生成",
    );
  }

  if (cleanOptions.includeLogs) {
    await addCandidate(
      candidateMap,
      "logs",
      "日志",
      "项目根目录日志；显式选择后清理",
    );
    await collectRootFiles(
      candidateMap,
      (name) => ROOT_LOG_PATTERNS.some((matches) => matches(name)),
      "日志",
      "项目根目录构建/包管理日志；显式选择后清理",
    );
  }

  return [...candidateMap.values()].sort((left, right) =>
    left.relativePath.localeCompare(right.relativePath),
  );
}

async function collectReleaseMetadata(candidateMap, releaseRoot) {
  for (const fileName of [
    ".cargo-artifact-lock",
    ".cargo-build-lock",
    ".cargo-lock",
  ]) {
    await addCandidate(
      candidateMap,
      path.posix.join(releaseRoot, fileName),
      "release 编译元数据",
      "Cargo 生成的临时锁文件",
    );
  }

  const releaseDirectory = absolutePath(releaseRoot);
  const entries = await readDirectory(releaseDirectory);

  for (const entry of entries) {
    if (entry.isFile() && entry.name.endsWith(".d")) {
      await addCandidate(
        candidateMap,
        path.posix.join(releaseRoot, entry.name),
        "release 编译元数据",
        "Cargo 依赖描述文件，可由下一次构建重新生成",
      );
    }
  }
}

async function collectRootFiles(candidateMap, matches, category, reason) {
  const entries = await readDirectory(rootDirectory);

  for (const entry of entries) {
    if (matches(entry.name)) {
      await addCandidate(candidateMap, entry.name, category, reason);
    }
  }
}

async function addCandidate(candidateMap, relativePath, category, reason) {
  const normalizedPath = relativePath.split(path.sep).join(path.posix.sep);
  const absolute = absolutePath(normalizedPath);
  const entry = await getEntry(absolute);

  if (!entry) {
    return;
  }

  assertSafeCandidate(absolute);

  const key = process.platform === "win32" ? absolute.toLowerCase() : absolute;
  if (!candidateMap.has(key)) {
    candidateMap.set(key, {
      absolutePath: absolute,
      category,
      reason,
      relativePath: toDisplayPath(absolute),
      size: await getSize(absolute, entry),
    });
  }
}

function absolutePath(relativePath) {
  return path.resolve(rootDirectory, relativePath);
}

function toDisplayPath(absolute) {
  const relative = path.relative(rootDirectory, absolute);
  return relative.split(path.sep).join("/");
}

function assertSafeCandidate(absolute) {
  const relative = path.relative(rootDirectory, absolute);

  if (
    relative.length === 0 ||
    relative === ".." ||
    relative.startsWith(`..${path.sep}`) ||
    path.isAbsolute(relative)
  ) {
    throw new Error(`拒绝删除仓库外或仓库根目录路径：${absolute}`);
  }

  for (const protectedPath of PROTECTED_PATHS) {
    const protectedAbsolute = absolutePath(protectedPath);
    const relativeToProtected = path.relative(protectedAbsolute, absolute);
    const isInsideProtectedPath =
      relativeToProtected.length === 0 ||
      (!relativeToProtected.startsWith(`..${path.sep}`) &&
        !path.isAbsolute(relativeToProtected));

    if (isInsideProtectedPath) {
      throw new Error(`拒绝删除受保护发布路径：${toDisplayPath(absolute)}`);
    }
  }
}

async function getEntry(absolute) {
  try {
    return await lstat(absolute);
  } catch (error) {
    if (error?.code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

async function readDirectory(absolute) {
  try {
    return await readdir(absolute, { withFileTypes: true });
  } catch (error) {
    if (error?.code === "ENOENT" || error?.code === "ENOTDIR") {
      return [];
    }
    throw error;
  }
}

async function getSize(absolute, entry) {
  if (!entry.isDirectory() || entry.isSymbolicLink()) {
    return entry.size;
  }

  let total = entry.size;
  const children = await readDirectory(absolute);

  for (const child of children) {
    const childAbsolute = path.join(absolute, child.name);
    const childEntry = await getEntry(childAbsolute);
    if (childEntry) {
      total += await getSize(childAbsolute, childEntry);
    }
  }

  return total;
}

function printPlan(candidates, cleanOptions) {
  console.log("安全构建产物清理");
  console.log(`工作区：${rootDirectory}`);
  console.log(
    `范围：${[
      "release 缓存/中间产物",
      cleanOptions.cleanFrontend ? "前端可重建产物" : null,
      cleanOptions.includeDebug ? "debug 构建产物" : null,
      cleanOptions.includeLogs ? "日志" : null,
    ]
      .filter(Boolean)
      .join("、")}`,
  );

  console.log("\n将清理：");
  for (const candidate of candidates) {
    console.log(
      `  - [${candidate.category}] ${candidate.relativePath} (${formatBytes(candidate.size)})\n    ${candidate.reason}`,
    );
  }

  const totalSize = candidates.reduce((sum, candidate) => sum + candidate.size, 0);
  console.log(`\n候选项：${candidates.length} 项，约 ${formatBytes(totalSize)}`);

  console.log("\n始终保留：");
  console.log("  - target/release/bundle/**、src-tauri/target/release/bundle/**：MSI、NSIS、DMG、APP、DEB、RPM、AppImage 等最终发布产物");
  console.log("  - 两套 target/release/resources/**：运行时资源");
  console.log("  - 两套 target/release/ 下的应用可执行文件和 *.pdb：运行与 release 调试所需");
  console.log("  - 源代码、配置、package lock、Cargo.lock、node_modules、.pnpm-store 和 Cargo registry/cache");

  if (cleanOptions.yes) {
    console.log("\n已启用 --yes：确认步骤将被跳过。\n");
  }
}

async function requestConfirmation() {
  const prompt = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  try {
    const answer = await prompt.question("确认删除以上路径？输入 yes 或 y 继续，其他输入取消：[y/N] ");
    return /^(?:y|yes)$/i.test(answer.trim());
  } finally {
    prompt.close();
  }
}

async function removeCandidates(candidates) {
  let failed = 0;

  console.log("开始清理……");
  for (const candidate of candidates) {
    try {
      assertSafeCandidate(candidate.absolutePath);
      await rm(candidate.absolutePath, {
        force: true,
        maxRetries: 2,
        recursive: true,
        retryDelay: 100,
      });
      console.log(`  ✓ 已删除 ${candidate.relativePath}`);
    } catch (error) {
      failed += 1;
      console.error(`  ✗ 删除失败 ${candidate.relativePath}: ${error.message}`);
    }
  }

  if (failed > 0) {
    console.error(`\n清理完成，但有 ${failed} 项删除失败。请确认应用和构建进程已关闭后重试。\n`);
    process.exitCode = 1;
    return;
  }

  const totalSize = candidates.reduce((sum, candidate) => sum + candidate.size, 0);
  console.log(`\n清理完成，已释放约 ${formatBytes(totalSize)}。\n`);
}

function formatBytes(bytes) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unitIndex = -1;

  do {
    value /= 1024;
    unitIndex += 1;
  } while (value >= 1024 && unitIndex < units.length - 1);

  return `${value.toFixed(value >= 10 ? 1 : 2)} ${units[unitIndex]}`;
}
