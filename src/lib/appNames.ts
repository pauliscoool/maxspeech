/** Map process / exe names to friendly app labels. */
const APP_LABELS: Record<string, string> = {
  "cursor.exe": "Cursor",
  "code.exe": "VS Code",
  "code - insiders.exe": "VS Code",
  "slack.exe": "Slack",
  "discord.exe": "Discord",
  "outlook.exe": "Outlook",
  "winword.exe": "Word",
  "excel.exe": "Excel",
  "powerpnt.exe": "PowerPoint",
  "notion.exe": "Notion",
  "chrome.exe": "Chrome",
  "msedge.exe": "Edge",
  "firefox.exe": "Firefox",
  "brave.exe": "Brave",
  "spotify.exe": "Spotify",
  "teams.exe": "Teams",
  "ms-teams.exe": "Teams",
  "notepad.exe": "Notepad",
  "notepad++.exe": "Notepad++",
  "windowsterminal.exe": "Terminal",
  "powershell.exe": "PowerShell",
  "cmd.exe": "Command Prompt",
  "explorer.exe": "File Explorer",
  "figma.exe": "Figma",
  "obsidian.exe": "Obsidian",
  "telegram.exe": "Telegram",
  "whatsapp.exe": "WhatsApp",
  "zoom.exe": "Zoom",
  unknown: "Unknown",
};

export function friendlyAppName(raw: string): string {
  if (!raw) return "Unknown";
  const lower = raw.toLowerCase().trim();
  if (APP_LABELS[lower]) return APP_LABELS[lower];

  // Strip path if present
  const base = lower.includes("\\")
    ? lower.split("\\").pop() || lower
    : lower.includes("/")
      ? lower.split("/").pop() || lower
      : lower;

  if (APP_LABELS[base]) return APP_LABELS[base];

  // chrome with Gmail title pattern handled elsewhere; strip .exe and title-case
  let name = base.replace(/\.exe$/i, "").replace(/\.app$/i, "");
  name = name.replace(/[-_]+/g, " ");
  return name
    .split(" ")
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}
