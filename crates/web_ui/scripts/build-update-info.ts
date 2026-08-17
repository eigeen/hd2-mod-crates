import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildUpdateInfo } from "./updateInfoBuilder";

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

await buildUpdateInfo({
  sourceDirectory: resolve(webRoot, "update-info/releases"),
  outputDirectory: resolve(webRoot, "public/update-info"),
});
