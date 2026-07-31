import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";

export type UpdateInfo = {
  version: string;
  body: string | null;
  currentVersion: string;
};

let cached: Update | null = null;

export async function getAppVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "0.1.0";
  }
}

/** Returns update info if a newer version is available; otherwise null. */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
    const update = await check();
    if (!update) {
      cached = null;
      return null;
    }
    cached = update;
    const currentVersion = await getAppVersion();
    return {
      version: update.version,
      body: update.body ?? null,
      currentVersion,
    };
  } catch {
    // No endpoint / offline / unsigned — treat as no update
    cached = null;
    return null;
  }
}

export async function installAvailableUpdate(
  onProgress?: (pct: number | null) => void,
): Promise<void> {
  let update = cached;
  if (!update) {
    update = await check();
  }
  if (!update) {
    throw new Error("No update available");
  }

  let downloaded = 0;
  let contentLength: number | null = null;

  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        contentLength = event.data.contentLength ?? null;
        onProgress?.(0);
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        if (contentLength && contentLength > 0) {
          onProgress?.(Math.min(99, Math.round((downloaded / contentLength) * 100)));
        } else {
          onProgress?.(null);
        }
        break;
      case "Finished":
        onProgress?.(100);
        break;
    }
  });

  await relaunch();
}
