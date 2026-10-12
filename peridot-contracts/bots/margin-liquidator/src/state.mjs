import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

const EMPTY_STATE = {
  initialized: false,
  lastEventLedger: null,
  activePositionIds: [],
};

export async function loadState(path) {
  try {
    const parsed = JSON.parse(await readFile(path, "utf8"));
    return {
      initialized: parsed.initialized === true,
      lastEventLedger: Number.isInteger(parsed.lastEventLedger)
        ? parsed.lastEventLedger
        : null,
      activePositionIds: Array.isArray(parsed.activePositionIds)
        ? [...new Set(parsed.activePositionIds.map(String))]
        : [],
    };
  } catch (error) {
    if (error.code === "ENOENT") return structuredClone(EMPTY_STATE);
    throw error;
  }
}

export async function saveState(path, state) {
  await mkdir(dirname(path), { recursive: true });
  const temporary = `${path}.tmp`;
  await writeFile(temporary, `${JSON.stringify(state, null, 2)}\n`, { mode: 0o600 });
  await rename(temporary, path);
}
