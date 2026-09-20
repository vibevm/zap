#!/usr/bin/env node
/** Compose the five-platform Zap release index. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { APPLICATION } from "./manifest.mjs";

const TAG = "v1.0.0";
const REPOSITORY = "vibevm/zap";

export async function createIndex(receiptPaths, output) {
  if (receiptPaths.length !== 5) failure("exactly five platform receipts are required");
  const receipts = await Promise.all(
    receiptPaths.map(async (path) => JSON.parse(await readFile(resolve(path), "utf8"))),
  );
  const identities = new Set();
  let source;
  const distributions = receipts.map((receipt) => {
    if (
      receipt.protocol !== "zap-distribution-build/1" ||
      receipt.application !== "org.vibevm.zap/zap@1.0.0"
    )
      failure("receipt identity is invalid");
    const identity = `${receipt.target.os}/${receipt.target.arch}/${receipt.target.libc ?? "none"}/${receipt.target.format}`;
    if (identities.has(identity)) failure(`duplicate platform receipt: ${identity}`);
    identities.add(identity);
    const observed = `${receipt.source.commit}\n${receipt.source.tree}`;
    if (source === undefined) source = observed;
    else if (source !== observed) failure("platform receipts name different source snapshots");
    return {
      os: receipt.target.os,
      arch: receipt.target.arch,
      ...(receipt.target.libc === undefined ? {} : { libc: receipt.target.libc }),
      format: receipt.target.format,
      url: `https://github.com/${REPOSITORY}/releases/download/${TAG}/${receipt.archive.file}`,
      sha256: receipt.archive.sha256,
      size: receipt.archive.size,
      sourceCommit: receipt.source.commit,
      sourceTree: receipt.source.tree,
    };
  });
  const expected = new Set([
    "windows/x86_64/none/zip",
    "linux/x86_64/musl/zip",
    "linux/x86_64/gnu/zip",
    "macos/x86_64/none/zip",
    "macos/aarch64/none/zip",
  ]);
  if (identities.size !== expected.size || [...expected].some((entry) => !identities.has(entry)))
    failure("platform receipt set is incomplete");
  distributions.sort((left, right) =>
    `${left.os}/${left.arch}/${left.libc ?? "none"}`.localeCompare(
      `${right.os}/${right.arch}/${right.libc ?? "none"}`,
      "en",
    ),
  );
  const index = {
    protocol: "vibe-application-distribution-index/1",
    application: {
      id: APPLICATION.id,
      package: { ...APPLICATION.package },
      installerPackage: { ...APPLICATION.installerPackage },
      commands: [...APPLICATION.commands],
    },
    releaseTag: TAG,
    distributions,
  };
  await writeFile(resolve(output), `${JSON.stringify(index, null, 2)}\n`, "utf8");
  return index;
}

function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-017#binary: ${message}; fix surface: rebuild all five Zap distributions from one source snapshot`,
  );
}

async function main() {
  const args = process.argv.slice(2);
  const outputAt = args.indexOf("--output");
  if (outputAt < 0 || outputAt + 1 >= args.length) failure("--output is required");
  const output = args[outputAt + 1];
  const receipts = args.filter((_, index) => index !== outputAt && index !== outputAt + 1);
  await createIndex(receipts, output);
}

if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
  await main();
