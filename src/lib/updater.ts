import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

export type UpdateInfo = {
  version: string;
  body: string | null;
  currentVersion: string;
  /** When set, in-app signed install isn't available — download/open this URL instead. */
  downloadUrl?: string;
  source: "tauri" | "manifest";
};

type RemoteManifest = {
  version: string;
  notes?: string;
  url?: string;
  github?: string;
  /** Optional per-OS installer URLs (preferred over `url` when present). */
  platforms?: {
    windows?: string;
    macos?: string;
    linux?: string;
  };
};

type HostOs = "windows" | "macos" | "linux" | "unknown";

const WEBSITE_MANIFEST = "https://maxspeech.vercel.app/updates/latest.json";
const GITHUB_LATEST_API =
  "https://api.github.com/repos/pauliscoool/maxspeech/releases/latest";
const GITHUB_RELEASES_PAGE =
  "https://github.com/pauliscoool/maxspeech/releases/latest";
/** Always-valid landing pages — never bake a versioned filename into the app. */
const WEBSITE_PAGES = {
  windows: "https://maxspeech.vercel.app/windows",
  macos: "https://maxspeech.vercel.app/mac",
  linux: "https://maxspeech.vercel.app/linux",
} as const;

let cached: Update | null = null;
let cachedManifest: UpdateInfo | null = null;

function detectHostOs(): HostOs {
  const p = (navigator.platform || "").toLowerCase();
  const ua = (navigator.userAgent || "").toLowerCase();
  if (p.includes("win") || ua.includes("windows")) return "windows";
  if (p.includes("mac") || ua.includes("mac")) return "macos";
  if (p.includes("linux") || ua.includes("linux")) return "linux";
  return "unknown";
}

function websitePageFallback(os: HostOs = detectHostOs()): string {
  if (os === "macos") return WEBSITE_PAGES.macos;
  if (os === "linux") return WEBSITE_PAGES.linux;
  return WEBSITE_PAGES.windows;
}

/** Pull `x.y.z` from an installer filename in a URL, if present. */
export function versionFromUrl(url: string): string | null {
  try {
    const path = new URL(url).pathname;
    const match = path.match(/(\d+\.\d+\.\d+)/);
    return match?.[1] ?? null;
  } catch {
    const match = url.match(/(\d+\.\d+\.\d+)/);
    return match?.[1] ?? null;
  }
}

/**
 * True when a direct installer URL is safe to use for `targetVersion`.
 * Version-less stable URLs (e.g. MaxSpeech_x64-setup.exe) are always allowed.
 * Versioned URLs are rejected when they embed an older version than the one
 * we're offering — this is what stopped GitHub's stale v0.1.1 asset from
 * winning over the website's current build.
 */
function installerUrlMatchesTarget(url: string, targetVersion: string): boolean {
  if (!looksLikeDirectInstaller(url)) return true;
  const embedded = versionFromUrl(url);
  if (!embedded) return true; // stable / unversioned filename
  // Reject anything older than the version we're advertising.
  return !isNewerVersion(targetVersion, embedded);
}

function pickGithubAssetUrl(
  assets: { name: string; browser_download_url: string }[] | undefined,
  os: HostOs,
  targetVersion?: string,
): string | undefined {
  if (!assets?.length) return undefined;
  const find = (re: RegExp) =>
    assets.find((a) => {
      if (!re.test(a.name)) return false;
      if (!targetVersion) return true;
      return installerUrlMatchesTarget(a.browser_download_url, targetVersion);
    })?.browser_download_url;

  if (os === "macos") {
    return (
      find(/aarch64.*\.dmg$/i) ||
      find(/_aarch64\.dmg$/i) ||
      find(/\.dmg$/i)
    );
  }
  if (os === "linux") {
    return (
      find(/\.AppImage$/i) ||
      find(/_amd64\.deb$/i) ||
      find(/\.deb$/i)
    );
  }
  // windows + unknown → Windows installer
  return (
    find(/x64-setup\.exe$/i) ||
    find(/\.exe$/i)
  );
}

