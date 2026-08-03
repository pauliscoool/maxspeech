/** Map process / exe / bundle names to friendly app labels. */
const APP_LABELS: Record<string, string> = {
  "cursor.exe": "Cursor",
  cursor: "Cursor",
  "cursor.app": "Cursor",
  "code.exe": "VS Code",
  code: "VS Code",
  "code.app": "VS Code",
  "code - insiders.exe": "VS Code",
  "visual studio code.app": "VS Code",
  "slack.exe": "Slack",
  slack: "Slack",
  "slack.app": "Slack",
  "discord.exe": "Discord",
  discord: "Discord",
  "discord.app": "Discord",
  "outlook.exe": "Outlook",
  "microsoft outlook.app": "Outlook",
  "winword.exe": "Word",
  "microsoft word.app": "Word",
  "excel.exe": "Excel",
  "microsoft excel.app": "Excel",
  "powerpnt.exe": "PowerPoint",
  "microsoft powerpoint.app": "PowerPoint",
  "notion.exe": "Notion",
  notion: "Notion",
  "notion.app": "Notion",
  "chrome.exe": "Chrome",
  chrome: "Chrome",
  "google-chrome": "Chrome",
  "google chrome.app": "Chrome",
  chromium: "Chrome",
  "msedge.exe": "Edge",
  "microsoft-edge": "Edge",
  "microsoft edge.app": "Edge",
  "firefox.exe": "Firefox",
  firefox: "Firefox",
  "firefox.app": "Firefox",
  "brave.exe": "Brave",
  brave: "Brave",
  "brave-browser": "Brave",
  "brave browser.app": "Brave",
  "spotify.exe": "Spotify",
  spotify: "Spotify",
  "spotify.app": "Spotify",
  "teams.exe": "Teams",
  "ms-teams.exe": "Teams",
  teams: "Teams",
  "microsoft teams.app": "Teams",
  "notepad.exe": "Notepad",
  "notepad++.exe": "Notepad++",
  gedit: "Text Editor",
  kate: "Text Editor",
  mousepad: "Text Editor",
  "textedit.app": "TextEdit",
  "windowsterminal.exe": "Terminal",
  "powershell.exe": "PowerShell",
  "cmd.exe": "Command Prompt",
  "explorer.exe": "File Explorer",
  "gnome-terminal": "Terminal",
  konsole: "Terminal",
  alacritty: "Terminal",
  kitty: "Terminal",
  wezterm: "Terminal",
  "terminal.app": "Terminal",
  "iterm2.app": "iTerm",
  "figma.exe": "Figma",
  figma: "Figma",
  "figma.app": "Figma",
  "obsidian.exe": "Obsidian",
  obsidian: "Obsidian",
  "obsidian.app": "Obsidian",
  "telegram.exe": "Telegram",
  telegram: "Telegram",
  "telegram-desktop": "Telegram",
  "telegram.app": "Telegram",
  "whatsapp.exe": "WhatsApp",
  whatsapp: "WhatsApp",
  "whatsapp.app": "WhatsApp",
  "zoom.exe": "Zoom",
  zoom: "Zoom",
  "zoom.us.app": "Zoom",
  "safari.app": "Safari",
  "mail.app": "Mail",
  "notes.app": "Notes",
  "messages.app": "Messages",
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

  let name = base
    .replace(/\.exe$/i, "")
    .replace(/\.app$/i, "")
    .replace(/[-_]+/g, " ");
  return name
    .split(" ")
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}
