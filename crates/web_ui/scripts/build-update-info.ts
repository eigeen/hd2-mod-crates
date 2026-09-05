import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildUpdateInfo } from "./updateInfoBuilder";

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sharedAppRoot = resolve(webRoot, "../migrator_app");

await buildUpdateInfo({
  sourceDirectory: resolve(webRoot, "update-info/releases"),
  outputDirectory: resolve(sharedAppRoot, "public/update-info"),
});