export function looksLikeDirectInstaller(url: string): boolean {
  return /\.(dmg|exe|msi|AppImage|deb|rpm)(\?|#|$)/i.test(url);
}

async function urlExists(url: string): Promise<boolean> {
  try {
    const head = await fetch(url, { method: "HEAD", cache: "no-store" });
    if (head.ok) return true;
    // Some hosts reject HEAD — try a ranged GET.
    if (head.status === 405 || head.status === 501) {
      const get = await fetch(url, {
        method: "GET",
        headers: { Range: "bytes=0-0" },
        cache: "no-store",
      });
      return get.ok || get.status === 206;
    }
    return false;
  } catch {
    return false;
  }
}

async function pickManifestDownloadUrl(
  manifest: RemoteManifest,
  os: HostOs,
  targetVersion: string,
  githubAssetUrl?: string,
): Promise<string> {
  const fromPlatforms =
    os === "macos"
      ? manifest.platforms?.macos
      : os === "linux"
        ? manifest.platforms?.linux
        : os === "windows"
          ? manifest.platforms?.windows
          : undefined;

  // Prefer the website's canonical / platform URL first. GitHub assets come
  // later and are filtered by version so a stale release can't win.
  const candidates = [
    fromPlatforms,
    os === "windows" || os === "unknown" ? manifest.url : undefined,
    githubAssetUrl,
    websitePageFallback(os),
    manifest.github,
    GITHUB_RELEASES_PAGE,
  ].filter((u): u is string => !!u);

  for (const url of candidates) {
    if (!installerUrlMatchesTarget(url, targetVersion)) continue;
    if (!looksLikeDirectInstaller(url)) return url;
    if (await urlExists(url)) return url;
  }

  return websitePageFallback(os);
}

export async function getAppVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "0.1.0";
  }
}

function parseSemver(v: string): number[] | null {
  const cleaned = v.trim().replace(/^v/i, "");
  const parts = cleaned.split(".").map((p) => Number.parseInt(p, 10));
  if (parts.length < 2 || parts.some((n) => Number.isNaN(n))) return null;
  while (parts.length < 3) parts.push(0);
  return parts.slice(0, 3);
}

/** True if `remote` is strictly newer than `current`. */
export function isNewerVersion(remote: string, current: string): boolean {
  const a = parseSemver(remote);
  const b = parseSemver(current);
  if (!a || !b) return remote.replace(/^v/i, "") !== current.replace(/^v/i, "");
  for (let i = 0; i < 3; i++) {
    if (a[i] > b[i]) return true;
    if (a[i] < b[i]) return false;
  }
  return false;
}

async function fetchJson<T>(url: string): Promise<T | null> {
  try {
    const res = await fetch(url, {
      headers: { Accept: "application/json" },
      cache: "no-store",
    });
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

type GhRelease = {
  tag_name?: string;
  name?: string;
  body?: string;
  html_url?: string;
  assets?: { name: string; browser_download_url: string }[];
};

async function fetchManifestUpdate(
  currentVersion: string,
): Promise<UpdateInfo | null> {
  const os = detectHostOs();
  const [release, fromSite] = await Promise.all([
    fetchJson<GhRelease>(GITHUB_LATEST_API),
    fetchJson<RemoteManifest>(WEBSITE_MANIFEST),
  ]);

  const siteVersion = fromSite?.version?.replace(/^v/i, "") ?? "";
  const ghTag = (release?.tag_name || release?.name || "").replace(/^v/i, "");

  const siteIsNewer =
    !!siteVersion && isNewerVersion(siteVersion, currentVersion);
  const ghIsNewer = !!ghTag && isNewerVersion(ghTag, currentVersion);

  if (!siteIsNewer && !ghIsNewer) return null;

  // Offer whichever source is genuinely newer. Prefer the website when tied
  // (its installer URL is the stable canonical one).
  const preferSite =
    siteIsNewer && (!ghIsNewer || !isNewerVersion(ghTag, siteVersion));

  if (preferSite && fromSite) {
    const targetVersion = siteVersion;
    const githubAsset = pickGithubAssetUrl(release?.assets, os, targetVersion);
    return {
      version: targetVersion,
      body: fromSite.notes ?? null,
      currentVersion,
      downloadUrl: await pickManifestDownloadUrl(
        fromSite,
        os,
        targetVersion,
        githubAsset,
      ),
      source: "manifest",
    };
  }

  // GitHub tag is newer (or website missing) — still filter the asset by version.
  const targetVersion = ghTag;
  const githubAsset = pickGithubAssetUrl(release?.assets, os, targetVersion);
  const setup =
    githubAsset ||
    release?.html_url ||
    websitePageFallback(os);

  return {
    version: targetVersion,
    body: release?.body?.trim() || null,
    currentVersion,
    downloadUrl: setup,
    source: "manifest",
  };
}

/** Returns update info if a newer version is available; otherwise null. */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  // Always re-resolve against live manifests — never reuse a stale download URL.
  cached = null;
  cachedManifest = null;

  const currentVersion = await getAppVersion();

  // Prefer signed Tauri updater (GitHub latest.json + .sig) when available.
  try {
    const update = await check();
    if (update && isNewerVersion(update.version, currentVersion)) {
      cached = update;
      return {
        version: update.version,
        body: update.body ?? null,
        currentVersion,
        source: "tauri",
      };
    }
  } catch (err) {
    console.warn("Tauri updater check failed; trying manifest fallback", err);
  }

  const manifest = await fetchManifestUpdate(currentVersion);
  cachedManifest = manifest;
  return manifest;
}

async function downloadAndRunInstaller(
  url: string,
  onProgress?: (pct: number | null) => void,
): Promise<void> {
  const unlisten = await listen<number>("installer-download-progress", (ev) => {
    const n = Number(ev.payload);
    onProgress?.(Number.isFinite(n) ? n : null);
  });
  try {
    onProgress?.(0);
    await invoke("download_and_run_installer", { url });
    onProgress?.(100);
  } finally {
    unlisten();
  }
}

export async function installAvailableUpdate(
  onProgress?: (pct: number | null) => void,
): Promise<void> {
  // Re-check every time Update is pressed so newly published builds are picked up.
  const info = await checkForUpdate();
  if (!info) {
    throw new Error("You're already on the latest version.");
  }

  if (info.source === "tauri" && cached) {
    let downloaded = 0;
    let contentLength: number | null = null;

    await cached.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          contentLength = event.data.contentLength ?? null;
          onProgress?.(0);
          break;
        case "Progress":
          downloaded += event.data.chunkLength;
          if (contentLength && contentLength > 0) {
            onProgress?.(
              Math.min(99, Math.round((downloaded / contentLength) * 100)),
            );
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
    return;
  }

  const url =
    info.downloadUrl ||
    cachedManifest?.downloadUrl ||
    websitePageFallback();

  // Hard stop: never download/run an installer whose filename embeds a version
  // older than what we're currently running. Open the landing page instead.
  if (looksLikeDirectInstaller(url)) {
    const embedded = versionFromUrl(url);
    if (embedded && isNewerVersion(info.currentVersion, embedded)) {
      console.warn(
        `Refusing downgrade installer ${url} (embedded ${embedded} < running ${info.currentVersion})`,
      );
      await openUrl(websitePageFallback());
      throw new Error(
        "Opened the download page — the linked installer was outdated.",
      );
    }
    await downloadAndRunInstaller(url, onProgress);
    return;
  }

  // Landing page / releases page — open in browser.
  await openUrl(url);
  throw new Error(
    "Opened the download page — install from there to finish updating.",
  );
}
